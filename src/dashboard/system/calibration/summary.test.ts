import { beforeEach, describe, expect, it } from "vitest";
import type { CalibrationPoint } from "../../../store/types";
import { ago, readMeasured, stampMeasured, summaryParts } from "./summary";

const point = (speed: number, scale = 0.9): CalibrationPoint => ({ speed, stride_length_cm: 68, distance_scale: scale, runs: 1, stride_runs: 0 });
const NOON = new Date(2026, 10, 25, 12).getTime();
const DAY = 86_400_000;

describe("calibration summary", () => {
  beforeEach(() => localStorage.clear());

  it("counts points and gives the speed range", () => {
    expect(summaryParts([point(2), point(5)], null, NOON)).toEqual(["2 points", "2.0 to 5.0 km/h"]);
    expect(summaryParts([point(3)], null, NOON)).toEqual(["1 point", "3.0 km/h"]);
  });

  it("adds the day when the points were seen changing", () => {
    expect(summaryParts([point(3)], NOON - 3 * DAY, NOON)).toEqual(["1 point", "3.0 km/h", "Measured 3 days ago"]);
  });

  it("names today and yesterday", () => {
    expect(ago(NOON - 3600_000, NOON)).toBe("today");
    expect(ago(NOON - DAY, NOON)).toBe("yesterday");
  });

  it("remembers a stamp only for the points it was taken for", () => {
    stampMeasured([point(3)], NOON);
    expect(readMeasured([point(3)])).toBe(NOON);
    expect(readMeasured([point(3, 0.95)])).toBeNull();
    expect(readMeasured([])).toBeNull();
  });
});
