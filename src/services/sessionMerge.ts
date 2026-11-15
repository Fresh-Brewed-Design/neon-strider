import type { Session } from "./db/schema";

/** A run of adjacent sessions folded into one. `mergedIds` are the DB ids, oldest first. */
export type MergedSession = Session & { mergedIds: number[] };

const MINUTE_MS = 60_000;

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
