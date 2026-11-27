// ?walkaway=warning: the idle warning counting down (STAY cancels it, reaching 0
// pauses). ?walkaway=paused (or =lock): the belt was paused because the PC was
// locked; ?walkaway=idle: paused after no input. ?walkaway=steps: the warning
// after the deck's step count stopped (FOOTFALL on); =stepspaused the pause.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

const endWalkaway = { card: "none", reason: null } as const;

export const walkaway: Scenario = {
  apply() {
    const walkawayFlag = params.get("walkaway");
    if (walkawayFlag === "warning" || walkawayFlag === "steps") {
      Object.assign(snapshot.walkaway, { card: "warning", pause_in_s: 30, reason: walkawayFlag === "steps" ? "steps" : "idle" });
    } else if (walkawayFlag === "stepspaused") {
      Object.assign(snapshot.walkaway, { card: "paused", reason: "steps" });
      snapshot.deck.status = "Paused";
    } else if (walkawayFlag === "paused" || walkawayFlag === "lock" || walkawayFlag === "idle") {
      Object.assign(snapshot.walkaway, { card: "paused", reason: walkawayFlag === "idle" ? "idle" : "lock" });
      snapshot.deck.status = "Paused";
    }
  },
  /** One second of the walk-away warning (Rust recomputes `pause_in_s` each snapshot). */
  tick() {
    const w = snapshot.walkaway;
    if (w.card !== "warning") return false;
    if (w.pause_in_s > 1) {
      w.pause_in_s -= 1;
    } else {
      Object.assign(w, { card: "paused", reason: w.reason ?? "idle", pause_in_s: 0 });
      snapshot.deck.status = "Paused";
    }
    return true;
  },
  commands: {
    walkaway_set_config: (c) => {
      if (c.idle_minutes < 2 || c.idle_minutes > 30) throw "Idle time must be between 2 and 30 minutes";
      Object.assign(snapshot.walkaway, { lock_enabled: c.lock_enabled, idle_enabled: c.idle_enabled, idle_minutes: c.idle_minutes });
      if (!c.idle_enabled && snapshot.walkaway.card === "warning") snapshot.walkaway.card = "none";
    },
    walkaway_stay: () => Object.assign(snapshot.walkaway, { card: "none", pause_in_s: 0 }),
    walkaway_resume: () => {
      if (!snapshot.connection.connected) throw "The treadmill is offline";
      Object.assign(snapshot.walkaway, endWalkaway);
      snapshot.deck.status = "Running";
    },
    walkaway_end: () => {
      Object.assign(snapshot.walkaway, endWalkaway);
      snapshot.deck.status = "Stopped";
      snapshot.active_program = null;
    },
  },
};
