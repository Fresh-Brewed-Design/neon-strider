import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const css = readFileSync(join(process.cwd(), "src/hud/hud.css"), "utf8");
const rule = (selector: string) => {
  const start = css.indexOf(`
${selector} {`);
  return start < 0 ? "" : css.slice(start, css.indexOf("}", start));
};

// At a larger Windows text size the HUD content is wider than at 100 %, so nothing may be
// pinned to one line when a card's width is fixed.
describe("HUD CSS at larger text sizes", () => {
  it("lets the blink step name wrap instead of clipping", () => {
    const cue = rule(".hud-eyebreak-cue");
    expect(cue).not.toBe("");
    expect(cue).not.toMatch(/white-space:\s*nowrap/);
    expect(cue).toMatch(/overflow-wrap:\s*anywhere/);
  });

  it("keeps card button names on one line and wraps the row", () => {
    expect(rule(".hud-seated-actions")).toMatch(/flex-wrap:\s*wrap/);
    expect(css).toMatch(/\.hud-seated-actions \.hud-btn-sm\s*\{\s*white-space:\s*nowrap/);
  });
});
