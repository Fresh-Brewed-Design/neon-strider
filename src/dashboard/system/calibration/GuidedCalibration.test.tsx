import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot, CalibrationResult, CalibrationRunView } from "../../../ipc";

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

const taps = () => dispatched().filter((c) => c.type === "calibration_tap");

const saved = { speed: 3, stride_length_cm: 68, distance_scale: 0.9, runs: 1 };

/** The mocked snapshot with the switch on and a 280 cm belt for this treadmill. */
function ready(): AppSnapshot {
  const s = structuredClone(mock);
  s.calibration.wizard_enabled = true;
  s.calibration.belt_cm = 280;
  s.calibration.belt_treadmill = true;
  return s;
}

function runView(view: Partial<CalibrationRunView>): CalibrationRunView {
  return {
    target_kmh: 3,
    phase: "settling",
    passes: 0,
    kept: 0,
    dropped: 0,
    elapsed_s: 0,
    measured_kmh: null,
    reported_kmh: null,
    note: null,
    result: null,
    steps: null,
    ...view,
  };
}

function result(r: Partial<CalibrationResult> = {}): CalibrationResult {
  return {
    point: { speed: 3, stride_length_cm: 68, distance_scale: 3 / 2.82, runs: 1 },
    real_kmh: 2.82,
    reported_kmh: 3,
    real_m: 25,
    reported_m: 26.6,
    cadence_spm: null,
    stride_measured: false,
    averaged: null,
    ...r,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = ready();
  useTreadmillStore.setState({ calibrationPoints: [saved], beltLength: 300 });
});

describe("GuidedCalibration", () => {
  it("lists every target speed, repeats a saved one and runs a new one", async () => {
    render(<GuidedCalibration />);
    expect(screen.getAllByText("RUN")).toHaveLength(4);
    expect(screen.getAllByText("ADD RUN")).toHaveLength(1);
    expect(screen.getByText("280 cm")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getAllByText("RUN")[1]));
    expect(dispatched()).toContainEqual({ type: "calibration_run_start", target_kmh: 4 });
  });

  it("asks the belt loop first, prefilled from the old setting, and keeps RUN off until then", async () => {
    current!.calibration.belt_cm = null;
    render(<GuidedCalibration />);
    expect(screen.getByText("LOOP_LENGTH")).toBeTruthy();
    expect(screen.queryByText("RUN ALL")).toBeNull();
    for (const b of screen.getAllByText("RUN")) expect((b as HTMLButtonElement).disabled).toBe(true);
    const input = screen.getByLabelText("Belt loop length in cm") as HTMLInputElement;
    expect(input.value).toBe("300");
    fireEvent.change(input, { target: { value: "276" } });
    await act(async () => fireEvent.click(screen.getByText("SAVE")));
    expect(dispatched()).toContainEqual({ type: "calibration_belt_set", cm: 276 });
  });

  it("says a treadmill must be known before a belt length can be kept", () => {
    current!.calibration.belt_cm = null;
    current!.calibration.belt_treadmill = false;
    render(<GuidedCalibration />);
    expect(screen.getByText(/Connect the treadmill first/)).toBeTruthy();
  });

  it("TAP, and Space in the dashboard, each send one tap; Rust times them", async () => {
    current!.calibration.run = runView({ phase: "tapping", passes: 4, kept: 3, measured_kmh: 2.8, reported_kmh: 3 });
    render(<GuidedCalibration />);
    expect(screen.getByText("TAPPING")).toBeTruthy();
    expect(screen.getByText("3 of 8 agreeing times between passes.")).toBeTruthy();
    expect(screen.getByText("The deck shows 3.0 km/h; the belt measures 2.80 km/h.")).toBeTruthy();
    await act(async () => fireEvent.pointerDown(screen.getByText("TAP"), { button: 0 }));
    await act(async () => fireEvent.keyDown(window, { key: " " }));
    // A held key repeats, and Alt+Space is the input hook's, not the window's.
    await act(async () => fireEvent.keyDown(window, { key: " ", repeat: true }));
    await act(async () => fireEvent.keyDown(window, { key: " ", altKey: true }));
    expect(taps()).toHaveLength(2);
  });

  it("Space typed into a field is not a tap", async () => {
    current!.calibration.run = runView({ phase: "tapping", passes: 2, kept: 1 });
    render(<GuidedCalibration />);
    await act(async () => fireEvent.keyDown(screen.getByLabelText("ADD_SPEED"), { key: " " }));
    expect(taps()).toHaveLength(0);
  });

  it("names dropped taps and Rust's restart note", () => {
    current!.calibration.run = runView({
      phase: "tapping",
      passes: 7,
      kept: 5,
      dropped: 1,
      note: "The speed changed (3.0 to 3.5 km/h), so tapping starts again.",
    });
    render(<GuidedCalibration />);
    expect(screen.getByText("1 tap dropped as a miss or a double.")).toBeTruthy();
    expect(screen.getByText(/so tapping starts again/)).toBeTruthy();
  });

  it("a measured run says the result in plain words and saves the point", async () => {
    useTreadmillStore.setState({ calibrationPoints: [] });
    current!.calibration.run = runView({ phase: "measured", passes: 9, kept: 8, result: result() });
    render(<GuidedCalibration />);
    expect(screen.getByText("At 3.0 km/h the deck runs 6.0 % slow.")).toBeTruthy();
    expect(screen.queryByText("TAP")).toBeNull();
    await act(async () => fireEvent.keyDown(window, { key: " " }));
    expect(taps()).toHaveLength(0);
    await act(async () => fireEvent.click(screen.getByText("SAVE POINT")));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([result().point]);
    expect(dispatched()).toContainEqual({ type: "calibration_run_close" });
  });

  it("a repeated point saves Rust's average of the runs", async () => {
    const averaged = { speed: 3, stride_length_cm: 68, distance_scale: 0.98, runs: 2 };
    current!.calibration.run = runView({ phase: "measured", passes: 9, kept: 8, result: result({ averaged }) });
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("AVERAGE IN")));
    expect(useTreadmillStore.getState().calibrationPoints).toEqual([averaged]);
  });

  it("TAP AGAIN asks Rust to clear the taps", async () => {
    current!.calibration.run = runView({ phase: "measured", passes: 9, kept: 8, result: result() });
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("TAP AGAIN")));
    expect(dispatched()).toContainEqual({ type: "calibration_tap_reset" });
  });

  it("shows a tapping run from the snapshot without pressing RUN (dashboard reopened)", () => {
    current!.calibration.run = runView({ target_kmh: 4, phase: "tapping", passes: 3, kept: 2, elapsed_s: 23 });
    render(<GuidedCalibration />);
    expect(screen.getByText("TAPPING")).toBeTruthy();
    expect(screen.getByText("0:23.0")).toBeTruthy();
    expect(screen.getByText("TAP")).toBeTruthy();
  });

  it("RUN ALL leaves the speeds to Rust's stored list", async () => {
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("RUN ALL")));
    expect(dispatched()).toContainEqual({ type: "calibration_sweep_start" });
  });

  it("a sweep between speeds offers RUN on the next one and locks the row buttons", async () => {
    current!.calibration.sweep = { targets_kmh: [2, 3, 4], marks: ["saved", "skipped", "pending"], current: 2 };
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
    current!.calibration.sweep = { targets_kmh: [3, 4], marks: ["pending", "pending"], current: 0 };
    current!.calibration.run = runView({ phase: "measured", passes: 9, kept: 8, result: result() });
    render(<GuidedCalibration />);
    expect(screen.getByText("STOP")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("AVERAGE IN")));
    expect(dispatched()).toContainEqual({ type: "calibration_sweep_next", saved: true });
    expect(dispatched()).not.toContainEqual({ type: "calibration_run_close" });
  });

  it("STOP in the run panel is never disabled", () => {
    current!.calibration.run = runView({ target_kmh: 4, phase: "tapping", passes: 2, kept: 1 });
    render(<GuidedCalibration />);
    expect((screen.getByText("STOP") as HTMLButtonElement).disabled).toBe(false);
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
    current!.calibration.run = runView({ target_kmh: 5 });
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
});
