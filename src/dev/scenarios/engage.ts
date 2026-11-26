// Engage and the deck's status. ?engage=timeout: the deck's screen is off. It
// shows the last Engage's failure, and Engage waits 8 s and fails again.
// ?engage=go: the same with a program waiting for GO (?program=ready unless
// another ?program= is given): the last GO's failure shows under GO, not under
// Engage, and GO waits 8 s and fails again there.
// ?status=N: the deck sent an unknown status byte N. ?status=countdown: the
// deck's 3-2-1 before the belt moves. ?status=stopping: the belt winding down
// after Dump (Engage is refused until it stops). ?status=standby: the deck is
// off or asleep (Engage is refused at once).
import type { StartPress } from "../../ipc";
import { later, params, snapshot } from "../mockState";
import { PROGRAM_SCENARIOS } from "./programs";
import type { Scenario } from "./types";

const engageFlag = params.get("engage");
const engageTimeout = engageFlag === "timeout";
const goTimeout = engageFlag === "go";
const START_NO_RESPONSE =
  "The treadmill didn't respond. If its screen is off, wake it with its power button or remote, then try again.";
const startFailed = (at_ms: number, press: StartPress = "engage") => ({
  start_press: press,
  start_result: { ok: false, message: START_NO_RESPONSE, at_ms },
  last_error: { message: START_NO_RESPONSE, at_ms },
});
const statusFlag = params.get("status");
const START_SPINNING_DOWN = "Belt is still stopping. Press Engage once it stops.";
const START_STANDBY = "Treadmill is off or asleep. Switch it on, then press Engage.";

/** ?status=stopping: Engage while the belt spins down is refused at once. */
export function refuseWhileStopping() {
  const message = statusFlag === "standby" ? START_STANDBY : START_SPINNING_DOWN;
  Object.assign(snapshot.connection, { start_press: "engage", start_result: { ok: false, message, at_ms: Date.now() } });
}

/** A start `press` made on a still deck waits, then reports no answer. */
function startWithTimeout(press: StartPress) {
  if (snapshot.deck.status === "Running") return;
  Object.assign(snapshot.connection, { start_pending: true, start_result: null, start_press: press });
  later(8000, () => Object.assign(snapshot.connection, { start_pending: false }, startFailed(Date.now(), press)));
}

/** ?engage=timeout: Engage on a stopped deck waits, then reports no answer. */
const engageWithTimeout = () => startWithTimeout("engage");

export const engage: Scenario = {
  apply() {
    if (engageTimeout) {
      Object.assign(snapshot.deck, { status: "Stopped", speed_kmh: 0, raw_speed_kmh: 0 });
      snapshot.active_program = null;
      Object.assign(snapshot.connection, startFailed(Date.now() - 2000));
    }
    if (goTimeout) {
      // A program waits for GO on a paused deck; the last GO went unanswered.
      if (snapshot.active_program && !params.has("program")) Object.assign(snapshot.active_program, PROGRAM_SCENARIOS.ready);
      Object.assign(snapshot.deck, { status: "Paused", speed_kmh: 0, raw_speed_kmh: 0, target_speed_kmh: 0 });
      Object.assign(snapshot.connection, startFailed(Date.now() - 2000, "program_go"));
    }
    if (statusFlag === "countdown") {
      Object.assign(snapshot.deck, { status: "Starting", speed_kmh: 0, raw_speed_kmh: 0 });
    } else if (statusFlag === "stopping") {
      Object.assign(snapshot.deck, { status: "Stopping", speed_kmh: 0.5, raw_speed_kmh: 0.5, target_speed_kmh: 0 });
      snapshot.active_program = null;
    } else if (statusFlag === "standby") {
      Object.assign(snapshot.deck, { status: "Stopped", standby: true, speed_kmh: 0, raw_speed_kmh: 0 });
      snapshot.active_program = null;
    } else if (statusFlag !== null) {
      snapshot.deck.status = "Stopped";
      snapshot.deck.unknown_status_code = Number(statusFlag) || 7;
    }
  },
  // These replace the belt's own Engage handlers, so this comes after `belt` in the list.
  commands: {
    ...(engageTimeout || goTimeout ? { start: engageWithTimeout, toggle_start: engageWithTimeout } : {}),
    ...(goTimeout ? { program_resume: () => startWithTimeout("program_go") } : {}),
    ...(statusFlag === "stopping" || statusFlag === "standby" ? { start: refuseWhileStopping, toggle_start: refuseWhileStopping } : {}),
  },
};
