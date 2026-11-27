// ?call: on a call, the belt held at the quiet speed (the HUD preview also shows the toast).
// ?quietnow: Quiet now holds the belt at the 2.0 km/h quiet speed, a second
// press puts 3.5 km/h back; `=slow` the belt already at 1.8 km/h, so a press
// changes nothing and the HUD says why.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** The deck walking at `kmh`, as the mock shows it. */
function walkAt(kmh: number) {
  Object.assign(snapshot.deck, { speed_kmh: kmh, raw_speed_kmh: kmh * 1.1, target_speed_kmh: kmh });
}

export const calls: Scenario = {
  apply() {
    if (params.has("call")) {
      // The belt walked at 3.5 km/h before the call.
      Object.assign(snapshot.calls, { mic_in_use: true, quiet_active: true, call_back_kmh: snapshot.deck.target_speed_kmh });
      Object.assign(snapshot.deck, { speed_kmh: 2, raw_speed_kmh: 2.2, target_speed_kmh: 2 });
    }
    if (params.has("quietnow")) {
      snapshot.deck.status = "Running";
      if (params.get("quietnow") === "slow") {
        walkAt(1.8);
      } else {
        Object.assign(snapshot.calls, { quiet_now: true, quiet_now_back_kmh: 3.5 });
        walkAt(snapshot.calls.quiet_kmh);
      }
    }
  },
  commands: {
    calls_set_config: (c) => Object.assign(snapshot.calls, { enabled: c.enabled, quiet_kmh: c.quiet_kmh }),
    // Rust's rules (`CallMachine::press_quiet_now`), short: only a running
    // belt, only ever slower, and the second press puts the speed back.
    quiet_now: () => {
      const { calls: c, deck } = snapshot;
      // During a call that holds the belt quiet, a press takes the hold over: nothing moves.
      if (c.quiet_now_back_kmh === null && c.quiet_active && c.call_back_kmh !== null) {
        Object.assign(c, { quiet_now: true, quiet_now_back_kmh: c.call_back_kmh, quiet_active: false, call_back_kmh: null });
        return;
      }
      if (c.quiet_now_back_kmh !== null) {
        if (deck.status !== "Running") throw "Quiet now puts the speed back once the belt runs.";
        walkAt(c.quiet_now_back_kmh);
        Object.assign(c, { quiet_now: false, quiet_now_back_kmh: null });
        return;
      }
      if (deck.status !== "Running") throw "Quiet now works on a running belt.";
      const quiet = Math.max(c.quiet_kmh, deck.min_speed_kmh);
      if (deck.target_speed_kmh <= quiet + 0.05) throw `The belt already runs at ${quiet.toFixed(1)} km/h or slower.`;
      Object.assign(c, { quiet_now: true, quiet_now_back_kmh: deck.target_speed_kmh });
      walkAt(quiet);
    },
  },
};
