import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

let rawSpeed: number | null = 0;
const listeners = new Set<() => void>();
vi.mock("../../../state/useAppState", async () => {
  const { useSyncExternalStore } = await import("react");
  return {
    useAppState: (
      sel: (s: { deck: { raw_speed_kmh: number | null } }) => unknown,
    ) =>
      useSyncExternalStore(
        (l) => {
          listeners.add(l);
          return () => listeners.delete(l);
        },
        () => sel({ deck: { raw_speed_kmh: rawSpeed } }),
      ),
  };
});

const { CalibrationWizard } = await import("./CalibrationWizard");

function setSpeed(v: number) {
  act(() => {
    rawSpeed = v;
    listeners.forEach((l) => l());
  });
}

/** Opens the wizard and walks it through the countdown to the WALKING phase. */
function startWalking(onSave: (p: unknown) => void) {
  render(<CalibrationWizard onClose={() => {}} onSave={onSave} />);
  fireEvent.click(screen.getByText("INITIATE"));
  act(() => {
    vi.advanceTimersByTime(3000);
  });
  expect(screen.getByText("STOP (DONE)")).toBeTruthy();
}

describe("CalibrationWizard target speed", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    rawSpeed = 0;
  });
  afterEach(() => vi.useRealTimers());

  it("keeps the walking speed when the belt stopped before DONE", () => {
    const onSave = vi.fn();
    setSpeed(5);
    startWalking(onSave);
    act(() => {
      vi.advanceTimersByTime(30000);
    });
    setSpeed(0); // stopped
    fireEvent.click(screen.getByText("STOP (DONE)"));

    expect(screen.queryByRole("alert")).toBeNull();
    fireEvent.click(screen.getByText("SAVE MULTI-AXIS DATA"));
    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as {
      speed: number;
      distance_scale: number;
    };
    expect(saved.speed).toBe(5);
    expect(saved.distance_scale).toBeGreaterThan(0);
  });

  it("blocks saving when no belt speed was seen", () => {
    const onSave = vi.fn();
    setSpeed(5);
    render(<CalibrationWizard onClose={() => {}} onSave={onSave} />);
    fireEvent.click(screen.getByText("INITIATE"));
    setSpeed(0); // belt stops during the countdown and stays stopped
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    fireEvent.click(screen.getByText("STOP (DONE)"));

    expect(screen.getByRole("alert").textContent).toBe(
      "No belt speed was seen. Run the calibration again.",
    );
    const save = screen.getByText("SAVE MULTI-AXIS DATA") as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    fireEvent.click(save);
    expect(onSave).not.toHaveBeenCalled();
  });
});
