import { render, screen } from "@testing-library/react";
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
    expect(line.textContent?.startsWith(`T-MINUS${NBSP}23:10 · ENDS${NBSP}`)).toBe(true);
    expect(screen.getByText(/cycle 3/)).toBeInTheDocument();
  });

  it("shows only the time left while paused", () => {
    current!.active_program = { ...current!.active_program!, paused: true, wait: null, program_seconds_remaining: 1390 };
    render(<ActiveProgramCard />);
    expect(screen.getByTestId("program-time-left").textContent).toBe(`T-MINUS${NBSP}23:10`);
  });

  it("shows nothing when the program has no fixed end", () => {
    current!.active_program = { ...current!.active_program!, program_seconds_remaining: null };
    render(<ActiveProgramCard />);
    expect(screen.queryByTestId("program-time-left")).toBeNull();
  });
});
