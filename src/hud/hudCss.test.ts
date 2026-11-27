import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const css = readFileSync(join(process.cwd(), "src/hud/hud.css"), "utf8");
/** The body of the first rule whose selector list names `selector`, alone or beside others (`.a, .b`). */
const rule = (selector: string) => {
  const bare = css.replace(/\/\*[\s\S]*?\*\//g, "");
  for (const chunk of bare.split("}")) {
    const [head, body] = chunk.split("{");
    if (body !== undefined && head.split(",").some((s) => s.trim() === selector)) return body;
  }
  return "";
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

  it("caps a program button at the row's width and wraps its line, as END does at 225 % text size", () => {
    // min-content: each button keeps its longest word whole, and the three share the row equally.
    expect(css).toMatch(/\.hud-auto-actions \.hud-btn\s*\{[^}]*min-width:\s*min\(min-content,\s*100%\)/);
    expect(rule(".hud-auto-actions .hud-btn-sub")).toMatch(/white-space:\s*normal/);
  });

  it("lets the steppers shrink to a column narrower than one of them, as at 225 % text size", () => {
    expect(rule(".hud-steppers")).toMatch(/minmax\(min\(104px,\s*100%\),\s*1fr\)/);
    expect(rule(".hud-stepper")).toMatch(/min-width:\s*0/);
    const button = rule(".hud-btn-step");
    expect(button).toMatch(/flex:\s*0 1 32px/);
    expect(button).toMatch(/min-width:\s*24px/);
    expect(button).not.toMatch(/(^|[\s;])width:/);
    expect(rule(".hud-stepper-value")).toMatch(/clamp\(11px,\s*16cqi,\s*15px\)/);
  });

  it("wraps the neural flow label instead of hiding its percentage", () => {
    const label = rule(".hud-neural .hud-label");
    expect(label).not.toBe("");
    expect(label).not.toMatch(/ellipsis|nowrap/);
    expect(rule(".hud-neural")).toMatch(/flex-wrap:\s*wrap/);
  });

  it("wraps the STRIDE_FIT line the same way", () => {
    expect(rule(".hud-stride .hud-label")).not.toMatch(/ellipsis|nowrap/);
    expect(rule(".hud-stride .hud-label")).toMatch(/overflow-wrap:\s*anywhere/);
    expect(rule(".hud-stride")).toMatch(/flex-wrap:\s*wrap/);
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

// The STASIS and DUMP buttons sit in the same column as the VEL and TILT steppers. At a larger text
// size STASIS can't shrink below its label, so a fixed-width DUMP beside it pushed the row about
// 6 CSS px past the steppers' right edge.
describe("HUD controls share one right edge", () => {
  it("wraps DUMP under STASIS when the two don't fit side by side", () => {
    expect(rule(".hud-transport")).toMatch(/flex-wrap:\s*wrap/);
    const main = css.match(/\.hud-transport \.hud-btn-main\s*\{([^}]*)\}/)?.[1] ?? "";
    // Never narrower than its longest word; its width comes from the row, not its words, so DUMP
    // and QUIET sit at one x in every state.
    expect(main).toMatch(/min-width:\s*min-content/);
    expect(main).toMatch(/flex:\s*\d+ 1 0/);
  });

  it("lets DUMP grow into the row instead of holding a fixed width", () => {
    const stop = css.match(/\.hud-transport \.hud-btn-stop\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(stop).toMatch(/flex:\s*1 0 72px/);
  });
});

// The header's right-hand group (SIM tag, link status, minify) was wider than its own box (the minify
// button's negative margin stuck out 6 px) and, with the SIM tag at 340 px and 175 % text size, wider
// than the header (#231).
describe("HUD header end group at 340 px", () => {
  it("shrinks to the header and wraps instead of overflowing", () => {
    const end = rule(".hud-header-end");
    expect(end).not.toBe("");
    // Right to left with lines stacking upward: minify and status share the
    // bottom line, the SIM tag wraps above them.
    expect(end).toMatch(/flex-direction:\s*row-reverse/);
    expect(end).toMatch(/flex-wrap:\s*wrap-reverse/);
    expect(rule(".hud-header-end > .hud-sim-tag")).toMatch(/order:\s*1/);
    expect(rule(".hud-header-end > .hud-header-live")).toMatch(/order:\s*0/);
    // Status and minify share a line; the button may wrap under the status,
    // at the right edge, and the status word stays on one line (#420).
    expect(rule(".hud-header-live")).toMatch(/flex-wrap:\s*wrap/);
    expect(rule(".hud-header-live")).toMatch(/justify-content:\s*flex-end/);
    const status = rule(".hud-status-slot > .hud-status");
    expect(status).toMatch(/white-space:\s*nowrap/);
    expect(status).not.toMatch(/anywhere|break-all|break-word/);
    expect(end).toMatch(/min-width:\s*0/);
    expect(end).toMatch(/max-width:\s*100%/);
  });

  it("keeps negative margins off the children, so they stay inside the group's box", () => {
    expect(rule(".hud-relink")).not.toMatch(/margin:[^;]*-\d+px\s+-\d+px/);
    expect(rule(".hud-btn.hud-btn-minify")).toMatch(/margin:\s*-4px 0;/);
    expect(css).toMatch(/\.hud-header-end:has\(\.hud-btn-minify\)\s*\{\s*margin-right:\s*-6px/);
  });
});

// While the belt rests only the live stats dim: errors, notes, the link state and GO stay readable.
describe("HUD at rest", () => {
  it("dims only the stats, and not while a note shows", () => {
    expect(rule(".hud-panel.is-resting")).toBe("");
    expect(rule(".hud-panel.is-resting:not(:has(.hud-note)) .hud-stats")).toMatch(/opacity:\s*0\.8/);
    expect(rule(".hud-panel.is-resting:not(:has(.hud-note)) .hud-strip-total")).toMatch(/opacity:\s*0\.8/);
  });

  it("keeps GO's plain-words line dark on its yellow fill", () => {
    expect(rule(".hud-btn.hud-btn-go .hud-btn-sub")).toMatch(/color:\s*color-mix\(in srgb, #05080c 75%/);
  });

  it("gives the header's relink and minify buttons a 24px target", () => {
    expect(rule(".hud-relink")).toMatch(/min-height:\s*24px/);
    expect(rule(".hud-btn.hud-btn-minify")).toMatch(/height:\s*24px/);
  });
});

// NO_CARRIER and RELINK 12S stay on one line up to 225 % text size (#392).
describe("HUD link chip in a narrow header", () => {
  it("closes up its letters and padding, sizers too, under 150px of header", () => {
    expect(css).toMatch(/\.hud-header\s*\{\s*container-type:\s*inline-size;\s*\}/);
    const narrow = css.match(/@container \(max-width: 150px\)\s*\{([\s\S]*?)\n\}/)?.[1] ?? "";
    expect(narrow).toMatch(/\.hud-header:has\(\.hud-led\) \.hud-status-slot > \.hud-status\s*\{\s*letter-spacing:\s*0;/);
    expect(narrow).toMatch(/\.hud-header:has\(\.hud-led\) \.hud-status-slot > \.hud-relink\s*\{\s*padding:\s*3px 2px;/);
    const narrower = css.match(/@container \(max-width: 170px\)\s*\{([\s\S]*?)\n\}/)?.[1] ?? "";
    expect(narrower).toMatch(/\.hud-header-row\s*\{\s*padding-left:\s*8px;\s*padding-right:\s*8px;/);
  });
});

// The header wrapped by the status word at 175 % and up, moving every control under it (#420).
describe("HUD header keeps one height whatever the chip reads", () => {
  it("gives the logo one letter spacing in every state", () => {
    expect(rule(".hud-title")).toMatch(/letter-spacing:\s*0\.2em/);
    expect(css).not.toMatch(/tone-(down|warn)\)[^{]*\.hud-title\s*\{/);
  });

  it("lays the chip's word over its widest words, unseen, in one cell", () => {
    expect(rule(".hud-status-slot")).toMatch(/display:\s*grid/);
    expect(rule(".hud-status-slot > .hud-status")).toMatch(/grid-area:\s*1 \/ 1/);
    const sizer = rule(".hud-status-slot > .hud-status-sizer");
    expect(sizer).toMatch(/visibility:\s*hidden/);
    expect(sizer).toMatch(/box-sizing:\s*border-box/);
    // Each sizer at the spacing its words take when shown.
    expect(rule(".hud-header .hud-status-slot > .hud-status-sizer.is-deck")).toMatch(/letter-spacing:\s*0\.18em/);
    expect(rule(".hud-header .hud-status-slot > .hud-status-sizer.is-link")).toMatch(/letter-spacing:\s*0\.04em/);
  });

  it("keeps the LED with the logo, so neither takes a line alone", () => {
    expect(rule(".hud-header-brand")).toMatch(/display:\s*inline-flex/);
    expect(rule(".hud-header-row")).toMatch(/flex-wrap:\s*wrap/);
  });
});

// The strip's stat slot pushed ENGAGE onto its own line where TODAY didn't (#436), and its
// note covered the logo row (#435).
describe("HUD strip keeps ENGAGE and the logo row in place", () => {
  it("sizes the stat slot from the row, never from what it shows", () => {
    const cell = rule(".hud-strip-cell");
    expect(cell).toMatch(/flex:\s*1 1 0;/);
    expect(cell).toMatch(/min-width:\s*60px/);
    expect(rule(".hud-strip-cell > .hud-strip-total")).toMatch(/min-width:\s*0/);
  });

  it("keeps the slot's value one line, its units wrapping out of sight", () => {
    const value = rule(".hud-strip-cell .hud-value");
    expect(value).toMatch(/flex-wrap:\s*wrap/);
    expect(value).toMatch(/height:\s*calc\(15px \* 1\.1\)/);
    expect(value).toMatch(/overflow:\s*hidden/);
    expect(rule(".hud-strip-cell .hud-value .hud-stasis-clock")).toMatch(/display:\s*contents/);
  });

  it("keeps the note inside the strip: from the header's edge to the row's foot", () => {
    const pop = rule(".hud-strip-cell > .hud-note-pop");
    expect(pop).toMatch(/top:\s*-7px/);
    expect(pop).toMatch(/bottom:\s*-6px/);
    expect(pop).toMatch(/overflow:\s*hidden/);
    expect(pop).not.toMatch(/min-height/);
    expect(rule(".hud-strip-cell > .hud-note-pop > .hud-note-text")).toMatch(/-webkit-line-clamp:\s*var\(--note-lines/);
  });
});

// A note, a quiet line or the link going down never moves a control (#387).
describe("HUD controls stay put", () => {
  it("draws the panel's note over the readings, out of the layout, letting clicks through", () => {
    const pop = rule(".hud-note.hud-note-pop");
    expect(pop).toMatch(/position:\s*absolute/);
    expect(pop).toMatch(/bottom:\s*0/);
    expect(pop).toMatch(/pointer-events:\s*none/);
  });

  it("keeps the status row one fixed line and shows only its first line", () => {
    expect(rule(".hud-status-row")).toMatch(/height:\s*30px/);
    expect(rule(".hud-status-line > :not(:first-child)")).toMatch(/display:\s*none/);
  });

  it("sizes QUIET for both of its lines, so turning it on moves nothing", () => {
    expect(rule(".hud-swap > *")).toMatch(/grid-area:\s*1 \/ 1/);
    expect(rule(".hud-swap > :not(.is-shown)")).toMatch(/visibility:\s*hidden/);
  });

  it("keeps the header one line while the link word is long", () => {
    expect(css).toMatch(/\.hud-header:has\(\.hud-led\.tone-down, \.hud-led\.tone-warn\) \.hud-status \{ letter-spacing: 0\.04em; \}/);
  });

  it("dims the readings while no treadmill is linked", () => {
    expect(rule(".hud-panel.is-offline .hud-stats")).toMatch(/opacity:\s*0\.45/);
  });
});
