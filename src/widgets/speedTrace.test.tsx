import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { snapshot } from "../dev/mockBackend";
import type { AppSnapshot } from "../ipc";
import { StatCell } from "../hud/HudPanel";
import { statWidget } from "./registry";
import { MIN_SPAN_BPM, MIN_SPAN_KMH, sparkGeometry } from "./spark";

/** The y of each point on the line's first run. */
const ys = (line: string) =>
  (line.split("M").filter(Boolean)[0].match(/[\d.]+ ([\d.]+)/g) ?? []).map((m) => Number(m.split(" ")[1]));

const withTrace = (speed_trace: (number | null)[]): AppSnapshot => ({ ...snapshot, speed_trace });

describe("spark chart minimum span", () => {
  it("spans at least 1 km/h for speed, so a steady walk stays near flat", () => {
    const g = sparkGeometry({ points: [3.45, 3.55], minSpan: MIN_SPAN_KMH, zone: null }, 100, 24);
    const [a, b] = ys(g?.line ?? "");
    // 0.1 km/h of a 1 km/h scale over 21 units of height.
    expect(Math.abs(a - b)).toBeCloseTo(0.1 * 21, 5);
  });

  it("fills the height once the speed moves more than the span", () => {
    const g = sparkGeometry({ points: [2, 3.5], minSpan: MIN_SPAN_KMH, zone: null }, 100, 24);
    expect(g?.line).toBe("M0 22.5 L100 1.5");
  });

  it("keeps PULSE_RATE at its 20 bpm span", () => {
    const g = sparkGeometry({ points: [110, 112], minSpan: MIN_SPAN_BPM, zone: null }, 100, 24);
    const [a, b] = ys(g?.line ?? "");
    expect(Math.abs(a - b)).toBeCloseTo((2 / 20) * 21, 5);
  });
});

describe("VELOCITY speed trace", () => {
  it("has no chart while Rust sends no trace", () => {
    expect(statWidget("VELOCITY")?.spark?.(withTrace([]))).toBeNull();
  });

  it("reads the trace from the snapshot with the km/h span and no band", () => {
    const trace = [3.5, null, 2.9];
    expect(statWidget("VELOCITY")?.spark?.(withTrace(trace))).toEqual({ points: trace, minSpan: MIN_SPAN_KMH, zone: null });
  });

  it("draws the chart under VEL on the HUD", () => {
    const { container, rerender } = render(<StatCell id="VELOCITY" snapshot={withTrace([])} />);
    expect(container.querySelector("[data-testid=sparkline]")).toBeNull();
    rerender(<StatCell id="VELOCITY" snapshot={withTrace([3.4, 3.5, 3.5])} />);
    expect(container.querySelector(".hud-stat [data-testid=sparkline]")).not.toBeNull();
  });

  it("keeps the chart's room on the HUD before the first walk, so nothing under VEL moves", () => {
    const { container, rerender } = render(<StatCell id="VELOCITY" snapshot={withTrace([])} />);
    expect(container.querySelector("[data-testid=hud-spark-reserve]")).not.toBeNull();
    rerender(<StatCell id="VELOCITY" snapshot={withTrace([3.4, 3.5, 3.5])} />);
    expect(container.querySelector("[data-testid=hud-spark-reserve]")).toBeNull();
  });

  it("reserves no room under PULSE_RATE without a sensor", () => {
    const { container } = render(<StatCell id="HEART_RATE" snapshot={{ ...snapshot, heart_rate: { ...snapshot.heart_rate, history: [] } }} />);
    expect(container.querySelector("[data-testid=hud-spark-reserve]")).toBeNull();
  });
});
