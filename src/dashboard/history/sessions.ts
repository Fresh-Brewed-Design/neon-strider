// Narrowing, totalling and grouping the History list. Pure, so it is tested
// without the page.

import type { MergedSession } from "../../services/sessionMerge";
import { dayKey, formatDayHeading } from "../../format";

export type Range = "7d" | "30d" | "1y" | "all";

export const RANGES: { id: Range; label: string; days: number | null }[] = [
  { id: "7d", label: "7 days", days: 7 },
  { id: "30d", label: "30 days", days: 30 },
  { id: "1y", label: "Year", days: 365 },
  { id: "all", label: "All", days: null },
];

/** The sessions on one day (`day` as YYYY-MM-DD), or else in the range ending today. */
export function visibleSessions(
  sessions: MergedSession[],
  range: Range,
  day: string | null,
  now = new Date(),
): MergedSession[] {
  if (day) return sessions.filter((s) => dayKey(s.start_time) === day);
  const days = RANGES.find((r) => r.id === range)?.days ?? null;
  if (days === null) return sessions;
  const from = new Date(now);
  from.setHours(0, 0, 0, 0);
  from.setDate(from.getDate() - (days - 1));
  return sessions.filter((s) => s.start_time >= from.getTime());
}

export function totals(sessions: MergedSession[]): { meters: number; seconds: number; kcal: number } {
  return sessions.reduce(
    (t, s) => ({ meters: t.meters + s.distance_meters, seconds: t.seconds + s.duration_seconds, kcal: t.kcal + s.calories }),
    { meters: 0, seconds: 0, kcal: 0 },
  );
}

export function metersByDay(sessions: MergedSession[]): Map<string, number> {
  const map = new Map<string, number>();
  for (const s of sessions) {
    const k = dayKey(s.start_time);
    map.set(k, (map.get(k) ?? 0) + s.distance_meters);
  }
  return map;
}

/** Consecutive sessions on the same local day, in the order given. */
export function groupByDay(sessions: MergedSession[]): { key: string; heading: string; items: MergedSession[] }[] {
  const out: { key: string; heading: string; items: MergedSession[] }[] = [];
  for (const s of sessions) {
    const key = dayKey(s.start_time);
    const last = out[out.length - 1];
    if (last && last.key === key) last.items.push(s);
    else out.push({ key, heading: formatDayHeading(s.start_time), items: [s] });
  }
  return out;
}
