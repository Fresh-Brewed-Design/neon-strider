import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { snapshot } from "../dev/mockBackend";
import { mockZone } from "../dev/scenarios/hrzone";
import type { AppSnapshot, ZoneBand } from "../ipc";
import { StatCell } from "../hud/HudPanel";
import { ZoneReadout } from "../dashboard/live/ZoneReadout";
import { statWidget } from "./registry";
import { zoneRange } from "./zone";

function withZone(zone: ZoneBand | null, bpm: number | null = 143): AppSnapshot {
  return {
    ...snapshot,
    heart_rate: {
      ...snapshot.heart_rate,
      sensor: { id: "x", name: "Watch" },
      status: "connected",
      bpm,
      history: [140, 141, bpm],
      zone,
    },
  };
}

describe("PULSE_RATE heart rate zone", () => {
  it("reads the zone Rust sends and nothing without one", () => {
    expect(statWidget("HEART_RATE")?.zone?.(snapshot)).toBeNull();
    const zone = mockZone(133, 152, 143);
    expect(statWidget("HEART_RATE")?.zone?.(withZone(zone))).toEqual(zone);
    // Other stats have no zone.
    expect(statWidget("VELOCITY")?.zone).toBeUndefined();
  });

  it("writes the range in words, with no dash", () => {
    expect(zoneRange(mockZone(133, 152, null))).toBe("133 to 152");
  });

  it.each([
    [124, "ZONE // UNDER", "accent-cyan"],
    [143, "ZONE // LOCKED", "accent-green"],
    [161, "ZONE // OVER", "accent-yellow"],
  ])("colours the HUD word for %i bpm", (bpm, label, accent) => {
    render(<StatCell id="HEART_RATE" snapshot={withZone(mockZone(133, 152, bpm), bpm)} />);
    const word = screen.getByText(label);
    expect(word).toHaveClass("hud-zone-label", "is-placed", accent);
    expect(screen.getByText("133 to 152")).toBeInTheDocument();
    // Rust's sentence is the tooltip.
    expect(screen.getByTestId("hud-zone")).toHaveAttribute("title", mockZone(133, 152, bpm).message);
  });

  it("shows the zone dimmed while there is no reading", () => {
    render(<StatCell id="HEART_RATE" snapshot={withZone(mockZone(133, 152, null), null)} />);
    expect(screen.getByText("ZONE // WAIT")).not.toHaveClass("is-placed");
    expect(screen.getByText("--")).toBeInTheDocument();
  });

  it("says SETTLING while the step waits for the pulse, coloured by its position", () => {
    const zone = mockZone(133, 152, 124, 75);
    render(<StatCell id="HEART_RATE" snapshot={withZone(zone, 124)} />);
    expect(screen.getByText("ZONE // SETTLING")).toHaveClass("is-placed", "accent-cyan");
    expect(screen.getByTestId("hud-zone")).toHaveAttribute("title", "Heart rate is settling before the zone trims.");
    expect(zone.message.split(". ")).toHaveLength(1);
  });

  it("leaves the HUD cell as it was without a zone", () => {
    const { container } = render(<StatCell id="HEART_RATE" snapshot={withZone(null)} />);
    expect(screen.queryByTestId("hud-zone")).toBeNull();
    expect(container.querySelector("[data-testid=sparkline]")).not.toBeNull();
  });

  it("writes the word and range on the Live page, with the sentence as its tooltip", () => {
    render(<ZoneReadout zone={mockZone(133, 152, 161)} />);
    expect(screen.getByText("ZONE // OVER")).toHaveClass("text-neon-yellow");
    expect(screen.getByTestId("live-zone")).toHaveAttribute("title", "Heart rate is above the zone.");
    expect(screen.getByTestId("live-zone")).toHaveTextContent("133 to 152 BPM");
  });

  it("keeps every zone sentence short with no dashes", () => {
    for (const bpm of [120, 140, 160, null]) {
      const { message } = mockZone(130, 150, bpm);
      expect(message.split(". ")).toHaveLength(1);
      expect(message).not.toMatch(/[—–;]/);
    }
  });
});
