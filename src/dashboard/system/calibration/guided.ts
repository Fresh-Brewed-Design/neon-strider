// The guided calibration's list of speeds. Rust does the math (calibration.rs);
// this only lays out which speeds to offer and which saved point each row is.
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
