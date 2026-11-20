import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { snapshot } from "../dev/mockBackend";
import { statWidget } from "./registry";
import { MIN_SPAN_BPM, sparkGeometry } from "./spark";
import { Sparkline } from "./Sparkline";

/** The subpaths of a path, one per run of readings. */
const runs = (d: string) => d.split("M").filter(Boolean);

describe("sparkGeometry", () => {
  it("draws nothing with no points, so no sensor means no chart", () => {
    expect(sparkGeometry({ points: [], zone: null }, 100, 24)).toBeNull();
  });

  it("puts the oldest point at the left and the newest at the right", () => {
    const g = sparkGeometry({ points: [100, 120, 140], zone: null }, 100, 24);
    expect(g?.line).toBe("M0 22.5 L50 12 L100 1.5");
  });

  it("breaks the line at a gap instead of joining across it", () => {
    const g = sparkGeometry({ points: [100, 110, null, null, 120, 130], zone: null }, 100, 24);
    const parts = runs(g?.line ?? "");
    expect(parts).toHaveLength(2);
    expect(g?.line).not.toMatch(/L40 |L60 /);
    expect(parts[1].startsWith("80 ")).toBe(true);
  });

  it("shows a lone reading between gaps as a dot", () => {
    const g = sparkGeometry({ points: [null, 110, null, 100, 120], zone: null }, 100, 24);
    const parts = runs(g?.line ?? "");
    expect(parts).toHaveLength(2);
    expect(parts[0]).toMatch(/h0\s*$/);
    expect(parts[1]).not.toMatch(/h0/);
  });

  it("draws no line when every step is a gap", () => {
    const g = sparkGeometry({ points: [null, null, null], zone: null }, 100, 24);
    expect(g).toEqual({ line: "", band: null });
  });

  it("keeps a steady walk flat by spanning at least the minimum", () => {
    // 110 and 112 sit near the middle, not at the top and bottom edges.
    const g = sparkGeometry({ points: [110, 112], zone: null }, 100, 24);
    const ys = runs(g?.line ?? "")[0].match(/[\d.]+ ([\d.]+)/g)?.map((m) => Number(m.split(" ")[1])) ?? [];
    const spread = Math.abs(ys[0] - ys[1]);
    expect(spread).toBeCloseTo((2 / MIN_SPAN_BPM) * 21, 5);
  });

  it("draws the zone as a band from its high to its low bound", () => {
    const g = sparkGeometry({ points: [100, 140], zone: { low_bpm: 120, high_bpm: 130 } }, 100, 24);
    // 100 to 140 over 21 units of height inside a 1.5 edge.
    expect(g?.band?.y).toBeCloseTo(1.5 + (10 / 40) * 21, 5);
    expect(g?.band?.height).toBeCloseTo((10 / 40) * 21, 5);
  });

  it("widens the scale to fit a zone above the line", () => {
    const g = sparkGeometry({ points: [100, 100], zone: { low_bpm: 150, high_bpm: 160 } }, 100, 24);
    expect(g?.band?.y).toBeCloseTo(1.5, 5);
    expect(g?.line).toBe("M0 22.5 L100 22.5");
  });

  it("shows the band even before a reading arrives", () => {
    const g = sparkGeometry({ points: [null, null], zone: { low_bpm: 120, high_bpm: 140 } }, 100, 24);
    expect(g?.line).toBe("");
    expect(g?.band?.height).toBeGreaterThan(0);
  });
});

describe("PULSE_RATE spark chart", () => {
  it("has no chart while no sensor is picked", () => {
    expect(statWidget("HEART_RATE")?.spark?.(snapshot)).toBeNull();
  });

  it("reads the history and zone from the snapshot", () => {
    const history = [100, null, 104];
    const zone = { low_bpm: 120, high_bpm: 140 };
    const s = { ...snapshot, heart_rate: { ...snapshot.heart_rate, history, zone } };
    expect(statWidget("HEART_RATE")?.spark?.(s)).toEqual({ points: history, zone });
  });

  it("renders a band only with a zone", () => {
    const { getByTestId, queryByTestId, rerender } = render(<Sparkline data={{ points: [100, 110], zone: null }} />);
    expect(getByTestId("sparkline")).toBeTruthy();
    expect(queryByTestId("spark-band")).toBeNull();
    rerender(<Sparkline data={{ points: [100, 110], zone: { low_bpm: 105, high_bpm: 115 } }} />);
    expect(getByTestId("spark-band")).toBeTruthy();
  });
});
