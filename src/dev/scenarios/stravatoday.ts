// DAILY_UPLINK (upload today as one Strava activity). ?stravatoday=ready has
// two walks of today not on Strava yet; =mixed one more already uploaded by
// itself (the note shows); =running a walk of today still on the belt; =none
// no walk today; =fail the upload ends with Strava refusing it. With no flag,
// today follows the mock History (its one walk today already on Strava).
// A press marks today's walks as on Strava after a moment, like Rust.
import type { StravaToday } from "../../ipc";
import { dayKey } from "../../format";
import { mergeSessions } from "../../services/sessionMerge";
import { later, params, snapshot } from "../mockState";
import { sessions } from "./history";
import type { Scenario } from "./types";

const flag = params.get("stravatoday");
const HOUR = 3_600_000;
/** Same sentences as `TodayBlock::sentence` and `today.rs` in Rust. */
const NO_WALKS = "No walks today yet.";
const RUNNING = "A walk is running. Stop the belt to upload today.";
const ALL_ON_STRAVA = "Today's walks are on Strava.";
const NOT_LINKED = "Link a Strava account to upload today.";
const MERGE_MINUTES = 10;

function todays() {
  const key = dayKey(Date.now());
  return sessions.filter((s) => dayKey(s.start_time) === key);
}

/** A stand-in for Rust's `plan_today` over the mock History. */
function mockToday(): StravaToday {
  const all = todays();
  const pending = all.filter((s) => !s.strava_id);
  const onStrava = new Set(all.filter((s) => s.strava_id).map((s) => s.strava_id)).size;
  const base = { walks: 0, distance_km: 0, on_strava: onStrava, note: null };
  if (!snapshot.strava.connected) return { ...base, blocked: NOT_LINKED };
  if (flag === "running") return { ...base, blocked: RUNNING };
  if (pending.length === 0) return { ...base, blocked: onStrava > 0 ? ALL_ON_STRAVA : NO_WALKS };
  const walks = mergeSessions(pending, MERGE_MINUTES).length;
  const one = onStrava === 1;
  return {
    walks,
    distance_km: pending.reduce((m, s) => m + s.distance_meters, 0) / 1000,
    on_strava: onStrava,
    blocked: null,
    note:
      onStrava === 0
        ? null
        : `${onStrava} ${one ? "activity" : "activities"} from today ${one ? "is" : "are"} already on Strava and ${one ? "stays" : "stay"} separate.`,
  };
}

/** A walk of today, `hoursAgo` back, 20 minutes long. */
function addWalk(hoursAgo: number, stravaId: string | null) {
  const start = Date.now() - hoursAgo * HOUR;
  const dur = 1200;
  const dist = dur * 0.9;
  sessions.push({
    ...sessions[sessions.length - 1],
    id: Math.max(0, ...sessions.map((s) => s.id)) + 1,
    start_time: start,
    end_time: start + dur * 1000,
    duration_seconds: dur,
    distance_meters: dist,
    raw_distance_meters: dist,
    strava_id: stravaId,
  });
  sessions.sort((a, b) => a.start_time - b.start_time);
}

export const stravaToday: Scenario = {
  install() {
    // Only walks that started today and are over by now.
    const startOfDay = new Date().setHours(0, 0, 0, 0);
    const hoursIn = (Date.now() - startOfDay) / HOUR;
    const at = (h: number) => Math.min(h, Math.max(0.5, hoursIn - 0.2));
    if (flag === "none") {
      for (const s of todays()) sessions.splice(sessions.indexOf(s), 1);
    } else if (flag === "ready" || flag === "running" || flag === "fail") {
      for (const s of todays()) s.strava_id = null;
      addWalk(at(2), null);
    } else if (flag === "mixed") {
      for (const s of todays()) s.strava_id = "321";
      addWalk(at(3), null);
      addWalk(at(1), null);
    }
    snapshot.strava.today = mockToday();
  },
  commands: {
    strava_upload_today: () => {
      const t = mockToday();
      if (t.blocked !== null) throw t.blocked;
      snapshot.strava.syncing = true;
      snapshot.strava.activity = { state: "uploading", label: `${t.distance_km.toFixed(2)} km`, step: "sending" };
      later(1800, () => {
        snapshot.strava.syncing = false;
        snapshot.strava.activity = { state: "idle" };
        if (flag === "fail") {
          snapshot.strava.last_result = {
            ok: false,
            message: "Upload failed. Can't reach Strava. Check your internet connection.",
            at_ms: Date.now(),
            hint: "check_internet",
          };
          return;
        }
        for (const s of todays()) if (!s.strava_id) s.strava_id = "456";
        snapshot.strava.last_result = {
          ok: true,
          message: `Today uploaded as one activity: ${t.walks} walk${t.walks === 1 ? "" : "s"}, ${t.distance_km.toFixed(2)} km (activity 456)`,
          at_ms: Date.now(),
          hint: "none",
        };
        snapshot.strava.today = mockToday();
      });
    },
  },
};
