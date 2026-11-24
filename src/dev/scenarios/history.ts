// History: a few weeks of fake walks and their samples. With ?heart=on, every
// other walk was saved with a watch linked. ?nohistory: nothing walked yet, so
// History shows its empty state (no walks, an empty week, DESK_SYNC and WALL_CLOCK).
// ?longhistory: three walks a day for 14 months, so History lists the newest
// months and offers Load older walks.
import { dayKey } from "../../format";
import { mergeSessions } from "../../services/sessionMerge";
import type { Session } from "../../services/db/schema";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Mock rows always carry an id. */
type MockSession = Session & { id: number };

const heartWalks = params.get("heart") === "on";
const longHistory = params.has("longhistory");

/** A few weeks of fake walks for the History page. */
function mockSessions(): MockSession[] {
  const out: MockSession[] = [];
  const day = 24 * 3600 * 1000;
  let id = 1;
  const hours = longHistory ? [6, 10, 14] : [6];
  for (let d = longHistory ? 425 : 40; d >= 0; d -= 1) {
    if (!longHistory && d % 3 === 1) continue;
    for (const h of hours) {
      const start = Date.now() - d * day - h * 3600 * 1000;
      const dur = 900 + ((d * 437) % 2400);
      const dist = dur * 0.9;
      out.push({
        id: id++, start_time: start, end_time: start + dur * 1000, duration_seconds: dur,
        distance_meters: dist, raw_distance_meters: dist, calories: dist * 0.06, steps: dist * 1.4,
        raw_steps: dist * 1.4, avg_speed: 3.2, raw_avg_speed: 3.6, avg_incline: 4,
        strava_id: d % 4 === 0 ? "123" : null, neural_heat: 0, neural_ticks: 0, last_hardware_time: dur,
        avg_heart_rate: heartWalks && d % 2 === 0 ? 104 + (d % 9) : null,
        max_heart_rate: heartWalks && d % 2 === 0 ? 121 + (d % 13) : null,
        program_name: null,
      });
    }
  }
  return out.sort((a, b) => a.start_time - b.start_time);
}

function mockSamples(sessionId: number) {
  // Walks saved with heart rate (?heart=on) get a pulse that follows the speed,
  // with the watch dropping out for a stretch in the middle.
  const avgHeart = sessions.find((s) => s.id === sessionId)?.avg_heart_rate ?? null;
  return Array.from({ length: 120 }, (_, i) => ({
    id: i, session_id: sessionId, timestamp: Date.now() - (120 - i) * 10000,
    speed: 3 + Math.sin(i / 10), raw_speed: 3.3, incline: 4 + (i > 60 ? 2 : 0),
    distance: i * 9, raw_distance: i * 9, steps: i * 13, raw_steps: i * 13,
    heart_rate: avgHeart !== null && (i < 70 || i > 80) ? Math.round(avgHeart + 9 * Math.sin(i / 10)) : null,
  }));
}

const noHistory = params.has("nohistory");

export const sessions: MockSession[] = noHistory ? [] : mockSessions();

/** Rust's merge threshold default; the preview never changes it. */
const MERGE_MINUTES = 10;

/** Newest first, like Rust's range reads (without the reach back for a cut walk). */
function between(from: number, before: number | null): Session[] {
  return sessions.filter((s) => s.start_time >= from && (before === null || s.start_time < before)).reverse();
}

function stats(from: number | null, to: number | null, calendarFrom: number) {
  const walks = mergeSessions(sessions, MERGE_MINUTES);
  const inRange = walks.filter((w) => (from === null || w.start_time >= from) && (to === null || w.start_time < to));
  const byDay = new Map<string, number>();
  for (const w of walks.filter((w) => w.start_time >= calendarFrom)) {
    byDay.set(dayKey(w.start_time), (byDay.get(dayKey(w.start_time)) ?? 0) + w.distance_meters);
  }
  return {
    totals: {
      meters: inRange.reduce((t, w) => t + w.distance_meters, 0),
      seconds: inRange.reduce((t, w) => t + w.duration_seconds, 0),
      kcal: inRange.reduce((t, w) => t + w.calories, 0),
      sessions: inRange.length,
    },
    meters_by_day: [...byDay].map(([d, meters]) => ({ day: d, meters })),
    oldest_start_ms: sessions[0]?.start_time ?? null,
  };
}

function older(before: number) {
  const newest = between(-Infinity, before)[0];
  if (!newest) return { sessions: [], from_ms: null };
  const at = new Date(newest.start_time);
  const from = new Date(at.getFullYear(), at.getMonth() - 2, 1).getTime();
  return { sessions: between(from, before), from_ms: from };
}
export const lastId = sessions[sessions.length - 1]?.id ?? 0;

const EMPTY_WEEK = { distance_km: 0, duration_s: 0, active_days: 0, sessions: 0, calories: 0, longest_day_km: 0 };

export const history: Scenario = {
  apply() {
    if (!noHistory) return;
    Object.assign(snapshot.weekly, {
      this_week: EMPTY_WEEK, last_week: EMPTY_WEEK, week_before: EMPTY_WEEK,
      distance_change: { pct: null, is_new: false },
    });
    Object.assign(snapshot.desk_sync, { days: [], walked: 0, desk_days: 0, today_open: false });
    Object.assign(snapshot.wall_clock, { hours: Array<number>(24).fill(0), desk_days: 0 });
  },
  queries: {
    export_sessions_csv: () => "C:/Users/you/Downloads/neon-strider-sessions-preview.csv",
    history_sessions: (args) => between(args.fromMs as number, (args.beforeMs as number | null) ?? null),
    history_older: (args) => older(args.beforeMs as number),
    history_stats: (args) =>
      stats((args.fromMs as number | null) ?? null, (args.toMs as number | null) ?? null, args.calendarFromMs as number),
    get_samples: (args) => mockSamples(args.sessionId as number),
  },
};
