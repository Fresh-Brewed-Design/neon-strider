import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot, CalibrationResult, CalibrationRunView, StepRunView } from "../../../ipc";

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
const types = () => dispatched().map((c) => c.type);

const saved = { speed: 3, stride_length_cm: 68, distance_scale: 0.9, runs: 1 };
const belt: CalibrationResult = {
  point: { speed: 3, stride_length_cm: 68, distance_scale: 3 / 2.82, runs: 1, stride_runs: 0 },
  real_kmh: 2.82,
  reported_kmh: 3,
  real_m: 25,
  reported_m: 26.6,
  cadence_spm: null,
  stride_measured: false,
  averaged: null,
};

function steps(view: Partial<StepRunView> = {}): StepRunView {
  return {
    phase: "settling",
    steps: 0,
    kept: 0,
    dropped: 0,
    elapsed_s: 0,
    cadence_spm: null,
    stride_cm: null,
    note: null,
    result: null,
    saved: false,
    undone: false,
    ...view,
  };
}

/** A measured belt run at 3.0 km/h, with the footstep phase `s`. */
function measuredRun(s: StepRunView | null): CalibrationRunView {
  return {
    target_kmh: 3,
    phase: "measured",
    passes: 9,
    kept: 8,
    dropped: 0,
    elapsed_s: 30,
    measured_kmh: 2.82,
    reported_kmh: 3,
    note: null,
    result: belt,
    saved: true,
    undone: false,
    steps: s,
  };
}

const stepPoint = { speed: 3, stride_length_cm: 56.5, distance_scale: 0.9, runs: 1, stride_runs: 1 };
const stepResult: CalibrationResult = {
  ...belt,
  point: { ...belt.point, stride_length_cm: 45 },
  cadence_spm: 104,
  stride_measured: true,
  averaged: stepPoint,
};

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  const s = structuredClone(mock);
  s.calibration.belt_cm = 280;
  s.calibration.belt_treadmill = true;
  current = s;
  useTreadmillStore.setState({ calibrationPoints: [saved] });
});

describe("GAIT_SYNC", () => {
  it("a measured belt offers the footstep phase, and TAP STEPS starts it", async () => {
    current!.calibration.run = measuredRun(null);
    render(<GuidedCalibration />);
    expect(screen.getByText("Tap each footstep, left and right, to measure your stride at 3.0 km/h.")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("TAP STEPS")));
    expect(types()).toContain("calibration_steps_start");
  });

  it("TAP and Space each tap a footstep, and the window stays pinned", async () => {
    current!.calibration.run = measuredRun(steps({ phase: "tapping", steps: 6, kept: 5, cadence_spm: 104, stride_cm: 45.2 }));
    render(<GuidedCalibration />);
    expect(types()).toContain("guided_task_start");
    expect(screen.getByText("5 of 20 agreeing times between footsteps.")).toBeTruthy();
    expect(screen.getByText("45.2 cm")).toBeTruthy();
    await act(async () => fireEvent.pointerDown(screen.getByText("TAP"), { button: 0 }));
    await act(async () => fireEvent.keyDown(window, { key: " " }));
    await act(async () => fireEvent.keyDown(window, { key: " ", altKey: true }));
    expect(types().filter((t) => t === "calibration_tap")).toHaveLength(2);
    expect(types()).not.toContain("guided_task_end");
  });

  it("SKIP leaves the stride to Rust as it was", async () => {
    current!.calibration.run = measuredRun(steps({ phase: "tapping", steps: 3, kept: 2 }));
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("SKIP")));
    expect(types()).toContain("calibration_steps_skip");
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([saved]);
  });

  it("a measured stride says it is saved, and UNDO asks Rust to put the point back", async () => {
    current!.calibration.run = measuredRun(steps({ phase: "measured", steps: 22, kept: 21, result: stepResult, saved: true }));
    render(<GuidedCalibration />);
    expect(screen.getByText("Stride 45.0 cm at 104 steps a minute for 3.0 km/h; saved.")).toBeTruthy();
    const undos = screen.getAllByText("UNDO");
    expect(undos).toHaveLength(2);
    await act(async () => fireEvent.click(undos[1]));
    expect(types()).toContain("calibration_steps_undo");
    expect(types()).not.toContain("calibration_undo");
    // Rust owns the save: the window never writes the points itself.
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([saved]);
  });

  it("TAP AGAIN on the steps starts them over through Rust", async () => {
    current!.calibration.run = measuredRun(steps({ phase: "measured", result: stepResult, saved: true }));
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getAllByText("TAP AGAIN")[1]));
    expect(types()).toContain("calibration_steps_start");
    expect(types()).not.toContain("calibration_tap_reset");
  });

  it("an undone stride says what UNDO did", () => {
    current!.calibration.run = measuredRun(steps({ phase: "measured", result: stepResult, undone: true }));
    render(<GuidedCalibration />);
    expect(screen.getByText("UNDO restored the 3.0 km/h point to the belt run's result.")).toBeTruthy();
    expect(screen.getAllByText("UNDO")).toHaveLength(1);
  });

  it("no footsteps are offered once UNDO took the belt's point back", () => {
    current!.calibration.run = { ...measuredRun(null), saved: false, undone: true };
    render(<GuidedCalibration />);
    expect(screen.queryByText("TAP STEPS")).toBeNull();
  });
});
