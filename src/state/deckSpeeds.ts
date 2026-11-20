import type { DeckSnapshot, DeckSpeedStep } from "../ipc";

/** Below this a speed is the still belt, which is never snapped. */
const STILL_KMH = 0.05;

/**
 * `v` moved to the nearest speed the deck runs, from `deck.speed_steps` (Rust's
 * DECK_PROFILE). The slower one wins a tie. Unchanged while the list is empty,
 * that is before the deck has answered a target, and for a still belt.
 *
 * `unit` picks the step's calibrated km/h (every speed field) or the deck's
 * own km/h (the top speed setting). The list ends at the top speed setting
 * unless the deck's ceiling is known: above it a deck-unit value is left as
 * typed, so the top speed can still be raised.
 */
export function snapToDeck(
  v: number,
  deck: Pick<DeckSnapshot, "speed_steps" | "profile"> | null | undefined,
  unit: keyof DeckSpeedStep = "kmh",
): number {
  const steps = deck?.speed_steps ?? [];
  if (steps.length === 0 || !Number.isFinite(v) || v < STILL_KMH) return v;
  const top = steps[steps.length - 1][unit];
  if (unit === "hw_kmh" && v > top && deck?.profile.ceiling_kmh == null) return v;
  let best = steps[0][unit];
  for (const s of steps) {
    if (Math.abs(s[unit] - v) < Math.abs(best - v) - 1e-6) best = s[unit];
  }
  return best;
}

/** A snapped speed for a field: to 0.01 km/h, so a calibrated speed stays readable. */
export function roundSpeed(v: number): number {
  return Math.round(v * 100) / 100;
}
