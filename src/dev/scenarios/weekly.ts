// ?weekly shows the last-week HUD card. Variants: ?weekly=long (123.4 km, 12h 05m,
// +1234%), ?weekly=new (nothing the week before), ?weekly=down.
import type { WeeklySummary } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

export const weekly: Scenario = {
  apply() {
    const weeklyFlag = params.get("weekly");
    const WEEKLY_SCENARIOS: Record<string, Partial<WeeklySummary>> = {
      long: {
        last_week: { ...snapshot.weekly.last_week, distance_km: 123.4, duration_s: 12 * 3600 + 5 * 60, active_days: 7, sessions: 21 },
        distance_change: { pct: 1234, is_new: false },
      },
      new: {
        week_before: { distance_km: 0, duration_s: 0, active_days: 0, sessions: 0, calories: 0, longest_day_km: 0 },
        distance_change: { pct: null, is_new: true },
      },
      down: { distance_change: { pct: -23, is_new: false } },
    };
    if (weeklyFlag !== null) Object.assign(snapshot.weekly, { show_card: true }, WEEKLY_SCENARIOS[weeklyFlag]);
  },
  commands: {
    weekly_summary_ack: () => (snapshot.weekly.show_card = false),
  },
};
