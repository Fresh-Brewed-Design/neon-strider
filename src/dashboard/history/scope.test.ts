import { describe, expect, it } from "vitest";
import type { Sample } from "../../services/db/schema";
import { heartAxis, heartRuns, scopePoints } from "./scope";

function sample(timestamp: number, heart_rate: number | null): Sample {
  return {
    session_id: 1,
    timestamp,
    speed: 3,
    raw_speed: 3,
    incline: 2,
    distance: 0,
    raw_distance: 0,
    steps: 0,
    raw_steps: 0,
    heart_rate,
  };
}

describe("scopePoints", () => {
  it("keeps one point per second, oldest first, with its heart rate", () => {
    const points = scopePoints([sample(3000, 120), sample(1000, 100), sample(2000, null), sample(1000, 101)]);
    expect(points.map((p) => [p.timestamp, p.heartRate])).toEqual([
      [1000, 101],
      [2000, null],
      [3000, 120],
    ]);
  });

  it("treats a zero reading as no reading", () => {
    expect(scopePoints([sample(1000, 0)])[0].heartRate).toBeNull();
  });
});

describe("heartRuns", () => {
  it("splits at a second without a reading instead of joining across it", () => {
    const runs = heartRuns(scopePoints([
      sample(1000, 100), sample(2000, 102), sample(3000, null), sample(4000, null),
      sample(5000, 110), sample(6000, null), sample(7000, 115),
    ]));
    expect(runs.map((r) => r.map((p) => p.bpm))).toEqual([[100, 102], [110], [115]]);
  });

  it("is empty for a walk without heart rate", () => {
    const runs = heartRuns(scopePoints([sample(1000, null), sample(2000, null)]));
    expect(runs).toEqual([]);
    expect(heartAxis(runs)).toBeNull();
  });
});

describe("heartAxis", () => {
  it("rounds out to tens with some room above and below", () => {
    expect(heartAxis([[{ timestamp: 0, bpm: 98 }, { timestamp: 1, bpm: 131 }]])).toEqual({ min: 90, max: 140 });
  });

  it("stays at least 20 bpm tall for a steady pulse", () => {
    expect(heartAxis([[{ timestamp: 0, bpm: 110 }, { timestamp: 1, bpm: 110 }]])).toEqual({ min: 100, max: 120 });
  });
});
