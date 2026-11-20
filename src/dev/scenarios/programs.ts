// Programs. ?noprogram: none running.
// ?program=ready: Workday's rest is over on a still belt, and cycle 4 waits for GO
// (REST COMPLETE // PRESS GO). ?program=hold: a walking step after a 0 km/h step waits.
// ?program=still: the belt was found still after a reconnect. ?program=resting: mid-rest.
// ?program=paused: paused by the user (plain GO, no wait).
import type { ActiveProgram, Program } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** What Rust reports when a program starts. */
export function startProgram(p: Program): ActiveProgram {
  const first = p.steps[0];
  return {
    program_id: p.id,
    name: p.name,
    phase: "walking",
    step_index: 0,
    step_count: p.steps.length,
    seconds_remaining: first.duration_minutes != null ? first.duration_minutes * 60 : null,
    cycle: 1,
    paused: false,
    wait: null,
    speed_kmh: first.speed_kmh,
    incline_pct: first.incline_pct,
  };
}

export const setProgramPaused = (paused: boolean) => {
  if (!snapshot.active_program) return;
  // GO ends a wait and starts the belt; HOLD pauses it.
  Object.assign(snapshot.active_program, { paused, wait: null });
  Object.assign(snapshot.deck, paused ? { status: "Paused", speed_kmh: 0 } : { status: "Running", speed_kmh: snapshot.active_program.speed_kmh });
};

const PROGRAM_SCENARIOS: Record<string, Partial<ActiveProgram>> = {
  ready: { cycle: 4, paused: true, wait: "rest_complete", seconds_remaining: 300 },
  hold: { step_index: 1, step_count: 3, paused: true, wait: "hold_complete", seconds_remaining: 120 },
  still: { paused: true, wait: "belt_still" },
  resting: { phase: "resting", seconds_remaining: 1294 },
  paused: { paused: true },
};

export const programs: Scenario = {
  apply() {
    if (params.has("noprogram")) snapshot.active_program = null;
    const programFlag = params.get("program");
    if (programFlag !== null && snapshot.active_program && PROGRAM_SCENARIOS[programFlag]) {
      Object.assign(snapshot.active_program, PROGRAM_SCENARIOS[programFlag]);
      if (programFlag !== "resting") Object.assign(snapshot.deck, { status: "Paused", speed_kmh: 0, raw_speed_kmh: 0, target_speed_kmh: 0 });
    }
  },
  commands: {
    program_run: (c) => {
      const p = snapshot.programs.find((x) => x.id === c.id);
      if (p) snapshot.active_program = startProgram(p);
    },
    program_save: (c) => {
      const i = snapshot.programs.findIndex((x) => x.id === c.program.id);
      // Notes are Rust's: whatever the editor sent is ignored.
      const program = { ...c.program, note: undefined };
      if (i >= 0) {
        // Like Rust: the note survives a rename, not a change to the plan.
        const old = snapshot.programs[i];
        const samePlan =
          JSON.stringify(old.steps) === JSON.stringify(program.steps) && old.repeat_every_minutes === program.repeat_every_minutes;
        snapshot.programs[i] = samePlan && old.note ? { ...program, note: old.note } : program;
      } else snapshot.programs.push({ ...program, id: Math.max(0, ...snapshot.programs.map((x) => x.id)) + 1 });
    },
    program_end: () => (snapshot.active_program = null),
    program_pause: () => setProgramPaused(true),
    program_resume: () => setProgramPaused(false),
  },
};
