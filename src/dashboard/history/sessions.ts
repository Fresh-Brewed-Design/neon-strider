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

/** Local midnight `back` days before `now`'s day. */
function midnightDaysBack(now: Date, back: number): Date {
  const d = new Date(now);
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - back);
  return d;
}

/**
 * Start and end (epoch ms, end exclusive) of one day (`day` as YYYY-MM-DD), or
 * else of the range ending today; null is an open end. Rust totals the same span.
 */
export function rangeBounds(range: Range, day: string | null, now = new Date()): { from: number | null; to: number | null } {
  if (day) {
    const start = new Date(`${day}T00:00:00`);
    const next = new Date(start);
    next.setDate(next.getDate() + 1);
    return { from: start.getTime(), to: next.getTime() };
  }
  const days = RANGES.find((r) => r.id === range)?.days ?? null;
  return { from: days === null ? null : midnightDaysBack(now, days - 1).getTime(), to: null };
}

/** The sessions on one day (`day` as YYYY-MM-DD), or else in the range ending today. */
export function visibleSessions(
  sessions: MergedSession[],
  range: Range,
  day: string | null,
  now = new Date(),
): MergedSession[] {
  const { from, to } = rangeBounds(range, day, now);
  return sessions.filter((s) => (from === null || s.start_time >= from) && (to === null || s.start_time < to));
}

/** Local midnight on the first of the month `back` months before `now`'s. */
export function monthStart(now: Date, back = 0): number {
  return new Date(now.getFullYear(), now.getMonth() - back, 1).getTime();
}

/** The local day key of today, as `groupByDay` keys its groups. */
export function todayKey(now = Date.now()): string {
  return dayKey(now);
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
