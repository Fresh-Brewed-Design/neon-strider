import { describe, expect, it } from "vitest";
import type { MergedSession } from "../../services/sessionMerge";
import { groupByDay, monthStart, rangeBounds, visibleSessions } from "./sessions";

const NOW = new Date(2026, 10, 14, 12, 0);

function walk(id: number, daysAgo: number, hour: number, meters: number): MergedSession {
  const start = new Date(2026, 10, 14 - daysAgo, hour).getTime();
  return {
    id,
    mergedIds: [id],
    start_time: start,
    end_time: start + 600_000,
    duration_seconds: 600,
    distance_meters: meters,
    raw_distance_meters: meters,
    calories: 40,
    steps: 0,
    raw_steps: 0,
    avg_speed: 3,
    raw_avg_speed: 3,
    avg_incline: 0,
    strava_id: null,
    neural_heat: 0,
    neural_ticks: 0,
    last_hardware_time: 0,
    avg_heart_rate: null,
    max_heart_rate: null,
    program_name: null,
  };
}

// Newest first, as the page holds them.
const SESSIONS = [walk(4, 0, 9, 1000), walk(3, 0, 7, 500), walk(2, 6, 8, 2000), walk(1, 7, 8, 3000)];

describe("visibleSessions", () => {
  it("counts today as the first of the range's days", () => {
    expect(visibleSessions(SESSIONS, "7d", null, NOW).map((s) => s.id)).toEqual([4, 3, 2]);
  });

  it("shows everything for All, and only the picked day when there is one", () => {
    expect(visibleSessions(SESSIONS, "all", null, NOW)).toHaveLength(4);
    expect(visibleSessions(SESSIONS, "all", "2026-11-14", NOW).map((s) => s.id)).toEqual([4, 3]);
  });
});

describe("rangeBounds and monthStart", () => {
  it("spans one local day for a picked day", () => {
    expect(rangeBounds("7d", "2026-11-14", NOW)).toEqual({
      from: new Date(2026, 10, 14).getTime(),
      to: new Date(2026, 10, 15).getTime(),
    });
  });

  it("starts a range at midnight and leaves All open", () => {
    expect(rangeBounds("7d", null, NOW)).toEqual({ from: new Date(2026, 10, 8).getTime(), to: null });
    expect(rangeBounds("all", null, NOW)).toEqual({ from: null, to: null });
  });

  it("finds the first of a month, across a year end", () => {
    expect(monthStart(new Date(2026, 0, 20), 2)).toBe(new Date(2025, 10, 1).getTime());
  });
});

describe("groupByDay", () => {
  it("groups consecutive sessions of a day under one heading", () => {
    const groups = groupByDay(SESSIONS);
    expect(groups.map((g) => g.items.map((s) => s.id))).toEqual([[4, 3], [2], [1]]);
  });
});
