import { describe, expect, it } from "vitest";
import type { ActiveProgram, ProgramStep } from "../../ipc";
import { cycleRestMinutes, nextStepLine, openStepMinutes, planRows, stepChart, stepCode } from "./stepMap";

const warm: ProgramStep = { duration_minutes: 5, speed_kmh: 4, incline_pct: 0, kind: "warm_up" };
const brisk: ProgramStep = { duration_minutes: 3, speed_kmh: 6, incline_pct: 5, kind: "interval", zone: { low_pct: 85, high_pct: 95 } };
const easy: ProgramStep = { duration_minutes: 3, speed_kmh: 3.5, incline_pct: 0, kind: "rest" };

function running(steps: ProgramStep[], over: Partial<ActiveProgram> = {}): ActiveProgram {
  return {
    program_id: 1,
    name: "Test",
    phase: "walking",
    step_index: 0,
    step_count: steps.length,
    seconds_remaining: steps[0]?.duration_minutes != null ? steps[0].duration_minutes * 60 : null,
    cycle: 1,
    paused: false,
    wait: null,
    speed_kmh: steps[0]?.speed_kmh ?? 0,
    incline_pct: steps[0]?.incline_pct ?? 0,
    steps,
    repeat_every_minutes: null,
    ...over,
  };
}

const interval = [warm, brisk, easy, brisk, easy];
const workday: ProgramStep[] = [{ duration_minutes: 5, speed_kmh: 3, incline_pct: 5 }];

describe("stepCode", () => {
  it("counts a kind within its kind, and names a single one by its code", () => {
    expect(stepCode(interval, 0)).toBe("WARM_UP");
    expect(stepCode(interval, 3)).toBe("INTERVAL 2/2");
    expect(stepCode(interval, 2)).toBe("REST 1/2");
  });

  it("numbers a step with no kind", () => {
    expect(stepCode(workday, 0)).toBe("STEP 1");
  });
});

describe("planRows", () => {
  it("marks steps before the running one done and the rest upcoming", () => {
    const rows = planRows(running(interval, { step_index: 2 }));
    expect(rows.map((r) => r.state)).toEqual(["done", "done", "current", "upcoming", "upcoming"]);
    expect(rows[1]).toMatchObject({ number: 2, code: "INTERVAL 1/2", minutes: 3, speedKmh: 6, inclinePct: 5, zone: { low_pct: 85, high_pct: 95 } });
  });

  it("adds the rest between cycles and marks it current while resting", () => {
    const walking = planRows(running(workday, { repeat_every_minutes: 30 }));
    expect(walking.map((r) => [r.code, r.minutes, r.state])).toEqual([
      ["STEP 1", 5, "current"],
      ["CYCLE_REST", 25, "upcoming"],
    ]);
    const resting = planRows(running(workday, { repeat_every_minutes: 30, phase: "resting", seconds_remaining: 600 }));
    expect(resting.map((r) => r.state)).toEqual(["done", "current"]);
    expect(resting[1].speedKmh).toBeNull();
  });

  it("keeps an open-ended step without minutes and adds no rest", () => {
    const steps: ProgramStep[] = [warm, { duration_minutes: null, speed_kmh: 2, incline_pct: 0 }];
    const rows = planRows(running(steps, { step_index: 1, seconds_remaining: null, repeat_every_minutes: 30 }));
    expect(rows).toHaveLength(2);
    expect(rows[1]).toMatchObject({ minutes: null, state: "current" });
  });
});

describe("cycleRestMinutes", () => {
  it("is the repeat minus the steps, and none without a rest", () => {
    expect(cycleRestMinutes(workday, 30)).toBe(25);
    expect(cycleRestMinutes(workday, 5)).toBeNull();
    expect(cycleRestMinutes(workday, null)).toBeNull();
  });
});

describe("stepChart", () => {
  it("lays timed steps out by their minutes with the line inside the running step", () => {
    // 5 + 3 + 3 + 3 + 3 = 17 min over 170 units: 10 per minute.
    const c = stepChart(running(interval, { step_index: 1, seconds_remaining: 60 }), 170, 50);
    expect(c.blocks.map((b) => [b.x0, b.x1])).toEqual([
      [0, 50],
      [50, 80],
      [80, 110],
      [110, 140],
      [140, 170],
    ]);
    // Two of the three minutes have run.
    expect(c.hereX).toBeCloseTo(70);
    expect(c.blocks.map((b) => b.state)).toEqual(["done", "current", "upcoming", "upcoming", "upcoming"]);
  });

  it("puts the speed and incline on their own scales, zero at the bottom", () => {
    const c = stepChart(running(interval), 170, 50);
    expect(c.maxSpeed).toBeCloseTo(6.6);
    expect(c.maxIncline).toBeCloseTo(8);
    expect(c.blocks[1].speedY).toBeCloseTo(50 - (6 / 6.6) * 50);
    expect(c.blocks[0].inclineY).toBe(50);
    expect(c.speedPath.startsWith("M0 ")).toBe(true);
    expect(c.speedPath.split("L")).toHaveLength(10);
  });

  it("draws the rest between cycles after the steps, with the line in it while resting", () => {
    // 5 min walk, 25 min rest over 300 units.
    const c = stepChart(running(workday, { repeat_every_minutes: 30, phase: "resting", seconds_remaining: 600 }), 300, 50);
    expect(c.blocks[1]).toMatchObject({ rest: true, x0: 50, x1: 300, speedY: null, state: "current" });
    // 15 of the 25 rest minutes have passed.
    expect(c.hereX).toBeCloseTo(200);
    // The rest has no speed of its own, so the outline stops at the steps.
    expect(c.speedPath).not.toContain("300");
  });

  it("gives an open-ended step a stand-in width and holds the line at its start", () => {
    const steps: ProgramStep[] = [{ ...warm, duration_minutes: 20 }, { duration_minutes: null, speed_kmh: 2, incline_pct: 0 }];
    expect(openStepMinutes(20)).toBe(5);
    expect(openStepMinutes(40)).toBe(10);
    const c = stepChart(running(steps, { step_index: 1, seconds_remaining: null }), 250, 50);
    expect(c.blocks[1]).toMatchObject({ open: true, x0: 200, x1: 250 });
    expect(c.hereX).toBe(200);
  });

  it("fills the chart with a program that is one open-ended step", () => {
    const c = stepChart(running([{ duration_minutes: null, speed_kmh: 2, incline_pct: 0 }]), 100, 50);
    expect(c.blocks[0]).toMatchObject({ x0: 0, x1: 100, open: true });
    expect(c.hereX).toBe(0);
  });

  it("keeps the line at the step's start while the step waits for GO", () => {
    const c = stepChart(running(interval, { step_index: 2, seconds_remaining: 180, paused: true, wait: "hold_complete" }), 170, 50);
    expect(c.hereX).toBeCloseTo(80);
  });
});

describe("nextStepLine", () => {
  it("names the next step with its kind, minutes and speed, and the incline when not flat", () => {
    expect(nextStepLine(running(interval, { step_index: 1 }))).toBe("NEXT // REST 1/2 · 3 min · 3.5 km/h");
    expect(nextStepLine(running(interval, { step_index: 0 }))).toBe("NEXT // INTERVAL 1/2 · 3 min · 6.0 km/h · 5%");
  });

  it("shows the rest between cycles after the last step, and the first step during the rest", () => {
    expect(nextStepLine(running(workday, { repeat_every_minutes: 30 }))).toBe("NEXT // CYCLE_REST · 25 min");
    expect(nextStepLine(running(workday, { repeat_every_minutes: 30, phase: "resting", seconds_remaining: 60 }))).toBe(
      "NEXT // STEP 1 · 5 min · 3.0 km/h · 5%",
    );
  });

  it("says the program ends after its last step", () => {
    expect(nextStepLine(running(interval, { step_index: 4 }))).toBe("NEXT // PROGRAM_END");
  });

  it("shows nothing while waiting for GO or on an open-ended step", () => {
    expect(nextStepLine(running(interval, { paused: true, wait: "rest_complete" }))).toBeNull();
    const open: ProgramStep[] = [{ duration_minutes: null, speed_kmh: 2, incline_pct: 0 }];
    expect(nextStepLine(running(open))).toBeNull();
  });

  it("names an open-ended next step as running until END", () => {
    const steps: ProgramStep[] = [warm, { duration_minutes: null, speed_kmh: 2, incline_pct: 0, kind: "walk" }];
    expect(nextStepLine(running(steps))).toBe("NEXT // WALK · until END · 2.0 km/h");
  });
});
