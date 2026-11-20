import { describe, expect, it } from "vitest";
import { MAX_TARGETS, addTarget, calibrationRows, chrono, parseTargets, removeTarget, runRowFor, targetProblem } from "./guided";

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

describe("target speed list", () => {
  const defaults = [2, 3, 4];

  it("falls back to the defaults when nothing usable is saved", () => {
    expect(parseTargets(null, defaults, 0.6, 12)).toEqual([2, 3, 4]);
    expect(parseTargets("not json", defaults, 0.6, 12)).toEqual([2, 3, 4]);
    expect(parseTargets('{"a":1}', defaults, 0.6, 12)).toEqual([2, 3, 4]);
  });

  it("keeps a saved list sorted, in range and without repeats, even an empty one", () => {
    expect(parseTargets("[5.04,2,99,2.0,\"x\"]", defaults, 0.6, 12)).toEqual([2, 5]);
    expect(parseTargets("[]", defaults, 0.6, 12)).toEqual([]);
  });

  it("adds in order and removes by speed", () => {
    expect(addTarget([2, 4], 3.5)).toEqual([2, 3.5, 4]);
    expect(removeTarget([2, 3.5, 4], 3.5)).toEqual([2, 4]);
  });

  it("says why a speed can't be added", () => {
    expect(targetProblem([2], NaN, 0.6, 12)).toMatch(/Enter/);
    expect(targetProblem([2], 20, 0.6, 12)).toMatch(/from 0.6 to 12.0/);
    expect(targetProblem([2], 2.02, 0.6, 12)).toMatch(/already/);
    expect(targetProblem(Array.from({ length: MAX_TARGETS }, (_, i) => 1 + i * 0.5), 11, 0.6, 12)).toMatch(/at most/);
    expect(targetProblem([2], 3, 0.6, 12)).toBeNull();
  });
});
