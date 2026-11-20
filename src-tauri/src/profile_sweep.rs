//! DECK_SWEEP (a Labs action, off until switched on): steps a walking belt
//! through every speed byte once, so DECK_PROFILE is complete without waiting
//! for normal use.
//!
//! It only ever changes the speed of a belt that is already moving, on the
//! user's press, and never starts or resumes one: while it runs the engine
//! makes no automatic start or resume (`control_packet`), and a sweep the belt
//! stopped or paused under puts the speed back and holds automatic starts
//! (`hold_auto_start`) until a press. Each byte the profile has no reply for is
//! asked in turn, slowest first; the deck's echo or nack is filed by the usual
//! path (`learn_deck_reply`). Anything else that touches the belt ends it:
//! STOP, PAUSE, a speed set by hand or on the deck's remote, a program, a lost
//! link. When it finishes the belt goes back to the speed it had.

use crate::engine::OrchestratorState;
use crate::protocol::ControlIntent;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// The settings key of the Labs switch.
const KEY_ENABLED: &str = "labs_deck_sweep";

/// Each speed is held at least this long: the engine files a reply that comes
/// within 2 s of the target (`TARGET_REPLY_WINDOW`), and 2 s also lets the
/// belt settle on one speed before the next, so the walker feels even steps.
const DWELL: Duration = Duration::from_secs(2);
/// A byte the deck has not answered by now is left and the sweep goes on: 8 s
/// is four reply windows, room for a slow deck or a target packet sent late
/// behind other traffic, without leaving the walker on one silent speed long.
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
    /// The speed (calibrated km/h) the belt was set to when it began, to go
    /// back to: the manual intent's, or the deck's own when there was none.
    start_kmh: f32,
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

/// Read the Labs switch into the engine. Call once after the database is
/// initialised.
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
        .profile_sweep_enabled = on;
}

/// The Labs switch (System -> Diagnostics), saved. Off also ends a sweep.
pub async fn set_enabled(app: &tauri::AppHandle, on: bool) -> Result<(), String> {
    let app_c = app.clone();
    crate::util::blocking(move || {
        crate::persistence::write_setting(&app_c, KEY_ENABLED, if on { "true" } else { "false" })
    })
    .await?;
    let mut state = crate::bluetooth::ORCHESTRATOR.lock().await;
    state.profile_sweep_enabled = on;
    if !on {
        state.profile_sweep_stop();
    }
    log::info!("Deck sweep {}", if on { "on" } else { "off" });
    Ok(())
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

    /// DECK_SWEEP, the user's press. Refused unless the Labs switch is on and
    /// the belt is moving.
    pub fn profile_sweep_start(&mut self) -> Result<(), String> {
        if !self.profile_sweep_enabled {
            return Err("Switch on LABS // DECK_SWEEP in System, Diagnostics first.".into());
        }
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
        let start_kmh = self
            .manual_intent
            .map(|i| i.speed)
            .filter(|kmh| *kmh >= 0.05)
            .unwrap_or_else(|| {
                let byte = (self.hardware_speed * 10.0).round().clamp(1.0, 255.0) as u8;
                self.physical_for_byte(byte, 0)
            });
        let now = self.now();
        let total = todo.len();
        let mut sweep = ProfileSweep {
            asked: todo[0],
            todo,
            total,
            asked_at: now,
            start_kmh,
            epoch: 0,
        };
        // A speed the deck set by itself before the press is not a takeover.
        self.deck_set_speed = None;
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

    /// The belt stopped or paused under the sweep, or the link is new: put the
    /// speed back to the one from before the sweep, without counting it as a
    /// hand speed, and hold automatic starts until a press, so neither that
    /// speed nor the last sweep step can start or resume the belt.
    fn sweep_belt_lost(&mut self, sweep: &ProfileSweep) {
        self.hold_auto_start = true;
        let kmh = self.snap_speed(sweep.start_kmh);
        if let Some(intent) = self.manual_intent.as_mut() {
            *intent = ControlIntent {
                speed: kmh,
                incline: intent.incline,
            };
        }
    }

    /// Once a second: move to the next byte when the deck has answered this
    /// one, and end the sweep when anything else took the belt.
    pub fn tick_profile_sweep(&mut self, now: Instant) {
        let Some(mut sweep) = self.profile_sweep.take() else {
            return;
        };
        if !self.last_status.is_moving() || self.awaiting_first_packet {
            log::info!("Deck sweep ended: the belt stopped or the link dropped");
            self.sweep_belt_lost(&sweep);
            return;
        }
        // A hand speed, a program or the deck's own remote took the belt:
        // their speed stands.
        if self.active_program().is_some()
            || self.speed_hand_epoch != sweep.epoch
            || self.deck_set_speed.is_some()
        {
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
                let kmh = self.snap_speed(sweep.start_kmh);
                crate::bluetooth::apply_manual_intent(self, Some(kmh), None);
                log::info!("Deck sweep done");
            }
        }
    }

    /// Tests: the current byte has been held past `GIVE_UP`, so the next tick
    /// steps on.
    #[cfg(test)]
    pub(crate) fn profile_sweep_step_due_for_test(&mut self) {
        if let Some(s) = self.profile_sweep.as_mut() {
            s.asked_at = s.asked_at.checked_sub(GIVE_UP).unwrap_or(s.asked_at);
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
