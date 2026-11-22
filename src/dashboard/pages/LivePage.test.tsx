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
const { LivePage } = await import("./LivePage");

const speedsSent = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "dispatch")
    .map(([, a]) => (a as { command: { type: string; kmh?: number } }).command)
    .filter((c) => c.type === "set_speed")
    .map((c) => c.kmh as number);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
  current.connection.connected = true;
  current.deck.min_speed_kmh = 0.93;
  current.deck.max_speed_kmh = 6.2;
  current.deck.speed_steps = [];
});

describe("Live page speed slider", () => {
  it("starts at the belt's floor", () => {
    render(<LivePage />);
    const [speed] = screen.getAllByRole("slider");
    expect(speed).toHaveAttribute("min", "0.93");
  });

  it("never sends 0 when dragged to its lowest position", async () => {
    render(<LivePage />);
    const [speed] = screen.getAllByRole("slider");
    for (const v of ["2", "1", "0.5", "0"]) {
      await act(async () => fireEvent.change(speed, { target: { value: v } }));
    }
    const sent = speedsSent();
    expect(sent.length).toBeGreaterThan(0);
    expect(Math.min(...sent)).toBeGreaterThanOrEqual(0.93);
  });
});

describe("Live page STEP_MAP", () => {
  beforeEach(() => localStorage.clear());

  it("shows the running program's steps in place of the stats until switched", () => {
    render(<LivePage />);
    expect(screen.getByRole("list", { name: "Program steps" })).toBeInTheDocument();
    expect(screen.queryByText("DECK_CYCLES")).not.toBeInTheDocument();

    const toggle = screen.getByRole("button", { name: /map/i });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(toggle);
    expect(screen.queryByRole("list", { name: "Program steps" })).not.toBeInTheDocument();
    expect(screen.getByText("DECK_CYCLES")).toBeInTheDocument();
    expect(localStorage.getItem("liveStepMap")).toBe("closed");
  });

  it("keeps the stats with no program running", () => {
    current!.active_program = null;
    render(<LivePage />);
    expect(screen.queryByRole("list", { name: "Program steps" })).not.toBeInTheDocument();
    expect(screen.getByText("DECK_CYCLES")).toBeInTheDocument();
  });
});
