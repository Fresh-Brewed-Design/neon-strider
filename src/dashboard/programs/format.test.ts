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
    expect(waitHeadline("lights_out")).toBe("LIGHTS OUT // PRESS GO");
  });

  it("describes the walk that is waiting, not one that is under way", () => {
    expect(describeActive(active)).toBe("Walk · 05:00 left · cycle 2");
    expect(describeActive({ ...active, paused: true, wait: "rest_complete" })).toBe("Next: walk · 05:00 · cycle 2");
  });
});
