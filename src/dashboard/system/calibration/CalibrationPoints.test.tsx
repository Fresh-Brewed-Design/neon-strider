import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(sel: (s: AppSnapshot) => T) => (current ? sel(current) : null),
}));

const { snapshot: mock } = await import("../../../dev/mockBackend");
const { useTreadmillStore } = await import("../../../store/treadmillStore");
const { CalibrationPanel } = await import("./CalibrationPanel");
const { EditPointModal } = await import("./EditPointModal");

beforeEach(() => {
  current = structuredClone(mock);
  current.calibration.wizard_enabled = false;
});

describe("EditPointModal factor limits", () => {
  const point = { speed: 5, stride_length_cm: 70, distance_scale: 1 };

  it("blocks COMMIT_FIX when the recalculated factor is too small", () => {
    const onSave = vi.fn();
    const fast = { ...point, speed: 1 }; // factor = 1 / real km/h
    render(<EditPointModal point={fast} onClose={() => {}} onSave={onSave} />);
    fireEvent.change(screen.getAllByRole("slider")[1], { target: { value: "10" } }); // 12.6 km/h: factor 0.08
    expect(screen.getByRole("alert").textContent).toBe("Factor 0.08 is outside 0.2 to 3.0. Change the time or the factor.");
    const commit = screen.getByText("COMMIT_FIX") as HTMLButtonElement;
    expect(commit.disabled).toBe(true);
    fireEvent.click(commit);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("saves a factor inside the limits", () => {
    const onSave = vi.fn();
    render(<EditPointModal point={point} onClose={() => {}} onSave={onSave} />);
    fireEvent.click(screen.getByText("COMMIT_FIX"));
    expect(onSave).toHaveBeenCalledTimes(1);
  });
});

describe("CalibrationPanel stored factors", () => {
  it("shows the applied value next to a stored factor below the limit", () => {
    useTreadmillStore.setState({
      calibrationPoints: [
        { speed: 3, stride_length_cm: 68, distance_scale: 0.912 },
        { speed: 4.5, stride_length_cm: 74, distance_scale: 0.15 },
      ],
    });
    render(<CalibrationPanel />);
    expect(screen.getAllByText("Applied as 0.20.")).toHaveLength(1);
    expect(screen.getByText("0.150x")).toBeTruthy();
  });

  it("shows no hint for factors inside the limits", () => {
    useTreadmillStore.setState({ calibrationPoints: [{ speed: 3, stride_length_cm: 68, distance_scale: 0.9 }] });
    render(<CalibrationPanel />);
    expect(screen.queryByText(/Applied as/)).toBeNull();
  });
});
