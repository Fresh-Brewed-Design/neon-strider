// Strava. ?strava=authorizing|uploading|processing|checking|backoff|error|paused
// previews each Strava state; ?nostrava has no credentials or account;
// ?reconnect is a login from before shoes and the duplicate check, so it needs
// a fresh grant; ?saved marks a walk as saved for Strava; ?savefail makes
// saving one fail; ?loginstorage is a login an older build kept in the
// database because Windows Credential Manager failed.
import type { StravaSnapshot } from "../../ipc";
import { DEFAULT_DESCRIPTION, DEFAULT_ROLLUP_TITLE, DEFAULT_TITLE, later, params, publish, snapshot } from "../mockState";
import { lastId, sessions } from "./history";
import type { Scenario } from "./types";

/** A rough stand-in for the Rust renderer: the real one is tested in Rust. */
function previewTemplate(template: string, fallback: string): string {
  const sample: Record<string, string> = {
    distance_km: "3.42", duration: "1:02:05", avg_speed: "3.3", avg_incline: "4.5", calories: "214",
    steps: "4980", walks: "3", date: "2026-11-13", time: "07:45",
  };
  const text = template.trim() === "" ? fallback : template;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => sample[name] ?? whole);
}

const stravaFlag = params.get("strava");
const INACTIVE_MESSAGE =
  "Strava turned off the API app because its owner needs a Strava subscription. Subscribe and reactivate the app at strava.com/settings/api, or use Save for Strava.";
const OFFLINE_MESSAGE = "Can't reach Strava. Check your internet connection.";
const LOGIN_STORAGE_MESSAGE = "The Strava login could not be saved securely in Windows Credential Manager.";
const PAUSED: Partial<StravaSnapshot> = {
  activity: { state: "paused", reason: INACTIVE_MESSAGE },
  last_result: {
    ok: false,
    message: `Auto-sync failed. ${INACTIVE_MESSAGE} Auto-sync is paused until you press Retry or change the credentials.`,
    at_ms: Date.now() - 60_000,
    hint: "open_api_settings",
  },
};
const STRAVA_SCENARIOS: Record<string, Partial<StravaSnapshot>> = {
  authorizing: { connected: false, athlete_name: null, activity: { state: "authorizing", expires_in_s: 252 } },
  uploading: { syncing: true, activity: { state: "uploading", label: "2.40 km", step: "sending" } },
  processing: { syncing: true, activity: { state: "uploading", label: "2.40 km", step: "processing" } },
  checking: { syncing: true, activity: { state: "checking_duplicates" } },
  backoff: {
    activity: { state: "backing_off", retry_in_s: 842, reason: OFFLINE_MESSAGE },
    last_result: {
      ok: false,
      message: `Auto-sync failed. ${OFFLINE_MESSAGE} Trying again in 15 minutes.`,
      at_ms: Date.now() - 60_000,
      hint: "check_internet",
    },
  },
  error: PAUSED,
  paused: PAUSED,
};

function finishStravaTask(message: string) {
  snapshot.strava.last_result = { ok: true, message, at_ms: Date.now(), hint: "none" };
}

/** Rust syncing in the background: "syncing" for a moment, then a result. */
function stravaSync(message: string) {
  snapshot.strava.syncing = true;
  later(1500, () => {
    snapshot.strava.syncing = false;
    finishStravaTask(message);
  });
}

export const strava: Scenario = {
  apply() {
    if (params.has("nostrava")) Object.assign(snapshot.strava, { has_credentials: false, connected: false, athlete_name: null, auto_sync: false, last_result: null });
    if (params.has("loginstorage")) snapshot.strava.login_storage = LOGIN_STORAGE_MESSAGE;
    if (params.has("reconnect")) Object.assign(snapshot.strava, { needs_reconnect: true, gear: [], gear_id: null });
    if (stravaFlag) Object.assign(snapshot.strava, STRAVA_SCENARIOS[stravaFlag]);
  },
  install() {
    const previous = sessions[sessions.length - 2];
    if (stravaFlag === "error" || stravaFlag === "paused" || stravaFlag === "backoff") {
      sessions[sessions.length - 1].strava_id = null;
      snapshot.strava.upload_errors = { [String(lastId)]: stravaFlag === "backoff" ? OFFLINE_MESSAGE : INACTIVE_MESSAGE };
    }
    if (stravaFlag === "uploading" || stravaFlag === "processing") {
      sessions[sessions.length - 1].strava_id = null;
      snapshot.strava.uploading_session_ids = [lastId];
    }
    if (params.has("saved")) {
      previous.strava_id = null;
      snapshot.strava.saved_session_ids = [previous.id];
    }
  },
  /** Strava's countdowns, once a second like the Rust ones. */
  tick() {
    const a = snapshot.strava.activity;
    if (a.state === "authorizing") {
      snapshot.strava.activity = { state: "authorizing", expires_in_s: Math.max(0, a.expires_in_s - 1) };
      return true;
    } else if (a.state === "backing_off") {
      snapshot.strava.activity = { ...a, retry_in_s: Math.max(0, a.retry_in_s - 1) };
      return true;
    }
    return false;
  },
  commands: {
    strava_cancel_connect: () => (snapshot.strava.activity = { state: "idle" }),
    strava_retry_now: () => {
      snapshot.strava.activity = { state: "idle" };
      stravaSync("Nothing to upload yet");
    },
    strava_mark_uploaded: (c) => {
      snapshot.strava.saved_session_ids = snapshot.strava.saved_session_ids.filter((i) => !c.session_ids.includes(i));
      for (const sess of sessions) if (c.session_ids.includes(sess.id)) sess.strava_id = "manual";
    },
    strava_set_credentials: () => (snapshot.strava.has_credentials = true),
    strava_connect: () => {
      Object.assign(snapshot.strava, { connected: true, athlete_name: "Preview Athlete" });
      finishStravaTask("Connected as Preview Athlete");
    },
    strava_disconnect: () => Object.assign(snapshot.strava, { connected: false, athlete_name: null }),
    strava_set_auto_sync: (c) => (snapshot.strava.auto_sync = c.enabled),
    strava_set_daily_rollup: (c) => Object.assign(snapshot.strava, { daily_rollup: c.enabled, shift_end: c.shift_end }),
    strava_set_gear: (c) => (snapshot.strava.gear_id = c.gear_id),
    strava_refresh_gear: () => {
      snapshot.strava.gear_loading = true;
      later(800, () => {
        snapshot.strava.gear_loading = false;
        finishStravaTask("Loaded 2 shoes from Strava");
      });
    },
    strava_set_mute: (c) => Object.assign(snapshot.strava, { mute_mode: c.mode, mute_under_minutes: c.under_minutes }),
    strava_set_templates: (c) =>
      Object.assign(snapshot.strava, {
        title_template: c.title.trim() === "" ? DEFAULT_TITLE : c.title,
        rollup_title_template: c.rollup_title.trim() === "" ? DEFAULT_ROLLUP_TITLE : c.rollup_title,
        description_template: c.description.trim() === "" ? DEFAULT_DESCRIPTION : c.description,
      }),
    // Rust uploads in the background: show "syncing", then a result.
    strava_upload: () => stravaSync("Uploaded 1.72 km to Strava (activity 123)"),
    strava_sync_now: () => stravaSync("Nothing to upload yet"),
  },
  queries: {
    strava_save_for_strava: (args) => {
      if (params.get("savefail") !== null) throw "No finished walk to save. Every walk is already on Strava, or one is still running.";
      const ids = args.sessionIds as number[] | null;
      snapshot.strava.saved_session_ids = [...new Set([...snapshot.strava.saved_session_ids, ...(ids ?? [lastId])])];
      publish();
      return { file_name: "2026-11-13 NEON STRIDER // 2.40 KM UPLINK.tcx", folder: "C:/Users/you/Downloads/Neon Strider" };
    },
    strava_template_preview: (args) => {
      const line = (t: unknown, fallback: string) => previewTemplate(t as string, fallback).replace(/\s+/g, " ").slice(0, 100);
      return {
        title: line(args.title, DEFAULT_TITLE),
        rollup_title: line(args.rollupTitle, DEFAULT_ROLLUP_TITLE),
        description: previewTemplate(args.description as string, DEFAULT_DESCRIPTION),
      };
    },
    strava_tcx_preview: () => "<TrainingCenterDatabase><!-- preview --></TrainingCenterDatabase>",
  },
};
