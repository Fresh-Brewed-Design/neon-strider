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

/** The most speeds the list holds, so the rows stay a list and not a wall. */
export const MAX_TARGETS = 12;

/** Speeds sorted, rounded to 0.1 km/h and without repeats. */
function tidy(speeds: number[]): number[] {
  const out: number[] = [];
  for (const s of [...speeds].sort((a, b) => a - b)) {
    const v = Math.round(s * 10) / 10;
    if (!out.some((o) => Math.abs(o - v) <= SAME_SPEED_KMH)) out.push(v);
  }
  return out;
}

/** The saved list of target speeds; anything unreadable falls back to the defaults. */
export function parseTargets(raw: string | null, defaults: number[], minKmh: number, maxKmh: number): number[] {
  try {
    const parsed: unknown = raw === null ? null : JSON.parse(raw);
    if (Array.isArray(parsed)) {
      const speeds = parsed.filter((v): v is number => typeof v === "number" && v >= minKmh && v <= maxKmh);
      return tidy(speeds).slice(0, MAX_TARGETS);
    }
  } catch {
    // A damaged value is the same as none.
  }
  return tidy(defaults);
}

/** Why a speed can't join the list, or null when it can. */
export function targetProblem(targets: number[], kmh: number, minKmh: number, maxKmh: number): string | null {
  if (!Number.isFinite(kmh)) return "Enter a speed in km/h.";
  if (kmh < minKmh || kmh > maxKmh) return `Pick a speed from ${minKmh.toFixed(1)} to ${maxKmh.toFixed(1)} km/h.`;
  if (targets.some((t) => Math.abs(t - kmh) <= SAME_SPEED_KMH)) return "That speed is already in the list.";
  if (targets.length >= MAX_TARGETS) return `The list holds at most ${MAX_TARGETS} speeds. Remove one first.`;
  return null;
}

export const addTarget = (targets: number[], kmh: number): number[] => tidy([...targets, kmh]);
export const removeTarget = (targets: number[], kmh: number): number[] =>
  targets.filter((t) => Math.abs(t - kmh) > SAME_SPEED_KMH);
