// ?call: on a call, the belt held at the quiet speed (the HUD preview also shows the toast).
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const calls: Scenario = {
  apply() {
    if (params.has("call")) {
      Object.assign(snapshot.calls, { mic_in_use: true, quiet_active: true });
      Object.assign(snapshot.deck, { speed_kmh: 2, raw_speed_kmh: 2.2, target_speed_kmh: 2 });
    }
  },
  commands: {
    calls_set_config: (c) => Object.assign(snapshot.calls, { enabled: c.enabled, quiet_kmh: c.quiet_kmh }),
  },
};
