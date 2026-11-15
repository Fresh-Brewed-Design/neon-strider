// Releases everything merged into main since the last v* tag, in one version.
// Run by .github/workflows/auto-release.yml on every push to main; pure helpers
// are tested in scripts/release-version.test.ts.
//
// Bump: the largest `release:major|minor|patch` label among the PRs merged since
// the last tag (a PR without one counts as patch; `release:none` PRs don't count).
// Nothing to release (no PRs, or only release:none, and no notes): no release.
//
// Notes: each PR adds its own file in `changes/` (no shared lines, so PRs never
// conflict): `changes/<new|changed|fixed>-<slug>.md`, holding "- " bullet lines.
// They are folded into CHANGELOG.md under the new version and deleted. A legacy
// "## Unreleased..." section in CHANGELOG.md is folded in too.
//
// Writes: package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml,
// src-tauri/Cargo.lock, CHANGELOG.md; deletes the used changes/ files. The new
// version's CHANGELOG section goes to $RELEASE_NOTES_FILE (release-notes.md by
// default) for the GitHub release body.
//
// `node scripts/release-version.mjs notes <tag> <file>` only writes that tag's
// CHANGELOG section to <file> (release.yml, when a v* tag is pushed by hand).
import { execSync } from "node:child_process";
import { appendFileSync, existsSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const RANK = { none: -1, patch: 0, minor: 1, major: 2 };
const SECTIONS = [
  ["new", "New"],
  ["changed", "Changed"],
  ["fixed", "Fixed"],
];

/** The highest vX.Y.Z tag, or 0.0.0. */
export function latestVersion(tags) {
  const parsed = tags
    .map((t) => /^v(\d+)\.(\d+)\.(\d+)$/.exec(t.trim()))
    .filter(Boolean)
    .map((m) => [Number(m[1]), Number(m[2]), Number(m[3])]);
  parsed.sort((a, b) => b[0] - a[0] || b[1] - a[1] || b[2] - a[2]);
  return parsed[0] ?? [0, 0, 0];
}

/** One PR's bump from its labels: "none" | "patch" | "minor" | "major". */
export function kindFromLabels(labels) {
  for (const kind of ["none", "major", "minor", "patch"]) {
    if (labels.includes(`release:${kind}`)) return kind;
  }
  return "patch";
}

/** The release's bump over all merged PRs (label lists), or null for no release. */
export function releaseKind(prLabels, hasNotes) {
  let best = null;
  for (const labels of prLabels) {
    const kind = kindFromLabels(labels);
    if (kind === "none") continue;
    if (best === null || RANK[kind] > RANK[best]) best = kind;
  }
  if (best === null && hasNotes) return "patch";
  return best;
}

/** The first release of a repo (no v* tag yet) ships the version already in package.json. */
export function nextVersion(tags, kind, packageVersion) {
  const hasTag = tags.some((t) => /^v\d+\.\d+\.\d+$/.test(t.trim()));
  return hasTag ? bump(latestVersion(tags), kind) : packageVersion;
}

export function bump([major, minor, patch], kind) {
  if (kind === "major") return `${major + 1}.0.0`;
  if (kind === "minor") return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

/** Group fragment files ({name, text}) into "### New/Changed/Fixed" bullet blocks. */
export function notesFromFragments(fragments, nl = "\n") {
  const blocks = [];
  for (const [prefix, title] of SECTIONS) {
    const lines = fragments
      .filter((f) => f.name.startsWith(`${prefix}-`))
      .sort((a, b) => a.name.localeCompare(b.name))
      .flatMap((f) => f.text.split(/\r?\n/).filter((l) => l.trim()));
    if (lines.length) blocks.push(`### ${title}${nl}${nl}${lines.join(nl)}`);
  }
  const other = fragments
    .filter((f) => !SECTIONS.some(([p]) => f.name.startsWith(`${p}-`)))
    .flatMap((f) => f.text.split(/\r?\n/).filter((l) => l.trim()));
  if (other.length) blocks.push(`### Other${nl}${nl}${other.join(nl)}`);
  return blocks.join(`${nl}${nl}`);
}

/**
 * Put `heading` + `notes` at the top of the changelog. A legacy "## Unreleased"
 * section becomes the version (its text kept, `notes` added after it).
 * `fallback` is used when there are no notes at all.
 */
export function stampChangelog(text, heading, notes, fallback) {
  const nl = text.includes("\r\n") ? "\r\n" : "\n";
  const unreleased = /^## Unreleased.*$/m;
  if (unreleased.test(text)) {
    const start = text.search(unreleased);
    const afterHeading = text.indexOf(nl, start) + nl.length;
    const rest = text.slice(afterHeading);
    const next = rest.search(/^## /m);
    const body = (next === -1 ? rest : rest.slice(0, next)).trim();
    const tail = next === -1 ? "" : rest.slice(next);
    const merged = [body, notes].filter(Boolean).join(`${nl}${nl}`);
    return `${text.slice(0, start)}${heading}${nl}${nl}${merged}${nl}${nl}${tail}`.replace(/\s+$/, nl);
  }
  const block = `${heading}${nl}${nl}${notes || `- ${fallback}`}${nl}${nl}`;
  const firstSection = text.search(/^## /m);
  if (firstSection === -1) return `${text.trimEnd()}${nl}${nl}${block}`;
  return text.slice(0, firstSection) + block + text.slice(firstSection);
}

/**
 * The text under `## <version> (...)` in the changelog, up to the next `## `
 * heading, trimmed and with "\n" line ends; null when there is no such section.
 * `version` may carry a leading "v".
 */
export function changelogSection(text, version) {
  const v = version.trim().replace(/^v/, "").replace(/[.]/g, "\\.");
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((l) => new RegExp(`^## ${v}(\\s|$)`).test(l));
  if (start === -1) return null;
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((l) => /^## /.test(l));
  return (end === -1 ? rest : rest.slice(0, end)).join("\n").trim();
}

export function setJsonVersion(text, version) {
  return text.replace(/("version"\s*:\s*")[^"]+(")/, `$1${version}$2`);
}

export function setCargoTomlVersion(text, version) {
  // The first `version =` at the start of a line is the [package] one.
  return text.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`);
}

export function setCargoLockVersion(text, pkg, version) {
  const re = new RegExp(`(name = "${pkg}"\\r?\\nversion = ")[^"]+(")`);
  return text.replace(re, `$1${version}$2`);
}

function rewrite(path, fn) {
  const before = readFileSync(path, "utf8");
  const after = fn(before);
  if (after !== before) writeFileSync(path, after);
}

function main() {
  const prLabels = JSON.parse(process.env.PR_LABELS || "[]");
  const titles = JSON.parse(process.env.PR_TITLES || "[]");
  const fragmentFiles = existsSync("changes")
    ? readdirSync("changes").filter((n) => n.endsWith(".md") && n !== "README.md")
    : [];
  const fragments = fragmentFiles.map((name) => ({
    name,
    text: readFileSync(join("changes", name), "utf8"),
  }));
  const changelog = readFileSync("CHANGELOG.md", "utf8");
  const hasNotes = fragments.length > 0 || /^## Unreleased/m.test(changelog);

  const kind = releaseKind(prLabels, hasNotes);
  if (!kind) {
    console.log("release-version: nothing to release");
    if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, "tag=\n");
    return;
  }

  const tags = execSync('git tag --list "v*"', { encoding: "utf8" }).split("\n");
  const version = nextVersion(tags, kind, JSON.parse(readFileSync("package.json", "utf8")).version);
  const date = new Date().toISOString().slice(0, 10);
  const nl = changelog.includes("\r\n") ? "\r\n" : "\n";
  const fallback = titles.length ? titles.join("; ") : "Maintenance release";

  for (const p of ["package.json", "src-tauri/tauri.conf.json"]) rewrite(p, (t) => setJsonVersion(t, version));
  rewrite("src-tauri/Cargo.toml", (t) => setCargoTomlVersion(t, version));
  rewrite("src-tauri/Cargo.lock", (t) => setCargoLockVersion(t, "neon-strider", version));
  rewrite("CHANGELOG.md", (t) =>
    stampChangelog(t, `## ${version} (${date})`, notesFromFragments(fragments, nl), fallback),
  );
  for (const name of fragmentFiles) rmSync(join("changes", name));
  writeNotes(readFileSync("CHANGELOG.md", "utf8"), version, process.env.RELEASE_NOTES_FILE || "release-notes.md");

  console.log(`release-version: v${version} (${kind})`);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `tag=v${version}\n`);
}

/** Write `version`'s changelog section to `file` (a placeholder line when it has none). */
function writeNotes(changelog, version, file) {
  const section = changelogSection(changelog, version);
  if (section === null) console.log(`release-version: no CHANGELOG section for ${version}`);
  writeFileSync(file, `${section || "- Maintenance release"}\n`);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const [cmd, tag, file] = process.argv.slice(2);
  if (cmd === "notes") {
    if (!tag || !file) throw new Error("usage: release-version.mjs notes <tag> <file>");
    writeNotes(readFileSync("CHANGELOG.md", "utf8"), tag, file);
  } else {
    main();
  }
}
