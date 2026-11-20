// ?neural=active: Neural flow is easing the belt off while you type.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const neural: Scenario = {
  apply() {
    if (params.get("neural") === "active") {
      Object.assign(snapshot.neural, { active: true, factor: 0.94, base_kmh: 3.5, target_kmh: 3.3, keys_per_min: 118 });
      Object.assign(snapshot.deck, { speed_kmh: 3.3, target_speed_kmh: 3.3 });
    }
  },
};
