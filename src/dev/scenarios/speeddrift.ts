// ?speeddrift=drifting|held|waiting|outside|ended: speed drift on (3.2 to
// 3.8 km/h, every 5 minutes) on a manual walk: setting the belt at 3.6 km/h
// (the HUD shows SPEED DRIFT), holding while Neural flow eases off, counting
// to its first change, waiting for a speed inside the band, or ended by a
// speed set by hand. `?speeddrift` alone is the switch on with the belt still.
import { SETTING_LIMITS } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

const L = SETTING_LIMITS;
const BAND = "3.2 to 3.8 km/h";

/** Rust's sentence for each state the flag shows. */
const MESSAGES: Record<string, string> = {
  idle: "Speed drift waits for a manual walk with the belt running.",
  drifting: `Speed drift has the belt at 3.6 km/h, within ${BAND}.`,
  held: "Speed drift holds its speed while Neural flow eases the belt off.",
  waiting: "Speed drift makes its first change in 3 min.",
  outside: `Speed drift starts once the belt runs at a speed you set from ${BAND}.`,
  ended: "A speed set by hand ended speed drift until the next walk.",
};

export const speedDrift: Scenario = {
  apply() {
    if (!params.has("speeddrift")) return;
    const which = params.get("speeddrift") || "idle";
    const state = which in MESSAGES ? which : "idle";
    const d = snapshot.speed_drift;
    Object.assign(d, {
      enabled: true,
      state,
      message: MESSAGES[state],
      speed_kmh: state === "drifting" || state === "held" ? 3.6 : null,
      starts_in_s: state === "waiting" ? 170 : null,
    });
    if (state === "idle") {
      Object.assign(snapshot.deck, { status: "Stopped", speed_kmh: 0, target_speed_kmh: 0 });
      return;
    }
    const speed = state === "outside" ? 4.5 : state === "ended" ? 3.0 : 3.6;
    Object.assign(snapshot.deck, { status: "Running", speed_kmh: speed, target_speed_kmh: speed });
    snapshot.active_program = null;
    if (state === "held") {
      Object.assign(snapshot.neural, { active: true, factor: 0.94, base_kmh: 3.6, target_kmh: 3.4, keys_per_min: 118 });
      Object.assign(snapshot.deck, { speed_kmh: 3.4, target_speed_kmh: 3.4 });
    }
  },
  commands: {
    speed_drift_set_config: (c) => {
      const inRange = (v: number) => v >= L.speed_drift_kmh.min && v <= L.speed_drift_kmh.max;
      if (!inRange(c.low_kmh) || !inRange(c.high_kmh)) {
        throw `Speed drift speeds must be between ${L.speed_drift_kmh.min} and ${L.speed_drift_kmh.max} km/h`;
      }
      const width = c.high_kmh - c.low_kmh;
      if (width < L.speed_drift_width_kmh.min - 1e-3 || width > L.speed_drift_width_kmh.max + 1e-3) {
        throw `The fastest speed must be ${L.speed_drift_width_kmh.min} to ${L.speed_drift_width_kmh.max} km/h above the slowest`;
      }
      const d = snapshot.speed_drift;
      Object.assign(d, {
        enabled: c.enabled,
        low_kmh: c.low_kmh,
        high_kmh: c.high_kmh,
        every_minutes: c.every_minutes,
        band_low_kmh: c.low_kmh,
        band_high_kmh: c.high_kmh,
      });
      if (!c.enabled) Object.assign(d, { state: "off", speed_kmh: null, starts_in_s: null, message: "Speed drift is off." });
      else if (d.state === "off") Object.assign(d, { state: "idle", message: MESSAGES.idle });
    },
  },
};
