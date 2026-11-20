//! The IPC types: the `AppSnapshot` every window reads and the `Command` every
//! window sends. ts-rs exports them to `src/ipc/bindings/`, doc comments
//! included, so the `///` text here is part of the generated TypeScript.

use super::{
    CallsSnapshot, DeskSync, LightsOutSnapshot, MaintenanceBasis, MaintenanceSnapshot,
    OdometerSnapshot, SeatedSnapshot, WalkAwaySnapshot, WeeklySummary,
};
use crate::link::{LastError, LinkState};
use crate::placement;
use crate::programs::{ActiveProgram, Program};
use crate::protocol::MachineStatus;
use crate::strava_status::{StravaActivity, StravaHint};
use serde::{Deserialize, Serialize};

// Fields are grouped by feature under `// ---- ... ----` markers: a new field
// goes at the end of its feature's group, or in a new group at the end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct AppSnapshot {
    // ---- The treadmill and the walk ----
    pub connection: ConnectionSnapshot,
    pub deck: DeckSnapshot,
    pub session: SessionSnapshot,

    // ---- Programs ----
    pub programs: Vec<Program>,
    pub active_program: Option<ActiveProgram>,

    // ---- Neural flow ----
    pub neural: NeuralSnapshot,

    // ---- HUD ----
    pub hud: HudConfig,
    /// How the HUD panel shows right now: in full, as a strip or not at all
    /// (`hud.idle_mode` while the belt is still; see `snapshot/stasis.rs`).
    pub hud_view: HudView,
    /// Displays the HUD can be placed on. Refreshed every few seconds, not per tick.
    pub hud_monitors: Vec<HudMonitor>,

    // ---- Strava ----
    pub strava: StravaSnapshot,

    // ---- Belt reminder and stats ----
    pub maintenance: MaintenanceSnapshot,
    /// Last week against the week before, and this week so far.
    pub weekly: WeeklySummary,
    /// DESK_SYNC: how many of the recent desk days (days the PC was used)
    /// were walked on.
    pub desk_sync: DeskSync,
    /// Distance and belt hours over every saved session plus the live one.
    pub odometer: OdometerSnapshot,

    // ---- Desk: idle nudge, eye breaks, calls, walk-away, LIGHTS_OUT ----
    /// The idle nudge: its settings and whether the HUD card shows.
    pub seated: SeatedSnapshot,
    /// Eye breaks: their settings, the phase and the tone counters.
    pub eye_break: crate::snapshot::EyeBreakSnapshot,
    /// Slow down on calls: its settings and whether the microphone is in use.
    pub calls: CallsSnapshot,
    /// Walk-away safety: its settings and which HUD card shows.
    pub walkaway: WalkAwaySnapshot,
    /// Lights out: stop the belt when the PC's display turns off.
    pub lights_out: LightsOutSnapshot,

    // ---- Auto-update ----
    /// Auto-update: the running version, any update on offer and its notes.
    pub update: crate::update::UpdateSnapshot,

    // ---- Diagnostics ----
    /// The Diagnostics traffic recorder (off at every start).
    pub traffic: crate::traffic::TrafficSnapshot,

    // ---- Labs ----
    /// The guided calibration (Labs): its switch, limits and any run.
    pub calibration: crate::calibration::CalibrationSnapshot,
    /// Labs: GHOST_POD, the PC as a foot pod for a watch (off by default).
    pub ghost_pod: crate::ghost_pod::GhostPodSnapshot,

    // ---- HEART_LINK ----
    pub heart_rate: crate::heart_rate::HeartRateSnapshot,

    // ---- Guided tasks ----
    /// The dashboard held open and on top during a guided task (GROUND_TRUTH).
    pub guided_task: crate::guided_task::GuidedTaskSnapshot,
}

/// Strava as the windows see it. Tokens and the client secret never appear here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct StravaSnapshot {
    /// A client ID and secret are saved.
    pub has_credentials: bool,
    /// An account is linked (a token is stored).
    pub connected: bool,
    pub athlete_name: Option<String>,
    pub auto_sync: bool,
    pub daily_rollup: bool,
    /// Local "HH:MM" after which a day is rolled up.
    pub shift_end: String,
    /// An upload is queued or running (`activity` says what exactly).
    pub syncing: bool,
    /// What the Strava worker is doing: authorizing, which upload step, backing off.
    pub activity: StravaActivity,
    /// Sessions in the upload that is running, for the History spinners.
    #[ts(type = "Array<number>")]
    pub uploading_session_ids: Vec<i64>,
    /// Why each session's last upload failed (session id to message). Cleared on
    /// success or when the upload is retried.
    #[ts(type = "Record<string, string>")]
    pub upload_errors: std::collections::BTreeMap<i64, String>,
    /// Walks saved as a TCX file for a manual Strava upload (not yet marked as on Strava).
    #[ts(type = "Array<number>")]
    pub saved_session_ids: Vec<i64>,
    pub last_result: Option<StravaResult>,
    /// The linked login predates a permission the app now asks for (shoes, duplicate check).
    pub needs_reconnect: bool,
    /// Cached shoes from the athlete's profile.
    pub gear: Vec<StravaGear>,
    /// Shoe applied to every upload; None = none.
    pub gear_id: Option<String>,
    /// The shoe list is being fetched.
    pub gear_loading: bool,
    pub mute_mode: MuteMode,
    /// Under this many minutes counts as short (used by `MuteMode::Short`).
    pub mute_under_minutes: u32,
    /// Templates in effect (a blank saved template shows its default).
    pub title_template: String,
    pub rollup_title_template: String,
    pub description_template: String,
}

/// Which uploads are hidden from the Strava home feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum MuteMode {
    All,
    Short,
    None,
}

/// One pair of shoes from the Strava profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct StravaGear {
    pub id: String,
    pub name: String,
    pub distance_km: f64,
    pub primary: bool,
    pub retired: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct StravaResult {
    pub ok: bool,
    pub message: String,
    /// Epoch ms; always increases, so a change means a new result.
    #[ts(type = "number")]
    pub at_ms: i64,
    /// What the person can do about a failure.
    pub hint: StravaHint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ConnectionSnapshot {
    /// The link is up (also true while `link` is `no_data`).
    pub connected: bool,
    pub connecting: bool,
    pub device_id: Option<String>,
    /// A treadmill is saved, so the watchdog keeps trying it and `ConnectNow`
    /// works. Without one, the HUD's link label sends `OpenConnectDialog`.
    pub has_saved_device: bool,
    /// What the link is doing; the fields above are kept for older readers.
    pub link: LinkState,
    /// The most recent link problem, in plain words.
    pub last_error: Option<LastError>,
    /// An Engage was sent and the deck hasn't reported Starting or Running yet.
    pub start_pending: bool,
    /// How the last Engage went; a failure says what to do. None while one is
    /// pending, and cleared once the deck's status changes (it woke up).
    pub start_result: Option<crate::link::StartResult>,
    /// When the HUD asked the dashboard to open its connect dialog (epoch ms);
    /// the dashboard opens it once per new value.
    #[ts(type = "number | null")]
    pub connect_dialog_at_ms: Option<i64>,
    /// The linked deck is the dev builds' simulated treadmill (the windows
    /// show a SIM tag). Always false in release builds.
    pub simulated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DeckSnapshot {
    pub status: MachineStatus,
    /// The deck sent a status byte we don't recognise (status then reads
    /// Stopped). Shown as "Treadmill state unknown (code N)".
    pub unknown_status_code: Option<u8>,
    /// The deck reports standby (off or asleep): it refuses starts.
    pub standby: bool,
    /// Calibrated belt speed.
    pub speed_kmh: f32,
    /// What the deck itself reports.
    pub raw_speed_kmh: f32,
    /// Commanded target (resolve_target), calibrated.
    pub target_speed_kmh: f32,
    pub incline_level: u8,
    pub incline_pct: f32,
    pub target_incline_level: u8,
    pub target_incline_pct: f32,
    /// Calibrated, like `max_speed_kmh`: the belt's floor in km/h.
    pub min_speed_kmh: f32,
    /// Calibrated.
    pub max_speed_kmh: f32,
    /// The highest incline level setting, or the deck's own top level once
    /// DECK_PROFILE has seen it, whichever is lower.
    pub max_incline_level: u8,
    pub max_incline_pct: f32,
    /// incline_pct = level * incline_pct_per_level + incline_offset_pct.
    pub incline_offset_pct: f32,
    pub incline_pct_per_level: f32,
    /// The deck changed or refused the last target ("asked X, got Y"), for a
    /// few seconds; the HUD shows it under the speed controls.
    pub target_note: Option<String>,
    /// The speeds the controls may offer, slowest first: every speed from
    /// the floor to the top that DECK_PROFILE does not know the deck skips.
    /// Empty until the deck has answered a target, and the controls then
    /// behave as before.
    pub speed_steps: Vec<DeckSpeedStep>,
    /// The incline levels the controls may offer, lowest first: flat up to
    /// the top level, without those DECK_PROFILE knows the deck skips. Empty
    /// until the deck has answered an incline.
    pub incline_steps: Vec<u8>,
    pub profile: DeckProfileSnapshot,
}

/// One speed the deck runs, or has not been asked yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DeckSpeedStep {
    /// The deck's own km/h (the hardware byte / 10), the unit of the top
    /// speed setting.
    pub hw_kmh: f32,
    /// Calibrated km/h, the unit of every other speed.
    pub kmh: f32,
}

/// DECK_PROFILE, for System -> Diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DeckProfileSnapshot {
    /// Speeds the deck has answered a target at.
    pub learned: u32,
    /// The deck's floor and ceiling, calibrated km/h, once seen.
    pub floor_kmh: Option<f32>,
    pub ceiling_kmh: Option<f32>,
    /// The same limits in the deck's own km/h, the unit of the min and max speed settings.
    pub floor_hw_kmh: Option<f32>,
    pub ceiling_hw_kmh: Option<f32>,
    /// Speeds asked that the deck changed or refused, calibrated km/h.
    pub skipped_kmh: Vec<f32>,
    /// Incline levels the deck has answered a target at.
    pub learned_levels: u32,
    /// The deck's top incline level, once seen.
    pub ceiling_level: Option<u8>,
    /// Incline levels asked that the deck changed or refused.
    pub skipped_levels: Vec<u8>,
    /// DECK_SWEEP, while it runs.
    pub sweep: Option<crate::profile_sweep::ProfileSweepView>,
    /// The DECK_SWEEP Labs switch.
    pub sweep_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SessionSnapshot {
    #[ts(type = "number | null")]
    pub active_id: Option<i64>,
    #[ts(type = "number")]
    pub duration_s: u64,
    pub distance_m: f64,
    #[ts(type = "number")]
    pub steps: u64,
    pub calories: f32,
    /// Today's saved sessions plus the live one.
    pub daily_distance_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct NeuralSnapshot {
    pub enabled: bool,
    /// Target / base: 1.0 while the belt is not being eased off.
    pub factor: f32,
    /// The belt is being eased off right now because of typing.
    pub active: bool,
    /// The speed set by hand or by the program step.
    pub base_kmh: f32,
    /// What Neural flow asks for (the base when not active).
    pub target_kmh: f32,
    /// Key presses per minute over the last 20 s (a count only).
    pub keys_per_min: f32,
    /// "Slowest speed", % of the base.
    pub floor_pct: f32,
    pub sensitivity: crate::neural::NeuralSensitivity,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HudConfig {
    pub widgets: Vec<HudWidget>,
    pub anchor: HudAnchor,
    /// Name of the display the HUD was put on, as the OS reported it; `None` is
    /// the primary display. Which display it is now is `HudMonitor::active`.
    pub monitor: Option<String>,
    pub dashboard_pinned: bool,
    /// That display's bounds when it was chosen, so it is still found after a
    /// driver update renames it. Rust-only.
    #[serde(skip)]
    #[ts(skip)]
    pub monitor_rect: Option<placement::Rect>,
    /// Dragged by its logo to a spot that is not a corner: `anchor` is then
    /// only what Reset goes back to. `None` = the `anchor` corner.
    pub custom: Option<placement::HudCustom>,
    /// What the panel does while the belt is still (paused, stopped or offline).
    pub idle_mode: HudIdleMode,
    /// Minified by the user (the HUD's minify button or Alt+Insert): the
    /// running strip instead of the whole panel, until expanded again.
    pub minified: bool,
}

/// The HUD setting "When the belt is still".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum HudIdleMode {
    /// The whole panel, as when the belt moves.
    #[default]
    Full,
    /// One small strip: link state, today's total, expand and Engage.
    Strip,
    /// No panel until the belt moves again (cards and toasts still show).
    Hidden,
}

/// How the HUD panel shows right now. Cards and toasts are not affected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum HudView {
    #[default]
    Full,
    /// The still-belt strip: today's total, expand and Engage.
    Strip,
    Hidden,
    /// Minified by the user: speed, time, distance, the program step, expand,
    /// Engage/Stasis and Dump.
    Minified,
}

impl HudConfig {
    /// The saved display, for `placement::choose_monitor`.
    pub fn saved_monitor(&self) -> Option<placement::SavedMonitor> {
        self.monitor.as_ref().map(|name| placement::SavedMonitor {
            name: Some(name.clone()),
            rect: self.monitor_rect,
        })
    }
}

/// One display, for the HUD page's selector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HudMonitor {
    /// The value to send back in `hud_set_monitor`.
    pub name: String,
    /// For example "Display 2 - 2560x1440".
    pub label: String,
    pub is_primary: bool,
    /// The HUD is on this display (the saved one as found, or the primary).
    pub active: bool,
    pub width: u32,
    pub height: u32,
}

// SCREAMING_SNAKE_CASE matches the names already stored in `pinned_elements`.
// Deserialisation goes through `HudWidgetRepr` so the earlier "AUTOMATION" name
// still loads (ts-rs can't parse `serde(alias)` and warns, so the alias lives there).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum HudWidget {
    Timer,
    Distance,
    Calories,
    Steps,
    Incline,
    Velocity,
    Neural,
    Programs,
    Daily,
    Controls,
    Odometer,
    HeartRate,
}

#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum HudWidgetRepr {
    Timer,
    Distance,
    Calories,
    Steps,
    Incline,
    Velocity,
    Neural,
    #[serde(alias = "AUTOMATION")]
    Programs,
    Daily,
    Controls,
    Odometer,
    HeartRate,
}

impl<'de> Deserialize<'de> for HudWidget {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        HudWidgetRepr::deserialize(d).map(Self::from)
    }
}

impl From<HudWidgetRepr> for HudWidget {
    fn from(r: HudWidgetRepr) -> Self {
        match r {
            HudWidgetRepr::Timer => Self::Timer,
            HudWidgetRepr::Distance => Self::Distance,
            HudWidgetRepr::Calories => Self::Calories,
            HudWidgetRepr::Steps => Self::Steps,
            HudWidgetRepr::Incline => Self::Incline,
            HudWidgetRepr::Velocity => Self::Velocity,
            HudWidgetRepr::Neural => Self::Neural,
            HudWidgetRepr::Programs => Self::Programs,
            HudWidgetRepr::Daily => Self::Daily,
            HudWidgetRepr::Controls => Self::Controls,
            HudWidgetRepr::Odometer => Self::Odometer,
            HudWidgetRepr::HeartRate => Self::HeartRate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum HudAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

// Variants are grouped by feature under `// ---- ... ----` markers, in the same
// order as the match in `commands.rs`: a new variant goes at the end of its
// feature's group, or in a new group at the end of both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum Command {
    // ---- Belt: start, stop, speed and incline ----
    Start,
    Pause,
    Stop,
    ToggleStart,
    NudgeSpeed {
        up: bool,
    },
    NudgeIncline {
        up: bool,
    },
    SetSpeed {
        kmh: f32,
    },
    SetInclineLevel {
        level: u8,
    },

    // ---- Treadmill link ----
    /// Try the saved treadmill now instead of waiting for the next retry.
    ConnectNow,
    /// Show the dashboard with its connect dialog open (the HUD's link label
    /// when no treadmill is saved yet).
    OpenConnectDialog,

    // ---- Programs ----
    ProgramSave {
        /// id 0 creates a new program.
        program: Program,
    },
    ProgramDelete {
        id: u32,
    },
    ProgramRun {
        id: u32,
    },
    ProgramPause,
    ProgramResume,
    ProgramSkip,
    ProgramEnd,

    // ---- HUD and dashboard windows ----
    HudToggleWidget {
        widget: HudWidget,
    },
    HudSetWidgets {
        widgets: Vec<HudWidget>,
    },
    /// Also drops a custom (dragged) position.
    HudSetAnchor {
        anchor: HudAnchor,
    },
    /// The HUD's logo was pressed and moved: Rust follows the cursor until the
    /// button is released. `grab_*` is where the logo was grabbed, relative to
    /// the content's top-left, in CSS px.
    HudDragStart {
        grab_x: f64,
        grab_y: f64,
    },
    /// Back to the corner anchor (double-click on the logo, or Reset).
    HudResetPosition,
    /// `None` puts the HUD on the primary display.
    HudSetMonitor {
        monitor: Option<String>,
    },
    DashboardSetPinned {
        pinned: bool,
    },
    /// What the HUD panel does while the belt is still.
    HudSetIdleMode {
        mode: HudIdleMode,
    },
    /// Show the whole panel until the belt next moves (the strip's expand
    /// button). Also ends a minify.
    HudExpand,
    /// Minify the HUD to the running strip, or expand it again (persisted).
    HudSetMinified {
        minified: bool,
    },

    // ---- Strava ----
    StravaSetCredentials {
        client_id: String,
        client_secret: String,
    },
    /// Open the browser to authorize; Rust receives and exchanges the code.
    StravaConnect,
    StravaDisconnect,
    StravaSetAutoSync {
        enabled: bool,
    },
    StravaSetDailyRollup {
        enabled: bool,
        /// Local "HH:MM".
        shift_end: String,
    },
    /// Upload exactly these sessions as one activity (queued; see `strava.last_result`).
    StravaUpload {
        #[ts(type = "Array<number>")]
        session_ids: Vec<i64>,
    },
    /// Run the auto-sync pass now.
    StravaSyncNow,
    /// Clear any back-off or pause and run the auto-sync pass now.
    StravaRetryNow,
    /// Stop waiting for the browser login.
    StravaCancelConnect,
    /// Open strava.com/settings/api in the browser.
    StravaOpenApiSettings,
    /// The user dropped these walks on Strava by hand: mark them as on Strava.
    StravaMarkUploaded {
        #[ts(type = "Array<number>")]
        session_ids: Vec<i64>,
    },
    /// Show the Downloads\Neon Strider folder.
    StravaOpenSaveFolder,
    /// Shoe to put on every upload; None = no shoe.
    StravaSetGear {
        gear_id: Option<String>,
    },
    /// Re-read the shoe list from Strava.
    StravaRefreshGear,
    /// Mute mode, and the minutes below which a walk counts as short.
    StravaSetMute {
        mode: MuteMode,
        under_minutes: u32,
    },
    /// Title and description templates; a blank one resets that template to its default.
    StravaSetTemplates {
        title: String,
        rollup_title: String,
        description: String,
    },

    // ---- Belt reminder (maintenance) ----
    /// The belt was just serviced.
    MaintenanceMarkServiced,
    /// Correct the last service date (epoch ms; not in the future, not before 2020).
    MaintenanceSetLastService {
        #[ts(type = "number")]
        at_ms: i64,
    },
    /// Distance interval 50 to 5000 km; days 0 to 730 (0 = no time limit).
    MaintenanceSetIntervals {
        km: f32,
        days: u32,
    },
    /// What the belt reminder counts (distance or belt hours) and the hours
    /// interval, 5 to 1000.
    MaintenanceSetBasis {
        basis: MaintenanceBasis,
        hours: f32,
    },

    // ---- Weekly summary ----
    /// The user dismissed the last-week card; remembered per ISO week.
    WeeklySummaryAck,

    // ---- Idle nudge (STASIS_ALERT) ----
    /// Idle nudge settings: on or off, minutes 15 to 240, and up to three
    /// programs for the card's buttons (`None` or empty = the first three saved).
    SeatedSetConfig {
        enabled: bool,
        minutes: u32,
        program_ids: Option<Vec<u32>>,
    },
    /// ENGAGE on the idle-nudge card: what Engage does on a still belt.
    SeatedEngage,
    /// A program button on the idle-nudge card.
    SeatedStart {
        id: u32,
    },
    /// SNOOZE 15 MIN on the card: hide it for 15 minutes; the idle time is kept.
    SeatedSnooze,

    // ---- Eye breaks (OCULAR_IMPLANT) ----
    /// Eye-break settings: on or off, minutes of screen use between breaks (10 to
    /// 120), seconds a break lasts (20 to 300), whether the HUD plays tones,
    /// whether a break pauses a walking belt and whether breaks fit into a
    /// running program's rests.
    EyeBreakSetConfig {
        enabled: bool,
        minutes: u32,
        seconds: u32,
        sound: bool,
        pause_belt: bool,
        fit_rests: bool,
    },
    /// SKIP on an eye-break card: no break now, and the count starts again. A
    /// belt the break paused stays paused, and the resume card shows.
    EyeBreakSkip,
    /// RESUME on the eye-break resume card: GO on the paused program, or the
    /// treadmill's resume on a manual walk. A press.
    EyeBreakResume,
    /// STAY STILL on the eye-break resume card: the card goes, the belt stays
    /// paused.
    EyeBreakStayStill,

    // ---- Slow down on calls ----
    /// Slow-down-on-calls settings: on or off, and the quiet speed 0.5 to 6 km/h.
    CallsSetConfig {
        enabled: bool,
        quiet_kmh: f32,
    },

    // ---- Walk-away safety ----
    /// Walk-away settings: pause on lock, and pause after no input for
    /// `idle_minutes` (2 to 30).
    WalkawaySetConfig {
        lock_enabled: bool,
        idle_enabled: bool,
        idle_minutes: u32,
    },
    /// STAY on the walk-away warning card.
    WalkawayStay,
    /// RESUME on the walk-away paused card.
    WalkawayResume,
    /// END on the walk-away paused card: stop and end the session.
    WalkawayEnd,

    // ---- LIGHTS_OUT ----
    /// Lights out: a full Stop (any program ends) when the PC's
    /// display turns off.
    LightsOutSetConfig {
        enabled: bool,
    },

    // ---- Auto-update ----
    /// Check for an update now (System -> Updates).
    UpdateCheck,
    /// Download, verify and install the offered update, then restart. A moving
    /// belt is paused first.
    UpdateInstall,
    /// "Not now": hide the offer for this version until the next daily check.
    UpdateDismiss,
    /// INSTALL (`open`) or LATER on the HUD's update card. INSTALL opens the
    /// dashboard on the notes; it never installs by itself.
    UpdateHudCard {
        open: bool,
    },
    /// The "What's new" notes were read.
    UpdateWhatsNewAck,
    /// Install an offered update by itself when the belt and the PC are idle
    /// (`on`), or only by hand.
    UpdateAutoInstall {
        on: bool,
    },

    // ---- Diagnostics: traffic recorder ----
    /// Diagnostics: record every packet to and from the treadmill to a new
    /// file (`on`), or stop. Sends nothing to the treadmill.
    TrafficRecord {
        on: bool,
    },
    /// Show the folder the traffic recordings are saved in.
    TrafficOpenFolder,

    // ---- DECK_PROFILE and DECK_SWEEP ----
    /// Diagnostics: forget the speeds DECK_PROFILE learned from the deck's
    /// replies. Sends nothing to the treadmill.
    DeckProfileForget,
    /// Labs: DECK_SWEEP steps the moving belt through every speed the profile
    /// has no reply for. A press; refused on a still belt or a program.
    DeckProfileSweepStart,
    /// End DECK_SWEEP. The belt keeps going.
    DeckProfileSweepStop,
    /// Labs: the DECK_SWEEP switch on or off (persisted, off by default). Off
    /// also ends a sweep; the belt keeps going.
    DeckProfileSweepSet {
        on: bool,
    },

    // ---- Guided calibration (Labs) ----
    /// Labs: the guided calibration wizard on or off (persisted, off by default).
    CalibrationSetWizard {
        on: bool,
    },
    /// RUN (or REPEAT) on a target speed: set the belt to `target_kmh` (deck
    /// km/h) and start it if it is still. The press is his.
    CalibrationRunStart {
        target_kmh: f32,
    },
    /// TAP: the belt mark passed now (the TAP button or Space in a window;
    /// Alt+Space anywhere comes through the input hook). Rust timestamps it;
    /// the point appears in `calibration.run.result` once the taps agree.
    /// Never moves the belt.
    CalibrationTap,
    /// TAP AGAIN: clear the run's taps and result. The belt keeps going.
    CalibrationTapReset,
    /// BELT_LOOP: the belt loop's length in cm, kept for this treadmill.
    CalibrationBeltSet {
        cm: f32,
    },
    /// Close the run (after saving its point, or to cancel). The belt keeps going.
    CalibrationRunClose,
    /// ADD_SPEED: put `kmh` (deck km/h, 0.1 steps, inside the treadmill's
    /// min/max, not already listed) in the stored target list.
    CalibrationTargetAdd {
        kmh: f32,
    },
    /// Take `kmh` out of the target list. Refused for a speed with a saved
    /// point or the open run's speed.
    CalibrationTargetRemove {
        kmh: f32,
    },
    /// RUN ALL: walk the stored target list and the saved points' speeds
    /// slowest first, running the first now. The press is his.
    CalibrationSweepStart,
    /// RUN on the sweep's current speed. The press is his.
    CalibrationSweepRun,
    /// SAVE (`saved`) or SKIP the sweep's current speed and close its run.
    /// The belt keeps its speed until the next RUN.
    CalibrationSweepNext {
        saved: bool,
    },
    /// End the sweep and any run. The belt keeps going.
    CalibrationSweepEnd,
    /// TAP STEPS (or TAP AGAIN on the steps): start the footstep phase on
    /// the measured run; TAP then times footsteps. Never moves the belt.
    CalibrationStepsStart,
    /// SKIP on the footsteps: the stride stays as it was.
    CalibrationStepsSkip,
    /// The window stored the footstep result's point (`run.steps.to_save`).
    CalibrationStepsSaved,
    /// UNDO on the stride: the window put back `run.steps.before`.
    CalibrationStepsUndo,

    // ---- GHOST_POD and WRIST_LINK (Labs) ----
    /// Labs: GHOST_POD on (`on`) or off. On, the PC advertises as a Bluetooth
    /// foot pod and sends the belt speed to a paired watch once a second.
    GhostPodSet {
        on: bool,
    },
    /// Labs: GHOST_POD's power switch. On, the pod also serves running power
    /// (the Cycling Power service) while GHOST_POD is on.
    GhostPodPowerSet {
        on: bool,
    },
    /// Labs: WRIST_LINK on (`on`) or off. On, the PC also serves the service
    /// the watch's Connect IQ data field reads the belt from. Needs GHOST_POD.
    WristLinkSet {
        on: bool,
    },

    // ---- HEART_LINK ----
    /// HEART_LINK: scan for heart rate sensors (about 5 s); the result is
    /// `heart_rate.found`.
    HeartRateScan,
    /// HEART_LINK: read the heart rate from this sensor (one from the scan).
    HeartRatePick {
        id: String,
        name: String,
    },
    /// HEART_LINK: stop reading and forget the sensor (the default).
    HeartRateForget,
    /// The maximum heart rate (bpm) that program heart rate zones are a
    /// percentage of.
    HeartRateMaxSet {
        bpm: u16,
    },

    // ---- Guided tasks ----
    /// Hold the dashboard open and on top while `task` runs (starting it again
    /// restarts its timeout).
    GuidedTaskStart {
        task: crate::guided_task::GuidedTask,
    },
    /// `task` is over; the dashboard unpins once no task is left.
    GuidedTaskEnd {
        task: crate::guided_task::GuidedTask,
    },
    /// UNPIN: end every guided task's pin now.
    GuidedTaskCancel,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::programs::{self, Program};

    #[test]
    fn command_deserialises_from_tagged_json() {
        let c: Command = serde_json::from_str(r#"{"type":"nudge_speed","up":true}"#).unwrap();
        assert_eq!(c, Command::NudgeSpeed { up: true });

        let c: Command = serde_json::from_str(r#"{"type":"toggle_start"}"#).unwrap();
        assert_eq!(c, Command::ToggleStart);

        let c: Command = serde_json::from_str(r#"{"type":"set_speed","kmh":4.5}"#).unwrap();
        assert_eq!(c, Command::SetSpeed { kmh: 4.5 });

        let c: Command =
            serde_json::from_str(r#"{"type":"hud_toggle_widget","widget":"TIMER"}"#).unwrap();
        assert_eq!(
            c,
            Command::HudToggleWidget {
                widget: HudWidget::Timer
            }
        );

        let c: Command =
            serde_json::from_str(r#"{"type":"hud_set_anchor","anchor":"top_left"}"#).unwrap();
        assert_eq!(
            c,
            Command::HudSetAnchor {
                anchor: HudAnchor::TopLeft
            }
        );

        let c: Command =
            serde_json::from_str(r#"{"type":"hud_set_monitor","monitor":"DISPLAY2"}"#).unwrap();
        assert_eq!(
            c,
            Command::HudSetMonitor {
                monitor: Some("DISPLAY2".to_string())
            }
        );
        let c: Command =
            serde_json::from_str(r#"{"type":"hud_set_monitor","monitor":null}"#).unwrap();
        assert_eq!(c, Command::HudSetMonitor { monitor: None });

        let c: Command =
            serde_json::from_str(r#"{"type":"hud_set_idle_mode","mode":"strip"}"#).unwrap();
        assert_eq!(
            c,
            Command::HudSetIdleMode {
                mode: HudIdleMode::Strip
            }
        );
        let c: Command = serde_json::from_str(r#"{"type":"hud_expand"}"#).unwrap();
        assert_eq!(c, Command::HudExpand);
        let c: Command =
            serde_json::from_str(r#"{"type":"hud_set_minified","minified":true}"#).unwrap();
        assert_eq!(c, Command::HudSetMinified { minified: true });
        assert!(
            serde_json::from_str::<Command>(r#"{"type":"hud_set_idle_mode","mode":"tiny"}"#)
                .is_err()
        );

        let c: Command = serde_json::from_str(
            r#"{"type":"program_save","program":{"id":0,"name":"A","steps":[{"duration_minutes":null,"speed_kmh":2.5,"incline_pct":5}],"repeat_every_minutes":null}}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::ProgramSave {
                program: Program {
                    id: 0,
                    name: "A".to_string(),
                    steps: vec![programs::ProgramStep {
                        duration_minutes: None,
                        speed_kmh: 2.5,
                        incline_pct: 5.0,
                        zone: None,
                    }],
                    repeat_every_minutes: None,
                    note: None
                }
            }
        );
        let c: Command = serde_json::from_str(r#"{"type":"program_run","id":3}"#).unwrap();
        assert_eq!(c, Command::ProgramRun { id: 3 });
        let c: Command = serde_json::from_str(r#"{"type":"program_skip"}"#).unwrap();
        assert_eq!(c, Command::ProgramSkip);
        assert!(serde_json::from_str::<Command>(r#"{"type":"automation_toggle"}"#).is_err());

        let c: Command = serde_json::from_str(
            r#"{"type":"strava_set_credentials","client_id":"1","client_secret":"s"}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::StravaSetCredentials {
                client_id: "1".to_string(),
                client_secret: "s".to_string()
            }
        );
        let c: Command = serde_json::from_str(
            r#"{"type":"strava_set_daily_rollup","enabled":true,"shift_end":"18:00"}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::StravaSetDailyRollup {
                enabled: true,
                shift_end: "18:00".to_string()
            }
        );
        let c: Command =
            serde_json::from_str(r#"{"type":"strava_upload","session_ids":[4,5]}"#).unwrap();
        assert_eq!(
            c,
            Command::StravaUpload {
                session_ids: vec![4, 5]
            }
        );
        let c: Command = serde_json::from_str(r#"{"type":"strava_sync_now"}"#).unwrap();
        assert_eq!(c, Command::StravaSyncNow);
        let c: Command =
            serde_json::from_str(r#"{"type":"strava_set_gear","gear_id":"g1"}"#).unwrap();
        assert_eq!(
            c,
            Command::StravaSetGear {
                gear_id: Some("g1".to_string())
            }
        );
        let c: Command =
            serde_json::from_str(r#"{"type":"strava_set_gear","gear_id":null}"#).unwrap();
        assert_eq!(c, Command::StravaSetGear { gear_id: None });
        let c: Command = serde_json::from_str(r#"{"type":"strava_refresh_gear"}"#).unwrap();
        assert_eq!(c, Command::StravaRefreshGear);
        let c: Command =
            serde_json::from_str(r#"{"type":"strava_set_mute","mode":"short","under_minutes":20}"#)
                .unwrap();
        assert_eq!(
            c,
            Command::StravaSetMute {
                mode: MuteMode::Short,
                under_minutes: 20
            }
        );
        assert!(serde_json::from_str::<Command>(
            r#"{"type":"strava_set_mute","mode":"most","under_minutes":20}"#
        )
        .is_err());
        let c: Command = serde_json::from_str(
            r#"{"type":"strava_set_templates","title":"a","rollup_title":"","description":"c"}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::StravaSetTemplates {
                title: "a".to_string(),
                rollup_title: String::new(),
                description: "c".to_string()
            }
        );

        let c: Command = serde_json::from_str(r#"{"type":"maintenance_mark_serviced"}"#).unwrap();
        assert_eq!(c, Command::MaintenanceMarkServiced);
        let c: Command = serde_json::from_str(
            r#"{"type":"maintenance_set_last_service","at_ms":1700000000000}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::MaintenanceSetLastService {
                at_ms: 1_700_000_000_000
            }
        );
        let c: Command =
            serde_json::from_str(r#"{"type":"maintenance_set_intervals","km":400.0,"days":60}"#)
                .unwrap();
        assert_eq!(
            c,
            Command::MaintenanceSetIntervals {
                km: 400.0,
                days: 60
            }
        );
        let c: Command = serde_json::from_str(r#"{"type":"weekly_summary_ack"}"#).unwrap();
        assert_eq!(c, Command::WeeklySummaryAck);
        let c: Command = serde_json::from_str(
            r#"{"type":"maintenance_set_basis","basis":"hours","hours":60.0}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::MaintenanceSetBasis {
                basis: MaintenanceBasis::Hours,
                hours: 60.0
            }
        );
        let c: Command = serde_json::from_str(
            r#"{"type":"seated_set_config","enabled":true,"minutes":45,"program_ids":[2,3]}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::SeatedSetConfig {
                enabled: true,
                minutes: 45,
                program_ids: Some(vec![2, 3])
            }
        );
        let c: Command = serde_json::from_str(
            r#"{"type":"seated_set_config","enabled":false,"minutes":60,"program_ids":null}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::SeatedSetConfig {
                enabled: false,
                minutes: 60,
                program_ids: None
            }
        );
        let c: Command = serde_json::from_str(r#"{"type":"seated_start","id":2}"#).unwrap();
        assert_eq!(c, Command::SeatedStart { id: 2 });
        let c: Command = serde_json::from_str(r#"{"type":"seated_snooze"}"#).unwrap();
        assert_eq!(c, Command::SeatedSnooze);
        let c: Command = serde_json::from_str(r#"{"type":"seated_engage"}"#).unwrap();
        assert_eq!(c, Command::SeatedEngage);
        let c: Command =
            serde_json::from_str(r#"{"type":"calls_set_config","enabled":true,"quiet_kmh":1.8}"#)
                .unwrap();
        assert_eq!(
            c,
            Command::CallsSetConfig {
                enabled: true,
                quiet_kmh: 1.8
            }
        );
        assert!(serde_json::from_str::<Command>(
            r#"{"type":"maintenance_set_basis","basis":"miles","hours":60.0}"#
        )
        .is_err());

        let c: Command = serde_json::from_str(r#"{"type":"update_check"}"#).unwrap();
        assert_eq!(c, Command::UpdateCheck);
        let c: Command = serde_json::from_str(r#"{"type":"update_install"}"#).unwrap();
        assert_eq!(c, Command::UpdateInstall);
        let c: Command = serde_json::from_str(r#"{"type":"update_dismiss"}"#).unwrap();
        assert_eq!(c, Command::UpdateDismiss);
        let c: Command = serde_json::from_str(r#"{"type":"open_connect_dialog"}"#).unwrap();
        assert_eq!(c, Command::OpenConnectDialog);
        let c: Command = serde_json::from_str(r#"{"type":"update_hud_card","open":true}"#).unwrap();
        assert_eq!(c, Command::UpdateHudCard { open: true });
        let c: Command = serde_json::from_str(r#"{"type":"update_whats_new_ack"}"#).unwrap();
        assert_eq!(c, Command::UpdateWhatsNewAck);
        let c: Command =
            serde_json::from_str(r#"{"type":"update_auto_install","on":false}"#).unwrap();
        assert_eq!(c, Command::UpdateAutoInstall { on: false });
        let c: Command = serde_json::from_str(r#"{"type":"traffic_record","on":true}"#).unwrap();
        assert_eq!(c, Command::TrafficRecord { on: true });
        let c: Command = serde_json::from_str(r#"{"type":"ghost_pod_set","on":true}"#).unwrap();
        assert_eq!(c, Command::GhostPodSet { on: true });
        let c: Command = serde_json::from_str(r#"{"type":"heart_rate_scan"}"#).unwrap();
        assert_eq!(c, Command::HeartRateScan);
        let c: Command =
            serde_json::from_str(r#"{"type":"heart_rate_pick","id":"A1","name":"Watch"}"#).unwrap();
        assert_eq!(
            c,
            Command::HeartRatePick {
                id: "A1".into(),
                name: "Watch".into()
            }
        );
        let c: Command = serde_json::from_str(r#"{"type":"heart_rate_forget"}"#).unwrap();
        assert_eq!(c, Command::HeartRateForget);
        let c: Command =
            serde_json::from_str(r#"{"type":"guided_task_start","task":"ground_truth"}"#).unwrap();
        assert_eq!(
            c,
            Command::GuidedTaskStart {
                task: crate::guided_task::GuidedTask::GroundTruth
            }
        );
        let c: Command =
            serde_json::from_str(r#"{"type":"guided_task_end","task":"ground_truth"}"#).unwrap();
        assert_eq!(
            c,
            Command::GuidedTaskEnd {
                task: crate::guided_task::GuidedTask::GroundTruth
            }
        );
        let c: Command = serde_json::from_str(r#"{"type":"guided_task_cancel"}"#).unwrap();
        assert_eq!(c, Command::GuidedTaskCancel);
        let c: Command =
            serde_json::from_str(r#"{"type":"ghost_pod_power_set","on":true}"#).unwrap();
        assert_eq!(c, Command::GhostPodPowerSet { on: true });
        let c: Command = serde_json::from_str(r#"{"type":"traffic_open_folder"}"#).unwrap();
        assert_eq!(c, Command::TrafficOpenFolder);

        assert!(serde_json::from_str::<Command>(r#"{"type":"explode"}"#).is_err());
        assert!(serde_json::from_str::<Command>(r#"{"type":"nudge_speed"}"#).is_err());
    }
}
