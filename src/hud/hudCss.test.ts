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

// Windows' text size reaches the HUD as a page zoom (WebView2 measured at 150 % display scale and
// 121 % text size: devicePixelRatio 1.815, the 340 px window laid out 281 CSS px wide), so every
// readout has to fit a narrower column without cutting digits.
describe("HUD distance readouts at larger text sizes", () => {
  it("sizes stat values to their own cell, so a long value can't push its neighbour off the panel", () => {
    expect(rule(".hud-stat")).toMatch(/container-type:\s*inline-size/);
    const value = rule(".hud-stat .hud-value");
    expect(value).toMatch(/flex-wrap:\s*wrap/);
    expect(value).toMatch(/font-size:\s*clamp\(\s*\d+px\s*,\s*[\d.]+cqi\s*,\s*22px\s*\)/);
  });

  it("stacks the stat cells when two columns would be too narrow", () => {
    expect(rule(".hud-stats")).toMatch(/grid-template-columns:\s*repeat\(auto-fit/);
  });

  it("wraps the running strip's stats, program line and buttons instead of overlapping them", () => {
    const stats = rule(".hud-run-stats");
    expect(stats).toMatch(/flex-wrap:\s*wrap/);
    expect(stats).not.toMatch(/minmax\(0/);
    expect(rule(".hud-run-program")).toMatch(/flex-wrap:\s*wrap/);
    expect(css).not.toMatch(/\.hud-run-strip \.hud-strip-row\s*\{[^}]*nowrap/);
    expect(css).not.toMatch(/\.hud-run-strip \.hud-btn-main\s*\{[^}]*min-width:\s*0/);
  });

  it("wraps the program buttons and steppers rather than pushing them past the panel", () => {
    expect(rule(".hud-auto-actions")).toMatch(/flex-wrap:\s*wrap/);
    expect(rule(".hud-steppers")).toMatch(/repeat\(auto-fit/);
  });

  it("wraps the neural flow label instead of hiding its percentage", () => {
    const label = rule(".hud-neural .hud-label");
    expect(label).not.toBe("");
    expect(label).not.toMatch(/ellipsis|nowrap/);
    expect(rule(".hud-neural")).toMatch(/flex-wrap:\s*wrap/);
  });

  it("never sizes text under 11px, including the smallest a fluid size can reach", () => {
    const sizes = [...css.matchAll(/font-size:\s*([^;]+);/g)].map((m) => m[1].trim());
    expect(sizes.length).toBeGreaterThan(0);
    for (const size of sizes) {
      const smallest = size.startsWith("clamp(") ? size.slice(6).split(",")[0].trim() : size;
      expect(smallest, size).toMatch(/^\d+(\.\d+)?px$/);
      expect(parseFloat(smallest), size).toBeGreaterThanOrEqual(11);
    }
  });
});
