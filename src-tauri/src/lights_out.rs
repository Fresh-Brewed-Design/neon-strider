//! Lights out: the treadmill goes dark when the PC's display does.
//!
//! When the display turns off (`platform::display_off`: the power plan's
//! timeout, the lock screen's, a monitor-off tool) while the treadmill is
//! awake, the app sends the normal Stop, whatever the belt is doing: a walk,
//! a manual pause, a program walking, resting or paused (it ends, as END
//! does), or a belt that is already still. The session is saved once, and the
//! deck then goes to its own sleep, screen off (status byte 6, about 14
//! minutes after a stop; docs/protocol.md). The deck has no known command for
//! standby or power-off, so Stop is all it gets. A deck that is already still
//! may refuse it (a nack); that is fine, and it is sent only once.
//!
//! Nothing is sent to a deck in standby or already stopping, or while a stop
//! is in flight. During a start (the 3-2-1 countdown, when the deck refuses a
//! Stop) it waits for the belt to run, then stops it.
//!
//! It acts once per display-off period, on the first second the display is
//! off: a walk started while the display is already off (at the treadmill)
//! is left alone. The display coming back on never starts anything: the Stop
//! cleared every intent and ended any program, and `hold_auto_start` is set.
//! Off by default.
//!
//! `LightsOut` is the pure decision; `belt` and `apply` read and act on the
//! orchestrator for `desk.rs`, which runs it once a second.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::engine::OrchestratorState;
use crate::persistence;
use crate::protocol::MachineStatus;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

const KEY_ENABLED: &str = "lights_out_enabled";

/// The HUD toast type (`SYS // LIGHTS_OUT`).
pub const TOAST_KIND: &str = "LIGHTS_OUT";
/// The HUD toast when the Stop goes out.
pub const TOAST_STOPPED: &str = "DISPLAY DARK // DECK SHUTDOWN";
/// The HUD toast when the display comes back on after it.
pub const TOAST_BACK: &str = "DISPLAY BACK // BELT HELD STILL";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct LightsOutSnapshot {
    /// Stop the treadmill (ending any program) when the PC's display turns off.
    pub enabled: bool,
}

/// What the deck is doing, as lights out sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Belt {
    /// Awake: walking, paused, resting, or still with its screen on. A Stop
    /// goes out.
    Awake,
    /// A start is on its way (requested, or the deck's 3-2-1 countdown, when
    /// it refuses a Stop): wait for it to run, then stop it.
    Starting,
    /// Nothing to send: in standby, already stopping, or a stop in flight.
    Asleep,
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
    /// Send the normal Stop (ends the session and any program).
    Stop,
    /// The display is back on after a Stop: say what happened. Touches
    /// nothing.
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    /// The display is on.
    #[default]
    Watching,
    /// The display went off during a start: stop once it runs.
    Armed,
    /// Done for this display-off period; `acted` if a Stop went out.
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
            Phase::Watching | Phase::Armed => match input.belt {
                Belt::Starting => {
                    self.phase = Phase::Armed;
                    None
                }
                Belt::Asleep => {
                    self.phase = Phase::Spent { acted: false };
                    None
                }
                Belt::Awake => {
                    self.phase = Phase::Spent { acted: true };
                    Some(Effect::Stop)
                }
            },
        }
    }
}

/// What the deck is doing on the orchestrator.
pub fn belt(state: &OrchestratorState) -> Belt {
    if state.stop_requested || state.deck_standby || state.last_status == MachineStatus::Stopping {
        return Belt::Asleep;
    }
    let starting = state.last_status == MachineStatus::Starting
        || (state.last_status == MachineStatus::Stopped && state.start_requested_at.is_some());
    if starting {
        Belt::Starting
    } else {
        Belt::Awake
    }
}

/// The HUD toast for an effect.
pub fn toast(effect: Effect) -> &'static str {
    match effect {
        Effect::Stop => TOAST_STOPPED,
        Effect::Back => TOAST_BACK,
    }
}

/// Do the Stop on the orchestrator. `Back` touches nothing.
pub fn apply(state: &mut OrchestratorState, effect: Effect) {
    match effect {
        Effect::Stop => {
            let program = state.program.as_ref().map(|r| r.program.id);
            match (state.last_status, program) {
                (MachineStatus::Stopped, _) => log::info!(
                    "Lights out: the display turned off; sending Stop to the still deck \
                     so it can sleep (it may refuse it)"
                ),
                (_, Some(id)) => log::info!(
                    "Lights out: the display turned off; stopping the belt and ending program {}",
                    id
                ),
                (status, None) => log::info!(
                    "Lights out: the display turned off; stopping the belt ({:?})",
                    status
                ),
            }
            // As END: stops the deck, ends any program, clears every intent.
            // At once, never eased (easy stop is for the STOP press only).
            let now = state.now();
            state.stop_now(now);
            // And no automatic start or resume when the display wakes.
            state.hold_auto_start = true;
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
    fn display_off_stops_an_awake_deck() {
        assert_eq!(off_with(true, Belt::Awake), Some(Effect::Stop));
    }

    #[test]
    fn a_deck_asleep_or_stopping_gets_nothing() {
        assert_eq!(off_with(true, Belt::Asleep), None);
    }

    #[test]
    fn off_by_default_and_does_nothing_when_off() {
        assert!(!Runtime::default().enabled);
        assert!(!enabled_from_raw(None));
        assert!(!enabled_from_raw(Some("junk")));
        assert!(enabled_from_raw(Some("true")));
        for belt in [Belt::Awake, Belt::Starting] {
            assert_eq!(off_with(false, belt), None, "{belt:?}");
        }
    }

    #[test]
    fn offline_does_nothing() {
        let mut m = LightsOut::default();
        let offline = Input {
            connected: false,
            ..input(true, Belt::Awake)
        };
        assert_eq!(m.tick(true, &offline), None);
    }

    #[test]
    fn one_stop_per_display_off_period() {
        let mut m = LightsOut::default();
        assert_eq!(m.tick(true, &input(true, Belt::Awake)), Some(Effect::Stop));
        // A still deck that refused it, or a walk started again at the
        // treadmill with the display still off: nothing is sent again.
        for belt in [Belt::Awake, Belt::Asleep, Belt::Starting, Belt::Awake] {
            for _ in 0..60 {
                assert_eq!(m.tick(true, &input(true, belt)), None, "{belt:?}");
            }
        }
    }

    #[test]
    fn a_walk_started_with_the_display_already_off_is_left_alone() {
        let mut m = LightsOut::default();
        for _ in 0..60 {
            assert_eq!(m.tick(true, &input(true, Belt::Asleep)), None);
        }
        for _ in 0..60 {
            assert_eq!(m.tick(true, &input(true, Belt::Awake)), None);
        }
    }

    #[test]
    fn turned_on_while_the_display_is_off_waits_for_the_next_off() {
        let mut m = LightsOut::default();
        assert_eq!(m.tick(false, &input(true, Belt::Awake)), None);
        assert_eq!(m.tick(true, &input(true, Belt::Awake)), None);
        assert_eq!(m.tick(true, &input(false, Belt::Awake)), None);
        assert_eq!(m.tick(true, &input(true, Belt::Awake)), Some(Effect::Stop));
    }

    #[test]
    fn a_start_in_flight_is_stopped_once_it_runs() {
        let mut m = LightsOut::default();
        for _ in 0..4 {
            assert_eq!(m.tick(true, &input(true, Belt::Starting)), None);
        }
        assert_eq!(m.tick(true, &input(true, Belt::Awake)), Some(Effect::Stop));
    }

    #[test]
    fn the_display_back_on_never_starts_anything() {
        let mut m = LightsOut::default();
        assert!(m.tick(true, &input(true, Belt::Awake)).is_some());
        for _ in 0..600 {
            assert_eq!(m.tick(true, &input(true, Belt::Asleep)), None);
        }
        // Back on: only the notice, once; never a start.
        assert_eq!(m.tick(true, &input(false, Belt::Awake)), Some(Effect::Back));
        for _ in 0..600 {
            assert_eq!(m.tick(true, &input(false, Belt::Awake)), None);
        }
        // Nothing was sent: no notice either.
        let mut m = LightsOut::default();
        m.tick(true, &input(true, Belt::Asleep));
        assert_eq!(m.tick(true, &input(false, Belt::Asleep)), None);
    }

    fn deck(status: MachineStatus) -> OrchestratorState {
        OrchestratorState {
            last_status: status,
            hold_auto_start: false,
            ..OrchestratorState::default()
        }
    }

    #[test]
    fn belt_reads_the_orchestrator() {
        for status in [
            MachineStatus::Running,
            MachineStatus::Paused,
            MachineStatus::Stopped,
        ] {
            assert_eq!(belt(&deck(status)), Belt::Awake, "{status:?}");
        }
        assert_eq!(belt(&deck(MachineStatus::Starting)), Belt::Starting);
        let s = OrchestratorState {
            start_requested_at: Some(std::time::Instant::now()),
            ..deck(MachineStatus::Stopped)
        };
        assert_eq!(belt(&s), Belt::Starting, "a start requested");
        assert_eq!(belt(&deck(MachineStatus::Stopping)), Belt::Asleep);
        let s = OrchestratorState {
            stop_requested: true,
            ..deck(MachineStatus::Running)
        };
        assert_eq!(belt(&s), Belt::Asleep, "a stop in flight");
        let s = OrchestratorState {
            deck_standby: true,
            ..deck(MachineStatus::Stopped)
        };
        assert_eq!(belt(&s), Belt::Asleep, "standby");
    }

    #[test]
    fn the_stop_clears_the_intents_and_holds_automatic_starts() {
        let mut s = deck(MachineStatus::Running);
        apply(&mut s, Effect::Stop);
        assert!(s.stop_requested);
        assert!(s.hold_auto_start);
        assert_eq!(s.manual_intent, None);
        assert_eq!(s.program_intent, None);
        assert!(s.program.is_none());
        // The display back on touches nothing.
        let before = (s.stop_requested, s.hold_auto_start, s.manual_intent);
        apply(&mut s, Effect::Back);
        assert_eq!(
            (s.stop_requested, s.hold_auto_start, s.manual_intent),
            before
        );
    }
}
