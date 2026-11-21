import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_APP_CONFIG } from "../ipc";
import { DEFAULT_SETTINGS } from "../store/slices/settingsSlice";
import { useTreadmillStore } from "../store/treadmillStore";
import { configFromSettings, settingsFromConfig, startSettingsSync, type RustConfig } from "./controlOrchestrator";

const CONFIG: RustConfig = {
  last_device_id: "deck-1",
  calibration_points: [{ speed: 3, stride_length_cm: 70, distance_scale: 1.1 }],
  belt_length: 120,
  min_speed: 0.8,
  max_speed: 10,
  max_incline: 12,
  tilt_multiplier: 1.25,
  incline_offset: -1,
  focus_stability_enabled: true,
  body_mass_kg: 82,
  neural_floor_pct: 70,
  neural_sensitivity: "high",
};

describe("DEFAULT_SETTINGS", () => {
  it("is Rust's AppConfig::default(), field for field", () => {
    expect(configFromSettings(DEFAULT_SETTINGS)).toEqual(DEFAULT_APP_CONFIG);
    expect(DEFAULT_SETTINGS.beltLength).toBe(120);
  });
});

describe("settingsFromConfig", () => {
  it("round-trips every field", () => {
    expect(configFromSettings(settingsFromConfig(CONFIG, DEFAULT_SETTINGS))).toEqual(CONFIG);
  });

  it("keeps the current values for missing, empty or zero fields", () => {
    const s = settingsFromConfig(
      { last_device_id: null, calibration_points: [], belt_length: 0, max_speed: 9 },
      DEFAULT_SETTINGS,
    );
    expect(s).toEqual({ ...DEFAULT_SETTINGS, maxSpeed: 9 });
  });
});

describe("startSettingsSync", () => {
  it("loads from Rust without pushing back, then pushes later changes", async () => {
    const calls = vi.mocked(invoke);
    calls.mockImplementation(async (cmd) => (cmd === "get_rust_config" ? CONFIG : undefined));
    startSettingsSync();
    await vi.waitFor(() => expect(useTreadmillStore.getState().isHydrated).toBe(true));
    expect(useTreadmillStore.getState().maxSpeed).toBe(10);
    expect(calls.mock.calls.some(([cmd]) => cmd === "sync_orchestrator_config")).toBe(false);

    useTreadmillStore.getState().updateSettings({ bodyMassKg: 90 });
    expect(calls).toHaveBeenLastCalledWith("sync_orchestrator_config", {
      config: { ...CONFIG, body_mass_kg: 90 },
    });
  });
});
