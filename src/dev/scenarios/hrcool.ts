// A cool-down step that ends on heart rate (#233). ?hrcool=high|holding|clear|wait:
// "Cool walk" on its COOL_DOWN step, ending at or under 60 % of 190 (114 bpm), with a
// watch reading. =high: past its minutes with the pulse at 126, so the step walks on
// (`COOL_DOWN · step 2/2 · pulse to 114`); =holding: past its minutes at 111 bpm with
// 18 s of the hold left; =clear: 1:35 of its minutes left with the pulse already down;
// =wait: the watch picked but not reading, so the minutes end the step.
import type { Program, PulseTarget } from "../../ipc";
import { params, snapshot } from "../mockState";
import { kindSpot, startProgram } from "./programs";
import type { Scenario } from "./types";

/** Rust's `heart_rate::pulse_target`, for the preview: same rule, same words. */
export function mockPulseTarget(target: number, bpm: number | null, holdS: number | null): PulseTarget {
  const down = bpm === null ? null : bpm <= target;
  if (down === null)
    return { target_bpm: target, down, label: "PULSE // WAIT", message: "With no heart rate reading, the cool-down step ends on its minutes.", hold_s: null };
  if (!down)
    return { target_bpm: target, down, label: "PULSE // HIGH", message: "The cool-down step walks on until heart rate drops to the pulse target.", hold_s: null };
  if (holdS !== null)
    return {
      target_bpm: target,
      down,
      label: `PULSE // HOLD ${holdS}S`,
      message: "Heart rate is at the pulse target and needs to hold there for 30 s.",
      hold_s: holdS,
    };
  return { target_bpm: target, down, label: "PULSE // CLEAR", message: "Heart rate is down, so the cool-down step ends on its minutes.", hold_s: null };
}

/** A 20 minute walk, then a 5 minute cool-down that ends on heart rate. */
export const COOL_WALK: Program = {
  id: 20,
  name: "Cool walk",
  steps: [
    { duration_minutes: 20, speed_kmh: 5, incline_pct: 0, kind: "walk" },
    { duration_minutes: 5, speed_kmh: 3, incline_pct: 0, kind: "cool_down", pulse_end: { below_pct: 60 } },
  ],
  repeat_every_minutes: null,
};

/** Each flag: the reading, whether the minutes are done, and the hold left. */
const FLAGS: Record<string, { bpm: number | null; held: boolean; holdS: number | null }> = {
  high: { bpm: 126, held: true, holdS: 30 },
  holding: { bpm: 111, held: true, holdS: 18 },
  clear: { bpm: 108, held: false, holdS: null },
  wait: { bpm: null, held: false, holdS: 30 },
};

/** Ten minutes at one point per 5 s, falling from 150 bpm toward `end` over the last few minutes. */
function history(end: number | null): (number | null)[] {
  if (end === null) return Array.from({ length: 120 }, () => null);
  return Array.from({ length: 120 }, (_, i) => {
    const fall = Math.min(Math.max(i - 50, 0), 70) / 70;
    return Math.round(150 - fall * (150 - end) + Math.sin(i * 0.35) * 1.5);
  });
}

export const hrCool: Scenario = {
  apply() {
    const flag = params.get("hrcool") ?? "";
    const f = FLAGS[flag];
    if (!f) return;
    if (!snapshot.programs.some((p) => p.id === COOL_WALK.id)) snapshot.programs.push(COOL_WALK);
    const step = COOL_WALK.steps[1];
    snapshot.active_program = {
      ...startProgram(COOL_WALK),
      step_index: 1,
      seconds_remaining: f.held ? null : 95,
      speed_kmh: step.speed_kmh,
      incline_pct: step.incline_pct,
      kind: kindSpot(COOL_WALK.steps, 1),
      pulse_to_bpm: f.held ? 114 : undefined,
      // A cool-down the pulse may keep going has no known end.
      program_seconds_remaining: f.bpm === null && !f.held ? 95 : null,
    };
    Object.assign(snapshot.deck, { status: "Running", speed_kmh: 3, raw_speed_kmh: 3, target_speed_kmh: 3 });
    // Pinned on the HUD after DAILY_TOTAL, so it pairs up in the stat grid.
    const w = snapshot.hud.widgets;
    if (!w.includes("HEART_RATE")) w.splice(w.indexOf("DAILY") + 1, 0, "HEART_RATE");
    Object.assign(snapshot.heart_rate, {
      sensor: { id: "AA:BB:CC:DD:EE:01", name: "Forerunner 265" },
      status: f.bpm === null ? "waiting" : "connected",
      label: f.bpm === null ? "WAITING" : "LINKED",
      message:
        f.bpm === null
          ? "Waiting for heart rate. On a Garmin watch, start an activity or turn on Broadcast Heart Rate."
          : "Reading heart rate from the sensor.",
      bpm: f.bpm,
      history: history(f.bpm),
      zone: null,
      cool_down: mockPulseTarget(114, f.bpm, f.holdS),
    });
  },
};
