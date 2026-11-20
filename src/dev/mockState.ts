// The mocked backend's shared state: the snapshot every preview starts from, the
// URL flags, and `publish` (Rust's `app-state` event). The URL-flag scenarios
// that change it live in `scenarios/`, one file per feature.
import { emit } from "@tauri-apps/api/event";
import type { AppSnapshot, DeskSync } from "../ipc";
import type { RustConfig } from "../services/controlOrchestrator";

// Same defaults as src-tauri/src/strava_template.rs.
export const DEFAULT_TITLE = "NEON STRIDER // {distance_km} KM UPLINK";
export const DEFAULT_ROLLUP_TITLE = "NEON STRIDER // DAILY UPLINK · {walks} WALKS · {distance_km} KM";
export const DEFAULT_DESCRIPTION =
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
    target_note: null,
    target_incline_level: 4,
    target_incline_pct: 5.0,
    min_speed_kmh: 0.6,
    max_speed_kmh: 10.5,
    max_incline_level: 15,
    max_incline_pct: 18.75,
    speed_steps: [],
    incline_steps: [],
    profile: {
      learned: 0,
      floor_kmh: null,
      ceiling_kmh: null,
      floor_hw_kmh: null,
      ceiling_hw_kmh: null,
      skipped_kmh: [],
      learned_levels: 0,
      ceiling_level: null,
      skipped_levels: [],
      sweep: null,
      sweep_enabled: false,
    },
  },
  session: { active_id: 1, duration_s: 1934, distance_m: 1720, steps: 2412, calories: 103, daily_distance_m: 4310 },
  programs: [
    { id: 1, name: "Post-meal", steps: [{ duration_minutes: 10, speed_kmh: 4, incline_pct: 5 }], repeat_every_minutes: null, note: "Walking soon after a meal blunts the rise in blood sugar." },
    { id: 2, name: "Desk breaks", steps: [{ duration_minutes: 5, speed_kmh: 3, incline_pct: 5 }], repeat_every_minutes: 30, note: "Short easy walks on a timer break up long hours at the desk." },
    { id: 3, name: "Focus", steps: [{ duration_minutes: null, speed_kmh: 2, incline_pct: 0 }], repeat_every_minutes: null, note: "A slow, flat walk keeps typing and thinking steady." },
    { id: 5, name: "Brisk", steps: [{ duration_minutes: 30, speed_kmh: 5, incline_pct: 0 }], repeat_every_minutes: null, note: "Thirty brisk minutes count toward a weekly 150 of moderate activity." },
    { id: 6, name: "VO2 max", steps: [{ duration_minutes: 5, speed_kmh: 4.5, incline_pct: 0 }, { duration_minutes: 4, speed_kmh: 6, incline_pct: 10, zone: { low_pct: 85, high_pct: 95 } }, { duration_minutes: 3, speed_kmh: 4, incline_pct: 0 }, { duration_minutes: 4, speed_kmh: 6, incline_pct: 10, zone: { low_pct: 85, high_pct: 95 } }, { duration_minutes: 3, speed_kmh: 4, incline_pct: 0 }, { duration_minutes: 4, speed_kmh: 6, incline_pct: 10, zone: { low_pct: 85, high_pct: 95 } }, { duration_minutes: 3, speed_kmh: 4, incline_pct: 0 }, { duration_minutes: 4, speed_kmh: 6, incline_pct: 10, zone: { low_pct: 85, high_pct: 95 } }, { duration_minutes: 3, speed_kmh: 4, incline_pct: 0 }, { duration_minutes: 3, speed_kmh: 3, incline_pct: 0 }], repeat_every_minutes: null, note: "Four hard climbs of four minutes, split by easy walks, train VO2 max." },
    { id: 7, name: "Interval walk", steps: [{ duration_minutes: 5, speed_kmh: 4, incline_pct: 0 }, { duration_minutes: 3, speed_kmh: 6, incline_pct: 5 }, { duration_minutes: 3, speed_kmh: 3.5, incline_pct: 0 }, { duration_minutes: 3, speed_kmh: 6, incline_pct: 5 }, { duration_minutes: 3, speed_kmh: 3.5, incline_pct: 0 }, { duration_minutes: 3, speed_kmh: 6, incline_pct: 5 }, { duration_minutes: 3, speed_kmh: 3.5, incline_pct: 0 }, { duration_minutes: 3, speed_kmh: 6, incline_pct: 5 }, { duration_minutes: 3, speed_kmh: 3.5, incline_pct: 0 }, { duration_minutes: 3, speed_kmh: 6, incline_pct: 5 }, { duration_minutes: 3, speed_kmh: 3.5, incline_pct: 0 }], repeat_every_minutes: null, note: "Five rounds alternate three brisk minutes with three easy ones." },
  ],
  active_program: {
    program_id: 2,
    name: "Desk breaks",
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
    blink: null,
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
    offer: false,
    limits: {
      min_stride_cm: 20, max_stride_cm: 200, min_scale: 0.2, max_scale: 3, min_belt_cm: 50, max_belt_cm: 1000,
      min_agreeing: 8,
      min_steps: 20,
    },
    belt_cm: null,
    belt_treadmill: false,
    targets: [2, 3, 4, 5, 6].map((kmh) => ({ kmh, runnable: true })),
    points_rev: 0,
    run: null,
    sweep: null,
    guide: null,
  },
  guided_task: { pinned: false, tasks: [], unpin_in_s: null },
  ghost_pod: {
    enabled: false,
    status: "off",
    label: "OFFLINE",
    message: "Switch it on to let a watch read the belt as a foot pod.",
    watches: 0,
    speed_kmh: 0,
    power_enabled: false,
    power_watches: 0,
    power_w: 0,
    wrist_link: {
      enabled: false,
      serving: false,
      watches: 0,
      message: "Switch it on to let the watch's data field read the belt.",
    },
  },
  heart_rate: {
    sensor: null,
    status: "off",
    label: "OFFLINE",
    message: "Scan for a heart rate sensor and pick your watch to show heart rate.",
    bpm: null,
    history: [],
    zone: null,
    scanning: false,
    found: [],
    max_bpm: 190,
    max_bpm_set: false,
  },
};

export const params = new URLSearchParams(location.search);

/** What `get_rust_config` returns (?calwizard adds its saved points). */
export const PREVIEW_CONFIG: RustConfig = {
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

/** Send the current snapshot to every listener, as Rust's `app-state` event does. */
export function publish() {
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
export function hudView(): AppSnapshot["hud_view"] {
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

/** The strip's expand button: the whole panel until the belt next moves. */
export function expandHud() {
  hudExpanded = true;
}

/** Rust finishing something in the background: apply `fn` after `ms`, then publish. */
export function later(ms: number, fn: () => void) {
  setTimeout(() => {
    fn();
    publish();
  }, ms);
}

/** The "update waiting" dot, as `update::show_update_dot` decides it. */
export function updateDot(): boolean {
  return snapshot.update.status.state === "available" && !snapshot.update.dismissed;
}
