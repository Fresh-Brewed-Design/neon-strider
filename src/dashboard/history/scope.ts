import type { Sample } from "../../services/db/schema";

export interface ScopePoint {
  timestamp: number;
  speedKmh: number;
  inclinePercent: number;
  /** Beats per minute saved with that second; null without a reading. */
  heartRate: number | null;
}

/** Samples of a (possibly merged) session, one per timestamp, oldest first. */
export function scopePoints(samples: Sample[]): ScopePoint[] {
  const byTime = new Map<number, Sample>();
  for (const s of samples) byTime.set(s.timestamp, s);
  return [...byTime.values()]
    .sort((a, b) => a.timestamp - b.timestamp)
    .map((s) => ({
      timestamp: s.timestamp,
      speedKmh: s.speed || 0,
      inclinePercent: s.incline || 0,
      heartRate: s.heart_rate !== null && s.heart_rate > 0 ? s.heart_rate : null,
    }));
}

export interface HeartPoint {
  timestamp: number;
  bpm: number;
}

/**
 * The heart rate as runs of back-to-back readings. A second without a reading
 * ends a run, so the chart leaves a gap there instead of joining across it.
 * Empty when the walk has no heart rate at all.
 */
export function heartRuns(points: ScopePoint[]): HeartPoint[][] {
  const runs: HeartPoint[][] = [];
  let run: HeartPoint[] = [];
  for (const p of points) {
    if (p.heartRate === null) {
      if (run.length > 0) runs.push(run);
      run = [];
    } else {
      run.push({ timestamp: p.timestamp, bpm: p.heartRate });
    }
  }
  if (run.length > 0) runs.push(run);
  return runs;
}

/** The heart rate axis: the readings with some room, on round tens, at least 20 bpm tall. */
export function heartAxis(runs: HeartPoint[][]): { min: number; max: number } | null {
  const all = runs.flat().map((p) => p.bpm);
  if (all.length === 0) return null;
  let min = Math.floor((Math.min(...all) - 5) / 10) * 10;
  let max = Math.ceil((Math.max(...all) + 5) / 10) * 10;
  if (max - min < 20) {
    const mid = (min + max) / 2;
    min = Math.floor((mid - 10) / 10) * 10;
    max = min + 20;
  }
  return { min: Math.max(0, min), max };
}
