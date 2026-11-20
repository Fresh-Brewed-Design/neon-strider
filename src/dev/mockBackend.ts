// Dev-only mocked backend shared by the HUD and dashboard previews
// (hud-preview.html, dashboard-preview.html). Not part of the production build.
// URL flags pick a scenario; each is described where it is applied below.
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import type { ActiveProgram, AppSnapshot, Command, ConnectionSnapshot } from "../ipc";
import type { DeskSync, MaintenanceSnapshot, Program, StravaSnapshot, WeeklySummary } from "../ipc";
import type { RustConfig } from "../services/controlOrchestrator";

// Same defaults as src-tauri/src/strava_template.rs.
const DEFAULT_TITLE = "NEON STRIDER // {distance_km} KM UPLINK";
const DEFAULT_ROLLUP_TITLE = "NEON STRIDER // DAILY UPLINK · {walks} WALKS · {distance_km} KM";
const DEFAULT_DESCRIPTION =
  "DECK_CYCLES {duration} · DISPLACEMENT {distance_km} km · ALTITUDE_BIAS {avg_incline}% · METABOLIC_BURN {calories} kcal\nLogged by Neon Strider";

/**
 * DESK_SYNC over the last 14 weekdays (weekends off the PC), ending today,
 * which is walked; one day in the middle had no walk.
 */
function mockDeskSync(): DeskSync {
  const days: DeskSync["days"] = [];
  const d = new Date();
  while (days.length < 14) {
    if (d.getDay() !== 0 && d.getDay() !== 6) {
      const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
      days.unshift({ date, walked: ![3, 8].includes(days.length) });
    }
    d.setDate(d.getDate() - 1);
  }
  return { walked: days.filter((x) => x.walked).length, desk_days: days.length, window: 14, days, today_open: false };
}

/** A rough stand-in for the Rust renderer: the real one is tested in Rust. */
function previewTemplate(template: string, fallback: string): string {
  const sample: Record<string, string> = {
    distance_km: "3.42", duration: "1:02:05", avg_speed: "3.3", avg_incline: "4.5", calories: "214",
    steps: "4980", walks: "3", date: "2026-11-13", time: "07:45",
  };
  const text = template.trim() === "" ? fallback : template;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => sample[name] ?? whole);
}

export const snapshot: AppSnapshot = {
  connection: {
    connected: true,
    connecting: false,
    device_id: "preview",
    has_saved_device: true,
    link: { state: "connected" },
    last_error: null,
    start_pending: false,
    start_result: null,
    connect_dialog_at_ms: null,
    simulated: false,
  },
  deck: {
    status: "Running",
    unknown_status_code: null,
    standby: false,
    speed_kmh: 3.2,
    raw_speed_kmh: 3.6,
    target_speed_kmh: 3.5,
    incline_level: 4,
    incline_pct: 5.0,
    incline_offset_pct: 0,
    incline_pct_per_level: 1.25,
    target_incline_level: 4,
    target_incline_pct: 5.0,
    min_speed_kmh: 0.6,
    max_speed_kmh: 10.5,
    max_incline_level: 15,
    max_incline_pct: 18.75,
  },
  session: { active_id: 1, duration_s: 1934, distance_m: 1720, steps: 2412, calories: 103, daily_distance_m: 4310 },
  programs: [
    { id: 1, name: "Post-meal", steps: [{ duration_minutes: 10, speed_kmh: 4, incline_pct: 5 }], repeat_every_minutes: null, note: "A walk soon after eating blunts the rise in blood sugar." },
    { id: 2, name: "Workday", steps: [{ duration_minutes: 5, speed_kmh: 3, incline_pct: 5 }], repeat_every_minutes: 30, note: "Five minutes every half hour offsets long sitting." },
    { id: 3, name: "Focus", steps: [{ duration_minutes: null, speed_kmh: 2, incline_pct: 0 }], repeat_every_minutes: null, note: "Slow and flat, so typing and thinking stay steady." },
    { id: 4, name: "Micro-break", steps: [{ duration_minutes: 2, speed_kmh: 3.2, incline_pct: 0 }], repeat_every_minutes: 20, note: "Two easy minutes every 20 lower blood sugar after meals." },
    { id: 5, name: "Brisk", steps: [{ duration_minutes: 30, speed_kmh: 5, incline_pct: 0 }], repeat_every_minutes: null, note: "Moderate walking toward the weekly 150 minutes." },
    { id: 6, name: "VO2 max", steps: [{ duration_minutes: 5, speed_kmh: 4.5, incline_pct: 0 }, { duration_minutes: 4, speed_kmh: 6, incline_pct: 10 }, { duration_minutes: 3, speed_kmh: 4, incline_pct: 0 }, { duration_minutes: 4, speed_kmh: 6, incline_pct: 10 }, { duration_minutes: 3, speed_kmh: 4, incline_pct: 0 }, { duration_minutes: 4, speed_kmh: 6, incline_pct: 10 }, { duration_minutes: 3, speed_kmh: 4, incline_pct: 0 }, { duration_minutes: 4, speed_kmh: 6, incline_pct: 10 }, { duration_minutes: 3, speed_kmh: 4, incline_pct: 0 }, { duration_minutes: 3, speed_kmh: 3, incline_pct: 0 }], repeat_every_minutes: null, note: "Four hard 4-minute climbs with easy walks between train VO2 max." },
  ],
  active_program: {
    program_id: 2,
    name: "Workday",
    phase: "walking",
    step_index: 0,
    step_count: 1,
    seconds_remaining: 252,
    cycle: 3,
    paused: false,
    wait: null,
    speed_kmh: 3,
    incline_pct: 5,
  },
  neural: {
    enabled: true,
    factor: 1,
    active: false,
    base_kmh: 3.5,
    target_kmh: 3.5,
    keys_per_min: 14,
    floor_pct: 90,
    sensitivity: "medium",
  },
  hud: {
    widgets: ["VELOCITY", "INCLINE", "TIMER", "DISTANCE", "DAILY", "CONTROLS", "PROGRAMS"],
    anchor: "bottom_right",
    monitor: null,
    dashboard_pinned: false,
    custom: null,
    idle_mode: "full",
    minified: false,
  },
  hud_view: "full",
  hud_monitors: [
    { name: "\\\\.\\DISPLAY1", label: "Display 1 · 1920×1080", is_primary: true, active: true, width: 1920, height: 1080 },
    { name: "\\\\.\\DISPLAY2", label: "Display 2 · 2560×1440", is_primary: false, active: false, width: 2560, height: 1440 },
  ],
  strava: {
    has_credentials: true,
    connected: true,
    athlete_name: "Preview Athlete",
    auto_sync: true,
    daily_rollup: false,
    shift_end: "18:00",
    syncing: false,
    activity: { state: "idle" },
    uploading_session_ids: [],
    upload_errors: {},
    saved_session_ids: [],
    last_result: { ok: true, message: "Uploaded 1.72 km to Strava (activity 123)", at_ms: Date.now() - 3600_000, hint: "none" },
    needs_reconnect: false,
    gear: [
      { id: "g1001", name: "Hoka Clifton 9", distance_km: 312.4, primary: true, retired: false },
      { id: "g1002", name: "Brooks Ghost 15 (long name to test the select width)", distance_km: 88.1, primary: false, retired: false },
      { id: "g1003", name: "Old Saucony", distance_km: 701.9, primary: false, retired: true },
    ],
    gear_id: "g1001",
    gear_loading: false,
    mute_mode: "all",
    mute_under_minutes: 20,
    title_template: DEFAULT_TITLE,
    rollup_title_template: DEFAULT_ROLLUP_TITLE,
    description_template: DEFAULT_DESCRIPTION,
  },
  maintenance: {
    last_service_ms: Date.now() - 38 * 86_400_000,
    distance_since_km: 84.5,
    interval_km: 200,
    basis: "distance",
    hours_since: 31.4,
    interval_hours: 50,
    days_since: 38,
    interval_days: 90,
    due: false,
    progress: 0.42,
  },
  weekly: {
    this_week: { distance_km: 12.4, duration_s: 11_460, active_days: 3, sessions: 4, calories: 740, longest_day_km: 5.1 },
    last_week: { distance_km: 31.6, duration_s: 29_700, active_days: 5, sessions: 9, calories: 1_890, longest_day_km: 8.2 },
    week_before: { distance_km: 28.2, duration_s: 26_100, active_days: 4, sessions: 7, calories: 1_650, longest_day_km: 7.4 },
    last_week_id: "2026-W45",
    distance_change: { pct: 12, is_new: false },
    show_card: false,
  },
  desk_sync: mockDeskSync(),
  seated: {
    enabled: true,
    minutes: 30,
    program_ids: [1, 2, 3],
    custom_programs: false,
    show_card: false,
    seated_minutes: 60,
    pending_program_id: null,
  },
  eye_break: {
    enabled: false,
    minutes: 20,
    seconds: 90,
    sound: true,
    pause_belt: true,
    fit_rests: true,
    holds_belt: false,
    at_rest: false,
    phase: "idle",
    remaining_s: 0,
    blink_seq: 0,
    end_seq: 0,
  },
  calls: { enabled: true, quiet_kmh: 2, mic_in_use: false, quiet_active: false },
  odometer: { distance_km: 412.6, belt_hours: 138.2, since_ms: Date.UTC(2026, 2, 12) },
  walkaway: { lock_enabled: true, idle_enabled: true, idle_minutes: 10, card: "none", pause_in_s: 0, reason: null },
  lights_out: { enabled: false },
  update: {
    current_version: "1.2.0",
    status: { state: "up_to_date" },
    offer: null,
    dismissed: false,
    dot: false,
    hud_card: false,
    notes_warning: null,
    show_notes_at_ms: null,
    last_checked_ms: Date.now() - 3 * 3_600_000,
    last_result: null,
    whats_new: null,
    auto_install: true,
  },
  traffic: { recording: false, file_path: null, file_name: null, packets: 0, bytes: 0, elapsed_s: 0, stop_reason: null },
  // Same limits as src-tauri/src/calibration.rs.
  calibration: {
    wizard_enabled: false,
    limits: {
      min_stride_cm: 20, max_stride_cm: 200, min_scale: 0.2, max_scale: 3, min_belt_cm: 50, max_belt_cm: 1000,
      min_run_s: 15, min_steps: 10, default_targets_kmh: [2, 3, 4, 5, 6],
    },
    run: null,
  },
  ghost_pod: {
    enabled: false,
    status: "off",
    label: "OFFLINE",
    message: "Switch it on to let a watch read the belt as a foot pod.",
    watches: 0,
    speed_kmh: 0,
  },
};

/** Send the current snapshot to every listener, as Rust's `app-state` event does. */
function publish() {
  snapshot.hud_view = hudView();
  snapshot.update.dot = updateDot();
  void emit("app-state", structuredClone(snapshot));
}

// The HUD during stasis, as `snapshot/stasis.rs` decides it: the whole panel
// while the belt moves, for 10 s after it stops, while a program waits for GO,
// and after expand until the belt next moves. Minified, the running strip
// takes the whole panel's place (a program waiting for GO still gets it).
const STASIS_GRACE_MS = 10_000;
let lastMovingAt: number | null = null;
let hudExpanded = false;
function hudView(): AppSnapshot["hud_view"] {
  const moving = snapshot.connection.connected && (snapshot.deck.status === "Running" || snapshot.deck.status === "Starting");
  if (moving) {
    lastMovingAt = Date.now();
    hudExpanded = false;
  }
  const mode = snapshot.hud.idle_mode;
  const a = snapshot.active_program;
  const inGrace = lastMovingAt !== null && Date.now() - lastMovingAt < STASIS_GRACE_MS;
  if (a?.wait) return "full";
  if (moving || mode === "full" || hudExpanded || inGrace) return snapshot.hud.minified ? "minified" : "full";
  return mode;
}

/** Rust finishing something in the background: apply `fn` after `ms`, then publish. */
function later(ms: number, fn: () => void) {
  setTimeout(() => {
    fn();
    publish();
  }, ms);
}

/** What Rust reports when a program starts. */
function startProgram(p: Program): ActiveProgram {
  const first = p.steps[0];
  return {
    program_id: p.id,
    name: p.name,
    phase: "walking",
    step_index: 0,
    step_count: p.steps.length,
    seconds_remaining: first.duration_minutes != null ? first.duration_minutes * 60 : null,
    cycle: 1,
    paused: false,
    wait: null,
    speed_kmh: first.speed_kmh,
    incline_pct: first.incline_pct,
  };
}

const params = new URLSearchParams(location.search);
const anchor = params.get("anchor");
if (anchor) snapshot.hud.anchor = anchor as AppSnapshot["hud"]["anchor"];
if (params.has("onemonitor")) snapshot.hud_monitors = snapshot.hud_monitors.slice(0, 1);
if (params.has("idle")) snapshot.deck.status = "Stopped";
// ?offline: no treadmill saved yet (the HUD's link label is not clickable).
if (params.has("offline")) Object.assign(snapshot.connection, { connected: false, has_saved_device: false, link: { state: "idle" } });
if (params.has("noprogram")) snapshot.active_program = null;
if (params.has("sim")) Object.assign(snapshot.connection, { simulated: true, device_id: "SIM_DECK" });
// ?daily=12.34: today's total (DAILY_TOTAL) in km, to check long values fit.
const dailyFlag = Number(params.get("daily"));
if (params.has("daily") && Number.isFinite(dailyFlag)) snapshot.session.daily_distance_m = Math.round(dailyFlag * 1000);
// ?program=ready: Workday's rest is over on a still belt, and cycle 4 waits for GO
// (REST COMPLETE // PRESS GO). ?program=hold: a walking step after a 0 km/h step waits.
// ?program=still: the belt was found still after a reconnect. ?program=resting: mid-rest.
// ?program=paused: paused by the user (plain GO, no wait).
const programFlag = params.get("program");
const PROGRAM_SCENARIOS: Record<string, Partial<ActiveProgram>> = {
  ready: { cycle: 4, paused: true, wait: "rest_complete", seconds_remaining: 300 },
  hold: { step_index: 1, step_count: 3, paused: true, wait: "hold_complete", seconds_remaining: 120 },
  still: { paused: true, wait: "belt_still" },
  resting: { phase: "resting", seconds_remaining: 1294 },
  paused: { paused: true },
};
if (programFlag !== null && snapshot.active_program && PROGRAM_SCENARIOS[programFlag]) {
  Object.assign(snapshot.active_program, PROGRAM_SCENARIOS[programFlag]);
  if (programFlag !== "resting") Object.assign(snapshot.deck, { status: "Paused", speed_kmh: 0, raw_speed_kmh: 0, target_speed_kmh: 0 });
}
if (params.has("nostrava")) Object.assign(snapshot.strava, { has_credentials: false, connected: false, athlete_name: null, auto_sync: false, last_result: null });
// ?reconnect: a login from before shoes and the duplicate check, so it needs a fresh grant.
if (params.has("reconnect")) Object.assign(snapshot.strava, { needs_reconnect: true, gear: [], gear_id: null });

// ?link=scanning|connecting|reconnecting|nodata|bt-off|failed previews each treadmill link state.
// Countdowns tick down in the preview the way Rust's do.
const linkFlag = params.get("link");
const LAST_ERROR = { message: "Treadmill not found. Is it switched on?", at_ms: Date.now() - 4000 };
const LINK_SCENARIOS: Record<string, Partial<ConnectionSnapshot>> = {
  scanning: { link: { state: "scanning" } },
  connecting: { link: { state: "connecting", attempt: 2, of: 3 } },
  reconnecting: { link: { state: "reconnecting", attempt: 3, next_retry_s: 12 }, last_error: LAST_ERROR },
  nodata: { link: { state: "no_data", since_s: 14 } },
  "bt-off": {
    link: { state: "adapter_unavailable" },
    last_error: { message: "Bluetooth is off. Turn it on in Windows settings.", at_ms: Date.now() },
  },
  failed: { link: { state: "failed", message: LAST_ERROR.message }, last_error: LAST_ERROR },
};
if (linkFlag) {
  const flags = { connected: linkFlag === "nodata", connecting: linkFlag === "connecting" };
  Object.assign(snapshot.connection, flags, LINK_SCENARIOS[linkFlag]);
}

// ?strava=authorizing|uploading|processing|checking|backoff|error|paused previews each Strava state;
// ?saved marks a walk as saved for Strava; ?reject makes every command fail; ?scan=empty|error|bt-off
// and ?connect=fail shape the connect dialog.
const stravaFlag = params.get("strava");
const INACTIVE_MESSAGE =
  "Strava turned off this API app: the account that owns it needs a Strava subscription. Subscribe, then reactivate it at strava.com/settings/api \u2014 or use Save for Strava below.";
const OFFLINE_MESSAGE = "Can't reach Strava. Check your internet connection.";
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
if (stravaFlag) Object.assign(snapshot.strava, STRAVA_SCENARIOS[stravaFlag]);
const rejecting = params.has("reject");
const scanFlag = params.get("scan");
const connectFails = params.get("connect") === "fail";

/** Count the preview's countdowns down once a second, like the Rust ones. */
function tickCountdowns(): boolean {
  let changed = false;
  const c = snapshot.connection;
  if (c.link.state === "reconnecting") {
    c.link = { ...c.link, next_retry_s: c.link.next_retry_s > 1 ? c.link.next_retry_s - 1 : 15 };
    changed = true;
  } else if (c.link.state === "no_data") {
    c.link = { state: "no_data", since_s: c.link.since_s + 1 };
    changed = true;
  }
  const a = snapshot.strava.activity;
  if (a.state === "authorizing") {
    snapshot.strava.activity = { state: "authorizing", expires_in_s: Math.max(0, a.expires_in_s - 1) };
    changed = true;
  } else if (a.state === "backing_off") {
    snapshot.strava.activity = { ...a, retry_in_s: Math.max(0, a.retry_in_s - 1) };
    changed = true;
  }
  return changed;
}

// ?desksync=open: today is a desk day not walked yet (left out of the count).
// ?desksync=new: nothing recorded yet. ?desksync=few: only three desk days so far.
const deskSyncFlag = params.get("desksync");
if (deskSyncFlag === "open") {
  const days = snapshot.desk_sync.days.slice(0, -1);
  Object.assign(snapshot.desk_sync, { days, walked: days.filter((x) => x.walked).length, desk_days: days.length, today_open: true });
} else if (deskSyncFlag === "new") {
  Object.assign(snapshot.desk_sync, { days: [], walked: 0, desk_days: 0, today_open: false });
} else if (deskSyncFlag === "few") {
  const days = snapshot.desk_sync.days.slice(-3);
  Object.assign(snapshot.desk_sync, { days, walked: days.filter((x) => x.walked).length, desk_days: days.length });
}

// ?weekly shows the last-week HUD card. Variants: ?weekly=long (123.4 km, 12h 05m,
// +1234%), ?weekly=new (nothing the week before), ?weekly=down.
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

// ?eyebreak=upcoming|break|resume shows the eye-break (OCULAR_IMPLANT) HUD card: the
// heads-up, the cooldown with the belt it paused, or the resume card after it
// (RESUME / STAY STILL). ?eyebreak=still is the cooldown with the belt left alone,
// ?eyebreak=rest the heads-up for a break set for the program's rest.
const eyeBreakFlag = params.get("eyebreak");
if (eyeBreakFlag === "upcoming" || eyeBreakFlag === "rest")
  Object.assign(snapshot.eye_break, {
    enabled: true,
    phase: "upcoming",
    remaining_s: 48,
    at_rest: eyeBreakFlag === "rest",
  });
if (eyeBreakFlag === "break" || eyeBreakFlag === "still")
  Object.assign(snapshot.eye_break, { enabled: true, phase: "break", remaining_s: 72, holds_belt: eyeBreakFlag === "break" });
if (eyeBreakFlag === "resume") Object.assign(snapshot.eye_break, { enabled: true, phase: "complete", holds_belt: true });
if (eyeBreakFlag === "break" || eyeBreakFlag === "resume") {
  // The break paused the walk: the program at its step, or the treadmill's pause.
  if (snapshot.active_program) Object.assign(snapshot.active_program, { paused: true, wait: null });
  Object.assign(snapshot.deck, { status: "Paused", speed_kmh: 0 });
}

// ?seated shows the idle-nudge HUD card (STASIS_ALERT). Variants, comma-separated: off (treadmill off,
// nothing picked yet), queued (treadmill off, second program remembered), long (long
// program names, to check wrapping), none / one (no or one saved program), many (six
// saved programs with long names; the card shows the first three).
const seatedFlags = new Set((params.get("seated") ?? "").split(","));
if (params.has("seated")) {
  Object.assign(snapshot.seated, { show_card: true, seated_minutes: 63 });
  if (seatedFlags.has("off") || seatedFlags.has("queued")) snapshot.connection.connected = false;
  if (seatedFlags.has("queued")) snapshot.seated.pending_program_id = 2;
  if (seatedFlags.has("long")) {
    ["Long afternoon recovery walk", "Standing desk intervals", "Post-lunch"].forEach((name, i) => {
      snapshot.programs[i].name = name;
    });
  }
  if (seatedFlags.has("none")) snapshot.programs = [];
  if (seatedFlags.has("one")) snapshot.programs = snapshot.programs.slice(0, 1);
  if (seatedFlags.has("many")) {
    const names = ["Long afternoon recovery walk", "Standing desk intervals", "Post-lunch digestion stroll", "Hill", "Endurance base builder", "Wind-down"];
    snapshot.programs = names.map((name, i) => ({ ...snapshot.programs[i % snapshot.programs.length], id: i + 1, name }));
  }
}

// ?call: on a call, the belt held at the quiet speed (the HUD preview also shows the toast).
if (params.has("call")) {
  Object.assign(snapshot.calls, { mic_in_use: true, quiet_active: true });
  Object.assign(snapshot.deck, { speed_kmh: 2, raw_speed_kmh: 2.2, target_speed_kmh: 2 });
}

// ?neural=active: Neural flow is easing the belt off while you type.
if (params.get("neural") === "active") {
  Object.assign(snapshot.neural, { active: true, factor: 0.94, base_kmh: 3.5, target_kmh: 3.3, keys_per_min: 118 });
  Object.assign(snapshot.deck, { speed_kmh: 3.3, target_speed_kmh: 3.3 });
}

// ?engage=timeout: the deck's screen is off. It shows the last Engage's failure, and
// Engage waits 8 s and fails again. ?status=N: the deck sent an unknown status byte N.
const engageTimeout = params.get("engage") === "timeout";
const START_NO_RESPONSE =
  "The treadmill didn't respond. If its screen is off, wake it with its power button or remote, then try again.";
const startFailed = (at_ms: number) => ({
  start_result: { ok: false, message: START_NO_RESPONSE, at_ms },
  last_error: { message: START_NO_RESPONSE, at_ms },
});
if (engageTimeout) {
  Object.assign(snapshot.deck, { status: "Stopped", speed_kmh: 0, raw_speed_kmh: 0 });
  snapshot.active_program = null;
  Object.assign(snapshot.connection, startFailed(Date.now() - 2000));
}
// ?status=countdown: the deck's 3-2-1 before the belt moves. ?status=stopping: the
// belt winding down after Dump (Engage is refused until it stops). ?status=standby:
// the deck is off or asleep (Engage is refused at once).
const statusFlag = params.get("status");
const START_SPINNING_DOWN = "Belt is still stopping. Press Engage once it stops.";
const START_STANDBY = "Treadmill is off or asleep. Switch it on, then press Engage.";
if (statusFlag === "countdown") {
  Object.assign(snapshot.deck, { status: "Starting", speed_kmh: 0, raw_speed_kmh: 0 });
} else if (statusFlag === "stopping") {
  Object.assign(snapshot.deck, { status: "Stopping", speed_kmh: 0.5, raw_speed_kmh: 0.5, target_speed_kmh: 0 });
  snapshot.active_program = null;
} else if (statusFlag === "standby") {
  Object.assign(snapshot.deck, { status: "Stopped", standby: true, speed_kmh: 0, raw_speed_kmh: 0 });
  snapshot.active_program = null;
} else if (statusFlag !== null) {
  snapshot.deck.status = "Stopped";
  snapshot.deck.unknown_status_code = Number(statusFlag) || 7;
}
/** ?status=stopping: Engage while the belt spins down is refused at once. */
function refuseWhileStopping() {
  const message = statusFlag === "standby" ? START_STANDBY : START_SPINNING_DOWN;
  snapshot.connection.start_result = { ok: false, message, at_ms: Date.now() };
}

// ?walkaway=warning: the idle warning counting down (STAY cancels it, reaching 0
// pauses). ?walkaway=paused (or =lock): the belt was paused because the PC was
// locked; ?walkaway=idle: paused after no input.
// ?lightsout: LIGHTS_OUT switched on (System -> Desk).
if (params.has("lightsout")) snapshot.lights_out.enabled = true;

const walkawayFlag = params.get("walkaway");
if (walkawayFlag === "warning") {
  Object.assign(snapshot.walkaway, { card: "warning", pause_in_s: 30 });
} else if (walkawayFlag === "paused" || walkawayFlag === "lock" || walkawayFlag === "idle") {
  Object.assign(snapshot.walkaway, { card: "paused", reason: walkawayFlag === "idle" ? "idle" : "lock" });
  snapshot.deck.status = "Paused";
}

/** One second of the walk-away warning (Rust recomputes `pause_in_s` each snapshot). */
function tickWalkaway(): boolean {
  const w = snapshot.walkaway;
  if (w.card !== "warning") return false;
  if (w.pause_in_s > 1) {
    w.pause_in_s -= 1;
  } else {
    Object.assign(w, { card: "paused", reason: "idle", pause_in_s: 0 });
    snapshot.deck.status = "Paused";
  }
  return true;
}

// Auto-update. ?update=available: v1.3.0 is on offer (notes for it and the
// skipped v1.2.1; the HUD shows UPDATE READY). ?update=downloading: it is
// downloading. ?update=failed: the last automatic check failed (offline).
// ?update=whatsnew: the first launch after updating to v1.2.0 ("What's new").
// ?autoinstall=off: "Install automatically" switched off.
const PREVIEW_NOTES = [
  "## v1.3.0",
  "",
  "### New",
  "",
  "- **Auto-update**: Neon Strider checks GitHub when it starts and once a day, shows the release notes, and",
  "  installs and restarts with one click. The walk carries on.",
  "- `NEON STRIDER // WHAT'S NEW` after each update.",
  "",
  "### Fixed",
  "",
  "- The HUD's link label reconnects when clicked.",
  "",
  "## v1.2.1",
  "",
  "### Fixed",
  "",
  "- Tilt calibration applies at once, without a reconnect.",
].join("\n");

/** The "update waiting" dot, as `update::show_update_dot` decides it. */
function updateDot(): boolean {
  return snapshot.update.status.state === "available" && !snapshot.update.dismissed;
}

function offerUpdate(dismissed = false) {
  const offer = { version: "1.3.0", notes: PREVIEW_NOTES, published: "2026-11-20T09:30:00Z" };
  Object.assign(snapshot.update, {
    status: { state: "available", ...offer },
    offer,
    dismissed,
    dot: !dismissed,
    hud_card: !dismissed,
  });
}

/** Rust's download progress, then the installer (which would close the app). */
function simulateInstall() {
  snapshot.update.status = { state: "downloading", percent: 0 };
  const step = () =>
    later(400, () => {
      const s = snapshot.update.status;
      if (s.state !== "downloading") return;
      const percent = Math.min(100, (s.percent ?? 0) + 17);
      snapshot.update.status = percent >= 100 ? { state: "installing" } : { state: "downloading", percent };
      if (percent < 100) step();
    });
  step();
}

const updateFlag = params.get("update");
if (params.get("autoinstall") === "off") snapshot.update.auto_install = false;
if (updateFlag === "available") offerUpdate();
if (updateFlag === "downloading") {
  offerUpdate();
  snapshot.update.hud_card = false;
  simulateInstall();
}
if (updateFlag === "failed") {
  snapshot.update.status = {
    state: "failed",
    message: "Can't reach GitHub. Check the internet connection, then try again.",
  };
}
if (updateFlag === "whatsnew") {
  snapshot.update.whats_new = {
    version: "1.2.0",
    notes: "## v1.2.0\n\n### New\n\n- **Walk-away safety**: locking the PC pauses the belt.\n- **Drag the HUD** by its NEON STRIDER logo.\n\n### Fixed\n\n- Engage now says when the treadmill's screen is off.",
  };
}

// ?stasis=strip|hidden: "When the belt is still" set to that, with the belt
// stopped and no program, so the HUD shows the strip or no panel at once.
// ?status=stopping keeps the belt winding down; ?deck=paused leaves it paused
// (Engage reads RE-JACK).
const stasisFlag = params.get("stasis");
if (stasisFlag === "strip" || stasisFlag === "hidden") {
  snapshot.hud.idle_mode = stasisFlag;
  const status = statusFlag === "stopping" ? "Stopping" : params.get("deck") === "paused" ? "Paused" : "Stopped";
  Object.assign(snapshot.deck, { status, speed_kmh: 0, raw_speed_kmh: 0 });
  snapshot.active_program = null;
}

// ?minified: the user minified the HUD, so it shows the running strip
// (combine with ?program=resting|paused, ?noprogram or ?idle).
if (params.has("minified")) snapshot.hud.minified = true;

// The Diagnostics traffic recorder. ?traffic=recording: a recording 42 s in
// (the packet count ticks). ?traffic=saved: one saved after reaching the
// 4-hour limit. Switching it on in the preview starts a fresh one.
const TRAFFIC_DIR = "C:/Users/you/AppData/Local/com.freshbreweddesign.neonstrider/logs/traffic";
let trafficTimer: ReturnType<typeof setInterval> | null = null;
function startTraffic(elapsed_s: number) {
  const file_name = "traffic-2026-11-15_09-30-12.jsonl";
  const packets = Math.round(elapsed_s * 3.3);
  Object.assign(snapshot.traffic, {
    recording: true,
    file_name,
    file_path: `${TRAFFIC_DIR}/${file_name}`,
    packets,
    bytes: packets * 105,
    elapsed_s,
    stop_reason: null,
  });
  if (trafficTimer) clearInterval(trafficTimer);
  trafficTimer = setInterval(() => {
    const t = snapshot.traffic;
    Object.assign(t, { elapsed_s: t.elapsed_s + 1, packets: t.packets + 3, bytes: t.bytes + 315 });
    publish();
  }, 1000);
}
function stopTraffic(stop_reason: string | null = null) {
  if (trafficTimer) clearInterval(trafficTimer);
  trafficTimer = null;
  Object.assign(snapshot.traffic, { recording: false, stop_reason });
}
const trafficFlag = params.get("traffic");
if (trafficFlag === "recording") startTraffic(42);
if (trafficFlag === "saved") {
  startTraffic(4 * 3600);
  stopTraffic("Stopped by itself at the 4-hour limit.");
}

// The guided calibration (Labs). ?calwizard: the switch is on, with two saved
// points. ?calwizard=counting: a count 23 s in (the clock ticks).
// ?calwizard=counted: a timed count waiting for the loops. The math is a rough
// stand-in for Rust's (calibration.rs, unit-tested there).
const calFlag = params.get("calwizard");
let calTimer: ReturnType<typeof setInterval> | null = null;
function calRun(target_kmh: number, phase: "settling" | "counting" | "counted", elapsed_s = 0) {
  if (calTimer) clearInterval(calTimer);
  calTimer = null;
  snapshot.calibration.run = {
    target_kmh, phase, elapsed_s, reported_kmh: phase === "counted" ? target_kmh : null, result: null,
  };
  if (phase === "counting") {
    calTimer = setInterval(() => {
      const r = snapshot.calibration.run;
      if (r?.phase === "counting") r.elapsed_s += 0.25;
      publish();
    }, 250);
  }
}
function closeCal() {
  if (calTimer) clearInterval(calTimer);
  calTimer = null;
  snapshot.calibration.run = null;
}
/** ?calwizard: two saved points and a 3 m belt loop. */
const CAL_CONFIG = {
  belt_length: 300,
  calibration_points: [
    { speed: 3, stride_length_cm: 68.4, distance_scale: 0.912 },
    { speed: 4.5, stride_length_cm: 74.1, distance_scale: 0.887 },
  ],
};
/** The preview runs no settings sync: the store settings ?calwizard starts with. */
export function previewSettings() {
  return calFlag === null ? null : { beltLength: CAL_CONFIG.belt_length, calibrationPoints: CAL_CONFIG.calibration_points };
}
if (calFlag !== null) {
  snapshot.calibration.wizard_enabled = true;
  if (calFlag === "counting" || calFlag === "counted") Object.assign(snapshot.deck, { raw_speed_kmh: 3, speed_kmh: 3.3 });
  if (calFlag === "counting") calRun(3, "counting", 23);
  if (calFlag === "counted") calRun(3, "counted", 62.4);
}
const CAL_COMMANDS: Partial<Record<Command["type"], (c: Command) => void>> = {
  calibration_set_wizard: (c) => {
    if (c.type !== "calibration_set_wizard") return;
    snapshot.calibration.wizard_enabled = c.on;
    if (!c.on) closeCal();
  },
  calibration_run_start: (c) => {
    if (c.type !== "calibration_run_start") return;
    Object.assign(snapshot.deck, { status: "Running", raw_speed_kmh: c.target_kmh, speed_kmh: c.target_kmh });
    calRun(c.target_kmh, "settling");
  },
  calibration_count_start: () => {
    const r = snapshot.calibration.run;
    if (!r) throw "Press RUN on a speed first.";
    if (snapshot.deck.raw_speed_kmh < 0.05) throw "The belt is not moving. Press RUN, then COUNT once it is up to speed.";
    calRun(r.target_kmh, "counting");
  },
  calibration_count_stop: () => {
    const r = snapshot.calibration.run;
    if (!r || r.phase !== "counting") throw "Press COUNT first.";
    if (r.elapsed_s < 15) throw `Keep counting: at least 15 s, so far ${r.elapsed_s.toFixed(0)} s.`;
    if (snapshot.deck.raw_speed_kmh < 0.05) {
      calRun(r.target_kmh, "settling");
      throw "The belt stopped during the count. Run it again.";
    }
    calRun(r.target_kmh, "counted", r.elapsed_s);
  },
  calibration_run_count: (c) => {
    const r = snapshot.calibration.run;
    if (c.type !== "calibration_run_count" || !r?.reported_kmh || c.count.method !== "loops") return;
    const { loops, belt_length_cm, steps } = c.count;
    if (!(loops > 0)) throw "Enter how many loops you counted.";
    const real_m = (loops * belt_length_cm) / 100;
    const reported_m = (r.reported_kmh / 3.6) * r.elapsed_s;
    const scale = reported_m / real_m;
    if (scale < 0.2 || scale > 3) throw `That gives a factor of ${scale.toFixed(2)}, outside 0.2 to 3. Check the count and run it again.`;
    r.result = {
      point: { speed: Math.round(r.reported_kmh * 10) / 10, stride_length_cm: steps ? (real_m / steps) * 100 : 74, distance_scale: scale },
      real_kmh: (real_m / r.elapsed_s) * 3.6,
      real_m,
      reported_m,
      cadence_spm: steps ? (steps / r.elapsed_s) * 60 : null,
      stride_measured: steps !== null,
    };
  },
  calibration_run_close: () => closeCal(),
};

// Labs: GHOST_POD. ?ghostpod=advertising|connected|error picks its status;
// switching it on in the preview advertises, then a watch connects 3 s later.
const GHOST_POD: Record<"off" | "starting" | "advertising" | "connected" | "error", [string, string]> = {
  off: ["OFFLINE", "Switch it on to let a watch read the belt as a foot pod."],
  starting: ["BOOTING", "Starting the pod."],
  advertising: ["BROADCASTING", "Waiting for a watch. Add a foot pod sensor on it; the pod shows under this PC's Bluetooth name."],
  connected: ["LINKED", "A watch is connected and gets the belt speed every second."],
  error: ["FAULT", "This PC's Bluetooth adapter can't act as a sensor (no peripheral role)."],
};
let ghostPodTimer: ReturnType<typeof setTimeout> | null = null;
function setGhostPod(status: keyof typeof GHOST_POD) {
  const [label, message] = GHOST_POD[status];
  Object.assign(snapshot.ghost_pod, {
    enabled: status !== "off",
    status,
    label,
    message,
    watches: status === "connected" ? 1 : 0,
    speed_kmh: status === "connected" ? snapshot.deck.speed_kmh : 0,
  });
}
function switchGhostPod(on: boolean) {
  if (ghostPodTimer) clearTimeout(ghostPodTimer);
  ghostPodTimer = null;
  setGhostPod(on ? "advertising" : "off");
  if (on) {
    ghostPodTimer = setTimeout(() => {
      setGhostPod("connected");
      publish();
    }, 3000);
  }
}
const ghostPodFlag = params.get("ghostpod");
if (ghostPodFlag && ghostPodFlag in GHOST_POD) setGhostPod(ghostPodFlag as keyof typeof GHOST_POD);

// ?custom: the HUD sits where it was dragged (a custom position).
if (params.has("custom")) snapshot.hud.custom = { dx: 400, top: 120 };

/**
 * A logo drag in the preview. Rust follows the real cursor across the desktop;
 * here the page can't move its window, so only the vertical part (the content's
 * top inside the strip) is simulated, with the same 24 px snapping to the top
 * and bottom corners.
 */
function simulateHudDrag(grabY: number) {
  const anchor = snapshot.hud.anchor;
  const right = anchor === "top_right" || anchor === "bottom_right";
  const content = document.querySelector(".hud-content");
  const height = content?.getBoundingClientRect().height ?? 200;
  const place = (clientY: number) => {
    const max = window.innerHeight - 8 - height;
    const top = Math.min(Math.max(clientY - grabY, 8), Math.max(max, 8));
    if (top - 8 <= 24) return { custom: null, anchor: right ? "top_right" : "top_left" } as const;
    if (max - top <= 24) return { custom: null, anchor: right ? "bottom_right" : "bottom_left" } as const;
    return { custom: { dx: 400, top }, anchor } as const;
  };
  let last: ReturnType<typeof place> | null = null;
  const move = (e: PointerEvent) => {
    last = place(e.clientY);
    // Mid-drag the content follows the pointer even over a snap zone, as the
    // real drag does (it snaps the frame, not just the drop).
    void emit("hud-drag", { custom: last.custom ?? { dx: 400, top: e.clientY - grabY }, anchor, done: false });
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    if (!last) return;
    snapshot.hud.custom = last.custom ? { ...last.custom } : null;
    snapshot.hud.anchor = last.anchor;
    void emit("hud-drag", { custom: snapshot.hud.custom, anchor: snapshot.hud.anchor, done: true });
    publish();
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

// ?hours: the belt reminder counts belt hours instead of distance.
if (params.has("hours")) Object.assign(snapshot.maintenance, { basis: "hours", hours_since: 38.2 });

/** The programs the card offers, as Rust resolves them. */
let seatedChosen: number[] | null = null;
function resolveSeatedPrograms() {
  const existing = snapshot.programs.map((p) => p.id);
  const picked = (seatedChosen ?? []).filter((id) => existing.includes(id)).slice(0, 3);
  snapshot.seated.program_ids = picked.length > 0 ? picked : existing.slice(0, 3);
  snapshot.seated.custom_programs = seatedChosen !== null;
}

/** Apply a maintenance change and recompute the derived fields (Rust does this for real). */
function setMaintenance(patch: Partial<MaintenanceSnapshot>) {
  const m = Object.assign(snapshot.maintenance, patch);
  m.days_since = Math.max(0, Math.floor((Date.now() - m.last_service_ms) / 86_400_000));
  const usage = m.basis === "hours" ? m.hours_since / m.interval_hours : m.distance_since_km / m.interval_km;
  const frac = Math.max(
    usage,
    m.interval_days > 0 ? (Date.now() - m.last_service_ms) / (m.interval_days * 86_400_000) : 0,
  );
  m.progress = frac;
  m.due = frac >= 1;
}
// ?due: well past the distance interval.
if (params.has("due")) setMaintenance({ last_service_ms: Date.now() - 95 * 86_400_000, distance_since_km: 212.4, hours_since: 52.6 });
resolveSeatedPrograms();

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
    });
  }
  return out;
}

function mockSamples(sessionId: number) {
  return Array.from({ length: 120 }, (_, i) => ({
    id: i, session_id: sessionId, timestamp: Date.now() - (120 - i) * 10000,
    speed: 3 + Math.sin(i / 10), raw_speed: 3.3, incline: 4 + (i > 60 ? 2 : 0),
    distance: i * 9, raw_distance: i * 9, steps: i * 13, raw_steps: i * 13,
  }));
}

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

const PREVIEW_CONFIG: RustConfig = {
  last_device_id: null,
  calibration_points: [],
  belt_length: 120,
  min_speed: 0.6,
  max_speed: 12,
  max_incline: 15,
  tilt_multiplier: 1.25,
  incline_offset: 0,
  focus_stability_enabled: true,
  body_mass_kg: 75,
  neural_floor_pct: 90,
  neural_sensitivity: "medium",
};

const sessions = mockSessions();
const lastId = sessions[sessions.length - 1].id;

/** ?engage=timeout: Engage on a stopped deck waits, then reports no answer. */
function engageWithTimeout() {
  if (snapshot.deck.status === "Running") return;
  Object.assign(snapshot.connection, { start_pending: true, start_result: null });
  later(8000, () => Object.assign(snapshot.connection, { start_pending: false }, startFailed(Date.now())));
}

const endWalkaway = { card: "none", reason: null } as const;
const setProgramPaused = (paused: boolean) => {
  if (!snapshot.active_program) return;
  // GO ends a wait and starts the belt; HOLD pauses it.
  Object.assign(snapshot.active_program, { paused, wait: null });
  Object.assign(snapshot.deck, paused ? { status: "Paused", speed_kmh: 0 } : { status: "Running", speed_kmh: snapshot.active_program.speed_kmh });
};

/** What each dispatched command does to the snapshot. Commands not listed only republish it. */
const COMMANDS: { [K in Command["type"]]?: (c: Extract<Command, { type: K }>) => void } = {
  // Engage / Stasis: a still belt starts (or a paused program goes), a moving one pauses.
  toggle_start: () => {
    const a = snapshot.active_program;
    if (a?.paused) return setProgramPaused(false);
    const moving = snapshot.deck.status === "Running" || snapshot.deck.status === "Starting";
    Object.assign(snapshot.deck, moving ? { status: "Paused", speed_kmh: 0 } : { status: "Running", speed_kmh: snapshot.deck.target_speed_kmh });
  },
  ...(engageTimeout ? { start: engageWithTimeout, toggle_start: engageWithTimeout } : {}),
  ...(statusFlag === "stopping" || statusFlag === "standby" ? { start: refuseWhileStopping, toggle_start: refuseWhileStopping } : {}),
  connect_now: () => {
    Object.assign(snapshot.connection, { link: { state: "connecting", attempt: 1, of: 3 }, connecting: true });
    later(1500, () =>
      Object.assign(snapshot.connection, { connected: true, connecting: false, link: { state: "connected" }, last_error: null }),
    );
  },
  open_connect_dialog: () => (snapshot.connection.connect_dialog_at_ms = Date.now()),
  strava_cancel_connect: () => (snapshot.strava.activity = { state: "idle" }),
  strava_retry_now: () => {
    snapshot.strava.activity = { state: "idle" };
    stravaSync("Nothing to upload yet");
  },
  strava_mark_uploaded: (c) => {
    snapshot.strava.saved_session_ids = snapshot.strava.saved_session_ids.filter((i) => !c.session_ids.includes(i));
    for (const sess of sessions) if (c.session_ids.includes(sess.id)) sess.strava_id = "manual";
  },
  program_run: (c) => {
    const p = snapshot.programs.find((x) => x.id === c.id);
    if (p) snapshot.active_program = startProgram(p);
  },
  program_save: (c) => {
    const i = snapshot.programs.findIndex((x) => x.id === c.program.id);
    // Notes are Rust's: whatever the editor sent is ignored.
    const program = { ...c.program, note: undefined };
    if (i >= 0) {
      // Like Rust: the note survives a rename, not a change to the plan.
      const old = snapshot.programs[i];
      const samePlan =
        JSON.stringify(old.steps) === JSON.stringify(program.steps) && old.repeat_every_minutes === program.repeat_every_minutes;
      snapshot.programs[i] = samePlan && old.note ? { ...program, note: old.note } : program;
    } else snapshot.programs.push({ ...program, id: Math.max(0, ...snapshot.programs.map((x) => x.id)) + 1 });
  },
  program_end: () => (snapshot.active_program = null),
  program_pause: () => setProgramPaused(true),
  program_resume: () => setProgramPaused(false),
  hud_toggle_widget: ({ widget }) => {
    const w = snapshot.hud.widgets;
    snapshot.hud.widgets = w.includes(widget) ? w.filter((x) => x !== widget) : [...w, widget];
  },
  hud_set_widgets: (c) => (snapshot.hud.widgets = c.widgets),
  hud_set_anchor: (c) => Object.assign(snapshot.hud, { anchor: c.anchor, custom: null }),
  hud_reset_position: () => (snapshot.hud.custom = null),
  hud_drag_start: (c) => simulateHudDrag(c.grab_y),
  hud_set_monitor: (c) => {
    snapshot.hud.monitor = c.monitor;
    for (const m of snapshot.hud_monitors) m.active = c.monitor === null ? m.is_primary : m.name === c.monitor;
  },
  dashboard_set_pinned: (c) => (snapshot.hud.dashboard_pinned = c.pinned),
  hud_set_idle_mode: (c) => (snapshot.hud.idle_mode = c.mode),
  hud_expand: () => {
    hudExpanded = true;
    snapshot.hud.minified = false;
  },
  hud_set_minified: (c) => (snapshot.hud.minified = c.minified),
  maintenance_mark_serviced: () => setMaintenance({ last_service_ms: Date.now(), distance_since_km: 0, hours_since: 0 }),
  maintenance_set_last_service: (c) => {
    if (c.at_ms > Date.now() || c.at_ms < Date.UTC(2020, 0, 1)) return;
    // Fake distance: about 5.6 km a day since the date.
    const days = (Date.now() - c.at_ms) / 86_400_000;
    setMaintenance({ last_service_ms: c.at_ms, distance_since_km: Math.round(days * 56) / 10, hours_since: Math.round(days * 17) / 10 });
  },
  maintenance_set_intervals: (c) => setMaintenance({ interval_km: c.km, interval_days: c.days }),
  maintenance_set_basis: (c) => setMaintenance({ basis: c.basis, interval_hours: c.hours }),
  weekly_summary_ack: () => (snapshot.weekly.show_card = false),
  seated_set_config: (c) => {
    seatedChosen = c.program_ids && c.program_ids.length > 0 ? c.program_ids : null;
    Object.assign(snapshot.seated, { enabled: c.enabled, minutes: c.minutes });
    resolveSeatedPrograms();
  },
  seated_start: (c) => {
    const p = snapshot.programs.find((x) => x.id === c.id);
    if (p && snapshot.connection.connected) {
      snapshot.seated.show_card = false;
      snapshot.active_program = startProgram(p);
    } else if (p) {
      snapshot.seated.pending_program_id = p.id;
    }
  },
  seated_engage: () => {
    if (!snapshot.connection.connected) throw "The treadmill is offline";
    if (snapshot.deck.standby) return refuseWhileStopping();
    Object.assign(snapshot.seated, { show_card: false, pending_program_id: null });
    Object.assign(snapshot.deck, { status: "Running", speed_kmh: snapshot.deck.target_speed_kmh });
  },
  seated_snooze: () => Object.assign(snapshot.seated, { show_card: false, pending_program_id: null }),
  eye_break_set_config: (c) => {
    const { enabled, minutes, seconds, sound, pause_belt, fit_rests } = c;
    Object.assign(snapshot.eye_break, { enabled, minutes, seconds, sound, pause_belt, fit_rests });
    if (!c.enabled) Object.assign(snapshot.eye_break, { phase: "idle", remaining_s: 0, holds_belt: false });
  },
  eye_break_skip: () =>
    Object.assign(snapshot.eye_break, {
      phase: snapshot.eye_break.holds_belt ? "complete" : "idle",
      remaining_s: 0,
    }),
  eye_break_resume: () => {
    if (!snapshot.connection.connected) throw "The treadmill is offline";
    Object.assign(snapshot.eye_break, { phase: "idle", holds_belt: false });
    if (snapshot.active_program) setProgramPaused(false);
    else snapshot.deck.status = "Running";
  },
  eye_break_stay_still: () => Object.assign(snapshot.eye_break, { phase: "idle", holds_belt: false }),
  calls_set_config: (c) => Object.assign(snapshot.calls, { enabled: c.enabled, quiet_kmh: c.quiet_kmh }),
  walkaway_set_config: (c) => {
    if (c.idle_minutes < 2 || c.idle_minutes > 30) throw "Idle time must be between 2 and 30 minutes";
    Object.assign(snapshot.walkaway, { lock_enabled: c.lock_enabled, idle_enabled: c.idle_enabled, idle_minutes: c.idle_minutes });
    if (!c.idle_enabled && snapshot.walkaway.card === "warning") snapshot.walkaway.card = "none";
  },
  lights_out_set_config: (c) => (snapshot.lights_out.enabled = c.enabled),
  walkaway_stay: () => Object.assign(snapshot.walkaway, { card: "none", pause_in_s: 0 }),
  walkaway_resume: () => {
    if (!snapshot.connection.connected) throw "The treadmill is offline";
    Object.assign(snapshot.walkaway, endWalkaway);
    snapshot.deck.status = "Running";
  },
  walkaway_end: () => {
    Object.assign(snapshot.walkaway, endWalkaway);
    snapshot.deck.status = "Stopped";
    snapshot.active_program = null;
  },
  update_check: () => {
    snapshot.update.status = { state: "checking" };
    later(1200, () => {
      const at_ms = Date.now();
      snapshot.update.last_checked_ms = at_ms;
      if (updateFlag === "failed") {
        const message = "Can't reach GitHub. Check the internet connection, then try again.";
        snapshot.update.status = { state: "failed", message };
        snapshot.update.last_result = { ok: false, message, at_ms };
      } else if (updateFlag === "available" || snapshot.update.offer) {
        offerUpdate(snapshot.update.dismissed);
        snapshot.update.last_result = { ok: true, message: "v1.3.0 is available.", at_ms };
      } else {
        snapshot.update.status = { state: "up_to_date" };
        snapshot.update.last_result = { ok: true, message: "Up to date: v1.2.0 is the newest.", at_ms };
      }
    });
  },
  update_install: () => {
    if (!snapshot.update.offer) throw "No update to install. Check for updates first.";
    simulateInstall();
  },
  update_dismiss: () => Object.assign(snapshot.update, { dismissed: true, hud_card: false }),
  update_hud_card: (c) => {
    snapshot.update.hud_card = false;
    if (c.open) snapshot.update.show_notes_at_ms = Date.now();
  },
  update_whats_new_ack: () => (snapshot.update.whats_new = null),
  update_auto_install: (c) => (snapshot.update.auto_install = c.on),
  traffic_record: (c) => (c.on ? startTraffic(0) : stopTraffic()),
  traffic_open_folder: () => {},
  ...CAL_COMMANDS,
  ghost_pod_set: (c) => switchGhostPod(c.on),
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
};

/** Resolve after a moment, the way a Bluetooth call takes a while. */
const slow = <T,>(fn: () => T) => new Promise((resolve) => setTimeout(resolve, 1500)).then(fn);

/** The app's named `invoke` calls besides `dispatch`. */
const QUERIES: Record<string, (args: Record<string, unknown>) => unknown> = {
  get_snapshot: () => {
    snapshot.hud_view = hudView();
    snapshot.update.dot = updateDot();
    return snapshot;
  },
  open_log_folder: () => null,
  scan_bluetooth_devices: () => {
    if (scanFlag === "error") throw { kind: "generic", message: "Could not reach the treadmill. Try again." };
    if (scanFlag === "bt-off") throw { kind: "adapter_off", message: "Bluetooth is off. Turn it on in Windows settings." };
    return slow(() => (scanFlag === "empty" ? [] : [{ id: "preview-deck", name: "FS-Preview" }]));
  },
  connect_bluetooth_device: () =>
    slow(() => {
      if (connectFails) throw { kind: "not_found", message: "Treadmill not found. Is it switched on?" };
      return null;
    }),
  strava_save_for_strava: (args) => {
    if (params.get("savefail") !== null) throw "No finished walk to save. Every walk is already on Strava, or one is still running.";
    const ids = args.sessionIds as number[] | null;
    snapshot.strava.saved_session_ids = [...new Set([...snapshot.strava.saved_session_ids, ...(ids ?? [lastId])])];
    publish();
    return { file_name: "2026-11-13 NEON STRIDER // 2.40 KM UPLINK.tcx", folder: "C:/Users/you/Downloads/Neon Strider" };
  },
  export_sessions_csv: () => "C:/Users/you/Downloads/neon-strider-sessions-preview.csv",
  get_sessions: () => sessions,
  get_samples: (args) => mockSamples(args.sessionId as number),
  strava_template_preview: (args) => {
    const line = (t: unknown, fallback: string) => previewTemplate(t as string, fallback).replace(/\s+/g, " ").slice(0, 100);
    return {
      title: line(args.title, DEFAULT_TITLE),
      rollup_title: line(args.rollupTitle, DEFAULT_ROLLUP_TITLE),
      description: previewTemplate(args.description as string, DEFAULT_DESCRIPTION),
    };
  },
  strava_tcx_preview: () => "<TrainingCenterDatabase><!-- preview --></TrainingCenterDatabase>",
  get_rust_config: () => (calFlag !== null ? { ...PREVIEW_CONFIG, ...CAL_CONFIG } : PREVIEW_CONFIG),
  dispatch: (args) => {
    if (rejecting) throw "Treadmill not connected";
    const command = args.command as Command;
    (COMMANDS[command.type] as ((c: Command) => void) | undefined)?.(command);
    publish();
    return null;
  },
};

/** Answers the app's `invoke` calls and applies dispatched commands to the snapshot. */
export function installMockBackend() {
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
  setInterval(() => {
    const walkawayTicked = tickWalkaway();
    const viewChanged = hudView() !== snapshot.hud_view;
    if (tickCountdowns() || walkawayTicked || viewChanged) publish();
  }, 1000);
  mockIPC((cmd, payload) => QUERIES[cmd]?.((payload ?? {}) as Record<string, unknown>) ?? null, { shouldMockEvents: true });
}
