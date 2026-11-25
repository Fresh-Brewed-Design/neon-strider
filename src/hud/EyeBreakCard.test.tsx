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

  it("says a break set for the program's rest pauses nothing", () => {
    const s = withEye({ phase: "upcoming", remaining_s: 30, at_rest: true });
    s.deck.status = "Running";
    render(<EyeBreakCard snapshot={s} />);
    expect(
      screen.getByText("Finish what you are typing, then look away. The break starts at the program's rest."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/The belt will pause/)).toBeNull();
  });

  it("counts down a break and SKIP sends one command", async () => {
    render(<EyeBreakCard snapshot={withEye({ phase: "break", remaining_s: 72 })} />);
    expect(screen.getByText("OCULAR_IMPLANT // COOLDOWN")).toBeInTheDocument();
    expect(screen.getByText("72 S")).toBeInTheDocument();
    expect(screen.getByText("Look away from the screen.")).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "SKIP" })));
    expect(sent()).toEqual([{ command: { type: "eye_break_skip" } }]);
  });

  it("shows each guided blink step as text", () => {
    const { rerender } = render(<EyeBreakCard snapshot={withEye({ phase: "break", remaining_s: 72, blink: "close" })} />);
    expect(screen.getByText("SHUTTER // CLOSE")).toBeInTheDocument();
    expect(screen.getByText("Close your eyes and hold them closed.")).toBeInTheDocument();
    rerender(<EyeBreakCard snapshot={withEye({ phase: "break", remaining_s: 70, blink: "squeeze" })} />);
    expect(screen.getByText("SHUTTER // SQUEEZE")).toBeInTheDocument();
    expect(screen.getByText("Squeeze your eyelids gently.")).toBeInTheDocument();
    rerender(
      <EyeBreakCard snapshot={withEye({ phase: "break", remaining_s: 68, blink: "open", holds_belt: true })} />,
    );
    expect(screen.getByText("SHUTTER // OPEN")).toBeInTheDocument();
    expect(screen.getByText("Open your eyes and look far away. The belt is paused.")).toBeInTheDocument();
  });

  it("says the belt is paused while the break holds it", () => {
    render(<EyeBreakCard snapshot={withEye({ phase: "break", remaining_s: 30, holds_belt: true })} />);
    expect(screen.getByText("Look away from the screen. The belt is paused.")).toBeInTheDocument();
  });

  it("says GO when a program is paused", () => {
    const s = withEye({ phase: "complete", holds_belt: true });
    s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: null };
    render(<EyeBreakCard snapshot={s} />);
    expect(screen.getByText("The belt stays paused until you press GO.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "GO" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "RE-JACK" })).toBeNull();
  });

  it("asks before the belt moves again after a break", async () => {
    render(<EyeBreakCard snapshot={withEye({ phase: "complete", holds_belt: true })} />);
    expect(screen.getByText("OCULAR_IMPLANT // RECALIBRATED")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "SKIP" })).toBeNull();
    expect(screen.getByText("The belt stays paused until you press RE-JACK.")).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "RE-JACK" })));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "STAY STILL" })));
    expect(sent()).toEqual([
      { command: { type: "eye_break_resume" } },
      { command: { type: "eye_break_stay_still" } },
    ]);
  });
});
