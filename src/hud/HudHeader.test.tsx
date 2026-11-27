import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HudHeader, SCRAMBLE_FRAMES } from "./HudHeader";
import { CHIP_SIZERS } from "./status";
import { ADVANCE, scrambled } from "./titleScramble";

const header = (simulated?: boolean) =>
  render(
    <HudHeader
      link={{ text: "LINKED", tone: "live" }}
      relink={null}
      onRelink={() => {}}
      logo={{} as Parameters<typeof HudHeader>[0]["logo"]}
      dragging={false}
      simulated={simulated}
    />,
  );

describe("HudHeader", () => {
  it("tags the dev builds' simulated treadmill", () => {
    header(true);
    expect(screen.getByText("SIM")).toBeInTheDocument();
  });

  it("shows no tag for a real treadmill", () => {
    header(false);
    expect(screen.queryByText("SIM")).not.toBeInTheDocument();
  });
});

// The header wrapped by the status word, moving every control under it (#420).
describe("the link chip's cell", () => {
  it("lays the widest words unseen beside the shown one, hidden from screen readers", () => {
    header();
    const slot = document.querySelector(".hud-status-slot");
    const sizers = Array.from(slot?.querySelectorAll(".hud-status-sizer") ?? []);
    expect(sizers.map((s) => s.textContent)).toEqual(CHIP_SIZERS.map((w) => w.text));
    sizers.forEach((s) => expect(s).toHaveAttribute("aria-hidden", "true"));
    expect(slot?.querySelector(".hud-status:not(.hud-status-sizer)")).toHaveTextContent("LINKED");
  });

  it("sizes for the longest deck word and the longest link words", () => {
    const words = CHIP_SIZERS.map((w) => w.text);
    for (const w of ["SPIN-DOWN", "NO_CARRIER", "RELINK 120S", "LINKING 3/3", "STATE ?255"]) expect(words).toContain(w);
  });
});

describe("LINK UP scramble", () => {
  it("never draws a glyph wider than the letter it covers, and keeps the space", () => {
    const title = "NEON STRIDER";
    for (let run = 0; run < 200; run++) {
      for (let frame = 1; frame < SCRAMBLE_FRAMES; frame++) {
        const noise = Array.from(scrambled(frame));
        expect(noise).toHaveLength(title.length);
        expect(noise[4]).toBe(" ");
        noise.forEach((g, i) => {
          if (title[i] !== " ") expect(ADVANCE[g]).toBeLessThanOrEqual(ADVANCE[title[i]]);
        });
      }
    }
  });
});
