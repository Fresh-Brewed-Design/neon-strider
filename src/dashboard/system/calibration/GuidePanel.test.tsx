import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot, CalibrationGuideView, CalibrationRunView, GuideCheck } from "../../../ipc";

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
/** Commands that set or start the belt. */
const BELT = ["calibration_run_start", "calibration_sweep_start", "calibration_sweep_run", "calibration_guide_run", "start"];
const beltMoves = () => types().filter((t) => BELT.includes(t));

function check(c: Partial<GuideCheck>): GuideCheck {
  return { lo_kmh: 0.6, hi_kmh: 5, mid_kmh: 2.8, verdict: "pending", error_pct: null, ...c };
}

function guide(g: Partial<CalibrationGuideView> = {}): CalibrationGuideView {
  return {
    slow_kmh: 0.6,
    top_kmh: 5,
    note: null,
    speeds: [
      { kmh: 0.6, mark: "pending", end: true },
      { kmh: 5, mark: "pending", end: true },
    ],
    checks: [],
    next_kmh: 0.6,
    remaining_error_pct: null,
    tolerance_pct: 2,
    ...g,
  };
}

function tapping(target_kmh: number): CalibrationRunView {
  return {
    target_kmh,
    phase: "tapping",
    passes: 3,
    kept: 2,
    dropped: 0,
    elapsed_s: 7,
    measured_kmh: 4.8,
    reported_kmh: 5,
    note: null,
    result: null,
    saved: false,
    undone: false,
    steps: null,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  const s = structuredClone(mock);
  s.calibration.belt_cm = 280;
  s.calibration.belt_treadmill = true;
  current = s;
  useTreadmillStore.setState({ calibrationPoints: [], beltLength: 300, maxSpeed: 6 });
});

describe("the guided walk", () => {
  it("asks for the top speed as the treadmill shows it, and START only plans", async () => {
    useTreadmillStore.setState({ minSpeed: 0.6, maxSpeed: 4 });
    render(<GuidedCalibration />);
    expect(screen.getByText("Fastest setting you'll walk at, as the treadmill shows it.")).toBeTruthy();
    expect(screen.queryByText(/km\/h on the deck/)).toBeNull();
    // The stepper starts at the deck's top and stops there.
    expect(screen.getByLabelText("Top speed setting").textContent).toBe("4.0");
    expect((screen.getByLabelText("Raise top speed setting") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByLabelText("Lower top speed setting"));
    fireEvent.click(screen.getByLabelText("Lower top speed setting"));
    await act(async () => fireEvent.click(screen.getByText("START")));
    expect(dispatched()).toContainEqual({ type: "calibration_guide_start", top_kmh: 3.8 });
    expect(beltMoves()).toEqual([]);
  });

  it("offers only the speeds DECK_PROFILE knows the deck runs, from its top", () => {
    current!.deck.speed_steps = [0.8, 1, 1.2, 1.3].map((hw_kmh) => ({ hw_kmh, kmh: hw_kmh }));
    render(<GuidedCalibration />);
    expect(screen.getByLabelText("Top speed setting").textContent).toBe("1.3");
    fireEvent.click(screen.getByLabelText("Lower top speed setting"));
    expect(screen.getByLabelText("Top speed setting").textContent).toBe("1.2");
    fireEvent.click(screen.getByLabelText("Lower top speed setting"));
    expect(screen.getByLabelText("Top speed setting").textContent).toBe("1.0");
    expect((screen.getByLabelText("Lower top speed setting") as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows the walk's note when the deck ran another speed", () => {
    current!.calibration.guide = guide({ note: "The deck ran 1.2, not 0.6, so 1.2 is the slowest speed." });
    render(<GuidedCalibration />);
    expect(screen.getByText("The deck ran 1.2, not 0.6, so 1.2 is the slowest speed.")).toBeTruthy();
  });

  it("shows Rust's refusal under START", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Pick a top speed above the slowest, 0.6 km/h.");
    render(<GuidedCalibration />);
    await act(async () => fireEvent.click(screen.getByText("START")));
    expect(screen.getByText("Pick a top speed above the slowest, 0.6 km/h.")).toBeTruthy();
  });

  it("waits for RUN on each speed, with SKIP and STOP beside it", async () => {
    current!.calibration.guide = guide();
    render(<GuidedCalibration />);
    expect(screen.queryByText("Fastest setting you'll walk at, as the treadmill shows it.")).toBeNull();
    expect(screen.getByText("RUN sets the belt to the next speed, 0.6 km/h.")).toBeTruthy();
    expect(screen.getByText("STOP")).toBeTruthy();
    expect(beltMoves()).toEqual([]);
    await act(async () => fireEvent.click(screen.getByText("RUN 0.6 km/h")));
    expect(beltMoves()).toEqual(["calibration_guide_run"]);
    await act(async () => fireEvent.click(screen.getByText("SKIP")));
    expect(types()).toContain("calibration_guide_skip");
  });

  it("says how the midpoint check measures the error", () => {
    current!.calibration.guide = guide({
      speeds: [
        { kmh: 0.6, mark: "saved", end: true },
        { kmh: 2.8, mark: "pending", end: false },
        { kmh: 5, mark: "saved", end: true },
      ],
      checks: [check({})],
      next_kmh: 2.8,
    });
    render(<GuidedCalibration />);
    expect(screen.getByText("A check run measures the distance error halfway between two calibrated speeds.")).toBeTruthy();
    expect(screen.getByText("0.6 to 5.0 km/h: the check at 2.8 km/h waits for RUN.")).toBeTruthy();
  });

  it("taps inside the walk, where Close skips the speed and the row buttons wait", async () => {
    current!.calibration.guide = guide({ next_kmh: 5 });
    current!.calibration.run = tapping(5);
    render(<GuidedCalibration />);
    expect(screen.queryByText("RUN 5.0 km/h")).toBeNull();
    expect(screen.getByText("TAP")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("SKIP")));
    expect(types()).toContain("calibration_guide_skip");
    expect(beltMoves()).toEqual([]);
  });

  it("names a split check and the one that agreed", () => {
    current!.calibration.guide = guide({
      checks: [
        check({ verdict: "split", error_pct: 3.16 }),
        check({ lo_kmh: 0.6, hi_kmh: 2.8, mid_kmh: 1.7, verdict: "agreed", error_pct: 0.54 }),
      ],
      next_kmh: 3.9,
    });
    render(<GuidedCalibration />);
    expect(
      screen.getByText("0.6 to 5.0 km/h: 3.2 % off at 2.8 km/h, so 2.8 km/h became a point and each half gets a check."),
    ).toBeTruthy();
    expect(screen.getByText("0.6 to 2.8 km/h: 0.5 % off at 1.7 km/h, within 2.0 %.")).toBeTruthy();
  });

  it("sums up the speeds calibrated and the error left", () => {
    current!.calibration.guide = guide({
      speeds: [0.6, 1.7, 2.8, 3.9, 5].map((kmh) => ({ kmh, mark: "saved" as const, end: kmh === 0.6 || kmh === 5 })),
      checks: [
        check({ verdict: "split", error_pct: 3.16 }),
        check({ lo_kmh: 0.6, hi_kmh: 2.8, mid_kmh: 1.7, verdict: "agreed", error_pct: 0.54 }),
        check({ lo_kmh: 2.8, hi_kmh: 5, mid_kmh: 3.9, verdict: "agreed", error_pct: 1.24 }),
      ],
      next_kmh: null,
      remaining_error_pct: 1.24,
    });
    render(<GuidedCalibration />);
    expect(screen.getByText("Calibrated 0.6, 1.7, 2.8, 3.9 and 5.0 km/h.")).toBeTruthy();
    expect(screen.getByText("Between the calibrated speeds, distance is within 1.2 % of the belt.")).toBeTruthy();
    expect(screen.getByText("Each check run measured the error halfway between two calibrated speeds.")).toBeTruthy();
    expect(screen.getByText("Close")).toBeTruthy();
    expect(screen.queryByText(/^RUN \d/)).toBeNull();
  });

  it("pins the dashboard through the walk, and again at each new speed", () => {
    current!.calibration.guide = guide({ next_kmh: 5 });
    const { rerender } = render(<GuidedCalibration />);
    expect(types().filter((t) => t === "guided_task_start")).toHaveLength(1);
    current = structuredClone(current!);
    current.calibration.run = tapping(5);
    rerender(<GuidedCalibration />);
    expect(types().filter((t) => t === "guided_task_start")).toHaveLength(2);
    expect(types()).not.toContain("guided_task_end");
  });

  it("keeps the target list and RUN ALL under ADVANCED", () => {
    render(<GuidedCalibration />);
    expect(screen.queryByText("RUN ALL")).toBeNull();
    fireEvent.click(screen.getByText("ADVANCED // TARGET_LIST"));
    expect(screen.getByText("RUN ALL")).toBeTruthy();
    expect(screen.getByLabelText("ADD_SPEED")).toBeTruthy();
  });
});
