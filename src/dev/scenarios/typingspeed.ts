// Labs: typing by speed. ?typingspeed shows the switch on with a table of
// five speed bands from 12 walks; =empty the switch on with no walks counted
// yet. Switching it on in the preview with neither flag shows the empty state.
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

const BANDS = [
  { from_kmh: 2.0, to_kmh: 2.5, minutes: 64, median_kpm: 182 },
  { from_kmh: 2.5, to_kmh: 3.0, minutes: 141, median_kpm: 176 },
  { from_kmh: 3.0, to_kmh: 3.5, minutes: 97, median_kpm: 158 },
  { from_kmh: 3.5, to_kmh: 4.0, minutes: 38, median_kpm: 121 },
  { from_kmh: 4.0, to_kmh: 4.5, minutes: 9, median_kpm: 74 },
];

export const typingSpeed: Scenario = {
  apply() {
    const flag = params.get("typingspeed");
    if (flag === null) return;
    snapshot.typing_speed.enabled = true;
    if (flag !== "empty") {
      Object.assign(snapshot.typing_speed, { bands: BANDS, walks: 12, typing_minutes: 362 });
    }
  },
  commands: {
    typing_speed_set: (c) => {
      snapshot.typing_speed.enabled = c.on;
    },
  },
};
