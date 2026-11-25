import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

const NBSP = String.fromCharCode(160);

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { ActiveProgramCard } = await import("./ActiveProgramCard");

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
});

describe("ActiveProgramCard step line", () => {
  it("shows the speed beside the label so the step line keeps its time left", () => {
    current!.active_program = {
      ...current!.active_program!,
      phase: "walking",
      wait: null,
      paused: false,
      speed_kmh: 6,
      incline_pct: 5,
    };
    render(<ActiveProgramCard />);
    const label = screen.getByTestId("program-label-line");
    expect(label).toHaveTextContent("6.0 km/h · 5.0%");
    expect(label.lastElementChild).toHaveClass("whitespace-nowrap");
    expect(screen.getByText(/left/)).not.toHaveTextContent("km/h");
  });
});

describe("ActiveProgramCard T-MINUS", () => {
  it("shows the whole program's time left and end under the step line", () => {
    current!.active_program = { ...current!.active_program!, paused: false, wait: null, program_seconds_remaining: 1390 };
    render(<ActiveProgramCard />);
    const line = screen.getByTestId("program-time-left");
    expect(line.textContent?.startsWith(`T-MINUS 23:10${NBSP}· ENDS${NBSP}`)).toBe(true);
    expect(screen.getByText(/cycle 3/)).toBeInTheDocument();
  });

  it("shows only the time left while paused", () => {
    current!.active_program = { ...current!.active_program!, paused: true, wait: null, program_seconds_remaining: 1390 };
    render(<ActiveProgramCard />);
    expect(screen.getByTestId("program-time-left").textContent).toBe(`T-MINUS 23:10`);
  });

  it("shows nothing when the program has no fixed end", () => {
    current!.active_program = { ...current!.active_program!, program_seconds_remaining: null };
    render(<ActiveProgramCard />);
    expect(screen.queryByTestId("program-time-left")).toBeNull();
  });
});

describe("ActiveProgramCard buttons", () => {
  it("show a rejected End in the card, under the buttons", async () => {
    current!.active_program = { ...current!.active_program!, paused: false, wait: null };
    vi.mocked(invoke).mockRejectedValueOnce("Treadmill not connected");
    render(<ActiveProgramCard />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: /End/ })));
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("dispatch", { command: { type: "program_end" } });
    expect(within(screen.getByRole("alert")).getByText("Treadmill not connected")).toBeInTheDocument();
  });

  it("show a rejected GO in the card", async () => {
    current!.active_program = { ...current!.active_program!, paused: true, wait: "rest_complete" };
    vi.mocked(invoke).mockRejectedValueOnce("No program is running");
    render(<ActiveProgramCard />);
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "GO" })));
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("dispatch", { command: { type: "program_resume" } });
    expect(screen.getAllByRole("alert").map((n) => n.textContent)).toContain("No program is running");
  });
});
