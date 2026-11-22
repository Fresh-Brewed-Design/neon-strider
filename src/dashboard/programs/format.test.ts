import { describe, expect, it } from "vitest";
import type { ActiveProgram, Program } from "../../ipc";
import { describeActive, describePlan, waitHeadline } from "./format";

const workday: Program = {
  id: 2,
  name: "Workday",
  steps: [{ duration_minutes: 5, speed_kmh: 3, incline_pct: 5 }],
  repeat_every_minutes: 30,
};

const active: ActiveProgram = {
  program_id: 2,
  name: "Workday",
  phase: "walking",
  step_index: 0,
  step_count: 1,
  seconds_remaining: 300,
  cycle: 2,
  paused: false,
  wait: null,
  speed_kmh: 3,
  incline_pct: 5,
  steps: workday.steps,
  repeat_every_minutes: 30,
};

describe("describePlan", () => {
  it("says a repeating program waits for GO after each rest", () => {
    expect(describePlan(workday)).toContain("After each rest, the belt waits for GO before it moves.");
  });

  it("says a walk after a 0 km/h step waits for GO, and only then", () => {
    const hold: Program = {
      ...workday,
      repeat_every_minutes: null,
      steps: [
        { duration_minutes: 1, speed_kmh: 0, incline_pct: 0 },
        { duration_minutes: 2, speed_kmh: 3, incline_pct: 0 },
      ],
    };
    expect(describePlan(hold)).toContain("After a 0 km/h step, the belt waits for GO before it moves.");
    expect(describePlan({ ...workday, repeat_every_minutes: null })).not.toContain("GO");
  });
});

describe("waiting for GO", () => {
  it("has a headline for each reason", () => {
    expect(waitHeadline("rest_complete")).toBe("REST COMPLETE // PRESS GO");
    expect(waitHeadline("hold_complete")).toBe("HOLD COMPLETE // PRESS GO");
    expect(waitHeadline("belt_still")).toBe("BELT STILL // PRESS GO");
  });

  it("describes the walk that is waiting, not one that is under way", () => {
    expect(describeActive(active)).toBe("Walk · 05:00 left · cycle 2");
    expect(describeActive({ ...active, paused: true, wait: "rest_complete" })).toBe("Next: walk · 05:00 · cycle 2");
  });
});

describe("step kinds", () => {
  const interval: ActiveProgram = {
    ...active,
    cycle: 1,
    step_index: 5,
    step_count: 11,
    seconds_remaining: 128,
    kind: { kind: "interval", nth: 3, of: 5, label: "INTERVAL 3/5" },
  };

  it("leads with the kind and keeps the step number", () => {
    expect(describeActive(interval)).toBe("INTERVAL\u00a03/5 · step\u00a06/11 · 02:08 left");
    expect(describeActive({ ...interval, paused: true, wait: "hold_complete" })).toBe("Next: INTERVAL\u00a03/5 · step\u00a06/11 · 02:08");
  });

  it("shows a one-step program by its kind alone", () => {
    expect(describeActive({ ...active, kind: { kind: "walk", nth: 1, of: 1, label: "WALK" } })).toBe("WALK · 05:00 left · cycle 2");
  });

  it("shows a step with no kind as before", () => {
    expect(describeActive({ ...interval, kind: undefined })).toBe("Walk 6/11 · 02:08 left");
  });

  it("names each kind in the plan", () => {
    const p: Program = { ...workday, steps: [{ duration_minutes: 5, speed_kmh: 4, incline_pct: 0, kind: "warm_up" }, { duration_minutes: 3, speed_kmh: 6, incline_pct: 5 }] };
    const plan = describePlan(p).split("\n");
    expect(plan[1]).toBe("1. WARM_UP 5 min at 4.0 km/h, 0% incline");
    expect(plan[2]).toBe("2. 3 min at 6.0 km/h, 5% incline");
  });
});
