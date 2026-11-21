// PULSE_RATE's heart rate zone (#125). ?hrzone=under|in|over: a watch reading
// below, inside or above a 133 to 152 bpm zone step (70 to 80 % of 190);
// =wait the zone step with the watch picked but no reading yet.
import type { ZoneBand } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Rust's `heart_rate::zone_band`, for the preview: same rule, same words. */
export function mockZone(low: number, high: number, bpm: number | null): ZoneBand {
  const position = bpm === null ? null : bpm > high ? "over" : bpm < low ? "under" : "in";
  const text = {
    under: ["ZONE // UNDER", "Heart rate is below the zone."],
    in: ["ZONE // LOCKED", "Heart rate is in the zone."],
    over: ["ZONE // OVER", "Heart rate is above the zone."],
  } as const;
  const [label, message] = position ? text[position] : ["ZONE // WAIT", "Waiting for a heart rate reading."];
  return { low_bpm: Math.ceil(low), high_bpm: Math.floor(high), position, label, message };
}

const BPM: Record<string, number | null> = { under: 124, in: 143, over: 161, wait: null };

/** Ten minutes at one point per 5 s, easing from 110 bpm to `end`. */
function history(end: number | null): (number | null)[] {
  if (end === null) return Array.from({ length: 120 }, () => null);
  return Array.from({ length: 120 }, (_, i) => {
    const rise = Math.min(Math.max(i - 30, 0), 60) / 60;
    return Math.round(110 + rise * (end - 110) + Math.sin(i * 0.35) * 1.5);
  });
}

export const hrZone: Scenario = {
  apply() {
    const flag = params.get("hrzone") ?? "";
    if (!(flag in BPM)) return;
    const bpm = BPM[flag];
    // Pinned on the HUD after DAILY_TOTAL, so it pairs up in the stat grid.
    const w = snapshot.hud.widgets;
    if (!w.includes("HEART_RATE")) w.splice(w.indexOf("DAILY") + 1, 0, "HEART_RATE");
    Object.assign(snapshot.heart_rate, {
      sensor: { id: "AA:BB:CC:DD:EE:01", name: "Forerunner 265" },
      status: bpm === null ? "waiting" : "connected",
      label: bpm === null ? "WAITING" : "LINKED",
      message:
        bpm === null
          ? "Waiting for heart rate. On a Garmin watch, start an activity or turn on Broadcast Heart Rate."
          : "Reading heart rate from the sensor.",
      bpm,
      history: history(bpm),
      zone: mockZone(133, 152, bpm),
    });
  },
};
