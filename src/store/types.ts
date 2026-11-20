// The store holds settings edited in the dashboard and a little UI state.
// Live machine state (speed, status, session totals...) comes from the Rust
// snapshot; see src/state/useAppState.ts.

import type { NeuralSensitivity } from "../ipc";

export interface CalibrationPoint {
  speed: number;
  stride_length_cm: number;
  distance_scale: number;
  /** Timed runs this point averages; absent on points saved before runs were counted (one). */
  runs?: number;
  /** Footstep runs (GAIT_SYNC) the stride averages; absent or 0 when the stride was never measured. */
  stride_runs?: number;
}

/** What Rust keeps in its config file. Every change is pushed back to Rust as a whole. */
export interface Settings {
  lastDeviceId: string | null;
  calibrationPoints: CalibrationPoint[];
  /** Belt circumference in cm, for the logo-count calibration. */
  beltLength: number;
  minSpeed: number;
  maxSpeed: number;
  maxIncline: number;
  tiltMultiplier: number;
  inclineOffset: number;
  /** Neural flow on or off. */
  focusStabilityEnabled: boolean;
  /** Body weight in kg, used for the calorie estimate. */
  bodyMassKg: number;
  /** Neural flow's "Slowest speed": % of the base speed, 50 to 95. */
  neuralFloorPct: number;
  neuralSensitivity: NeuralSensitivity;
}

export interface SettingsSlice extends Settings {
  /** False until the settings have been loaded from Rust; nothing is pushed back before that. */
  isHydrated: boolean;

  hydrateSettings: (settings: Settings) => void;
  updateSettings: (patch: Partial<Settings>) => void;
  /** Adds a point, replacing any existing one at (nearly) the same speed. */
  addCalibrationPoint: (point: CalibrationPoint) => void;
  updateCalibrationPoint: (index: number, point: CalibrationPoint) => void;
  removeCalibrationPoint: (index: number) => void;
}

export interface HistorySlice {
  /** Bumped whenever the History list should reload. */
  historyRefreshTrigger: number;
  sessionMergeThresholdMinutes: number;

  triggerHistoryRefresh: () => void;
  setSessionMergeThreshold: (minutes: number) => void;
}

export interface ConfirmState {
  title: string;
  message: string;
  /** Optional detail line under the message, in small type. */
  hint?: string;
  onConfirm: () => void;
  onCancel?: () => void;
}

export interface UISlice {
  pendingConfirmation: ConfirmState | null;

  /** Opens the shared confirm dialog. */
  confirm: (config: ConfirmState) => void;
  clearConfirmation: () => void;
}

export type TreadmillStore = SettingsSlice & HistorySlice & UISlice;
