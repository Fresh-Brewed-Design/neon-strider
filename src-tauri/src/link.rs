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

/// Connected but no telemetry frame for this long: the deck is asleep or out of range.
const NO_DATA_AFTER: Duration = Duration::from_secs(10);
/// The watchdog's retry interval.
pub const RETRY_EVERY: Duration = Duration::from_secs(15);
/// Connect tries inside one connect attempt (see `bluetooth.rs`).
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
pub const START_TIMEOUT: Duration = Duration::from_secs(8);

/// Shown under Engage (and in the HUD) when the deck never answered.
const START_NO_RESPONSE: &str =
    "The treadmill didn't respond. If its screen is off, wake it with its power button or remote, then try again.";

/// Shown under Engage (and in the HUD) for a start asked for while the deck
/// winds the belt down after a stop: it refuses starts until the belt is still.
pub const START_SPINNING_DOWN: &str = "Belt is still stopping. Press Engage once it stops.";

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
    log::warn!("treadmill link: {} (raw: {})", err.message, raw);
    let mut l = live();
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
    l.retries = 0;
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

/// Once a second from the protocol worker: log when status frames stop
/// arriving on a live link, and when they come back.
pub fn log_frame_flow() {
    let mut l = live();
    let now = Instant::now();
    let Some(since) = l.linked_at.max(l.last_frame) else {
        return;
    };
    let quiet = now.saturating_duration_since(since);
    if quiet > NO_DATA_AFTER && !l.frames_quiet_logged {
        l.frames_quiet_logged = true;
        log::info!(
            "no status frames for {} s (last status byte {})",
            quiet.as_secs(),
            l.last_status_byte
                .map_or("none".to_string(), |b| b.to_string())
        );
    } else if quiet <= Duration::from_secs(2) && l.frames_quiet_logged {
        l.frames_quiet_logged = false;
        log::info!("status frames arriving again");
    }
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

pub fn count_retry() {
    live().retries += 1;
}

/// The watchdog is about to sleep until `after` from now (or until woken).
pub fn set_wake(after: Duration) {
    live().wake_at = Some(Instant::now() + after);
}

/// The watchdog slot waits on this; `connect_now` wakes it.
pub async fn wait_for_retry(after: Duration) {
    tokio::select! {
        _ = tokio::time::sleep(after) => {}
        _ = RETRY_NOW.notified() => {}
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
    RETRY_NOW.notify_one();
    Ok(())
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
