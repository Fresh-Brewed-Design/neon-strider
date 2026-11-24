// Programs. ?noprogram: none running. ?noprograms: none saved.
// ?program=ready: Workday's rest is over on a still belt, and cycle 4 waits for GO
// (REST COMPLETE // PRESS GO). ?program=hold: a walking step after a 0 km/h step waits.
// ?program=still: the belt was found still after a reconnect. ?program=resting: mid-rest.
// ?program=paused: paused by the user (plain GO, no wait).
// ?program=interval: Interval walk on its 3rd brisk round (INTERVAL 3/5, step 6 of 11), with
// T-MINUS 17:08 to its end.
import type { ActiveProgram, Program, ProgramStep, StepKindSpot } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Rust's `StepKindSpot::of_step`: the step's kind, counted within that kind. */
export function kindSpot(steps: ProgramStep[], index: number): StepKindSpot | undefined {
  const kind = steps[index]?.kind;
  if (!kind) return undefined;
  const nth = steps.slice(0, index + 1).filter((s) => s.kind === kind).length;
  const of = steps.filter((s) => s.kind === kind).length;
  const code = kind.toUpperCase();
  return { kind, nth, of, label: of > 1 ? `${code} ${nth}/${of}` : code };
}

/** Below this a speed holds the belt still: Rust's `engine::STILL_KMH`. */
const MOVING_KMH = 0.05;

/**
 * Rust's `programs::program_left_s` (T-MINUS): `stepLeft` seconds of step
 * `index` plus every later step. Null when the program repeats, a step to
 * come has no minutes, a cool-down with a pulse target meets a heart rate
 * reading, or a walking step follows a 0 km/h one (it waits for GO).
 */
export function programLeft(steps: ProgramStep[], repeat: number | null, index: number, stepLeft: number | null, pulseReading = false): number | null {
  if (repeat != null || stepLeft == null) return null;
  let left = stepLeft;
  for (let i = index; i < steps.length; i++) {
    const st = steps[i];
    if (st.pulse_end && pulseReading) return null;
    if (i > index) {
      if (steps[i - 1].speed_kmh < MOVING_KMH && st.speed_kmh >= MOVING_KMH) return null;
      if (st.duration_minutes == null) return null;
      left += st.duration_minutes * 60;
    }
  }
  return left;
}

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
    kind: kindSpot(p.steps, 0),
    steps: structuredClone(p.steps),
    repeat_every_minutes: p.repeat_every_minutes,
    program_seconds_remaining: programLeft(p.steps, p.repeat_every_minutes, 0, first.duration_minutes != null ? first.duration_minutes * 60 : null),
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
  hold: {
    step_index: 1,
    step_count: 3,
    paused: true,
    wait: "hold_complete",
    seconds_remaining: 120,
    kind: undefined,
    // A 0 km/h step, then two walking steps; no kinds, as before #235.
    steps: [
      { duration_minutes: 1, speed_kmh: 0, incline_pct: 0 },
      { duration_minutes: 2, speed_kmh: 3, incline_pct: 5 },
      { duration_minutes: 5, speed_kmh: 3, incline_pct: 5 },
    ],
    repeat_every_minutes: null,
    program_seconds_remaining: 420,
  },
  still: { paused: true, wait: "belt_still" },
  resting: { phase: "resting", seconds_remaining: 1294 },
  paused: { paused: true },
};

/** `?program=interval`: Interval walk (id 7) running its 6th step. */
function intervalRound(): ActiveProgram | null {
  const p = snapshot.programs.find((x) => x.id === 7);
  if (!p) return null;
  const index = 5;
  const step = p.steps[index];
  return {
    ...startProgram(p),
    step_index: index,
    seconds_remaining: 128,
    speed_kmh: step.speed_kmh,
    incline_pct: step.incline_pct,
    kind: kindSpot(p.steps, index),
    program_seconds_remaining: programLeft(p.steps, p.repeat_every_minutes, index, 128),
  };
}

export const programs: Scenario = {
  apply() {
    if (params.has("noprogram")) snapshot.active_program = null;
    if (params.has("noprograms")) {
      snapshot.programs = [];
      snapshot.active_program = null;
    }
    const programFlag = params.get("program");
    if (programFlag === "interval" && snapshot.active_program) {
      snapshot.active_program = intervalRound();
      return;
    }
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
