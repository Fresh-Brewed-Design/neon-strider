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
  it("stays on one line: the step text truncates and the speed keeps its km/h", () => {
    current!.active_program = {
      ...current!.active_program!,
      phase: "walking",
      wait: null,
      paused: false,
      speed_kmh: 6,
      incline_pct: 5,
    };
    render(<ActiveProgramCard />);
    const line = screen.getByTestId("program-step-line");
    const [step, trailing] = Array.from(line.children);
    expect(step).toHaveClass("truncate");
    expect(trailing).toHaveClass("shrink-0", "whitespace-nowrap");
    expect(trailing).toHaveTextContent("6.0 km/h · 5.0%");
  });
});
