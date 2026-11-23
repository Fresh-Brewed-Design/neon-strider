// History's Today timeline. ?today: a desk day from 08:02 to 14:25, with a
// morning walk, a walk before lunch, a long sit after lunch and a walk now.
// ?today=early: 20 minutes in. ?today=long: 06:40 to 21:30, the widest span.
// The PC time walked stat matches each day.
import type { TimelineMark, TimelineRun, TodayTimeline } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** `[start, mark]` stretches in clock time, the way Rust merges its 5-minute slots. */
type Day = { stretches: [string, TimelineMark][]; now: string };

const DAYS: Record<string, Day> = {
  "": {
    stretches: [
      ["08:00", "desk"],
      ["09:15", "walked"],
      ["09:50", "desk"],
      ["10:05", "away"],
      ["10:20", "desk"],
      ["11:30", "walked"],
      ["12:00", "away"],
      ["12:45", "desk"],
      ["14:05", "walked"],
    ],
    now: "14:25",
  },
  early: {
    stretches: [
      ["08:00", "desk"],
      ["08:15", "walked"],
    ],
    now: "08:22",
  },
  long: {
    stretches: [
      ["06:40", "desk"],
      ["07:30", "walked"],
      ["08:20", "desk"],
      ["10:00", "walked"],
      ["10:40", "away"],
      ["11:00", "desk"],
      ["12:30", "away"],
      ["13:15", "desk"],
      ["15:00", "walked"],
      ["16:10", "desk"],
      ["18:00", "away"],
      ["19:30", "desk"],
      ["20:40", "walked"],
      ["21:10", "desk"],
    ],
    now: "21:30",
  },
};

function minutes(clock: string): number {
  const [h, m] = clock.split(":").map(Number);
  return h * 60 + m;
}

export function mockTimeline(day: Day): TodayTimeline {
  const now_min = minutes(day.now);
  const runs: TimelineRun[] = day.stretches.map(([start, mark], i) => ({
    start_min: minutes(start),
    end_min: i + 1 < day.stretches.length ? minutes(day.stretches[i + 1][0]) : now_min,
    mark,
  }));
  return { start_min: runs[0].start_min, now_min, slot_min: 5, runs };
}

export const today: Scenario = {
  apply() {
    if (!params.has("today")) return;
    const day = DAYS[params.get("today") ?? ""] ?? DAYS[""];
    const timeline = mockTimeline(day);
    snapshot.today_timeline = timeline;
    let active_s = 0;
    let walked_s = 0;
    for (const r of timeline.runs) {
      const s = (r.end_min - r.start_min) * 60;
      if (r.mark !== "away") active_s += s;
      if (r.mark === "walked") walked_s += s;
    }
    snapshot.pc_walk_share = { active_s, walked_s, share_pct: Math.round((walked_s / active_s) * 100) };
  },
};
