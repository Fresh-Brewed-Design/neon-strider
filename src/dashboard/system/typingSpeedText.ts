import type { TypingBand } from "../../ipc/bindings/TypingBand";
import type { TypingSpeedSnapshot } from "../../ipc/bindings/TypingSpeedSnapshot";

/** A speed band's label, both ends in km/h. */
export function bandLabel(band: TypingBand): string {
  return `${band.from_kmh.toFixed(1)} to ${band.to_kmh.toFixed(1)} km/h`;
}

/** Each band's median as a share of the highest one, for its bar (0 to 1). */
export function barShares(bands: TypingBand[]): number[] {
  const top = Math.max(0, ...bands.map((b) => b.median_kpm));
  return bands.map((b) => (top > 0 ? b.median_kpm / top : 0));
}

/** The lines under the table, or the empty state's lines when no band shows. */
export function typingSpeedLines(t: TypingSpeedSnapshot): string[] {
  const walks = t.walks === 1 ? "1 walk" : `${t.walks} walks`;
  const minLine = `A speed shows once it has ${t.min_minutes} minutes with typing.`;
  const skipLine = "Minutes without a key press are left out.";
  if (t.walks === 0) {
    return ["No walks with key counts saved yet.", minLine];
  }
  if (t.bands.length === 0) {
    return [`No speed has ${t.min_minutes} minutes with typing yet.`, `Key counts so far come from ${walks}.`, skipLine];
  }
  return [`Medians over your last ${walks} with key counts.`, skipLine];
}
