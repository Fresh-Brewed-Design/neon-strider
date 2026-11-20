import { describe, expect, it } from "vitest";
import { calibrationRows, chrono } from "./guided";

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
