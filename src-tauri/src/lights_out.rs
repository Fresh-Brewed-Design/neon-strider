//! Lights out: the treadmill goes dark when the PC's display does.
//!
//! When the display turns off (`platform::display_off`: the power plan's
//! timeout, the lock screen's, a monitor-off tool) while the treadmill is
//! on, the belt stops so the deck can go to its own sleep, screen off
//! (status byte 6, about 14 minutes after a stop; docs/protocol.md). The deck
//! has no known command for standby or power-off, so a normal Stop is all the
//! app sends:
//!
//! - **Manual walk, or a manual pause:** Stop, which ends and saves the
//!   session.
//! - **A program walking:** the program pauses and waits for GO
//!   (`ProgramWait::LightsOut`), so it carries on at the same step later. Its
//!   pause winds the belt down; the deck ends a long pause by itself.
//! - **Anything else** (still, stopping, a program resting or already paused,
//!   in standby, offline): nothing; the deck sleeps on its own.
//!
//! It acts once per display-off period, on the first second the display is
//! off: a walk started while the display is already off (at the treadmill)
//! is left alone. The display coming back on never starts anything: after a
//! Stop the intents are gone and `hold_auto_start` is set, and a waiting
//! program needs GO. Off by default.
//!
//! `LightsOut` is the pure decision; `belt` and `apply` read and act on the
//! orchestrator for `desk.rs`, which runs it once a second.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::engine::{DeckAction, OrchestratorState};
use crate::persistence;
use crate::programs::{ProgramPhase, ProgramWait};
use crate::protocol::MachineStatus;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Instant;
use tauri::AppHandle;

const KEY_ENABLED: &str = "lights_out_enabled";

/// The HUD toast type (`SYS // LIGHTS_OUT`).
pub const TOAST_KIND: &str = "LIGHTS_OUT";
/// The HUD toast when the belt stops.
pub const TOAST_STOPPED: &str = "DISPLAY DARK // DECK POWERING DOWN";
/// The HUD toast when a program pauses.
pub const TOAST_PAUSED: &str = "DISPLAY DARK // PROGRAM HELD FOR GO";
/// The HUD toast when the display comes back on after either.
pub const TOAST_BACK: &str = "DISPLAY BACK // BELT HELD STILL";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct LightsOutSnapshot {
    /// Stop the belt when the PC's display turns off.
    pub enabled: bool,
}

/// What the belt is doing, as lights out sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Belt {
    /// Not moving and nothing to stop: stopped, stopping, a stop in flight,
    /// in standby, or a program resting or paused.
    Still,
    /// A start is on its way (requested, or the deck's 3-2-1 countdown, when
    /// it refuses a Stop): wait for it to run, then act.
    Starting,
    /// Walking with no program.
    Walking,
    /// A program's walking step.
    ProgramWalking,
    /// Paused by the treadmill's pause with no program: the deck is still on,
    /// with the session open.
    Paused,
}

/// What the desk loop tells the machine each second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Input {
    /// The treadmill link is up and has reported.
    pub connected: bool,
    pub display_off: bool,
    pub belt: Belt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Stop the belt (ends the session).
    Stop,
    /// Pause the walking program until GO.
    PauseProgram,
    /// The display is back on after a stop or pause: say what happened.
    /// Touches nothing.
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    /// The display is on.
    #[default]
    Watching,
    /// The display went off during a start: act once it runs.
    Armed,
    /// Done for this display-off period; `acted` if something was stopped.
    Spent { acted: bool },
}

#[derive(Debug, Default)]
pub struct LightsOut {
    phase: Phase,
}

impl LightsOut {
    /// Advance one second. Returns what to do, if anything. Never starts or
    /// resumes the belt.
    pub fn tick(&mut self, enabled: bool, input: &Input) -> Option<Effect> {
        if !input.display_off {
            let acted = matches!(self.phase, Phase::Spent { acted: true });
            self.phase = Phase::Watching;
            return acted.then_some(Effect::Back);
        }
        match self.phase {
            Phase::Spent { .. } => None,
            Phase::Watching | Phase::Armed if !enabled || !input.connected => {
                self.phase = Phase::Spent { acted: false };
                None
            }
            Phase::Watching | Phase::Armed => {
                let effect = match input.belt {
                    Belt::Starting => {
                        self.phase = Phase::Armed;
                        return None;
                    }
                    Belt::Still => None,
                    Belt::Walking | Belt::Paused => Some(Effect::Stop),
                    Belt::ProgramWalking => Some(Effect::PauseProgram),
                };
                self.phase = Phase::Spent {
                    acted: effect.is_some(),
                };
                effect
            }
        }
    }
}

/// What the belt is doing on the orchestrator.
pub fn belt(state: &OrchestratorState) -> Belt {
    if state.stop_requested || state.deck_standby {
        return Belt::Still;
    }
    if let Some(run) = state.program.as_ref() {
        let walking = run.phase == ProgramPhase::Walking && !run.paused;
        return match state.last_status {
            MachineStatus::Running if walking => Belt::ProgramWalking,
            MachineStatus::Starting if walking => Belt::Starting,
            _ if walking && state.start_requested_at.is_some() => Belt::Starting,
            _ => Belt::Still,
        };
    }
    match state.last_status {
        MachineStatus::Running => Belt::Walking,
        MachineStatus::Paused => Belt::Paused,
        MachineStatus::Starting => Belt::Starting,
        MachineStatus::Stopped if state.start_requested_at.is_some() => Belt::Starting,
        MachineStatus::Stopped | MachineStatus::Stopping => Belt::Still,
    }
}

/// The HUD toast for an effect.
pub fn toast(effect: Effect) -> &'static str {
    match effect {
        Effect::Stop => TOAST_STOPPED,
        Effect::PauseProgram => TOAST_PAUSED,
        Effect::Back => TOAST_BACK,
    }
}

/// Do a Stop or PauseProgram on the orchestrator. `Back` touches nothing.
pub fn apply(state: &mut OrchestratorState, effect: Effect, now: Instant) {
    match effect {
        Effect::Stop => {
            log::info!("Lights out: the display turned off; stopping the belt");
            state.trigger_action(DeckAction::Stop);
            // The stop already cleared every intent; the hold makes sure no
            // automatic start or resume can follow when the display wakes.
            state.hold_auto_start = true;
        }
        Effect::PauseProgram => {
            log::info!("Lights out: the display turned off; pausing the program until GO");
            if !state.hold_program_for_go(ProgramWait::LightsOut, now) {
                log::warn!("Lights out: the program was not walking; nothing paused");
            }
        }
        Effect::Back => log::info!("Lights out: the display is back on; the belt stays still"),
    }
}

// ---------------------------------------------------------------------------
// Settings and shared state
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Runtime {
    pub enabled: bool,
    pub machine: LightsOut,
}

static STATE: Mutex<Option<Runtime>> = Mutex::new(None);

/// Run `f` on the shared setting and machine.
pub fn with<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Runtime::default))
}

/// The stored value; anything unreadable is off.
pub fn enabled_from_raw(raw: Option<&str>) -> bool {
    persistence::parse_setting(raw).unwrap_or(false)
}

/// Load the setting. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let enabled = enabled_from_raw(persistence::read_setting(app, KEY_ENABLED).as_deref());
    with(|rt| rt.enabled = enabled);
}

/// Blocking (SQLite).
pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_ENABLED, &enabled.to_string())?;
    with(|rt| rt.enabled = enabled);
    log::info!("Lights out {}", if enabled { "on" } else { "off" });
    Ok(())
}

/// For the snapshot. No I/O.
pub fn snapshot() -> LightsOutSnapshot {
    with(|rt| LightsOutSnapshot {
        enabled: rt.enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(display_off: bool, belt: Belt) -> Input {
        Input {
            connected: true,
            display_off,
            belt,
        }
    }

    /// Display on for a while, then off: what the first off second does.
    fn off_with(enabled: bool, belt: Belt) -> Option<Effect> {
        let mut m = LightsOut::default();
        for _ in 0..5 {
            assert_eq!(m.tick(enabled, &input(false, belt)), None);
        }
        m.tick(enabled, &input(true, belt))
    }

    #[test]
    fn display_off_stops_a_walk_or_a_manual_pause_and_pauses_a_program() {
        assert_eq!(off_with(true, Belt::Walking), Some(Effect::Stop));
        assert_eq!(off_with(true, Belt::Paused), Some(Effect::Stop));
        assert_eq!(
            off_with(true, Belt::ProgramWalking),
            Some(Effect::PauseProgram)
        );
    }

    #[test]
    fn a_still_belt_is_left_to_sleep_by_itself() {
        assert_eq!(off_with(true, Belt::Still), None);
    }

    #[test]
    fn off_by_default_and_does_nothing_when_off() {
        assert!(!Runtime::default().enabled);
        assert!(!enabled_from_raw(None));
        assert!(!enabled_from_raw(Some("junk")));
        assert!(enabled_from_raw(Some("true")));
        for belt in [Belt::Walking, Belt::Paused, Belt::ProgramWalking] {
            assert_eq!(off_with(false, belt), None, "{belt:?}");
        }
    }

    #[test]
    fn offline_does_nothing() {
        let mut m = LightsOut::default();
        let offline = Input {
            connected: false,
            ..input(true, Belt::Walking)
        };
        assert_eq!(m.tick(true, &offline), None);
    }

    #[test]
    fn acts_once_per_display_off_period() {
        let mut m = LightsOut::default();
        assert_eq!(
            m.tick(true, &input(true, Belt::Walking)),
            Some(Effect::Stop)
        );
        // Started again at the treadmill with the display still off: the
        // person is there, so it is left alone.
        for belt in [Belt::Still, Belt::Starting, Belt::Walking, Belt::Paused] {
            for _ in 0..60 {
                assert_eq!(m.tick(true, &input(true, belt)), None, "{belt:?}");
            }
        }
    }

    #[test]
    fn a_walk_started_with_the_display_already_off_is_left_alone() {
        let mut m = LightsOut::default();
        for _ in 0..60 {
            assert_eq!(m.tick(true, &input(true, Belt::Still)), None);
        }
        for _ in 0..60 {
            assert_eq!(m.tick(true, &input(true, Belt::Walking)), None);
        }
    }

    #[test]
    fn turned_on_while_the_display_is_off_waits_for_the_next_off() {
        let mut m = LightsOut::default();
        assert_eq!(m.tick(false, &input(true, Belt::Walking)), None);
        assert_eq!(m.tick(true, &input(true, Belt::Walking)), None);
        assert_eq!(m.tick(true, &input(false, Belt::Walking)), None);
        assert_eq!(
            m.tick(true, &input(true, Belt::Walking)),
            Some(Effect::Stop)
        );
    }

    #[test]
    fn a_start_in_flight_is_stopped_once_it_runs() {
        let mut m = LightsOut::default();
        for _ in 0..4 {
            assert_eq!(m.tick(true, &input(true, Belt::Starting)), None);
        }
        assert_eq!(
            m.tick(true, &input(true, Belt::Walking)),
            Some(Effect::Stop)
        );
        let mut m = LightsOut::default();
        m.tick(true, &input(true, Belt::Starting));
        assert_eq!(
            m.tick(true, &input(true, Belt::ProgramWalking)),
            Some(Effect::PauseProgram)
        );
    }

    #[test]
    fn the_display_back_on_never_starts_anything() {
        for belt in [Belt::Walking, Belt::ProgramWalking] {
            let mut m = LightsOut::default();
            assert!(m.tick(true, &input(true, belt)).is_some());
            for _ in 0..600 {
                assert_eq!(m.tick(true, &input(true, Belt::Still)), None);
            }
            // Back on: only the notice, once; never a start.
            assert_eq!(m.tick(true, &input(false, Belt::Still)), Some(Effect::Back));
            for _ in 0..600 {
                assert_eq!(m.tick(true, &input(false, Belt::Still)), None);
            }
        }
        // Nothing was stopped: no notice either.
        let mut m = LightsOut::default();
        m.tick(true, &input(true, Belt::Still));
        assert_eq!(m.tick(true, &input(false, Belt::Still)), None);
    }

    fn walking_manual() -> OrchestratorState {
        OrchestratorState {
            last_status: MachineStatus::Running,
            hold_auto_start: false,
            ..OrchestratorState::default()
        }
    }

    #[test]
    fn belt_reads_the_orchestrator() {
        assert_eq!(belt(&walking_manual()), Belt::Walking);
        let s = OrchestratorState {
            last_status: MachineStatus::Paused,
            ..walking_manual()
        };
        assert_eq!(belt(&s), Belt::Paused);
        for status in [MachineStatus::Stopped, MachineStatus::Stopping] {
            let s = OrchestratorState {
                last_status: status,
                ..walking_manual()
            };
            assert_eq!(belt(&s), Belt::Still, "{status:?}");
        }
        let s = OrchestratorState {
            stop_requested: true,
            ..walking_manual()
        };
        assert_eq!(belt(&s), Belt::Still, "a stop in flight");
        let s = OrchestratorState {
            deck_standby: true,
            last_status: MachineStatus::Stopped,
            ..walking_manual()
        };
        assert_eq!(belt(&s), Belt::Still, "standby");
        let s = OrchestratorState {
            last_status: MachineStatus::Starting,
            ..walking_manual()
        };
        assert_eq!(belt(&s), Belt::Starting);
    }

    #[test]
    fn a_stop_clears_the_intents_and_holds_automatic_starts() {
        let mut s = walking_manual();
        apply(&mut s, Effect::Stop, Instant::now());
        assert!(s.stop_requested);
        assert!(s.hold_auto_start);
        assert_eq!(s.manual_intent, None);
        assert_eq!(s.program_intent, None);
        // The display back on touches nothing.
        let before = (s.stop_requested, s.hold_auto_start, s.manual_intent);
        apply(&mut s, Effect::Back, Instant::now());
        assert_eq!(
            (s.stop_requested, s.hold_auto_start, s.manual_intent),
            before
        );
    }
}
