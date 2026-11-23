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
const { EasyStartSetting } = await import("./EasyStartSetting");

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("System -> Treadmill Easy start", () => {
  it("shows Rust's switch, on by default", () => {
    render(<EasyStartSetting />);
    expect(screen.getByRole("switch", { name: "Easy start" })).toHaveAttribute("aria-checked", "true");
  });

  it("asks Rust to switch it off", async () => {
    render(<EasyStartSetting />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: "Easy start" })));
    expect(dispatched()).toEqual([{ command: { type: "easy_start_set", on: false } }]);
  });

  it("shows it off when Rust says so", () => {
    current!.easy_start = { enabled: false };
    render(<EasyStartSetting />);
    expect(screen.getByRole("switch", { name: "Easy start" })).toHaveAttribute("aria-checked", "false");
  });
});
