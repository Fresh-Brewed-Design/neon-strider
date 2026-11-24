// History's walks, by date range: the newest months listed, older months on
// demand, and totals and the calendar from Rust's aggregates over every walk.

import { useEffect, useMemo, useRef, useState } from "react";
import { useAction } from "../../components/useAction";
import type { HistoryStats } from "../../ipc";
import { SessionRepository } from "../../services/db/repositories/sessionRepository";
import type { Session } from "../../services/db/schema";
import { monthStart, rangeBounds, type Range } from "./sessions";

/**
 * Months of walks listed when History opens: this one and the two before, so
 * the 7 and 30 day lists are always whole and the list stays quick at any
 * archive size. Older months load on demand.
 */
export const FIRST_MONTHS = 3;

/** Days the calendar reaches back: 53 weeks, enough for every cell up to this Saturday. */
const CALENDAR_DAYS = 53 * 7;

/** `rows` with the rows of `more` it lacks, newest first. */
function addRows(rows: Session[], more: Session[]): Session[] {
  const have = new Set(rows.map((r) => r.id));
  return [...rows, ...more.filter((r) => !have.has(r.id))].sort((a, b) => b.start_time - a.start_time);
}

/** "March 2026". */
export function monthName(ms: number): string {
  return new Date(ms).toLocaleDateString([], { month: "long", year: "numeric" });
}

/** `reload` counts refreshes (a saved, deleted or rejoined walk); any change reads the listed window again. */
export function useHistoryWalks(range: Range, day: string | null, reload: unknown) {
  const [raw, setRaw] = useState<Session[] | null>(null);
  // Start of the listed window; the oldest row may start a little earlier when a joined walk crosses it.
  const [loadedFrom, setLoadedFrom] = useState(() => monthStart(new Date(), FIRST_MONTHS - 1));
  const fromRef = useRef(loadedFrom);
  useEffect(() => {
    fromRef.current = loadedFrom;
  }, [loadedFrom]);
  const [stats, setStats] = useState<HistoryStats | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [dayLoading, setDayLoading] = useState(false);
  const older = useAction();

  useEffect(() => {
    let alive = true;
    SessionRepository.getSessionsSince(fromRef.current)
      .then((rows) => {
        if (!alive) return;
        setRaw(rows);
        setLoadError(null);
      })
      .catch((e) => {
        console.error("[history] load failed", e);
        if (alive) setLoadError(String(e));
      });
    return () => {
      alive = false;
    };
  }, [reload]);

  const bounds = useMemo(() => rangeBounds(range, day), [range, day]);

  useEffect(() => {
    let alive = true;
    const calendarFrom = new Date();
    calendarFrom.setHours(0, 0, 0, 0);
    calendarFrom.setDate(calendarFrom.getDate() - CALENDAR_DAYS);
    SessionRepository.getStats(bounds.from, bounds.to, calendarFrom.getTime())
      .then((s) => {
        if (alive) setStats(s);
      })
      .catch((e) => {
        console.error("[history] totals failed", e);
        if (alive) setLoadError(String(e));
      });
    return () => {
      alive = false;
    };
  }, [bounds, reload]);

  // A calendar day before the listed window lists the months up to it.
  useEffect(() => {
    if (!day || bounds.from === null || bounds.from >= fromRef.current) return;
    let alive = true;
    const from = monthStart(new Date(bounds.from));
    setDayLoading(true);
    SessionRepository.getSessionsSince(from, fromRef.current)
      .then((rows) => {
        if (!alive) return;
        setRaw((have) => addRows(have ?? [], rows));
        setLoadedFrom(from);
      })
      .catch((e) => {
        console.error("[history] day load failed", e);
        if (alive) setLoadError(String(e));
      })
      .finally(() => {
        if (alive) setDayLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [day, bounds]);

  const oldestListed = raw?.length ? Math.min(loadedFrom, raw[raw.length - 1].start_time) : loadedFrom;
  // Walks older than the list exist, and the time frame reaches back past the list.
  const olderToLoad =
    !day &&
    stats?.oldest_start_ms != null &&
    stats.oldest_start_ms < oldestListed &&
    (bounds.from === null || bounds.from < loadedFrom);

  const loadOlder = () =>
    void older.run(
      async () => {
        const page = await SessionRepository.getOlderSessions(oldestListed);
        const from = page.fromMs;
        if (from !== null) {
          setRaw((have) => addRows(have ?? [], page.sessions));
          setLoadedFrom(from);
        }
        return page.sessions.length;
      },
      { done: (n) => (n === 0 ? "No older walks saved." : `Loaded ${n} older ${n === 1 ? "walk" : "walks"}.`) },
    );

  return { raw, stats, loadError, loadedFrom, dayLoading, olderToLoad, loadOlder, older };
}
