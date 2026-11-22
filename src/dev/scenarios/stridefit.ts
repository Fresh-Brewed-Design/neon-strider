// STRIDE_FIT (#223). ?stridefit=4.5: the fastest speed the stride still fits
// the deck set to 4.5 km/h in System, Calibration, so the program editor flags
// Brisk's 5.0 km/h step and Interval walk's 6.0 km/h steps (Post-meal's 4.0
// stays unflagged). The belt runs at 5.0 km/h, so the Live page's speed
// slider shows the FIT mark below the thumb and the HUD shows its
// ABOVE STRIDE_FIT line (#226).
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Rust's `stride_fit::in_deck_range`: held to the speed range, in tenths. */
function inDeckRange(kmh: number): number {
  const { min_speed_kmh: min, max_speed_kmh: max } = snapshot.deck;
  return Math.round(Math.min(Math.max(kmh, min), max) * 10) / 10;
}

/** Rust's `stride_fit::above`: running, and the lower of reading and target above the fit, in tenths. */
function above(): boolean {
  const { status, speed_kmh, target_speed_kmh, stride_fit_kmh: fit } = snapshot.deck;
  if (fit === null || status !== "Running") return false;
  return Math.round(Math.min(speed_kmh, target_speed_kmh) * 10) > Math.round(fit * 10);
}

/** The belt speed the scenario runs at, faster than its 4.5 km/h fit. */
const BELT_KMH = 5;

export const strideFit: Scenario = {
  apply() {
    const flag = params.get("stridefit");
    if (flag === null) return;
    const kmh = Number(flag || "4.5");
    if (Number.isFinite(kmh) && kmh > 0) snapshot.deck.stride_fit_kmh = inDeckRange(kmh);
    if (snapshot.deck.status === "Running") {
      snapshot.deck.speed_kmh = BELT_KMH;
      snapshot.deck.target_speed_kmh = BELT_KMH;
    }
    snapshot.deck.above_stride_fit = above();
  },
  tick() {
    const next = above();
    if (next === snapshot.deck.above_stride_fit) return false;
    snapshot.deck.above_stride_fit = next;
    return true;
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
