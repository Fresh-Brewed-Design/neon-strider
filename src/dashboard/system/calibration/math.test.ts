import { describe, expect, it } from "vitest";
import { averagedPoint, retimedPoint, wizardResult, type WizardInput } from "./math";

describe("retimedPoint", () => {
  it("compares the reported speed with the walked one", () => {
    // 50 steps of 100 cm in 30 s is 50 m in 30 s: 6 km/h.
    const r = retimedPoint(100, 30, 6.6);
    expect(r.realKmh).toBeCloseTo(6, 6);
    expect(r.scale).toBeCloseTo(1.1, 6);
  });

  it("gives a neutral scale when nothing was timed", () => {
    expect(retimedPoint(100, 0, 5)).toEqual({ realKmh: 0, scale: 1 });
  });
});

describe("wizardResult", () => {
  const base: WizardInput = {
    mode: "MANUAL",
    strideCm: 80,
    logoPasses: 10,
    beltLengthCm: 300,
    steps: 50,
    elapsedSeconds: 36,
    reportedKmh: 4.4,
  };

  it("uses the stride in manual mode", () => {
    // 50 x 80 cm = 40 m in 36 s: 4 km/h.
    const r = wizardResult(base);
    expect(r.distanceM).toBeCloseTo(40, 6);
    expect(r.scale).toBeCloseTo(1.1, 6);
    expect(r.strideCm).toBeCloseTo(80, 6);
  });

  it("uses belt laps in logo mode and derives the stride from them", () => {
    // 10 laps of 300 cm = 30 m over 50 steps: 60 cm a step.
    const r = wizardResult({ ...base, mode: "LOGO" });
    expect(r.distanceM).toBeCloseTo(30, 6);
    expect(r.strideCm).toBeCloseTo(60, 6);
  });
});

describe("averagedPoint", () => {
  const saved = { speed: 3, stride_length_cm: 70, distance_scale: 0.9, runs: 2 };

  it("weighs every run equally", () => {
    const p = averagedPoint(saved, { stride_length_cm: 76, distance_scale: 1.2 }, true);
    expect(p.runs).toBe(3);
    expect(p.stride_length_cm).toBeCloseTo(72, 5);
    expect(p.distance_scale).toBeCloseTo(1.0, 5);
  });

  it("counts a point from before runs were tracked as one run", () => {
    const p = averagedPoint({ speed: 3, stride_length_cm: 70, distance_scale: 1 }, { stride_length_cm: 70, distance_scale: 1.2 }, true);
    expect(p.runs).toBe(2);
    expect(p.distance_scale).toBeCloseTo(1.1, 5);
  });

  it("keeps the saved stride when the run counted no steps", () => {
    expect(averagedPoint(saved, { stride_length_cm: 90, distance_scale: 0.9 }, false).stride_length_cm).toBe(70);
  });
});
