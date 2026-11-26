// The HUD's one-shot motion, replayed so it can be watched and checked
// mid-animation. ?linkup: the treadmill links 3 s after load (LINKING 1/3
// before), then drops and links again every 8 s, so the logo decodes each
// time. ?stepsweep: the running program moves to its next step 3 s after load
// and every 6 s after, so the scanline sweeps with the step's name; with
// ?program=interval the names are Interval walk's (WARM_UP, INTERVAL 3/5...).
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Seconds after load before the first replay: the HUD takes its baseline first. */
const FIRST_AT_S = 3;
/** ?linkup: seconds the link stays up before it drops for the next replay. */
const LINK_UP_S = 5;
/** ?stepsweep: seconds between step changes. */
const STEP_EVERY_S = 6;

let elapsed = 0;

function linkDown() {
  Object.assign(snapshot.connection, { connected: false, connecting: true, link: { state: "connecting", attempt: 1, of: 3 } });
}

function linkUp() {
  Object.assign(snapshot.connection, { connected: true, connecting: false, link: { state: "connected" }, last_error: null });
}

/** The running program's next step, with its kind counted the way Rust labels it. */
function nextStep() {
  const a = snapshot.active_program;
  if (!a || a.steps.length === 0) return;
  const index = (a.step_index + 1) % a.steps.length;
  const step = a.steps[index];
  const sameKind = a.steps.filter((s) => s.kind === step.kind);
  const nth = a.steps.slice(0, index + 1).filter((s) => s.kind === step.kind).length;
  const code = step.kind?.toUpperCase();
  Object.assign(a, {
    step_index: index,
    cycle: index === 0 ? a.cycle + 1 : a.cycle,
    seconds_remaining: step.duration_minutes != null ? step.duration_minutes * 60 : null,
    speed_kmh: step.speed_kmh,
    incline_pct: step.incline_pct,
    kind:
      step.kind && code
        ? { kind: step.kind, nth, of: sameKind.length, label: sameKind.length > 1 ? `${code} ${nth}/${sameKind.length}` : code }
        : undefined,
  });
}

export const hudMotion: Scenario = {
  apply() {
    if (params.has("linkup")) linkDown();
  },
  tick() {
    const link = params.has("linkup");
    const step = params.has("stepsweep");
    if (!link && !step) return false;
    elapsed += 1;
    const t = elapsed - FIRST_AT_S;
    if (t < 0) return false;
    let changed = false;
    if (link) {
      const at = t % (LINK_UP_S + FIRST_AT_S);
      if (at === 0) linkUp();
      else if (at === LINK_UP_S) linkDown();
      changed ||= at === 0 || at === LINK_UP_S;
    }
    if (step && t % STEP_EVERY_S === 0) {
      nextStep();
      changed = true;
    }
    return changed;
  },
};
