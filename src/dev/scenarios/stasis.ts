// ?stasis=strip|hidden: "When the belt is still" set to that, with the belt
// stopped and no program, so the HUD shows the strip or no panel at once.
// ?status=stopping keeps the belt winding down; ?deck=paused leaves it paused
// (Engage reads RE-JACK).
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const stasis: Scenario = {
  apply() {
    const stasisFlag = params.get("stasis");
    if (stasisFlag === "strip" || stasisFlag === "hidden") {
      snapshot.hud.idle_mode = stasisFlag;
      const status = params.get("status") === "stopping" ? "Stopping" : params.get("deck") === "paused" ? "Paused" : "Stopped";
      Object.assign(snapshot.deck, { status, speed_kmh: 0, raw_speed_kmh: 0 });
      snapshot.active_program = null;
    }
  },
};
