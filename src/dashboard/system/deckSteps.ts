import type { DeckStepsReading } from "../../ipc";

/**
 * A count that moved this many seconds ago or less reads as moving. The deck
 * reports about twice a second and a walker steps about once a second, so a
 * walking count is never older than this; a longer gap shows its seconds.
 */
const MOVING_WITHIN_S = 3;

/** The Diagnostics line for the deck's own step counter (FOOTFALL). */
export function deckStepsText(r: DeckStepsReading): string {
  if (r.count === null) return "No step count from the treadmill yet.";
  const head = `Deck step count ${r.count}`;
  if (r.still_s === null) {
    return r.moves > 0 ? `${head}, moved ${r.moves} times while walking.` : `${head}, not moved while walking.`;
  }
  if (r.still_s <= MOVING_WITHIN_S && r.moves > 0) return `${head}, moving.`;
  return `${head}, still for ${r.still_s} s.`;
}

