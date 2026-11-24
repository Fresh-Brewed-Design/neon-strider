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
const { ProgramTonesSetting } = await import("./ProgramTonesSetting");

const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("Programs page, Program tones", () => {
  it("shows Rust's switch, off by default", () => {
    render(<ProgramTonesSetting />);
    expect(screen.getByRole("switch", { name: "Program tones" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("The HUD beeps when a program changes step or waits for GO.")).toBeInTheDocument();
  });

  it("asks Rust to switch it on", async () => {
    render(<ProgramTonesSetting />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: "Program tones" })));
    expect(dispatched()).toEqual([{ command: { type: "program_tones_set", on: true } }]);
  });

  it("shows it on when Rust says so", () => {
    current!.program_tones = { ...current!.program_tones, enabled: true };
    render(<ProgramTonesSetting />);
    expect(screen.getByRole("switch", { name: "Program tones" })).toHaveAttribute("aria-checked", "true");
  });
});
