// @ts-expect-error plain .mjs build script, no types
import * as rv from "./release-version.mjs";

describe("release-version", () => {
  it("finds the highest tag numerically", () => {
    expect(rv.latestVersion(["v1.9.0", "v1.10.0", "v1.2.3", "junk", ""])).toEqual([1, 10, 0]);
    expect(rv.latestVersion([])).toEqual([0, 0, 0]);
  });

  it("takes the largest bump over every merged PR", () => {
    expect(rv.releaseKind([["release:patch"], ["release:minor"], []], false)).toBe("minor");
    expect(rv.releaseKind([["bug"], ["release:none"]], false)).toBe("patch");
    expect(rv.releaseKind([["release:none"]], false)).toBeNull();
    expect(rv.releaseKind([["release:none"]], true)).toBe("patch");
    expect(rv.releaseKind([], false)).toBeNull();
    expect(rv.bump([1, 1, 4], "minor")).toBe("1.2.0");
    expect(rv.bump([1, 2, 3], "major")).toBe("2.0.0");
    expect(rv.bump([1, 2, 0], "patch")).toBe("1.2.1");
  });

  it("ships package.json's version when there is no tag yet", () => {
    expect(rv.nextVersion([""], "patch", "1.2.0")).toBe("1.2.0");
    expect(rv.nextVersion(["v1.2.0", ""], "patch", "1.2.0")).toBe("1.2.1");
  });

  it("groups fragments by kind", () => {
    const notes = rv.notesFromFragments([
      { name: "fixed-b.md", text: "- Fix B\n" },
      { name: "new-a.md", text: "- New A\r\n- New A2\r\n" },
      { name: "fixed-a.md", text: "- Fix A\n" },
    ]);
    expect(notes).toBe("### New\n\n- New A\n- New A2\n\n### Fixed\n\n- Fix A\n- Fix B");
  });

  it("folds a legacy Unreleased section and fragments into the version", () => {
    const text = "# Changelog\r\n\r\n## Unreleased (1.2)\r\n\r\n### New\r\n\r\n- Big thing\r\n\r\n## 1.1.0 (2026-11-13)\r\n\r\n- Old\r\n";
    const out = rv.stampChangelog(text, "## 1.2.0 (2026-11-20)", "### Fixed\r\n\r\n- Small fix", "t");
    expect(out).toBe(
      "# Changelog\r\n\r\n## 1.2.0 (2026-11-20)\r\n\r\n### New\r\n\r\n- Big thing\r\n\r\n### Fixed\r\n\r\n- Small fix\r\n\r\n## 1.1.0 (2026-11-13)\r\n\r\n- Old\r\n",
    );
  });

  it("adds a section above the newest version, or the PR titles when there are no notes", () => {
    expect(rv.stampChangelog("# Changelog\n\n## 1.1.0 (x)\n", "## 1.1.1 (d)", "### Fixed\n\n- A", "t")).toBe(
      "# Changelog\n\n## 1.1.1 (d)\n\n### Fixed\n\n- A\n\n## 1.1.0 (x)\n",
    );
    expect(rv.stampChangelog("# Changelog\n\n## 1.1.0 (x)\n", "## 1.1.1 (d)", "", "Faster startup")).toBe(
      "# Changelog\n\n## 1.1.1 (d)\n\n- Faster startup\n\n## 1.1.0 (x)\n",
    );
  });

  it("extracts one version's changelog section", () => {
    const text =
      "# Changelog\r\n\r\n## 1.10.0 (2026-12-01)\r\n\r\n### New\r\n\r\n- Ten\r\n\r\n## 1.1.0 (2026-11-13)\r\n\r\n### Fixed\r\n\r\n- One\r\n- Two\r\n";
    expect(rv.changelogSection(text, "1.10.0")).toBe("### New\n\n- Ten");
    expect(rv.changelogSection(text, "v1.1.0")).toBe("### Fixed\n\n- One\n- Two");
    // 1.1.0 must not match the 1.10.0 heading, nor "." match any character.
    expect(rv.changelogSection("## 1.10.0 (x)\n- a\n", "1.1.0")).toBeNull();
    expect(rv.changelogSection("## 1x1x0 (x)\n- a\n", "1.1.0")).toBeNull();
    expect(rv.changelogSection(text, "2.0.0")).toBeNull();
    expect(rv.changelogSection("## 1.2.0\n", "1.2.0")).toBe("");
  });

  it("sets versions in each file", () => {
    expect(rv.setJsonVersion('{\n  "name": "x",\n  "version": "1.1.0",\n}', "1.2.0")).toContain('"version": "1.2.0"');
    expect(rv.setCargoTomlVersion('[package]\nname = "neon-strider"\nversion = "1.1.0"\n[dependencies]\nfoo = { version = "2" }', "1.2.0")).toBe(
      '[package]\nname = "neon-strider"\nversion = "1.2.0"\n[dependencies]\nfoo = { version = "2" }',
    );
    const lock = '[[package]]\r\nname = "neon-strider"\r\nversion = "1.1.0"\r\n\r\n[[package]]\r\nname = "other"\r\nversion = "1.1.0"\r\n';
    expect(rv.setCargoLockVersion(lock, "neon-strider", "1.2.0")).toBe(
      lock.replace('"neon-strider"\r\nversion = "1.1.0"', '"neon-strider"\r\nversion = "1.2.0"'),
    );
  });
});
