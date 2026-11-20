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
const { CalibrateOffer } = await import("./CalibrateOffer");

const types = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, args]) => (args as { command: { type: string } }).command.type);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("the first-connect offer", () => {
  it("stays hidden unless Rust offers it", () => {
    render(<CalibrateOffer onCalibrate={() => {}} />);
    expect(screen.queryByText("Calibrate this treadmill")).toBeNull();
  });

  it("opens calibration without touching the belt", () => {
    current!.calibration.offer = true;
    const onCalibrate = vi.fn();
    render(<CalibrateOffer onCalibrate={onCalibrate} />);
    expect(screen.getByText("The connected treadmill has no calibration yet.")).toBeTruthy();
    fireEvent.click(screen.getByText("Calibrate this treadmill"));
    expect(onCalibrate).toHaveBeenCalledOnce();
    expect(types()).toEqual([]);
  });

  it("SKIP tells Rust, which keeps it for the treadmill", async () => {
    current!.calibration.offer = true;
    render(<CalibrateOffer onCalibrate={() => {}} />);
    await act(async () => fireEvent.click(screen.getByText("SKIP")));
    expect(types()).toEqual(["calibration_offer_skip"]);
  });

  it("shows a failed SKIP under the button", async () => {
    current!.calibration.offer = true;
    vi.mocked(invoke).mockRejectedValueOnce("Connect the treadmill first.");
    render(<CalibrateOffer onCalibrate={() => {}} />);
    await act(async () => fireEvent.click(screen.getByText("SKIP")));
    expect(screen.getByText("Connect the treadmill first.")).toBeTruthy();
  });
});
