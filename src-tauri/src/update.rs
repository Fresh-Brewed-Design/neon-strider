//! Auto-update with release notes.
//!
//! Rust drives the whole thing; no window has updater permissions and the CSP
//! stays closed. Three parts:
//!
//! - the pure logic (tested below): version order, joining the notes of every
//!   version skipped since the installed one, the check schedule, "not now",
//!   and plain-English error text;
//! - the auto-install rule (`auto_install`, pure): an offered update installs
//!   by itself only when the belt has been still, with no program, for 10
//!   minutes and the PC has had no input for 5, checked again right before the
//!   installer runs;
//! - the runtime: checks (30 s after startup, then hourly, or on demand), the
//!   install (download and verify through `tauri-plugin-updater`, save the
//!   running program, run the NSIS installer in passive mode, which restarts
//!   the app). The belt is left alone: the treadmill keeps going by itself
//!   while the app is closed, and the walk carries on after the reconnect;
//! - `snapshot()`, which feeds `AppSnapshot::update`.
//!
//! The update itself comes from `latest.json` on the newest GitHub release
//! (signed with the updater key; see `tauri.conf.json`). Its notes come from
//! GitHub's public releases list, so a jump over several versions shows each
//! of them; when that list can't be read, the notes in `latest.json` are used.

use crate::bluetooth::ORCHESTRATOR;
use crate::persistence;
use crate::protocol::MachineStatus;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;
use tauri::AppHandle;

// Settings keys (raw values in the `settings` table).
const KEY_DISMISSED: &str = "update_dismissed";
const KEY_HUD_SEEN: &str = "update_hud_seen";
const KEY_WHATS_NEW: &str = "update_whats_new";
const KEY_AUTO_INSTALL: &str = "update_auto_install";

const RELEASES_API: &str = "https://api.github.com/repos/Fresh-Brewed-Design/neon-strider/releases";
const USER_AGENT: &str = "neon-strider-updater";
/// The release body's installer note starts here; the app shows only the text above it.
const INSTALLER_NOTE_MARKER: &str = "<!-- installer-note -->";

const MINUTE_MS: u64 = 60_000;
const DAY_MS: u64 = 24 * 60 * MINUTE_MS;
/// A successful background check is repeated this often: releases land several
/// times a day, and one check is a couple of small GitHub requests, far under the
/// unauthenticated API limit.
const CHECK_EVERY_MS: u64 = 60 * MINUTE_MS;
/// The first check waits this long after startup, so it doesn't compete with the connect.
/// A treadmill connect takes up to about 15 s (scan, settle, retries); 30 s is after it.
const STARTUP_DELAY_MS: u64 = 30_000;
/// A failed background check (offline at startup, say) is retried this soon.
/// Hourly is soon enough for a release and stays far inside GitHub's 60
/// anonymous requests an hour.
const RETRY_AFTER_FAILURE_MS: u64 = 60 * MINUTE_MS;
/// How often the scheduler looks at the clock and the belt.
/// The waits it checks are minutes long, so 5 s is precise enough and costs nothing.
const SCHEDULER_TICK: Duration = Duration::from_secs(5);
/// Auto-install waits for the belt to be still, with no program, this long...
/// As long as walk-away's default idle pause: a walk that stopped that long ago is over.
const AUTO_BELT_STILL_MS: u64 = 10 * MINUTE_MS;
/// ...and for this long without keyboard or mouse input on the PC.
/// The same 5 minutes that counts as PC use elsewhere (`seated::ACTIVE_WINDOW_MS`).
const AUTO_PC_IDLE_MS: u64 = 5 * MINUTE_MS;
/// After an automatic install fails, the next automatic try waits this long.
/// As for a failed check: hourly, so a broken download doesn't retry in a loop.
const AUTO_RETRY_MS: u64 = 60 * MINUTE_MS;
/// "What's new" opens the dashboard by itself only this soon after the install began.
/// An install restarts the app within a minute or two; 15 minutes covers a slow
/// PC, and a later start (next morning) shouldn't pop the dashboard open.
const WHATS_NEW_OPEN_WITHIN_MS: u64 = 15 * MINUTE_MS;
/// GitHub's API answers in under a second; 20 s covers a slow link without
/// leaving Check now spinning.
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------------------
// Snapshot types (exported to TypeScript)
// ---------------------------------------------------------------------------

/// Where the updater is.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    /// A newer version can be installed. `notes` is Markdown covering it and every
    /// version skipped since the installed one, newest first; `published` is an
    /// RFC 3339 time when known.
    Available {
        version: String,
        notes: String,
        published: Option<String>,
    },
    /// `percent` is None while the size is unknown.
    Downloading {
        percent: Option<u8>,
    },
    /// The installer is starting; the app closes and reopens.
    Installing,
    UpToDate,
    /// The last check or install failed, in plain words.
    Failed {
        message: String,
    },
}

/// The update on offer, kept while it downloads or after a failed install so
/// its notes stay on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct UpdateOffer {
    pub version: String,
    pub notes: String,
    pub published: Option<String>,
}

/// The notes of the version just installed, shown once after the restart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct WhatsNew {
    pub version: String,
    pub notes: String,
}

/// The outcome of the last check or install started by a button.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct UpdateResult {
    pub ok: bool,
    pub message: String,
    /// Epoch ms; always increases, so a change means a new result.
    #[ts(type = "number")]
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct UpdateSnapshot {
    /// The running version.
    pub current_version: String,
    pub status: UpdateStatus,
    pub offer: Option<UpdateOffer>,
    /// The offered version was put off with "Not now" (for a day).
    pub dismissed: bool,
    /// The pink "update waiting" dot on the System nav, the Updates tab and the
    /// tray icon (`show_update_dot`).
    pub dot: bool,
    /// Show the HUD's UPDATE READY card (once per version).
    pub hud_card: bool,
    /// Set when only the newest version's notes could be shown, and why.
    pub notes_warning: Option<String>,
    /// When the HUD asked the dashboard to open the notes (epoch ms).
    #[ts(type = "number | null")]
    pub show_notes_at_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub last_checked_ms: Option<i64>,
    /// Buttons' outcome (Check now, Install); background checks never set it.
    pub last_result: Option<UpdateResult>,
    pub whats_new: Option<WhatsNew>,
    /// Install an offered update by itself when the belt and the PC are idle.
    pub auto_install: bool,
}

// ---------------------------------------------------------------------------
// Pure logic
// ---------------------------------------------------------------------------

/// A version as (major, minor, patch, is_release). A pre-release ("1.3.0-beta")
/// sorts before its release.
pub type Version = (u64, u64, u64, bool);

/// "v1.2.3", "1.2.3" or "1.2.3-beta.1"; None if it isn't one.
pub fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim().trim_start_matches('v');
    let s = s.split('+').next()?;
    let (core, pre) = match s.split_once('-') {
        Some((core, pre)) => (core, !pre.is_empty()),
        None => (s, false),
    };
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??, !pre);
    parts.next().is_none().then_some(v)
}

/// `a` is a later version than `b`. Unparseable versions are never newer.
pub fn is_newer(a: &str, b: &str) -> bool {
    match (parse_version(a), parse_version(b)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// The part of a release body the app shows: above the installer note, with
/// "\n" line ends, trimmed.
pub fn notes_text(body: &str) -> String {
    let body = body.replace("\r\n", "\n");
    let above = body.split(INSTALLER_NOTE_MARKER).next().unwrap_or("");
    above.trim().to_string()
}

/// One entry of GitHub's releases list.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

fn section(version: &str, body: &str) -> String {
    let text = notes_text(body);
    let text = if text.is_empty() {
        "- No notes for this version.".to_string()
    } else {
        text
    };
    format!("## v{}\n\n{}", version.trim_start_matches('v'), text)
}

/// The notes for going from `installed` to `available`: every published
/// release after `installed` up to `available`, newest first, each under its
/// own `## vX.Y.Z` heading. None when `available` itself isn't in the list
/// (GitHub's list is behind), so the caller falls back to `latest.json`.
pub fn notes_since(releases: &[Release], installed: &str, available: &str) -> Option<String> {
    let mut picked: Vec<(Version, &Release)> = releases
        .iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| parse_version(&r.tag_name).map(|v| (v, r)))
        .filter(|(_, r)| is_newer(&r.tag_name, installed) && !is_newer(&r.tag_name, available))
        .collect();
    let target = parse_version(available)?;
    if !picked.iter().any(|(v, _)| *v == target) {
        return None;
    }
    picked.sort_by_key(|a| std::cmp::Reverse(a.0));
    picked.dedup_by(|a, b| a.0 == b.0);
    Some(
        picked
            .iter()
            .map(|(_, r)| section(&r.tag_name, r.body.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

/// The fallback: just the available version's notes from `latest.json`.
pub fn single_version_notes(version: &str, body: Option<&str>) -> String {
    section(version, body.unwrap_or(""))
}

/// The outcome of the last check, for the schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastCheck {
    pub at_ms: u64,
    pub ok: bool,
}

/// A background check is due: 30 s after startup, then an hour after the last
/// check, whether it worked or failed.
pub fn check_due(now_ms: u64, started_ms: u64, last: Option<LastCheck>) -> bool {
    match last {
        None => now_ms >= started_ms + STARTUP_DELAY_MS,
        Some(LastCheck { at_ms, ok: true }) => now_ms >= at_ms + CHECK_EVERY_MS,
        Some(LastCheck { at_ms, ok: false }) => now_ms >= at_ms + RETRY_AFTER_FAILURE_MS,
    }
}

/// The belt and the program right now, as auto-install sees them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeltNow {
    pub connected: bool,
    pub status: MachineStatus,
    /// Any program: walking, resting, paused or waiting for GO.
    pub program_active: bool,
    /// A start asked for and not yet seen on the deck.
    pub start_pending: bool,
}

impl BeltNow {
    /// The link shows the belt turning, counting down or winding down.
    fn deck_busy(&self) -> bool {
        self.connected
            && matches!(
                self.status,
                MachineStatus::Running | MachineStatus::Starting | MachineStatus::Stopping
            )
    }

    /// Something is happening, or may be: the deck is busy, a start is in
    /// flight, or a program is active.
    pub fn busy(&self) -> bool {
        self.deck_busy() || self.program_active || self.start_pending
    }
}

/// When the belt was last busy. App start counts as busy (the belt may be
/// going after a restart), so the 10 minutes run from startup at the earliest.
/// While the link is down the belt can't be seen, so it counts as whatever it
/// was when the link dropped: a link lost mid-walk keeps it busy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BeltWatch {
    last_busy_ms: u64,
    busy_when_seen: bool,
}

impl BeltWatch {
    pub fn new(started_ms: u64) -> Self {
        Self {
            last_busy_ms: started_ms,
            busy_when_seen: false,
        }
    }

    pub fn observe(&mut self, now_ms: u64, b: BeltNow) {
        if b.connected {
            self.busy_when_seen = b.deck_busy();
        }
        if b.busy() || self.busy_when_seen {
            self.last_busy_ms = now_ms;
        }
    }

    pub fn still_for_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.last_busy_ms)
    }
}

/// Everything the auto-install decision depends on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoInput {
    /// The setting.
    pub enabled: bool,
    /// An update is on offer and ready to download.
    pub offered: bool,
    /// "Not now" holds for the offered version.
    pub dismissed: bool,
    /// The updater is checking, downloading or installing.
    pub updater_busy: bool,
    /// The belt is busy right now.
    pub belt_busy: bool,
    pub belt_still_ms: u64,
    /// Since the last keyboard or mouse input; `None` when the OS can't say.
    pub pc_idle_ms: Option<u64>,
    /// Since the last automatic install failed.
    pub since_auto_failure_ms: Option<u64>,
}

/// What auto-install does now; `Wait` says why, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoInstall {
    Install,
    Wait(&'static str),
}

/// Install by itself only when it's on, an update is offered and not put off,
/// the belt has been still with no program for 10 minutes, and the PC has had
/// no input for 5. Called on every scheduler tick and again right before the
/// installer runs.
pub fn auto_install(i: AutoInput) -> AutoInstall {
    use AutoInstall::Wait;
    if !i.enabled {
        return Wait("auto-install is off");
    }
    if !i.offered {
        return Wait("no update on offer");
    }
    if i.updater_busy {
        return Wait("the updater is busy");
    }
    if i.dismissed {
        return Wait("put off with Not now");
    }
    if i.since_auto_failure_ms.is_some_and(|t| t < AUTO_RETRY_MS) {
        return Wait("the last automatic install failed less than an hour ago");
    }
    if i.belt_busy || i.belt_still_ms < AUTO_BELT_STILL_MS {
        return Wait("the belt or a program was active in the last 10 minutes");
    }
    match i.pc_idle_ms {
        None => Wait("the PC's idle time is unknown"),
        Some(t) if t < AUTO_PC_IDLE_MS => Wait("the PC was used in the last 5 minutes"),
        Some(_) => AutoInstall::Install,
    }
}

/// "Not now" for one version, remembered in settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dismissal {
    pub version: String,
    pub at_ms: u64,
}

/// "Not now" still holds for `version`: same version, and not yet a day old
/// (the first check after that offers it again).
pub fn dismissal_active(d: Option<&Dismissal>, version: &str, now_ms: u64) -> bool {
    d.is_some_and(|d| d.version == version && now_ms < d.at_ms + DAY_MS)
}

/// The "update waiting" dot (System nav, System -> Updates tab, tray icon): an
/// update is on offer and wasn't put off with "Not now". It clears when the
/// offer is put off, starts downloading or installing, or goes away.
pub fn show_update_dot(status: &UpdateStatus, dismissed: bool) -> bool {
    matches!(status, UpdateStatus::Available { .. }) && !dismissed
}

/// The notes saved just before installing, for "What's new" after the restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedWhatsNew {
    pub version: String,
    pub notes: String,
    pub saved_at_ms: u64,
}

/// What to do with the saved notes at launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhatsNewOnLaunch {
    /// Nothing saved, or it doesn't belong to this version: forget it.
    Drop,
    /// Show it; `open` also opens the dashboard (only right after the install).
    Show { open: bool },
}

pub fn whats_new_on_launch(saved: &SavedWhatsNew, running: &str, now_ms: u64) -> WhatsNewOnLaunch {
    match (parse_version(&saved.version), parse_version(running)) {
        (Some(a), Some(b)) if a == b => WhatsNewOnLaunch::Show {
            open: now_ms.saturating_sub(saved.saved_at_ms) < WHATS_NEW_OPEN_WITHIN_MS,
        },
        _ => WhatsNewOnLaunch::Drop,
    }
}

pub const OFFLINE: &str = "Can't reach GitHub. Check the internet connection, then try again.";
pub const RATE_LIMITED: &str =
    "GitHub is limiting requests from this connection for now. Try again in an hour.";
pub const SIGNATURE: &str = "The download isn't signed with Neon Strider's key, so the update was not installed. Try again later, and report repeat failures on GitHub.";
pub const NO_RELEASE: &str =
    "GitHub didn't send the update information (no release yet, or GitHub is busy). Try again later.";
pub const DOWNLOAD_STOPPED: &str =
    "The download stopped part way. Check the internet connection, then try again.";

/// One plain sentence for an updater failure.
pub fn describe(e: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Reqwest(r) => match r.status().map(|s| s.as_u16()) {
            Some(403 | 429) => RATE_LIMITED.to_string(),
            Some(_) => NO_RELEASE.to_string(),
            None => OFFLINE.to_string(),
        },
        E::Network(m) if m.contains("429") || m.contains("403") => RATE_LIMITED.to_string(),
        E::Network(_) => DOWNLOAD_STOPPED.to_string(),
        E::ReleaseNotFound | E::Serialization(_) => NO_RELEASE.to_string(),
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion => SIGNATURE.to_string(),
        E::TargetNotFound(_) | E::TargetsNotFound(_) => format!(
            "The newest release has no {} yet. Try again later.",
            crate::platform::WORDS.release_download
        ),
        E::Io(err) => format!("The installer couldn't start: {}.", err),
        other => format!("The update failed: {}.", other),
    }
}

/// Why the releases list couldn't be used, for `notes_warning`.
fn notes_warning_for(status: Option<u16>) -> String {
    let why = match status {
        Some(403 | 429) => "GitHub's limit for anonymous requests was reached",
        _ => "GitHub's release list didn't load",
    };
    format!("Showing this version's notes only: {}.", why)
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Runtime {
    current_version: String,
    started_ms: u64,
    status: UpdateStatus,
    offer: Option<UpdateOffer>,
    /// The plugin's handle to the offered update, for the install.
    pending: Option<tauri_plugin_updater::Update>,
    last_check: Option<LastCheck>,
    notes_warning: Option<String>,
    show_notes_at_ms: Option<i64>,
    last_result: Option<UpdateResult>,
    dismissed: Option<Dismissal>,
    hud_seen: Option<String>,
    whats_new: Option<WhatsNew>,
    auto_install: bool,
    belt: BeltWatch,
    /// The last auto-install decision, so the log gets each change once.
    last_auto: Option<AutoInstall>,
    auto_failed_ms: Option<u64>,
}

static RT: Mutex<Option<Runtime>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Runtime) -> T) -> T {
    let mut guard = RT.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(Runtime::default))
}

fn busy(status: &UpdateStatus) -> bool {
    matches!(
        status,
        UpdateStatus::Checking | UpdateStatus::Downloading { .. } | UpdateStatus::Installing
    )
}

fn now() -> u64 {
    crate::util::now_ms_u64()
}

fn record_result(rt: &mut Runtime, ok: bool, message: String) {
    let prev = rt.last_result.as_ref().map_or(0, |r| r.at_ms);
    rt.last_result = Some(UpdateResult {
        ok,
        message,
        at_ms: crate::util::now_ms().max(prev + 1),
    });
}

/// Load what settings remember, and the saved "What's new". Call once after
/// the DB is initialised.
pub fn init(app: &AppHandle) {
    let running = app.package_info().version.to_string();
    let dismissed = persistence::read_setting(app, KEY_DISMISSED)
        .and_then(|s| serde_json::from_str::<Dismissal>(&s).ok());
    let hud_seen = persistence::read_setting(app, KEY_HUD_SEEN).filter(|s| !s.is_empty());
    let saved = persistence::read_setting(app, KEY_WHATS_NEW)
        .and_then(|s| serde_json::from_str::<SavedWhatsNew>(&s).ok());
    let auto_install =
        persistence::parse_setting(persistence::read_setting(app, KEY_AUTO_INSTALL).as_deref())
            .unwrap_or(true);

    let mut open_dashboard = false;
    let whats_new = match saved {
        Some(saved) => match whats_new_on_launch(&saved, &running, now()) {
            WhatsNewOnLaunch::Show { open } => {
                log::info!("Updated to v{}; showing what's new", running);
                open_dashboard = open;
                Some(WhatsNew {
                    version: saved.version,
                    notes: saved.notes,
                })
            }
            WhatsNewOnLaunch::Drop => {
                log::info!(
                    "Saved notes for v{} don't match v{}; dropping them",
                    saved.version,
                    running
                );
                let _ = persistence::delete_setting(app, KEY_WHATS_NEW);
                None
            }
        },
        None => None,
    };

    with(|rt| {
        rt.current_version = running;
        rt.started_ms = now();
        rt.belt = BeltWatch::new(rt.started_ms);
        rt.auto_install = auto_install;
        rt.dismissed = dismissed;
        rt.hud_seen = hud_seen;
        rt.whats_new = whats_new;
    });
    if open_dashboard {
        crate::dashboard::show_dashboard(app);
    }
}

/// The background schedule: a check 30 s after startup, then hourly; and on
/// every tick, whether an offered update may install by itself.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(SCHEDULER_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let due =
                with(|rt| !busy(&rt.status) && check_due(now(), rt.started_ms, rt.last_check));
            if due {
                check(&app, false).await;
            }
            if auto_decide(belt_now().await) == AutoInstall::Install {
                if let Err(e) = start_install(&app, true) {
                    log::info!("Auto-install didn't start: {}", e);
                }
            }
        }
    });
}

/// The belt and program now, read from the engine. Reads only.
async fn belt_now() -> BeltNow {
    let connected = crate::bluetooth::is_bluetooth_connected().await;
    let state = ORCHESTRATOR.lock().await;
    BeltNow {
        connected,
        status: state.last_status,
        program_active: state.active_program().is_some(),
        start_pending: state.start_requested_at.is_some(),
    }
}

fn pc_idle_ms() -> Option<u64> {
    crate::platform::input_idle_for().map(|d| d.as_millis() as u64)
}

/// Record the belt, then decide whether to install by itself now. Each new
/// decision is logged once.
fn auto_decide(belt: BeltNow) -> AutoInstall {
    let pc_idle_ms = pc_idle_ms();
    with(|rt| {
        let now_ms = now();
        rt.belt.observe(now_ms, belt);
        let version = rt.offer.as_ref().map(|o| o.version.clone());
        let decision = auto_install(AutoInput {
            enabled: rt.auto_install,
            offered: rt.pending.is_some()
                && matches!(
                    rt.status,
                    UpdateStatus::Available { .. } | UpdateStatus::Failed { .. }
                ),
            dismissed: version
                .as_deref()
                .is_some_and(|v| dismissal_active(rt.dismissed.as_ref(), v, now_ms)),
            updater_busy: busy(&rt.status),
            belt_busy: belt.busy(),
            belt_still_ms: rt.belt.still_for_ms(now_ms),
            pc_idle_ms,
            since_auto_failure_ms: rt.auto_failed_ms.map(|t| now_ms.saturating_sub(t)),
        });
        log_auto(rt, decision, version.as_deref(), pc_idle_ms);
        decision
    })
}

fn log_auto(rt: &mut Runtime, decision: AutoInstall, version: Option<&str>, pc_idle: Option<u64>) {
    if rt.last_auto == Some(decision) {
        return;
    }
    rt.last_auto = Some(decision);
    let version = version.unwrap_or("-");
    match decision {
        AutoInstall::Install => log::info!(
            "Auto-install: installing v{} (belt still {} s, PC idle {:?} s)",
            version,
            rt.belt.still_for_ms(now()) / 1000,
            pc_idle.map(|t| t / 1000)
        ),
        AutoInstall::Wait(why) => log::info!("Auto-install: waiting (v{}): {}", version, why),
    }
}

/// The notes for `update`: every version since the installed one from
/// GitHub's releases list, or `latest.json`'s notes when that fails.
async fn notes_for(update: &tauri_plugin_updater::Update) -> (String, Option<String>) {
    let fallback = || single_version_notes(&update.version, update.body.as_deref());
    match fetch_releases().await {
        Ok(releases) => match notes_since(&releases, &update.current_version, &update.version) {
            Some(notes) => (notes, None),
            None => {
                log::info!(
                    "Releases list has no v{} yet; using latest.json notes",
                    update.version
                );
                (fallback(), Some(notes_warning_for(None)))
            }
        },
        Err(status) => {
            log::warn!(
                "Could not read the releases list (status {:?}); using latest.json notes",
                status
            );
            (fallback(), Some(notes_warning_for(status)))
        }
    }
}

/// GitHub's public releases list, unauthenticated. Err carries the HTTP
/// status when there was one.
async fn fetch_releases() -> Result<Vec<Release>, Option<u16>> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|_| None)?;
    let res = client
        .get(RELEASES_API)
        .query(&[("per_page", "50")])
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| None)?;
    let status = res.status();
    if !status.is_success() {
        return Err(Some(status.as_u16()));
    }
    res.json::<Vec<Release>>().await.map_err(|_| None)
}

/// Look for an update. `manual` (Check now) shows progress and records a
/// result for the button; a background check stays quiet, and a failure only
/// shows in System -> Updates.
pub async fn check(app: &AppHandle, manual: bool) {
    let started = with(|rt| {
        if busy(&rt.status) {
            return false;
        }
        if manual || rt.offer.is_none() {
            rt.status = UpdateStatus::Checking;
        }
        true
    });
    if !started {
        return;
    }

    let result = async {
        use tauri_plugin_updater::UpdaterExt;
        app.updater()?.check().await
    }
    .await;

    let found = match result {
        Ok(Some(update)) => {
            let (notes, warning) = notes_for(&update).await;
            Ok(Some((update, notes, warning)))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    };

    with(|rt| {
        let at_ms = now();
        match found {
            Ok(Some((update, notes, warning))) => {
                let published = update
                    .raw_json
                    .get("pub_date")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                log::info!(
                    "Update available: v{} (running v{})",
                    update.version,
                    update.current_version
                );
                let offer = UpdateOffer {
                    version: update.version.clone(),
                    notes,
                    published,
                };
                rt.status = UpdateStatus::Available {
                    version: offer.version.clone(),
                    notes: offer.notes.clone(),
                    published: offer.published.clone(),
                };
                if manual {
                    record_result(rt, true, format!("v{} is available.", offer.version));
                }
                rt.offer = Some(offer);
                rt.pending = Some(update);
                rt.notes_warning = warning;
                rt.last_check = Some(LastCheck { at_ms, ok: true });
            }
            Ok(None) => {
                log::info!("No update (running v{})", rt.current_version);
                rt.status = UpdateStatus::UpToDate;
                rt.offer = None;
                rt.pending = None;
                rt.notes_warning = None;
                if manual {
                    let msg = format!("Up to date: v{} is the newest.", rt.current_version);
                    record_result(rt, true, msg);
                }
                rt.last_check = Some(LastCheck { at_ms, ok: true });
            }
            Err(e) => {
                let message = describe(&e);
                log::warn!("Update check failed: {}", e);
                // A background failure never replaces an update already on offer.
                if manual || rt.offer.is_none() {
                    rt.status = UpdateStatus::Failed {
                        message: message.clone(),
                    };
                } else if let Some(offer) = rt.offer.clone() {
                    rt.status = UpdateStatus::Available {
                        version: offer.version,
                        notes: offer.notes,
                        published: offer.published,
                    };
                }
                if manual {
                    record_result(rt, false, message);
                }
                rt.last_check = Some(LastCheck { at_ms, ok: false });
            }
        }
    });
}

fn fail_install(message: String, auto: bool) {
    let kind = if auto { "Automatic" } else { "Update" };
    log::warn!("{} install failed: {}", kind, message);
    with(|rt| {
        rt.status = UpdateStatus::Failed {
            message: message.clone(),
        };
        if auto {
            rt.auto_failed_ms = Some(now());
        } else {
            record_result(rt, false, message);
        }
    });
}

/// The automatic install's last look, right before the installer runs: if the
/// belt or the PC woke up meanwhile, put the offer back and wait again.
async fn still_idle_for_auto(app: &AppHandle) -> bool {
    let belt = belt_now().await;
    let pc_idle_ms = pc_idle_ms();
    let decision = with(|rt| {
        let now_ms = now();
        rt.belt.observe(now_ms, belt);
        auto_install(AutoInput {
            enabled: rt.auto_install,
            offered: true,
            dismissed: false,
            // This install is the updater's own work.
            updater_busy: false,
            belt_busy: belt.busy(),
            belt_still_ms: rt.belt.still_for_ms(now_ms),
            pc_idle_ms,
            since_auto_failure_ms: None,
        })
    });
    if let AutoInstall::Wait(why) = decision {
        log::info!("Auto-install: called off right before installing: {}", why);
        let _ = persistence::delete_setting(app, KEY_WHATS_NEW);
        with(|rt| {
            rt.last_auto = Some(decision);
            if let Some(o) = rt.offer.clone() {
                rt.status = UpdateStatus::Available {
                    version: o.version,
                    notes: o.notes,
                    published: o.published,
                };
            }
        });
        return false;
    }
    log::info!("Auto-install: still idle; running the installer");
    true
}

/// Save the running program just before the app closes (the emitter only
/// saves every few seconds), so it comes back at the same step. Nothing is
/// sent to the treadmill: installing never pauses or stops the belt, which
/// keeps going by itself while the app is closed.
async fn save_running_program(app: &AppHandle) {
    let run = ORCHESTRATOR.lock().await.saved_run(crate::util::now_ms());
    let app = app.clone();
    let _ = crate::util::blocking(move || {
        crate::programs::persist_active_run(&app, run, &mut None);
        Ok(())
    })
    .await;
}

/// Install the offered update: download and verify it, save its notes for
/// "What's new" and the running program, then run the installer, which closes
/// the app and opens the new version (on macOS: replace the app, then relaunch). The belt is never paused or stopped.
/// Returns at once; progress is in the snapshot.
pub fn install(app: &AppHandle) -> Result<(), String> {
    start_install(app, false)
}

/// `auto`: started by the idle rule, not a button. It records no button
/// result, and checks the belt and the PC once more before the installer runs.
fn start_install(app: &AppHandle, auto: bool) -> Result<(), String> {
    let (update, offer) = with(|rt| {
        if busy(&rt.status) {
            return Err("An update is already being installed.".to_string());
        }
        let update = rt.pending.clone();
        let offer = rt.offer.clone();
        match (update, offer) {
            (Some(u), Some(o)) => {
                rt.status = UpdateStatus::Downloading { percent: Some(0) };
                Ok((u, o))
            }
            _ => Err("No update to install. Check for updates first.".to_string()),
        }
    })?;

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let saved = SavedWhatsNew {
            version: offer.version.clone(),
            notes: offer.notes.clone(),
            saved_at_ms: now(),
        };
        if let Ok(json) = serde_json::to_string(&saved) {
            if let Err(e) = persistence::write_setting(&app, KEY_WHATS_NEW, &json) {
                log::warn!("Could not save the update notes: {}", e);
            }
        }

        let mut received: u64 = 0;
        let bytes = update
            .download(
                |chunk, total| {
                    received += chunk as u64;
                    let percent = total
                        .filter(|t| *t > 0)
                        .map(|t| ((received * 100) / t).min(100) as u8);
                    with(|rt| rt.status = UpdateStatus::Downloading { percent });
                },
                || {},
            )
            .await;
        let bytes = match bytes {
            Ok(b) => b,
            Err(e) => {
                let _ = persistence::delete_setting(&app, KEY_WHATS_NEW);
                fail_install(describe(&e), auto);
                return;
            }
        };
        log::info!(
            "Update v{} downloaded and verified; installing",
            offer.version
        );

        save_running_program(&app).await;
        with(|rt| rt.status = UpdateStatus::Installing);
        // Let the windows show "Installing" before the app closes.
        // Two snapshot ticks (250 ms) plus a repaint, so the windows show it.
        tokio::time::sleep(Duration::from_millis(600)).await;
        if auto && !still_idle_for_auto(&app).await {
            return;
        }
        // On Windows this starts the installer, which exits the app and opens
        // the new version. On macOS it puts the new app in place of this one
        // and returns, so the app opens it itself.
        match update.install(bytes) {
            Ok(()) => {
                log::info!("Update v{} installed; relaunching", offer.version);
                crate::allow_exit();
                app.restart();
            }
            Err(e) => {
                let _ = persistence::delete_setting(&app, KEY_WHATS_NEW);
                fail_install(describe(&e), auto);
            }
        }
    });
    Ok(())
}

/// "Not now": hide the offer (badge and HUD card) for this version for a day. The Updates section still shows it.
pub fn dismiss(app: &AppHandle) -> Result<(), String> {
    let version = with(|rt| rt.offer.as_ref().map(|o| o.version.clone()))
        .ok_or_else(|| "No update to put off.".to_string())?;
    let d = Dismissal {
        version: version.clone(),
        at_ms: now(),
    };
    let json = serde_json::to_string(&d).map_err(|e| e.to_string())?;
    persistence::write_setting(app, KEY_DISMISSED, &json)?;
    persistence::write_setting(app, KEY_HUD_SEEN, &version)?;
    with(|rt| {
        rt.dismissed = Some(d);
        rt.hud_seen = Some(version);
    });
    Ok(())
}

/// NOTES or LATER on the HUD card: the card has been seen for this version.
/// NOTES (`open`) opens the dashboard on the notes rather than installing.
pub fn hud_card(app: &AppHandle, open: bool) -> Result<(), String> {
    let version = with(|rt| rt.offer.as_ref().map(|o| o.version.clone()));
    if let Some(version) = version {
        persistence::write_setting(app, KEY_HUD_SEEN, &version)?;
        with(|rt| rt.hud_seen = Some(version));
    }
    if open {
        with(|rt| rt.show_notes_at_ms = Some(crate::util::now_ms()));
        crate::dashboard::show_dashboard(app);
    }
    Ok(())
}

/// The auto-install setting (System -> Updates).
pub fn set_auto_install(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_AUTO_INSTALL, if on { "true" } else { "false" })?;
    log::info!("Auto-install {}", if on { "on" } else { "off" });
    with(|rt| {
        rt.auto_install = on;
        rt.last_auto = None;
    });
    Ok(())
}

/// "What's new" was read.
pub fn whats_new_ack(app: &AppHandle) -> Result<(), String> {
    persistence::delete_setting(app, KEY_WHATS_NEW)?;
    with(|rt| rt.whats_new = None);
    Ok(())
}

/// The updater's part of the snapshot.
pub fn snapshot(now_ms: u64) -> UpdateSnapshot {
    with(|rt| {
        let version = rt.offer.as_ref().map(|o| o.version.as_str());
        let dismissed = version.is_some_and(|v| dismissal_active(rt.dismissed.as_ref(), v, now_ms));
        let available = matches!(rt.status, UpdateStatus::Available { .. });
        let hud_card =
            available && !dismissed && version.is_some_and(|v| rt.hud_seen.as_deref() != Some(v));
        UpdateSnapshot {
            current_version: rt.current_version.clone(),
            status: rt.status.clone(),
            offer: rt.offer.clone(),
            dismissed,
            dot: show_update_dot(&rt.status, dismissed),
            hud_card,
            notes_warning: rt.notes_warning.clone(),
            show_notes_at_ms: rt.show_notes_at_ms,
            last_checked_ms: rt.last_check.map(|c| c.at_ms as i64),
            last_result: rt.last_result.clone(),
            whats_new: rt.whats_new.clone(),
            auto_install: rt.auto_install,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(tag: &str, body: &str) -> Release {
        Release {
            tag_name: tag.to_string(),
            body: Some(body.to_string()),
            draft: false,
            prerelease: false,
        }
    }

    #[test]
    fn versions_compare_numerically_with_prereleases_first() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3, true)));
        assert_eq!(parse_version("1.10.0"), Some((1, 10, 0, true)));
        assert_eq!(parse_version("1.3.0-beta.1"), Some((1, 3, 0, false)));
        assert_eq!(parse_version("1.3.0+build5"), Some((1, 3, 0, true)));
        assert_eq!(parse_version("1.3"), None);
        assert_eq!(parse_version("1.3.0.1"), None);
        assert_eq!(parse_version("latest"), None);

        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(is_newer("v2.0.0", "1.99.99"));
        assert!(is_newer("1.3.0", "1.3.0-beta"));
        assert!(!is_newer("1.3.0-beta", "1.3.0"));
        assert!(!is_newer("1.2.0", "v1.2.0"));
        assert!(!is_newer("junk", "1.0.0"));
        assert!(!is_newer("1.0.0", "junk"));
    }

    #[test]
    fn notes_text_drops_the_installer_note() {
        let body = "### New\r\n\r\n- A\r\n\r\n<!-- installer-note -->\r\n---\r\n\r\nWindows installer below.";
        assert_eq!(notes_text(body), "### New\n\n- A");
        assert_eq!(notes_text("  - Only notes  \n"), "- Only notes");
        assert_eq!(notes_text(""), "");
    }

    #[test]
    fn notes_cover_every_skipped_version_newest_first() {
        let releases = vec![
            rel("v1.5.0", "- Five (not offered yet)"),
            rel(
                "v1.4.0",
                "### New\n\n- Four\n\n<!-- installer-note -->\nSmartScreen",
            ),
            Release {
                draft: true,
                ..rel("v1.3.5", "- Draft")
            },
            Release {
                prerelease: true,
                ..rel("v1.3.1", "- Beta")
            },
            rel("v1.3.0", ""),
            rel("v1.2.0", "- Installed one"),
            rel("v1.1.0", "- Older"),
            rel("nightly", "- Not a version"),
        ];
        let notes = notes_since(&releases, "1.2.0", "1.4.0").unwrap();
        assert_eq!(
            notes,
            "## v1.4.0\n\n### New\n\n- Four\n\n## v1.3.0\n\n- No notes for this version."
        );
        // Just one step: only that version.
        assert_eq!(
            notes_since(&releases, "1.3.0", "v1.4.0").unwrap(),
            "## v1.4.0\n\n### New\n\n- Four"
        );
        // The list doesn't have the offered version yet: fall back.
        assert_eq!(notes_since(&releases, "1.2.0", "1.6.0"), None);
        assert_eq!(notes_since(&[], "1.2.0", "1.3.0"), None);
    }

    #[test]
    fn fallback_notes_are_the_one_version() {
        assert_eq!(
            single_version_notes("1.3.0", Some("- A\n<!-- installer-note -->\nX")),
            "## v1.3.0\n\n- A"
        );
        assert_eq!(
            single_version_notes("v1.3.0", None),
            "## v1.3.0\n\n- No notes for this version."
        );
    }

    #[test]
    fn checks_run_after_startup_then_hourly_and_retry_failures_hourly() {
        let start = 1_000_000;
        assert!(!check_due(start, start, None));
        assert!(!check_due(start + STARTUP_DELAY_MS - 1, start, None));
        assert!(check_due(start + STARTUP_DELAY_MS, start, None));

        let ok = Some(LastCheck {
            at_ms: start + 40_000,
            ok: true,
        });
        assert!(!check_due(start + 40_000 + CHECK_EVERY_MS - 1, start, ok));
        assert!(check_due(start + 40_000 + CHECK_EVERY_MS, start, ok));

        let failed = Some(LastCheck {
            at_ms: start + 40_000,
            ok: false,
        });
        assert!(!check_due(
            start + 40_000 + RETRY_AFTER_FAILURE_MS - 1,
            start,
            failed
        ));
        assert!(check_due(
            start + 40_000 + RETRY_AFTER_FAILURE_MS,
            start,
            failed
        ));
    }

    #[test]
    fn a_release_hours_after_startup_is_checked_for_without_a_manual_check() {
        let start = 1_000_000;
        let checked = start + STARTUP_DELAY_MS;
        let ok = Some(LastCheck {
            at_ms: checked,
            ok: true,
        });
        assert!(!check_due(checked + 59 * MINUTE_MS, start, ok));
        assert!(check_due(checked + 60 * MINUTE_MS, start, ok));
        // Each hourly check that finds nothing schedules the next one an hour on,
        // so a release published 3 hours in is picked up within an hour of it.
        let mut at = checked;
        let mut checks = 0;
        for minute in 1..=4 * 60 {
            let now = checked + minute * MINUTE_MS;
            let last = Some(LastCheck {
                at_ms: at,
                ok: true,
            });
            if check_due(now, start, last) {
                at = now;
                checks += 1;
            }
        }
        assert_eq!(checks, 4);
    }

    fn belt(connected: bool, status: MachineStatus) -> BeltNow {
        BeltNow {
            connected,
            status,
            program_active: false,
            start_pending: false,
        }
    }

    /// Idle long enough on every count: the one case that installs.
    fn idle() -> AutoInput {
        AutoInput {
            enabled: true,
            offered: true,
            dismissed: false,
            updater_busy: false,
            belt_busy: false,
            belt_still_ms: AUTO_BELT_STILL_MS,
            pc_idle_ms: Some(AUTO_PC_IDLE_MS),
            since_auto_failure_ms: None,
        }
    }

    #[test]
    fn auto_install_needs_ten_still_minutes_and_five_idle_pc_minutes() {
        assert_eq!(auto_install(idle()), AutoInstall::Install);
        let short_still = AutoInput {
            belt_still_ms: AUTO_BELT_STILL_MS - 1,
            ..idle()
        };
        assert!(matches!(auto_install(short_still), AutoInstall::Wait(_)));
        let pc_in_use = AutoInput {
            pc_idle_ms: Some(AUTO_PC_IDLE_MS - 1),
            ..idle()
        };
        assert!(matches!(auto_install(pc_in_use), AutoInstall::Wait(_)));
        let unknown_pc = AutoInput {
            pc_idle_ms: None,
            ..idle()
        };
        assert!(matches!(auto_install(unknown_pc), AutoInstall::Wait(_)));
    }

    #[test]
    fn auto_install_never_runs_with_the_belt_busy_off_or_nothing_offered() {
        for i in [
            AutoInput {
                belt_busy: true,
                ..idle()
            },
            AutoInput {
                enabled: false,
                ..idle()
            },
            AutoInput {
                offered: false,
                ..idle()
            },
            AutoInput {
                updater_busy: true,
                ..idle()
            },
            AutoInput {
                dismissed: true,
                ..idle()
            },
        ] {
            assert!(matches!(auto_install(i), AutoInstall::Wait(_)), "{i:?}");
        }
    }

    #[test]
    fn a_failed_auto_install_waits_an_hour_before_trying_again() {
        let recent = AutoInput {
            since_auto_failure_ms: Some(AUTO_RETRY_MS - 1),
            ..idle()
        };
        assert!(matches!(auto_install(recent), AutoInstall::Wait(_)));
        let later = AutoInput {
            since_auto_failure_ms: Some(AUTO_RETRY_MS),
            ..idle()
        };
        assert_eq!(auto_install(later), AutoInstall::Install);
    }

    #[test]
    fn moving_counting_down_stopping_or_any_program_is_busy() {
        use MachineStatus as M;
        for status in [M::Running, M::Starting, M::Stopping] {
            assert!(belt(true, status).busy(), "{status:?}");
        }
        for status in [M::Stopped, M::Paused] {
            assert!(!belt(true, status).busy(), "{status:?}");
        }
        // A program walking, resting, paused or waiting for GO: all active.
        let program = BeltNow {
            program_active: true,
            ..belt(true, M::Stopped)
        };
        assert!(program.busy());
        let starting = BeltNow {
            start_pending: true,
            ..belt(true, M::Stopped)
        };
        assert!(starting.busy());
    }

    #[test]
    fn the_still_time_counts_from_startup_and_from_the_last_busy_moment() {
        use MachineStatus as M;
        let start = 1_000_000;
        let mut w = BeltWatch::new(start);
        w.observe(start + 1_000, belt(true, M::Stopped));
        assert_eq!(w.still_for_ms(start + 60_000), 60_000);

        w.observe(start + 100_000, belt(true, M::Running));
        w.observe(start + 200_000, belt(true, M::Stopped));
        assert_eq!(w.still_for_ms(start + 200_000), 100_000);
        let paused_program = BeltNow {
            program_active: true,
            ..belt(true, M::Paused)
        };
        w.observe(start + 300_000, paused_program);
        assert_eq!(w.still_for_ms(start + 300_000), 0);
    }

    #[test]
    fn a_link_lost_mid_walk_keeps_the_belt_busy_until_it_is_seen_still() {
        use MachineStatus as M;
        let mut w = BeltWatch::new(0);
        w.observe(10_000, belt(true, M::Running));
        // Offline: the belt may still be going.
        w.observe(DAY_MS, belt(false, M::Running));
        assert_eq!(w.still_for_ms(DAY_MS), 0);
        w.observe(DAY_MS + 1_000, belt(true, M::Stopped));
        w.observe(DAY_MS + 2_000, belt(false, M::Stopped));
        assert_eq!(w.still_for_ms(DAY_MS + 600_000), 600_000);
    }

    #[test]
    fn not_now_lasts_a_day_for_that_version_only() {
        let d = Dismissal {
            version: "1.3.0".to_string(),
            at_ms: 5_000,
        };
        assert!(dismissal_active(Some(&d), "1.3.0", 5_000));
        assert!(dismissal_active(Some(&d), "1.3.0", 5_000 + DAY_MS - 1));
        assert!(!dismissal_active(Some(&d), "1.3.0", 5_000 + DAY_MS));
        // A newer version is offered again at once.
        assert!(!dismissal_active(Some(&d), "1.3.1", 6_000));
        assert!(!dismissal_active(None, "1.3.0", 6_000));
    }

    #[test]
    fn the_update_dot_shows_only_for_an_offer_not_put_off() {
        let available = UpdateStatus::Available {
            version: "1.3.0".into(),
            notes: String::new(),
            published: None,
        };
        assert!(show_update_dot(&available, false));
        // "Not now" clears it everywhere at once.
        assert!(!show_update_dot(&available, true));
        // So does installing it, and any other state.
        for status in [
            UpdateStatus::Idle,
            UpdateStatus::Checking,
            UpdateStatus::Downloading { percent: Some(40) },
            UpdateStatus::Installing,
            UpdateStatus::UpToDate,
            UpdateStatus::Failed {
                message: "offline".into(),
            },
        ] {
            assert!(!show_update_dot(&status, false), "{status:?}");
        }
    }

    #[test]
    fn whats_new_shows_only_on_the_version_it_was_saved_for() {
        let saved = SavedWhatsNew {
            version: "1.3.0".to_string(),
            notes: "## v1.3.0".to_string(),
            saved_at_ms: 1_000,
        };
        assert_eq!(
            whats_new_on_launch(&saved, "1.3.0", 1_000 + 60_000),
            WhatsNewOnLaunch::Show { open: true }
        );
        // A later launch still shows it (until read), without opening the dashboard.
        assert_eq!(
            whats_new_on_launch(&saved, "v1.3.0", 1_000 + WHATS_NEW_OPEN_WITHIN_MS),
            WhatsNewOnLaunch::Show { open: false }
        );
        // The install didn't happen (still the old version), or a later one did.
        assert_eq!(
            whats_new_on_launch(&saved, "1.2.0", 2_000),
            WhatsNewOnLaunch::Drop
        );
        assert_eq!(
            whats_new_on_launch(&saved, "1.4.0", 2_000),
            WhatsNewOnLaunch::Drop
        );
    }

    #[test]
    fn failures_read_as_plain_sentences() {
        use tauri_plugin_updater::Error as E;
        assert_eq!(describe(&E::ReleaseNotFound), NO_RELEASE);
        assert_eq!(
            describe(&E::Network(
                "Download request failed with status: 429 Too Many Requests".to_string()
            )),
            RATE_LIMITED
        );
        assert_eq!(
            describe(&E::Network("connection reset".to_string())),
            DOWNLOAD_STOPPED
        );
        assert_eq!(
            describe(&E::SignedVersionMismatch {
                signed: "1.2.0".to_string(),
                announced: "1.3.0".to_string()
            }),
            SIGNATURE
        );
        assert_eq!(describe(&E::MissingSignedVersion), SIGNATURE);
        assert_eq!(describe(&E::SignatureUtf8("x".to_string())), SIGNATURE);
        assert!(describe(&E::TargetNotFound("windows-x86_64".to_string()))
            .contains(crate::platform::WORDS.release_download));

        assert!(notes_warning_for(Some(403)).contains("limit"));
        assert!(notes_warning_for(None).contains("didn't load"));
    }

    #[test]
    fn the_shipped_updater_config_parses() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = conf["plugins"]["updater"].clone();
        let parsed: tauri_plugin_updater::Config = serde_json::from_value(updater).unwrap();
        assert!(parsed.require_signed_version);
        assert_eq!(
            parsed.endpoints[0].as_str(),
            "https://github.com/Fresh-Brewed-Design/neon-strider/releases/latest/download/latest.json"
        );
        // Local builds make no updater artifacts (release.yml turns them on).
        assert_eq!(conf["bundle"]["createUpdaterArtifacts"], false);
    }

    /// Installing never pauses or stops the belt: nothing in the install path
    /// may send the treadmill a control verb or pause a program.
    #[test]
    fn installing_sends_nothing_to_the_treadmill() {
        let source = include_str!("update.rs");
        let runtime = source.split("#[cfg(test)]").next().unwrap();
        for verb in [
            concat!("trigger", "_action"),
            concat!("DeckAction", "::"),
            concat!("pause", "_program"),
            concat!("pending", "_control"),
            concat!("end", "_program"),
        ] {
            assert!(!runtime.contains(verb), "update.rs uses {verb}");
        }
    }

    #[test]
    fn the_snapshot_hides_a_dismissed_offer_and_shows_the_hud_card_once() {
        let offer = UpdateOffer {
            version: "1.3.0".to_string(),
            notes: "## v1.3.0".to_string(),
            published: None,
        };
        with(|rt| {
            *rt = Runtime {
                current_version: "1.2.0".to_string(),
                status: UpdateStatus::Available {
                    version: offer.version.clone(),
                    notes: offer.notes.clone(),
                    published: None,
                },
                offer: Some(offer.clone()),
                ..Runtime::default()
            };
        });
        let s = snapshot(10_000);
        assert!(s.hud_card);
        assert!(!s.dismissed);

        with(|rt| rt.hud_seen = Some("1.3.0".to_string()));
        assert!(!snapshot(10_000).hud_card);

        with(|rt| {
            rt.hud_seen = None;
            rt.dismissed = Some(Dismissal {
                version: "1.3.0".to_string(),
                at_ms: 10_000,
            });
        });
        let s = snapshot(10_000 + 1);
        assert!(s.dismissed);
        assert!(!s.hud_card);
        assert!(!snapshot(10_000 + DAY_MS).dismissed);
        with(|rt| *rt = Runtime::default());
    }
}
