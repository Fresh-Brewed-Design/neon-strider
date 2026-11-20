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

beforeEach(() => vi.mocked(invoke).mockClear());

describe("EyeBreakCard", () => {
  it("warns ahead of a break", () => {
    render(<EyeBreakCard snapshot={withEye({ phase: "upcoming", remaining_s: 48 })} />);
    expect(screen.getByText("EYE BREAK IN 48 S")).toBeInTheDocument();
  });

  it("counts down a break and SKIP sends one command", async () => {
    render(<EyeBreakCard snapshot={withEye({ phase: "break", remaining_s: 72 })} />);
    expect(screen.getByText("LOOK AWAY // 72 S")).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "SKIP" })));
    const sent = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);
    expect(sent).toEqual([{ command: { type: "eye_break_skip" } }]);
  });
});
