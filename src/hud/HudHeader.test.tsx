import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HudHeader, SCRAMBLE_FRAMES } from "./HudHeader";
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
