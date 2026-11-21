// Prints an inventory of what the app already does, read straight from the source.
// Run: node scripts/feature-inventory.mjs [filter]   (filter: case-insensitive word or regex)
//
// A heuristic lookup aid, not a spec: every extractor is a small regex over the
// source, so it can miss entries or list a stray one. Nothing here is committed
// output, so PRs never have to keep it in sync. Use it before building a feature
// (see CLAUDE.md), then grep the code for the behaviour itself.
//
// Groups: HUD toasts, HUD cards, Snapshot, Commands, Settings, Dashboard,
// Shortcuts, and (with a filter) Release notes. Each entry carries path:line.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

function read(rel) {
  try {
    return readFileSync(join(ROOT, rel), "utf8");
  } catch {
    return "";
  }
}

/** Files under `rel` (relative to the repo root) whose name matches `re`, as repo-relative paths. */
function walk(rel, re) {
  const out = [];
  const visit = (dir) => {
    let entries = [];
    try {
      entries = readdirSync(join(ROOT, dir), { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries.sort((a, b) => a.name.localeCompare(b.name))) {
      const p = `${dir}/${e.name}`;
      if (e.isDirectory()) visit(p);
      else if (re.test(e.name)) out.push(p);
    }
  };
  visit(rel);
  return out;
}

const entry = (path, line, name, detail = "") => ({ path, line, name, detail });
const lines = (text) => text.split(/\r?\n/);
const notTest = (p) => !/\.test\./.test(p);

/** `crate::hud::toast(` calls in Rust: type and message as written, plus types the frontend declares. */
export function hudToasts() {
  const out = [];
  const emitted = new Set();
  for (const path of walk("src-tauri/src", /\.rs$/)) {
    const ls = lines(read(path));
    ls.forEach((l, i) => {
      if (!l.includes("crate::hud::toast(")) return;
      const block = ls.slice(i, i + 16);
      const type = block.map((b) => /"type":\s*"(\w+)"/.exec(b)).find(Boolean)?.[1] ?? "?";
      const raw = block.map((b) => /"message":\s*(.*?),?\s*$/.exec(b)).find(Boolean)?.[1] ?? "";
      const msg = raw.replace(/,\s*"\w+":.*$/, "").replace(/\s*\}\)\s*,?$/, "");
      emitted.add(type);
      out.push(entry(path, i + 1, type, msg));
    });
  }
  const ts = "src/hud/useToasts.ts";
  const src = read(ts);
  const m = /type:\s*((?:"\w+"\s*\|?\s*)+);/.exec(src);
  if (m) {
    const line = src.slice(0, m.index).split("\n").length;
    for (const t of m[1].match(/"(\w+)"/g) ?? []) {
      const name = t.replace(/"/g, "");
      if (!emitted.has(name)) out.push(entry(ts, line, name, "declared, no Rust code emits it"));
    }
  }
  return out;
}

/** Exported components in HudCards.tsx and other *Card components under src/hud, with their hud-label text. */
export function hudCards() {
  const out = [];
  for (const path of walk("src/hud", /\.tsx$/).filter(notTest)) {
    const ls = lines(read(path));
    ls.forEach((l, i) => {
      const m = /^(?:export\s+)?(?:default\s+)?function\s+(\w+)/.exec(l);
      if (!m) return;
      const isCard = /Card$/.test(m[1]) || (path.endsWith("HudCards.tsx") && /^export/.test(l));
      if (!isCard) return;
      const labels = [];
      for (let j = i + 1; j < ls.length && !/^(export\s+)?function\s/.test(ls[j]); j++) {
        const lm = /hud-label[^>]*>([^<{]+)</.exec(ls[j]);
        if (lm) labels.push(lm[1].trim());
      }
      out.push(entry(path, i + 1, m[1], labels.join(" | ")));
    });
  }
  return out;
}

/** Fields of the pub structs in snapshot/types.rs (AppSnapshot and what it nests), with `///` docs. */
export function snapshotFields() {
  const path = "src-tauri/src/snapshot/types.rs";
  const out = [];
  let struct = null;
  let doc = "";
  lines(read(path)).forEach((l, i) => {
    const s = /^pub struct (\w+)/.exec(l);
    if (s) {
      struct = s[1];
      doc = "";
      return;
    }
    if (!struct) return;
    if (/^}/.test(l)) {
      struct = null;
      return;
    }
    const d = /^\s*\/\/\/\s?(.*)/.exec(l);
    if (d) {
      if (!doc) doc = d[1];
      return;
    }
    const f = /^\s*pub (\w+):\s*(.+?),?\s*$/.exec(l);
    if (f) {
      out.push(entry(path, i + 1, `${struct}.${f[1]}`, [f[2], doc].filter(Boolean).join("  ")));
      doc = "";
    } else if (!/^\s*(#\[|\/\/)/.test(l)) doc = "";
  });
  return out;
}

/** `Command` enum variants in snapshot/types.rs with their first doc line. */
export function commands() {
  const path = "src-tauri/src/snapshot/types.rs";
  const ls = lines(read(path));
  const start = ls.findIndex((l) => /^pub enum Command\b/.test(l));
  if (start < 0) return [];
  const out = [];
  let doc = "";
  for (let i = start + 1; i < ls.length && !/^}/.test(ls[i]); i++) {
    const l = ls[i];
    const d = /^\s{4}\/\/\/\s?(.*)/.exec(l);
    if (d) {
      if (!doc) doc = d[1];
      continue;
    }
    const v = /^\s{4}([A-Z]\w*)/.exec(l);
    if (v) {
      out.push(entry(path, i + 1, v[1], doc));
      doc = "";
    } else if (/^\s{4}\/\/ ----/.test(l)) doc = "";
  }
  return out;
}

/** Persisted setting keys: `const KEY_*: &str = "..."` in Rust, with a default when `unwrap_or(x)` sits on a line using the const. */
export function settings() {
  const out = [];
  for (const path of walk("src-tauri/src", /\.rs$/)) {
    const ls = lines(read(path));
    ls.forEach((l, i) => {
      const m = /^\s*(?:pub(?:\([a-z]+\))?\s+)?const (\w*KEY\w*):\s*&str\s*=\s*"([^"]+)"/.exec(l);
      if (!m) return;
      const word = new RegExp(`\\b${m[1]}\\b`);
      const used = ls.findIndex((u, j) => j !== i && word.test(u) && /unwrap_or\(/.test(u));
      const def = used >= 0 ? /unwrap_or\(([^)]*)\)/.exec(ls[used])?.[1] : undefined;
      out.push(entry(path, i + 1, m[2], def !== undefined ? `default ${def}` : ""));
    });
  }
  return out;
}

/** Dashboard pages and section components, plus the HUD widgets in widgets/registry.ts (id and codename). */
export function dashboard() {
  const out = [];
  for (const path of walk("src/dashboard/pages", /Page\.tsx$/).filter(notTest)) {
    out.push(entry(path, 1, "page " + path.split("/").pop().replace(".tsx", "")));
  }
  for (const path of walk("src/dashboard", /Section\.tsx$/).filter(notTest)) {
    out.push(entry(path, 1, "section " + path.split("/").pop().replace(".tsx", "")));
  }
  const reg = "src/widgets/registry.ts";
  const ls = lines(read(reg));
  ls.forEach((l, i) => {
    const id = /^\s{4}id:\s*"(\w+)"/.exec(l);
    if (!id) return;
    const block = ls.slice(i, i + 6).join("\n");
    const label = /label:\s*"([^"]*)"/.exec(block)?.[1] ?? "";
    const code = /codename:\s*"([^"]*)"/.exec(block)?.[1] ?? "";
    out.push(entry(reg, i + 1, `widget ${id[1]}`, `${label} / ${code}`));
  });
  return out;
}

/** Hotkeys in hooks.rs: the keys the minify hotkey can bind. */
export function shortcuts() {
  const path = "src-tauri/src/hooks.rs";
  const out = [];
  lines(read(path)).forEach((l, i) => {
    const m = /Some\(HudMinifyKey::(\w+)\)\s*=>/.exec(l);
    if (m) out.push(entry(path, i + 1, `minify hotkey ${m[1]}`));
  });
  return out;
}

/** Matching lines of CHANGELOG.md and changes/*.md (only used with a filter). */
export function releaseNotes(re) {
  const out = [];
  const files = ["CHANGELOG.md", ...walk("changes", /\.md$/).filter((p) => !/README/.test(p))];
  for (const path of files) {
    lines(read(path)).forEach((l, i) => {
      if (l.trim() && re.test(l)) out.push(entry(path, i + 1, l.trim().replace(/^-\s*/, "")));
    });
  }
  return out;
}

export const GROUPS = [
  ["HUD toasts", hudToasts],
  ["HUD cards", hudCards],
  ["Snapshot", snapshotFields],
  ["Commands", commands],
  ["Settings", settings],
  ["Dashboard", dashboard],
  ["Shortcuts", shortcuts],
];

/** Builds { group, entries } lists, applying the optional filter to name, detail and path. */
export function inventory(filter = "") {
  let re = null;
  if (filter) {
    try {
      re = new RegExp(filter, "i");
    } catch {
      re = new RegExp(filter.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "i");
    }
  }
  const keep = (e) => !re || re.test(`${e.name} ${e.detail} ${e.path}`);
  const result = GROUPS.map(([group, fn]) => {
    let entries = [];
    try {
      entries = fn();
    } catch {
      entries = [];
    }
    return { group, entries: entries.filter(keep) };
  });
  if (re) result.push({ group: "Release notes", entries: releaseNotes(re) });
  return result;
}

export function format(groups) {
  const out = [];
  for (const { group, entries } of groups) {
    out.push(`== ${group} (${entries.length}) ==`);
    for (const e of entries) out.push(`${e.path}:${e.line}  ${e.name}${e.detail ? "  " + e.detail : ""}`);
    out.push("");
  }
  return out.join("\n");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.stdout.write(format(inventory(process.argv.slice(2).join(" "))));
}
