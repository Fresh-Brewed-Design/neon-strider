import { beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "./slices/settingsSlice";
import { useTreadmillStore } from "./treadmillStore";

const point = (speed: number, distance_scale = 1) => ({ speed, stride_length_cm: 74, distance_scale });

describe("settings slice", () => {
  beforeEach(() => useTreadmillStore.getState().updateSettings(DEFAULT_SETTINGS));

  it("keeps calibration points sorted and removes by index", () => {
    const s = useTreadmillStore.getState();
    s.updateSettings({ calibrationPoints: [point(4), point(2), point(6)] });
    expect(useTreadmillStore.getState().calibrationPoints.map((p) => p.speed)).toEqual([2, 4, 6]);
    s.removeCalibrationPoint(0);
    expect(useTreadmillStore.getState().calibrationPoints.map((p) => p.speed)).toEqual([4, 6]);
  });

  it("clamps and rounds the neural floor", () => {
    const s = useTreadmillStore.getState();
    s.updateSettings({ neuralFloorPct: 99 });
    expect(useTreadmillStore.getState().neuralFloorPct).toBe(95);
    s.updateSettings({ neuralFloorPct: 62.6 });
    expect(useTreadmillStore.getState().neuralFloorPct).toBe(63);
  });
});
