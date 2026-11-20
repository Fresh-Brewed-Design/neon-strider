//! DECK_SWEEP (a Labs action, off until switched on): steps a walking belt
//! through every speed byte, or every incline level, once, so DECK_PROFILE is
//! complete without waiting for normal use.
//!
//! It only ever changes the speed or incline of a belt that is already
//! moving, on the user's press, and never starts or resumes one: while it runs
//! the engine makes no automatic start or resume (`control_packet`), and a
//! sweep the belt stopped or paused under puts the speed and incline back and
//! holds automatic starts (`hold_auto_start`) until a press. Each byte or
//! level the profile has no reply for is asked in turn, lowest first; the
//! deck's echo or nack is filed by the usual path (`learn_deck_reply`).
//! Anything else that touches the belt ends it: STOP, PAUSE, a speed or
//! incline set by hand or on the deck's remote, a program, a lost link. When
//! it finishes the belt goes back to the speed or incline it had.
//!
//! Speeds and incline levels are two sweeps, each its own press, one at a
//! time. A walker who takes every speed up to Top speed may not want every
//! incline up to the top level on the same walk, so each is the user's
//! choice; and the incline motor moves slower than the belt motor, so the
//! incline sweep holds each level longer.

use crate::engine::{OrchestratorState, INCLINE_LEVEL_LIMIT};
use crate::protocol::ControlIntent;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// The settings key of the Labs switch.
const KEY_ENABLED: &str = "labs_deck_sweep";

/// Each speed is held at least this long: the engine files a reply that comes
/// within 2 s of the target (`TARGET_REPLY_WINDOW`), and 2 s also lets the
/// belt settle on one speed before the next, so the walker feels even steps.
const DWELL: Duration = Duration::from_secs(2);
/// Each incline level is held at least this long. The reply comes within the
/// same 2 s window as a speed's, but the incline motor lifts the whole deck
/// and is slower than the belt motor, so 4 s (two reply windows) lets the
/// deck reach each level before the next is asked, and the walker climbs one
/// level at a time instead of on one long ramp.
const DWELL_INCLINE: Duration = Duration::from_secs(4);
/// A byte or level the deck has not answered by now is left and the sweep
/// goes on: 8 s is four reply windows, room for a slow deck or a target packet
/// sent late behind other traffic, without leaving the walker on one silent
/// speed or level long.
const GIVE_UP: Duration = Duration::from_secs(8);

/// What a sweep steps through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepKind {
    /// Speed bytes, from the min speed up to the top speed: Top speed, or the
    /// deck's ceiling once seen (the user chose Top speed as the end).
    Speed,
    /// Incline levels, from 1 up to the top level: the Highest incline level
    /// setting, or the deck's own top level once DECK_PROFILE has seen it,
    /// whichever is lower (`top_incline_level`). Flat (0) is never skipped, so
    /// it needs no asking.
    Incline,
}

/// A sweep in progress.
#[derive(Debug, Clone)]
pub struct ProfileSweep {
    kind: SweepKind,
    /// Bytes or levels still to ask, lowest first; the one being asked is
    /// first.
    todo: Vec<u8>,
    total: usize,
    /// The byte or level asked now and when it was set.
    asked: u8,
    asked_at: Instant,
    /// The speed (calibrated km/h) and incline level the belt was set to when
    /// it began, to go back to: the deck's own when its remote set them since
    /// the app's last target, else the manual intent's, else what the deck
    /// reports.
    start_kmh: f32,
    start_incline: u8,
    /// `speed_hand_epoch` after the sweep's own last ask: any other hand
    /// speed moves it on and ends the sweep.
    epoch: u64,
    /// The manual intent's incline after the sweep's own last ask: any other
    /// incline (a press, a typed level) ends the sweep.
    incline: u8,
}

/// What the dashboard shows while it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ProfileSweepView {
    /// Speeds or incline levels asked so far, and in all.
    pub done: u32,
    pub total: u32,
    /// The speed being asked, calibrated km/h; on an incline sweep, the
    /// belt's speed, which that sweep leaves alone.
    pub kmh: f32,
    /// The incline level being asked, on an incline sweep; None on a speed
    /// sweep.
    pub level: Option<u8>,
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
    /// The bytes a speed sweep would ask: from the min speed to the top speed
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

    /// The levels an incline sweep would ask: from 1 to the top level, kept
    /// within what the engine ever sends (`INCLINE_LEVEL_LIMIT`, or a level
    /// above it would go out as another and never be answered), without
    /// those the profile already has a reply for or knows the deck skips.
    fn sweep_levels(&self) -> Vec<u8> {
        let hi = self.top_incline_level().min(INCLINE_LEVEL_LIMIT);
        (1..=hi)
            .filter(|l| !self.deck_profile.knows_level(*l) && !self.deck_profile.skips_level(*l))
            .collect()
    }

    /// DECK_SWEEP over the speeds, the user's press. Refused unless the Labs
    /// switch is on and the belt is moving.
    pub fn profile_sweep_start(&mut self) -> Result<(), String> {
        self.sweep_begin(SweepKind::Speed)
    }

    /// DECK_SWEEP over the incline levels, the user's press. Refused as for
    /// the speeds.
    pub fn profile_sweep_incline_start(&mut self) -> Result<(), String> {
        self.sweep_begin(SweepKind::Incline)
    }

    fn sweep_begin(&mut self, kind: SweepKind) -> Result<(), String> {
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
            return Err(match kind {
                SweepKind::Speed => {
                    "Start the belt first: the sweep only changes the speed of a moving belt."
                }
                SweepKind::Incline => {
                    "Start the belt first: the sweep only changes the incline of a moving belt."
                }
            }
            .into());
        }
        if self.active_program().is_some() {
            return Err("End the program first, then sweep.".into());
        }
        let todo = match kind {
            SweepKind::Speed => self.sweep_bytes(),
            SweepKind::Incline => self.sweep_levels(),
        };
        if todo.is_empty() {
            return Err(match kind {
                SweepKind::Speed => "Every speed is already learned.",
                SweepKind::Incline => "Every incline level is already learned.",
            }
            .into());
        }
        // Start from what the deck runs: a speed or incline its remote set
        // since the app's last target, else the intent, else the deck's own.
        let deck_speed = self.deck_set_speed.take();
        let deck_incline = self.deck_set_incline.take();
        let intent = self.manual_intent;
        let start_kmh = match deck_speed {
            Some(byte) => self.physical_for_byte(byte, 0),
            None => intent
                .map(|i| i.speed)
                .filter(|kmh| *kmh >= 0.05)
                .unwrap_or_else(|| {
                    let byte = (self.hardware_speed * 10.0).round().clamp(1.0, 255.0) as u8;
                    self.physical_for_byte(byte, 0)
                }),
        };
        let start_incline = deck_incline
            .or(intent.map(|i| i.incline))
            .unwrap_or(self.hardware_incline);
        // The sweep's targets carry the other value as the deck runs it, so a
        // speed sweep never moves the incline, nor an incline sweep the speed.
        self.manual_intent = Some(ControlIntent {
            speed: start_kmh,
            incline: start_incline,
        });
        let now = self.now();
        let total = todo.len();
        let mut sweep = ProfileSweep {
            kind,
            asked: todo[0],
            todo,
            total,
            asked_at: now,
            start_kmh,
            start_incline,
            epoch: 0,
            incline: start_incline,
        };
        self.sweep_ask(&mut sweep, now);
        log::info!(
            "Deck sweep over {total} {}",
            match kind {
                SweepKind::Speed => "speeds",
                SweepKind::Incline => "incline levels",
            }
        );
        self.profile_sweep = Some(sweep);
        Ok(())
    }

    /// STOP on the sweep: the speed or incline goes back to the one from
    /// before, since a step the sweep reached is not one the walker chose.
    /// On a belt that stopped or paused before the sweep's tick saw it, that
    /// is done as when the belt is lost (`sweep_belt_lost`): never as a hand
    /// speed, which could resume a paused belt.
    pub fn profile_sweep_stop(&mut self) {
        if let Some(sweep) = self.profile_sweep.take() {
            log::info!("Deck sweep stopped");
            if self.last_status.is_moving() && !self.awaiting_first_packet {
                self.sweep_restore(&sweep);
            } else {
                self.sweep_belt_lost(&sweep);
            }
        }
    }

    /// Set the belt to the sweep's current byte or level.
    fn sweep_ask(&mut self, sweep: &mut ProfileSweep, now: Instant) {
        match sweep.kind {
            SweepKind::Speed => {
                let kmh = self.physical_for_byte(sweep.asked, 0);
                crate::bluetooth::apply_manual_intent(self, Some(kmh), None);
            }
            SweepKind::Incline => {
                crate::bluetooth::apply_manual_intent(self, None, Some(sweep.asked));
            }
        }
        // Sent even when its bytes match the last ones: after the deck took
        // another byte or level for the one before (asked 4, ran 5), the
        // engine reads 5 as already sent and would never ask it.
        self.resend_target = true;
        sweep.asked_at = now;
        sweep.epoch = self.speed_hand_epoch;
        sweep.incline = self.manual_intent.map_or(0, |i| i.incline);
    }

    /// Put back what the sweep changed, on a moving belt: the speed after a
    /// speed sweep, the incline level after an incline sweep.
    fn sweep_restore(&mut self, sweep: &ProfileSweep) {
        match sweep.kind {
            SweepKind::Speed => {
                let kmh = self.snap_speed(sweep.start_kmh);
                crate::bluetooth::apply_manual_intent(self, Some(kmh), None);
            }
            SweepKind::Incline => {
                let level = self.snap_incline_level(sweep.start_incline);
                crate::bluetooth::apply_manual_intent(self, None, Some(level));
            }
        }
    }

    /// The belt stopped or paused under the sweep, or the link is new: put the
    /// speed and incline back to those from before the sweep, without counting
    /// either as set by hand, and hold automatic starts until a press, so
    /// neither they nor the last sweep step can start or resume the belt.
    fn sweep_belt_lost(&mut self, sweep: &ProfileSweep) {
        self.hold_auto_start = true;
        let kmh = self.snap_speed(sweep.start_kmh);
        let incline = self.snap_incline_level(sweep.start_incline);
        if let Some(intent) = self.manual_intent.as_mut() {
            *intent = ControlIntent {
                speed: kmh,
                incline,
            };
        }
    }

    /// Once a second: move to the next byte or level when the deck has
    /// answered this one, and end the sweep when anything else took the belt.
    pub fn tick_profile_sweep(&mut self, now: Instant) {
        let Some(mut sweep) = self.profile_sweep.take() else {
            return;
        };
        if !self.last_status.is_moving() || self.awaiting_first_packet {
            log::info!("Deck sweep ended: the belt stopped or the link dropped");
            self.sweep_belt_lost(&sweep);
            return;
        }
        // A hand speed or incline, a program or the deck's own remote took
        // the belt: their speed and incline stand.
        if self.active_program().is_some()
            || self.speed_hand_epoch != sweep.epoch
            || self.manual_intent.map(|i| i.incline) != Some(sweep.incline)
            || self.deck_set_speed.is_some()
            || self.deck_set_incline.is_some()
        {
            log::info!("Deck sweep ended: the belt was taken over");
            return;
        }
        let held = now.saturating_duration_since(sweep.asked_at);
        let (answered, dwell) = match sweep.kind {
            SweepKind::Speed => (self.deck_profile.knows(sweep.asked), DWELL),
            SweepKind::Incline => (self.deck_profile.knows_level(sweep.asked), DWELL_INCLINE),
        };
        if held < dwell || (!answered && held < GIVE_UP) {
            self.profile_sweep = Some(sweep);
            return;
        }
        sweep.todo.remove(0);
        // A reply may have filed bytes or levels further on (a floor or
        // ceiling).
        let p = &self.deck_profile;
        match sweep.kind {
            SweepKind::Speed => sweep.todo.retain(|b| !p.knows(*b) && !p.skips(*b)),
            SweepKind::Incline => sweep
                .todo
                .retain(|l| !p.knows_level(*l) && !p.skips_level(*l)),
        }
        match sweep.todo.first() {
            Some(&next) => {
                sweep.asked = next;
                self.sweep_ask(&mut sweep, now);
                self.profile_sweep = Some(sweep);
            }
            None => {
                self.sweep_restore(&sweep);
                log::info!("Deck sweep done");
            }
        }
    }

    /// Tests: the current byte or level has been held past `GIVE_UP`, so the
    /// next tick steps on.
    #[cfg(test)]
    pub(crate) fn profile_sweep_step_due_for_test(&mut self) {
        if let Some(s) = self.profile_sweep.as_mut() {
            s.asked_at = s.asked_at.checked_sub(GIVE_UP).unwrap_or(s.asked_at);
        }
    }

    /// Tests: which kind of sweep runs, if one does.
    #[cfg(test)]
    pub(crate) fn profile_sweep_kind(&self) -> Option<SweepKind> {
        self.profile_sweep.as_ref().map(|s| s.kind)
    }

    /// The running sweep, for the snapshot.
    pub fn profile_sweep_view(&self) -> Option<ProfileSweepView> {
        let s = self.profile_sweep.as_ref()?;
        let (kmh, level) = match s.kind {
            SweepKind::Speed => (self.to_physical_speed(s.asked as f32 / 10.0), None),
            SweepKind::Incline => (s.start_kmh, Some(s.asked)),
        };
        Some(ProfileSweepView {
            done: (s.total - s.todo.len()) as u32,
            total: s.total as u32,
            kmh,
            level,
        })
    }
}
