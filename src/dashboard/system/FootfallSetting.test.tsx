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
const { FootfallSetting } = await import("./FootfallSetting");

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);
const LABEL = "FOOTFALL: Count the deck's steps as presence";

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("System -> Diagnostics FOOTFALL", () => {
  it("is off by default and shows no count before a frame", () => {
    render(<FootfallSetting />);
    expect(screen.getByRole("switch", { name: LABEL })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("No step count from the treadmill yet.")).toBeTruthy();
  });

  it("asks Rust to switch it on", async () => {
    render(<FootfallSetting />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: LABEL })));
    expect(dispatched()).toEqual([{ command: { type: "walkaway_steps_set", on: true } }]);
  });

  it("shows Rust's state and the seconds Rust waits", () => {
    current!.walkaway = { ...current!.walkaway, steps_presence: true, steps_away_s: 45 };
    render(<FootfallSetting />);
    expect(screen.getByRole("switch", { name: LABEL })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(screen.getByRole("button", { name: `More about ${LABEL}` }));
    expect(screen.getByText("No steps and no input for 45 s starts the walk-away warning.")).toBeTruthy();
  });

  it("shows the deck's count with the switch off", () => {
    current!.walkaway = {
      ...current!.walkaway,
      deck_steps: { count: 2318, moves: 40, live: true, still_s: 0 },
    };
    render(<FootfallSetting />);
    expect(screen.getByText("Deck step count 2318, moving.")).toBeTruthy();
  });

  it("shows a failed switch under itself", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("database is locked");
    render(<FootfallSetting />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: LABEL })));
    expect(screen.getByText("Could not switch FOOTFALL on: database is locked")).toBeTruthy();
  });
});
