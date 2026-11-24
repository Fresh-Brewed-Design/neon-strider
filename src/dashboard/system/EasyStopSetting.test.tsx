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
const { EasyStopSetting } = await import("./EasyStopSetting");

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("System -> Treadmill Easy stop", () => {
  it("shows Rust's switch, off by default, with Rust's ramp length", () => {
    render(<EasyStopSetting />);
    expect(screen.getByRole("switch", { name: "Easy stop" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText(/over 20 s, then stops it\./)).toBeInTheDocument();
  });

  it("asks Rust to switch it on", async () => {
    render(<EasyStopSetting />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: "Easy stop" })));
    expect(dispatched()).toEqual([{ command: { type: "easy_stop_set", on: true } }]);
  });

  it("shows it on when Rust says so", () => {
    current!.easy_stop = { ...current!.easy_stop, enabled: true };
    render(<EasyStopSetting />);
    expect(screen.getByRole("switch", { name: "Easy stop" })).toHaveAttribute("aria-checked", "true");
  });
});
