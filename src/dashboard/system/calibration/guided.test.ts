import { describe, expect, it } from "vitest";
import type { CalibrationGuideView } from "../../../ipc";
import {
  calibrationRows,
  checkLine,
  chrono,
  deckVerdict,
  droppedLine,
  guideSummary,
  listedTarget,
  runRowFor,
  speedList,
} from "./guided";

const point = (speed: number) => ({ speed, stride_length_cm: 70, distance_scale: 0.9 });

describe("calibrationRows", () => {
  it("offers every target and marks the ones already measured", () => {
    const rows = calibrationRows([point(3.0), point(4.5)], [2, 3, 4]);
    expect(rows.map((r) => [r.targetKmh, r.index])).toEqual([
      [2, null],
      [3, 0],
      [4, null],
      [4.5, 1],
    ]);
  });

  it("treats a point within 0.05 km/h as that target's", () => {
    const rows = calibrationRows([point(2.96)], [3]);
    expect(rows).toHaveLength(1);
    expect(rows[0].index).toBe(0);
  });
});

describe("chrono", () => {
  it("shows minutes, seconds and tenths", () => {
    expect(chrono(0)).toBe("0:00.0");
    expect(chrono(7.25)).toBe("0:07.3");
    expect(chrono(83.4)).toBe("1:23.4");
    expect(chrono(59.96)).toBe("1:00.0");
  });
});

describe("runRowFor", () => {
  it("finds the row of a run's target, saved point or not", () => {
    const rows = calibrationRows([point(2.96)], [3, 4]);
    expect(runRowFor(rows, 2.96)?.index).toBe(0);
    expect(runRowFor(rows, 4)?.targetKmh).toBe(4);
    expect(runRowFor(rows, 5)).toBeNull();
  });
});

describe("listedTarget", () => {
  it("finds the list entry for a row's speed", () => {
    const targets = [{ kmh: 2, runnable: true }, { kmh: 14, runnable: false }];
    expect(listedTarget(targets, 14.02)).toEqual({ kmh: 14, runnable: false });
    expect(listedTarget(targets, 3)).toBeNull();
  });
});

describe("deckVerdict", () => {
  it("says how far the deck is from the belt, in plain words", () => {
    expect(deckVerdict(3, 2.82, 3)).toBe("At 3.0 km/h the deck runs 6.0 % slow.");
    expect(deckVerdict(4, 4.6, 4)).toBe("At 4.0 km/h the deck runs 15 % fast.");
    expect(deckVerdict(3, 3.01, 3)).toBe("At 3.0 km/h the deck matches the belt.");
    expect(deckVerdict(3, 2.82, 3, "saved")).toBe("At 3.0 km/h the deck runs 6.0 % slow; saved.");
    expect(deckVerdict(3, 3.01, 3, "undone")).toBe("At 3.0 km/h the deck matches the belt; undone.");
  });
});

describe("droppedLine", () => {
  it("names dropped taps only when there are some", () => {
    expect(droppedLine(0)).toBeNull();
    expect(droppedLine(1)).toBe("1 tap dropped as a miss or a double.");
    expect(droppedLine(3)).toBe("3 taps dropped as misses or doubles.");
  });
});

describe("the guided walk's summary", () => {
  const view = (g: Partial<CalibrationGuideView>): CalibrationGuideView => ({
    slow_kmh: 0.6,
    top_kmh: 5,
    note: null,
    speeds: [],
    checks: [],
    next_kmh: null,
    remaining_error_pct: null,
    tolerance_pct: 2,
    ...g,
  });
  const saved = (kmh: number[]) => kmh.map((k) => ({ kmh: k, ran_kmh: null, mark: "saved" as const, end: false }));

  it("lists speeds in plain words", () => {
    expect(speedList([0.6])).toBe("0.6 km/h");
    expect(speedList([0.6, 5])).toBe("0.6 and 5.0 km/h");
    expect(speedList([0.6, 2.8, 5])).toBe("0.6, 2.8 and 5.0 km/h");
  });

  it("says when distance is still off after the last check level", () => {
    const lines = guideSummary(
      view({
        speeds: saved([0.6, 2.8, 5]),
        checks: [{ lo_kmh: 0.6, hi_kmh: 5, mid_kmh: 2.8, halfway_kmh: null, ran_kmh: null, verdict: "at_limit", error_pct: 2.6 }],
        remaining_error_pct: 2.6,
      }),
    );
    expect(lines).toContain("Distance is still up to 2.6 % off between some calibrated speeds.");
  });

  it("says a skipped check leaves the error unknown", () => {
    const g = view({
      speeds: saved([0.6, 5]),
      checks: [{ lo_kmh: 0.6, hi_kmh: 5, mid_kmh: 2.8, halfway_kmh: null, ran_kmh: null, verdict: "skipped", error_pct: null }],
    });
    expect(guideSummary(g)).toEqual(["Calibrated 0.6 and 5.0 km/h.", "A skipped check leaves the distance error at its midpoint unknown."]);
    expect(checkLine(g.checks[0], 2)).toBe("0.6 to 5.0 km/h: the check at 2.8 km/h was skipped, so the error there is unknown.");
  });

  it("says when the deck skips the halfway speed and the check moved", () => {
    const c = { lo_kmh: 0.6, hi_kmh: 5, mid_kmh: 2.7, halfway_kmh: 2.8, ran_kmh: null, verdict: "pending" as const, error_pct: null };
    expect(checkLine(c, 2)).toBe("0.6 to 5.0 km/h: the deck skips 2.8, so the check at 2.7 km/h waits for RUN.");
    expect(checkLine({ ...c, halfway_kmh: null }, 2)).toBe("0.6 to 5.0 km/h: the check at 2.7 km/h waits for RUN.");
  });

  it("says when no deck speed fits between the two ends", () => {
    expect(guideSummary(view({ top_kmh: 0.7, speeds: saved([0.6, 0.7]) }))).toEqual([
      "Calibrated 0.6 and 0.7 km/h.",
      "The deck has no speed between the two calibrated speeds to check.",
    ]);
  });
});
