import type { Change } from "../../ipc";

export type ChangeTone = "up" | "down" | "flat" | "new";

/** Largest percentage shown as a number; beyond it the figure would crowd the HUD card. */
const MAX_SHOWN_PCT = 999;

/**
 * The comparison chip for a week-over-week change: "▲ 12%", "▼ 8%", "NEW" (the
 * week before had nothing), or null when there is nothing to compare.
 */
export function formatChange(change: Change): { text: string; tone: ChangeTone } | null {
  if (change.is_new) return { text: "NEW", tone: "new" };
  const pct = change.pct;
  if (pct === null) return null;
  if (pct === 0) return { text: "±0%", tone: "flat" };
  const shown = Math.min(Math.abs(pct), MAX_SHOWN_PCT);
  const more = Math.abs(pct) > MAX_SHOWN_PCT ? "+" : "";
  return pct > 0 ? { text: `▲ ${shown}%${more}`, tone: "up" } : { text: `▼ ${shown}%${more}`, tone: "down" };
}
