import { describe, expect, it } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

// Each window may call only the app commands its capability file allows
// (build.rs declares them in the app manifest). This walks each window's
// import graph from its HTML entry, collects the `invoke("...")` names, and
// checks them against the capability, so a new call can't be denied at runtime.

const root = process.cwd();
const read = (p: string) => readFileSync(join(root, p), "utf8");

function capability(file: string): Set<string> {
  const cap = JSON.parse(read(`src-tauri/capabilities/${file}`)) as { permissions: string[] };
  return new Set(
    cap.permissions.filter((p) => p.startsWith("allow-")).map((p) => p.slice("allow-".length).replace(/-/g, "_")),
  );
}

function resolveImport(from: string, spec: string): string | null {
  const base = resolve(dirname(from), spec);
  for (const p of [base, `${base}.ts`, `${base}.tsx`, join(base, "index.ts"), join(base, "index.tsx")]) {
    if (/\.tsx?$/.test(p) && existsSync(p)) return p;
  }
  return null;
}

/** Every app command invoked by the code `entry` (a path under the root) loads. */
function invokedFrom(entry: string): Set<string> {
  const seen = new Set<string>();
  const names = new Set<string>();
  const queue = [resolve(root, entry)];
  while (queue.length) {
    const file = queue.pop() as string;
    if (seen.has(file)) continue;
    seen.add(file);
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(/\binvoke\s*(?:<[^>]*>)?\s*\(\s*(["'`])([^"'`]+)\1/g)) names.add(m[2]);
    for (const m of text.matchAll(/(?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s+)(["'])(\.[^"']+)\1/g)) {
      const next = resolveImport(file, m[2]);
      if (next) queue.push(next);
    }
  }
  return names;
}

function entryScript(html: string): string {
  const m = read(html).match(/<script[^>]*src="\/([^"]+)"/);
  if (!m) throw new Error(`no module script in ${html}`);
  return m[1];
}

const declared = [...read("src-tauri/build.rs").matchAll(/^\s*"([a-z_]+)",$/gm)].map((m) => m[1]);
const handlerBlock = read("src-tauri/src/lib.rs").match(/generate_handler!\[([\s\S]*?)\]/);
const handled = (handlerBlock?.[1] ?? "")
  .split(",")
  .map((s) => s.trim().split("::").pop() ?? "")
  .filter(Boolean);

describe("IPC capabilities", () => {
  it("build.rs declares exactly the commands lib.rs registers", () => {
    expect(handled.length).toBeGreaterThan(0);
    expect([...declared].sort()).toEqual([...handled].sort());
  });

  it("both capability files allow only declared commands", () => {
    for (const file of ["default.json", "hud.json"]) {
      for (const name of capability(file)) expect(declared, `${file}: allow-${name}`).toContain(name);
    }
  });

  it("the dashboard may call every command it invokes", () => {
    const allowed = capability("default.json");
    const used = invokedFrom(entryScript("index.html"));
    expect(used.size).toBeGreaterThan(5);
    for (const name of used) expect(allowed, `dashboard invokes ${name}`).toContain(name);
  });

  it("the HUD may call only what it invokes, and every one of those", () => {
    const allowed = capability("hud.json");
    const used = invokedFrom(entryScript("hud.html"));
    expect([...used].sort()).toEqual([...allowed].sort());
    expect([...allowed].sort()).toEqual(["dispatch", "get_snapshot", "set_hud_hit_region"]);
  });
});
