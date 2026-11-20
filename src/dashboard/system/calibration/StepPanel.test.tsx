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
  point: { speed: 3, stride_length_cm: 68, distance_scale: 3 / 2.82, runs: 1 },
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
    to_save: null,
    before: null,
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
    steps: s,
  };
}

const stepPoint = { speed: 3, stride_length_cm: 56.5, distance_scale: 0.9, runs: 2 };
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
  s.calibration.wizard_enabled = true;
  s.calibration.belt_cm = 280;
  s.calibration.belt_treadmill = true;
  current = s;
  useTreadmillStore.setState({ calibrationPoints: [saved], beltLength: 300 });
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

  it("a measured stride is saved by itself, once, and says so in plain words", async () => {
    current!.calibration.run = measuredRun(
      steps({ phase: "measured", steps: 22, kept: 21, result: stepResult, to_save: stepPoint, before: saved }),
    );
    const { rerender } = render(<GuidedCalibration />);
    expect(screen.getByText("Stride 45.0 cm at 104 steps a minute, saved for 3.0 km/h.")).toBeTruthy();
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([stepPoint]);
    expect(types().filter((t) => t === "calibration_steps_saved")).toHaveLength(1);
    rerender(<GuidedCalibration />);
    expect(types().filter((t) => t === "calibration_steps_saved")).toHaveLength(1);
  });

  it("a stride Rust already saw saved is not stored again (dashboard reopened)", () => {
    useTreadmillStore.setState({ calibrationPoints: [] });
    current!.calibration.run = measuredRun(
      steps({ phase: "measured", result: stepResult, to_save: stepPoint, before: saved, saved: true }),
    );
    render(<GuidedCalibration />);
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([]);
    expect(types()).not.toContain("calibration_steps_saved");
  });

  it("UNDO puts the point back as it was before the stride", async () => {
    useTreadmillStore.setState({ calibrationPoints: [stepPoint] });
    current!.calibration.run = measuredRun(
      steps({ phase: "measured", result: stepResult, to_save: stepPoint, before: saved, saved: true }),
    );
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("UNDO")));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([saved]);
    expect(types()).toContain("calibration_steps_undo");
  });

  it("UNDO on a speed that had no point removes the new one", async () => {
    const fresh = { ...belt.point, stride_length_cm: 45 };
    useTreadmillStore.setState({ calibrationPoints: [fresh] });
    current!.calibration.run = measuredRun(
      steps({ phase: "measured", result: { ...stepResult, averaged: null }, to_save: fresh, saved: true }),
    );
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("UNDO")));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([]);
  });

  it("an undone stride says what UNDO did", () => {
    current!.calibration.run = measuredRun(
      steps({ phase: "measured", result: stepResult, to_save: stepPoint, before: saved, saved: true, undone: true }),
    );
    render(<GuidedCalibration />);
    expect(screen.getByText("UNDO put back the stride at 3.0 km/h as it was.")).toBeTruthy();
    expect(screen.queryByText("UNDO")).toBeNull();
  });
});
