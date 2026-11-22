import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../../ipc";

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
