// FOOTFALL (Labs, in System -> Diagnostics): the deck's own step count as
// walk-away presence. ?footfall: switched on with the count moving about once
// a second; =frozen the count still for 38 s with the belt running; =none the
// deck sends 0 the whole walk; =off switched off with the count moving.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const footfall: Scenario = {
  apply() {
    const flag = params.get("footfall");
    if (flag === null) return;
    const w = snapshot.walkaway;
    w.steps_presence = flag !== "off";
    if (flag === "none") {
      w.deck_steps = { count: 0, moves: 0, live: false, still_s: 212 };
    } else if (flag === "frozen") {
      w.deck_steps = { count: 2318, moves: 2318, live: true, still_s: 38 };
    } else {
      w.deck_steps = { count: 2318, moves: 2318, live: true, still_s: 0 };
    }
  },
  /** One second of the walk: a moving count moves, a frozen one ages. */
  tick() {
    const flag = params.get("footfall");
    const r = snapshot.walkaway.deck_steps;
    if (flag === null || r.count === null || r.still_s === null) return false;
    if (flag === "frozen" || flag === "none") {
      r.still_s += 1;
    } else {
      r.count += 1;
      r.moves += 1;
    }
    return true;
  },
  commands: {
    walkaway_steps_set: (c) => {
      snapshot.walkaway.steps_presence = c.on;
    },
  },
};
