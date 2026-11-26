// ?programtones: Program tones switched on (System, Desk). Three seconds after
// the preview loads a cue fires, as Rust's desk loop would: `=step` (or no value)
// the running program moves to its next step, `=wait` its rest ends and it waits
// for GO (REST COMPLETE // PRESS GO), `=speedup` the next step is faster and
// starts in 5 s. SKIP with the switch on fires the step cue too.
import type { ProgramCue } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Seconds after load before the flag's cue fires: the HUD takes its baseline first. */
const FIRE_AFTER_S = 3;

let elapsed = 0;
let fired = false;

function fire(cue: ProgramCue) {
  const t = snapshot.program_tones;
  Object.assign(t, { seq: t.seq + 1, cue, at_ms: Date.now() });
}

/** The running program's next step (or the first step of the next cycle). */
function nextStep() {
  const a = snapshot.active_program;
  if (!a || a.steps.length === 0) return;
  const index = (a.step_index + 1) % a.steps.length;
  const step = a.steps[index];
  Object.assign(a, {
    step_index: index,
    cycle: index === 0 ? a.cycle + 1 : a.cycle,
    seconds_remaining: step.duration_minutes != null ? step.duration_minutes * 60 : null,
    speed_kmh: step.speed_kmh,
    incline_pct: step.incline_pct,
  });
}

function fireFlag(flag: string) {
  const a = snapshot.active_program;
  if (flag === "wait") {
    if (a) Object.assign(a, { phase: "walking", paused: true, wait: "rest_complete", cycle: a.cycle + 1, step_index: 0 });
    Object.assign(snapshot.deck, { status: "Paused", speed_kmh: 0, raw_speed_kmh: 0, target_speed_kmh: 0 });
    fire("wait");
  } else if (flag === "speedup") {
    if (a) a.seconds_remaining = 5;
    fire("speed_up");
  } else {
    nextStep();
    fire("step");
  }
}

export const programTones: Scenario = {
  apply() {
    if (params.has("programtones")) snapshot.program_tones.enabled = true;
  },
  tick() {
    const flag = params.get("programtones");
    if (flag === null || fired) return false;
    elapsed += 1;
    if (elapsed < FIRE_AFTER_S) return false;
    fired = true;
    fireFlag(flag);
    return true;
  },
  commands: {
    program_tones_set: (c) => (snapshot.program_tones.enabled = c.on),
    program_skip: () => {
      nextStep();
      // Like Rust: a step change cues only while the belt runs.
      if (snapshot.program_tones.enabled && snapshot.deck.status === "Running") fire("step");
    },
  },
};
