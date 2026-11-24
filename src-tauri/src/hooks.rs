//! What the global input hooks record, and the shortcuts they fire (their
//! keys are `crate::shortcuts`).
//!
//! The hooks themselves are OS code (`platform`); these are the functions they
//! call on every key press and mouse move, so nothing here may block: atomics
//! only, and a shortcut's work runs on a thread of its own.
//!
//! Only two things are kept: the time of the last input, and (while Neural
//! flow wants it) a count of typing key presses. Never which key or button, or
//! where the pointer was.

use crate::engine::DeckAction;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

/// Typing key presses since the protocol worker last took the count.
static KEY_COUNT: AtomicUsize = AtomicUsize::new(0);
static KEY_COUNTING: AtomicBool = AtomicBool::new(false);
/// Epoch ms of the last keyboard or mouse input, plus one; 0 = none yet.
/// Wall-clock time, so a sleeping PC counts as time away.
static LAST_INPUT_MS: AtomicU64 = AtomicU64::new(0);

/// A dev build leaves every shortcut to the release app while it runs.
/// Both builds install the same system-wide hooks, and the one installed last
/// sees a key first and swallows it, so without this a dev build would take
/// the shortcuts meant for the release app. The hooks stay installed (they
/// also keep the last-input time and the HUD's click-through), and the check
/// is made per press, so it follows the release app starting or quitting.
/// Release builds never yield. Only called once a key press is a shortcut.
pub(crate) fn shortcuts_left_to_release() -> bool {
    cfg!(debug_assertions) && crate::platform::app_instance_running(crate::RELEASE_IDENTIFIER)
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Record that input just happened (any key down, any mouse event).
pub(crate) fn note_input() {
    LAST_INPUT_MS.store(epoch_ms().saturating_add(1), Ordering::Relaxed);
}

/// Record that the last input was `ago`, for an OS that reports the session's
/// idle time rather than each input (macOS, `platform`).
#[cfg(target_os = "macos")]
pub(crate) fn note_input_ago(ago: std::time::Duration) {
    LAST_INPUT_MS.store(input_at(epoch_ms(), ago), Ordering::Relaxed);
}

/// The stored form (epoch ms plus one) of an input `ago` before `now_ms`.
#[cfg_attr(not(target_os = "macos"), cfg(test))]
fn input_at(now_ms: u64, ago: std::time::Duration) -> u64 {
    let ago_ms = u64::try_from(ago.as_millis()).unwrap_or(u64::MAX);
    now_ms.saturating_sub(ago_ms).saturating_add(1)
}

/// How long before `now_ms` (epoch ms) the last keyboard or mouse input was;
/// `None` if there has been none since the app started.
pub(crate) fn input_idle_ms(now_ms: u64) -> Option<u64> {
    idle_since(LAST_INPUT_MS.load(Ordering::Relaxed), now_ms)
}

fn idle_since(stored: u64, now_ms: u64) -> Option<u64> {
    match stored {
        0 => None,
        v => Some(now_ms.saturating_sub(v - 1)),
    }
}

/// Switch key-press counting for Neural flow. Rust decides this itself (see
/// `OrchestratorState::neural_monitor_wanted`); the hooks stay installed
/// either way, for the shortcuts and the last-input time.
pub(crate) fn set_key_counting(active: bool) {
    if KEY_COUNTING.swap(active, Ordering::SeqCst) != active {
        if !active {
            KEY_COUNT.store(0, Ordering::SeqCst);
        }
        log::info!("Activity counting {}", if active { "on" } else { "off" });
    }
}

pub(crate) fn key_counting() -> bool {
    KEY_COUNTING.load(Ordering::Relaxed)
}

pub(crate) fn count_key() {
    KEY_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// The key presses counted since the last call, and start again from zero.
/// A GROUND_TRUTH tap run is listening, so the tap shortcut (Alt+Space by
/// default) is a tap. An atomic read, safe on the hook thread.
pub(crate) fn tap_listening() -> bool {
    crate::calibration::tap_listening()
}

pub(crate) fn take_key_count() -> u32 {
    u32::try_from(KEY_COUNT.swap(0, Ordering::SeqCst)).unwrap_or(u32::MAX)
}

/// A global shortcut. Its keys are a setting (`crate::shortcuts`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum Shortcut {
    SpeedUp,
    SpeedDown,
    InclineUp,
    InclineDown,
    /// Pause a moving belt, otherwise start it.
    ToggleStart,
    /// End the running program, or run the last one again.
    ToggleProgram,
    /// Minify the HUD to the running strip, or expand it again.
    ToggleMinified,
    /// GROUND_TRUTH: the belt mark passed (only while a tap run listens).
    /// Never moves the belt.
    CalibrationTap,
}

impl Shortcut {
    pub const ALL: [Shortcut; 8] = [
        Shortcut::SpeedUp,
        Shortcut::SpeedDown,
        Shortcut::InclineUp,
        Shortcut::InclineDown,
        Shortcut::ToggleStart,
        Shortcut::ToggleProgram,
        Shortcut::ToggleMinified,
        Shortcut::CalibrationTap,
    ];

    /// Its name in messages: "Speed up".
    pub fn name(self) -> &'static str {
        match self {
            Shortcut::SpeedUp => "Speed up",
            Shortcut::SpeedDown => "Speed down",
            Shortcut::InclineUp => "Incline up",
            Shortcut::InclineDown => "Incline down",
            Shortcut::ToggleStart => "Start or pause",
            Shortcut::ToggleProgram => "Run or end a program",
            Shortcut::ToggleMinified => "Minify the HUD",
            Shortcut::CalibrationTap => "GROUND_TRUTH tap",
        }
    }

    /// What it does, after "Alt+Home already ...".
    pub fn what_it_does(self) -> &'static str {
        match self {
            Shortcut::SpeedUp => "speeds the belt up",
            Shortcut::SpeedDown => "slows the belt down",
            Shortcut::InclineUp => "raises the incline",
            Shortcut::InclineDown => "lowers the incline",
            Shortcut::ToggleStart => "starts or pauses the belt",
            Shortcut::ToggleProgram => "runs or ends a program",
            Shortcut::ToggleMinified => "minifies the HUD",
            Shortcut::CalibrationTap => "taps for GROUND_TRUTH",
        }
    }

    /// The engine action, or `None` for the program and minify toggles (not
    /// engine actions).
    fn deck_action(self) -> Option<DeckAction> {
        match self {
            Shortcut::SpeedUp => Some(DeckAction::SpeedUp),
            Shortcut::SpeedDown => Some(DeckAction::SpeedDown),
            Shortcut::InclineUp => Some(DeckAction::InclineUp),
            Shortcut::InclineDown => Some(DeckAction::InclineDown),
            Shortcut::ToggleStart => Some(DeckAction::ToggleStart),
            Shortcut::ToggleProgram | Shortcut::ToggleMinified | Shortcut::CalibrationTap => None,
        }
    }

    fn label(self) -> &'static str {
        match (self, self.deck_action()) {
            (_, Some(action)) => action.as_str(),
            (Shortcut::ToggleMinified, None) => "toggle-minified",
            (Shortcut::CalibrationTap, None) => "calibration-tap",
            (_, None) => "toggle-automation",
        }
    }
}

/// Run a shortcut, off the hook thread. Ignored until the app has started.
pub(crate) fn shortcut_pressed(shortcut: Shortcut) {
    let Some(app) = crate::app_handle() else {
        return;
    };
    if shortcut == Shortcut::CalibrationTap {
        // The moment of the key press, taken on the hook thread before any
        // lock, so the tap's time is the press's. Not logged: one a pass.
        let at = Instant::now();
        std::thread::spawn(move || {
            tauri::async_runtime::block_on(async move {
                let tapped = {
                    let mut state = crate::bluetooth::ORCHESTRATOR.lock().await;
                    crate::calibration::tap(&mut state, at)
                };
                match tapped {
                    // The tap that finished the run saved its point.
                    Ok(Some(points)) => crate::calibration::persist_points(app, &points),
                    Ok(None) => {}
                    Err(e) => log::info!("calibration tap ignored: {e}"),
                }
            });
        });
        return;
    }
    std::thread::spawn(move || {
        log::info!("native shortcut action: {}", shortcut.label());
        // Shortcuts work with no window open; the HUD shows what they did.
        crate::hud::ensure_hud_native(app);
        if shortcut == Shortcut::ToggleMinified {
            crate::snapshot::toggle_minified(app);
            return;
        }
        tauri::async_runtime::block_on(async move {
            let mut state = crate::bluetooth::ORCHESTRATOR.lock().await;
            match shortcut.deck_action() {
                Some(action) => state.trigger_action(action),
                None => {
                    state.toggle_program(Instant::now());
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_time_is_none_until_the_first_input() {
        assert_eq!(idle_since(0, 5_000), None);
        // Stored as time + 1, so an input at epoch 0 is still "seen".
        assert_eq!(idle_since(1, 5_000), Some(5_000));
        assert_eq!(idle_since(4_001, 5_000), Some(1_000));
        // A clock that went backwards reads as no idle time.
        assert_eq!(idle_since(9_001, 5_000), Some(0));
    }

    #[test]
    fn idle_time_reported_by_the_os_reads_back_the_same() {
        use std::time::Duration;
        let now = 1_000_000;
        assert_eq!(idle_since(input_at(now, Duration::ZERO), now), Some(0));
        assert_eq!(
            idle_since(input_at(now, Duration::from_millis(4_500)), now),
            Some(4_500)
        );
        // Idle for longer than the epoch has run still reads as input "seen".
        assert_eq!(
            idle_since(input_at(10, Duration::from_secs(60)), 10),
            Some(10)
        );
    }

    #[test]
    fn shortcuts_name_the_actions_they_run() {
        assert_eq!(Shortcut::SpeedUp.deck_action(), Some(DeckAction::SpeedUp));
        assert_eq!(
            Shortcut::ToggleStart.deck_action(),
            Some(DeckAction::ToggleStart)
        );
        assert_eq!(Shortcut::ToggleProgram.deck_action(), None);
        assert_eq!(Shortcut::InclineDown.label(), "incline-down");
        assert_eq!(Shortcut::ToggleProgram.label(), "toggle-automation");
        assert_eq!(Shortcut::ToggleMinified.deck_action(), None);
        assert_eq!(Shortcut::ToggleMinified.label(), "toggle-minified");
    }

    #[test]
    fn the_key_count_is_taken_once() {
        // Shared statics: this is the only test that touches them.
        let _ = take_key_count();
        count_key();
        count_key();
        assert_eq!(take_key_count(), 2);
        assert_eq!(take_key_count(), 0);
    }
}
