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

  it("shows a timed run waiting for its tally under the repeated point's row", () => {
    current = structuredClone(mock);
    current.calibration.wizard_enabled = true;
    current.calibration.run = { target_kmh: 3, phase: "counted", elapsed_s: 62.4, reported_kmh: 3, result: null };
    render(<GuidedCalibration />);
    expect(screen.getByText("TALLY")).toBeTruthy();
    expect(screen.getByText("BELT_LOOPS")).toBeTruthy();
    expect(screen.getByText("REPLACE POINT")).toBeTruthy();
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
