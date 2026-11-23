//! Easy start (System -> Treadmill, on by default): every belt start begins at
//! the deck's slowest speed and eases up to the target over `RAMP`, instead of
//! the deck running straight up to it.
//!
//! It only ever holds the speed sent below the target, on a belt that is
//! already running after a start: GO or Engage on a manual walk, a program
//! start, a resume after a pause, GO on a program step after a rest or a
//! 0 km/h step, a speed set by hand on a still belt, and a start at the
//! treadmill when the app then sets a target (GO on a waiting program, say).
//! It never starts, resumes or pauses the belt, so belt safety is as before:
//! the start itself is still the user's press. A pause or stop during the ramp
//! goes out at once as always (the engine's pause and stop paths come before
//! the target), and the ramp ends there. A speed set by hand ends it at once
//! (`speed_hand_epoch`), and so does DECK_SWEEP. A belt the app finds already
//! moving (a restart, a reconnect, an update) carries on untouched: a ramp
//! needs the belt seen still on this connection first.
//!
//! Calibration runs are never eased: GROUND_TRUTH's RUN, RUN ALL and the
//! guided walk set their speed through `calibration::begin_run`, which calls
//! `measure_at_set_speed`. The belt then starts at the run's speed at once,
//! and taps (belt passes or GAIT_SYNC footsteps) never time a belt still
//! easing up, which would save a wrong point. The hold lasts until a speed is
//! set some other way, a program runs or the belt is stopped, so a pause and
//! Engage mid-run resumes at the run's speed too.
//!
//! Each speed is one the deck runs: a byte DECK_PROFILE knows the deck skips
//! is stepped down past, and the usual snap in `control_packet` follows.
//!
//! Program steps: the step's timer and its heart rate zone settling count from
//! the start as before, so the ramp's seconds are part of the step (a 5 minute
//! step still lasts 5 minutes, about 15 s of it below the step's speed on
//! average). `RAMP` is well under `programs::ZONE_SETTLE`, so a zone never
//! trims while the belt is still easing up, and its pulse has the rest of the
//! settling time to follow the full speed.

use crate::engine::OrchestratorState;
use crate::protocol::MachineStatus;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// The settings key of the switch.
const KEY_ENABLED: &str = "easy_start";

/// The whole ease from the slowest speed to the target. About 30 s, as asked
/// (#259): the deck alone climbs about 0.5 km/h a second (`sim.rs`, from the
/// recorded wind-down), so 0.6 to 5 km/h takes it under 10 s; 30 s is a third
/// of that pace, the first dozen steps at a slow walk, and short against any
/// program step (whole minutes) and the 2 minute zone settling.
pub(crate) const RAMP: Duration = Duration::from_secs(30);
/// One new speed at most this often: the deck takes a second or two to reach
/// a target (`engine::DECK_SETTLED`'s note), so each step lands before the
/// next, and 15 targets over the ramp keep the link free for polls.
pub(crate) const STEP: Duration = Duration::from_secs(2);
/// Speed toasts stay quiet this long after the ramp ends, while the deck
/// reaches the last step (one `STEP` and a second for the frames to show it).
const QUIET_AFTER: Duration = Duration::from_secs(3);

const _: () = assert!(RAMP.as_secs() < crate::programs::ZONE_SETTLE.as_secs());

/// Easy start's state on the engine: whether the belt was seen still on this
/// connection, whether that start was the app's own, and the ramp under way.
#[derive(Debug, Clone, Default)]
pub struct EasyStart {
    /// The belt was seen still (stopped or paused) since the last ramp, on
    /// this connection: the next run is a start, not a belt carried on.
    seen_still: bool,
    /// The belt is starting (countdown or running) and the ramp's start is
    /// decided: `Some(true)` when the app sent the Start.
    app_start: Option<bool>,
    ramp: Option<Ramp>,
    /// When the last ramp ended, for the quiet toasts (`QUIET_AFTER`).
    ended_at: Option<Instant>,
    /// A calibration run set the speed: `speed_hand_epoch` then. No ramp
    /// begins while the epoch is unchanged (`measure_at_set_speed`).
    measuring: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct Ramp {
    started: Instant,
    /// The speed byte the ramp starts from: the slowest speed, or what a
    /// start at the treadmill already runs when that is faster.
    from: u8,
    /// `speed_hand_epoch` when the ramp began: any later speed set by hand
    /// ends it.
    hand_epoch: u64,
    /// A pause or stop was asked at this point: the ramp sends nothing new
    /// while the deck has not yet reported it (its frames still say running).
    frozen_at: Option<Instant>,
}

/// The ramp's byte `elapsed` into it, from `from` toward `to`, in `STEP`s;
/// None once the ramp is over. Pure.
pub(crate) fn ramp_byte(from: u8, to: u8, elapsed: Duration) -> Option<u8> {
    if elapsed >= RAMP {
        return None;
    }
    if to <= from {
        return Some(to);
    }
    let steps = (elapsed.as_millis() / STEP.as_millis()) as u64;
    let frac = (steps * STEP.as_millis() as u64) as f64 / RAMP.as_millis() as f64;
    let byte = from as f64 + (to - from) as f64 * frac.min(1.0);
    Some((byte.round() as u8).clamp(from, to))
}

/// The switch as the windows see it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct EasyStartSnapshot {
    /// Every belt start eases up from the slowest speed.
    pub enabled: bool,
}

/// Load the saved switch at startup (on when never set).
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
    .unwrap_or(true);
    crate::bluetooth::ORCHESTRATOR
        .lock()
        .await
        .easy_start_enabled = on;
}

/// The switch (System -> Treadmill), saved. Off ends a ramp under way: the
/// belt goes on up to its target as the deck does.
pub async fn set_enabled(app: &tauri::AppHandle, on: bool) -> Result<(), String> {
    let app_c = app.clone();
    crate::util::blocking(move || {
        crate::persistence::write_setting(&app_c, KEY_ENABLED, if on { "true" } else { "false" })
    })
    .await?;
    crate::bluetooth::ORCHESTRATOR
        .lock()
        .await
        .easy_start_enabled = on;
    log::info!("Easy start {}", if on { "on" } else { "off" });
    Ok(())
}

impl OrchestratorState {
    /// Once per engine tick, before the program's own start watch: notice a
    /// start, begin the ramp when the belt runs, end it when it is over.
    pub(crate) fn track_easy_start(&mut self, now: Instant) {
        if !self.easy_start_enabled || self.awaiting_first_packet {
            // A new connection starts over: a belt found moving is carried on.
            self.end_easy_start(now, None);
            self.easy_start.seen_still = false;
            self.easy_start.app_start = None;
            return;
        }
        if self.stop_requested || self.profile_sweep.is_some() {
            self.end_easy_start(now, Some("a stop or DECK_SWEEP"));
        }
        // A calibration run's hold ends with a speed set another way, a
        // program or a stop.
        if self.easy_start.measuring.is_some_and(|epoch| {
            epoch != self.speed_hand_epoch || self.stop_requested || self.program.is_some()
        }) {
            self.easy_start.measuring = None;
        }
        match self.last_status {
            MachineStatus::Stopped | MachineStatus::Paused => {
                self.end_easy_start(now, Some("the belt paused or stopped"));
                self.easy_start.seen_still = true;
                self.easy_start.app_start = None;
            }
            MachineStatus::Stopping => self.end_easy_start(now, Some("the belt is stopping")),
            MachineStatus::Starting | MachineStatus::Running if self.easy_start.seen_still => {
                // Decided at the first moving frame, while the app's Start is
                // still on record (`watch_for_deck_start` clears it then).
                let app_start = *self.easy_start.app_start.get_or_insert_with(|| {
                    self.start_sent_at.is_some()
                        || self.handshake_step != crate::protocol::HandshakeStep::Idle
                });
                if self.last_status == MachineStatus::Running
                    && !self.stop_requested
                    && self.profile_sweep.is_none()
                {
                    if self.easy_start.measuring.is_some() {
                        // A calibration run measures at its speed: no ease.
                        self.easy_start.seen_still = false;
                        self.easy_start.app_start = None;
                        log::info!("Easy start skipped: a calibration run sets the speed");
                    } else {
                        self.begin_easy_start(now, app_start);
                    }
                }
            }
            _ => {}
        }
        // A pause or stop asked: hold the step already sent, so no target
        // goes out between the pause and the deck's report of it.
        if self.pending_control.is_some() {
            if let Some(ramp) = self.easy_start.ramp.as_mut() {
                ramp.frozen_at.get_or_insert(now);
            }
        }
        if let Some(ramp) = self.easy_start.ramp {
            if self.speed_hand_epoch != ramp.hand_epoch {
                self.end_easy_start(now, Some("a speed set by hand"));
            } else if now.saturating_duration_since(ramp.started) >= RAMP {
                self.end_easy_start(now, Some("at the target"));
            }
        }
    }

    /// A calibration run sets the belt's speed (`calibration::begin_run`,
    /// after its speed is set): end any ramp, and start no new one while the
    /// run's speed stands, so the belt runs at the measured speed at once.
    pub(crate) fn measure_at_set_speed(&mut self) {
        let now = self.now();
        self.end_easy_start(now, Some("a calibration run"));
        self.easy_start.measuring = Some(self.speed_hand_epoch);
    }

    fn begin_easy_start(&mut self, now: Instant, app_start: bool) {
        let floor = (self.floor_hw_speed() * 10.0).round().clamp(0.0, 255.0) as u8;
        // A start at the treadmill already runs at the deck's own start
        // speed: the ease goes on from there and never slows the belt.
        let running = (self.hardware_speed * 10.0).round().clamp(0.0, 255.0) as u8;
        let from = if app_start { floor } else { floor.max(running) };
        self.easy_start.ramp = Some(Ramp {
            started: now,
            from,
            hand_epoch: self.speed_hand_epoch,
            frozen_at: None,
        });
        self.easy_start.seen_still = false;
        self.easy_start.app_start = None;
        log::info!(
            "Easy start: easing up from byte {from} over {} s ({})",
            RAMP.as_secs(),
            if app_start {
                "app start"
            } else {
                "start at the treadmill"
            }
        );
    }

    fn end_easy_start(&mut self, now: Instant, why: Option<&str>) {
        if self.easy_start.ramp.take().is_some() {
            self.easy_start.ended_at = Some(now);
            if let Some(why) = why {
                log::info!("Easy start ended: {why}");
            }
        }
    }

    /// The speed byte to send for the target's `byte` while the belt eases
    /// up: the ramp's, at a speed the deck runs, never above `byte`. `byte`
    /// itself with no ramp under way.
    pub(crate) fn easy_start_byte(&self, byte: u8, now: Instant) -> u8 {
        let Some(ramp) = self.easy_start.ramp else {
            return byte;
        };
        if self.speed_hand_epoch != ramp.hand_epoch {
            return byte;
        }
        let at = ramp.frozen_at.unwrap_or(now);
        let Some(mut eased) =
            ramp_byte(ramp.from, byte, at.saturating_duration_since(ramp.started))
        else {
            return byte;
        };
        // Down past a speed the deck skips, so each step is one it runs.
        while eased > ramp.from && self.deck_profile.skips(eased) {
            eased -= 1;
        }
        eased.min(byte)
    }

    /// The belt is easing up, or just finished: its speed changes are the
    /// ramp's own, not worth a toast each.
    pub(crate) fn easy_start_quiet(&self, now: Instant) -> bool {
        self.easy_start.ramp.is_some()
            || self
                .easy_start
                .ended_at
                .is_some_and(|t| now.saturating_duration_since(t) < QUIET_AFTER)
    }

    /// The belt is easing up right now (tests and the log).
    #[cfg(test)]
    pub(crate) fn easing_up(&self) -> bool {
        self.easy_start.ramp.is_some()
    }

    pub(crate) fn easy_start_snapshot(&self) -> EasyStartSnapshot {
        EasyStartSnapshot {
            enabled: self.easy_start_enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ramp_climbs_in_steps_from_the_floor_to_the_target() {
        let at = |s: u64| ramp_byte(6, 36, Duration::from_secs(s));
        assert_eq!(at(0), Some(6));
        assert_eq!(at(1), Some(6), "one step every 2 s");
        assert_eq!(at(2), Some(8));
        assert_eq!(at(15), Some(20), "7 steps of 15: 6 + 30 * 14/30");
        assert_eq!(at(29), Some(34));
        assert_eq!(at(30), None, "over at 30 s");
        let bytes: Vec<u8> = (0..30).map(|s| at(s).unwrap()).collect();
        assert!(bytes.windows(2).all(|w| w[0] <= w[1]), "{bytes:?}");
    }

    #[test]
    fn a_target_at_or_under_the_start_is_sent_as_it_is() {
        assert_eq!(ramp_byte(6, 6, Duration::from_secs(4)), Some(6));
        assert_eq!(ramp_byte(10, 6, Duration::ZERO), Some(6));
    }

    #[test]
    fn the_ramp_is_shorter_than_zone_settling() {
        assert!(RAMP < crate::programs::ZONE_SETTLE);
        assert!(STEP < RAMP);
    }
}
