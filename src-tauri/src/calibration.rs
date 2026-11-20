//! GROUND_TRUTH, the guided calibration (a Labs switch, off by default): the
//! limits a calibration point must stay inside (shared with the engine and
//! the UI), the tap run Rust keeps while the user walks it, and the point it
//! gives.
//!
//! A point is `(speed, stride_length_cm, distance_scale)`: `speed` is what the
//! deck reports (hardware km/h) and `distance_scale` is reported distance over
//! real distance, so physical km/h = hardware km/h / scale (see `engine.rs`).
//!
//! One run per target speed: RUN (the user's press) sets the belt to the
//! target; then the user taps (Space, Alt+Space anywhere, or the TAP button)
//! each time the mark on the belt passes. Rust timestamps every tap; the real
//! speed is the belt loop's length over the median time between taps
//! (`tap.rs`), and the run finishes by itself once enough of those times
//! agree. Rust also samples the deck's reported speed while the user taps,
//! so a belt that stops or changes speed starts the tapping over instead of
//! giving a wrong point. Tapping never sends anything to the treadmill.
//!
//! Once the belt is timed, a short footstep phase (`steps.rs`) can follow:
//! the user taps each footstep with the same inputs, and the stride is the
//! measured belt speed over the cadence from those taps.
//!
//! The belt loop's length is asked once and kept per treadmill, by the same
//! Bluetooth identity as DECK_PROFILE (`belt_key_for`).
//!
//! RUN ALL walks the whole list as a sweep: Rust keeps which speeds are saved,
//! skipped or still to do. Saving or skipping one never touches the belt; the
//! next speed waits for the user's RUN.
//!
//! The list of target speeds is a setting the user edits (ADD_SPEED and
//! remove): kept in the settings table, checked here, and read by RUN ALL.

mod steps;
mod tap;

pub use steps::StepRunView;

use crate::engine::OrchestratorState;
use crate::protocol::CalibrationPoint;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tauri::AppHandle;

/// Stride a point may have, cm. Wider than a walking stride on purpose: short
/// shuffles at a desk and long strides both happen.
pub const MIN_STRIDE_CM: f32 = 20.0;
pub const MAX_STRIDE_CM: f32 = 200.0;
/// Distance scale a point may have (reported over real distance).
pub const MIN_SCALE: f32 = 0.2;
pub const MAX_SCALE: f32 = 3.0;
/// The belt loop length the user can enter, cm: from the shortest walking
/// pad loop (about 2 m) with room to spare, to well past a running deck's.
pub const MIN_BELT_CM: f32 = 50.0;
pub const MAX_BELT_CM: f32 = 1000.0;
/// The speeds the wizard offers until the user edits the list, in deck km/h.
pub const DEFAULT_TARGETS_KMH: [f32; 5] = [2.0, 3.0, 4.0, 5.0, 6.0];
/// A reported speed below this is a still belt.
const STOPPED_BELOW_KMH: f32 = 0.05;
/// Reported speeds during one run may differ by at most this much: the deck
/// reports tenths and wobbles by one, while a real change is a step of 0.5 or
/// more from a press or a program.
const MAX_SPREAD_KMH: f32 = 0.25;

const KEY_WIZARD: &str = "labs_calibration_wizard";
/// The edited list of target speeds, a JSON array of deck km/h.
const KEY_TARGETS: &str = "labs_calibration_targets";
static WIZARD_ON: AtomicBool = AtomicBool::new(false);
static RUN: Mutex<Option<Run>> = Mutex::new(None);
/// A run is open and still taking taps: the input hook reads this to know
/// whether Alt+Space is a tap, without taking a lock.
static TAP_LISTENING: AtomicBool = AtomicBool::new(false);
static SWEEP: Mutex<Option<Sweep>> = Mutex::new(None);
/// The stored target list; None until the user edits it (the defaults apply).
static TARGETS: Mutex<Option<Vec<f32>>> = Mutex::new(None);
/// Targets this close together are one speed.
const SAME_TARGET_KMH: f32 = 0.05;

/// The limits as the UI sees them, so both sides use one set of numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationLimits {
    pub min_stride_cm: f32,
    pub max_stride_cm: f32,
    pub min_scale: f32,
    pub max_scale: f32,
    pub min_belt_cm: f32,
    pub max_belt_cm: f32,
    /// Agreeing times between taps a run needs (`tap::MIN_AGREEING`).
    pub min_agreeing: u32,
    /// Agreeing times between footsteps a step run needs (`steps::MIN_STEPS`).
    pub min_steps: u32,
}

impl Default for CalibrationLimits {
    fn default() -> Self {
        Self {
            min_stride_cm: MIN_STRIDE_CM,
            max_stride_cm: MAX_STRIDE_CM,
            min_scale: MIN_SCALE,
            max_scale: MAX_SCALE,
            min_belt_cm: MIN_BELT_CM,
            max_belt_cm: MAX_BELT_CM,
            min_agreeing: tap::MIN_AGREEING as u32,
            min_steps: steps::MIN_STEPS as u32,
        }
    }
}

/// The point one run gives, with the numbers behind it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationResult {
    // `CalibrationPoint` has serde aliases, which ts-rs can't parse.
    #[ts(
        type = "{ speed: number, stride_length_cm: number, distance_scale: number, runs: number }"
    )]
    pub point: CalibrationPoint,
    /// The speed the belt really ran, km/h.
    pub real_kmh: f32,
    /// The deck's mean reported speed over the taps, km/h.
    pub reported_kmh: f32,
    /// The distance the belt really ran between the first and last tap, m.
    pub real_m: f32,
    /// The distance the deck reported for the same time, m.
    pub reported_m: f32,
    /// Steps a minute, when steps were timed (none for a belt-only run).
    pub cadence_spm: Option<f32>,
    /// False when the stride came from the current calibration.
    pub stride_measured: bool,
    /// The saved point at the run's speed with this run averaged in (ADD
    /// RUN); none when the speed has no saved point yet.
    #[ts(
        type = "{ speed: number, stride_length_cm: number, distance_scale: number, runs: number } | null"
    )]
    pub averaged: Option<CalibrationPoint>,
}

/// A saved point this close to a run's target is that target's point: the
/// same tolerance the dashboard's rows use (`guided.ts`), so the row and the
/// averaged point agree.
const SAME_SPEED_KMH: f32 = 0.05;

/// `saved` with one more run folded in, every run weighing the same. A run
/// whose stride was borrowed from the calibration keeps the saved stride; one
/// whose factor was borrowed (`factor_counted` false) keeps the saved factor.
pub fn averaged_point(
    saved: &CalibrationPoint,
    run: &CalibrationResult,
    factor_counted: bool,
) -> CalibrationPoint {
    let n = saved.runs.max(1);
    let mean = |old: f32, new: f32| (old * n as f32 + new) / (n + 1) as f32;
    CalibrationPoint {
        speed: saved.speed,
        stride_length_cm: if run.stride_measured {
            mean(saved.stride_length_cm, run.point.stride_length_cm)
        } else {
            saved.stride_length_cm
        },
        distance_scale: if factor_counted {
            mean(saved.distance_scale, run.point.distance_scale)
        } else {
            saved.distance_scale
        },
        runs: n + 1,
    }
}

fn in_range(v: f32, lo: f32, hi: f32) -> bool {
    v.is_finite() && v >= lo && v <= hi
}

/// Refuses a save that adds or changes a point to a factor outside the
/// limits, whichever window sent it. A point already stored as it is stays,
/// so an old out-of-range point does not block saving anything else.
pub fn check_point_scales(
    new: &[CalibrationPoint],
    stored: &[CalibrationPoint],
) -> Result<(), String> {
    for p in new {
        if !in_range(p.distance_scale, MIN_SCALE, MAX_SCALE) && !stored.contains(p) {
            return Err(format!(
                "Factor {:.2} at {:.1} km/h is outside {MIN_SCALE:.1} to {MAX_SCALE:.1}. Run the calibration again.",
                p.distance_scale, p.speed
            ));
        }
    }
    Ok(())
}

/// The deck's speeds sampled during a run: their mean, or why the run can't
/// be used. A still belt (the treadmill stopped) and a speed that changed are
/// refused.
pub fn summarize_samples(samples: &[f32]) -> Result<f32, String> {
    if samples.is_empty() {
        return Err("No speed was read during the run. Run the speed again.".into());
    }
    let (min, max) = samples
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &s| (lo.min(s), hi.max(s)));
    if min < STOPPED_BELOW_KMH {
        return Err("The belt stopped, so tapping starts again.".into());
    }
    if max - min > MAX_SPREAD_KMH {
        return Err(format!(
            "The speed changed ({min:.1} to {max:.1} km/h), so tapping starts again."
        ));
    }
    Ok(samples.iter().sum::<f32>() / samples.len() as f32)
}

/// The point from a settled tap run: `median_s` between passes of a
/// `belt_cm` loop over `elapsed_s`, while the deck reported `reported_kmh`.
/// `stride_at` gives the current calibration's stride at a real speed, since
/// a belt run times no steps.
pub fn point_from_taps(
    belt_cm: f32,
    median_s: f64,
    elapsed_s: f32,
    reported_kmh: f32,
    stride_at: impl Fn(f32) -> f32,
) -> Result<CalibrationResult, String> {
    if !(median_s.is_finite() && median_s > 0.0) {
        return Err("Tap each time the mark passes.".into());
    }
    if !reported_kmh.is_finite() || reported_kmh < STOPPED_BELOW_KMH {
        return Err("The belt was not moving. Run the speed again.".into());
    }
    let real_kmh = (belt_cm as f64 / 100.0 / median_s * 3.6) as f32;
    let scale = reported_kmh / real_kmh;
    if !in_range(scale, MIN_SCALE, MAX_SCALE) {
        return Err(format!(
            "The taps give a factor of {scale:.2}, outside {MIN_SCALE} to {MAX_SCALE}. Check the belt length and tap each pass of one mark."
        ));
    }
    Ok(CalibrationResult {
        point: CalibrationPoint {
            // The deck reports tenths of km/h.
            speed: (reported_kmh * 10.0).round() / 10.0,
            stride_length_cm: stride_at(real_kmh).clamp(MIN_STRIDE_CM, MAX_STRIDE_CM),
            distance_scale: scale,
            runs: 1,
        },
        real_kmh,
        reported_kmh,
        real_m: real_kmh / 3.6 * elapsed_s,
        reported_m: reported_kmh / 3.6 * elapsed_s,
        cadence_spm: None,
        stride_measured: false,
        averaged: None,
    })
}

/// Where a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum CalibrationPhase {
    /// The belt is on its way to the target, or no tap has come yet.
    Settling,
    /// Taps are coming in; the run finishes by itself once they agree.
    Tapping,
    /// The taps agreed: the result is ready to save.
    Measured,
}

/// One run, kept by Rust from RUN until it is saved or closed.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub target_kmh: f32,
    belt_cm: f32,
    /// When each tap came, since the tapping last started over.
    taps: Vec<Instant>,
    /// The deck's reported speed, read while the user taps.
    samples: Vec<f32>,
    /// Why the tapping started over, or why a tap was not taken.
    note: Option<String>,
    result: Option<CalibrationResult>,
    /// The footstep phase, once offered on a measured run.
    steps: Option<steps::StepRun>,
}

impl Run {
    pub fn new(target_kmh: f32, belt_cm: f32) -> Self {
        Self {
            target_kmh,
            belt_cm,
            taps: vec![],
            samples: vec![],
            note: None,
            result: None,
            steps: None,
        }
    }

    fn phase(&self) -> CalibrationPhase {
        match (self.result.is_some(), self.taps.is_empty()) {
            (true, _) => CalibrationPhase::Measured,
            (false, true) => CalibrationPhase::Settling,
            (false, false) => CalibrationPhase::Tapping,
        }
    }

    /// Still taking taps: of the belt mark, or of footsteps.
    pub fn listening(&self) -> bool {
        self.result.is_none() || self.steps.as_ref().is_some_and(steps::StepRun::listening)
    }

    /// The taps as seconds since the first.
    fn tap_seconds(&self) -> Vec<f64> {
        let Some(first) = self.taps.first() else {
            return vec![];
        };
        self.taps
            .iter()
            .map(|t| t.duration_since(*first).as_secs_f64())
            .collect()
    }

    /// Start the tapping over, saying why.
    fn restart(&mut self, why: impl Into<String>) {
        self.taps.clear();
        self.samples.clear();
        self.result = None;
        self.note = Some(why.into());
    }

    /// TAP AGAIN: clear the taps and any result. The belt keeps going.
    pub fn retap(&mut self) {
        self.steps = None;
        self.taps.clear();
        self.samples.clear();
        self.result = None;
        self.note = None;
    }

    /// One tap at `now`, with the deck reporting `raw_kmh`. Never touches the
    /// belt. `stride_at` gives the current calibration's stride at a real
    /// speed, for the point once the taps agree.
    pub fn tap(&mut self, now: Instant, raw_kmh: f32, stride_at: impl Fn(f32) -> f32) {
        if self.result.is_some() {
            self.note = Some("The run is measured. TAP AGAIN starts it over.".into());
            return;
        }
        if raw_kmh < STOPPED_BELOW_KMH {
            self.restart("The belt is not moving. Press RUN, then tap once it is up to speed.");
            return;
        }
        if let Some(last) = self.taps.last() {
            if now.duration_since(*last).as_secs_f64() < tap::DOUBLE_TAP_S {
                return;
            }
        }
        if self.taps.len() == 1 {
            // The second tap is the first sign the restart's reason is past.
            self.note = None;
        }
        self.taps.push(now);
        self.samples.push(raw_kmh);
        if let Err(why) = summarize_samples(&self.samples) {
            // This tap stands as the first of a fresh start at the new speed.
            self.restart(why);
            self.taps.push(now);
            self.samples.push(raw_kmh);
            return;
        }
        self.try_finish(stride_at);
    }

    /// The deck's reported speed, read each snapshot tick while tapping. A
    /// belt that stopped or changed speed starts the tapping over.
    pub fn sample(&mut self, raw_kmh: f32) {
        if let Some(s) = self.steps.as_mut() {
            s.sample(raw_kmh);
            return;
        }
        if self.taps.is_empty() || self.result.is_some() {
            return;
        }
        self.samples.push(raw_kmh);
        if let Err(why) = summarize_samples(&self.samples) {
            self.restart(why);
        }
    }

    fn try_finish(&mut self, stride_at: impl Fn(f32) -> f32) {
        let secs = self.tap_seconds();
        let stats = tap::analyse(&secs);
        let (true, Some(median_s)) = (stats.settled, stats.median_s) else {
            return;
        };
        let elapsed_s = secs.last().copied().unwrap_or(0.0) as f32;
        let reported = summarize_samples(&self.samples);
        match reported
            .and_then(|kmh| point_from_taps(self.belt_cm, median_s, elapsed_s, kmh, stride_at))
        {
            Ok(r) => {
                log::info!(
                    "Calibration run at {:.1} km/h measured: {:.2} km/h real over {} passes",
                    self.target_kmh,
                    r.real_kmh,
                    self.taps.len()
                );
                self.result = Some(r);
                self.note = None;
            }
            Err(why) => self.restart(why),
        }
    }

    /// Average the result into the saved point at the run's target, if any.
    pub fn average_into(&mut self, points: &[CalibrationPoint], factor_counted: bool) {
        let target = self.target_kmh;
        if let Some(r) = self.result.as_mut() {
            r.averaged = points
                .iter()
                .find(|p| (p.speed - target).abs() <= SAME_SPEED_KMH)
                .map(|saved| averaged_point(saved, r, factor_counted));
        }
    }

    pub fn view(&self, now: Instant) -> CalibrationRunView {
        let secs = self.tap_seconds();
        let stats = tap::analyse(&secs);
        let elapsed_s = match (self.taps.first(), self.result.is_some()) {
            (Some(_), true) => secs.last().copied().unwrap_or(0.0) as f32,
            (Some(first), false) => now.duration_since(*first).as_secs_f32(),
            (None, _) => 0.0,
        };
        CalibrationRunView {
            target_kmh: self.target_kmh,
            phase: self.phase(),
            passes: self.taps.len() as u32,
            kept: stats.kept as u32,
            dropped: stats.dropped as u32,
            elapsed_s,
            measured_kmh: stats
                .median_s
                .map(|m| (self.belt_cm as f64 / 100.0 / m * 3.6) as f32),
            reported_kmh: summarize_samples(&self.samples).ok(),
            note: self.note.clone(),
            result: self.result.clone(),
            steps: self.steps.as_ref().map(|s| s.view(now)),
        }
    }

    /// The belt was stopped: false when the run can't go on and closes. A
    /// footstep phase still taking taps ends, and the belt result stays.
    pub fn stopped(&mut self) -> bool {
        if self.result.is_none() {
            return false;
        }
        if self.listening() {
            self.close_steps();
        }
        true
    }

    // ---- The footstep phase ----

    /// TAP STEPS (or TAP AGAIN on the steps): a fresh footstep phase on the
    /// measured belt. Never touches the belt.
    pub fn start_steps(&mut self) -> Result<(), String> {
        self.close_steps();
        let belt = self
            .result
            .clone()
            .ok_or("Time the belt first, then tap each footstep.")?;
        self.steps = Some(steps::StepRun::new(belt));
        Ok(())
    }

    /// SKIP, or the phase ending: no step phase, and the belt result as the
    /// belt taps gave it, so its stride is the one from before.
    pub fn close_steps(&mut self) {
        if let Some(s) = self.steps.take() {
            self.result = Some(s.belt);
        }
    }

    /// One footstep. Once the steps agree, the belt result still to be saved
    /// carries the measured stride too, so saving it keeps that stride.
    pub fn step_tap(&mut self, now: Instant, raw_kmh: f32, points: &[CalibrationPoint]) {
        let target = self.target_kmh;
        let Some(s) = self.steps.as_mut() else {
            return;
        };
        s.tap(now, raw_kmh, target, points);
        self.carry_stride();
    }

    /// The belt result with the step result's stride and cadence in it, or
    /// as the belt taps gave it when there is none (or it was undone).
    fn carry_stride(&mut self) {
        let Some(s) = self.steps.as_ref() else {
            return;
        };
        let mut belt = s.belt.clone();
        if let (Some(r), false) = (s.result(), s.undone()) {
            belt.point.stride_length_cm = r.point.stride_length_cm;
            belt.cadence_spm = r.cadence_spm;
            belt.stride_measured = true;
            if let (Some(avg), Some(step_avg)) = (belt.averaged.as_mut(), r.averaged.as_ref()) {
                avg.stride_length_cm = step_avg.stride_length_cm;
            }
        }
        self.result = Some(belt);
    }

    /// The window stored the step result's point.
    pub fn steps_saved(&mut self) -> Result<(), String> {
        self.steps
            .as_mut()
            .ok_or("Tap each footstep first.")?
            .mark_saved()
    }

    /// UNDO on the stride: the window puts back the point from before.
    pub fn steps_undo(&mut self) -> Result<(), String> {
        self.steps
            .as_mut()
            .ok_or("No saved stride to undo.")?
            .undo()?;
        self.carry_stride();
        Ok(())
    }
}

/// A run as the windows see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationRunView {
    /// Deck km/h the run asked for.
    pub target_kmh: f32,
    pub phase: CalibrationPhase,
    /// Taps taken since the tapping last started over.
    pub passes: u32,
    /// Times between taps that agree with the median.
    pub kept: u32,
    /// Times between taps dropped as a missed or doubled tap.
    pub dropped: u32,
    /// Seconds since the first tap (to the last tap once measured), from
    /// Rust's clock.
    pub elapsed_s: f32,
    /// The belt's real speed from the taps so far, km/h.
    pub measured_kmh: Option<f32>,
    /// The deck's mean reported speed over the taps so far, km/h.
    pub reported_kmh: Option<f32>,
    /// Why the tapping started over, or why a tap was not taken.
    pub note: Option<String>,
    /// The point, once the taps agree.
    pub result: Option<CalibrationResult>,
    /// The footstep phase, once started on the measured run.
    pub steps: Option<StepRunView>,
}

/// The guided calibration in the snapshot.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationSnapshot {
    /// Labs switch: the guided wizard replaces the old calibration panel.
    pub wizard_enabled: bool,
    pub limits: CalibrationLimits,
    /// The belt loop's length for this treadmill, cm; none until entered.
    pub belt_cm: Option<f32>,
    /// A treadmill is known (connected or saved), so a belt length can be
    /// kept for it.
    pub belt_treadmill: bool,
    /// The target speeds the user keeps, slowest first.
    pub targets: Vec<CalibrationTarget>,
    pub run: Option<CalibrationRunView>,
    /// RUN ALL: the list being walked, if any.
    pub sweep: Option<CalibrationSweepView>,
}

/// One speed in the target list.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationTarget {
    /// Deck km/h, in 0.1 steps.
    pub kmh: f32,
    /// Inside the treadmill's min/max now. A stored speed outside it stays
    /// listed, but RUN and RUN ALL leave it out.
    pub runnable: bool,
}

/// What became of one speed in a sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum SweepMark {
    Pending,
    Saved,
    Skipped,
}

/// RUN ALL as the windows see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationSweepView {
    /// Deck km/h, slowest first.
    pub targets_kmh: Vec<f32>,
    /// One per target.
    pub marks: Vec<SweepMark>,
    /// The target being run or up next; none once every speed is done.
    pub current: Option<usize>,
}

/// RUN ALL: the speeds to walk and what became of each.
#[derive(Debug, Clone, PartialEq)]
pub struct Sweep {
    targets: Vec<f32>,
    marks: Vec<SweepMark>,
}

impl Sweep {
    /// The speeds, sorted and without repeats. Refused when empty. No cap:
    /// the speed range and 0.1 km/h steps already bound the length.
    pub fn new(targets_kmh: &[f32]) -> Result<Self, String> {
        let mut targets: Vec<f32> = targets_kmh
            .iter()
            .copied()
            .filter(|t| t.is_finite())
            .collect();
        targets.sort_by(f32::total_cmp);
        targets.dedup_by(|a, b| (*a - *b).abs() <= SAME_TARGET_KMH);
        if targets.is_empty() {
            return Err("Add a speed to run first.".into());
        }
        Ok(Self {
            marks: vec![SweepMark::Pending; targets.len()],
            targets,
        })
    }

    /// The first speed not yet saved or skipped.
    pub fn current(&self) -> Option<usize> {
        self.marks.iter().position(|m| *m == SweepMark::Pending)
    }

    pub fn current_target(&self) -> Option<f32> {
        self.current().map(|i| self.targets[i])
    }

    /// The current speed is saved or skipped; the next one becomes current.
    pub fn advance(&mut self, saved: bool) -> Result<(), String> {
        let i = self.current().ok_or("Every speed is done.")?;
        self.marks[i] = if saved {
            SweepMark::Saved
        } else {
            SweepMark::Skipped
        };
        Ok(())
    }

    pub fn view(&self) -> CalibrationSweepView {
        CalibrationSweepView {
            targets_kmh: self.targets.clone(),
            marks: self.marks.clone(),
            current: self.current(),
        }
    }
}

/// The physical (calibrated) intent that puts `target_hw_kmh` on the deck
/// with the current calibration. Calibration intents are physical, so this
/// goes through the same lookup the engine uses.
pub(crate) fn intent_for_target(state: &OrchestratorState, target_hw_kmh: f32) -> f32 {
    state.to_physical_speed(target_hw_kmh)
}

/// RUN, his press: set the belt to the target, which starts a still belt.
/// Refused while a program runs (it would take the speed back).
pub(crate) fn begin_run(state: &mut OrchestratorState, target_hw_kmh: f32) -> Result<(), String> {
    check_target(state, target_hw_kmh)?;
    if state.active_program().is_some() {
        return Err("End the program first, then calibrate.".into());
    }
    // A speed set by hand starts a still belt (`note_hand_speed`), so this
    // is the same path as typing the speed in.
    let speed = intent_for_target(state, target_hw_kmh);
    crate::bluetooth::apply_manual_intent(state, Some(speed), None);
    Ok(())
}

/// A target the deck can run.
fn check_target(state: &OrchestratorState, target_hw_kmh: f32) -> Result<(), String> {
    check_range(target_hw_kmh, state.min_speed, state.max_speed)
}

/// `kmh` lies inside the treadmill's `min_kmh..=max_kmh`.
fn check_range(kmh: f32, min_kmh: f32, max_kmh: f32) -> Result<(), String> {
    if !kmh.is_finite() || kmh < min_kmh.max(0.1) {
        return Err(format!("Pick a speed of at least {min_kmh:.1} km/h."));
    }
    if kmh > max_kmh {
        return Err(format!("The treadmill tops out at {max_kmh:.1} km/h."));
    }
    Ok(())
}

/// `kmh` rounded to 0.1 km/h.
fn tenths(kmh: f32) -> f32 {
    (kmh * 10.0).round() / 10.0
}

/// A list made tidy: finite speeds above 0, in 0.1 steps, sorted, one per
/// speed. Speeds outside the range stay (shown, not run).
fn tidy_targets(speeds: &[f32]) -> Vec<f32> {
    let mut out: Vec<f32> = speeds
        .iter()
        .copied()
        .filter(|s| s.is_finite() && *s > 0.0)
        .map(tenths)
        .collect();
    out.sort_by(f32::total_cmp);
    out.dedup_by(|a, b| (*a - *b).abs() <= SAME_TARGET_KMH);
    out
}

/// The stored setting as a list; missing or damaged gives the defaults.
pub(crate) fn decode_targets(raw: Option<&str>) -> Vec<f32> {
    raw.and_then(|r| serde_json::from_str::<Vec<f32>>(r).ok())
        .map(|v| tidy_targets(&v))
        .unwrap_or_else(|| DEFAULT_TARGETS_KMH.to_vec())
}

pub(crate) fn encode_targets(targets: &[f32]) -> String {
    serde_json::to_string(targets).unwrap_or_else(|_| "[]".into())
}

/// ADD_SPEED: the list with `kmh` in it, or why it can't join.
pub(crate) fn with_target_added(
    targets: &[f32],
    kmh: f32,
    min_kmh: f32,
    max_kmh: f32,
) -> Result<Vec<f32>, String> {
    if !kmh.is_finite() {
        return Err("Enter a speed in km/h.".into());
    }
    if (kmh * 10.0 - (kmh * 10.0).round()).abs() > 1e-3 {
        return Err("Pick a speed in 0.1 km/h steps.".into());
    }
    check_range(kmh, min_kmh, max_kmh)?;
    let kmh = tenths(kmh);
    if targets.iter().any(|t| (t - kmh).abs() <= SAME_TARGET_KMH) {
        return Err(format!("{kmh:.1} km/h is already in the list."));
    }
    let mut next = targets.to_vec();
    next.push(kmh);
    Ok(tidy_targets(&next))
}

/// Remove: the list without `kmh`. A speed with a saved point, or the open
/// run's speed, keeps its row.
pub(crate) fn with_target_removed(
    targets: &[f32],
    kmh: f32,
    point_speeds: &[f32],
    run_kmh: Option<f32>,
) -> Result<Vec<f32>, String> {
    let same = |t: f32| (t - kmh).abs() <= SAME_TARGET_KMH;
    if !targets.iter().any(|t| same(*t)) {
        return Err(format!("{kmh:.1} km/h is not in the list."));
    }
    if point_speeds.iter().any(|p| same(*p)) {
        return Err(format!("Delete the saved point at {kmh:.1} km/h first."));
    }
    if run_kmh.is_some_and(same) {
        return Err(format!("Close the run at {kmh:.1} km/h first."));
    }
    Ok(targets.iter().copied().filter(|t| !same(*t)).collect())
}

/// RUN ALL's speeds: the list and every saved point's speed, the ones the
/// treadmill can run now.
pub(crate) fn sweep_targets(
    targets: &[f32],
    point_speeds: &[f32],
    min_kmh: f32,
    max_kmh: f32,
) -> Vec<f32> {
    let all: Vec<f32> = targets.iter().chain(point_speeds).copied().collect();
    tidy_targets(&all)
        .into_iter()
        .filter(|t| check_range(*t, min_kmh, max_kmh).is_ok())
        .collect()
}

fn with_targets<T>(f: impl FnOnce(&mut Option<Vec<f32>>) -> T) -> T {
    f(&mut TARGETS.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The target list in use: the stored one, or the defaults.
pub fn targets() -> Vec<f32> {
    with_targets(|t| t.clone().unwrap_or_else(|| DEFAULT_TARGETS_KMH.to_vec()))
}

/// Change the list and store it. The lock is held through the write so two
/// edits can't lose one another.
fn edit_targets(
    app: &AppHandle,
    edit: impl FnOnce(&[f32]) -> Result<Vec<f32>, String>,
) -> Result<(), String> {
    with_targets(|stored| {
        let current = stored
            .clone()
            .unwrap_or_else(|| DEFAULT_TARGETS_KMH.to_vec());
        let next = edit(&current)?;
        crate::persistence::write_setting(app, KEY_TARGETS, &encode_targets(&next))?;
        *stored = Some(next);
        Ok(())
    })
}

/// ADD_SPEED, checked against the treadmill's `min_kmh..=max_kmh`.
pub fn add_target(app: &AppHandle, kmh: f32, min_kmh: f32, max_kmh: f32) -> Result<(), String> {
    require_wizard()?;
    edit_targets(app, |t| with_target_added(t, kmh, min_kmh, max_kmh))?;
    log::info!("Calibration target {kmh:.1} km/h added");
    Ok(())
}

/// Remove one speed; `point_speeds` are the saved points' speeds.
pub fn remove_target(app: &AppHandle, kmh: f32, point_speeds: &[f32]) -> Result<(), String> {
    require_wizard()?;
    let run_kmh = with_run(|r| r.as_ref().map(|run| run.target_kmh));
    edit_targets(app, |t| with_target_removed(t, kmh, point_speeds, run_kmh))?;
    log::info!("Calibration target {kmh:.1} km/h removed");
    Ok(())
}

fn with_run<T>(f: impl FnOnce(&mut Option<Run>) -> T) -> T {
    let mut run = RUN.lock().unwrap_or_else(|e| e.into_inner());
    let out = f(&mut run);
    TAP_LISTENING.store(run.as_ref().is_some_and(Run::listening), Ordering::Relaxed);
    out
}

/// A run is open and taking taps, so Alt+Space is a tap (read by the input
/// hook on every key press, so an atomic and no lock).
pub fn tap_listening() -> bool {
    TAP_LISTENING.load(Ordering::Relaxed)
}

/// The settings key the belt loop length of the treadmill with this device
/// id (as DECK_PROFILE normalizes it) is stored under, in cm.
pub fn belt_key_for(device_id: &str) -> String {
    format!("beltLoop:{device_id}")
}

/// The belt loop of the treadmill in use: its settings key and the length.
struct Belt {
    key: Option<String>,
    cm: Option<f32>,
}

static BELT: Mutex<Belt> = Mutex::new(Belt {
    key: None,
    cm: None,
});

fn with_belt<T>(f: impl FnOnce(&mut Belt) -> T) -> T {
    f(&mut BELT.lock().unwrap_or_else(|e| e.into_inner()))
}

/// A stored belt length, if it is a number inside the limits.
fn decode_belt(raw: Option<&str>) -> Option<f32> {
    raw.and_then(|r| r.trim().parse::<f32>().ok())
        .filter(|cm| in_range(*cm, MIN_BELT_CM, MAX_BELT_CM))
}

/// Make the belt length the one stored for this treadmill. Called with
/// DECK_PROFILE's switch (startup with the saved treadmill, each connection).
pub fn switch_belt(app: &AppHandle, device_id: &str) {
    let key = belt_key_for(device_id);
    let cm = decode_belt(crate::persistence::read_setting(app, &key).as_deref());
    with_belt(|b| {
        b.key = Some(key);
        b.cm = cm;
    });
}

/// BELT_LOOP: the length of this treadmill's belt loop, kept for it.
pub fn set_belt(app: &AppHandle, cm: f32) -> Result<(), String> {
    if !in_range(cm, MIN_BELT_CM, MAX_BELT_CM) {
        return Err(format!(
            "Belt length must be {MIN_BELT_CM:.0} to {MAX_BELT_CM:.0} cm."
        ));
    }
    let key = with_belt(|b| b.key.clone())
        .ok_or("Connect the treadmill first, so the belt length is kept for it.")?;
    crate::persistence::write_setting(app, &key, &format!("{cm}"))?;
    with_belt(|b| b.cm = Some(cm));
    log::info!("Belt loop set to {cm:.0} cm");
    Ok(())
}

/// The belt length in use, cm.
pub fn belt_cm() -> Option<f32> {
    with_belt(|b| b.cm)
}

/// Tests: a treadmill with this belt length, without a database.
#[cfg(test)]
pub(crate) fn set_belt_for_test(cm: Option<f32>) {
    with_belt(|b| {
        b.key = Some(belt_key_for("TEST"));
        b.cm = cm;
    });
}

/// Read the Labs switch. Call once after the database is initialised.
pub fn init(app: &AppHandle) {
    let on = crate::persistence::parse_setting(
        crate::persistence::read_setting(app, KEY_WIZARD).as_deref(),
    )
    .unwrap_or(false);
    WIZARD_ON.store(on, Ordering::Relaxed);
    let raw = crate::persistence::read_setting(app, KEY_TARGETS);
    with_targets(|t| *t = raw.map(|r| decode_targets(Some(&r))));
}

/// Tests: the Labs switch without a database.
#[cfg(test)]
pub(crate) fn set_wizard_for_test(on: bool) {
    WIZARD_ON.store(on, Ordering::Relaxed);
}

pub fn wizard_enabled() -> bool {
    WIZARD_ON.load(Ordering::Relaxed)
}

/// The Labs switch (System -> Diagnostics). Off also drops any run.
pub fn set_wizard(app: &AppHandle, on: bool) -> Result<(), String> {
    crate::persistence::write_setting(app, KEY_WIZARD, if on { "true" } else { "false" })?;
    WIZARD_ON.store(on, Ordering::Relaxed);
    if !on {
        close_all();
    }
    log::info!("Guided calibration {}", if on { "on" } else { "off" });
    Ok(())
}

fn require_wizard() -> Result<(), String> {
    if wizard_enabled() {
        Ok(())
    } else {
        Err("Turn on the guided calibration in System, Diagnostics first.".into())
    }
}

fn with_sweep<T>(f: impl FnOnce(&mut Option<Sweep>) -> T) -> T {
    f(&mut SWEEP.lock().unwrap_or_else(|e| e.into_inner()))
}

fn open_run(state: &mut OrchestratorState, target_hw_kmh: f32) -> Result<(), String> {
    let belt_cm = belt_cm().ok_or("Enter the belt loop length first.")?;
    begin_run(state, target_hw_kmh)?;
    log::info!("Calibration run at {target_hw_kmh:.1} km/h");
    with_run(|r| *r = Some(Run::new(target_hw_kmh, belt_cm)));
    Ok(())
}

/// RUN: a new run at `target_hw_kmh` (also "repeat this point"). A single
/// run leaves any sweep.
pub(crate) fn run_start(state: &mut OrchestratorState, target_hw_kmh: f32) -> Result<(), String> {
    require_wizard()?;
    open_run(state, target_hw_kmh)?;
    with_sweep(|s| *s = None);
    Ok(())
}

/// RUN ALL, his press: a sweep over the stored list and the saved points'
/// speeds, running the slowest now.
pub(crate) fn sweep_start(state: &mut OrchestratorState) -> Result<(), String> {
    let points: Vec<f32> = state.calibration_points.iter().map(|p| p.speed).collect();
    let speeds = sweep_targets(&targets(), &points, state.min_speed, state.max_speed);
    sweep_start_over(state, &speeds)
}

/// RUN ALL over `targets_kmh`, slowest first.
pub(crate) fn sweep_start_over(
    state: &mut OrchestratorState,
    targets_kmh: &[f32],
) -> Result<(), String> {
    require_wizard()?;
    let sweep = Sweep::new(targets_kmh)?;
    for &t in &sweep.targets {
        check_target(state, t)?;
    }
    open_run(state, sweep.targets[0])?;
    log::info!("Calibration sweep over {} speeds", sweep.targets.len());
    with_sweep(|s| *s = Some(sweep));
    Ok(())
}

/// RUN on the sweep's current speed, his press.
pub(crate) fn sweep_run(state: &mut OrchestratorState) -> Result<(), String> {
    require_wizard()?;
    let target = with_sweep(|s| s.as_ref().map(Sweep::current_target))
        .ok_or("Press RUN ALL first.")?
        .ok_or("Every speed is done.")?;
    open_run(state, target)
}

/// SAVE or SKIP in a sweep: mark the current speed and close its run. The
/// belt keeps its speed; the next one waits for RUN.
pub(crate) fn sweep_next(saved: bool) -> Result<(), String> {
    require_wizard()?;
    with_sweep(|s| s.as_mut().ok_or("Press RUN ALL first.")?.advance(saved))?;
    with_run(|r| *r = None);
    Ok(())
}

/// End the sweep and any run. The belt keeps going.
pub(crate) fn close_all() {
    with_run(|r| *r = None);
    with_sweep(|s| *s = None);
}

/// STOP was pressed: a run still settling or tapping can't go on, so it
/// closes (a sweep stays on its speed for the next RUN). A measured run
/// waiting to be saved stays open; a footstep phase still taking taps ends,
/// and the belt result stays.
pub(crate) fn on_stop() {
    with_run(|r| {
        if r.as_mut().is_some_and(|run| !run.stopped()) {
            *r = None;
        }
    });
}

fn on_run<T>(f: impl FnOnce(&mut Run) -> Result<T, String>) -> Result<T, String> {
    require_wizard()?;
    with_run(|r| match r.as_mut() {
        Some(run) => f(run),
        None => Err("Press RUN on a speed first.".into()),
    })
}

/// TAP: the belt mark passed at `at` (taken as the press arrived, before any
/// lock). Only reads the deck's speed; never sends anything to the belt.
pub(crate) fn tap(state: &OrchestratorState, at: Instant) -> Result<(), String> {
    on_run(|run| {
        if run.steps.is_some() {
            // A footstep: the same inputs, timed against the measured belt.
            run.step_tap(at, state.hardware_speed, &state.calibration_points);
            return Ok(());
        }
        run.tap(at, state.hardware_speed, |kmh| {
            state.calculate_stride_length(kmh)
        });
        // A belt run times the belt alone: the factor is measured, the stride kept.
        run.average_into(&state.calibration_points, true);
        Ok(())
    })
}

/// TAP AGAIN: clear the run's taps and result. The belt keeps going.
pub(crate) fn retap() -> Result<(), String> {
    on_run(|run| {
        run.retap();
        Ok(())
    })
}

/// TAP STEPS: start the footstep phase on the measured run. Never moves the belt.
pub(crate) fn steps_start() -> Result<(), String> {
    on_run(Run::start_steps)
}

/// SKIP on the footsteps: the stride stays as it was.
pub(crate) fn steps_skip() -> Result<(), String> {
    on_run(|run| {
        run.close_steps();
        Ok(())
    })
}

/// The window stored the footstep result's point.
pub(crate) fn steps_saved() -> Result<(), String> {
    on_run(Run::steps_saved)
}

/// UNDO on the stride.
pub(crate) fn steps_undo() -> Result<(), String> {
    on_run(Run::steps_undo)
}

/// Close the run (saved or cancelled). The belt keeps going.
pub(crate) fn close() {
    with_run(|r| *r = None);
}

/// Each snapshot tick: sample the deck's speed into the taps and build the view.
pub fn snapshot(state: &OrchestratorState) -> CalibrationSnapshot {
    let raw_kmh = state.hardware_speed;
    let targets = targets()
        .into_iter()
        .map(|kmh| CalibrationTarget {
            kmh,
            runnable: check_range(kmh, state.min_speed, state.max_speed).is_ok(),
        })
        .collect();
    let run = with_run(|r| {
        r.as_mut().map(|run| {
            run.sample(raw_kmh);
            run.view(Instant::now())
        })
    });
    CalibrationSnapshot {
        wizard_enabled: wizard_enabled(),
        limits: CalibrationLimits::default(),
        belt_cm: belt_cm(),
        belt_treadmill: with_belt(|b| b.key.is_some()),
        targets,
        run,
        sweep: with_sweep(|s| s.as_ref().map(Sweep::view)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn pt(speed: f32, scale: f32) -> CalibrationPoint {
        CalibrationPoint {
            speed,
            stride_length_cm: 70.0,
            distance_scale: scale,
            runs: 1,
        }
    }

    #[test]
    fn a_save_with_a_factor_outside_the_limits_is_refused() {
        let err = check_point_scales(&[pt(3.0, 0.9), pt(4.0, 0.15)], &[]).unwrap_err();
        assert_eq!(
            err,
            "Factor 0.15 at 4.0 km/h is outside 0.2 to 3.0. Run the calibration again."
        );
        assert!(check_point_scales(&[pt(3.0, 3.5)], &[]).is_err());
        assert!(check_point_scales(&[pt(3.0, f32::NAN)], &[]).is_err());
        assert!(check_point_scales(&[pt(3.0, 0.2), pt(4.0, 3.0)], &[]).is_ok());
    }

    #[test]
    fn a_point_already_stored_out_of_range_does_not_block_a_save() {
        let old = pt(3.0, 0.1);
        assert!(
            check_point_scales(&[old.clone(), pt(5.0, 1.0)], std::slice::from_ref(&old)).is_ok()
        );
        // Changing that point to another bad factor is refused.
        assert!(check_point_scales(&[pt(3.0, 0.12)], &[old]).is_err());
    }

    const NO_STRIDE: fn(f32) -> f32 = |_| 74.0;

    /// A run at 3.0 km/h on a 300 cm belt.
    fn run_at_3() -> Run {
        Run::new(3.0, 300.0)
    }

    /// Tap `run` every `gap_s` s, `n` times from `t`, with the deck at `kmh`.
    fn tap_n(run: &mut Run, t: Instant, gap_s: f64, n: usize, kmh: f32) -> Instant {
        let mut at = t;
        for i in 0..n {
            at = t + Duration::from_secs_f64(i as f64 * gap_s);
            run.tap(at, kmh, NO_STRIDE);
        }
        at
    }

    #[test]
    fn taps_give_the_real_speed_and_the_factor() {
        // 300 cm every 4 s is 0.75 m/s, 2.7 km/h; the deck says 3.0.
        let r = point_from_taps(300.0, 4.0, 36.0, 3.0, NO_STRIDE).unwrap();
        assert!((r.real_kmh - 2.7).abs() < 1e-4);
        assert!((r.point.distance_scale - 3.0 / 2.7).abs() < 1e-4);
        assert_eq!(r.point.speed, 3.0);
        assert!((r.real_m - 27.0).abs() < 1e-3);
        assert!((r.reported_m - 30.0).abs() < 1e-3);
        assert_eq!(r.point.stride_length_cm, 74.0);
        assert!(!r.stride_measured);
        assert_eq!(r.cadence_spm, None);
        // The point's speed is the reported speed in tenths.
        let r = point_from_taps(300.0, 4.0, 36.0, 2.96, NO_STRIDE).unwrap();
        assert_eq!(r.point.speed, 3.0);
    }

    #[test]
    fn taps_that_give_a_factor_outside_the_limits_are_refused() {
        // 300 cm every 60 s is 0.18 km/h against 3.0 reported: factor 16.7.
        let e = point_from_taps(300.0, 60.0, 600.0, 3.0, NO_STRIDE).unwrap_err();
        assert!(e.contains("belt length"), "{e}");
        assert!(point_from_taps(300.0, 4.0, 36.0, 0.0, NO_STRIDE).is_err());
        assert!(point_from_taps(300.0, f64::NAN, 36.0, 3.0, NO_STRIDE).is_err());
        // A factor of 0.31, as seen on a real deck (issue #62), is fine.
        let real_kmh: f64 = 3.1 / 0.31;
        let gap = 3.0 / (real_kmh / 3.6);
        let r = point_from_taps(300.0, gap, 30.0, 3.1, NO_STRIDE).unwrap();
        assert!((r.point.distance_scale - 0.31).abs() < 1e-3);
    }

    #[test]
    fn samples_refuse_a_belt_that_stopped_or_changed_speed() {
        assert_eq!(summarize_samples(&[3.0, 3.0, 3.0]), Ok(3.0));
        assert!((summarize_samples(&[3.0, 3.1]).unwrap() - 3.05).abs() < 1e-5);
        assert!(summarize_samples(&[]).is_err());
        let stopped = summarize_samples(&[3.0, 2.0, 0.0]).unwrap_err();
        assert!(stopped.contains("stopped"), "{stopped}");
        let changed = summarize_samples(&[3.0, 3.5]).unwrap_err();
        assert!(changed.contains("changed"), "{changed}");
    }

    #[test]
    fn a_run_finishes_by_itself_once_the_taps_agree() {
        let t = Instant::now();
        let mut run = run_at_3();
        assert_eq!(run.view(t).phase, CalibrationPhase::Settling);
        assert!(run.listening());
        // Eight taps are seven gaps: not yet.
        tap_n(&mut run, t, 4.0, 8, 3.0);
        let v = run.view(t + Duration::from_secs(30));
        assert_eq!(v.phase, CalibrationPhase::Tapping);
        assert_eq!((v.passes, v.kept, v.dropped), (8, 7, 0));
        assert!((v.measured_kmh.unwrap() - 2.7).abs() < 1e-3, "live speed");
        assert_eq!(v.reported_kmh, Some(3.0));
        assert_eq!(v.elapsed_s, 30.0, "from the first tap, Rust's clock");
        // The ninth settles it.
        run.tap(t + Duration::from_secs(32), 3.0, NO_STRIDE);
        let v = run.view(t + Duration::from_secs(99));
        assert_eq!(v.phase, CalibrationPhase::Measured);
        assert_eq!(v.elapsed_s, 32.0, "the clock stops at the last tap");
        let r = v.result.unwrap();
        assert!((r.real_kmh - 2.7).abs() < 1e-3);
        assert!(!run.listening());
        // More taps change nothing and say why.
        run.tap(t + Duration::from_secs(36), 3.0, NO_STRIDE);
        let v = run.view(t);
        assert_eq!(v.passes, 9);
        assert!(v.note.unwrap().contains("TAP AGAIN"));
        // TAP AGAIN clears it.
        run.retap();
        let v = run.view(t);
        assert_eq!(
            (v.phase, v.passes, v.result),
            (CalibrationPhase::Settling, 0, None)
        );
    }

    #[test]
    fn a_missed_pass_is_dropped_and_the_run_still_finishes() {
        let t = Instant::now();
        let mut run = run_at_3();
        // Passes every 4 s; the one at 16 s was never tapped.
        for i in (0..=10).filter(|i| *i != 4) {
            run.tap(t + Duration::from_secs(i * 4), 3.0, NO_STRIDE);
        }
        let v = run.view(t);
        assert_eq!((v.kept, v.dropped), (8, 1));
        assert!((v.result.unwrap().real_kmh - 2.7).abs() < 1e-3);
    }

    #[test]
    fn a_bounced_tap_is_one_tap() {
        let t = Instant::now();
        let mut run = run_at_3();
        run.tap(t, 3.0, NO_STRIDE);
        run.tap(t + Duration::from_millis(120), 3.0, NO_STRIDE);
        assert_eq!(run.view(t).passes, 1);
        run.tap(t + Duration::from_secs(4), 3.0, NO_STRIDE);
        assert_eq!(run.view(t).passes, 2);
    }

    #[test]
    fn a_still_belt_takes_no_taps() {
        let t = Instant::now();
        let mut run = run_at_3();
        run.tap(t, 0.0, NO_STRIDE);
        let v = run.view(t);
        assert_eq!(v.passes, 0);
        assert!(v.note.unwrap().contains("not moving"));
    }

    #[test]
    fn a_belt_that_stops_or_changes_speed_starts_the_tapping_over() {
        let t = Instant::now();
        let mut run = run_at_3();
        tap_n(&mut run, t, 4.0, 5, 3.0);
        run.sample(3.0);
        run.sample(0.0);
        let v = run.view(t);
        assert_eq!((v.phase, v.passes), (CalibrationPhase::Settling, 0));
        assert!(v.note.unwrap().contains("stopped"));

        // A tap at a new speed starts over from that tap.
        tap_n(&mut run, t, 4.0, 5, 3.0);
        run.tap(t + Duration::from_secs(20), 3.5, NO_STRIDE);
        let v = run.view(t);
        assert_eq!(v.passes, 1);
        assert!(v.note.as_deref().unwrap().contains("changed"));
        assert_eq!(v.reported_kmh, Some(3.5));
        // The next tap clears the note.
        run.tap(t + Duration::from_secs(24), 3.5, NO_STRIDE);
        assert_eq!(run.view(t).note, None);
    }

    #[test]
    fn samples_only_count_while_tapping() {
        let t = Instant::now();
        let mut run = run_at_3();
        run.sample(0.0); // the belt ramping up before the first tap
        tap_n(&mut run, t, 4.0, 9, 3.0);
        run.sample(0.0); // the user stopped the belt after the run was measured
        let v = run.view(t);
        assert_eq!(v.phase, CalibrationPhase::Measured);
        assert_eq!(v.reported_kmh, Some(3.0));
    }

    #[test]
    fn a_stored_belt_length_must_be_a_number_inside_the_limits() {
        assert_eq!(decode_belt(Some("280")), Some(280.0));
        assert_eq!(decode_belt(Some(" 312.5 ")), Some(312.5));
        assert_eq!(decode_belt(Some("10")), None);
        assert_eq!(decode_belt(Some("long")), None);
        assert_eq!(decode_belt(None), None);
        assert_ne!(belt_key_for("AABB"), belt_key_for("CCDD"));
        assert!(belt_key_for("AABB").ends_with("AABB"));
    }

    #[test]
    fn a_sweep_walks_its_speeds_slowest_first_and_marks_each() {
        let mut s = Sweep::new(&[4.0, 2.0, 3.0, 3.02, f32::NAN]).unwrap();
        assert_eq!(s.targets, vec![2.0, 3.0, 4.0], "sorted, one per speed");
        assert_eq!(s.current_target(), Some(2.0));
        s.advance(true).unwrap();
        s.advance(false).unwrap();
        let v = s.view();
        assert_eq!(v.current, Some(2));
        assert_eq!(
            v.marks,
            vec![SweepMark::Saved, SweepMark::Skipped, SweepMark::Pending]
        );
        s.advance(true).unwrap();
        assert_eq!(s.view().current, None, "every speed done");
        assert!(s.advance(true).is_err());
        assert!(Sweep::new(&[]).is_err());
        // No cap: every 0.1 step from 0.6 to 12.0 km/h is one sweep.
        let every: Vec<f32> = (6..=120).map(|i| i as f32 / 10.0).collect();
        assert_eq!(Sweep::new(&every).unwrap().targets.len(), 115);
    }

    #[test]
    fn a_target_joins_in_tenths_inside_the_range_and_only_once() {
        let list = [2.0, 4.0];
        assert_eq!(
            with_target_added(&list, 3.5, 0.6, 12.0).unwrap(),
            vec![2.0, 3.5, 4.0]
        );
        assert_eq!(
            with_target_added(&list, 0.6, 0.6, 12.0).unwrap(),
            vec![0.6, 2.0, 4.0],
            "the floor itself is fine"
        );
        let err = |kmh| with_target_added(&list, kmh, 0.6, 12.0).unwrap_err();
        assert!(err(f32::NAN).contains("Enter"));
        assert!(err(3.55).contains("0.1 km/h steps"));
        assert!(err(0.5).contains("at least 0.6"));
        assert!(err(12.1).contains("tops out at 12.0"));
        assert!(err(4.0).contains("already"));
        assert!(err(4.04).contains("0.1 km/h steps"));
        // The treadmill's own limits, not fixed ones.
        assert!(with_target_added(&list, 14.0, 1.0, 16.0).is_ok());
        assert!(with_target_added(&list, 0.8, 1.0, 16.0).is_err());
    }

    #[test]
    fn a_target_with_a_point_or_the_open_run_keeps_its_row() {
        let list = [2.0, 3.0, 4.0];
        assert_eq!(
            with_target_removed(&list, 3.0, &[], None).unwrap(),
            vec![2.0, 4.0]
        );
        assert!(with_target_removed(&list, 3.0, &[3.0], None)
            .unwrap_err()
            .contains("saved point"));
        assert!(with_target_removed(&list, 3.0, &[], Some(3.0))
            .unwrap_err()
            .contains("Close the run"));
        assert!(with_target_removed(&list, 5.0, &[], None)
            .unwrap_err()
            .contains("not in the list"));
        assert_eq!(
            with_target_removed(&[3.0], 3.0, &[], Some(4.0)).unwrap(),
            Vec::<f32>::new(),
            "the list may be empty"
        );
    }

    #[test]
    fn the_stored_list_round_trips_and_a_damaged_one_gives_the_defaults() {
        let list = vec![0.6, 2.5, 14.0];
        assert_eq!(decode_targets(Some(&encode_targets(&list))), list);
        assert_eq!(decode_targets(Some("[]")), Vec::<f32>::new());
        assert_eq!(
            decode_targets(Some("[5.04, 2, 2.0, -1, 3]")),
            vec![2.0, 3.0, 5.0]
        );
        assert_eq!(decode_targets(None), DEFAULT_TARGETS_KMH.to_vec());
        assert_eq!(
            decode_targets(Some("not json")),
            DEFAULT_TARGETS_KMH.to_vec()
        );
        assert_eq!(
            decode_targets(Some(r#"{"a":1}"#)),
            DEFAULT_TARGETS_KMH.to_vec()
        );
    }

    #[test]
    fn run_all_walks_the_list_and_the_points_the_treadmill_can_run() {
        assert_eq!(
            sweep_targets(&[2.0, 14.0, 3.0], &[2.5, 3.0], 0.6, 12.0),
            vec![2.0, 2.5, 3.0],
            "14.0 is listed but out of range"
        );
        assert_eq!(sweep_targets(&[14.0], &[], 0.6, 12.0), Vec::<f32>::new());
    }

    #[test]
    fn the_run_target_reaches_the_deck_through_the_calibration() {
        let mut state = OrchestratorState::default();
        state.sync_config(&crate::persistence::AppConfig {
            calibration_points: vec![CalibrationPoint {
                speed: 3.0,
                stride_length_cm: 70.0,
                distance_scale: 0.8,
                runs: 1,
            }],
            ..Default::default()
        });
        let phys = intent_for_target(&state, 3.0);
        let (byte, _) =
            crate::engine::calculate_hardware_bytes(phys, 0, &state.calibration_points, 12.0);
        assert_eq!(byte, 30);
    }

    #[test]
    fn a_new_run_is_averaged_into_the_saved_point() {
        // Two runs saved at 0.9 and 70 cm; a third at 1.2 and 76 cm.
        let saved = CalibrationPoint {
            runs: 2,
            ..pt(3.0, 0.9)
        };
        let mut r = point_from_taps(300.0, 4.0, 36.0, 3.0, NO_STRIDE).unwrap();
        r.point.distance_scale = 1.2;
        r.point.stride_length_cm = 76.0;
        r.stride_measured = true;
        let p = averaged_point(&saved, &r, true);
        assert_eq!(p.runs, 3);
        assert_eq!(p.speed, 3.0);
        assert!((p.distance_scale - 1.0).abs() < 1e-5);
        assert!((p.stride_length_cm - 72.0).abs() < 1e-4);
        // A belt run (no steps) keeps the saved stride; an old point is one run.
        r.stride_measured = false;
        let old = CalibrationPoint {
            runs: 0,
            ..pt(3.0, 0.9)
        };
        let p = averaged_point(&old, &r, true);
        assert_eq!(p.runs, 2);
        assert_eq!(p.stride_length_cm, 70.0);
        assert!((p.distance_scale - 1.05).abs() < 1e-5);
        // A stride-only run: the stride averages, the saved factor stays.
        r.stride_measured = true;
        let p = averaged_point(&saved, &r, false);
        assert_eq!(p.runs, 3);
        assert!((p.stride_length_cm - 72.0).abs() < 1e-4);
        assert_eq!(p.distance_scale, 0.9);
    }

    /// A measured run at 3.0 km/h (2.7 real) with 21 footsteps 0.6 s apart.
    fn stepped(points: &[CalibrationPoint]) -> (Run, Instant) {
        let t = Instant::now();
        let mut run = run_at_3();
        tap_n(&mut run, t, 4.0, 9, 3.0);
        run.average_into(points, true);
        run.start_steps().unwrap();
        for i in 0..21 {
            run.step_tap(
                t + Duration::from_secs_f64(40.0 + i as f64 * 0.6),
                3.0,
                points,
            );
        }
        (run, t)
    }

    #[test]
    fn footsteps_need_a_measured_belt_and_listen_like_taps() {
        let mut run = run_at_3();
        assert!(run.start_steps().unwrap_err().contains("Time the belt"));
        let t = Instant::now();
        tap_n(&mut run, t, 4.0, 9, 3.0);
        assert!(!run.listening());
        run.start_steps().unwrap();
        assert!(run.listening(), "Alt+Space and Space tap footsteps now");
        let v = run.view(t).steps.unwrap();
        assert_eq!((v.phase, v.steps), (CalibrationPhase::Settling, 0));
    }

    #[test]
    fn a_stride_saves_with_the_factor_kept_and_the_belt_result_carries_it() {
        let saved = CalibrationPoint {
            runs: 2,
            ..pt(3.0, 0.9)
        };
        let (run, t) = stepped(std::slice::from_ref(&saved));
        let v = run.view(t);
        let steps = v.steps.unwrap();
        // 0.75 m/s over a step every 0.6 s is 45 cm, 100 steps a minute.
        let r = steps.result.unwrap();
        assert!(r.stride_measured);
        assert_eq!(r.cadence_spm.map(f32::round), Some(100.0));
        let p = steps.to_save.unwrap();
        assert_eq!(p.distance_scale, 0.9, "the saved factor stays");
        assert!((p.stride_length_cm - (70.0 * 2.0 + 45.0) / 3.0).abs() < 1e-3);
        assert_eq!(steps.before, Some(saved));
        // The belt result still to be saved keeps the measured stride.
        let belt = v.result.unwrap();
        assert!(belt.stride_measured);
        assert!((belt.point.stride_length_cm - 45.0).abs() < 1e-3);
        assert_eq!(belt.averaged.unwrap().stride_length_cm, p.stride_length_cm);
    }

    #[test]
    fn skip_or_undo_leaves_the_stride_as_it_was() {
        let (mut run, t) = stepped(&[]);
        run.close_steps();
        let v = run.view(t);
        assert_eq!(v.steps, None);
        let belt = v.result.unwrap();
        assert_eq!(belt.point.stride_length_cm, 74.0, "the borrowed stride");
        assert!(!belt.stride_measured && belt.cadence_spm.is_none());

        let (mut run, t) = stepped(&[]);
        assert!(run.steps_undo().is_err(), "nothing saved yet");
        run.steps_saved().unwrap();
        run.steps_undo().unwrap();
        let v = run.view(t);
        assert!(v.steps.unwrap().undone);
        assert_eq!(v.result.unwrap().point.stride_length_cm, 74.0);
    }

    #[test]
    fn stop_ends_the_footsteps_but_keeps_the_belt_result() {
        let t = Instant::now();
        let mut run = run_at_3();
        tap_n(&mut run, t, 4.0, 5, 3.0);
        assert!(!run.stopped(), "a run still tapping closes");
        let mut run = run_at_3();
        tap_n(&mut run, t, 4.0, 9, 3.0);
        run.start_steps().unwrap();
        assert!(run.stopped(), "a measured run stays open");
        let v = run.view(t);
        assert_eq!(v.steps, None);
        assert!(v.result.is_some());
        assert!(!run.listening());
    }

    #[test]
    fn a_run_averages_into_the_point_at_its_target_only() {
        let t = Instant::now();
        let mut run = run_at_3();
        tap_n(&mut run, t, 4.0, 9, 3.0);
        run.average_into(&[pt(4.0, 0.8)], true);
        assert_eq!(run.view(t).result.unwrap().averaged, None);
        run.average_into(&[pt(4.0, 0.8), pt(3.0, 0.9)], true);
        let avg = run.view(t).result.unwrap().averaged.unwrap();
        assert_eq!((avg.speed, avg.runs), (3.0, 2));
    }
}
