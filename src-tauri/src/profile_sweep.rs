//! DECK_SWEEP (a Labs action): steps a walking belt through every speed byte
//! once, so DECK_PROFILE is complete without waiting for normal use.
//!
//! It only ever changes the speed of a belt that is already moving, on the
//! user's press, and never starts or resumes one. Each byte the profile has no
//! reply for is asked in turn, slowest first; the deck's echo or nack is filed
//! by the usual path (`learn_deck_reply`). Anything else that touches the belt
//! ends it: STOP, PAUSE, a speed set by hand, a program, a lost link. When it
//! finishes the belt goes back to the speed it had.

use crate::engine::OrchestratorState;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Each speed is held at least this long, so the deck has settled on it.
const DWELL: Duration = Duration::from_secs(2);
/// A byte the deck has not answered by now is left and the sweep goes on.
const GIVE_UP: Duration = Duration::from_secs(8);

/// A sweep in progress.
#[derive(Debug, Clone)]
pub struct ProfileSweep {
    /// Bytes still to ask, slowest first; the one being asked is first.
    todo: Vec<u8>,
    total: usize,
    /// The byte asked now and when it was set.
    asked: u8,
    asked_at: Instant,
    /// The deck byte the belt ran at when it began, to go back to.
    start_byte: u8,
    /// `speed_hand_epoch` after the sweep's own last speed: any other hand
    /// speed moves it on and ends the sweep.
    epoch: u64,
}

/// What the dashboard shows while it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ProfileSweepView {
    /// Speeds asked so far, and in all.
    pub done: u32,
    pub total: u32,
    /// The speed being asked, calibrated km/h.
    pub kmh: f32,
}

impl OrchestratorState {
    /// The bytes a sweep would ask: from the min speed to the top speed
    /// setting, without those the profile already has a reply for, those
    /// below the deck's floor or above its ceiling, and those the calibration
    /// would send as another byte.
    fn sweep_bytes(&self) -> Vec<u8> {
        let lo = ((self.min_speed * 10.0).round() as u8).max(1);
        let hi = (self.top_hw_speed() * 10.0).round().clamp(0.0, 255.0) as u8;
        (lo..=hi)
            .filter(|b| !self.deck_profile.knows(*b) && !self.deck_profile.skips(*b))
            .filter(|b| self.speed_byte(self.physical_for_byte(*b, 0)) == *b)
            .collect()
    }

    /// DECK_SWEEP, the user's press. Refused unless the belt is moving.
    pub fn profile_sweep_start(&mut self) -> Result<(), String> {
        if !self.profile_learning {
            return Err("The simulated deck has no profile of its own to learn.".into());
        }
        if self.profile_sweep.is_some() {
            return Err("The sweep is already running.".into());
        }
        if !self.last_status.is_moving() || self.hardware_speed < 0.05 {
            return Err(
                "Start the belt first: the sweep only changes the speed of a moving belt.".into(),
            );
        }
        if self.active_program().is_some() {
            return Err("End the program first, then sweep.".into());
        }
        let todo = self.sweep_bytes();
        if todo.is_empty() {
            return Err("Every speed is already learned.".into());
        }
        let start_byte = (self.hardware_speed * 10.0).round().clamp(1.0, 255.0) as u8;
        let now = self.now();
        let total = todo.len();
        let mut sweep = ProfileSweep {
            asked: todo[0],
            todo,
            total,
            asked_at: now,
            start_byte,
            epoch: 0,
        };
        self.sweep_ask(&mut sweep, now);
        log::info!("Deck sweep over {total} speeds");
        self.profile_sweep = Some(sweep);
        Ok(())
    }

    /// STOP on the sweep. The belt keeps going at the speed it has.
    pub fn profile_sweep_stop(&mut self) {
        if self.profile_sweep.take().is_some() {
            log::info!("Deck sweep stopped");
        }
    }

    /// Set the belt to the sweep's current byte.
    fn sweep_ask(&mut self, sweep: &mut ProfileSweep, now: Instant) {
        let kmh = self.physical_for_byte(sweep.asked, 0);
        crate::bluetooth::apply_manual_intent(self, Some(kmh), None);
        sweep.asked_at = now;
        sweep.epoch = self.speed_hand_epoch;
    }

    /// Once a second: move to the next byte when the deck has answered this
    /// one, and end the sweep when anything else took the belt.
    pub fn tick_profile_sweep(&mut self, now: Instant) {
        let Some(mut sweep) = self.profile_sweep.take() else {
            return;
        };
        let interrupted = !self.last_status.is_moving()
            || self.awaiting_first_packet
            || self.active_program().is_some()
            || self.speed_hand_epoch != sweep.epoch;
        if interrupted {
            log::info!("Deck sweep ended: the belt was taken over");
            return;
        }
        let held = now.saturating_duration_since(sweep.asked_at);
        let answered = self.deck_profile.knows(sweep.asked);
        if held < DWELL || (!answered && held < GIVE_UP) {
            self.profile_sweep = Some(sweep);
            return;
        }
        sweep.todo.remove(0);
        // A reply may have filed bytes further on (a floor or ceiling).
        sweep
            .todo
            .retain(|b| !self.deck_profile.knows(*b) && !self.deck_profile.skips(*b));
        match sweep.todo.first() {
            Some(&next) => {
                sweep.asked = next;
                self.sweep_ask(&mut sweep, now);
                self.profile_sweep = Some(sweep);
            }
            None => {
                let kmh = self.physical_for_byte(sweep.start_byte, 0);
                crate::bluetooth::apply_manual_intent(self, Some(self.snap_speed(kmh)), None);
                log::info!("Deck sweep done");
            }
        }
    }

    /// The running sweep, for the snapshot.
    pub fn profile_sweep_view(&self) -> Option<ProfileSweepView> {
        let s = self.profile_sweep.as_ref()?;
        Some(ProfileSweepView {
            done: (s.total - s.todo.len()) as u32,
            total: s.total as u32,
            kmh: self.to_physical_speed(s.asked as f32 / 10.0),
        })
    }
}
