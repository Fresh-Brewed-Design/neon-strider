// Engage and the deck's status. ?engage=timeout: the deck's screen is off. It
// shows the last Engage's failure, and Engage waits 8 s and fails again.
// ?status=N: the deck sent an unknown status byte N. ?status=countdown: the
// deck's 3-2-1 before the belt moves. ?status=stopping: the belt winding down
// after Dump (Engage is refused until it stops). ?status=standby: the deck is
// off or asleep (Engage is refused at once).
import { later, params, snapshot } from "../mockState";
import type { Scenario } from "./types";

const engageTimeout = params.get("engage") === "timeout";
const START_NO_RESPONSE =
  "The treadmill didn't respond. If its screen is off, wake it with its power button or remote, then try again.";
const startFailed = (at_ms: number) => ({
  start_result: { ok: false, message: START_NO_RESPONSE, at_ms },
  last_error: { message: START_NO_RESPONSE, at_ms },
});
const statusFlag = params.get("status");
const START_SPINNING_DOWN = "Belt is still stopping. Press Engage once it stops.";
const START_STANDBY = "Wake the deck first at the treadmill or remote.";

/** ?status=stopping: Engage while the belt spins down is refused at once. */
export function refuseWhileStopping() {
  const message = statusFlag === "standby" ? START_STANDBY : START_SPINNING_DOWN;
  snapshot.connection.start_result = { ok: false, message, at_ms: Date.now() };
}

/** ?engage=timeout: Engage on a stopped deck waits, then reports no answer. */
function engageWithTimeout() {
  if (snapshot.deck.status === "Running") return;
  Object.assign(snapshot.connection, { start_pending: true, start_result: null });
  later(8000, () => Object.assign(snapshot.connection, { start_pending: false }, startFailed(Date.now())));
}

export const engage: Scenario = {
  apply() {
    if (engageTimeout) {
      Object.assign(snapshot.deck, { status: "Stopped", speed_kmh: 0, raw_speed_kmh: 0 });
      snapshot.active_program = null;
      Object.assign(snapshot.connection, startFailed(Date.now() - 2000));
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
    ...(engageTimeout ? { start: engageWithTimeout, toggle_start: engageWithTimeout } : {}),
    ...(statusFlag === "stopping" || statusFlag === "standby" ? { start: refuseWhileStopping, toggle_start: refuseWhileStopping } : {}),
  },
};
