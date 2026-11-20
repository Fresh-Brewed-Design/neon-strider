//! The guided calibration (a Labs switch, off by default): the limits a
//! calibration point must stay inside (shared with the engine and the UI), the
//! math that turns one timed run into a point, and the run Rust keeps while he
//! walks it.
//!
//! A point is `(speed, stride_length_cm, distance_scale)`: `speed` is what the
//! deck reports (hardware km/h) and `distance_scale` is reported distance over
//! real distance, so physical km/h = hardware km/h / scale (see `engine.rs`).
//!
//! One run per target speed: RUN (his press) sets the belt to the target; once
//! it is steady he presses COUNT and counts belt loops (and steps if he can)
//! until DONE. Rust times the run itself and samples the deck's reported speed
//! while he counts, so a belt that stopped or changed speed during the count
//! is refused instead of saved as a 0 km/h point.

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
/// The belt loop length he can enter, cm.
pub const MIN_BELT_CM: f32 = 50.0;
pub const MAX_BELT_CM: f32 = 1000.0;
/// A count shorter than this is too coarse to trust.
pub const MIN_RUN_S: f32 = 15.0;
/// Fewest steps that give a stride.
pub const MIN_STEPS: u32 = 10;
/// The speeds the wizard offers, in deck km/h.
pub const DEFAULT_TARGETS_KMH: [f32; 5] = [2.0, 3.0, 4.0, 5.0, 6.0];
/// A reported speed below this is a still belt.
const STOPPED_BELOW_KMH: f32 = 0.05;
/// Reported speeds during one count may differ by at most this much.
const MAX_SPREAD_KMH: f32 = 0.25;

const KEY_WIZARD: &str = "labs_calibration_wizard";
static WIZARD_ON: AtomicBool = AtomicBool::new(false);
static RUN: Mutex<Option<Run>> = Mutex::new(None);

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
    pub min_run_s: f32,
    pub min_steps: u32,
    pub default_targets_kmh: Vec<f32>,
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
            min_run_s: MIN_RUN_S,
            min_steps: MIN_STEPS,
            default_targets_kmh: DEFAULT_TARGETS_KMH.to_vec(),
        }
    }
}

/// What he counted during one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "method", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum CalibrationCount {
    /// Passes of a mark on the belt, each one belt loop; steps too if he
    /// counted them (for the stride).
    Loops {
        loops: f32,
        belt_length_cm: f32,
        steps: Option<u32>,
    },
    /// Steps of a stride he already knows.
    Steps { steps: u32, stride_cm: f32 },
}

/// A timed count: how long it ran and the deck's mean reported speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub elapsed_s: f32,
    pub reported_kmh: f32,
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
    /// The speed he really walked, km/h.
    pub real_kmh: f32,
    /// The distance he really walked, m.
    pub real_m: f32,
    /// The distance the deck reported for the same time, m.
    pub reported_m: f32,
    /// Steps a minute, when he counted steps.
    pub cadence_spm: Option<f32>,
    /// False when the stride came from the current calibration (loops only).
    pub stride_measured: bool,
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

/// The deck's speeds sampled during a count: their mean, or why the count
/// can't be used. A still belt (the treadmill stopped mid-count) and a speed
/// that changed are refused.
pub fn summarize_samples(samples: &[f32]) -> Result<f32, String> {
    if samples.is_empty() {
        return Err("No speed was read during the count. Run the speed again.".into());
    }
    let (min, max) = samples
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &s| (lo.min(s), hi.max(s)));
    if min < STOPPED_BELOW_KMH {
        return Err("The belt stopped during the count. Run the speed again.".into());
    }
    if max - min > MAX_SPREAD_KMH {
        return Err(format!(
            "The speed changed during the count ({min:.1} to {max:.1} km/h). Run the speed again."
        ));
    }
    Ok(samples.iter().sum::<f32>() / samples.len() as f32)
}

/// The point from one run. `stride_at` gives the current calibration's stride
/// at a real speed, used when he counted loops but no steps.
pub fn point_from_count(
    m: Measurement,
    count: &CalibrationCount,
    stride_at: impl Fn(f32) -> f32,
) -> Result<CalibrationResult, String> {
    if !m.elapsed_s.is_finite() || m.elapsed_s < MIN_RUN_S {
        return Err(format!(
            "Count for at least {MIN_RUN_S:.0} s. The count ran {:.1} s.",
            m.elapsed_s
        ));
    }
    if !m.reported_kmh.is_finite() || m.reported_kmh < STOPPED_BELOW_KMH {
        return Err("The belt was not moving. Run the speed again.".into());
    }
    let reported_m = m.reported_kmh / 3.6 * m.elapsed_s;
    let (real_m, steps) = match *count {
        CalibrationCount::Loops {
            loops,
            belt_length_cm,
            steps,
        } => {
            if !(loops.is_finite() && loops > 0.0) {
                return Err("Enter how many loops you counted.".into());
            }
            if !in_range(belt_length_cm, MIN_BELT_CM, MAX_BELT_CM) {
                return Err(format!(
                    "Belt length must be {MIN_BELT_CM:.0} to {MAX_BELT_CM:.0} cm."
                ));
            }
            (loops * belt_length_cm / 100.0, steps)
        }
        CalibrationCount::Steps { steps, stride_cm } => {
            if !in_range(stride_cm, MIN_STRIDE_CM, MAX_STRIDE_CM) {
                return Err(format!(
                    "Stride must be {MIN_STRIDE_CM:.0} to {MAX_STRIDE_CM:.0} cm."
                ));
            }
            (steps as f32 * stride_cm / 100.0, Some(steps))
        }
    };
    if let Some(n) = steps {
        if n < MIN_STEPS {
            return Err(format!("Count at least {MIN_STEPS} steps."));
        }
    }
    let real_kmh = real_m / m.elapsed_s * 3.6;
    let scale = reported_m / real_m;
    if !in_range(scale, MIN_SCALE, MAX_SCALE) {
        return Err(format!(
            "The loop count gives a factor of {scale:.2}, outside {MIN_SCALE} to {MAX_SCALE}. Check the loop count and run the speed again."
        ));
    }
    let measured_stride = match count {
        CalibrationCount::Loops { .. } => steps.map(|n| real_m / n as f32 * 100.0),
        CalibrationCount::Steps { stride_cm, .. } => Some(*stride_cm),
    };
    let stride_cm = match measured_stride {
        Some(cm) if !in_range(cm, MIN_STRIDE_CM, MAX_STRIDE_CM) => {
            return Err(format!(
                "The step count gives a {cm:.0} cm stride, outside {MIN_STRIDE_CM:.0} to {MAX_STRIDE_CM:.0} cm. Check the step count."
            ));
        }
        Some(cm) => cm,
        None => stride_at(real_kmh).clamp(MIN_STRIDE_CM, MAX_STRIDE_CM),
    };
    Ok(CalibrationResult {
        point: CalibrationPoint {
            // The deck reports tenths of km/h.
            speed: (m.reported_kmh * 10.0).round() / 10.0,
            stride_length_cm: stride_cm,
            distance_scale: scale,
            runs: 1,
        },
        real_kmh,
        real_m,
        reported_m,
        cadence_spm: steps.map(|n| n as f32 / m.elapsed_s * 60.0),
        stride_measured: measured_stride.is_some(),
    })
}

/// Where a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum CalibrationPhase {
    /// The belt is on its way to the target; COUNT when it is steady.
    Settling,
    /// He is counting; DONE stops the clock.
    Counting,
    /// The count is timed; he enters what he counted.
    Counted,
}

/// One run, kept by Rust from RUN until it is saved or closed.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub target_kmh: f32,
    phase: CalibrationPhase,
    started: Option<Instant>,
    samples: Vec<f32>,
    measurement: Option<Measurement>,
    result: Option<CalibrationResult>,
}

impl Run {
    pub fn new(target_kmh: f32) -> Self {
        Self {
            target_kmh,
            phase: CalibrationPhase::Settling,
            started: None,
            samples: vec![],
            measurement: None,
            result: None,
        }
    }

    /// COUNT: start the clock. Refused while the belt is still.
    pub fn start_count(&mut self, now: Instant, raw_kmh: f32) -> Result<(), String> {
        if self.phase == CalibrationPhase::Counting {
            return Ok(());
        }
        if raw_kmh < STOPPED_BELOW_KMH {
            return Err(
                "The belt is not moving. Press RUN, then COUNT once it is up to speed.".into(),
            );
        }
        self.phase = CalibrationPhase::Counting;
        self.started = Some(now);
        self.samples = vec![raw_kmh];
        self.measurement = None;
        self.result = None;
        Ok(())
    }

    /// The deck's reported speed, read while counting (each snapshot tick).
    pub fn sample(&mut self, raw_kmh: f32) {
        if self.phase == CalibrationPhase::Counting {
            self.samples.push(raw_kmh);
        }
    }

    /// DONE: stop the clock. A count the samples refuse goes back to settling.
    pub fn stop_count(&mut self, now: Instant, raw_kmh: f32) -> Result<(), String> {
        let Some(started) = self
            .started
            .filter(|_| self.phase == CalibrationPhase::Counting)
        else {
            return Err("Press COUNT first.".into());
        };
        // Too early: the clock keeps running, so he can carry on counting.
        let elapsed = now.duration_since(started).as_secs_f32();
        if elapsed < MIN_RUN_S {
            return Err(format!(
                "Keep counting: at least {MIN_RUN_S:.0} s, so far {elapsed:.0} s."
            ));
        }
        self.samples.push(raw_kmh);
        let summary = summarize_samples(&self.samples);
        self.samples.clear();
        self.started = None;
        match summary {
            Ok(reported_kmh) => {
                self.measurement = Some(Measurement {
                    elapsed_s: now.duration_since(started).as_secs_f32(),
                    reported_kmh,
                });
                self.phase = CalibrationPhase::Counted;
                Ok(())
            }
            Err(e) => {
                self.phase = CalibrationPhase::Settling;
                Err(e)
            }
        }
    }

    /// What he counted: work out the point.
    pub fn count(
        &mut self,
        count: &CalibrationCount,
        stride_at: impl Fn(f32) -> f32,
    ) -> Result<(), String> {
        let m = self.measurement.ok_or("Time a count first.")?;
        let result = point_from_count(m, count, stride_at);
        self.result = result.as_ref().ok().cloned();
        result.map(|_| ())
    }

    pub fn view(&self, now: Instant) -> CalibrationRunView {
        CalibrationRunView {
            target_kmh: self.target_kmh,
            phase: self.phase,
            elapsed_s: match (self.started, self.measurement) {
                (Some(t), _) => now.duration_since(t).as_secs_f32(),
                (None, Some(m)) => m.elapsed_s,
                _ => 0.0,
            },
            reported_kmh: self.measurement.map(|m| m.reported_kmh),
            result: self.result.clone(),
        }
    }
}

/// A run as the windows see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationRunView {
    /// Deck km/h the run asked for.
    pub target_kmh: f32,
    pub phase: CalibrationPhase,
    /// The count's time so far (counting) or in all (counted), s, from Rust's clock.
    pub elapsed_s: f32,
    /// Mean deck-reported speed over the count, once it is timed.
    pub reported_kmh: Option<f32>,
    /// The point, once he entered his count.
    pub result: Option<CalibrationResult>,
}

/// The guided calibration in the snapshot.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationSnapshot {
    /// Labs switch: the guided wizard replaces the old calibration panel.
    pub wizard_enabled: bool,
    pub limits: CalibrationLimits,
    pub run: Option<CalibrationRunView>,
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
    if !target_hw_kmh.is_finite() || target_hw_kmh < state.min_speed.max(0.1) {
        return Err(format!(
            "Pick a speed of at least {:.1} km/h.",
            state.min_speed
        ));
    }
    if target_hw_kmh > state.max_speed {
        return Err(format!(
            "The treadmill tops out at {:.1} km/h.",
            state.max_speed
        ));
    }
    if state.active_program().is_some() {
        return Err("End the program first, then calibrate.".into());
    }
    // A speed set by hand starts a still belt (`note_hand_speed`), so this
    // is the same path as typing the speed in.
    let speed = intent_for_target(state, target_hw_kmh);
    crate::bluetooth::apply_manual_intent(state, Some(speed), None);
    Ok(())
}

fn with_run<T>(f: impl FnOnce(&mut Option<Run>) -> T) -> T {
    f(&mut RUN.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Read the Labs switch. Call once after the database is initialised.
pub fn init(app: &AppHandle) {
    let on = crate::persistence::parse_setting(
        crate::persistence::read_setting(app, KEY_WIZARD).as_deref(),
    )
    .unwrap_or(false);
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
        with_run(|r| *r = None);
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

/// RUN: a new run at `target_hw_kmh` (also "repeat this point").
pub(crate) fn run_start(state: &mut OrchestratorState, target_hw_kmh: f32) -> Result<(), String> {
    require_wizard()?;
    begin_run(state, target_hw_kmh)?;
    log::info!("Calibration run at {target_hw_kmh:.1} km/h");
    with_run(|r| *r = Some(Run::new(target_hw_kmh)));
    Ok(())
}

fn on_run<T>(f: impl FnOnce(&mut Run) -> Result<T, String>) -> Result<T, String> {
    require_wizard()?;
    with_run(|r| match r.as_mut() {
        Some(run) => f(run),
        None => Err("Press RUN on a speed first.".into()),
    })
}

pub(crate) fn count_start(raw_kmh: f32) -> Result<(), String> {
    on_run(|run| run.start_count(Instant::now(), raw_kmh))
}

pub(crate) fn count_stop(raw_kmh: f32) -> Result<(), String> {
    on_run(|run| run.stop_count(Instant::now(), raw_kmh))
}

pub(crate) fn count(state: &OrchestratorState, count: &CalibrationCount) -> Result<(), String> {
    on_run(|run| run.count(count, |kmh| state.calculate_stride_length(kmh)))
}

/// Close the run (saved or cancelled). The belt keeps going.
pub(crate) fn close() {
    with_run(|r| *r = None);
}

/// Each snapshot tick: sample the deck's speed into a count and build the view.
pub fn snapshot(raw_kmh: f32) -> CalibrationSnapshot {
    let run = with_run(|r| {
        r.as_mut().map(|run| {
            run.sample(raw_kmh);
            run.view(Instant::now())
        })
    });
    CalibrationSnapshot {
        wizard_enabled: wizard_enabled(),
        limits: CalibrationLimits::default(),
        run,
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

    fn m(elapsed_s: f32, reported_kmh: f32) -> Measurement {
        Measurement {
            elapsed_s,
            reported_kmh,
        }
    }

    fn loops(loops: f32, steps: Option<u32>) -> CalibrationCount {
        CalibrationCount::Loops {
            loops,
            belt_length_cm: 300.0,
            steps,
        }
    }

    const NO_STRIDE: fn(f32) -> f32 = |_| 74.0;

    #[test]
    fn loops_give_the_scale_and_steps_the_stride() {
        // 3.6 km/h reported for 60 s is 60 m; 25 loops of 3 m is 75 m really.
        let r = point_from_count(m(60.0, 3.6), &loops(25.0, Some(100)), NO_STRIDE).unwrap();
        assert!((r.reported_m - 60.0).abs() < 1e-3);
        assert!((r.real_m - 75.0).abs() < 1e-3);
        assert!((r.point.distance_scale - 0.8).abs() < 1e-4);
        assert!((r.real_kmh - 4.5).abs() < 1e-3);
        assert!((r.point.stride_length_cm - 75.0).abs() < 1e-3);
        assert_eq!(r.point.speed, 3.6);
        assert_eq!(r.cadence_spm, Some(100.0));
        assert!(r.stride_measured);
    }

    #[test]
    fn loops_without_steps_keep_the_current_stride_at_the_real_speed() {
        let seen = std::cell::Cell::new(0.0);
        let r = point_from_count(m(60.0, 3.6), &loops(20.0, None), |kmh| {
            seen.set(kmh);
            66.0
        })
        .unwrap();
        // 60 m really: scale 1.0, real speed 3.6 km/h.
        assert!((r.point.distance_scale - 1.0).abs() < 1e-4);
        assert!((seen.get() - 3.6).abs() < 1e-3);
        assert_eq!(r.point.stride_length_cm, 66.0);
        assert!(!r.stride_measured);
        assert_eq!(r.cadence_spm, None);
    }

    #[test]
    fn steps_of_a_known_stride_give_the_scale() {
        // 80 steps of 75 cm = 60 m; the deck said 3.0 km/h for 60 s = 50 m.
        let c = CalibrationCount::Steps {
            steps: 80,
            stride_cm: 75.0,
        };
        let r = point_from_count(m(60.0, 3.0), &c, NO_STRIDE).unwrap();
        assert!((r.point.distance_scale - 50.0 / 60.0).abs() < 1e-4);
        assert_eq!(r.point.stride_length_cm, 75.0);
    }

    #[test]
    fn the_point_speed_is_the_reported_speed_in_tenths() {
        let r = point_from_count(m(30.0, 2.96), &loops(8.0, None), NO_STRIDE).unwrap();
        assert_eq!(r.point.speed, 3.0);
    }

    #[test]
    fn short_still_or_miscounted_runs_are_refused() {
        assert!(point_from_count(m(10.0, 3.0), &loops(4.0, None), NO_STRIDE).is_err());
        assert!(point_from_count(m(60.0, 0.0), &loops(20.0, None), NO_STRIDE).is_err());
        assert!(point_from_count(m(60.0, 3.0), &loops(0.0, None), NO_STRIDE).is_err());
        assert!(point_from_count(m(60.0, 3.0), &loops(f32::NAN, None), NO_STRIDE).is_err());
        // 50 m reported against 1 loop (3 m): factor 16.7.
        assert!(point_from_count(m(60.0, 3.0), &loops(1.0, None), NO_STRIDE).is_err());
        // Five steps are too few for a stride.
        assert!(point_from_count(m(60.0, 3.0), &loops(17.0, Some(5)), NO_STRIDE).is_err());
        // 50 m in 20 steps is a 250 cm stride.
        assert!(point_from_count(m(60.0, 3.0), &loops(17.0, Some(20)), NO_STRIDE).is_err());
        let bad_belt = CalibrationCount::Loops {
            loops: 17.0,
            belt_length_cm: 10.0,
            steps: None,
        };
        assert!(point_from_count(m(60.0, 3.0), &bad_belt, NO_STRIDE).is_err());
        let bad_stride = CalibrationCount::Steps {
            steps: 80,
            stride_cm: 10.0,
        };
        assert!(point_from_count(m(60.0, 3.0), &bad_stride, NO_STRIDE).is_err());
    }

    #[test]
    fn the_relaxed_limits_take_short_and_long_strides() {
        // A 30 cm shuffle and a 150 cm stride: both outside the old 40 to 130.
        for (steps, stride) in [(250u32, 30.0f32), (50, 150.0)] {
            let real_m = steps as f32 * stride / 100.0;
            let c = CalibrationCount::Loops {
                loops: real_m / 3.0,
                belt_length_cm: 300.0,
                steps: Some(steps),
            };
            let reported = real_m / 60.0 * 3.6;
            let r = point_from_count(m(60.0, reported), &c, NO_STRIDE).unwrap();
            assert!((r.point.stride_length_cm - stride).abs() < 0.01, "{stride}");
        }
        // And a factor of 0.31, as seen on a real deck (issue #62).
        // 3.1 km/h for 60 s is 51.67 m reported; 166.67 m really is 55.56 loops.
        let r = point_from_count(m(60.0, 3.1), &loops(55.5556, None), NO_STRIDE).unwrap();
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
    fn a_run_times_with_its_own_clock_and_refuses_a_still_belt() {
        let t = Instant::now();
        let mut run = Run::new(3.0);
        assert!(
            run.start_count(t, 0.0).is_err(),
            "never counts a still belt"
        );
        assert_eq!(run.view(t).phase, CalibrationPhase::Settling);
        assert!(run.stop_count(t, 3.0).is_err(), "DONE before COUNT");

        run.start_count(t, 3.0).unwrap();
        run.sample(3.0);
        let early = run
            .stop_count(t + Duration::from_secs(10), 3.0)
            .unwrap_err();
        assert!(early.contains("Keep counting"), "{early}");
        assert_eq!(
            run.view(t).phase,
            CalibrationPhase::Counting,
            "DONE too early keeps counting"
        );
        run.sample(3.0);
        let later = t + Duration::from_secs(40);
        assert_eq!(run.view(t + Duration::from_secs(5)).elapsed_s, 5.0);
        run.stop_count(later, 3.0).unwrap();
        let v = run.view(later + Duration::from_secs(99));
        assert_eq!(v.phase, CalibrationPhase::Counted);
        assert_eq!(v.elapsed_s, 40.0, "the clock stops at DONE");
        assert_eq!(v.reported_kmh, Some(3.0));

        // 3.0 km/h for 40 s = 33.3 m reported; 10 loops of 3 m = 30 m.
        run.count(&loops(10.0, Some(45)), NO_STRIDE).unwrap();
        let p = run.view(later).result.unwrap().point;
        assert!((p.distance_scale - 33.333 / 30.0).abs() < 1e-3);
        assert_eq!(p.speed, 3.0);

        // A bad count keeps no result.
        assert!(run.count(&loops(0.0, None), NO_STRIDE).is_err());
        assert_eq!(run.view(later).result, None);
    }

    #[test]
    fn a_belt_that_stops_mid_count_goes_back_to_settling() {
        let t = Instant::now();
        let mut run = Run::new(4.0);
        run.start_count(t, 4.0).unwrap();
        run.sample(4.0);
        run.sample(0.0);
        let e = run
            .stop_count(t + Duration::from_secs(30), 0.0)
            .unwrap_err();
        assert!(e.contains("stopped"), "{e}");
        let v = run.view(t);
        assert_eq!(v.phase, CalibrationPhase::Settling);
        assert_eq!(v.reported_kmh, None, "nothing at 0 km/h to save");
        assert!(run.count(&loops(10.0, None), NO_STRIDE).is_err());
    }

    #[test]
    fn samples_only_count_while_counting() {
        let t = Instant::now();
        let mut run = Run::new(3.0);
        run.sample(0.0); // the belt ramping up before COUNT
        run.start_count(t, 3.0).unwrap();
        run.stop_count(t + Duration::from_secs(20), 3.0).unwrap();
        run.sample(0.0); // he stopped the belt after DONE
        assert_eq!(run.view(t).reported_kmh, Some(3.0));
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
}
