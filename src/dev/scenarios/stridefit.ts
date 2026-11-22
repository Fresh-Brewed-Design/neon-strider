// STRIDE_FIT (#223). ?stridefit=4.5: the fastest speed the stride still fits
// the deck set to 4.5 km/h in System, Calibration, so the program editor flags
// Brisk's 5.0 km/h step (Post-meal's 4.0 stays unflagged).
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Rust's `stride_fit::in_deck_range`: held to the speed range, in tenths. */
function inDeckRange(kmh: number): number {
  const { min_speed_kmh: min, max_speed_kmh: max } = snapshot.deck;
  return Math.round(Math.min(Math.max(kmh, min), max) * 10) / 10;
}

export const strideFit: Scenario = {
  apply() {
    const flag = params.get("stridefit");
    if (flag === null) return;
    const kmh = Number(flag || "4.5");
    if (Number.isFinite(kmh) && kmh > 0) snapshot.deck.stride_fit_kmh = inDeckRange(kmh);
  },
  commands: {
    stride_fit_set: (c) => {
      if (c.kmh === null) {
        snapshot.deck.stride_fit_kmh = null;
        return;
      }
      if (!Number.isFinite(c.kmh) || c.kmh <= 0) throw "STRIDE_FIT must be a speed above 0 km/h.";
      snapshot.deck.stride_fit_kmh = inDeckRange(c.kmh);
    },
  },
};
