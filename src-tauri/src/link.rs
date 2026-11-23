//! The treadmill link as the windows see it: what the connection is doing right
//! now (scanning, linking, waiting to retry, no data) and why the last attempt
//! failed, in words a person can act on.
//!
//! Visibility only. Nothing here decides whether to connect; `bluetooth.rs` and
//! the watchdog in `lib.rs` report what they do, and `snapshot.rs` asks
//! [`current`] for the state to show. Countdowns are seconds computed here on
//! every snapshot (`next_retry_s`, `since_s`), so they change each second.

use crate::util::now_ms;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Connected but no telemetry frame for this long: the deck is out of range or
/// the subscription went stale, and the protocol worker drops the link so the
/// watchdog reconnects (`log_frame_flow`).
/// About 16 missed polls of 600 ms: no BLE hiccup lasts that long on a live link.
const NO_DATA_AFTER: Duration = Duration::from_secs(10);
/// The watchdog's fast retry interval.
/// Three treadmill scans' worth (5 s each) apart, so the radio is free most of
/// the time and a deck switched on is found within a quarter of a minute.
pub const RETRY_EVERY: Duration = Duration::from_secs(15);
/// The PC counts as in use when it has had input this recently.
/// Five minutes, as for the idle nudge's screen use (`seated::ACTIVE_WINDOW_MS`).
const PC_ACTIVE_WITHIN: Duration = Duration::from_secs(300);
/// The watchdog re-checks the PC this often while it waits.
/// Coming back to the PC brings the fast retries back within 5 s.
const WAIT_SLICE: Duration = Duration::from_secs(5);
/// The slowest the watchdog retries while the treadmill stays away.
/// Nobody steps on a treadmill they switched on and waits two minutes: past
/// that the deck is off, and a scan every 2 min costs little.
const RETRY_MAX: Duration = Duration::from_secs(120);

/// How long the watchdog waits between tries. While the PC is in use it stays
/// fast (the user may be switching the deck on). Once the PC has been idle,
/// each further miss doubles the wait (`idle_misses` counts those): 30 s, 1
/// min, then 2 min.
pub fn retry_delay(idle_misses: u32, pc_active: bool) -> Duration {
    if pc_active {
        return RETRY_EVERY;
    }
    // The cap only keeps the shift from overflowing; `RETRY_MAX` caps the wait.
    let secs = RETRY_EVERY.as_secs() << idle_misses.min(10);
    Duration::from_secs(secs).min(RETRY_MAX)
}

/// Whether the PC has had input lately. Unknown counts as in use (stay fast).
pub fn pc_active_from(idle: Option<Duration>) -> bool {
    idle.is_none_or(|d| d < PC_ACTIVE_WITHIN)
}

fn pc_active() -> bool {
    let idle = crate::platform::input_idle_for()
        .or_else(|| crate::hooks::input_idle_ms(now_ms().max(0) as u64).map(Duration::from_millis));
    pc_active_from(idle)
}

fn describe_interval(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 120 && s.is_multiple_of(60) {
        format!("{} min", s / 60)
    } else {
        format!("{s} s")
    }
}

/// The watchdog's miss count and what it has already said in the log.
#[derive(Debug, Default, PartialEq)]
struct Backoff {
    /// Misses while the PC was idle.
    misses: u32,
    /// The interval last announced.
    announced: Option<Duration>,
}

impl Backoff {
    /// A try failed: the wait before the next, and a log line when the
    /// interval changed (and one for the first miss).
    fn miss(&mut self, pc_active: bool) -> (Duration, Option<String>) {
        self.misses = if pc_active { 0 } else { self.misses + 1 };
        let delay = retry_delay(self.misses, pc_active);
        let line = if self.announced == Some(delay) {
            None
        } else if delay > RETRY_EVERY {
            Some(format!(
                "Treadmill not found; retrying every {}.",
                describe_interval(delay)
            ))
        } else if self.announced.is_none() {
            Some("Treadmill not found; will keep trying.".to_string())
        } else {
            None
        };
        self.announced = Some(delay);
        (delay, line)
    }

    fn reset(&mut self) {
        *self = Backoff::default();
    }
}
/// Connect tries inside one connect attempt (see `bluetooth.rs`).
/// A Windows BLE connect often fails once and works on the next try; three
/// tries, then the watchdog takes over.
pub const CONNECT_TRIES: u32 = 3;

/// What the link is doing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum LinkState {
    /// No saved treadmill and nothing in progress.
    Idle,
    Scanning,
    Connecting {
        attempt: u32,
        of: u32,
    },
    Connected,
    /// The link is down and the watchdog will try again.
    Reconnecting {
        attempt: u32,
        next_retry_s: u32,
    },
    /// No Bluetooth adapter, or Bluetooth is switched off.
    AdapterUnavailable,
    /// The last connect attempt failed and nothing is retrying.
    Failed {
        message: String,
    },
    /// Connected, but no telemetry has arrived for a while.
    NoData {
        since_s: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct LastError {
    pub message: String,
    /// Epoch ms.
    #[ts(type = "number")]
    pub at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum LinkErrorKind {
    NotFound,
    /// A scan saw no Bluetooth device of any kind: the PC's Bluetooth is
    /// stuck, not the treadmill.
    NoDevices,
    AdapterOff,
    AdapterMissing,
    Permission,
    Timeout,
    Generic,
}

/// A scan or connect failure, as returned to the window that asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct LinkError {
    pub kind: LinkErrorKind,
    pub message: String,
}

/// Sort a raw btleplug / connect error into a kind. Pure.
fn classify(raw: &str) -> LinkErrorKind {
    let r = raw.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| r.contains(n));
    if has(&["no bluetooth adapter", "no adapters", "adapter not found"]) {
        LinkErrorKind::AdapterMissing
    } else if has(&[
        "access is denied",
        "access denied",
        "permission",
        "not authorized",
        "unauthorized",
    ]) {
        LinkErrorKind::Permission
    } else if has(&[
        "not powered",
        "powered off",
        "radio",
        "turned off",
        "bluetooth is off",
        "bluetooth is disabled",
        "adapter is off",
        "adapter is disabled",
    ]) {
        LinkErrorKind::AdapterOff
    } else if has(&["timed out", "timeout", "after 3 full cycles"]) {
        LinkErrorKind::Timeout
    } else if has(&[NO_DEVICES_SEEN]) {
        LinkErrorKind::NoDevices
    } else if has(&["not found", "no such device"]) {
        LinkErrorKind::NotFound
    } else {
        LinkErrorKind::Generic
    }
}

/// The raw error for a scan that saw no device at all (lower case: `classify`
/// matches on it).
pub const NO_DEVICES_SEEN: &str = "no bluetooth devices seen";

/// The raw error for a BLE call (`step`, such as "Connect") that passed its
/// time limit. `classify` reads it as a timeout, so the windows show the
/// timeout sentence.
pub fn timed_out(step: &str, limit: Duration) -> String {
    format!("{step} timed out after {} s", limit.as_secs_f32())
}

/// The short human message for a raw error. Pure.
pub fn describe(raw: &str) -> LinkError {
    let kind = classify(raw);
    let message = match kind {
        LinkErrorKind::NotFound => "Treadmill not found. Is it switched on?",
        LinkErrorKind::NoDevices => {
            "This PC's Bluetooth isn't seeing any devices. Turn Bluetooth off and on, or restart the PC."
        }
        LinkErrorKind::AdapterOff => "Bluetooth is off. Turn it on in Windows settings.",
        LinkErrorKind::AdapterMissing => "No Bluetooth adapter found on this PC.",
        LinkErrorKind::Permission => {
            "Windows blocked Bluetooth access. Check Settings, Privacy, Bluetooth."
        }
        LinkErrorKind::Timeout => "The treadmill did not answer. Move closer and try again.",
        LinkErrorKind::Generic => "Could not reach the treadmill. Try again.",
    };
    LinkError {
        kind,
        message: message.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Pure state derivation
// ---------------------------------------------------------------------------

/// Everything [`derive`] needs, already reduced to plain values.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub connected: bool,
    pub connecting: bool,
    pub scanning: bool,
    pub adapter_problem: bool,
    /// Current connect try, when one is running.
    pub attempt: Option<(u32, u32)>,
    pub failed: Option<String>,
    /// A treadmill is saved, so the watchdog keeps trying.
    pub has_saved: bool,
    /// Seconds until the watchdog's next try.
    pub next_retry_s: Option<u32>,
    /// Watchdog tries since the link was last up.
    pub retries: u32,
    /// Seconds since the last frame (or since linking, before the first).
    pub quiet_s: Option<u32>,
}

pub fn derive(i: &Inputs) -> LinkState {
    if i.connected {
        return match i.quiet_s {
            Some(s) if s > NO_DATA_AFTER.as_secs() as u32 => LinkState::NoData { since_s: s },
            _ => LinkState::Connected,
        };
    }
    if i.scanning && !i.connecting {
        return LinkState::Scanning;
    }
    if i.connecting {
        let (attempt, of) = i.attempt.unwrap_or((1, CONNECT_TRIES));
        return LinkState::Connecting { attempt, of };
    }
    if i.adapter_problem {
        return LinkState::AdapterUnavailable;
    }
    if i.has_saved {
        if let Some(next_retry_s) = i.next_retry_s {
            return LinkState::Reconnecting {
                attempt: i.retries.max(1),
                next_retry_s,
            };
        }
    }
    if let Some(message) = &i.failed {
        return LinkState::Failed {
            message: message.clone(),
        };
    }
    LinkState::Idle
}

// ---------------------------------------------------------------------------
// Engage: did the deck answer?
// ---------------------------------------------------------------------------

/// After an Engage, the deck has this long to report Starting or Running.
/// The handshake plus the deck's 3 s countdown take about 4 s; 8 s is twice that.
pub const START_TIMEOUT: Duration = Duration::from_secs(8);

/// Shown under Engage (and in the HUD) when the deck never answered.
const START_NO_RESPONSE: &str =
    "The treadmill didn't respond. If its screen is off, wake it with its power button or remote, then try again.";

/// Shown under Engage (and in the HUD) for a start asked for while the deck
/// winds the belt down after a stop: it refuses starts until the belt is still.
pub const START_SPINNING_DOWN: &str = "Belt is still stopping. Press Engage once it stops.";

/// Shown under Engage (and in the HUD) for a start asked for while the deck
/// is in standby: it refuses every start, and the app can't wake it. It can't
/// tell a screen timeout from a power-off, hence "off or asleep".
pub const START_STANDBY: &str = "Treadmill is off or asleep. Switch it on, then press Engage.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartCheck {
    Waiting,
    Started,
    TimedOut,
}

/// Where an Engage stands, `elapsed` after it was sent. Pure.
pub fn start_check(elapsed: Duration, status: crate::protocol::MachineStatus) -> StartCheck {
    use crate::protocol::MachineStatus;
    if matches!(status, MachineStatus::Starting | MachineStatus::Running) {
        StartCheck::Started
    } else if elapsed >= START_TIMEOUT {
        StartCheck::TimedOut
    } else {
        StartCheck::Waiting
    }
}

/// The outcome of the last Engage, for the button that sent it (same shape as
/// the Strava results, so `useBackendAction` can wait on it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct StartResult {
    pub ok: bool,
    pub message: String,
    /// Epoch ms.
    #[ts(type = "number")]
    pub at_ms: i64,
}

// ---------------------------------------------------------------------------
// Live state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Live {
    start_result: Option<StartResult>,
    /// The last raw 0x51 status byte, for the change log.
    last_status_byte: Option<u8>,
    /// We have logged that frames stopped arriving.
    frames_quiet_logged: bool,
    scanning: u32,
    adapter_problem: bool,
    attempt: Option<(u32, u32)>,
    failed: Option<String>,
    last_error: Option<LastError>,
    has_saved: bool,
    wake_at: Option<Instant>,
    retries: u32,
    backoff: Backoff,
    linked_at: Option<Instant>,
    last_frame: Option<Instant>,
    /// When the HUD last asked the dashboard to open its connect dialog (epoch ms).
    connect_dialog_at_ms: Option<i64>,
}

static LIVE: Mutex<Live> = Mutex::new(Live {
    start_result: None,
    last_status_byte: None,
    frames_quiet_logged: false,
    scanning: 0,
    adapter_problem: false,
    attempt: None,
    failed: None,
    last_error: None,
    has_saved: false,
    wake_at: None,
    retries: 0,
    backoff: Backoff {
        misses: 0,
        announced: None,
    },
    linked_at: None,
    last_frame: None,
    connect_dialog_at_ms: None,
});

static RETRY_NOW: tokio::sync::Notify = tokio::sync::Notify::const_new();

fn live() -> std::sync::MutexGuard<'static, Live> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The state and last error to put in the snapshot.
pub fn current(connected: bool, connecting: bool) -> (LinkState, Option<LastError>) {
    let l = live();
    let now = Instant::now();
    // `None` sorts first, so this is the later of the two that are set.
    let quiet_since = l.linked_at.max(l.last_frame);
    let inputs = Inputs {
        connected,
        connecting,
        scanning: l.scanning > 0,
        adapter_problem: l.adapter_problem,
        attempt: l.attempt,
        failed: l.failed.clone(),
        has_saved: l.has_saved,
        next_retry_s: l
            .wake_at
            .map(|t| t.saturating_duration_since(now).as_secs_f32().ceil() as u32),
        retries: l.retries,
        quiet_s: quiet_since.map(|t| now.saturating_duration_since(t).as_secs() as u32),
    };
    (derive(&inputs), l.last_error.clone())
}

/// Held while a scan runs.
pub struct ScanGuard;

impl ScanGuard {
    #[allow(clippy::new_without_default)] // `new` has a side effect; `default()` must not.
    pub fn new() -> Self {
        live().scanning += 1;
        ScanGuard
    }
}
impl Drop for ScanGuard {
    fn drop(&mut self) {
        let mut l = live();
        l.scanning = l.scanning.saturating_sub(1);
    }
}

/// Record a failure: remember it for the tooltip, and flag the adapter when
/// that is the cause.
pub fn note_error(err: &LinkError, raw: &str) {
    let mut l = live();
    // The watchdog hits the same failure over and over: say it once.
    if l.last_error
        .as_ref()
        .is_some_and(|e| e.message == err.message)
    {
        log::debug!("treadmill link: {} (raw: {})", err.message, raw);
    } else {
        log::warn!("treadmill link: {} (raw: {})", err.message, raw);
    }
    l.last_error = Some(LastError {
        message: err.message.clone(),
        at_ms: now_ms(),
    });
    if matches!(
        err.kind,
        LinkErrorKind::AdapterOff | LinkErrorKind::AdapterMissing
    ) {
        l.adapter_problem = true;
    }
}

/// A scan or connect got far enough to prove the adapter works.
pub fn adapter_ok() {
    live().adapter_problem = false;
}

pub fn begin_connect() {
    let mut l = live();
    l.failed = None;
    l.attempt = Some((1, CONNECT_TRIES));
}

pub fn set_attempt(attempt: u32) {
    live().attempt = Some((attempt, CONNECT_TRIES));
}

pub fn connect_failed(err: &LinkError, raw: &str) {
    note_error(err, raw);
    let mut l = live();
    l.attempt = None;
    l.failed = Some(err.message.clone());
}

pub fn mark_linked() {
    let mut l = live();
    l.attempt = None;
    l.failed = None;
    l.last_error = None;
    l.adapter_problem = false;
    if l.retries > 0 {
        log::info!("Treadmill reconnected after {} tries.", l.retries);
    }
    l.retries = 0;
    l.backoff.reset();
    l.has_saved = true;
    l.linked_at = Some(Instant::now());
    l.last_frame = None;
    l.last_status_byte = None;
    l.frames_quiet_logged = false;
}

/// A telemetry frame arrived.
pub fn mark_frame() {
    live().last_frame = Some(Instant::now());
}

/// Log the raw 0x51 status byte whenever it changes, known or not, so a log
/// shows what the deck sends in states we don't understand yet (screen off).
/// A change also means the deck is awake, which clears a failed Engage.
pub fn note_status_byte(code: u8, known: Option<crate::protocol::MachineStatus>) {
    let mut l = live();
    if l.last_status_byte == Some(code) {
        return;
    }
    let previous = l.last_status_byte.replace(code);
    log::info!(
        "deck status byte {} ({}), was {}; frames arriving",
        code,
        known.map_or("UNKNOWN".to_string(), |s| format!("{:?}", s)),
        previous.map_or("none".to_string(), |p| p.to_string())
    );
    if previous.is_some() && l.start_result.as_ref().is_some_and(|r| !r.ok) {
        l.start_result = None;
    }
}

/// How long a link has gone without a status frame, when that is past
/// `NO_DATA_AFTER`. `since` is the later of the link coming up and its last
/// frame. Pure.
pub(crate) fn silent_for(since: Option<Instant>, now: Instant) -> Option<Duration> {
    let quiet = now.saturating_duration_since(since?);
    (quiet > NO_DATA_AFTER).then_some(quiet)
}

/// A write error that means the link's GATT session is gone for good: Windows
/// closed the device object (RO_E_CLOSED, `HRESULT(0x80000013)`, "The object
/// has been closed."). Every later write on that link fails the same way
/// while it can still read as connected, so the worker drops it at once and
/// reconnects instead of writing into it until the silent-link rule fires.
/// Pure.
pub(crate) fn write_lost_link(raw: &str) -> bool {
    raw.contains("0x80000013") || raw.to_ascii_lowercase().contains("object has been closed")
}

/// Once a second from the protocol worker: log when status frames stop
/// arriving on a live link, and when they come back. Returns how long the
/// link has been silent once that is past `NO_DATA_AFTER`: the deck answers
/// every poll (in standby too), so a link that reads as up but stays silent
/// that long has a stale subscription, and the worker drops it to reconnect.
pub fn log_frame_flow() -> Option<Duration> {
    let mut l = live();
    let now = Instant::now();
    let since = l.linked_at.max(l.last_frame)?;
    let silent = silent_for(Some(since), now);
    let quiet = now.saturating_duration_since(since);
    if silent.is_some() && !l.frames_quiet_logged {
        l.frames_quiet_logged = true;
        log::info!(
            "no status frames for {} s (last status byte {})",
            quiet.as_secs(),
            l.last_status_byte
                .map_or("none".to_string(), |b| b.to_string())
        );
    // Frames are arriving again: about three polls apart at most.
    } else if quiet <= Duration::from_secs(2) && l.frames_quiet_logged {
        l.frames_quiet_logged = false;
        log::info!("status frames arriving again");
    }
    silent
}

/// An Engage was sent.
pub fn start_requested() {
    live().start_result = None;
}

pub fn start_succeeded() {
    live().start_result = Some(StartResult {
        ok: true,
        message: String::new(),
        at_ms: now_ms(),
    });
}

/// The deck never answered an Engage. Logged with what we know, so the log
/// shows the state it was in.
pub fn start_timed_out(status: crate::protocol::MachineStatus, unknown_code: Option<u8>) {
    let mut l = live();
    let quiet_s = l
        .last_frame
        .map(|t| Instant::now().saturating_duration_since(t).as_secs());
    log::warn!(
        "Engage: no answer from the deck in {} s; stopped re-sending start. status={:?} unknown_code={:?} last_status_byte={:?} last_frame={}",
        START_TIMEOUT.as_secs(),
        status,
        unknown_code,
        l.last_status_byte,
        quiet_s.map_or("never".to_string(), |s| format!("{} s ago", s))
    );
    let at_ms = now_ms();
    l.start_result = Some(StartResult {
        ok: false,
        message: START_NO_RESPONSE.to_string(),
        at_ms,
    });
    l.last_error = Some(LastError {
        message: START_NO_RESPONSE.to_string(),
        at_ms,
    });
}

/// A start the deck can't take now was not sent: say why at once, where the
/// Engage result shows.
pub fn start_refused(message: &str) {
    live().start_result = Some(StartResult {
        ok: false,
        message: message.to_string(),
        at_ms: now_ms(),
    });
}

/// How the last Engage went (None while one is waiting, or before any).
pub fn start_result() -> Option<StartResult> {
    live().start_result.clone()
}

// Watchdog reporting.

pub fn set_saved_device(saved: bool) {
    live().has_saved = saved;
}

/// What the watchdog last saw: a treadmill is saved.
pub fn has_saved_device() -> bool {
    live().has_saved
}

/// The watchdog starts a try. Only the first of a run is logged at info.
pub fn count_retry(device: &str) {
    let mut l = live();
    l.retries += 1;
    if l.backoff.announced.is_none() {
        log::info!("Treadmill link is down. Trying to reconnect to {device}.");
    } else {
        log::debug!("Reconnect try {} for {device}.", l.retries);
    }
}

/// The watchdog's try ended: how long to wait before the next one. Logs the
/// first miss and each change of interval (`mark_linked` logs the reconnect).
pub fn retry_finished(linked: bool) -> Duration {
    let active = pc_active();
    let mut l = live();
    if linked {
        l.backoff.reset();
        return RETRY_EVERY;
    }
    let (delay, line) = l.backoff.miss(active);
    if let Some(line) = line {
        log::info!("{line}");
    }
    delay
}

/// Back to the fast interval: the user pressed Connect or ran a scan.
pub fn reset_backoff() {
    live().backoff.reset();
}

/// The watchdog is about to sleep until `after` from now (or until woken).
pub fn set_wake(after: Duration) {
    live().wake_at = Some(Instant::now() + after);
}

/// The watchdog waits here after a try; `connect_now` wakes it. It waits in
/// short slices and re-checks the PC each time, so input after a long idle
/// brings the next try back to the fast interval at once. The windows'
/// countdown (`set_wake`) follows each slice.
pub async fn wait_for_retry(delay: Duration) {
    let started = Instant::now();
    loop {
        let target = if pc_active() {
            RETRY_EVERY.min(delay)
        } else {
            delay
        };
        let remaining = target.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return;
        }
        set_wake(remaining);
        tokio::select! {
            _ = tokio::time::sleep(remaining.min(WAIT_SLICE)) => {}
            _ = RETRY_NOW.notified() => return,
        }
    }
}

/// "Retry now": wake the watchdog so it tries immediately.
pub fn connect_now(app: &tauri::AppHandle) -> Result<(), String> {
    if crate::persistence::load_config(app)
        .last_device_id
        .is_none()
    {
        return Err("No treadmill is saved yet. Use Connect to pick one.".to_string());
    }
    reset_backoff();
    RETRY_NOW.notify_one();
    Ok(())
}

/// Wake the watchdog for a try now: the protocol worker dropped a link that
/// went silent.
pub fn retry_soon() {
    RETRY_NOW.notify_one();
}

/// The next "open the connect dialog" request time: now, and always later
/// than `prev`, so the dashboard sees every click as new.
fn next_request_ms(prev: Option<i64>, now_ms: i64) -> i64 {
    prev.map_or(now_ms, |p| now_ms.max(p + 1))
}

/// The HUD's link label with no treadmill saved: show the dashboard and have
/// it open the connect dialog (it follows `connection.connect_dialog_at_ms`).
pub fn open_connect_dialog(app: &tauri::AppHandle) {
    {
        let mut live = live();
        live.connect_dialog_at_ms = Some(next_request_ms(
            live.connect_dialog_at_ms,
            crate::util::now_ms(),
        ));
    }
    crate::dashboard::show_dashboard(app);
}

/// When the dashboard was last asked to open its connect dialog.
pub fn connect_dialog_at_ms() -> Option<i64> {
    live().connect_dialog_at_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_closed_object_write_means_the_link_is_lost() {
        assert!(write_lost_link(crate::sim::CLOSED_WRITE));
        assert!(write_lost_link("The object has been closed."));
        assert!(!write_lost_link(
            r#"Error { code: HRESULT(0x800704C7), message: "The operation was canceled by the user." }"#
        ));
        assert!(!write_lost_link(&timed_out(
            "Write",
            std::time::Duration::from_secs(2)
        )));
        assert!(!write_lost_link("Simulated deck not linked"));
    }

    #[test]
    fn retry_delay_is_fast_then_doubles_to_a_cap() {
        let secs = |m| retry_delay(m, false).as_secs();
        assert_eq!(
            (1..=5).map(secs).collect::<Vec<_>>(),
            [30, 60, 120, 120, 120]
        );
        assert_eq!(secs(u32::MAX), 120);
    }

    #[test]
    fn retry_delay_stays_fast_while_the_pc_is_in_use() {
        for m in [0, 1, 4, 50, u32::MAX] {
            assert_eq!(retry_delay(m, true).as_secs(), 15);
        }
    }

    #[test]
    fn pc_counts_as_idle_after_five_minutes() {
        let s = Duration::from_secs;
        assert!(pc_active_from(None));
        assert!(pc_active_from(Some(s(299))));
        assert!(!pc_active_from(Some(s(300))));
    }

    #[test]
    fn backoff_logs_the_first_miss_and_each_change_then_resets() {
        let mut b = Backoff::default();
        let run: Vec<(u64, bool)> = (0..8)
            .map(|i| {
                // Two misses while the PC is in use, then it goes idle.
                let (d, line) = b.miss(i < 2);
                (d.as_secs(), line.is_some())
            })
            .collect();
        assert_eq!(
            run,
            [
                (15, true),
                (15, false),
                (30, true),
                (60, true),
                (120, true),
                (120, false),
                (120, false),
                (120, false)
            ]
        );
        b.reset();
        assert_eq!(b, Backoff::default());
        assert_eq!(b.miss(true).0.as_secs(), 15);
    }

    #[test]
    fn activity_after_idle_goes_back_to_fast() {
        let mut b = Backoff::default();
        for _ in 0..5 {
            b.miss(false);
        }
        assert_eq!(b.miss(false).0.as_secs(), 120);
        assert_eq!(b.miss(true).0.as_secs(), 15);
        assert_eq!(b.miss(false).0.as_secs(), 30);
    }

    #[test]
    fn each_connect_dialog_request_is_later_than_the_last() {
        assert_eq!(next_request_ms(None, 5_000), 5_000);
        assert_eq!(next_request_ms(Some(4_000), 5_000), 5_000);
        // Two clicks in the same millisecond (or a clock step back) still differ.
        assert_eq!(next_request_ms(Some(5_000), 5_000), 5_001);
        assert_eq!(next_request_ms(Some(9_000), 5_000), 9_001);
    }

    fn base() -> Inputs {
        Inputs::default()
    }

    #[test]
    fn classifies_known_errors() {
        assert_eq!(
            classify("No Bluetooth adapters found"),
            LinkErrorKind::AdapterMissing
        );
        assert_eq!(classify("Device not found"), LinkErrorKind::NotFound);
        assert_eq!(
            classify("Device not found, previously discovered?"),
            LinkErrorKind::NotFound
        );
        assert_eq!(classify("Permission denied"), LinkErrorKind::Permission);
        assert_eq!(
            classify("Access is denied. (0x80070005)"),
            LinkErrorKind::Permission
        );
        assert_eq!(
            classify("The Bluetooth radio is turned off"),
            LinkErrorKind::AdapterOff
        );
        assert_eq!(
            classify("Failed to connect to X after 3 full cycles."),
            LinkErrorKind::Timeout
        );
        assert_eq!(
            classify("Timed out waiting for the device"),
            LinkErrorKind::Timeout
        );
        assert_eq!(classify("something odd"), LinkErrorKind::Generic);
    }

    #[test]
    fn a_ble_call_past_its_limit_reads_as_a_timeout() {
        let raw = timed_out("Connect", Duration::from_secs(15));
        assert_eq!(raw, "Connect timed out after 15 s");
        let e = describe(&raw);
        assert_eq!(e.kind, LinkErrorKind::Timeout);
        assert_eq!(
            e.message,
            "The treadmill did not answer. Move closer and try again."
        );
        assert_eq!(
            timed_out("Write", Duration::from_millis(2_500)),
            "Write timed out after 2.5 s"
        );
    }

    #[test]
    fn messages_are_short_and_human() {
        let none = describe(NO_DEVICES_SEEN);
        assert_eq!(none.kind, LinkErrorKind::NoDevices);
        assert!(none.message.contains("isn't seeing any devices"));
        let e = describe("Device not found");
        assert_eq!(e.kind, LinkErrorKind::NotFound);
        assert_eq!(e.message, "Treadmill not found. Is it switched on?");
        for raw in [
            "",
            "No Bluetooth adapters found",
            "boom",
            "Permission denied",
            "timeout",
        ] {
            let m = describe(raw).message;
            assert!(m.len() < 80 && !m.contains('\n'), "{m}");
        }
    }

    #[test]
    fn connected_shows_no_data_after_ten_seconds() {
        let i = Inputs {
            connected: true,
            quiet_s: Some(3),
            ..base()
        };
        assert_eq!(derive(&i), LinkState::Connected);
        let i = Inputs {
            connected: true,
            quiet_s: Some(10),
            ..base()
        };
        assert_eq!(derive(&i), LinkState::Connected);
        let i = Inputs {
            connected: true,
            quiet_s: Some(11),
            ..base()
        };
        assert_eq!(derive(&i), LinkState::NoData { since_s: 11 });
        let i = Inputs {
            connected: true,
            quiet_s: None,
            ..base()
        };
        assert_eq!(derive(&i), LinkState::Connected);
    }

    #[test]
    fn a_link_counts_as_silent_after_ten_seconds_without_a_frame() {
        let t = Instant::now();
        let s = Duration::from_secs;
        assert_eq!(silent_for(None, t + s(60)), None, "never linked");
        assert_eq!(silent_for(Some(t), t + s(10)), None);
        assert_eq!(silent_for(Some(t), t + s(11)), Some(s(11)));
        // A frame resets it.
        assert_eq!(silent_for(Some(t + s(5)), t + s(11)), None);
    }

    #[test]
    fn scanning_connecting_and_adapter() {
        assert_eq!(
            derive(&Inputs {
                scanning: true,
                ..base()
            }),
            LinkState::Scanning
        );
        assert_eq!(
            derive(&Inputs {
                connecting: true,
                attempt: Some((2, 3)),
                ..base()
            }),
            LinkState::Connecting { attempt: 2, of: 3 }
        );
        assert_eq!(
            derive(&Inputs {
                connecting: true,
                scanning: true,
                ..base()
            }),
            LinkState::Connecting { attempt: 1, of: 3 }
        );
        assert_eq!(
            derive(&Inputs {
                adapter_problem: true,
                has_saved: true,
                next_retry_s: Some(5),
                ..base()
            }),
            LinkState::AdapterUnavailable
        );
    }

    #[test]
    fn engage_waits_eight_seconds_for_the_deck() {
        use crate::protocol::MachineStatus::*;
        let s = Duration::from_secs;
        assert_eq!(start_check(s(0), Stopped), StartCheck::Waiting);
        assert_eq!(
            start_check(Duration::from_millis(7_900), Stopped),
            StartCheck::Waiting
        );
        assert_eq!(
            start_check(Duration::from_millis(7_900), Paused),
            StartCheck::Waiting
        );
        assert_eq!(start_check(s(8), Stopped), StartCheck::TimedOut);
        assert_eq!(start_check(s(30), Paused), StartCheck::TimedOut);
        assert_eq!(start_check(s(1), Starting), StartCheck::Started);
        assert_eq!(start_check(s(3), Running), StartCheck::Started);
        // Late is still a start, not a failure.
        assert_eq!(start_check(s(20), Running), StartCheck::Started);
        assert!(START_NO_RESPONSE.starts_with("The treadmill didn't respond."));
    }

    #[test]
    fn reconnecting_needs_a_saved_device() {
        let i = Inputs {
            has_saved: true,
            next_retry_s: Some(12),
            retries: 2,
            failed: Some("x".into()),
            ..base()
        };
        assert_eq!(
            derive(&i),
            LinkState::Reconnecting {
                attempt: 2,
                next_retry_s: 12
            }
        );
        let i = Inputs {
            has_saved: false,
            next_retry_s: Some(12),
            failed: Some("x".into()),
            ..base()
        };
        assert_eq!(
            derive(&i),
            LinkState::Failed {
                message: "x".into()
            }
        );
        assert_eq!(derive(&base()), LinkState::Idle);
    }
}
