// History: a few weeks of fake walks and their samples. With ?heart=on, every
// other walk was saved with a watch linked.
import { params } from "../mockState";
import type { Scenario } from "./types";

const heartWalks = params.get("heart") === "on";

/** A few weeks of fake walks for the History page. */
function mockSessions() {
  const out = [];
  const day = 24 * 3600 * 1000;
  let id = 1;
  for (let d = 40; d >= 0; d -= 1) {
    if (d % 3 === 1) continue;
    const start = Date.now() - d * day - 6 * 3600 * 1000;
    const dur = 900 + ((d * 437) % 2400);
    const dist = dur * 0.9;
    out.push({
      id: id++, start_time: start, end_time: start + dur * 1000, duration_seconds: dur,
      distance_meters: dist, raw_distance_meters: dist, calories: dist * 0.06, steps: dist * 1.4,
      raw_steps: dist * 1.4, avg_speed: 3.2, raw_avg_speed: 3.6, avg_incline: 4,
      strava_id: d % 4 === 0 ? "123" : null, neural_heat: 0, neural_ticks: 0, last_hardware_time: dur,
      avg_heart_rate: heartWalks && d % 2 === 0 ? 104 + (d % 9) : null,
      max_heart_rate: heartWalks && d % 2 === 0 ? 121 + (d % 13) : null,
    });
  }
  return out;
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

export const sessions = mockSessions();
export const lastId = sessions[sessions.length - 1].id;

export const history: Scenario = {
  queries: {
    export_sessions_csv: () => "C:/Users/you/Downloads/neon-strider-sessions-preview.csv",
    get_sessions: () => sessions,
    get_samples: (args) => mockSamples(args.sessionId as number),
  },
};
