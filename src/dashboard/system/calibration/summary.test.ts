import { describe, expect, it } from "vitest";
import type { CalibrationPoint } from "../../../store/types";
import { summaryParts } from "./summary";

const point = (speed: number): CalibrationPoint => ({ speed, stride_length_cm: 68, distance_scale: 0.9, runs: 1, stride_runs: 0 });

describe("calibration summary", () => {
  it("counts points and gives the speed range", () => {
    expect(summaryParts([point(2), point(5)])).toEqual(["2 points", "2.0 to 5.0 km/h"]);
    expect(summaryParts([point(3)])).toEqual(["1 point", "3.0 km/h"]);
  });
});
