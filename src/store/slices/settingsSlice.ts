import type { StateCreator } from "zustand";
import type { CalibrationPoint, Settings, SettingsSlice, TreadmillStore } from "../types";

/** Same default and range as src-tauri/src/neural.rs. */
const NEURAL_FLOOR_DEFAULT = 90;
export const NEURAL_FLOOR_MIN = 50;
export const NEURAL_FLOOR_MAX = 95;

/** Shown until Rust's saved values arrive (see services/controlOrchestrator.ts). */
export const DEFAULT_SETTINGS: Settings = {
  lastDeviceId: null,
  calibrationPoints: [],
  beltLength: 280,
  minSpeed: 0.6,
  maxSpeed: 12,
  maxIncline: 15,
  tiltMultiplier: 1.0,
  inclineOffset: 0.0,
  focusStabilityEnabled: false,
  bodyMassKg: 75,
  neuralFloorPct: NEURAL_FLOOR_DEFAULT,
  neuralSensitivity: "medium",
};

const bySpeed = (points: CalibrationPoint[]) => [...points].sort((a, b) => a.speed - b.speed);

/** Calibration points stay sorted by speed; the neural floor stays a whole number in range. */
function normalize(patch: Partial<Settings>): Partial<Settings> {
  const out = { ...patch };
  if (out.calibrationPoints) out.calibrationPoints = bySpeed(out.calibrationPoints);
  if (out.neuralFloorPct !== undefined) {
    out.neuralFloorPct = Math.min(NEURAL_FLOOR_MAX, Math.max(NEURAL_FLOOR_MIN, Math.round(out.neuralFloorPct)));
  }
  return out;
}

/** A point at speed 0 or with a scale of 0 would corrupt every distance, so it is never stored. */
const validPoint = (p: { speed: number; distance_scale: number }) => p.speed > 0 && p.distance_scale > 0;

export const createSettingsSlice: StateCreator<TreadmillStore, [], [], SettingsSlice> = (set) => ({
  ...DEFAULT_SETTINGS,
  isHydrated: false,

  hydrateSettings: (settings) => set({ ...normalize(settings), isHydrated: true }),
  updateSettings: (patch) => set(normalize(patch)),

  addCalibrationPoint: (point) =>
    set((s) =>
      validPoint(point)
        ? {
            calibrationPoints: bySpeed([
              ...s.calibrationPoints.filter((p) => Math.abs(p.speed - point.speed) > 0.05),
              point,
            ]),
          }
        : s,
    ),
  updateCalibrationPoint: (index, point) =>
    set((s) =>
      validPoint(point)
        ? { calibrationPoints: bySpeed(s.calibrationPoints.map((p, i) => (i === index ? point : p))) }
        : s,
    ),
  removeCalibrationPoint: (index) =>
    set((s) => ({ calibrationPoints: s.calibrationPoints.filter((_, i) => i !== index) })),
});
