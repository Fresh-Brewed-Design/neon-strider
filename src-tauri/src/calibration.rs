//! GROUND_TRUTH, the guided calibration (always on in System, Calibration):
//! the limits a calibration point must stay inside (shared with the engine
//! and the UI), the tap run Rust keeps while the user walks it, and the point
//! it gives. A treadmill that connects with no calibration gets an offer to
//! calibrate it, which SKIP turns off for that treadmill.
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
//! It is the advanced way in. The guided walk (`guide.rs`) picks the speeds
//! from one answer, the fastest the user walks, and checks the midpoints.

mod guide;
mod steps;
mod tap;

pub use guide::{CalibrationGuideView, CheckVerdict};
pub use steps::StepRunView;

use crate::engine::OrchestratorState;
use crate::protocol::{CalibrationPoint, MachineStatus};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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

/// The edited list of target speeds, a JSON array of deck km/h.
const KEY_TARGETS: &str = "labs_calibration_targets";
static RUN: Mutex<Option<Run>> = Mutex::new(None);
/// A run is open and still taking taps: the input hook reads this to know
/// whether Alt+Space is a tap, without taking a lock.
static TAP_LISTENING: AtomicBool = AtomicBool::new(false);
/// Bumped when Rust changes the saved points itself; see `points_rev`.
static POINTS_REV: AtomicU32 = AtomicU32::new(0);
static SWEEP: Mutex<Option<Sweep>> = Mutex::new(None);
/// The guided walk from the slowest speed to the user's top speed, if open.
static GUIDE: Mutex<Option<guide::Guide>> = Mutex::new(None);
/// Tests that drive the shared run, sweep and walk hold this, so they don't
/// run into one another.
#[cfg(test)]
pub(crate) static TEST_GLOBALS: Mutex<()> = Mutex::new(());
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
        type = "{ speed: number, stride_length_cm: number, distance_scale: number, runs: number, stride_runs: number }"
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
        type = "{ speed: number, stride_length_cm: number, distance_scale: number, runs: number, stride_runs: number } | null"
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
///
/// Strides are counted apart from belt runs (`stride_runs`): a measured stride
/// replaces a stride no footstep run ever measured (a count of 0), and after
/// that it averages only with measured strides.
pub fn averaged_point(
    saved: &CalibrationPoint,
    run: &CalibrationResult,
    factor_counted: bool,
) -> CalibrationPoint {
    let n = saved.runs.max(1);
    let mean = |old: f32, new: f32| (old * n as f32 + new) / (n + 1) as f32;
    let (stride_length_cm, stride_runs) = if run.stride_measured {
        let m = saved.stride_runs;
        let stride =
            (saved.stride_length_cm * m as f32 + run.point.stride_length_cm) / (m + 1) as f32;
        (stride, m + 1)
    } else {
        (saved.stride_length_cm, saved.stride_runs)
    };
    CalibrationPoint {
        speed: saved.speed,
        stride_length_cm,
        distance_scale: if factor_counted {
            mean(saved.distance_scale, run.point.distance_scale)
        } else {
            saved.distance_scale
        },
        runs: n + 1,
        stride_runs,
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
            stride_runs: 0,
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

/// What a run's automatic save did to the saved points, so UNDO can put them
/// back exactly: the point written, and the points it replaced.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedPoint {
    written: CalibrationPoint,
    replaced: Vec<CalibrationPoint>,
}

fn by_speed(points: &mut [CalibrationPoint]) {
    points.sort_by(|a, b| a.speed.total_cmp(&b.speed));
}

/// `points` with `result` saved: its average with the point at the run's
/// speed when there is one, else the new point. Any point at that speed is
/// replaced, and kept in the `SavedPoint` for UNDO.
fn with_result_saved(
    points: &[CalibrationPoint],
    result: &CalibrationResult,
) -> (Vec<CalibrationPoint>, SavedPoint) {
    let written = result
        .averaged
        .clone()
        .unwrap_or_else(|| result.point.clone());
    let (replaced, mut kept): (Vec<_>, Vec<_>) = points
        .iter()
        .cloned()
        .partition(|p| (p.speed - written.speed).abs() <= SAME_SPEED_KMH);
    kept.push(written.clone());
    by_speed(&mut kept);
    (kept, SavedPoint { written, replaced })
}

/// `points` as they were before `saved` was written.
fn with_save_undone(points: &[CalibrationPoint], saved: &SavedPoint) -> Vec<CalibrationPoint> {
    let mut out: Vec<CalibrationPoint> = points
        .iter()
        .filter(|p| (p.speed - saved.written.speed).abs() > SAME_SPEED_KMH)
        .cloned()
        .collect();
    out.extend(saved.replaced.iter().cloned());
    by_speed(&mut out);
    out
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
    /// What the automatic save wrote, until UNDO puts it back.
    saved: Option<SavedPoint>,
    /// UNDO was pressed on this result.
    undone: bool,
}

impl Run {
    /// The deck speed the run measured: the result's point speed once
    /// measured, else the speed RUN asked for. A start at the treadmill or
    /// its remote can put the belt at another speed than the one asked.
    pub fn ran_kmh(&self) -> f32 {
        self.result
            .as_ref()
            .map_or(self.target_kmh, |r| r.point.speed)
    }

    pub fn new(target_kmh: f32, belt_cm: f32) -> Self {
        Self {
            target_kmh,
            belt_cm,
            taps: vec![],
            samples: vec![],
            note: None,
            result: None,
            steps: None,
            saved: None,
            undone: false,
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
        self.saved = None;
        self.undone = false;
        self.note = None;
    }

    /// One tap at `now`, with the deck reporting `raw_kmh`. Never touches the
    /// belt. `stride_at` gives the current calibration's stride at a real
    /// speed, for the point once the taps agree.
    pub fn tap(&mut self, now: Instant, raw_kmh: f32, stride_at: impl Fn(f32) -> f32) {
        if self.result.is_some() {
            self.note = Some("The run is measured. TAP AGAIN starts the run over.".into());
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
    /// Once the result is saved the points already hold it, so it stays.
    pub fn average_into(&mut self, points: &[CalibrationPoint], factor_counted: bool) {
        if self.saved.is_some() {
            return;
        }
        let target = self.ran_kmh();
        if let Some(r) = self.result.as_mut() {
            r.averaged = points
                .iter()
                .find(|p| (p.speed - target).abs() <= SAME_SPEED_KMH)
                .map(|saved| averaged_point(saved, r, factor_counted));
        }
    }

    /// Save the measured result into `state`'s points, by itself. Gives the
    /// points to keep, or none when there is no new result to save.
    pub fn autosave(&mut self, state: &mut OrchestratorState) -> Option<Vec<CalibrationPoint>> {
        if self.saved.is_some() || self.undone {
            return None;
        }
        let result = self.result.as_ref()?;
        let (points, saved) = with_result_saved(&state.calibration_points, result);
        Self::keep_points(self.ran_kmh(), state, points.clone());
        log::info!(
            "Calibration point at {:.1} km/h saved ({} runs)",
            saved.written.speed,
            saved.written.runs
        );
        self.saved = Some(saved);
        Some(points)
    }

    /// UNDO: put the saved points back as they were before this run. Gives
    /// the points to keep.
    pub fn undo(&mut self, state: &mut OrchestratorState) -> Result<Vec<CalibrationPoint>, String> {
        let saved = self
            .saved
            .take()
            .ok_or("There is no saved point to undo.")?;
        // A stride saved on top of this point goes first, so the points come
        // back as they were before the whole run.
        self.undo_steps(state);
        self.steps = None;
        let points = with_save_undone(&state.calibration_points, &saved);
        Self::keep_points(self.ran_kmh(), state, points.clone());
        self.undone = true;
        log::info!(
            "Calibration point at {:.1} km/h undone",
            saved.written.speed
        );
        Ok(points)
    }

    /// Use `points` from here on, keeping the deck at the run's speed. The
    /// belt's intent is in calibrated km/h, so a point saved at the run's
    /// speed would otherwise move the deck off it by itself (and restart the
    /// footstep taps). Only a moving belt's intent is re-set, to the same deck
    /// speed; this never starts the belt.
    fn keep_points(target_kmh: f32, state: &mut OrchestratorState, points: Vec<CalibrationPoint>) {
        state.set_calibration_points(points);
        if state.hardware_speed < STOPPED_BELOW_KMH || state.active_program().is_some() {
            return;
        }
        let speed = intent_for_target(state, target_kmh);
        if let Some(intent) = state.manual_intent.as_mut() {
            intent.speed = speed;
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
            saved: self.saved.is_some(),
            undone: self.undone,
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
    /// measured belt, taking back a stride the last one saved. Never touches
    /// the belt. Gives the points to keep when it took one back.
    pub fn start_steps(
        &mut self,
        state: &mut OrchestratorState,
    ) -> Result<Option<Vec<CalibrationPoint>>, String> {
        if self.undone {
            return Err(
                "UNDO took the point at this speed back. TAP AGAIN times the belt again first."
                    .into(),
            );
        }
        let belt = self
            .result
            .clone()
            .ok_or("Time the belt first, then tap each footstep.")?;
        let undone = self.undo_steps(state);
        self.steps = Some(steps::StepRun::new(belt));
        Ok(undone)
    }

    /// SKIP: no footstep phase. A stride not yet measured is never saved, so
    /// the stride stays as it was.
    pub fn close_steps(&mut self) {
        self.steps = None;
    }

    /// One footstep at `now`, if the footstep phase is open. The footstep
    /// that settles the stride saves it by itself, the same way a belt run
    /// saves its point: gives the points to keep then.
    pub fn step_tap(
        &mut self,
        now: Instant,
        state: &mut OrchestratorState,
    ) -> Option<Vec<CalibrationPoint>> {
        let target = self.ran_kmh();
        let s = self.steps.as_mut()?;
        let was_measured = s.result().is_some();
        s.tap(now, state.hardware_speed, target, &state.calibration_points);
        if was_measured {
            return None;
        }
        let result = s.result()?.clone();
        let (points, saved) = with_result_saved(&state.calibration_points, &result);
        Self::keep_points(target, state, points.clone());
        log::info!(
            "Calibration stride at {:.1} km/h saved: {:.1} cm",
            saved.written.speed,
            saved.written.stride_length_cm
        );
        s.set_saved(saved);
        Some(points)
    }

    /// Take back the stride the footstep phase saved, if it saved one.
    fn undo_steps(&mut self, state: &mut OrchestratorState) -> Option<Vec<CalibrationPoint>> {
        let saved = self.steps.as_mut()?.take_saved()?;
        let points = with_save_undone(&state.calibration_points, &saved);
        Self::keep_points(self.ran_kmh(), state, points.clone());
        log::info!(
            "Calibration stride at {:.1} km/h undone",
            saved.written.speed
        );
        Some(points)
    }

    /// UNDO on the stride: the point at the speed goes back to how the belt
    /// run left it. Gives the points to keep.
    pub fn steps_undo(
        &mut self,
        state: &mut OrchestratorState,
    ) -> Result<Vec<CalibrationPoint>, String> {
        let points = self
            .undo_steps(state)
            .ok_or("There is no saved stride to undo.")?;
        if let Some(s) = self.steps.as_mut() {
            s.mark_undone();
        }
        Ok(points)
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
    /// The result is saved, and UNDO can put the points back.
    pub saved: bool,
    /// UNDO was pressed: the result is not saved.
    pub undone: bool,
}

/// The guided calibration in the snapshot.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationSnapshot {
    /// Offer "Calibrate this treadmill": a treadmill is known, it has no belt
    /// length and there are no points (never calibrated), and SKIP was not
    /// pressed for it. The emitter also needs it connected.
    pub offer: bool,
    pub limits: CalibrationLimits,
    /// The belt loop's length for this treadmill, cm; none until entered.
    pub belt_cm: Option<f32>,
    /// A treadmill is known (connected or saved), so a belt length can be
    /// kept for it.
    pub belt_treadmill: bool,
    /// The target speeds the user keeps, slowest first.
    pub targets: Vec<CalibrationTarget>,
    /// Counts each time Rust changes the saved points by itself (a run's
    /// save, UNDO), so a window knows to reload them.
    pub points_rev: u32,
    pub run: Option<CalibrationRunView>,
    /// RUN ALL: the list being walked, if any.
    pub sweep: Option<CalibrationSweepView>,
    /// The guided walk from the top speed, if open.
    pub guide: Option<CalibrationGuideView>,
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

    /// The speed nearest `target_kmh`, if it is one of the sweep's.
    fn index_of(&self, target_kmh: f32) -> Option<usize> {
        self.targets
            .iter()
            .position(|t| (t - target_kmh).abs() <= SAME_TARGET_KMH)
    }

    /// A run at `target_kmh` was saved by itself: mark its speed saved.
    pub fn mark_saved(&mut self, target_kmh: f32) {
        if let Some(i) = self.index_of(target_kmh) {
            self.marks[i] = SweepMark::Saved;
        }
    }

    /// UNDO or TAP AGAIN took a saved point back: its speed is up again.
    pub fn mark_pending(&mut self, target_kmh: f32) {
        if let Some(i) = self.index_of(target_kmh) {
            if self.marks[i] == SweepMark::Saved {
                self.marks[i] = SweepMark::Pending;
            }
        }
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
    // The speed is set as if typed in by hand (`note_hand_speed`).
    let speed = intent_for_target(state, target_hw_kmh);
    crate::bluetooth::apply_manual_intent(state, Some(speed), None);
    // RUN is the user's press, so a paused or stopped belt gets an explicit
    // start or resume, as GO sends. The automatic resume only fires on a
    // target above the paused speed, so a RUN at the speed the belt paused
    // at would otherwise send nothing. Not while Stopping: the deck refuses
    // a start then, and the Stopped branch starts it once the belt is still.
    if matches!(
        state.last_status,
        MachineStatus::Paused | MachineStatus::Stopped
    ) {
        state.trigger_action(crate::engine::DeckAction::Start);
    }
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
    edit_targets(app, |t| with_target_added(t, kmh, min_kmh, max_kmh))?;
    log::info!("Calibration target {kmh:.1} km/h added");
    Ok(())
}

/// Remove one speed; `point_speeds` are the saved points' speeds.
pub fn remove_target(app: &AppHandle, kmh: f32, point_speeds: &[f32]) -> Result<(), String> {
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

/// The settings key that remembers SKIP on the first-connect offer for the
/// treadmill with this device id.
pub fn offer_skip_key_for(device_id: &str) -> String {
    format!("calibrationOfferSkipped:{device_id}")
}

/// The belt loop of the treadmill in use: its settings key and the length,
/// and whether SKIP turned the first-connect offer off for it.
struct Belt {
    key: Option<String>,
    cm: Option<f32>,
    device_id: Option<String>,
    offer_skipped: bool,
}

static BELT: Mutex<Belt> = Mutex::new(Belt {
    key: None,
    cm: None,
    device_id: None,
    offer_skipped: false,
});

fn with_belt<T>(f: impl FnOnce(&mut Belt) -> T) -> T {
    f(&mut BELT.lock().unwrap_or_else(|e| e.into_inner()))
}

/// A stored belt length, if it is a number inside the limits.
fn decode_belt(raw: Option<&str>) -> Option<f32> {
    raw.and_then(|r| r.trim().parse::<f32>().ok())
        .filter(|cm| in_range(*cm, MIN_BELT_CM, MAX_BELT_CM))
}

/// The settings key and stored belt length of the treadmill with this id.
fn belt_for(
    store: &impl crate::persistence::SettingsStore,
    device_id: &str,
) -> (String, Option<f32>) {
    let key = belt_key_for(device_id);
    let cm = decode_belt(store.get(&key).as_deref());
    (key, cm)
}

/// SKIP was pressed on the first-connect offer for the treadmill with this id.
fn offer_skipped_for(store: &impl crate::persistence::SettingsStore, device_id: &str) -> bool {
    crate::persistence::parse_setting(store.get(&offer_skip_key_for(device_id)).as_deref())
        .unwrap_or(false)
}

/// Make the belt length the one stored for this treadmill. Called with
/// DECK_PROFILE's switch (startup with the saved treadmill, each connection).
pub fn switch_belt(store: &impl crate::persistence::SettingsStore, device_id: &str) {
    let (key, cm) = belt_for(store, device_id);
    let skipped = offer_skipped_for(store, device_id);
    with_belt(|b| {
        b.key = Some(key);
        b.cm = cm;
        b.device_id = Some(device_id.to_string());
        b.offer_skipped = skipped;
    });
}

/// SKIP on "Calibrate this treadmill": the offer stays off for this
/// treadmill. Calibration stays in System, Calibration.
pub fn skip_offer(app: &AppHandle) -> Result<(), String> {
    let id = with_belt(|b| b.device_id.clone()).ok_or("Connect the treadmill first.")?;
    crate::persistence::write_setting(app, &offer_skip_key_for(&id), "true")?;
    with_belt(|b| b.offer_skipped = true);
    log::info!("Calibration offer skipped for this treadmill");
    Ok(())
}

/// A treadmill that was never calibrated: no belt length for it and no
/// points. With a known treadmill and no SKIP for it, it gets the offer.
pub fn offer_wanted(
    treadmill_known: bool,
    belt_cm: Option<f32>,
    points: &[CalibrationPoint],
    skipped: bool,
) -> bool {
    treadmill_known && belt_cm.is_none() && points.is_empty() && !skipped
}

/// BELT_LOOP: the length of this treadmill's belt loop, kept for it.
pub fn set_belt(app: &AppHandle, cm: f32) -> Result<(), String> {
    if !in_range(cm, MIN_BELT_CM, MAX_BELT_CM) {
        return Err(format!(
            "Belt length must be {MIN_BELT_CM:.0} to {MAX_BELT_CM:.0} cm."
        ));
    }
    let key = with_belt(|b| b.key.clone())
        .ok_or("Connect the treadmill first, so the belt length is kept for that treadmill.")?;
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
        b.device_id = Some("TEST".into());
        b.offer_skipped = false;
    });
}

/// Tests: SKIP on the offer, without a database.
#[cfg(test)]
pub(crate) fn skip_offer_for_test() {
    with_belt(|b| b.offer_skipped = true);
}

/// Read the stored target list. Call once after the database is initialised.
pub fn init(app: &AppHandle) {
    let raw = crate::persistence::read_setting(app, KEY_TARGETS);
    with_targets(|t| *t = raw.map(|r| decode_targets(Some(&r))));
}

fn with_sweep<T>(f: impl FnOnce(&mut Option<Sweep>) -> T) -> T {
    f(&mut SWEEP.lock().unwrap_or_else(|e| e.into_inner()))
}

fn with_guide<T>(f: impl FnOnce(&mut Option<guide::Guide>) -> T) -> T {
    f(&mut GUIDE.lock().unwrap_or_else(|e| e.into_inner()))
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
    open_run(state, target_hw_kmh)?;
    with_sweep(|s| *s = None);
    with_guide(|g| *g = None);
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
    let sweep = Sweep::new(targets_kmh)?;
    for &t in &sweep.targets {
        check_target(state, t)?;
    }
    open_run(state, sweep.targets[0])?;
    log::info!("Calibration sweep over {} speeds", sweep.targets.len());
    with_sweep(|s| *s = Some(sweep));
    with_guide(|g| *g = None);
    Ok(())
}

/// RUN on the sweep's current speed, his press.
pub(crate) fn sweep_run(state: &mut OrchestratorState) -> Result<(), String> {
    let target = with_sweep(|s| s.as_ref().map(Sweep::current_target))
        .ok_or("Press RUN ALL first.")?
        .ok_or("Every speed is done.")?;
    open_run(state, target)
}

/// SAVE or SKIP in a sweep: mark the current speed and close its run. The
/// belt keeps its speed; the next one waits for RUN.
pub(crate) fn sweep_next(saved: bool) -> Result<(), String> {
    with_sweep(|s| s.as_mut().ok_or("Press RUN ALL first.")?.advance(saved))?;
    with_run(|r| *r = None);
    Ok(())
}

/// End the sweep, the guided walk and any run. The belt keeps going.
pub(crate) fn close_all() {
    with_run(|r| *r = None);
    with_sweep(|s| *s = None);
    with_guide(|g| *g = None);
}

/// The slowest speed the deck runs, in tenths (rounded up, so the deck can
/// run it).
/// DECK_PROFILE's floor, once seen, is the slowest the deck holds.
fn slowest(state: &OrchestratorState) -> f32 {
    let floor = state.deck_profile.floor().map_or(0.0, |b| b as f32 / 10.0);
    (state.min_speed.max(floor).max(0.1) * 10.0 - 1e-3).ceil() / 10.0
}

/// "What's the fastest you'll walk?": open the guided walk from the deck's
/// slowest speed to `top_kmh`. Plans the speeds only: the belt is not
/// touched, and the first speed waits for the user's RUN.
pub(crate) fn guide_start(state: &OrchestratorState, top_kmh: f32) -> Result<(), String> {
    belt_cm().ok_or("Enter the belt loop length first.")?;
    if !top_kmh.is_finite() {
        return Err("Enter your top speed in km/h.".into());
    }
    if top_kmh > state.max_speed {
        return Err(format!(
            "The treadmill tops out at {:.1} km/h.",
            state.max_speed
        ));
    }
    let walk = guide::Guide::new(slowest(state), top_kmh)?;
    log::info!("Calibration walk to {top_kmh:.1} km/h");
    with_run(|r| *r = None);
    with_sweep(|s| *s = None);
    with_guide(|g| *g = Some(walk));
    Ok(())
}

/// RUN on the walk's next speed, the user's press: it sets the belt there.
pub(crate) fn guide_run(state: &mut OrchestratorState) -> Result<(), String> {
    let next = with_guide(|g| g.as_ref().map(guide::Guide::next))
        .ok_or("Pick your top speed first.")?
        .ok_or("Every speed is done.")?;
    open_run(state, next)
}

/// SKIP on the walk's next speed: no run there. Closes its run; the belt
/// keeps its speed.
pub(crate) fn guide_skip() -> Result<(), String> {
    with_guide(|g| g.as_mut().ok_or("Pick your top speed first.")?.skip_next())?;
    with_run(|r| *r = None);
    Ok(())
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
    with_run(|r| match r.as_mut() {
        Some(run) => f(run),
        None => Err("Press RUN on a speed first.".into()),
    })
}

/// TAP: the belt mark passed at `at` (taken as the press arrived, before any
/// lock). Only reads the deck's speed; never sends anything to the belt.
pub(crate) fn tap(
    state: &mut OrchestratorState,
    at: Instant,
) -> Result<Option<Vec<CalibrationPoint>>, String> {
    if let Some(points) = on_run(|run| Ok(run.step_tap(at, state)))? {
        // A footstep: the same inputs, timed against the measured belt. The
        // footsteps that settle the stride saved it.
        points_changed();
        return Ok(Some(points));
    }
    if with_run(|r| r.as_ref().is_some_and(|run| run.steps.is_some())) {
        return Ok(None);
    }
    let saved = on_run(|run| {
        let was_measured = run.result.is_some();
        run.tap(at, state.hardware_speed, |kmh| {
            state.calculate_stride_length(kmh)
        });
        // A belt run times the belt alone: the factor is measured, the stride kept.
        run.average_into(&state.calibration_points, true);
        // The taps that finish the run also save its point, averaged into the
        // one at that speed if there is one.
        if was_measured {
            return Ok(None);
        }
        let measured = run
            .result
            .as_ref()
            .map(|r| (r.point.speed, r.point.distance_scale));
        Ok(run
            .autosave(state)
            .map(|points| (run.target_kmh, measured, points)))
    })?;
    Ok(saved.map(|(target, measured, points)| {
        with_sweep(|s| s.iter_mut().for_each(|s| s.mark_saved(target)));
        if let Some((at_kmh, scale)) = measured {
            // The walk reads the run's own measurement, not the average.
            with_guide(|g| g.iter_mut().for_each(|g| g.record(target, at_kmh, scale)));
        }
        points_changed();
        points
    }))
}

/// UNDO: put the saved points back as they were before this run. Stays
/// available until the next run starts or the run closes. Gives the points to
/// keep.
pub(crate) fn undo(state: &mut OrchestratorState) -> Result<Vec<CalibrationPoint>, String> {
    let (target, ran, points) =
        on_run(|run| run.undo(state).map(|p| (run.target_kmh, run.ran_kmh(), p)))?;
    with_sweep(|s| s.iter_mut().for_each(|s| s.mark_pending(target)));
    with_guide(|g| g.iter_mut().for_each(|g| g.forget(target, ran)));
    points_changed();
    Ok(points)
}

/// TAP AGAIN: clear the run's taps and result, taking back the point the
/// result saved so the new measurement replaces it. The belt keeps going.
/// Gives the points to keep when it took one back.
pub(crate) fn retap(
    state: &mut OrchestratorState,
) -> Result<Option<Vec<CalibrationPoint>>, String> {
    let undone = on_run(|run| {
        let points = run
            .undo(state)
            .ok()
            .map(|p| (run.target_kmh, run.ran_kmh(), p));
        run.retap();
        Ok(points)
    })?;
    Ok(undone.map(|(target, ran, points)| {
        with_sweep(|s| s.iter_mut().for_each(|s| s.mark_pending(target)));
        with_guide(|g| g.iter_mut().for_each(|g| g.forget(target, ran)));
        points_changed();
        points
    }))
}

fn points_changed() {
    POINTS_REV.fetch_add(1, Ordering::Relaxed);
}

/// Keep `points` as the saved calibration points, after Rust changed them.
/// Run off the async runtime: it writes the database.
pub fn persist_points(app: &AppHandle, points: &[CalibrationPoint]) {
    let mut config = crate::persistence::load_config(app);
    config.calibration_points = points.to_vec();
    if let Err(e) = crate::persistence::save_config(app, &config) {
        log::warn!("could not save the calibration points: {e}");
    }
}

/// TAP STEPS: start the footstep phase on the measured run. Never moves the
/// belt. Gives the points to keep when TAP AGAIN took a saved stride back.
pub(crate) fn steps_start(
    state: &mut OrchestratorState,
) -> Result<Option<Vec<CalibrationPoint>>, String> {
    let undone = on_run(|run| run.start_steps(state))?;
    if undone.is_some() {
        points_changed();
    }
    Ok(undone)
}

/// SKIP on the footsteps: the stride stays as it was.
pub(crate) fn steps_skip() -> Result<(), String> {
    on_run(|run| {
        run.close_steps();
        Ok(())
    })
}

/// UNDO on the stride. Gives the points to keep.
pub(crate) fn steps_undo(state: &mut OrchestratorState) -> Result<Vec<CalibrationPoint>, String> {
    let points = on_run(|run| run.steps_undo(state))?;
    points_changed();
    Ok(points)
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
    let (belt_treadmill, offer_skipped) = with_belt(|b| (b.key.is_some(), b.offer_skipped));
    CalibrationSnapshot {
        offer: offer_wanted(
            belt_treadmill,
            belt_cm(),
            &state.calibration_points,
            offer_skipped,
        ),
        limits: CalibrationLimits::default(),
        belt_cm: belt_cm(),
        belt_treadmill,
        targets,
        points_rev: POINTS_REV.load(Ordering::Relaxed),
        run,
        sweep: with_sweep(|s| s.as_ref().map(Sweep::view)),
        guide: with_guide(|g| g.as_ref().map(guide::Guide::view)),
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
            stride_runs: 0,
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
                stride_runs: 0,
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
        // Two belt runs saved at 0.9, with 70 cm from two footstep runs; a
        // third run at 1.2 and 76 cm.
        let saved = CalibrationPoint {
            runs: 2,
            stride_runs: 2,
            ..pt(3.0, 0.9)
        };
        let mut r = point_from_taps(300.0, 4.0, 36.0, 3.0, NO_STRIDE).unwrap();
        r.point.distance_scale = 1.2;
        r.point.stride_length_cm = 76.0;
        r.stride_measured = true;
        let p = averaged_point(&saved, &r, true);
        assert_eq!((p.runs, p.stride_runs), (3, 3));
        assert_eq!(p.speed, 3.0);
        assert!((p.distance_scale - 1.0).abs() < 1e-5);
        assert!((p.stride_length_cm - 72.0).abs() < 1e-4);
        // A belt run (no steps) keeps the saved stride and its count; an old
        // point is one run.
        r.stride_measured = false;
        let old = CalibrationPoint {
            runs: 0,
            ..pt(3.0, 0.9)
        };
        let p = averaged_point(&old, &r, true);
        assert_eq!((p.runs, p.stride_runs), (2, 0));
        assert_eq!(p.stride_length_cm, 70.0);
        assert!((p.distance_scale - 1.05).abs() < 1e-5);
        // A stride-only run: the stride averages, the saved factor stays.
        r.stride_measured = true;
        let p = averaged_point(&saved, &r, false);
        assert_eq!(p.runs, 3);
        assert!((p.stride_length_cm - 72.0).abs() < 1e-4);
        assert_eq!(p.distance_scale, 0.9);
    }

    #[test]
    fn the_first_measured_stride_replaces_a_borrowed_one() {
        // Five belt runs, but the stride was only ever borrowed (count 0).
        let saved = CalibrationPoint {
            runs: 5,
            stride_length_cm: 70.0,
            ..pt(3.0, 0.9)
        };
        let mut r = point_from_taps(300.0, 4.0, 36.0, 3.0, NO_STRIDE).unwrap();
        r.point.stride_length_cm = 46.0;
        r.stride_measured = true;
        let p = averaged_point(&saved, &r, false);
        assert_eq!(p.stride_length_cm, 46.0, "full weight, not a sixth");
        assert_eq!(p.stride_runs, 1);
        // The second measured stride averages with the first only.
        r.point.stride_length_cm = 50.0;
        let p = averaged_point(&p, &r, false);
        assert!((p.stride_length_cm - 48.0).abs() < 1e-4);
        assert_eq!(p.stride_runs, 2);
        // A belt run after that keeps both strides and their count.
        r.stride_measured = false;
        let p = averaged_point(&p, &r, true);
        assert!((p.stride_length_cm - 48.0).abs() < 1e-4);
        assert_eq!(p.stride_runs, 2);
    }

    #[test]
    fn a_stored_point_without_a_stride_count_reads_as_never_measured() {
        let p: CalibrationPoint = serde_json::from_str(
            r#"{"speed":3.0,"stride_length_cm":70.0,"distance_scale":0.9,"runs":4}"#,
        )
        .unwrap();
        assert_eq!((p.runs, p.stride_runs), (4, 0));
        let p: CalibrationPoint =
            serde_json::from_str(r#"{"speed":3.0,"strideLengthCm":70.0,"distanceScale":0.9}"#)
                .unwrap();
        assert_eq!((p.runs, p.stride_runs), (1, 0));
        // And it round-trips with the count.
        let back = serde_json::to_string(&CalibrationPoint {
            stride_runs: 2,
            ..p
        })
        .unwrap();
        assert!(back.contains(r#""stride_runs":2"#));
    }

    /// The 3.0 km/h point a belt run saves over `stride` cm and a factor of
    /// 0.9 from two runs; the run's own factor is 3.0 / 2.7.
    fn three(stride: f32) -> CalibrationPoint {
        CalibrationPoint {
            runs: 2,
            stride_length_cm: stride,
            ..pt(3.0, 0.9)
        }
    }

    /// A belt run measured and saved over `points`, its footstep phase open,
    /// with the deck at 3.0 km/h.
    fn stepping(points: Vec<CalibrationPoint>) -> (OrchestratorState, Run) {
        let (mut state, mut run) = measured_run(points);
        run.autosave(&mut state).unwrap();
        state.hardware_speed = 3.0;
        assert_eq!(run.start_steps(&mut state), Ok(None));
        (state, run)
    }

    /// 21 footsteps 0.6 s apart (100 a minute, 45 cm at 2.7 km/h): what the
    /// last one saved, if anything.
    fn footsteps(run: &mut Run, state: &mut OrchestratorState) -> Option<Vec<CalibrationPoint>> {
        let t = Instant::now() + Duration::from_secs(60);
        (0..21)
            .map(|i| run.step_tap(t + Duration::from_secs_f64(i as f64 * 0.6), state))
            .last()
            .flatten()
    }

    #[test]
    fn footsteps_need_a_measured_belt_and_listen_like_taps() {
        let mut state = OrchestratorState::default();
        assert!(run_at_3()
            .start_steps(&mut state)
            .unwrap_err()
            .contains("Time the belt"));
        let (_, run) = stepping(vec![]);
        assert!(run.listening(), "Alt+Space and Space tap footsteps now");
        let v = run.view(Instant::now()).steps.unwrap();
        assert_eq!(
            (v.phase, v.steps, v.saved),
            (CalibrationPhase::Settling, 0, false)
        );
    }

    #[test]
    fn a_stride_saves_by_itself_and_the_factor_stays() {
        let (mut state, mut run) = stepping(vec![three(70.0), pt(4.5, 0.887)]);
        let belt_saved = state.calibration_points[0].clone();
        assert_eq!(belt_saved.runs, 3);
        assert_eq!(
            belt_saved.stride_length_cm, 70.0,
            "a belt run keeps the stride"
        );
        let kept = footsteps(&mut run, &mut state).expect("saved by itself");
        assert_eq!(kept, state.calibration_points);
        let p = &state.calibration_points[0];
        assert_eq!(
            p.distance_scale, belt_saved.distance_scale,
            "the factor stays"
        );
        assert_eq!(p.runs, 3, "the belt save already counted the run");
        assert!(
            (p.stride_length_cm - 45.0).abs() < 1e-3,
            "the first measured stride replaces the borrowed one"
        );
        assert_eq!(p.stride_runs, 1);
        assert_eq!(state.calibration_points[1], pt(4.5, 0.887));
        let v = run.view(Instant::now()).steps.unwrap();
        assert!(v.saved && !v.undone);
        let r = v.result.unwrap();
        assert!(r.stride_measured);
        assert_eq!(r.cadence_spm.map(f32::round), Some(100.0));
        // More footsteps save nothing again.
        assert_eq!(
            run.step_tap(Instant::now() + Duration::from_secs(99), &mut state),
            None
        );
        assert!(!run.listening());
    }

    #[test]
    fn a_speed_with_no_point_gets_one_at_the_belt_factor() {
        let (mut state, mut run) = stepping(vec![]);
        // The user deleted the belt's point before the footsteps settled.
        state.set_calibration_points(vec![]);
        footsteps(&mut run, &mut state).unwrap();
        let p = &state.calibration_points[0];
        assert!((p.stride_length_cm - 45.0).abs() < 1e-3);
        assert!((p.distance_scale - 3.0 / 2.7).abs() < 1e-4);
        run.steps_undo(&mut state).unwrap();
        assert!(state.calibration_points.is_empty());
    }

    #[test]
    fn undo_on_the_stride_puts_back_the_belt_point() {
        let (mut state, mut run) = stepping(vec![three(70.0)]);
        let belt_saved = state.calibration_points.clone();
        assert!(run.steps_undo(&mut state).is_err(), "nothing saved yet");
        footsteps(&mut run, &mut state).unwrap();
        assert_eq!(run.steps_undo(&mut state).unwrap(), belt_saved);
        assert_eq!(state.calibration_points, belt_saved);
        let v = run.view(Instant::now()).steps.unwrap();
        assert!(!v.saved && v.undone);
        assert!(run.steps_undo(&mut state).is_err());
    }

    #[test]
    fn undo_on_the_belt_takes_back_the_stride_too() {
        let (mut state, mut run) = stepping(vec![three(70.0)]);
        footsteps(&mut run, &mut state).unwrap();
        run.undo(&mut state).unwrap();
        assert_eq!(state.calibration_points, vec![three(70.0)]);
        assert_eq!(run.view(Instant::now()).steps, None);
        assert!(run
            .start_steps(&mut state)
            .unwrap_err()
            .contains("TAP AGAIN"));
    }

    #[test]
    fn tap_again_on_the_steps_replaces_the_stride() {
        let (mut state, mut run) = stepping(vec![three(70.0)]);
        let belt_saved = state.calibration_points.clone();
        footsteps(&mut run, &mut state).unwrap();
        assert_eq!(run.start_steps(&mut state), Ok(Some(belt_saved.clone())));
        assert_eq!(state.calibration_points, belt_saved);
        footsteps(&mut run, &mut state).unwrap();
        assert!(
            (state.calibration_points[0].stride_length_cm - 45.0).abs() < 1e-3,
            "replaced, not averaged in twice"
        );
        assert_eq!(state.calibration_points[0].stride_runs, 1);
    }

    #[test]
    fn skip_leaves_the_stride_as_it_was() {
        let (mut state, mut run) = stepping(vec![three(70.0)]);
        let belt_saved = state.calibration_points.clone();
        run.step_tap(Instant::now(), &mut state);
        run.close_steps();
        assert_eq!(run.view(Instant::now()).steps, None);
        assert_eq!(state.calibration_points, belt_saved);
    }

    #[test]
    fn stop_ends_the_footsteps_but_keeps_the_belt_result() {
        let t = Instant::now();
        let mut run = run_at_3();
        tap_n(&mut run, t, 4.0, 5, 3.0);
        assert!(!run.stopped(), "a run still tapping closes");
        let (_, mut run) = stepping(vec![]);
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
    /// A state holding `points`, and a run at 3.0 km/h tapped to a result
    /// (the belt really at 2.7 km/h, the deck at 3.0: a factor of 3.0 / 2.7).
    fn measured_run(points: Vec<CalibrationPoint>) -> (OrchestratorState, Run) {
        let mut state = OrchestratorState::default();
        state.sync_config(&crate::persistence::AppConfig {
            calibration_points: points,
            ..Default::default()
        });
        let mut run = run_at_3();
        tap_n(&mut run, Instant::now(), 4.0, 9, 3.0);
        run.average_into(&state.calibration_points, true);
        assert_eq!(run.view(Instant::now()).phase, CalibrationPhase::Measured);
        (state, run)
    }

    #[test]
    fn a_measured_run_saves_its_point_by_itself() {
        let (mut state, mut run) = measured_run(vec![pt(4.0, 0.8)]);
        let kept = run.autosave(&mut state).unwrap();
        assert_eq!(kept, state.calibration_points);
        assert_eq!(state.calibration_points.len(), 2);
        assert_eq!(state.calibration_points[0].speed, 3.0, "kept by speed");
        assert_eq!(state.calibration_points[0].runs, 1);
        assert!(run.view(Instant::now()).saved);
        // The engine uses it now: the sorted points and the lookup follow.
        assert_eq!(state.sorted_points, state.calibration_points);
        // Saved once: another call does nothing.
        assert_eq!(run.autosave(&mut state), None);
        assert_eq!(state.calibration_points.len(), 2);
    }

    #[test]
    fn a_run_at_a_saved_speed_is_averaged_in() {
        let saved = CalibrationPoint {
            runs: 2,
            ..pt(3.0, 0.9)
        };
        let (mut state, mut run) = measured_run(vec![saved.clone()]);
        let scale = run
            .view(Instant::now())
            .result
            .unwrap()
            .point
            .distance_scale;
        run.autosave(&mut state).unwrap();
        assert_eq!(state.calibration_points.len(), 1);
        let p = &state.calibration_points[0];
        assert_eq!(p.runs, 3);
        assert!((p.distance_scale - (0.9 * 2.0 + scale) / 3.0).abs() < 1e-5);
        // Taps after the save, and the next snapshot, never fold it in twice.
        run.average_into(&state.calibration_points, true);
        assert_eq!(
            run.view(Instant::now())
                .result
                .unwrap()
                .averaged
                .unwrap()
                .runs,
            3
        );
    }

    #[test]
    fn undo_removes_a_point_the_run_added() {
        let (mut state, mut run) = measured_run(vec![pt(4.0, 0.8)]);
        run.autosave(&mut state).unwrap();
        let back = run.undo(&mut state).unwrap();
        assert_eq!(back, vec![pt(4.0, 0.8)]);
        assert_eq!(state.calibration_points, back);
        assert_eq!(state.sorted_points, back);
        let v = run.view(Instant::now());
        assert!(!v.saved && v.undone);
        // A second UNDO has nothing to put back, and a run undone is not saved again.
        assert!(run.undo(&mut state).is_err());
        assert_eq!(run.autosave(&mut state), None);
        assert_eq!(state.calibration_points, back);
    }

    #[test]
    fn undo_restores_an_averaged_point_exactly() {
        let saved = CalibrationPoint {
            runs: 2,
            stride_length_cm: 68.4,
            ..pt(3.0, 0.912)
        };
        let (mut state, mut run) = measured_run(vec![saved.clone(), pt(4.5, 0.887)]);
        run.autosave(&mut state).unwrap();
        assert_eq!(state.calibration_points[0].runs, 3);
        run.undo(&mut state).unwrap();
        assert_eq!(state.calibration_points, vec![saved, pt(4.5, 0.887)]);
    }

    #[test]
    fn undo_leaves_points_changed_since_alone() {
        let (mut state, mut run) = measured_run(vec![pt(4.0, 0.8)]);
        run.autosave(&mut state).unwrap();
        // The user deleted the 4.0 point meanwhile.
        state.set_calibration_points(
            state
                .calibration_points
                .iter()
                .filter(|p| p.speed != 4.0)
                .cloned()
                .collect(),
        );
        run.undo(&mut state).unwrap();
        assert!(state.calibration_points.is_empty());
    }

    #[test]
    fn undo_goes_with_its_run() {
        // The next run is a new `Run` (RUN replaces the open one), and TAP
        // AGAIN clears the result: neither can undo the earlier save.
        let (mut state, mut run) = measured_run(vec![]);
        run.autosave(&mut state).unwrap();
        let mut next = run_at_3();
        assert!(next.undo(&mut state).is_err());
        assert_eq!(state.calibration_points.len(), 1);
        run.retap();
        assert!(run.undo(&mut state).is_err());
        let v = run.view(Instant::now());
        assert!(!v.saved && !v.undone && v.result.is_none());
    }

    #[test]
    fn a_sweep_speed_is_saved_with_its_point_and_up_again_after_undo() {
        let mut s = Sweep::new(&[2.0, 3.0, 4.0]).unwrap();
        s.mark_saved(3.0);
        assert_eq!(s.view().marks[1], SweepMark::Saved);
        assert_eq!(s.current_target(), Some(2.0));
        s.mark_pending(3.0);
        assert_eq!(s.view().marks[1], SweepMark::Pending);
        // A skipped speed stays skipped.
        s.advance(false).unwrap();
        s.mark_pending(2.0);
        assert_eq!(s.view().marks[0], SweepMark::Skipped);
    }

    #[test]
    fn never_calibrated_is_no_belt_length_and_no_points() {
        assert!(offer_wanted(true, None, &[], false));
        assert!(!offer_wanted(false, None, &[], false), "no treadmill known");
        assert!(
            !offer_wanted(true, Some(280.0), &[], false),
            "a belt length"
        );
        assert!(!offer_wanted(true, None, &[pt(3.0, 0.9)], false), "a point");
        assert!(!offer_wanted(true, None, &[], true), "SKIP");
    }

    #[test]
    fn skip_on_the_offer_is_kept_per_treadmill() {
        let _globals = TEST_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let state = OrchestratorState::default();
        set_belt_for_test(None);
        assert!(snapshot(&state).offer, "a new treadmill gets the offer");
        skip_offer_for_test();
        assert!(!snapshot(&state).offer, "SKIP turns it off");
        // Another treadmill (switched in with its own settings) has its own.
        set_belt_for_test(None);
        assert!(snapshot(&state).offer);
        assert_eq!(offer_skip_key_for("AAAA"), "calibrationOfferSkipped:AAAA");
        assert_ne!(offer_skip_key_for("AAAA"), offer_skip_key_for("BBBB"));
    }

    #[test]
    fn the_guided_walk_starts_from_the_slowest_speed_and_needs_a_top_above_it() {
        let _globals = TEST_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        close_all();
        let state = OrchestratorState {
            min_speed: 0.55,
            max_speed: 8.0,
            ..Default::default()
        };
        assert_eq!(slowest(&state), 0.6, "rounded up to a speed the deck runs");
        set_belt_for_test(None);
        assert!(guide_start(&state, 5.0).unwrap_err().contains("belt loop"));
        set_belt_for_test(Some(280.0));
        assert!(guide_start(&state, 9.0).unwrap_err().contains("tops out"));
        assert!(guide_start(&state, 0.6)
            .unwrap_err()
            .contains("above the slowest"));
        assert!(guide_start(&state, f32::NAN).is_err());
        guide_start(&state, 5.0).unwrap();
        let g = snapshot(&state).guide.unwrap();
        assert_eq!((g.slow_kmh, g.top_kmh, g.next_kmh), (0.6, 5.0, Some(0.6)));
        assert_eq!(snapshot(&state).run, None, "planning opens no run");
        // SKIP moves on to the top speed; a single RUN leaves the walk.
        guide_skip().unwrap();
        assert_eq!(snapshot(&state).guide.unwrap().next_kmh, Some(5.0));
        close_all();
        assert_eq!(snapshot(&state).guide, None);
    }

    #[test]
    fn belt_length_follows_the_treadmill() {
        use crate::persistence::SettingsStore;
        let conn = crate::persistence::memory_db();
        conn.put(&belt_key_for("AAAA"), "300").unwrap();
        conn.put(&belt_key_for("BBBB"), "320").unwrap();
        assert_eq!(belt_for(&conn, "AAAA"), (belt_key_for("AAAA"), Some(300.0)));
        assert_eq!(belt_for(&conn, "BBBB"), (belt_key_for("BBBB"), Some(320.0)));
        // A treadmill with none stored starts without one.
        assert_eq!(belt_for(&conn, "CCCC"), (belt_key_for("CCCC"), None));
        assert_eq!(belt_for(&conn, "AAAA").1, Some(300.0));
    }

    #[test]
    fn skip_on_the_offer_is_stored_per_treadmill() {
        use crate::persistence::SettingsStore;
        let conn = crate::persistence::memory_db();
        conn.put(&offer_skip_key_for("AAAA"), "true").unwrap();
        assert!(offer_skipped_for(&conn, "AAAA"));
        assert!(
            !offer_skipped_for(&conn, "BBBB"),
            "another treadmill still gets it"
        );
    }
}
