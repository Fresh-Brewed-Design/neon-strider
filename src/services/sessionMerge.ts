import type { Session } from "./db/schema";

/** A run of adjacent sessions folded into one. `mergedIds` are the DB ids, oldest first. */
export type MergedSession = Session & { mergedIds: number[] };

const MINUTE_MS = 60_000;

/** The higher of two optional values; null when both are. */
function maxOf(a: number | null, b: number | null): number | null {
  if (a === null) return b;
  if (b === null) return a;
  return Math.max(a, b);
}

/**
 * Average heart rate of two joined walks, weighted by duration. A walk without
 * heart rate doesn't count, so joining one keeps the other's average.
 */
function mergedAvgHeartRate(a: Session, b: Session): number | null {
  const parts = [a, b].filter((s) => s.avg_heart_rate !== null);
  if (parts.length === 0) return null;
  const seconds = parts.reduce((sum, s) => sum + s.duration_seconds, 0);
  if (seconds <= 0) return parts[0].avg_heart_rate;
  return Math.round(parts.reduce((sum, s) => sum + (s.avg_heart_rate ?? 0) * s.duration_seconds, 0) / seconds);
}

/**
 * Fold sessions that start within `thresholdMinutes` of the previous session's
 * end into one. Rust applies the same rule when it groups walks for Strava.
 *
 * Counters are summed, averages are recomputed, and `strava_id` is kept if any
 * member has one. Input is not mutated. Output is oldest first.
 */
export function mergeSessions(sessions: readonly Session[], thresholdMinutes: number): MergedSession[] {
  const thresholdMs = thresholdMinutes * MINUTE_MS;
  const merged: MergedSession[] = [];

  const ordered = [...sessions].sort((a, b) => a.start_time - b.start_time);
  for (const curr of ordered) {
    const ids = curr.id === undefined ? [] : [curr.id];
    const last = merged[merged.length - 1];

    if (last && curr.start_time - last.end_time < thresholdMs) {
      last.avg_heart_rate = mergedAvgHeartRate(last, curr);
      last.max_heart_rate = maxOf(last.max_heart_rate, curr.max_heart_rate);
      const totalSeconds = last.duration_seconds + curr.duration_seconds;
      const hours = totalSeconds / 3600;
      last.avg_incline =
        totalSeconds > 0
          ? (last.avg_incline * last.duration_seconds + curr.avg_incline * curr.duration_seconds) / totalSeconds
          : 0;
      last.end_time = curr.end_time;
      last.duration_seconds = totalSeconds;
      last.distance_meters += curr.distance_meters;
      last.raw_distance_meters += curr.raw_distance_meters;
      last.calories += curr.calories;
      last.steps += curr.steps;
      last.raw_steps += curr.raw_steps;
      last.neural_heat += curr.neural_heat;
      last.neural_ticks += curr.neural_ticks;
      last.strava_id = last.strava_id || curr.strava_id;
      last.mergedIds.push(...ids);
      last.avg_speed = hours > 0 ? last.distance_meters / 1000 / hours : 0;
      last.raw_avg_speed = hours > 0 ? last.raw_distance_meters / 1000 / hours : 0;
    } else {
      merged.push({ ...curr, mergedIds: ids });
    }
  }
  return merged;
}
