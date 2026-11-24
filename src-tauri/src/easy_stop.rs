//! Easy stop (System -> Treadmill, off by default): STOP on a manual walk (the
//! end-walk control: DUMP on the HUD and dashboard, the tray's Stop, the stop
//! shortcut) first eases the belt down to the deck's slowest speed over
//! `RAMP`, then stops it, instead of the deck winding down from the walking
//! speed at once. The companion to easy start (#259); the ramp maths is
//! easy start's (`step_fraction`, `stepped_byte`, its `STEP`).
//!
//! It only ever lowers the speed sent and then sends the Stop, so it never
//! starts or resumes the belt. Everything else stays immediate: Pause (and
//! walk-away, which pauses), Lights out (`stop_now`), a stop or pause at the
//! treadmill, a program's own stop, a lost link. During the ramp:
//! - a second STOP stops at once;
//! - a speed set by hand (`speed_hand_epoch`) ends the ramp and the walk goes
//!   on at that speed;
//! - a pause goes out as always; the ramp holds its Stop while the pause is in
//!   flight and ends once the deck reports the belt paused, so the walk waits
//!   for GO or another STOP;
//! - a program run, DECK_SWEEP or another stop ends it;
//! - switching easy stop off stops the belt at once (STOP was pressed);
//! - a lost link ends it and clears the intents as a STOP does
//!   (`easy_stop_link_lost`), so a reconnect never speeds the belt back up.
//!
//! Only the bytes sent change: the intents, the session and its stats carry on
//! until the deck reports the belt stopped, so the saved walk includes the
//! ramp. Speed drift holds still while the belt eases down
//! (`tick_speed_drift`), so a drift step never fights the ramp, and the sent
//! byte is never above the target, so Neural flow or a call's quiet speed can
//! only take it lower. `last_physical_speed` keeps the target, as with easy
//! start, so a pause at the treadmill mid-ramp is never resumed by the engine.

use crate::easy_start::{step_fraction, stepped_byte, STEP};
use crate::engine::OrchestratorState;
use crate::protocol::HandshakeStep;
use crate::protocol::MachineStatus;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// The settings key of the switch.
const KEY_ENABLED: &str = "easy_stop";

/// The whole ease from the walking speed down to the slowest speed. The deck
/// winds down about 0.5 km/h a second by itself (`sim.rs`, from the recorded
/// wind-down), so 5 km/h to still takes it about 10 s. 20 s eases a 5 km/h
/// walk down at under half that pace, a dozen or so slowing steps to step off
/// at a desk, and stays short enough that STOP still feels like a stop.
pub(crate) const RAMP: Duration = Duration::from_secs(20);

const _: () = assert!(STEP.as_secs() < RAMP.as_secs());

/// The ramp under way.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ramp {
    started: Instant,
    /// The byte the deck ran when STOP was pressed.
    from: u8,
    /// The slowest speed's byte (`floor_hw_speed`), where the ramp ends.
    to: u8,
    /// `speed_hand_epoch` at the press: any later speed set by hand ends it.
    hand_epoch: u64,
}

/// Easy stop's state on the engine.
#[derive(Debug, Clone, Default)]
pub struct EasyStop {
    ramp: Option<Ramp>,
}

/// The ramp's byte `elapsed` into it, from `from` down to `to`, one `STEP`
/// at a time, the first on the press itself, so the belt slows at once and
/// sits at `to` for the last step before the Stop. None once it is over. Pure.
pub(crate) fn ramp_down_byte(from: u8, to: u8, elapsed: Duration) -> Option<u8> {
    if elapsed >= RAMP {
        return None;
    }
    if from <= to {
        return Some(to);
    }
    // The last step lands on `to`: (RAMP / STEP) steps, the first at 0 s.
    let frac = step_fraction(elapsed, RAMP, 1);
    Some(stepped_byte(from, to, frac))
}

/// The switch and the ramp as the windows see them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct EasyStopSnapshot {
    /// STOP on a manual walk eases the belt down first.
    pub enabled: bool,
    /// The ramp's length, for the setting's description.
    pub ramp_s: u32,
    /// The belt is easing down to stop right now.
    pub easing: bool,
    /// Seconds until the Stop goes out, while easing.
    pub stop_in_s: Option<u32>,
    /// The slowest speed the belt eases down to (calibrated km/h), while easing.
    pub to_kmh: Option<f32>,
    /// Rust's sentence for the HUD line, while easing.
    pub message: Option<String>,
}

/// Load the saved switch at startup (off when never set).
pub async fn init(app: &tauri::AppHandle) {
    let app_c = app.clone();
    let on = crate::util::blocking(move || {
        Ok::<_, String>(crate::persistence::parse_setting::<bool>(
            crate::persistence::read_setting(&app_c, KEY_ENABLED).as_deref(),
        ))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(false);
    crate::bluetooth::ORCHESTRATOR
        .lock()
        .await
        .easy_stop_enabled = on;
}

/// The switch (System -> Treadmill), saved. Off during a ramp stops the belt
/// at once (the next tick, `track_easy_stop`): STOP was already pressed.
pub async fn set_enabled(app: &tauri::AppHandle, on: bool) -> Result<(), String> {
    let app_c = app.clone();
    crate::util::blocking(move || {
        crate::persistence::write_setting(&app_c, KEY_ENABLED, if on { "true" } else { "false" })
    })
    .await?;
    crate::bluetooth::ORCHESTRATOR
        .lock()
        .await
        .easy_stop_enabled = on;
    log::info!("Easy stop {}", if on { "on" } else { "off" });
    Ok(())
}

impl OrchestratorState {
    /// STOP pressed (`DeckAction::Stop`): a second press during the ramp
    /// stops at once; with easy stop on, a manual walk running above the
    /// slowest speed eases down first; anything else stops at once.
    pub(crate) fn stop_pressed(&mut self, now: Instant) {
        if self.easy_stop.ramp.is_some() {
            log::info!("Easy stop: STOP pressed again, stopping now");
            self.stop_now(now);
            return;
        }
        let floor = self.floor_byte();
        let running = (self.hardware_speed * 10.0).round().clamp(0.0, 255.0) as u8;
        let may_ease = self.easy_stop_enabled
            && self.last_status == MachineStatus::Running
            && !self.awaiting_first_packet
            && !self.stop_requested
            && self.program.is_none()
            && self.profile_sweep.is_none()
            && self.pending_control.is_none()
            && self.handshake_step == HandshakeStep::Idle
            && running > floor;
        if !may_ease {
            self.stop_now(now);
            return;
        }
        // The belt eases down now, not up: any easy start ramp is over.
        self.end_easy_start(now, Some("easy stop"));
        self.easy_stop.ramp = Some(Ramp {
            started: now,
            from: running,
            to: floor,
            hand_epoch: self.speed_hand_epoch,
        });
        self.last_manual_interaction = now;
        log::info!(
            "Easy stop: easing down from byte {running} to {floor} over {} s, then stopping",
            RAMP.as_secs()
        );
    }

    /// Stop the belt at once, ending any ramp (a second STOP, Lights out, the
    /// end of the ramp).
    pub(crate) fn stop_now(&mut self, now: Instant) {
        self.easy_stop.ramp = None;
        self.do_stop(now);
    }

    fn floor_byte(&self) -> u8 {
        (self.floor_hw_speed() * 10.0).round().clamp(0.0, 255.0) as u8
    }

    /// Once per engine tick: end the ramp when something else takes the belt,
    /// and send the Stop when it is over.
    pub(crate) fn track_easy_stop(&mut self, now: Instant) {
        let Some(ramp) = self.easy_stop.ramp else {
            return;
        };
        let ended = if !self.easy_stop_enabled {
            log::info!("Easy stop switched off mid-ramp: stopping now");
            self.stop_now(now);
            return;
        } else if self.stop_requested {
            Some("another stop")
        } else if self.awaiting_first_packet {
            Some("the link dropped")
        } else if !matches!(self.last_status, MachineStatus::Running) {
            Some("the belt paused or stopped")
        } else if self.speed_hand_epoch != ramp.hand_epoch {
            Some("a speed set by hand; the walk goes on")
        } else if self.program.is_some() {
            Some("a program started")
        } else if self.profile_sweep.is_some() {
            Some("DECK_SWEEP")
        } else {
            None
        };
        if let Some(why) = ended {
            self.easy_stop.ramp = None;
            log::info!("Easy stop ended: {why}");
            return;
        }
        // A pause in flight goes first: the Stop waits until the deck has
        // reported it (then the ramp ends above) or it went nowhere.
        let pause_in_flight =
            self.pending_control.is_some() || self.handshake_step != HandshakeStep::Idle;
        if now.saturating_duration_since(ramp.started) >= RAMP && !pause_in_flight {
            log::info!("Easy stop: at the slowest speed, stopping");
            self.stop_now(now);
        }
    }

    /// The link went down mid-ramp (`reset_connection`): STOP was pressed, so
    /// the intents clear as a STOP clears them, and a reconnect to a belt
    /// still moving adopts its speed instead of sending the walk's speed again.
    pub(crate) fn easy_stop_link_lost(&mut self) {
        if self.easy_stop.ramp.take().is_some() {
            self.manual_intent = None;
            self.program_intent = None;
            self.engage_speed = None;
            log::info!(
                "Easy stop ended: the link dropped; the intents are cleared as a STOP clears them"
            );
        }
    }

    /// The speed byte to send for the target's `byte` while the belt eases
    /// down: the ramp's, at a speed the deck runs, never above `byte`. `byte`
    /// itself with no ramp under way.
    pub(crate) fn easy_stop_byte(&self, byte: u8, now: Instant) -> u8 {
        let Some(ramp) = self.easy_stop.ramp else {
            return byte;
        };
        if self.speed_hand_epoch != ramp.hand_epoch {
            return byte;
        }
        let elapsed = now.saturating_duration_since(ramp.started);
        let mut eased = ramp_down_byte(ramp.from, ramp.to, elapsed).unwrap_or(ramp.to);
        // Down past a speed the deck skips, so each step is one it runs.
        while eased > ramp.to && self.deck_profile.skips(eased) {
            eased -= 1;
        }
        eased.min(byte)
    }

    /// The belt is easing down to stop.
    pub(crate) fn easing_down(&self) -> bool {
        self.easy_stop.ramp.is_some()
    }

    pub(crate) fn easy_stop_snapshot(&self) -> EasyStopSnapshot {
        let mut snap = EasyStopSnapshot {
            enabled: self.easy_stop_enabled,
            ramp_s: RAMP.as_secs() as u32,
            ..Default::default()
        };
        if let Some(ramp) = self.easy_stop.ramp {
            let left = RAMP.saturating_sub(self.now().saturating_duration_since(ramp.started));
            let to_kmh = self.physical_for_byte(ramp.to, 0);
            snap.easing = true;
            snap.stop_in_s = Some(left.as_secs_f32().ceil() as u32);
            snap.to_kmh = Some(to_kmh);
            snap.message = Some(format!(
                "Slowing to {to_kmh:.1} km/h, then stopping."
            ));
        }
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ramp_steps_down_from_the_press_to_the_floor() {
        let at = |s: u64| ramp_down_byte(50, 6, Duration::from_secs(s));
        assert!(at(0).unwrap() < 50, "slower on the press: {:?}", at(0));
        assert_eq!(at(1), at(0), "one step every 2 s");
        assert_eq!(at(18), Some(6), "at the floor for the last step");
        assert_eq!(at(19), Some(6));
        assert_eq!(at(20), None, "over at 20 s");
        let bytes: Vec<u8> = (0..20).map(|s| at(s).unwrap()).collect();
        assert!(bytes.windows(2).all(|w| w[0] >= w[1]), "{bytes:?}");
        // Ten steps of at most 4.4 deck units each: none a jolt.
        assert!(bytes.windows(2).all(|w| w[0] - w[1] <= 5), "{bytes:?}");
    }

    #[test]
    fn a_belt_at_or_under_the_floor_is_sent_the_floor() {
        assert_eq!(ramp_down_byte(6, 6, Duration::from_secs(4)), Some(6));
        assert_eq!(ramp_down_byte(5, 6, Duration::ZERO), Some(6));
    }

    #[test]
    fn the_ramp_is_whole_steps() {
        assert_eq!(RAMP.as_millis() % STEP.as_millis(), 0);
    }

    fn running_state() -> OrchestratorState {
        let mut s = OrchestratorState {
            easy_stop_enabled: true,
            ..Default::default()
        };
        s.set_test_now(Instant::now());
        s.awaiting_first_packet = false;
        s.last_status = MachineStatus::Running;
        s.hardware_speed = 4.0;
        s
    }

    #[test]
    fn stop_eases_a_running_manual_walk_and_a_second_press_stops() {
        let mut s = running_state();
        let now = s.now();
        s.stop_pressed(now);
        assert!(s.easing_down());
        assert!(!s.stop_requested, "no Stop yet");
        assert!(s.easy_stop_byte(40, now) < 40);
        s.stop_pressed(now);
        assert!(!s.easing_down());
        assert!(s.stop_requested, "the second press stops");
    }

    #[test]
    fn stop_is_immediate_when_off_paused_or_at_the_floor() {
        let mut off = running_state();
        off.easy_stop_enabled = false;
        let now = off.now();
        off.stop_pressed(now);
        assert!(off.stop_requested && !off.easing_down());

        let mut paused = running_state();
        paused.last_status = MachineStatus::Paused;
        paused.stop_pressed(now);
        assert!(paused.stop_requested && !paused.easing_down());

        let mut slow = running_state();
        slow.hardware_speed = slow.floor_hw_speed();
        slow.stop_pressed(now);
        assert!(slow.stop_requested && !slow.easing_down());
    }

    #[test]
    fn the_snapshot_counts_down_while_easing() {
        let mut s = running_state();
        assert!(!s.easy_stop_snapshot().easing);
        let now = s.now();
        s.stop_pressed(now);
        let snap = s.easy_stop_snapshot();
        assert!(snap.easing);
        assert_eq!(snap.stop_in_s, Some(RAMP.as_secs() as u32));
        assert!(snap.message.is_some_and(|m| m.ends_with("then stopping.")));
    }
}
