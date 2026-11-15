import { describe, it, expect } from "vitest";
import { mergeSessions } from "./sessionMerge";
import type { Session } from "./db/schema";

const MIN = 60_000;

function session(id: number, startMin: number, durationMin: number, extra: Partial<Session> = {}): Session {
  return {
    id,
    start_time: startMin * MIN,
    end_time: (startMin + durationMin) * MIN,
    duration_seconds: durationMin * 60,
    distance_meters: durationMin * 100,
    raw_distance_meters: durationMin * 110,
    calories: durationMin * 4,
    steps: durationMin * 90,
    raw_steps: durationMin * 95,
    avg_speed: 6,
    raw_avg_speed: 6.6,
    avg_incline: 0,
    strava_id: null,
    neural_heat: 0,
    neural_ticks: 0,
    last_hardware_time: 0,
    ...extra,
  };
}

describe("mergeSessions", () => {
  it("returns nothing for no sessions", () => {
    expect(mergeSessions([], 10)).toEqual([]);
  });

  it("keeps a lone session and records its id", () => {
    const [only] = mergeSessions([session(1, 0, 10)], 10);
    expect(only.mergedIds).toEqual([1]);
    expect(only.distance_meters).toBe(1000);
  });

  it("sums counters of sessions closer than the threshold", () => {
    const [m] = mergeSessions([session(1, 0, 10), session(2, 15, 20)], 10);
    expect(m.mergedIds).toEqual([1, 2]);
    expect(m.duration_seconds).toBe(30 * 60);
    expect(m.distance_meters).toBe(3000);
    expect(m.raw_distance_meters).toBe(3300);
    expect(m.calories).toBe(120);
    expect(m.steps).toBe(2700);
    expect(m.raw_steps).toBe(2850);
    expect(m.start_time).toBe(0);
    expect(m.end_time).toBe(35 * MIN);
  });

  it("measures the gap from the previous end_time, not its start", () => {
    // Session 1 ends at minute 60. Session 2 starts at 65: a 5 minute gap.
    // Measured from the start it would look like 65 minutes apart.
    expect(mergeSessions([session(1, 0, 60), session(2, 65, 5)], 10)).toHaveLength(1);
  });

  it("splits when the gap is at or beyond the threshold", () => {
    // Gap of exactly 10 minutes is not "less than" the threshold.
    expect(mergeSessions([session(1, 0, 10), session(2, 20, 10)], 10)).toHaveLength(2);
    expect(mergeSessions([session(1, 0, 10), session(2, 25, 10)], 10)).toHaveLength(2);
  });

  it("chains a run, measuring each gap from the growing group end", () => {
    const merged = mergeSessions([session(1, 0, 10), session(2, 15, 10), session(3, 30, 10)], 10);
    expect(merged).toHaveLength(1);
    expect(merged[0].mergedIds).toEqual([1, 2, 3]);
  });

  it("sorts unsorted input and returns oldest first", () => {
    const merged = mergeSessions([session(3, 200, 10), session(1, 0, 10), session(2, 12, 10)], 10);
    expect(merged.map((m) => m.mergedIds)).toEqual([[1, 2], [3]]);
  });

  it("recomputes average speed from the summed totals", () => {
    const [m] = mergeSessions([session(1, 0, 30), session(2, 31, 30)], 10);
    // 6 km in 1 h.
    expect(m.avg_speed).toBeCloseTo(6, 6);
    expect(m.raw_avg_speed).toBeCloseTo(6.6, 6);
  });

  it("weights average incline by duration", () => {
    const [m] = mergeSessions(
      [session(1, 0, 10, { avg_incline: 2 }), session(2, 11, 30, { avg_incline: 6 })],
      10,
    );
    expect(m.avg_incline).toBeCloseTo(5, 6);
  });

  it("sums neural counters and keeps any strava id", () => {
    const [m] = mergeSessions(
      [
        session(1, 0, 10, { neural_heat: 5, neural_ticks: 2 }),
        session(2, 11, 10, { neural_heat: 7, neural_ticks: 3, strava_id: "abc" }),
      ],
      10,
    );
    expect(m.neural_heat).toBe(12);
    expect(m.neural_ticks).toBe(5);
    expect(m.strava_id).toBe("abc");
  });

  it("handles zero-duration groups without dividing by zero", () => {
    const [m] = mergeSessions([session(1, 0, 0), session(2, 1, 0)], 10);
    expect(m.avg_speed).toBe(0);
    expect(m.avg_incline).toBe(0);
  });

  it("does not mutate its input", () => {
    const a = session(1, 0, 10);
    const b = session(2, 11, 10);
    mergeSessions([a, b], 10);
    expect(a.duration_seconds).toBe(600);
    expect(a.end_time).toBe(10 * MIN);
  });
});
