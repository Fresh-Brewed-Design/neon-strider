// Keeps the dashboard's settings in step with Rust: loads them once at startup
// and pushes every later change back. Rust saves them, and may auto-connect to
// the saved device when it receives them. Live state is not handled here; it
// comes from the app snapshot.

import { invoke } from "@tauri-apps/api/core";
import type { NeuralSensitivity } from "../ipc";
import { subscribeAppSnapshot } from "../state/useAppState";
import { DEFAULT_SETTINGS } from "../store/slices/settingsSlice";
import { useTreadmillStore } from "../store/treadmillStore";
import type { CalibrationPoint, Settings } from "../store/types";

/** Rust's `AppConfig`, as `get_rust_config` returns it and `sync_orchestrator_config` takes it. */
export interface RustConfig {
  last_device_id: string | null;
  calibration_points: CalibrationPoint[];
  /** Whose points `calibration_points` are; Rust refuses a push carrying another treadmill's. */
  calibration_treadmill?: string | null;
  min_speed: number;
  max_speed: number;
  max_incline: number;
  tilt_multiplier: number;
  incline_offset: number;
  focus_stability_enabled: boolean;
  body_mass_kg: number;
  neural_floor_pct: number;
  neural_sensitivity: NeuralSensitivity;
}

const SETTINGS_KEYS = Object.keys(DEFAULT_SETTINGS) as (keyof Settings)[];

/** The settings to show for a loaded config. Missing, empty or zero values keep `current`'s. */
export function settingsFromConfig(config: Partial<RustConfig>, current: Settings): Settings {
  return {
    lastDeviceId: config.last_device_id || current.lastDeviceId,
    calibrationPoints: config.calibration_points?.length ? config.calibration_points : current.calibrationPoints,
    minSpeed: config.min_speed ?? current.minSpeed,
    maxSpeed: config.max_speed ?? current.maxSpeed,
    maxIncline: config.max_incline ?? current.maxIncline,
    tiltMultiplier: config.tilt_multiplier ?? current.tiltMultiplier,
    inclineOffset: config.incline_offset ?? current.inclineOffset,
    focusStabilityEnabled: config.focus_stability_enabled ?? current.focusStabilityEnabled,
    bodyMassKg: config.body_mass_kg ?? current.bodyMassKg,
    neuralFloorPct: config.neural_floor_pct ?? current.neuralFloorPct,
    neuralSensitivity: config.neural_sensitivity ?? current.neuralSensitivity,
  };
}

/** The config to push for `s`; `treadmill` is the one its points were loaded for. */
export function configFromSettings(s: Settings, treadmill: string | null = null): RustConfig {
  return {
    last_device_id: s.lastDeviceId,
    calibration_points: s.calibrationPoints,
    calibration_treadmill: treadmill,
    min_speed: s.minSpeed,
    max_speed: s.maxSpeed,
    max_incline: s.maxIncline,
    tilt_multiplier: s.tiltMultiplier,
    incline_offset: s.inclineOffset,
    focus_stability_enabled: s.focusStabilityEnabled,
    body_mass_kg: s.bodyMassKg,
    neural_floor_pct: s.neuralFloorPct,
    neural_sensitivity: s.neuralSensitivity,
  };
}

let started = false;

export function startSettingsSync(): void {
  if (started) return;
  started = true;
  // The treadmill the store's points were loaded for (points are kept per treadmill).
  let pointsTreadmill: string | null = null;

  useTreadmillStore.subscribe((state, prev) => {
    // Loading from Rust is not a change to send back.
    if (!state.isHydrated || !prev.isHydrated) return;
    if (!SETTINGS_KEYS.some((k) => state[k] !== prev[k])) return;
    invoke("sync_orchestrator_config", { config: configFromSettings(state, pointsTreadmill) }).catch((e) =>
      console.error("[settings] sync to Rust failed", e),
    );
  });

  // Loads run one after another, so a later load is never overwritten by an earlier one.
  let loading = invoke<Partial<RustConfig>>("get_rust_config")
    .then((config) => {
      pointsTreadmill = config.calibration_treadmill ?? null;
      const store = useTreadmillStore.getState();
      store.hydrateSettings(settingsFromConfig(config, store));
    })
    .catch((e) => console.error("[settings] load from Rust failed", e));

  // Rust changes the points by itself (a run's save, UNDO, START FRESH, another
  // treadmill connecting): when it says so, load them, so the next push carries
  // them. The first snapshot reloads too, in case the treadmill switched after the
  // first load.
  let seenRev: number | null = null;
  subscribeAppSnapshot((snap) => {
    const rev = snap.calibration.points_rev;
    if (rev === seenRev) return;
    seenRev = rev;
    loading = loading
      .then(() => invoke<Partial<RustConfig>>("get_rust_config"))
      .then((config) => {
        if (!config.calibration_points) return;
        const owner = config.calibration_treadmill ?? null;
        const store = useTreadmillStore.getState();
        const same =
          owner === pointsTreadmill && JSON.stringify(config.calibration_points) === JSON.stringify(store.calibrationPoints);
        pointsTreadmill = owner;
        if (!same) store.updateSettings({ calibrationPoints: config.calibration_points });
      })
      .catch((e) => console.error("[settings] reload of calibration points failed", e));
  });
}
