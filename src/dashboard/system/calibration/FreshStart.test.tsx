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

const types = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, args]) => (args as { command: { type: string } }).command.type);

const point = (speed: number) => ({ speed, stride_length_cm: 68, distance_scale: 0.9, runs: 1, stride_runs: 0 });

/** A known treadmill with a 280 cm belt and these points. */
function ready(points: number[], aside: number | null = null) {
  const s = structuredClone(mock);
  s.calibration.belt_cm = 280;
  s.calibration.belt_treadmill = true;
  s.calibration.aside_count = aside;
  current = s;
  useTreadmillStore.setState({ calibrationPoints: points.map(point), pendingConfirmation: null });
}

/** GROUND_TRUTH starts shut for a calibrated treadmill; open it unless a run holds it open. */
function renderOpen() {
  const view = render(<GuidedCalibration />);
  const details = screen.queryByText("GROUND_TRUTH details");
  if (details) fireEvent.click(details);
  return view;
}

beforeEach(() => vi.mocked(invoke).mockClear());

describe("START FRESH", () => {
  it("is offered with the top speed question while the treadmill has points", async () => {
    ready([3, 4.5]);
    renderOpen();
    expect(screen.getByText("START FRESH sets the saved points aside for a new walk.")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("START FRESH")));
    expect(types()).toEqual(["calibration_start_fresh"]);
  });

  it("is not offered with no points, or while a set is aside", () => {
    ready([]);
    const { unmount } = renderOpen();
    expect(screen.queryByText("START FRESH")).toBeNull();
    unmount();
    ready([3], 2);
    renderOpen();
    expect(screen.queryByText("START FRESH")).toBeNull();
  });

  it("shows a refusal under the button", async () => {
    ready([3]);
    vi.mocked(invoke).mockRejectedValueOnce("End the open run or walk first.");
    renderOpen();
    await act(async () => fireEvent.click(screen.getByText("START FRESH")));
    expect(screen.getByText("End the open run or walk first.")).toBeTruthy();
  });
});

describe("POINTS_ASIDE", () => {
  it("says how many points are aside", () => {
    ready([], 1);
    renderOpen();
    expect(screen.getByText("1 old point is set aside.")).toBeTruthy();
  });

  it("RESTORE with no new points needs no confirmation", async () => {
    ready([], 4);
    renderOpen();
    expect(screen.getByText("4 old points are set aside.")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByText("RESTORE")));
    expect(types()).toEqual(["calibration_restore"]);
  });

  it("RESTORE asks first when it would drop new points", async () => {
    ready([3, 4], 4);
    renderOpen();
    fireEvent.click(screen.getByText("RESTORE"));
    expect(types()).toEqual([]);
    const ask = useTreadmillStore.getState().pendingConfirmation;
    expect(ask?.message).toBe("RESTORE drops the 2 new points.");
    await act(async () => ask?.onConfirm());
    expect(types()).toEqual(["calibration_restore"]);
  });

  it("KEEP NEW asks before deleting the old set", async () => {
    ready([3], 4);
    renderOpen();
    fireEvent.click(screen.getByText("KEEP NEW"));
    const ask = useTreadmillStore.getState().pendingConfirmation;
    expect(ask?.message).toBe("KEEP NEW deletes the 4 old points for good.");
    expect(types()).toEqual([]);
    await act(async () => ask?.onConfirm());
    expect(types()).toEqual(["calibration_keep_new"]);
  });
});
