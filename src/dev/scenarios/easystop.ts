// ?easystop=on|easing: Easy stop switched on (System -> Treadmill); `=easing`
// a manual walk easing down after DUMP, 12 s before the Stop (the HUD shows
// EASY STOP and DUMP reads "stop now").
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const easyStop: Scenario = {
  apply() {
    if (!params.has("easystop")) return;
    snapshot.easy_stop.enabled = true;
    if (params.get("easystop") !== "easing") return;
    Object.assign(snapshot.easy_stop, {
      easing: true,
      stop_in_s: 12,
      to_kmh: 0.6,
      message: "Slowing to 0.6 km/h, then stopping.",
    });
    Object.assign(snapshot.deck, { status: "Running", speed_kmh: 2.4, target_speed_kmh: 4.5 });
    snapshot.active_program = null;
  },
  commands: {
    easy_stop_set: (c) => {
      snapshot.easy_stop.enabled = c.on;
      if (!c.on) Object.assign(snapshot.easy_stop, { easing: false, stop_in_s: null, to_kmh: null, message: null });
    },
  },
};
