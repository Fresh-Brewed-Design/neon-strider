import type { DeckProfileSnapshot, DeckSnapshot, DeckSpeedStep } from "../ipc";

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

/**
 * `level` moved to the nearest incline level the deck runs, from
 * `deck.incline_steps` (Rust's DECK_PROFILE). The lower one wins a tie.
 * Unchanged while the list is empty, that is before the deck has answered an
 * incline, and above the list's top (the highest incline setting can still be
 * raised until the deck's own ceiling is known).
 */
export function snapToLevel(level: number, deck: Pick<DeckSnapshot, "incline_steps"> | null | undefined): number {
  const steps = deck?.incline_steps ?? [];
  if (steps.length === 0 || !Number.isFinite(level) || level > steps[steps.length - 1]) return level;
  let best = steps[0];
  for (const s of steps) {
    if (Math.abs(s - level) < Math.abs(best - level)) best = s;
  }
  return best;
}

/** A snapped speed for a field: to 0.01 km/h, so a calibrated speed stays readable. */
export function roundSpeed(v: number): number {
  return Math.round(v * 100) / 100;
}

/** Settings the deck's limits would replace, in the deck's own km/h. */
export interface DeckLimitOffer {
  minSpeed?: number;
  maxSpeed?: number;
}

/**
 * What DECK_PROFILE's floor and ceiling offer in place of the min and max speed
 * settings: a limit only when the setting is looser than the deck's. A setting
 * you tightened yourself, or that already matches, is left as it is. Null when
 * there is nothing to offer.
 */
export function deckLimitOffer(
  p: DeckProfileSnapshot,
  minSpeed: number,
  maxSpeed: number,
): DeckLimitOffer | null {
  const offer: DeckLimitOffer = {};
  if (p.floor_hw_kmh != null && minSpeed < p.floor_hw_kmh - 1e-6) offer.minSpeed = p.floor_hw_kmh;
  if (p.ceiling_hw_kmh != null && maxSpeed > p.ceiling_hw_kmh + 1e-6) offer.maxSpeed = p.ceiling_hw_kmh;
  return Object.keys(offer).length > 0 ? offer : null;
}

const kmh = (v: number) => `${v.toFixed(1)} km/h`;

/** One line on what DECK_PROFILE knows. */
export function profileSummary(p: DeckProfileSnapshot): string {
  if (p.learned === 0) return "No speeds learned yet.";
  const parts = [`${p.learned} ${p.learned === 1 ? "speed" : "speeds"} learned`];
  if (p.floor_kmh != null) parts.push(`slowest ${kmh(p.floor_kmh)}`);
  if (p.ceiling_kmh != null) parts.push(`fastest ${kmh(p.ceiling_kmh)}`);
  if (p.skipped_kmh.length > 0) parts.push(`skips ${p.skipped_kmh.map((v) => v.toFixed(1)).join(", ")}`);
  return `${parts.join(", ")}.`;
}

/** One line on the incline levels DECK_PROFILE knows. */
export function inclineSummary(p: DeckProfileSnapshot): string {
  if (p.learned_levels === 0) return "No incline levels learned yet.";
  const parts = [`${p.learned_levels} ${p.learned_levels === 1 ? "incline level" : "incline levels"} learned`];
  if (p.ceiling_level != null) parts.push(`top level ${p.ceiling_level}`);
  if (p.skipped_levels.length > 0) parts.push(`skips ${p.skipped_levels.join(", ")}`);
  return `${parts.join(", ")}.`;
}

/**
 * The deck's seen limits as a range, e.g. "0.6 to 4.0 km/h", in the deck's own
 * km/h: the unit of the Slowest speed and Top speed fields the offer fills.
 */
export function limitsText(p: DeckProfileSnapshot): string {
  const lo = p.floor_hw_kmh;
  const hi = p.ceiling_hw_kmh;
  if (lo != null && hi != null) return `${lo.toFixed(1)} to ${kmh(hi)}`;
  if (lo != null) return `no slower than ${kmh(lo)}`;
  if (hi != null) return `no faster than ${kmh(hi)}`;
  return "within its own limits";
}
