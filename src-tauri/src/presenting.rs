//! Hold cards while presenting (System -> Desk, on by default).
//!
//! While Windows says the user is presenting or a full-screen app is in front
//! (`platform::presenting`, polled by the desk loop once a second), the HUD
//! holds back what can wait: the sit-too-long card, the eye-break heads-up,
//! the last-week card, the update offer, the belt service card and plain
//! toasts. Cards that need an answer still show: walk-away, the eye-break
//! resume card, a break already running (its pause holds the belt), and a
//! program waiting for GO, which lives on the panel. Program toasts (a rest
//! ending, GO taken at the treadmill) still show too.
//!
//! Each card is still decided by its own machine, every second, so when the
//! hold ends the ones still relevant show at once and the rest are gone. The
//! eye-break machine is told (`eyebreak::Tick::held`) so no break begins
//! unseen; it keeps a heads-up's minute in reserve for when the hold ends.
//! Toasts are moments, so a held toast is dropped, not saved for later.
//!
//! It never touches the belt: it only hides cards and toasts.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use super::{eyebreak::EyeBreakPhase, AppSnapshot};
use crate::persistence;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

const KEY_ENABLED: &str = "presenting_hold_cards";

/// The toast type that still shows while held: a running program's toasts,
/// which carry SKIP REST or say GO was taken at the treadmill.
const URGENT_TOAST: &str = "AUTOMATION";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PresentingSnapshot {
    /// The switch: hold cards while presenting.
    pub enabled: bool,
    /// Windows says the user is presenting or a full-screen app is in front.
    pub presenting: bool,
    /// Cards and toasts are held back now: switched on and presenting.
    pub holding: bool,
}

#[derive(Debug)]
struct Runtime {
    enabled: bool,
    presenting: bool,
}

static STATE: Mutex<Runtime> = Mutex::new(Runtime {
    enabled: true,
    presenting: false,
});

fn with<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// On when never set.
fn enabled_from_raw(raw: Option<&str>) -> bool {
    persistence::parse_setting(raw).unwrap_or(true)
}

/// Load the saved switch. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let enabled = enabled_from_raw(persistence::read_setting(app, KEY_ENABLED).as_deref());
    with(|rt| rt.enabled = enabled);
}

/// The switch, saved.
pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_ENABLED, &enabled.to_string())?;
    with(|rt| rt.enabled = enabled);
    log::info!(
        "Hold cards while presenting {}",
        if enabled { "on" } else { "off" }
    );
    Ok(())
}

/// What Windows says this second (the desk loop).
pub fn observe(presenting: bool) {
    let changed = with(|rt| std::mem::replace(&mut rt.presenting, presenting) != presenting);
    if changed {
        log::info!(
            "Presenting {}",
            if presenting { "started" } else { "ended" }
        );
    }
}

/// Cards and toasts are held back now.
pub fn holding() -> bool {
    with(|rt| rt.enabled && rt.presenting)
}

pub fn snapshot() -> PresentingSnapshot {
    with(|rt| PresentingSnapshot {
        enabled: rt.enabled,
        presenting: rt.presenting,
        holding: rt.enabled && rt.presenting,
    })
}

/// Hide the cards that can wait from the HUD, while `s.presenting` holds.
/// Pure. Only the HUD's own card flags change; the dashboard's data (the
/// service due on the Live page, the eye-break settings) stays as it is.
pub fn hold_cards(s: &mut AppSnapshot) {
    if !s.presenting.holding {
        return;
    }
    s.seated.show_card = false;
    s.weekly.show_card = false;
    s.update.hud_card = false;
    s.maintenance.hud_card = false;
    // The heads-up can wait. A break under way and the resume card stay: the
    // belt is paused for them.
    if s.eye_break.phase == EyeBreakPhase::Upcoming {
        s.eye_break.phase = EyeBreakPhase::Idle;
        s.eye_break.at_rest = false;
    }
}

/// A toast of this type is held back (dropped) while `holding`.
pub fn holds_toast(kind: Option<&str>, holding: bool) -> bool {
    holding && kind != Some(URGENT_TOAST)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::walkaway::WalkAwayCard;

    /// A default snapshot with every non-urgent card up.
    fn all_cards(holding: bool) -> AppSnapshot {
        let mut s = crate::snapshot::tests::sample_snapshot();
        s.seated.show_card = true;
        s.weekly.show_card = true;
        s.update.hud_card = true;
        s.maintenance.due = true;
        s.maintenance.hud_card = true;
        s.eye_break.phase = EyeBreakPhase::Upcoming;
        s.presenting = PresentingSnapshot {
            enabled: true,
            presenting: holding,
            holding,
        };
        s
    }

    #[test]
    fn presenting_holds_the_cards_that_can_wait() {
        let mut s = all_cards(true);
        hold_cards(&mut s);
        assert!(!s.seated.show_card);
        assert!(!s.weekly.show_card);
        assert!(!s.update.hud_card);
        assert!(!s.maintenance.hud_card);
        assert_eq!(s.eye_break.phase, EyeBreakPhase::Idle);
        assert!(
            s.maintenance.due,
            "the dashboard still shows the service due"
        );
    }

    #[test]
    fn the_cards_show_again_once_presenting_ends() {
        // Each tick builds the cards afresh from their machines, so the next
        // snapshot without the hold carries them as decided.
        let mut s = all_cards(false);
        hold_cards(&mut s);
        assert!(s.seated.show_card);
        assert!(s.weekly.show_card);
        assert!(s.update.hud_card);
        assert!(s.maintenance.hud_card);
        assert_eq!(s.eye_break.phase, EyeBreakPhase::Upcoming);
    }

    #[test]
    fn switched_off_nothing_is_held() {
        let mut s = all_cards(false);
        s.presenting.presenting = true;
        hold_cards(&mut s);
        assert!(s.seated.show_card);
        assert!(!holds_toast(Some("STATUS"), false));
    }

    #[test]
    fn cards_that_need_an_answer_still_show() {
        for phase in [EyeBreakPhase::Break, EyeBreakPhase::Complete] {
            let mut s = all_cards(true);
            s.eye_break.phase = phase;
            s.walkaway.card = WalkAwayCard::Warning;
            hold_cards(&mut s);
            assert_eq!(s.eye_break.phase, phase);
            assert_eq!(s.walkaway.card, WalkAwayCard::Warning);
        }
    }

    #[test]
    fn plain_toasts_are_held_and_program_toasts_pass() {
        for kind in ["STATUS", "VELOCITY", "ELEVATION", "STRAVA", "CALL", "TRAY"] {
            assert!(holds_toast(Some(kind), true), "{kind}");
        }
        assert!(!holds_toast(Some(URGENT_TOAST), true));
        assert!(!holds_toast(Some("STATUS"), false));
    }

    #[test]
    fn the_switch_is_on_when_never_set() {
        assert!(enabled_from_raw(None));
        assert!(!enabled_from_raw(Some("false")));
        assert!(enabled_from_raw(Some("true")));
    }
}
