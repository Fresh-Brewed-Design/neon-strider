import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { HeartRateSection } = await import("./HeartRateSection");
const { useTreadmillStore } = await import("../../store/treadmillStore");

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("System's max heart rate", () => {
  it("says zones use the estimate until a max is set", () => {
    render(<HeartRateSection />);
    expect((screen.getByLabelText(/^Max heart rate/) as HTMLInputElement).value).toBe("190");
    expect(screen.getByText(/use 190 bpm, the estimate for age 30/)).toBeTruthy();
  });

  it("sends a typed max to Rust", async () => {
    current!.heart_rate = { ...current!.heart_rate, max_bpm: 178, max_bpm_set: true };
    render(<HeartRateSection />);
    const field = screen.getByLabelText(/^Max heart rate/);
    expect(screen.getByText("Program heart zones are a percentage of the max heart rate.")).toBeTruthy();
    fireEvent.change(field, { target: { value: "182" } });
    await act(async () => fireEvent.blur(field));
    expect(dispatched()).toEqual([{ command: { type: "heart_rate_max_set", bpm: 182 } }]);
  });
});

describe("System's heart rate sensor", () => {
  it("asks before Forget drops the picked sensor", async () => {
    current!.heart_rate = { ...current!.heart_rate, sensor: { id: "hr-1", name: "Forerunner 265" } };
    render(<HeartRateSection />);
    fireEvent.click(screen.getByRole("button", { name: "Forget" }));
    expect(dispatched()).toEqual([]);
    const ask = useTreadmillStore.getState().pendingConfirmation;
    expect(ask).toMatchObject({
      message: "Neon Strider stops reading Forerunner 265 until you pick a sensor again.",
      confirmLabel: "Forget",
      tone: "danger",
    });
    await act(async () => ask?.onConfirm());
    expect(dispatched()).toEqual([{ command: { type: "heart_rate_forget" } }]);
  });
});
