// ?lightsout: LIGHTS_OUT switched on (System -> Desk).
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const lightsOut: Scenario = {
  apply() {
    if (params.has("lightsout")) snapshot.lights_out.enabled = true;
  },
  commands: {
    lights_out_set_config: (c) => (snapshot.lights_out.enabled = c.enabled),
  },
};
