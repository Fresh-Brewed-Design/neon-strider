// ?deckprofile: DECK_PROFILE has learned the deck's floor (0.6), ceiling (4.0)
// and a skipped 1.5 km/h, plus its top incline level (12) and a skipped level 7, so the Live slider, the speed fields and the
// program editor snap to it and System -> Diagnostics shows FORGET.
// ?deckprofile=sweep shows DECK_SWEEP switched on and running over the speeds,
// =inclinesweep over the incline levels, =sweepon switched on with no sweep running.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const deckProfile: Scenario = {
  apply() {
    if (!params.has("deckprofile")) return;
    const flag = params.get("deckprofile");
    const steps = [];
    for (let b = 6; b <= 40; b++) if (b !== 15) steps.push({ hw_kmh: b / 10, kmh: b / 10 });
    Object.assign(snapshot.deck, {
      speed_steps: steps,
      incline_steps: [0, 1, 2, 3, 4, 5, 6, 8, 9, 10, 11, 12],
      max_speed_kmh: 4,
      max_incline_level: 12,
      max_incline_pct: 15,
      target_speed_kmh: 3.5,
      profile: {
        learned: 9,
        floor_kmh: 0.6,
        ceiling_kmh: 4,
        floor_hw_kmh: 0.6,
        ceiling_hw_kmh: 4,
        skipped_kmh: [0.3, 1.5, 5],
        learned_levels: 6,
        ceiling_level: 12,
        skipped_levels: [7, 15],
        sweep:
          flag === "sweep"
            ? { done: 12, total: 30, kmh: 1.8, level: null }
            : flag === "inclinesweep"
              ? { done: 3, total: 10, kmh: 3.5, level: 4 }
              : null,
        sweep_enabled: flag === "sweep" || flag === "inclinesweep" || flag === "sweepon",
      },
    });
  },
};
