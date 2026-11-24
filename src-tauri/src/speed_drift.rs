//! Speed drift (#251): an optional mode, off by default, that moves the belt
//! speed a little within a band the user sets (say 3.2 to 3.8 km/h), slowly,
//! so a long manual walk at a desk does not sit at one speed for hours.
//!
//! It only adjusts a manual walk whose belt is already running at a speed the
//! user set; it never starts, resumes or stops the belt. Anything else that
//! owns the speed wins: a program (any phase), a pause or stop, a call's quiet
//! speed, Neural flow easing off, DECK_SWEEP, a GROUND_TRUTH run, walk-away
//! (which pauses). A speed set by hand, in the app or on the deck's remote,
//! while drift has the belt ends drift for that walk; the belt goes to the
//! speed set. The next walk (after a stop) starts drift again.
//!
//! Drift waits until the belt has run steadily at the user's speed, inside
//! the band, for one interval, so a ramp after a start (the deck's own, or an
//! easy start) is never taken for a steady speed. Then, every interval, it
//! picks a new speed at most `MAX_CHANGE_KMH` away and moves there one deck
//! speed at a time, each step only once the belt has reached the one before.
//! Every speed it asks is one the deck runs (DECK_PROFILE: its floor, top and
//! skipped speeds) inside the band, and none above STRIDE_FIT when set.
//!
//! Three parts: [`SpeedDrift`], the pure machine driven with injected epoch-ms
//! times; the engine side (`OrchestratorState::tick_speed_drift`, once a
//! second, and `drift_speed`, which the target path applies to the manual
//! intent); and the settings and snapshot.

use crate::engine::{OrchestratorState, TargetSource, STILL_KMH};
use crate::persistence;
use crate::protocol::MachineStatus;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::AppHandle;

// Settings keys (raw values in the `settings` table).
const KEY_ENABLED: &str = "speed_drift_enabled";
const KEY_LOW_KMH: &str = "speed_drift_low_kmh";
const KEY_HIGH_KMH: &str = "speed_drift_high_kmh";
const KEY_EVERY_MINUTES: &str = "speed_drift_every_minutes";

/// The band's ends, calibrated km/h. 0.5 is below the slowest floor a FitShow
/// deck has shown and 12 is the Ares 11 Elite's top: the deck's own floor and
/// top (and STRIDE_FIT) narrow the band further on each treadmill.
pub(crate) const MIN_BAND_KMH: f32 = 0.5;
pub(crate) const MAX_BAND_KMH: f32 = 12.0;
/// The narrowest band: two deck steps (0.1 km/h each), so there are at least
/// three speeds to move between.
pub(crate) const MIN_BAND_WIDTH_KMH: f32 = 0.2;
/// The widest band: past 1.5 km/h the change alters how hard the walk is,
/// which is a program's job, while drift only varies the stride.
pub(crate) const MAX_BAND_WIDTH_KMH: f32 = 1.5;
/// A typical easy desk walk (3.5 km/h) give or take 0.3: the example in #251.
const DEFAULT_LOW_KMH: f32 = 3.2;
const DEFAULT_HIGH_KMH: f32 = 3.8;

/// Minutes between changes. Under 2 the belt would barely settle before the
/// next change and the walk feels restless; past 30 a working day sees only a
/// handful of changes, which is close to off.
pub(crate) const MIN_EVERY_MINUTES: u32 = 2;
pub(crate) const MAX_EVERY_MINUTES: u32 = 30;
/// Five minutes: long enough to settle into each speed, short enough that an
/// hour's walk sees a dozen.
const DEFAULT_EVERY_MINUTES: u32 = 5;

/// The furthest one change moves from the speed before it: three deck steps,
/// a small change in stride the walker notices without breaking step.
pub(crate) const MAX_CHANGE_KMH: f32 = 0.3;
/// The belt moves one deck speed per step, and a step waits at least this long
/// after the one before (and for the belt to reach it), so a 0.3 km/h change
/// takes about 30 s and never feels like a jolt.
const STEP_MS: u64 = 10_000;
/// The belt counts as at a speed within this of it: a deck step (0.1 km/h)
/// plus the calibration's rounding between the byte sent and the one read.
const STEADY_KMH: f32 = 0.15;
/// Two speeds this close are the same one: well under a deck step.
const SAME_KMH: f32 = 1e-3;

const MINUTE_MS: u64 = 60_000;

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub low_kmh: f32,
    pub high_kmh: f32,
    pub every_minutes: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            low_kmh: DEFAULT_LOW_KMH,
            high_kmh: DEFAULT_HIGH_KMH,
            every_minutes: DEFAULT_EVERY_MINUTES,
        }
    }
}

impl Config {
    fn interval_ms(&self) -> u64 {
        u64::from(self.every_minutes) * MINUTE_MS
    }
}

/// The band and interval checked against the limits, or why not.
pub fn validate(low_kmh: f32, high_kmh: f32, every_minutes: u32) -> Result<(), String> {
    let in_range = |v: f32| v.is_finite() && (MIN_BAND_KMH..=MAX_BAND_KMH).contains(&v);
    if !in_range(low_kmh) || !in_range(high_kmh) {
        return Err(format!(
            "Speed drift speeds must be between {MIN_BAND_KMH} and {MAX_BAND_KMH} km/h"
        ));
    }
    let width = high_kmh - low_kmh;
    // A hair of slack, so 3.2 to 3.4 typed in tenths counts as 0.2 wide.
    if width < MIN_BAND_WIDTH_KMH - SAME_KMH || width > MAX_BAND_WIDTH_KMH + SAME_KMH {
        return Err(format!(
            "The fastest speed must be {MIN_BAND_WIDTH_KMH} to {MAX_BAND_WIDTH_KMH} km/h above the slowest"
        ));
    }
    if !(MIN_EVERY_MINUTES..=MAX_EVERY_MINUTES).contains(&every_minutes) {
        return Err(format!(
            "Speed drift changes every {MIN_EVERY_MINUTES} to {MAX_EVERY_MINUTES} minutes"
        ));
    }
    Ok(())
}

/// The config from raw stored values; a bad or inconsistent band falls back
/// to the default band, a bad interval to the default interval.
pub fn config_from_raw(
    enabled: Option<&str>,
    low: Option<&str>,
    high: Option<&str>,
    every: Option<&str>,
) -> Config {
    let d = Config::default();
    let low_kmh = persistence::parse_setting::<f32>(low).unwrap_or(d.low_kmh);
    let high_kmh = persistence::parse_setting::<f32>(high).unwrap_or(d.high_kmh);
    let every_minutes = persistence::setting_in(
        every,
        MIN_EVERY_MINUTES..=MAX_EVERY_MINUTES,
        DEFAULT_EVERY_MINUTES,
    );
    let (low_kmh, high_kmh) = if validate(low_kmh, high_kmh, every_minutes).is_ok() {
        (low_kmh, high_kmh)
    } else {
        (d.low_kmh, d.high_kmh)
    };
    Config {
        enabled: persistence::parse_setting(enabled).unwrap_or(false),
        low_kmh,
        high_kmh,
        every_minutes,
    }
}

static CONFIG: Mutex<Option<Config>> = Mutex::new(None);

/// The saved settings (the defaults until `init`). No I/O.
pub fn config() -> Config {
    CONFIG
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .unwrap_or_default()
}

/// Load the settings. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let read = |k: &str| persistence::read_setting(app, k);
    let cfg = config_from_raw(
        read(KEY_ENABLED).as_deref(),
        read(KEY_LOW_KMH).as_deref(),
        read(KEY_HIGH_KMH).as_deref(),
        read(KEY_EVERY_MINUTES).as_deref(),
    );
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = Some(cfg);
}

/// Save the settings. The engine picks them up on its next second.
pub fn set_config(
    app: &AppHandle,
    enabled: bool,
    low_kmh: f32,
    high_kmh: f32,
    every_minutes: u32,
) -> Result<(), String> {
    validate(low_kmh, high_kmh, every_minutes)?;
    persistence::write_setting(app, KEY_ENABLED, &enabled.to_string())?;
    persistence::write_setting(app, KEY_LOW_KMH, &low_kmh.to_string())?;
    persistence::write_setting(app, KEY_HIGH_KMH, &high_kmh.to_string())?;
    persistence::write_setting(app, KEY_EVERY_MINUTES, &every_minutes.to_string())?;
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = Some(Config {
        enabled,
        low_kmh,
        high_kmh,
        every_minutes,
    });
    log::info!(
        "Speed drift {}: {low_kmh:.1} to {high_kmh:.1} km/h every {every_minutes} min",
        if enabled { "on" } else { "off" }
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// The machine
// ---------------------------------------------------------------------------

/// What drift is doing, for the snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum SpeedDriftState {
    /// Switched off.
    #[default]
    Off,
    /// No manual walk with the belt running (still, paused, a program, a
    /// call, a sweep or a calibration run has the belt).
    Idle,
    /// The speed set by hand is outside the band.
    Outside,
    /// The deck runs fewer than two speeds in the band (after DECK_PROFILE
    /// and STRIDE_FIT).
    NoRoom,
    /// Counting to the first change: the belt runs steadily inside the band.
    Waiting,
    /// Drift sets the speed.
    Drifting,
    /// Drift has the belt but holds still while Neural flow eases it off.
    Held,
    /// A speed set by hand ended drift for this walk.
    Ended,
}

/// What the engine tells the machine each second.
#[derive(Debug, Clone, Copy)]
pub struct Tick<'a> {
    pub now_ms: u64,
    /// The deck reports the belt running.
    pub running: bool,
    /// The belt runs a manual walk drift may adjust: running, no stop in
    /// flight, no program, and nothing else (a call, a sweep, a calibration
    /// run) setting the speed.
    pub manual_running: bool,
    /// The belt is stopped or stopping: the walk is over. A pause is not.
    pub walk_over: bool,
    /// The speed the user set (the manual intent).
    pub base_kmh: f32,
    /// The belt's speed now, calibrated.
    pub reading_kmh: f32,
    /// `OrchestratorState::speed_hand_epoch`: bumped by every speed set by hand.
    pub hand_epoch: u64,
    /// The deck's remote or its own buttons set the speed since the app's
    /// last target.
    pub deck_set: bool,
    /// Neural flow is easing the belt off: drift holds where it is.
    pub yielded: bool,
    /// The speeds drift may ask, slowest first: the ones the deck runs inside
    /// the band, none above STRIDE_FIT.
    pub speeds: &'a [f32],
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    /// Steady inside the band since `since_ms` at the speed set with `epoch`.
    Waiting {
        since_ms: u64,
        epoch: u64,
        base: f32,
    },
    Drifting {
        /// The hand epoch and the user's speed drift began from: a change to
        /// either is someone else's speed.
        epoch: u64,
        base: f32,
        /// The speed asked now, the one before the last step (the deck may
        /// not have been sent the step yet), and the one it is moving to.
        speed: f32,
        prev: f32,
        goal: f32,
        /// When the next step may go, and when the next goal is picked.
        next_step_ms: u64,
        next_goal_ms: u64,
    },
}

#[derive(Debug, Clone)]
pub struct SpeedDrift {
    phase: Phase,
    /// The belt stopped running (a pause, the countdown after a resume)
    /// while drift had it: the speed to keep asking until it runs again, for
    /// the hand epoch drift had. Letting go at once would raise the target
    /// above the one last sent, and the engine resumes a paused belt whose
    /// manual target rises: so the target never rises while the belt is not
    /// running. A speed set by hand (a new epoch) or a stop drops it.
    parked: Option<(u64, f32)>,
    /// A speed set by hand ended drift for this walk.
    ended: bool,
    state: SpeedDriftState,
    /// The config the phase was worked out under: a change starts over.
    cfg: Option<Config>,
    /// xorshift32 state for picking the next speed; 0 until first seeded.
    rng: u32,
}

impl Default for SpeedDrift {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            parked: None,
            ended: false,
            state: SpeedDriftState::Off,
            cfg: None,
            rng: 0,
        }
    }
}

/// The speed in `speeds` nearest `kmh`.
fn nearest(speeds: &[f32], kmh: f32) -> f32 {
    speeds
        .iter()
        .copied()
        .min_by(|a, b| (a - kmh).abs().total_cmp(&(b - kmh).abs()))
        .unwrap_or(kmh)
}

fn listed(speeds: &[f32], kmh: f32) -> bool {
    speeds.iter().any(|s| (s - kmh).abs() < SAME_KMH)
}

impl SpeedDrift {
    /// The speed drift asks now, for a manual intent set with `hand_epoch`:
    /// none unless drift has the belt and no speed was set by hand since.
    pub fn target(&self, hand_epoch: u64) -> Option<f32> {
        match (self.phase, self.parked) {
            (Phase::Drifting { epoch, speed, .. }, _) if epoch == hand_epoch => Some(speed),
            (_, Some((epoch, speed))) if epoch == hand_epoch => Some(speed),
            _ => None,
        }
    }

    pub fn state(&self) -> SpeedDriftState {
        self.state
    }

    /// While waiting: milliseconds until the first change, if the belt stays
    /// steady.
    pub fn starts_in_ms(&self, cfg: &Config, now_ms: u64) -> Option<u64> {
        match (self.state, self.phase) {
            (SpeedDriftState::Waiting, Phase::Waiting { since_ms, .. }) => {
                Some((since_ms + cfg.interval_ms()).saturating_sub(now_ms))
            }
            _ => None,
        }
    }

    fn random(&mut self, now_ms: u64) -> u32 {
        if self.rng == 0 {
            self.rng = (now_ms as u32 ^ (now_ms >> 32) as u32) | 1;
        }
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    /// A new goal within `MAX_CHANGE_KMH` of `from`, other than `from`; `from`
    /// itself when the band has no other.
    fn pick_goal(&mut self, speeds: &[f32], from: f32, now_ms: u64) -> f32 {
        let options: Vec<f32> = speeds
            .iter()
            .copied()
            .filter(|s| {
                let d = (s - from).abs();
                (SAME_KMH..=MAX_CHANGE_KMH + SAME_KMH).contains(&d)
            })
            .collect();
        if options.is_empty() {
            return from;
        }
        options[self.random(now_ms) as usize % options.len()]
    }

    /// One second. True when the speed drift asks changed (or drift let go).
    pub fn tick(&mut self, cfg: &Config, t: &Tick) -> bool {
        let before = self.asked();
        self.step(cfg, t);
        let after = self.asked();
        match (before, after) {
            (Some(a), Some(b)) => (a - b).abs() >= SAME_KMH,
            (None, None) => false,
            _ => true,
        }
    }

    fn asked(&self) -> Option<f32> {
        match (self.phase, self.parked) {
            (Phase::Drifting { speed, .. }, _) => Some(speed),
            (_, Some((_, speed))) => Some(speed),
            _ => None,
        }
    }

    fn step(&mut self, cfg: &Config, t: &Tick) {
        if t.running || t.walk_over {
            self.parked = None;
        } else if let Phase::Drifting {
            epoch, speed, prev, ..
        } = self.phase
        {
            // The lower of the two: whichever the deck was sent last.
            self.parked = Some((epoch, speed.min(prev)));
        }
        if self.cfg != Some(*cfg) {
            // New settings start over, from the user's own speed.
            self.cfg = Some(*cfg);
            self.phase = Phase::Idle;
            self.ended = false;
        }
        if !cfg.enabled {
            self.phase = Phase::Idle;
            self.state = SpeedDriftState::Off;
            return;
        }
        if t.walk_over {
            self.ended = false;
        }
        if let Phase::Drifting { epoch, .. } = self.phase {
            if t.hand_epoch != epoch || t.deck_set {
                log::info!("Speed drift ended for this walk: a speed was set by hand");
                self.ended = true;
            }
        }
        if self.ended {
            self.phase = Phase::Idle;
            self.state = SpeedDriftState::Ended;
            return;
        }
        if !t.manual_running {
            self.phase = Phase::Idle;
            self.state = SpeedDriftState::Idle;
            return;
        }
        if t.speeds.len() < 2 {
            self.phase = Phase::Idle;
            self.state = SpeedDriftState::NoRoom;
            return;
        }
        let now = t.now_ms;
        let interval = cfg.interval_ms();
        let lo = t.speeds[0];
        let hi = t.speeds[t.speeds.len() - 1];
        let inside = t.base_kmh >= lo - SAME_KMH && t.base_kmh <= hi + SAME_KMH;

        // Another owner moved the user's speed (a resume, a call ending):
        // start again from the new one.
        if let Phase::Drifting { base, .. } = self.phase {
            if (base - t.base_kmh).abs() >= SAME_KMH {
                self.phase = Phase::Idle;
            }
        }
        match self.phase {
            Phase::Waiting { epoch, base, .. }
                if epoch == t.hand_epoch && (base - t.base_kmh).abs() < SAME_KMH => {}
            Phase::Drifting { .. } => {}
            _ => {
                self.phase = Phase::Waiting {
                    since_ms: now,
                    epoch: t.hand_epoch,
                    base: t.base_kmh,
                };
            }
        }

        match self.phase {
            Phase::Waiting {
                since_ms,
                epoch,
                base,
            } => {
                let steady =
                    !t.deck_set && !t.yielded && (t.reading_kmh - t.base_kmh).abs() <= STEADY_KMH;
                if !inside || !steady {
                    self.phase = Phase::Waiting {
                        since_ms: now,
                        epoch,
                        base,
                    };
                    self.state = if inside {
                        SpeedDriftState::Waiting
                    } else {
                        SpeedDriftState::Outside
                    };
                } else if now.saturating_sub(since_ms) >= interval {
                    let speed = nearest(t.speeds, t.base_kmh);
                    let goal = self.pick_goal(t.speeds, speed, now);
                    // The deck was last sent the user's own speed.
                    let prev = speed.min(t.base_kmh);
                    log::info!("Speed drift starts at {speed:.2} km/h, heading for {goal:.2} km/h");
                    self.phase = Phase::Drifting {
                        epoch,
                        base,
                        speed,
                        prev,
                        goal,
                        next_step_ms: now,
                        next_goal_ms: now + interval,
                    };
                    self.state = SpeedDriftState::Drifting;
                } else {
                    self.state = SpeedDriftState::Waiting;
                }
            }
            Phase::Drifting {
                epoch,
                base,
                mut speed,
                mut prev,
                mut goal,
                mut next_step_ms,
                mut next_goal_ms,
            } => {
                // The band, DECK_PROFILE or STRIDE_FIT moved under it: keep to
                // the speeds allowed now.
                if !listed(t.speeds, speed) {
                    let moved = nearest(t.speeds, speed);
                    prev = prev.min(speed);
                    speed = moved;
                }
                if !listed(t.speeds, goal) {
                    goal = nearest(t.speeds, goal);
                }
                let at_speed = (t.reading_kmh - speed).abs() <= STEADY_KMH;
                if t.yielded {
                    // Neural flow eases off from drift's speed; drift waits.
                    next_step_ms = next_step_ms.max(now + STEP_MS);
                    next_goal_ms = next_goal_ms.max(now + STEP_MS);
                    self.state = SpeedDriftState::Held;
                } else {
                    if (speed - goal).abs() >= SAME_KMH {
                        if now >= next_step_ms && at_speed {
                            prev = speed;
                            speed = next_toward(t.speeds, speed, goal);
                            next_step_ms = now + STEP_MS;
                            if (speed - goal).abs() < SAME_KMH {
                                next_goal_ms = now + interval;
                            }
                        }
                    } else if now >= next_goal_ms && at_speed {
                        goal = self.pick_goal(t.speeds, speed, now);
                        next_step_ms = now;
                        next_goal_ms = now + interval;
                        log::info!("Speed drift heads for {goal:.2} km/h");
                    }
                    self.state = SpeedDriftState::Drifting;
                }
                self.phase = Phase::Drifting {
                    epoch,
                    base,
                    speed,
                    prev,
                    goal,
                    next_step_ms,
                    next_goal_ms,
                };
            }
            Phase::Idle => {}
        }
    }
}

/// The speed after `from` in `speeds` toward `goal`: one deck speed.
fn next_toward(speeds: &[f32], from: f32, goal: f32) -> f32 {
    if goal > from {
        speeds
            .iter()
            .copied()
            .find(|s| *s > from + SAME_KMH)
            .map_or(goal, |s| s.min(goal))
    } else {
        speeds
            .iter()
            .rev()
            .copied()
            .find(|s| *s < from - SAME_KMH)
            .map_or(goal, |s| s.max(goal))
    }
}

// ---------------------------------------------------------------------------
// Engine side
// ---------------------------------------------------------------------------

/// Drift's part of `OrchestratorState`: the settings the engine last read, the
/// machine, and when it last changed the speed.
#[derive(Debug, Clone, Default)]
pub struct DriftRun {
    pub cfg: Config,
    pub machine: SpeedDrift,
    /// When drift last changed the target, so the worker doesn't toast the
    /// deck's speed change that follows (the HUD line explains it).
    pub changed_at: Option<Instant>,
}

impl DriftRun {
    /// Drift changed the target within `d`.
    pub fn changed_within(&self, d: Duration) -> bool {
        self.changed_at.is_some_and(|at| at.elapsed() < d)
    }
}

impl OrchestratorState {
    /// The speeds drift may ask under `cfg`, slowest first: one per byte the
    /// deck runs (DECK_PROFILE's skips left out) from the belt's floor to the
    /// top speed, inside the band, none above STRIDE_FIT when set.
    pub(crate) fn drift_speeds(&self, cfg: &Config) -> Vec<f32> {
        let floor = self.min_physical_speed();
        let mut high = cfg.high_kmh;
        if let Some(fit) = self.stride_fit_kmh {
            high = high.min(fit);
        }
        let top = (self.top_hw_speed() * 10.0)
            .round()
            .clamp(0.0, f32::from(u8::MAX)) as u8;
        let mut out: Vec<f32> = vec![];
        for b in 1..=top {
            if self.deck_profile.skips(b) {
                continue;
            }
            let kmh = self.physical_for_byte(b, 0);
            if kmh < floor - SAME_KMH || kmh < cfg.low_kmh - SAME_KMH || kmh > high + SAME_KMH {
                continue;
            }
            if out.last().is_some_and(|k| (k - kmh).abs() < SAME_KMH) {
                continue;
            }
            out.push(kmh);
        }
        out
    }

    /// Drift, once a second, before Neural flow (which eases from drift's
    /// speed). `quiet`: a call holds the belt at the quiet speed;
    /// `calibrating`: a GROUND_TRUTH run has the belt.
    pub fn tick_speed_drift(&mut self, now: Instant, now_ms: u64, quiet: bool, calibrating: bool) {
        // Easy stop eases the belt down to stop: drift holds as it is, so no
        // drift step lands mid-ramp. A speed set by hand ends the ramp, and
        // drift then sees it as always.
        if self.easing_down() {
            return;
        }
        let (base, _, source) = self.resolve_base(now);
        let cfg = self.speed_drift.cfg;
        let speeds = self.drift_speeds(&cfg);
        let manual_running = self.last_status == MachineStatus::Running
            && !self.stop_requested
            && !self.awaiting_first_packet
            && self.program.is_none()
            && self.profile_sweep.is_none()
            && !calibrating
            && !quiet
            && matches!(source, TargetSource::Manual | TargetSource::ManualIdle)
            && base >= STILL_KMH;
        let tick = Tick {
            now_ms,
            running: self.last_status == MachineStatus::Running,
            manual_running,
            walk_over: matches!(
                self.last_status,
                MachineStatus::Stopped | MachineStatus::Stopping
            ),
            base_kmh: base,
            reading_kmh: self.to_physical_speed(self.hardware_speed),
            hand_epoch: self.speed_hand_epoch,
            deck_set: self.deck_set_speed.is_some(),
            yielded: self.focus_stability_enabled && self.neural.active(),
            speeds: &speeds,
        };
        let was = self.speed_drift.machine.target(self.speed_hand_epoch);
        if self.speed_drift.machine.tick(&cfg, &tick) {
            self.speed_drift.changed_at = Some(now);
            let now_asked = self.speed_drift.machine.target(self.speed_hand_epoch);
            log::info!(
                "Speed drift: {} -> {} km/h (set by hand {:.2})",
                was.map_or("none".into(), |k| format!("{k:.2}")),
                now_asked.map_or("none".into(), |k| format!("{k:.2}")),
                base
            );
        }
    }

    /// `base` (what the manual and program intents ask) with drift's speed in
    /// place of a manual one while drift has the belt. A program's target, a
    /// still target and a call's quiet speed pass through unchanged, and a
    /// speed set by hand since drift's last tick wins at once.
    pub(crate) fn drift_speed(&self, base: f32, source: TargetSource) -> f32 {
        if base < STILL_KMH
            || !matches!(source, TargetSource::Manual | TargetSource::ManualIdle)
            || self.program.is_some()
            || crate::snapshot::calls::quiet_active()
        {
            return base;
        }
        self.speed_drift
            .machine
            .target(self.speed_hand_epoch)
            .unwrap_or(base)
    }

    /// Drift for the snapshot: `cfg` is the saved settings (fresh after a
    /// save), the state is the engine's.
    pub fn speed_drift_snapshot(&self, cfg: &Config, now_ms: u64) -> SpeedDriftSnapshot {
        let m = &self.speed_drift.machine;
        // Settings saved since the engine's last second show as starting over.
        let state = if !cfg.enabled {
            SpeedDriftState::Off
        } else if self.speed_drift.cfg != *cfg {
            SpeedDriftState::Idle
        } else {
            m.state()
        };
        let speeds = self.drift_speeds(cfg);
        let band = (speeds.len() >= 2).then(|| (speeds[0], speeds[speeds.len() - 1]));
        let speed_kmh = match state {
            SpeedDriftState::Drifting | SpeedDriftState::Held => m.target(self.speed_hand_epoch),
            _ => None,
        };
        let starts_in_s = m
            .starts_in_ms(cfg, now_ms)
            .filter(|_| state == SpeedDriftState::Waiting)
            .map(|ms| ms.div_ceil(1000) as u32);
        SpeedDriftSnapshot {
            enabled: cfg.enabled,
            low_kmh: cfg.low_kmh,
            high_kmh: cfg.high_kmh,
            every_minutes: cfg.every_minutes,
            state,
            band_low_kmh: band.map(|b| b.0),
            band_high_kmh: band.map(|b| b.1),
            speed_kmh,
            starts_in_s,
            message: message(state, cfg, band, speed_kmh, starts_in_s),
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SpeedDriftSnapshot {
    pub enabled: bool,
    /// The band as set, calibrated km/h.
    pub low_kmh: f32,
    pub high_kmh: f32,
    /// Minutes between changes, 2 to 30.
    pub every_minutes: u32,
    pub state: SpeedDriftState,
    /// The band drift uses on this treadmill: the slowest and fastest speeds
    /// the deck runs inside the band, none above STRIDE_FIT. None while the
    /// deck runs fewer than two there.
    pub band_low_kmh: Option<f32>,
    pub band_high_kmh: Option<f32>,
    /// While drifting or held: the speed drift asks.
    pub speed_kmh: Option<f32>,
    /// While waiting: whole seconds until the first change.
    pub starts_in_s: Option<u32>,
    /// Rust's sentence for System's "Right now" line and the HUD tooltip.
    pub message: String,
}

fn band_text(band: Option<(f32, f32)>, cfg: &Config) -> String {
    let (lo, hi) = band.unwrap_or((cfg.low_kmh, cfg.high_kmh));
    format!("{lo:.1} to {hi:.1} km/h")
}

fn message(
    state: SpeedDriftState,
    cfg: &Config,
    band: Option<(f32, f32)>,
    speed: Option<f32>,
    starts_in_s: Option<u32>,
) -> String {
    let range = band_text(band, cfg);
    match state {
        SpeedDriftState::Off => "Speed drift is off.".into(),
        SpeedDriftState::Idle => {
            "Speed drift waits for a manual walk with the belt running.".into()
        }
        SpeedDriftState::Outside => {
            format!("Speed drift starts once the belt runs at a speed you set from {range}.")
        }
        SpeedDriftState::NoRoom => format!(
            "The deck runs fewer than two speeds from {:.1} to {:.1} km/h, so speed drift waits.",
            cfg.low_kmh, cfg.high_kmh
        ),
        SpeedDriftState::Waiting => {
            let mins = starts_in_s.unwrap_or(0).div_ceil(60).max(1);
            format!("Speed drift makes its first change in {mins} min.")
        }
        SpeedDriftState::Drifting => format!(
            "Speed drift has the belt at {:.1} km/h, within {range}.",
            speed.unwrap_or(0.0)
        ),
        SpeedDriftState::Held => {
            "Speed drift holds its speed while Neural flow eases the belt off.".into()
        }
        SpeedDriftState::Ended => {
            "A speed set by hand ended speed drift until the next walk.".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;
    const SPEEDS: [f32; 7] = [3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8];

    fn on() -> Config {
        Config {
            enabled: true,
            ..Config::default()
        }
    }

    fn tick<'a>(now_ms: u64, base: f32, reading: f32, epoch: u64) -> Tick<'a> {
        Tick {
            now_ms,
            running: true,
            manual_running: true,
            walk_over: false,
            base_kmh: base,
            reading_kmh: reading,
            hand_epoch: epoch,
            deck_set: false,
            yielded: false,
            speeds: &SPEEDS,
        }
    }

    /// Runs `secs` seconds with the belt following drift's speed at once.
    fn follow(d: &mut SpeedDrift, cfg: &Config, from_s: u64, secs: u64, base: f32) -> Vec<f32> {
        let mut asked = vec![];
        for s in from_s..from_s + secs {
            let reading = d.target(1).unwrap_or(base);
            d.tick(cfg, &tick(T0 + s * 1000, base, reading, 1));
            if let Some(k) = d.target(1) {
                asked.push(k);
            }
        }
        asked
    }

    #[test]
    fn off_by_default_and_never_asks_a_speed() {
        let mut d = SpeedDrift::default();
        let cfg = Config::default();
        assert!(!cfg.enabled);
        for s in 0..3600 {
            d.tick(&cfg, &tick(T0 + s * 1000, 3.5, 3.5, 1));
        }
        assert_eq!(d.target(1), None);
        assert_eq!(d.state(), SpeedDriftState::Off);
    }

    #[test]
    fn waits_one_interval_at_a_steady_speed_then_moves_in_small_slow_steps() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        let asked = follow(&mut d, &cfg, 0, 5 * 60 - 1, 3.5);
        assert!(asked.is_empty(), "nothing before the interval");
        assert_eq!(d.state(), SpeedDriftState::Waiting);
        let asked = follow(&mut d, &cfg, 5 * 60 - 1, 3 * 3600, 3.5);
        assert!(!asked.is_empty());
        for w in asked.windows(2) {
            assert!(
                (w[1] - w[0]).abs() <= 0.1 + 1e-3,
                "one deck step at a time: {w:?}"
            );
        }
        assert!(asked.iter().all(|k| (3.2 - 1e-3..=3.8 + 1e-3).contains(k)));
        let distinct: std::collections::BTreeSet<i32> =
            asked.iter().map(|k| (k * 10.0).round() as i32).collect();
        assert!(distinct.len() >= 3, "it actually varies: {distinct:?}");
        // Steps at least STEP_MS apart.
        let mut last_change: Option<usize> = None;
        for (i, w) in asked.windows(2).enumerate() {
            if (w[1] - w[0]).abs() > 1e-3 {
                if let Some(l) = last_change {
                    assert!(i - l >= 10, "steps {l} and {i} too close");
                }
                last_change = Some(i);
            }
        }
    }

    #[test]
    fn no_step_until_the_belt_reaches_the_speed_before() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        follow(&mut d, &cfg, 0, 5 * 60 + 1, 3.5);
        let first = d.target(1).unwrap();
        // The belt never gets there: drift stays put.
        for s in 0..120 {
            d.tick(&cfg, &tick(T0 + (400 + s) * 1000, 3.5, 2.0, 1));
        }
        let held = d.target(1).unwrap();
        assert!((held - first).abs() <= 0.1 + 1e-3);
    }

    #[test]
    fn a_ramp_after_a_start_is_not_a_steady_speed() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        // Ten minutes with the belt still below the speed set.
        for s in 0..600 {
            d.tick(&cfg, &tick(T0 + s * 1000, 3.5, 2.5, 1));
        }
        assert_eq!(d.target(1), None);
        assert_eq!(d.state(), SpeedDriftState::Waiting);
    }

    #[test]
    fn a_speed_set_by_hand_ends_drift_for_the_walk_and_a_stop_resets_it() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        follow(&mut d, &cfg, 0, 6 * 60, 3.5);
        assert!(d.target(1).is_some());
        // The press: epoch 2. The target falls back at once.
        assert_eq!(d.target(2), None);
        d.tick(&cfg, &tick(T0 + 400_000, 3.6, 3.6, 2));
        assert_eq!(d.state(), SpeedDriftState::Ended);
        for s in 0..3600 {
            d.tick(&cfg, &tick(T0 + 401_000 + s * 1000, 3.6, 3.6, 2));
        }
        assert_eq!(d.target(2), None, "ended for the rest of the walk");
        // A pause keeps it ended.
        let mut paused = tick(T0 + 5_000_000, 3.6, 0.0, 2);
        paused.manual_running = false;
        paused.running = false;
        d.tick(&cfg, &paused);
        assert_eq!(d.state(), SpeedDriftState::Ended);
        // A stop ends the walk; the next one drifts again.
        let mut stopped = paused;
        stopped.walk_over = true;
        d.tick(&cfg, &stopped);
        assert_eq!(d.state(), SpeedDriftState::Idle);
        for s in 0..6 * 60 {
            d.tick(&cfg, &tick(T0 + 6_000_000 + s * 1000, 3.6, 3.6, 3));
        }
        assert!(d.target(3).is_some());
    }

    #[test]
    fn the_remote_ends_drift_like_a_press() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        follow(&mut d, &cfg, 0, 6 * 60, 3.5);
        let mut t = tick(T0 + 400_000, 3.5, 3.9, 1);
        t.deck_set = true;
        d.tick(&cfg, &t);
        assert_eq!(d.state(), SpeedDriftState::Ended);
        assert_eq!(d.target(1), None);
    }

    #[test]
    fn a_hand_change_before_drift_starts_only_restarts_the_wait() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        follow(&mut d, &cfg, 0, 4 * 60, 3.5);
        for s in 0..4 * 60 {
            d.tick(&cfg, &tick(T0 + (300 + s) * 1000, 3.4, 3.4, 2));
        }
        assert_eq!(d.state(), SpeedDriftState::Waiting, "a fresh interval");
        for s in 0..90 {
            d.tick(&cfg, &tick(T0 + (540 + s) * 1000, 3.4, 3.4, 2));
        }
        assert!(d.target(2).is_some());
    }

    /// Paused, drift keeps asking its speed (never a higher one, which would
    /// read as a resume), and lets go once the belt runs again by a press,
    /// where it waits a full interval.
    #[test]
    fn a_pause_parks_drift_and_the_resume_waits_a_full_interval() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        follow(&mut d, &cfg, 0, 10 * 60, 3.5);
        let at = d.target(1).unwrap();
        let mut paused = tick(T0 + 700_000, 3.5, 0.0, 1);
        paused.manual_running = false;
        paused.running = false;
        for s in 0..600 {
            paused.now_ms = T0 + 700_000 + s * 1000;
            d.tick(&cfg, &paused);
            let k = d.target(1).unwrap();
            assert!(k <= at + 1e-6, "never above the speed it had: {k} > {at}");
        }
        assert_eq!(d.state(), SpeedDriftState::Idle);
        // A speed set by hand while paused is the user's.
        assert_eq!(d.target(2), None);
        // Running again: drift lets go and waits.
        let asked = follow(&mut d, &cfg, 1400, 5 * 60 - 1, 3.5);
        assert!(asked.is_empty(), "{asked:?}");
        assert_eq!(d.state(), SpeedDriftState::Waiting);
    }

    /// A pause just after a step parks at the speed before it, which the
    /// deck may still be running.
    #[test]
    fn a_pause_right_after_a_step_parks_at_the_lower_speed() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        // Run until a step up has just happened.
        let mut s = 0;
        let (before, after) = loop {
            let reading = d.target(1).unwrap_or(3.5);
            let was = d.target(1);
            d.tick(&cfg, &tick(T0 + s * 1000, 3.5, reading, 1));
            if let (Some(a), Some(b)) = (was, d.target(1)) {
                if b > a + 1e-3 {
                    break (a, b);
                }
            }
            s += 1;
            assert!(s < 6 * 3600, "no step up");
        };
        assert!(after > before);
        let mut paused = tick(T0 + (s + 1) * 1000, 3.5, 0.0, 1);
        paused.manual_running = false;
        paused.running = false;
        d.tick(&cfg, &paused);
        assert_eq!(d.target(1), Some(before));
    }

    #[test]
    fn neural_flow_easing_holds_drift_where_it_is() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        follow(&mut d, &cfg, 0, 6 * 60, 3.5);
        let at = d.target(1).unwrap();
        for s in 0..600 {
            let mut t = tick(T0 + (400 + s) * 1000, 3.5, 3.0, 1);
            t.yielded = true;
            d.tick(&cfg, &t);
        }
        assert_eq!(d.state(), SpeedDriftState::Held);
        assert_eq!(d.target(1), Some(at));
    }

    #[test]
    fn a_speed_outside_the_band_is_left_alone() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        let asked = follow(&mut d, &cfg, 0, 3600, 4.5);
        assert!(asked.is_empty());
        assert_eq!(d.state(), SpeedDriftState::Outside);
    }

    #[test]
    fn a_band_with_one_speed_has_no_room() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        let mut t = tick(T0, 3.5, 3.5, 1);
        let one = [3.5];
        t.speeds = &one;
        d.tick(&cfg, &t);
        assert_eq!(d.state(), SpeedDriftState::NoRoom);
        assert_eq!(d.target(1), None);
    }

    #[test]
    fn a_narrower_band_pulls_the_speed_in_at_once() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        follow(&mut d, &cfg, 0, 3600, 3.5);
        let narrow = [3.4, 3.5];
        for s in 0..60 {
            let mut t = tick(T0 + (3600 + s) * 1000, 3.5, 3.5, 1);
            t.speeds = &narrow;
            d.tick(&cfg, &t);
            let k = d.target(1).unwrap();
            assert!((3.4 - 1e-3..=3.5 + 1e-3).contains(&k), "{k}");
        }
    }

    #[test]
    fn new_settings_start_over_from_the_speed_set() {
        let mut d = SpeedDrift::default();
        let cfg = on();
        follow(&mut d, &cfg, 0, 6 * 60, 3.5);
        assert!(d.target(1).is_some());
        let cfg2 = Config {
            every_minutes: 10,
            ..cfg
        };
        d.tick(&cfg2, &tick(T0 + 400_000, 3.5, 3.5, 1));
        assert_eq!(d.target(1), None);
        assert_eq!(d.state(), SpeedDriftState::Waiting);
    }

    #[test]
    fn validate_keeps_the_band_small_and_the_interval_in_range() {
        assert!(validate(3.2, 3.8, 5).is_ok());
        assert!(validate(3.2, 3.4, 2).is_ok());
        assert!(validate(3.2, 3.3, 5).is_err(), "too narrow");
        assert!(validate(3.0, 4.6, 5).is_err(), "too wide");
        assert!(validate(3.8, 3.2, 5).is_err(), "backwards");
        assert!(validate(0.2, 0.6, 5).is_err(), "below the range");
        assert!(validate(3.2, 3.8, 1).is_err());
        assert!(validate(3.2, 3.8, 31).is_err());
        assert!(validate(f32::NAN, 3.8, 5).is_err());
    }

    #[test]
    fn stored_values_fall_back_to_the_defaults() {
        assert_eq!(config_from_raw(None, None, None, None), Config::default());
        let c = config_from_raw(Some("true"), Some("4.0"), Some("4.6"), Some("8"));
        assert_eq!(
            c,
            Config {
                enabled: true,
                low_kmh: 4.0,
                high_kmh: 4.6,
                every_minutes: 8
            }
        );
        let bad = config_from_raw(Some("true"), Some("5"), Some("1"), Some("99"));
        assert_eq!(bad.low_kmh, DEFAULT_LOW_KMH);
        assert_eq!(bad.high_kmh, DEFAULT_HIGH_KMH);
        assert_eq!(bad.every_minutes, DEFAULT_EVERY_MINUTES);
    }

    #[test]
    fn the_messages_name_the_band() {
        let cfg = on();
        let band = Some((3.2, 3.8));
        assert_eq!(
            message(SpeedDriftState::Drifting, &cfg, band, Some(3.5), None),
            "Speed drift has the belt at 3.5 km/h, within 3.2 to 3.8 km/h."
        );
        assert_eq!(
            message(SpeedDriftState::Waiting, &cfg, band, None, Some(61)),
            "Speed drift makes its first change in 2 min."
        );
    }
}
