import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(sel: (s: AppSnapshot) => T) => (current ? sel(current) : null),
}));

const { snapshot: mock } = await import("../../../dev/mockBackend");
const { useTreadmillStore } = await import("../../../store/treadmillStore");
const { GuidedCalibration } = await import("./GuidedCalibration");

const dispatched = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, args]) => (args as { command: { type: string } }).command);

const saved = { speed: 3, stride_length_cm: 68, distance_scale: 0.9 };

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
  current.calibration.wizard_enabled = true;
  useTreadmillStore.setState({ calibrationPoints: [saved], beltLength: 300 });
});

describe("GuidedCalibration", () => {
  it("lists every target speed, repeats a saved one and runs a new one", async () => {
    render(<GuidedCalibration />);
    expect(screen.getAllByText("RUN")).toHaveLength(4);
    expect(screen.getAllByText("REPEAT")).toHaveLength(1);
    await act(async () => fireEvent.click(screen.getAllByText("RUN")[1]));
    expect(dispatched()).toContainEqual({ type: "calibration_run_start", target_kmh: 4 });
  });

  it("a repeated point's result replaces that point", async () => {
    const { rerender } = render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("REPEAT")));
    const point = { speed: 3, stride_length_cm: 72.5, distance_scale: 0.95 };
    const next = structuredClone(mock);
    next.calibration.wizard_enabled = true;
    next.calibration.run = {
      target_kmh: 3,
      phase: "counted",
      elapsed_s: 60,
      reported_kmh: 3,
      result: { point, real_kmh: 3.16, real_m: 52.6, reported_m: 50, cadence_spm: 72, stride_measured: true },
    };
    current = next;
    rerender(<GuidedCalibration />);
    expect(screen.getByText("72.5 cm")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("REPLACE POINT")));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([point]);
    expect(dispatched()).toContainEqual({ type: "calibration_run_close" });
  });

  it("shows a counting run from the snapshot without pressing RUN (dashboard reopened)", () => {
    current = structuredClone(mock);
    current.calibration.wizard_enabled = true;
    current.calibration.run = { target_kmh: 4, phase: "counting", elapsed_s: 23, reported_kmh: null, result: null };
    render(<GuidedCalibration />);
    expect(screen.getByText("COUNT_WINDOW")).toBeTruthy();
    expect(screen.getByText("0:23.0")).toBeTruthy();
    expect(screen.getByText("DONE")).toBeTruthy();
  });

  it("RUN ALL sends every row's speed, slowest first", async () => {
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("RUN ALL")));
    expect(dispatched()).toContainEqual({ type: "calibration_sweep_start", targets_kmh: [2, 3, 4, 5, 6] });
  });

  it("a sweep between speeds offers RUN on the next one and locks the row buttons", async () => {
    current = structuredClone(mock);
    current.calibration.wizard_enabled = true;
    current.calibration.sweep = { targets_kmh: [2, 3, 4], marks: ["saved", "skipped", "pending"], current: 2 };
    render(<GuidedCalibration />);
    expect(screen.getByText("SWEEP 3/3")).toBeTruthy();
    expect(screen.queryByText("RUN ALL")).toBeNull();
    for (const b of screen.getAllByText("RUN")) expect((b as HTMLButtonElement).disabled).toBe(true);
    await act(async () => fireEvent.click(screen.getByText("RUN 4.0 km/h")));
    expect(dispatched()).toContainEqual({ type: "calibration_sweep_run" });
    // STOP is there too, and nothing in the sweep starts the belt by itself.
    await act(async () => fireEvent.click(screen.getByText("STOP")));
    expect(dispatched()).toContainEqual({ type: "stop" });
  });

  it("in a sweep, saving marks the speed saved and the run panel has a STOP", async () => {
    current = structuredClone(mock);
    current.calibration.wizard_enabled = true;
    current.calibration.sweep = { targets_kmh: [3, 4], marks: ["pending", "pending"], current: 0 };
    const point = { speed: 3, stride_length_cm: 70, distance_scale: 0.9 };
    current.calibration.run = {
      target_kmh: 3,
      phase: "counted",
      elapsed_s: 60,
      reported_kmh: 3,
      result: { point, real_kmh: 3.3, real_m: 55, reported_m: 50, cadence_spm: null, stride_measured: false },
    };
    render(<GuidedCalibration />);
    expect(screen.getByText("STOP")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("REPLACE POINT")));
    expect(dispatched()).toContainEqual({ type: "calibration_sweep_next", saved: true });
    expect(dispatched()).not.toContainEqual({ type: "calibration_run_close" });
  });

  it("STOP in the run panel is never disabled", () => {
    current = structuredClone(mock);
    current.calibration.wizard_enabled = true;
    current.calibration.run = { target_kmh: 4, phase: "counting", elapsed_s: 5, reported_kmh: null, result: null };
    render(<GuidedCalibration />);
    expect((screen.getByText("STOP") as HTMLButtonElement).disabled).toBe(false);
  });

  it("shows a timed run waiting for its tally under the repeated point's row", () => {
    current = structuredClone(mock);
    current.calibration.wizard_enabled = true;
    current.calibration.run = { target_kmh: 3, phase: "counted", elapsed_s: 62.4, reported_kmh: 3, result: null };
    render(<GuidedCalibration />);
    expect(screen.getByText("TALLY")).toBeTruthy();
    expect(screen.getByText("BELT_LOOPS")).toBeTruthy();
    expect(screen.getByText("REPLACE POINT")).toBeTruthy();
  });
});
