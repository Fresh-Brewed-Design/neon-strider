import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { snapshot: mock } = await import("../dev/mockBackend");
const { WalkAwayCard } = await import("./HudCards");

const sent = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, a]) => a);

beforeEach(() => vi.mocked(invoke).mockClear());

describe("WalkAwayCard", () => {
  it("names the belt's words after a pause", async () => {
    const s = structuredClone(mock);
    s.walkaway = {
      ...s.walkaway,
      card: "paused",
      reason: "idle",
      idle_minutes: 10,
    };
    s.active_program = null;
    render(<WalkAwayCard snapshot={s} />);
    expect(screen.getByText(/Press RE-JACK when you are back/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "DUMP end walk" }),
    ).toHaveAttribute("title", "Stop the belt and end the session.");
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "RE-JACK" })),
    );
    await act(async () =>
      fireEvent.click(screen.getByRole("button", { name: "DUMP end walk" })),
    );
    expect(sent()).toEqual([
      { command: { type: "walkaway_resume" } },
      { command: { type: "walkaway_end" } },
    ]);
  });

  it("says GO when a program is paused", () => {
    const s = structuredClone(mock);
    s.walkaway = { ...s.walkaway, card: "paused", reason: "lock", idle_minutes: 10 };
    s.active_program = { ...structuredClone(mock.active_program!), phase: "walking", paused: true, wait: null };
    render(<WalkAwayCard snapshot={s} />);
    expect(screen.getByRole("button", { name: "GO" })).toBeInTheDocument();
    expect(screen.getByText(/Press GO when you are back\./)).toBeInTheDocument();
  });
});
