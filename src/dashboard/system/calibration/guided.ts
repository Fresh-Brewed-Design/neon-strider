// The guided calibration's rows. Rust does the math and keeps the target list
// (calibration.rs); this only lays out which saved point each row is.
import type { CalibrationPoint } from "../../../store/types";

export interface CalibrationRow {
  /** Deck km/h the run asks for. */
  targetKmh: number;
  /** The saved point at this speed, if any. */
  point: CalibrationPoint | null;
  /** Its index in the saved list, for "repeat this point". */
  index: number | null;
}

/** A saved point this close to a target is that target's point. */
const SAME_SPEED_KMH = 0.05;

/**
 * One row per offered speed, plus one per saved point at another speed, by
 * speed. A row with a point repeats it; one without measures a new one.
 */
export function calibrationRows(points: CalibrationPoint[], targets: number[]): CalibrationRow[] {
  const rows: CalibrationRow[] = points.map((point, index) => ({ targetKmh: point.speed, point, index }));
  for (const t of targets) {
    if (!rows.some((r) => Math.abs(r.targetKmh - t) <= SAME_SPEED_KMH)) rows.push({ targetKmh: t, point: null, index: null });
  }
  return rows.sort((a, b) => a.targetKmh - b.targetKmh);
}

/** The row an open run belongs to: the one nearest its target speed. */
export function runRowFor(rows: CalibrationRow[], targetKmh: number): CalibrationRow | null {
  let best: CalibrationRow | null = null;
  for (const r of rows) {
    const d = Math.abs(r.targetKmh - targetKmh);
    if (d <= SAME_SPEED_KMH && (!best || d < Math.abs(best.targetKmh - targetKmh))) best = r;
  }
  return best;
}

/** Seconds as m:ss.t for the run's clock. */
export function chrono(seconds: number): string {
  const tenths = Math.round(Math.max(0, seconds) * 10);
  const m = Math.floor(tenths / 600);
  const rest = ((tenths - m * 600) / 10).toFixed(1).padStart(4, "0");
  return `${m}:${rest}`;
}

/** The list entry for a row's speed, if the row comes from the stored list. */
export function listedTarget<T extends { kmh: number }>(targets: T[], kmh: number): T | null {
  return targets.find((t) => Math.abs(t.kmh - kmh) <= SAME_SPEED_KMH) ?? null;
}

/** A deck within this much of the belt reads as matching it: half a percent is under the deck's own 0.1 km/h step at a walk. */
const MATCH_PCT = 0.5;

/**
 * The plain-words result of a tap run: how far the deck's display is from the
 * belt at the run's speed. `real` and `reported` are km/h.
 */
export function deckVerdict(speedKmh: number, realKmh: number, reportedKmh: number): string {
  const pct = (realKmh / reportedKmh - 1) * 100;
  const at = `At ${speedKmh.toFixed(1)} km/h`;
  if (!Number.isFinite(pct)) return `${at} the taps gave no speed.`;
  if (Math.abs(pct) < MATCH_PCT) return `${at} the deck matches the belt.`;
  const size = Math.abs(pct) < 10 ? Math.abs(pct).toFixed(1) : Math.abs(pct).toFixed(0);
  return `${at} the deck runs ${size} % ${pct < 0 ? "slow" : "fast"}.`;
}

/** The line under a tap run's counter about taps dropped against the median, if any. */
export function droppedLine(dropped: number): string | null {
  if (dropped <= 0) return null;
  return dropped === 1 ? "1 tap dropped as a miss or a double." : `${dropped} taps dropped as misses or doubles.`;
}
