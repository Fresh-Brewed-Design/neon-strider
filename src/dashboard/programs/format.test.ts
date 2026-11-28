import { describe, expect, it } from "vitest";
import type { ActiveProgram, Program } from "../../ipc";
import { describeActive, describePlan, describeProgram, passDistanceKm, describeTimeLeft, finishClock, waitHeadline, waitReason } from "./format";

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
  program_seconds_remaining: null,
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

  // jsdom has no layout, so the 760x600 fit is measured in the dashboard preview
  // (?program=ready|hold|still). This keeps each reason to the one line it has beside the buttons.
  it("keeps each reason short enough for one line on the program card", () => {
    for (const w of ["rest_complete", "hold_complete", "belt_still"] as const) {
      expect(waitReason(w).length).toBeLessThanOrEqual(34);
    }
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

  it("names the pulse in place of the time while a cool-down waits on it (#233)", () => {
    const cooling: ActiveProgram = {
      ...active,
      cycle: 1,
      step_index: 1,
      step_count: 2,
      seconds_remaining: null,
      pulse_to_bpm: 114,
      kind: { kind: "cool_down", nth: 1, of: 1, label: "COOL_DOWN" },
    };
    expect(describeActive(cooling)).toBe("COOL_DOWN · step\u00a02/2 · pulse\u00a0to\u00a0114");
    expect(describeActive({ ...cooling, kind: undefined })).toBe("Walk 2/2 · pulse\u00a0to\u00a0114");
    // During its minutes the countdown shows as for any step.
    expect(describeActive({ ...cooling, pulse_to_bpm: undefined, seconds_remaining: 95 })).toBe("COOL_DOWN · step\u00a02/2 · 01:35 left");
  });

  it("names a cool-down's pulse target in the plan", () => {
    const p: Program = { ...workday, repeat_every_minutes: null, steps: [{ duration_minutes: 5, speed_kmh: 3, incline_pct: 0, kind: "cool_down", pulse_end: { below_pct: 60 } }] };
    expect(describePlan(p).split("\n")[1]).toBe("COOL_DOWN 5 min at 3.0 km/h, 0% incline, then on until heart rate is down to 60 % of max");
  });
});

describe("T-MINUS", () => {
  const now = new Date(2026, 10, 24, 15, 17, 0);
  const NBSP = String.fromCharCode(160);
  const finite: ActiveProgram = { ...active, name: "Brisk", repeat_every_minutes: null, cycle: 1, program_seconds_remaining: 23 * 60 + 10 };

  it("shows the time left and the end on the PC clock", () => {
    expect(describeTimeLeft(finite, now, "en-GB")).toBe(`T-MINUS 23:10${NBSP}· ENDS${NBSP}15:40`);
  });

  it("writes the end in the user's time format, with no seconds", () => {
    expect(finishClock(23 * 60 + 10, now, "en-US")).toMatch(/^3:40\sPM$/);
    expect(finishClock(23 * 60 + 10, now, "de-DE")).toBe("15:40");
  });

  it("counts hours past the first", () => {
    expect(describeTimeLeft({ ...finite, program_seconds_remaining: 3600 + 125 }, now, "en-GB")).toBe(`T-MINUS 1:02:05${NBSP}· ENDS${NBSP}16:19`);
  });

  it("drops the end while paused, since it drifts", () => {
    expect(describeTimeLeft({ ...finite, paused: true }, now, "en-GB")).toBe(`T-MINUS 23:10`);
  });

  it("shows nothing for a program with no fixed end", () => {
    expect(describeTimeLeft(active, now, "en-GB")).toBeNull();
  });
});

describe("T-MINUS wrapping", () => {
  it("wraps after the dot or after T-MINUS, never inside the end time", () => {
    const a: ActiveProgram = { ...active, repeat_every_minutes: null, program_seconds_remaining: 1390 };
    const line = describeTimeLeft(a, new Date(2026, 10, 24, 15, 17, 0), "en-US")!;
    expect(line.split(" ")).toHaveLength(3);
    expect(line.split(" ")[2]).toMatch(/^ENDS.3:40.PM$/);
  });
});

describe("program distance", () => {
  const step = (speed_kmh: number, duration_minutes: number | null) => ({ duration_minutes, speed_kmh, incline_pct: 0 });
  const prog = (steps: Program["steps"], repeat_every_minutes: number | null = null): Program => ({ id: 9, name: "T", steps, repeat_every_minutes });

  it("sums speed times minutes over every step", () => {
    const p = prog([step(4, 15), step(6, 30)]);
    expect(passDistanceKm(p)).toBeCloseTo(4, 6);
    expect(describeProgram(p)).toBe("45 min · 2 steps · ≈4.0 km");
    expect(describePlan(p).split("\n").at(-1)).toBe("About 4.0 km a pass at the set speeds.");
  });

  it("counts one cycle of a repeating program", () => {
    expect(describeProgram(workday)).toBe("5 min every 30 min · 3.0 km/h · 5% · ≈0.3 km a cycle");
    expect(describePlan(workday).split("\n").at(-1)).toBe("About 0.3 km each cycle at the set speeds.");
  });

  it("shows nothing for an open-ended program", () => {
    const p = prog([step(3, 10), step(3, null)]);
    expect(passDistanceKm(p)).toBeNull();
    expect(describeProgram(p)).not.toContain("≈");
    expect(describePlan(p)).not.toContain("About");
  });

  it("adds nothing for a 0 km/h step", () => {
    expect(passDistanceKm(prog([step(0, 10), step(3, 20)]))).toBeCloseTo(1, 6);
  });

  it("skips a distance that rounds to 0.0", () => {
    const p = prog([step(1, 2)]);
    expect(passDistanceKm(p)).toBeCloseTo(0.0333, 3);
    expect(describeProgram(p)).not.toContain("≈");
    expect(passDistanceKm(prog([]))).toBeNull();
  });
});
