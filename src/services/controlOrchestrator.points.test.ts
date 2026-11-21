import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";
import { useTreadmillStore } from "../store/treadmillStore";
import { startSettingsSync, type RustConfig } from "./controlOrchestrator";

// Hand the snapshot callback to the test, so it can say the points changed.
const listeners: ((s: AppSnapshot) => void)[] = [];
vi.mock("../state/useAppState", () => ({
  subscribeAppSnapshot: (cb: (s: AppSnapshot) => void) => {
    listeners.push(cb);
    return () => undefined;
  },
}));

const snap = (rev: number) => ({ calibration: { points_rev: rev } }) as unknown as AppSnapshot;
const point = (speed: number) => ({ speed, stride_length_cm: 70, distance_scale: 1.1 });

function config(treadmill: string | null, speeds: number[]): Partial<RustConfig> {
  return { calibration_treadmill: treadmill, calibration_points: speeds.map(point), max_speed: 10 };
}

describe("calibration points per treadmill", () => {
  it("reloads the points when another treadmill connects, and pushes them as that treadmill's", async () => {
    const calls = vi.mocked(invoke);
    let rust = config("AAAA", [3]);
    calls.mockImplementation(async (cmd) => (cmd === "get_rust_config" ? rust : undefined));
    startSettingsSync();
    await vi.waitFor(() => expect(useTreadmillStore.getState().isHydrated).toBe(true));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([point(3)]);

    // A second treadmill connects: Rust switched the points and bumped points_rev.
    rust = config("BBBB", []);
    listeners.forEach((l) => l(snap(1)));
    await vi.waitFor(() => expect(useTreadmillStore.getState().calibrationPoints).toEqual([]));

    useTreadmillStore.getState().updateSettings({ bodyMassKg: 90 });
    expect(calls).toHaveBeenLastCalledWith("sync_orchestrator_config", {
      config: expect.objectContaining({ calibration_treadmill: "BBBB", calibration_points: [] }),
    });
  });

  it("leaves the store alone when a reload brings the same points", async () => {
    const calls = vi.mocked(invoke);
    calls.mockClear();
    calls.mockImplementation(async (cmd) => (cmd === "get_rust_config" ? config("BBBB", []) : undefined));
    listeners.forEach((l) => l(snap(2)));
    await vi.waitFor(() => expect(calls).toHaveBeenCalledWith("get_rust_config"));
    await Promise.resolve();
    expect(calls.mock.calls.some(([cmd]) => cmd === "sync_orchestrator_config")).toBe(false);
  });
});
