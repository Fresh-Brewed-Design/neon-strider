import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ActiveProgram } from "../../ipc";
import { StepMap } from "./StepMap";
import { joinedLine } from "../../test/joined";

const { snapshot: mock } = await import("../../dev/mockBackend");

/** Interval walk (id 7 in the mock) on its 6th step, two minutes in, as `?program=interval` shows it. */
function intervalRound(): ActiveProgram {
  const p = mock.programs.find((x) => x.id === 7)!;
  return {
    ...structuredClone(mock.active_program!),
    program_id: 7,
    name: p.name,
    step_index: 5,
    step_count: p.steps.length,
    seconds_remaining: 128,
    cycle: 1,
    steps: structuredClone(p.steps),
    repeat_every_minutes: null,
  };
}

describe("STEP_MAP", () => {
  it("lists every step with the running one marked and the ones before it done", () => {
    render(<StepMap a={intervalRound()} />);
    const rows = within(screen.getByRole("list", { name: "Program steps" })).getAllByRole("listitem");
    expect(rows).toHaveLength(11);
    expect(rows.map((r) => r.dataset.state)).toEqual([...Array(5).fill("done"), "current", ...Array(5).fill("upcoming")]);
    expect(rows[5]).toHaveAttribute("aria-current", "step");
    expect(rows[5]).toHaveTextContent("INTERVAL 3/5");
    expect(rows[5]).toHaveTextContent("3 min");
    expect(rows[5]).toHaveTextContent("6.0 km/h");
    expect(rows[5]).toHaveTextContent("5%");
  });

  it("shows the cycle, the rest between cycles and a heart zone", () => {
    const a = { ...intervalRound(), step_index: 0, seconds_remaining: 300, cycle: 2, repeat_every_minutes: 60 };
    a.steps[1].zone = { low_pct: 85, high_pct: 95 };
    render(<StepMap a={a} />);
    expect(screen.getByText(joinedLine("Cycle 2 · repeats every 60 min"))).toBeInTheDocument();
    const rows = screen.getAllByRole("listitem");
    expect(rows[rows.length - 1]).toHaveTextContent("CYCLE_REST");
    expect(rows[rows.length - 1]).toHaveTextContent("25 min");
    expect(rows[rows.length - 1]).toHaveTextContent("speed before program");
    // The rest row's words take the speed and incline columns together, so they stay on one line.
    expect(screen.getByText("speed before program")).toHaveClass("col-span-2");
    expect(rows[1]).toHaveTextContent("HR 85-95%");
  });

  it("draws the chart with a legend of the axis tops", () => {
    render(<StepMap a={intervalRound()} />);
    expect(screen.getByRole("img", { name: /speed and incline/i })).toBeInTheDocument();
    expect(screen.getByText("Max 6.6 km/h")).toBeInTheDocument();
    expect(screen.getByText("Max 8.0%")).toBeInTheDocument();
  });
});
