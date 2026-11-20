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

const saved = { speed: 3, stride_length_cm: 68, distance_scale: 0.9, runs: 1 };

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
    expect(screen.getAllByText("ADD RUN")).toHaveLength(1);
    await act(async () => fireEvent.click(screen.getAllByText("RUN")[1]));
    expect(dispatched()).toContainEqual({ type: "calibration_run_start", target_kmh: 4 });
  });

  it("a repeated point saves Rust's average of the runs", async () => {
    const { rerender } = render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("ADD RUN")));
    const point = { speed: 3, stride_length_cm: 72, distance_scale: 1.0, runs: 1 };
    const averaged = { speed: 3, stride_length_cm: 70, distance_scale: 0.95, runs: 2 };
    const next = structuredClone(mock);
    next.calibration.wizard_enabled = true;
    next.calibration.run = {
      target_kmh: 3,
      phase: "counted",
      elapsed_s: 60,
      reported_kmh: 3,
      result: { point, real_kmh: 3.16, real_m: 52.6, reported_m: 50, cadence_spm: 72, stride_measured: true, averaged },
    };
    current = next;
    rerender(<GuidedCalibration />);
    expect(screen.getByText("72.0 cm")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("AVERAGE IN")));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([averaged]);
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

  it("RUN ALL leaves the speeds to Rust's stored list", async () => {
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("RUN ALL")));
    expect(dispatched()).toContainEqual({ type: "calibration_sweep_start" });
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
    const point = { speed: 3, stride_length_cm: 70, distance_scale: 0.9, runs: 1 };
    current.calibration.run = {
      target_kmh: 3,
      phase: "counted",
      elapsed_s: 60,
      reported_kmh: 3,
      result: { point, real_kmh: 3.3, real_m: 55, reported_m: 50, cadence_spm: null, stride_measured: false, averaged: null },
    };
    render(<GuidedCalibration />);
    expect(screen.getByText("STOP")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("AVERAGE IN")));
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
    expect(screen.getByText("AVERAGE IN")).toBeTruthy();
  });

  it("ADD sends the speed to Rust and shows the list Rust keeps", async () => {
    const { rerender } = render(<GuidedCalibration />);
    fireEvent.change(screen.getByLabelText("ADD_SPEED"), { target: { value: "3.5" } });
    await act(async () => fireEvent.click(screen.getByText("ADD")));
    expect(dispatched()).toContainEqual({ type: "calibration_target_add", kmh: 3.5 });
    expect(screen.queryByText("3.5 km/h")).toBeNull();
    current = structuredClone(current!);
    current.calibration.targets.push({ kmh: 3.5, runnable: true });
    rerender(<GuidedCalibration />);
    expect(screen.getByText("3.5 km/h")).toBeTruthy();
  });

  it("shows Rust's refusal under ADD_SPEED", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("4.0 km/h is already in the list.");
    render(<GuidedCalibration />);
    fireEvent.change(screen.getByLabelText("ADD_SPEED"), { target: { value: "4" } });
    await act(async () => fireEvent.click(screen.getByText("ADD")));
    expect(screen.getByText("4.0 km/h is already in the list.")).toBeTruthy();
    expect((screen.getByLabelText("ADD_SPEED") as HTMLInputElement).value).toBe("4");
  });

  it("removes an uncalibrated speed through Rust, never a saved point's or the open run's", async () => {
    current!.calibration.run = { target_kmh: 5, phase: "settling", elapsed_s: 2, reported_kmh: null, result: null };
    render(<GuidedCalibration />);
    expect(screen.queryByLabelText("Remove 3.0 km/h from the target list")).toBeNull();
    expect(screen.queryByLabelText("Remove 5.0 km/h from the target list")).toBeNull();
    await act(async () => fireEvent.click(screen.getByLabelText("Remove 4.0 km/h from the target list")));
    expect(dispatched()).toContainEqual({ type: "calibration_target_remove", kmh: 4 });
  });

  it("a stored speed outside the treadmill's range is listed but can't be run", () => {
    current!.calibration.targets.push({ kmh: 14, runnable: false });
    render(<GuidedCalibration />);
    expect(screen.getByText("14.0 km/h")).toBeTruthy();
    expect(screen.getByText(/14.0 km\/h is outside the treadmill's speed range/)).toBeTruthy();
    const runs = screen.getAllByText("RUN") as HTMLButtonElement[];
    expect(runs[runs.length - 1].disabled).toBe(true);
    expect(screen.getByLabelText("Remove 14.0 km/h from the target list")).toBeTruthy();
  });

  it("counts steps alone when there is no mark on the belt", async () => {
    current = structuredClone(mock);
    current.calibration.wizard_enabled = true;
    current.calibration.run = { target_kmh: 3, phase: "counted", elapsed_s: 62.4, reported_kmh: 3, result: null };
    render(<GuidedCalibration />);
    fireEvent.click(screen.getByLabelText(/NO_MARK/));
    expect(screen.queryByText("BELT_LOOPS")).toBeNull();
    const compute = screen.getByText("COMPUTE") as HTMLButtonElement;
    expect(compute.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("STEP_COUNT"), { target: { value: "120" } });
    await act(async () => fireEvent.click(compute));
    expect(dispatched()).toContainEqual({
      type: "calibration_run_count",
      count: { method: "steps_only", steps: 120 },
    });
  });
});
