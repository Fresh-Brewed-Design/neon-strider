import { beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS } from "./slices/settingsSlice";
import { useTreadmillStore } from "./treadmillStore";

const point = (speed: number, distance_scale = 1) => ({ speed, stride_length_cm: 74, distance_scale });

describe("settings slice", () => {
  beforeEach(() => useTreadmillStore.getState().updateSettings(DEFAULT_SETTINGS));

  it("keeps calibration points sorted and replaces one at nearly the same speed", () => {
    const s = useTreadmillStore.getState();
    s.addCalibrationPoint(point(5));
    s.addCalibrationPoint(point(3));
    s.addCalibrationPoint(point(5.04, 1.2));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([point(3), point(5.04, 1.2)]);
  });

  it("re-sorts after an edit and removes by index", () => {
    const s = useTreadmillStore.getState();
    s.updateSettings({ calibrationPoints: [point(4), point(2)] });
    expect(useTreadmillStore.getState().calibrationPoints.map((p) => p.speed)).toEqual([2, 4]);
    s.updateCalibrationPoint(0, point(6));
    expect(useTreadmillStore.getState().calibrationPoints.map((p) => p.speed)).toEqual([4, 6]);
    s.removeCalibrationPoint(0);
    expect(useTreadmillStore.getState().calibrationPoints.map((p) => p.speed)).toEqual([6]);
  });

  it("clamps and rounds the neural floor", () => {
    const s = useTreadmillStore.getState();
    s.updateSettings({ neuralFloorPct: 99 });
    expect(useTreadmillStore.getState().neuralFloorPct).toBe(95);
    s.updateSettings({ neuralFloorPct: 62.6 });
    expect(useTreadmillStore.getState().neuralFloorPct).toBe(63);
  });
});

describe("calibration point validation", () => {
  beforeEach(() => useTreadmillStore.getState().updateSettings(DEFAULT_SETTINGS));

  it("never stores a point with speed 0 or scale 0", () => {
    const s = useTreadmillStore.getState();
    s.addCalibrationPoint(point(0, 1));
    s.addCalibrationPoint(point(5, 0));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([]);
    s.addCalibrationPoint(point(5));
    s.updateCalibrationPoint(0, point(5, 0));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([point(5)]);
  });
});
