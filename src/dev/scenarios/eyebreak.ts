// ?eyebreak=upcoming|break|resume shows the eye-break (OCULAR_IMPLANT) HUD card: the
// heads-up, the cooldown with the belt it paused, or the resume card after it
// (RESUME / STAY STILL). ?eyebreak=still is the cooldown with the belt left alone,
// ?eyebreak=rest the heads-up for a break set for the program's rest, ?eyebreak=moved
// one moved earlier to fit the rest the program is in (with ?program=resting), ?eyebreak=waiting a break due
// mid-walk that waits for the belt to stop (DEFERRED, 8:12 before the pause), and
// ?eyebreak=silent the cooldown during a call (no tones). During the cooldown,
// ?blink=squeeze|open picks the guided blink step (CLOSE by default).
import { params, snapshot } from "../mockState";
import { setProgramPaused } from "./programs";
import type { Scenario } from "./types";

export const eyeBreak: Scenario = {
  apply() {
    const eyeBreakFlag = params.get("eyebreak");
    const blinkFlag = params.get("blink");
    if (eyeBreakFlag === "upcoming" || eyeBreakFlag === "rest" || eyeBreakFlag === "moved")
      Object.assign(snapshot.eye_break, {
        enabled: true,
        phase: "upcoming",
        remaining_s: 48,
        at_rest: eyeBreakFlag !== "upcoming",
        moved_to_rest: eyeBreakFlag === "moved",
      });
    if (eyeBreakFlag === "waiting") Object.assign(snapshot.eye_break, { enabled: true, phase: "waiting", remaining_s: 492 });
    if (eyeBreakFlag === "break" || eyeBreakFlag === "still" || eyeBreakFlag === "silent")
      Object.assign(snapshot.eye_break, {
        enabled: true,
        phase: "break",
        remaining_s: 72,
        holds_belt: eyeBreakFlag === "break",
        silent: eyeBreakFlag === "silent",
        blink: blinkFlag === "squeeze" || blinkFlag === "open" ? blinkFlag : "close",
      });
    if (eyeBreakFlag === "silent") snapshot.calls.mic_in_use = true;
    if (eyeBreakFlag === "resume") Object.assign(snapshot.eye_break, { enabled: true, phase: "complete", holds_belt: true });
    if (eyeBreakFlag === "break" || eyeBreakFlag === "resume") {
      // The break paused the walk: the program at its step, or the treadmill's pause.
      if (snapshot.active_program) Object.assign(snapshot.active_program, { paused: true, wait: null });
      Object.assign(snapshot.deck, { status: "Paused", speed_kmh: 0 });
    }
  },
  commands: {
    eye_break_set_config: (c) => {
      const { enabled, minutes, seconds, sound, pause_belt, fit_rests } = c;
      Object.assign(snapshot.eye_break, { enabled, minutes, seconds, sound, pause_belt, fit_rests });
      if (!c.enabled) Object.assign(snapshot.eye_break, { phase: "idle", remaining_s: 0, holds_belt: false });
    },
    eye_break_skip: () =>
      Object.assign(snapshot.eye_break, {
        phase: snapshot.eye_break.holds_belt ? "complete" : "idle",
        remaining_s: 0,
      }),
    eye_break_resume: () => {
      if (!snapshot.connection.connected) throw "The treadmill is offline";
      Object.assign(snapshot.eye_break, { phase: "idle", holds_belt: false });
      if (snapshot.active_program) setProgramPaused(false);
      else snapshot.deck.status = "Running";
    },
    eye_break_stay_still: () => Object.assign(snapshot.eye_break, { phase: "idle", holds_belt: false }),
  },
};
