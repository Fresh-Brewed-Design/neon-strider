import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../ipc";

const { snapshot: mock } = await import("../dev/mockBackend");
const { EyeBreakCard } = await import("./HudCards");

function withEye(patch: Partial<AppSnapshot["eye_break"]>): AppSnapshot {
  const s = structuredClone(mock);
  s.eye_break = { ...s.eye_break, enabled: true, ...patch };
  return s;
}

const sent = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, a]) => a);

beforeEach(() => vi.mocked(invoke).mockClear());

describe("EyeBreakCard", () => {
  it("warns ahead of a break", () => {
    render(<EyeBreakCard snapshot={withEye({ phase: "upcoming", remaining_s: 48 })} />);
    expect(screen.getByText("OCULAR_IMPLANT")).toBeInTheDocument();
    expect(screen.getByText("IMPLANT COOLDOWN IN 48 S")).toBeInTheDocument();
  });

  it("counts down a break and SKIP sends one command", async () => {
    render(<EyeBreakCard snapshot={withEye({ phase: "break", remaining_s: 72 })} />);
    expect(screen.getByText("OCULAR_IMPLANT // COOLDOWN")).toBeInTheDocument();
    expect(screen.getByText("72 S")).toBeInTheDocument();
    expect(screen.getByText("Look away from the screen.")).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "SKIP" })));
    expect(sent()).toEqual([{ command: { type: "eye_break_skip" } }]);
  });

  it("says the belt is paused while the break holds it", () => {
    render(<EyeBreakCard snapshot={withEye({ phase: "break", remaining_s: 30, holds_belt: true })} />);
    expect(screen.getByText("Look away from the screen. The belt is paused.")).toBeInTheDocument();
  });

  it("asks before the belt moves again after a break", async () => {
    render(<EyeBreakCard snapshot={withEye({ phase: "complete", holds_belt: true })} />);
    expect(screen.getByText("OCULAR_IMPLANT // RECALIBRATED")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "SKIP" })).toBeNull();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "RESUME" })));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "STAY STILL" })));
    expect(sent()).toEqual([
      { command: { type: "eye_break_resume" } },
      { command: { type: "eye_break_stay_still" } },
    ]);
  });
});
