import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const css = readFileSync(join(process.cwd(), "src/index.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

/** The body of the `@container` block whose query starts with `query`, braces matched. */
function containerBlock(query: string): string {
  const start = css.indexOf(`@container ${query}`);
  if (start < 0) return "";
  const open = css.indexOf("{", start);
  let depth = 0;
  for (let i = open; i < css.length; i++) {
    if (css[i] === "{") depth++;
    if (css[i] === "}" && --depth === 0) return css.slice(open + 1, i);
  }
  return "";
}

/** The declarations of `selector` inside a block (or the whole sheet). */
function rule(block: string, selector: string): string {
  const at = block.indexOf(`${selector} {`);
  if (at < 0) return "";
  return block.slice(block.indexOf("{", at) + 1, block.indexOf("}", at));
}

// Windows' text size reaches the dashboard as a page zoom, so the 760 px window lays out
// 760 / scale CSS px wide (338 at 225 %). The layout must follow that width, not the screen's.
describe("dashboard CSS at larger text sizes", () => {
  it("sizes the dashboard against its own root, which the zoom shrinks", () => {
    expect(rule(css, ".dash-root")).toMatch(/container:\s*dash\s*\/\s*size/);
  });

  it("collapses the rail to icons only below 740 px, keeping 100 % (760 px) as it is", () => {
    const compact = containerBlock("dash (max-width: 739px)");
    expect(compact).not.toBe("");
    expect(rule(compact, ".dash-rail")).toMatch(/width:\s*3\.25rem/);
    // The names stay in the accessibility tree (no display: none), only out of sight.
    const label = rule(compact, ".dash-rail-text");
    expect(label).toMatch(/clip-path:\s*inset\(50%\)/);
    expect(label).not.toMatch(/display:\s*none/);
  });

  it("lets the page header scroll away on a short window", () => {
    const short = containerBlock("dash (max-height: 399px)");
    expect(rule(short, ".dash-page")).toMatch(/overflow-y:\s*auto/);
  });

  it("wraps STEP_MAP rows and the editor's step rows when narrow", () => {
    expect(rule(containerBlock("(max-width: 30rem)"), ".step-map-row")).toMatch(/flex-wrap:\s*wrap/);
    expect(rule(containerBlock("(max-width: 28rem)"), ".program-step-row")).toMatch(/flex-wrap:\s*wrap/);
  });
});
