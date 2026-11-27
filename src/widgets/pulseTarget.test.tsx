import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { snapshot } from "../dev/mockBackend";
import { mockPulseTarget } from "../dev/scenarios/hrcool";
import type { AppSnapshot, PulseTarget } from "../ipc";
import { StatCell } from "../hud/HudPanel";
import { TargetReadout } from "../dashboard/live/TargetReadout";
import { statWidget } from "./registry";
import { MIN_SPAN_BPM, sparkGeometry } from "./spark";

function withTarget(target: PulseTarget | null, bpm: number | null = 126): AppSnapshot {
  return {
    ...snapshot,
    heart_rate: {
      ...snapshot.heart_rate,
      sensor: { id: "x", name: "Watch" },
      status: "connected",
      bpm,
      history: [140, 132, bpm],
      zone: null,
      cool_down: target,
    },
  };
}

describe("PULSE_RATE cool-down target (#233)", () => {
  it("reads the target Rust sends and nothing without one", () => {
    expect(statWidget("HEART_RATE")?.target?.(snapshot)).toBeNull();
    const t = mockPulseTarget(114, 126, 30);
    expect(statWidget("HEART_RATE")?.target?.(withTarget(t))).toEqual(t);
    expect(statWidget("VELOCITY")?.target).toBeUndefined();
  });

  it("shows the target under the HUD value, warm while the pulse is high and green once down", () => {
    const { unmount } = render(<StatCell id="HEART_RATE" snapshot={withTarget(mockPulseTarget(114, 126, 30))} />);
    expect(screen.getByText("PULSE // HIGH")).toHaveClass("is-placed", "accent-yellow");
    expect(screen.getByTestId("hud-pulse-target")).toHaveTextContent("to 114 BPM");
    unmount();
    render(<StatCell id="HEART_RATE" snapshot={withTarget(mockPulseTarget(114, 111, 18), 111)} />);
    expect(screen.getByText("PULSE // HOLD 18S")).toHaveClass("is-placed", "accent-green");
  });

  it("dims the word without a reading, and leaves a plain cell as it was", () => {
    const { unmount } = render(<StatCell id="HEART_RATE" snapshot={withTarget(mockPulseTarget(114, null, 30), null)} />);
    expect(screen.getByText("PULSE // WAIT")).not.toHaveClass("is-placed");
    unmount();
    render(<StatCell id="HEART_RATE" snapshot={withTarget(null)} />);
    expect(screen.queryByTestId("hud-pulse-target")).toBeNull();
  });

  it("shows the word and target on the Live page, with Rust's sentence", () => {
    render(<TargetReadout target={mockPulseTarget(114, 126, 30)} />);
    expect(screen.getByTestId("live-pulse-target")).toHaveTextContent("PULSE // HIGH");
    expect(screen.getByTestId("live-pulse-target")).toHaveTextContent("to 114 BPM");
    expect(screen.getByTestId("live-pulse-target")).toHaveAttribute("title", "The cool-down step walks on until heart rate drops to the pulse target.");
  });

  it("shades the chart from the target down", () => {
    const g = sparkGeometry({ minSpan: MIN_SPAN_BPM, points: [100, 140], zone: null, target: 120 }, 100, 24);
    // 100 to 140 over 21 units of height inside a 1.5 edge: 120 is the middle.
    expect(g?.band?.y).toBeCloseTo(1.5 + 10.5, 5);
    expect(g?.band?.height).toBeCloseTo(24 - 12, 5);
    expect(statWidget("HEART_RATE")?.spark?.(withTarget(mockPulseTarget(114, 126, 30)))?.target).toBe(114);
  });

  it("keeps every target sentence short with no dashes", () => {
    for (const [bpm, hold] of [[126, 30], [111, 18], [100, null], [null, 30]] as const) {
      const { message, label } = mockPulseTarget(114, bpm, hold);
      expect(message.split(". ")).toHaveLength(1);
      expect(message).not.toMatch(/[—–;]/);
      expect(label).toMatch(/^PULSE \/\/ [A-Z0-9 ]+$/);
    }
  });
});
