//! GROUND_TRUTH's footstep phase: after a belt run has timed the belt at a
//! speed, the user taps each footstep (both feet) for about 20 steps. The
//! same inputs and the same maths as the belt taps (`tap.rs`): missed and
//! doubled taps are dropped against the median time between steps, and the
//! kept times give the cadence. Their mean is used, not their median: left
//! and right steps alternate two slightly different times, and a median
//! picks one foot's time whenever the count is uneven, while the mean sits
//! between the two. The stride is the belt's measured speed over that cadence,
//! saved by itself for the run's speed with the belt's factor left as it is
//! (`averaged_point(.., factor_counted = false)`), through the same save and
//! UNDO as a belt run (`with_result_saved`, `with_save_undone`). Tapping only
//! reads the deck's speed; nothing here ever sends anything to the treadmill.

use super::tap::{self, Rules};
use super::{
    averaged_point, in_range, summarize_samples, CalibrationPhase, CalibrationResult, SavedPoint,
    MAX_STRIDE_CM, MIN_STRIDE_CM, SAME_SPEED_KMH, STOPPED_BELOW_KMH,
};
use crate::protocol::CalibrationPoint;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Kept times between steps a step run needs. Left and right steps often
/// differ by a few percent; twenty times are ten of each foot, so the median
/// sits between the two. At a walk's two steps a second they take about 10 s,
/// the "about 20 steps" the step asks for.
pub const MIN_STEPS: usize = 20;

/// The kept times must agree this closely: their standard error over the
/// median. The stride is the belt's speed over the cadence, so the two errors
/// add: the belt is held to 1 % (`tap::MAX_STANDARD_ERROR`), and 1.5 % here
/// keeps both together under the 2 % the calibration aims for (the root of
/// 1² + 1.5² is about 1.8). A footstep lands less evenly than a mark on a
/// belt, so a tighter bound would keep a walker tapping for a minute or more.
pub const MAX_STEP_STANDARD_ERROR: f32 = 0.015;

/// The rules a step run finishes by.
pub const STEPS: Rules = Rules {
    min_agreeing: MIN_STEPS,
    max_standard_error: MAX_STEP_STANDARD_ERROR,
};

/// Cadence (steps a minute) and stride (cm) from a belt really moving at
/// `real_kmh` and a typical `step_s` between footsteps. A stride
/// outside `MIN_STRIDE_CM..=MAX_STRIDE_CM` is refused with the reason.
pub fn stride_from(real_kmh: f32, step_s: f64) -> Result<(f32, f32), String> {
    if !(step_s.is_finite() && step_s > 0.0) {
        return Err("Tap each footstep, left and right.".into());
    }
    let cadence_spm = (60.0 / step_s) as f32;
    let stride_cm = (real_kmh as f64 / 3.6 * step_s * 100.0) as f32;
    if !in_range(stride_cm, MIN_STRIDE_CM, MAX_STRIDE_CM) {
        return Err(format!(
            "The taps give a stride of {stride_cm:.0} cm, outside {MIN_STRIDE_CM:.0} to {MAX_STRIDE_CM:.0} cm, so tapping starts again. Tap every footstep, left and right."
        ));
    }
    Ok((cadence_spm, stride_cm))
}

/// One step run, kept inside the belt run it follows.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRun {
    /// The belt run's result: the real speed the stride comes from.
    belt: CalibrationResult,
    taps: Vec<Instant>,
    /// The deck's reported speed while the user taps.
    samples: Vec<f32>,
    note: Option<String>,
    result: Option<CalibrationResult>,
    /// What the stride's save wrote, until UNDO puts it back.
    saved: Option<SavedPoint>,
    /// UNDO was pressed on this stride.
    undone: bool,
}

impl StepRun {
    pub fn new(belt: CalibrationResult) -> Self {
        Self {
            belt,
            taps: vec![],
            samples: vec![],
            note: None,
            result: None,
            saved: None,
            undone: false,
        }
    }

    pub fn listening(&self) -> bool {
        self.result.is_none()
    }

    pub fn result(&self) -> Option<&CalibrationResult> {
        self.result.as_ref()
    }

    fn tap_seconds(&self) -> Vec<f64> {
        let Some(first) = self.taps.first() else {
            return vec![];
        };
        self.taps
            .iter()
            .map(|t| t.duration_since(*first).as_secs_f64())
            .collect()
    }

    fn restart(&mut self, why: impl Into<String>) {
        self.taps.clear();
        self.samples.clear();
        self.note = Some(why.into());
    }

    /// The deck's speeds so far, checked against the speed the belt run
    /// timed: the stride needs the belt at that same speed.
    fn check_speed(&self) -> Result<f32, String> {
        let mut all = vec![self.belt.reported_kmh];
        all.extend_from_slice(&self.samples);
        summarize_samples(&all).map_err(|_| {
            format!(
                "The belt left the timed {:.1} km/h, so step tapping starts again.",
                self.belt.reported_kmh
            )
        })
    }

    /// One footstep at `now`, with the deck reporting `raw_kmh`. `points` are
    /// the saved points, read when the steps agree. Never touches the belt.
    pub fn tap(
        &mut self,
        now: Instant,
        raw_kmh: f32,
        target_kmh: f32,
        points: &[CalibrationPoint],
    ) {
        if self.result.is_some() {
            self.note = Some("The stride is measured. TAP AGAIN starts the steps over.".into());
            return;
        }
        if raw_kmh < STOPPED_BELOW_KMH {
            self.restart("The belt is not moving, so step tapping starts again.");
            return;
        }
        if let Some(last) = self.taps.last() {
            if now.duration_since(*last).as_secs_f64() < tap::DOUBLE_TAP_S {
                return;
            }
        }
        if self.taps.len() == 1 {
            self.note = None;
        }
        self.taps.push(now);
        self.samples.push(raw_kmh);
        if let Err(why) = self.check_speed() {
            self.restart(why);
            return;
        }
        self.try_finish(target_kmh, points);
    }

    /// The deck's speed each snapshot tick while tapping.
    pub fn sample(&mut self, raw_kmh: f32) {
        if self.taps.is_empty() || self.result.is_some() {
            return;
        }
        self.samples.push(raw_kmh);
        if let Err(why) = self.check_speed() {
            self.restart(why);
        }
    }

    fn try_finish(&mut self, target_kmh: f32, points: &[CalibrationPoint]) {
        let secs = self.tap_seconds();
        let stats = tap::analyse_with(&secs, &STEPS);
        let (true, Some(step_s)) = (stats.settled, stats.mean_s) else {
            return;
        };
        let (cadence_spm, stride_cm) = match stride_from(self.belt.real_kmh, step_s) {
            Ok(v) => v,
            Err(why) => return self.restart(why),
        };
        let reported_kmh = self.check_speed().unwrap_or(self.belt.reported_kmh);
        let elapsed_s = secs.last().copied().unwrap_or(0.0) as f32;
        let mut r = CalibrationResult {
            point: CalibrationPoint {
                stride_length_cm: stride_cm,
                runs: 1,
                ..self.belt.point.clone()
            },
            real_kmh: self.belt.real_kmh,
            reported_kmh,
            real_m: self.belt.real_kmh / 3.6 * elapsed_s,
            reported_m: reported_kmh / 3.6 * elapsed_s,
            cadence_spm: Some(cadence_spm),
            stride_measured: true,
            averaged: None,
        };
        let saved = points
            .iter()
            .find(|p| (p.speed - target_kmh).abs() <= SAME_SPEED_KMH);
        // The stride joins the saved point; its factor stays as it is. The
        // belt run already counted this run when it saved, so the count of
        // runs stays too.
        r.averaged = saved.map(|s| CalibrationPoint {
            runs: s.runs,
            ..averaged_point(s, &r, false)
        });
        log::info!(
            "Calibration steps at {target_kmh:.1} km/h measured: {cadence_spm:.0} steps a minute, stride {stride_cm:.1} cm over {} steps",
            self.taps.len()
        );
        self.result = Some(r);
        self.note = None;
    }

    /// The stride's save wrote `saved`.
    pub(super) fn set_saved(&mut self, saved: SavedPoint) {
        self.saved = Some(saved);
    }

    /// What the stride's save wrote, taken for UNDO.
    pub(super) fn take_saved(&mut self) -> Option<SavedPoint> {
        self.saved.take()
    }

    pub(super) fn mark_undone(&mut self) {
        self.undone = true;
    }

    pub fn view(&self, now: Instant) -> StepRunView {
        let secs = self.tap_seconds();
        let stats = tap::analyse_with(&secs, &STEPS);
        let elapsed_s = match (self.taps.first(), self.result.is_some()) {
            (Some(_), true) => secs.last().copied().unwrap_or(0.0) as f32,
            (Some(first), false) => now.duration_since(*first).as_secs_f32(),
            (None, _) => 0.0,
        };
        StepRunView {
            phase: match (self.result.is_some(), self.taps.is_empty()) {
                (true, _) => CalibrationPhase::Measured,
                (false, true) => CalibrationPhase::Settling,
                (false, false) => CalibrationPhase::Tapping,
            },
            steps: self.taps.len() as u32,
            kept: stats.kept as u32,
            dropped: stats.dropped as u32,
            elapsed_s,
            cadence_spm: stats.mean_s.map(|m| (60.0 / m) as f32),
            stride_cm: stats
                .mean_s
                .map(|m| (self.belt.real_kmh as f64 / 3.6 * m * 100.0) as f32),
            note: self.note.clone(),
            result: self.result.clone(),
            saved: self.saved.is_some(),
            undone: self.undone,
        }
    }
}

/// The footstep phase as the windows see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct StepRunView {
    /// Settling: no step tapped yet; Tapping; Measured: the steps agreed.
    pub phase: CalibrationPhase,
    /// Footsteps tapped since the tapping last started over.
    pub steps: u32,
    /// Times between steps that agree with the median.
    pub kept: u32,
    /// Times dropped as a missed or doubled tap.
    pub dropped: u32,
    /// Seconds since the first step (to the last once measured).
    pub elapsed_s: f32,
    /// Steps a minute from the taps so far.
    pub cadence_spm: Option<f32>,
    /// The stride from the taps so far, cm.
    pub stride_cm: Option<f32>,
    pub note: Option<String>,
    /// The measured stride, cadence and point, once the steps agree.
    pub result: Option<CalibrationResult>,
    /// The stride is saved, and UNDO can put the point back.
    pub saved: bool,
    /// UNDO put the point back: the stride is not saved.
    pub undone: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A belt run at 3.0 km/h on the deck that really ran 2.7 km/h.
    fn belt() -> CalibrationResult {
        super::super::point_from_taps(300.0, 4.0, 36.0, 3.0, |_| 74.0).unwrap()
    }

    fn pt(speed: f32, stride: f32, scale: f32, runs: u32) -> CalibrationPoint {
        CalibrationPoint {
            speed,
            stride_length_cm: stride,
            distance_scale: scale,
            runs,
        }
    }

    /// `n` footsteps every `gap` s from `t`, with the deck at `kmh`.
    fn steps(
        s: &mut StepRun,
        t: Instant,
        gap: f64,
        n: usize,
        kmh: f32,
        points: &[CalibrationPoint],
    ) {
        for i in 0..n {
            s.tap(
                t + Duration::from_secs_f64(i as f64 * gap),
                kmh,
                3.0,
                points,
            );
        }
    }

    #[test]
    fn cadence_is_the_median_step_and_stride_is_speed_over_cadence() {
        // 2.7 km/h is 0.75 m/s; a step every 0.6 s is 100 a minute and 45 cm.
        let (cadence, stride) = stride_from(2.7, 0.6).unwrap();
        assert!((cadence - 100.0).abs() < 1e-3);
        assert!((stride - 45.0).abs() < 1e-3);
        assert!(stride_from(2.7, f64::NAN).is_err());
        assert!(stride_from(2.7, 0.0).is_err());
    }

    #[test]
    fn a_stride_outside_the_limits_is_refused_in_plain_words() {
        // Every other footstep tapped at 6 km/h: 1.4 s apart is 233 cm.
        let e = stride_from(6.0, 1.4).unwrap_err();
        assert!(e.contains("233 cm, outside 20 to 200 cm"), "{e}");
        assert!(e.contains("every footstep"), "{e}");
        // A shuffle at 0.6 km/h, 0.1 s apart, is 1.7 cm.
        assert!(stride_from(0.6, 0.1).is_err());
        assert!(stride_from(0.6, 1.25).is_ok(), "21 cm, just over the floor");
    }

    #[test]
    fn steps_finish_by_themselves_after_twenty_agreeing_times() {
        let t = Instant::now();
        let mut s = StepRun::new(belt());
        assert_eq!(s.view(t).phase, CalibrationPhase::Settling);
        steps(&mut s, t, 0.6, 20, 3.0, &[]);
        let v = s.view(t);
        assert_eq!(
            (v.phase, v.steps, v.kept),
            (CalibrationPhase::Tapping, 20, 19)
        );
        assert!(
            (v.cadence_spm.unwrap() - 100.0).abs() < 1e-3,
            "live cadence"
        );
        assert!((v.stride_cm.unwrap() - 45.0).abs() < 1e-3, "live stride");
        assert!(s.listening());
        s.tap(t + Duration::from_secs_f64(12.0), 3.0, 3.0, &[]);
        let v = s.view(t);
        assert_eq!(v.phase, CalibrationPhase::Measured);
        let r = v.result.unwrap();
        assert_eq!(r.cadence_spm.map(|c| c.round()), Some(100.0));
        assert!(r.stride_measured);
        assert!((r.point.stride_length_cm - 45.0).abs() < 1e-3);
        // The belt's factor and speed carry over.
        assert_eq!(r.point.distance_scale, belt().point.distance_scale);
        assert_eq!(r.point.speed, 3.0);
        assert!(!s.listening());
        assert_eq!(r.averaged, None, "no saved point at 3.0 km/h");
    }

    #[test]
    fn the_stride_joins_the_saved_point_and_the_factor_stays() {
        let t = Instant::now();
        let saved = pt(3.0, 70.0, 0.9, 1);
        let mut s = StepRun::new(belt());
        steps(
            &mut s,
            t,
            0.6,
            21,
            3.0,
            &[pt(4.0, 80.0, 1.1, 1), saved.clone()],
        );
        let v = s.view(t);
        let p = v.result.unwrap().averaged.unwrap();
        assert_eq!(p.distance_scale, 0.9, "the factor stays");
        assert!((p.stride_length_cm - (70.0 + 45.0) / 2.0).abs() < 1e-3);
        assert_eq!(p.runs, saved.runs, "the belt save counted the run");
    }

    #[test]
    fn missed_and_doubled_steps_are_dropped_and_honest_jitter_is_kept() {
        let t = Instant::now();
        let mut s = StepRun::new(belt());
        // ±15 ms on each step at 0.6 s, one step missed and one tapped twice.
        let mut at = vec![];
        for i in 0..26 {
            let j = if i % 2 == 0 { 0.015 } else { -0.015 };
            at.push(i as f64 * 0.6 + j);
        }
        at.remove(9);
        at.insert(15, at[14] + 0.3);
        for a in at {
            s.tap(t + Duration::from_secs_f64(a), 3.0, 3.0, &[]);
        }
        let v = s.view(t);
        assert_eq!(v.phase, CalibrationPhase::Measured, "{v:?}");
        assert!(v.dropped >= 2);
        let stride = v.result.unwrap().point.stride_length_cm;
        assert!((stride - 45.0).abs() < 0.5, "{stride}");
    }

    #[test]
    fn uneven_left_and_right_steps_give_the_stride_between_them() {
        // Left 0.57 s, right 0.63 s, 21 steps: 11 short gaps and 10 long. A
        // median would say 0.57 s (42.8 cm); the mean says 0.6 s.
        let t = Instant::now();
        let mut s = StepRun::new(belt());
        let mut at = 0.0;
        for i in 0..22 {
            s.tap(t + Duration::from_secs_f64(at), 3.0, 3.0, &[]);
            at += if i % 2 == 0 { 0.57 } else { 0.63 };
        }
        let r = s.view(t).result.expect("settled");
        assert!(
            (r.point.stride_length_cm - 45.0).abs() < 0.2,
            "{}",
            r.point.stride_length_cm
        );
    }

    #[test]
    fn every_other_footstep_is_refused_and_starts_over() {
        // 1.6 s between taps at 6 km/h real: 267 cm.
        let mut b = belt();
        b.real_kmh = 6.0;
        let t = Instant::now();
        let mut s = StepRun::new(b);
        steps(&mut s, t, 1.6, 21, 3.0, &[]);
        let v = s.view(t);
        assert_eq!(v.result, None);
        assert_eq!(v.steps, 0);
        assert!(v.note.unwrap().contains("every footstep"));
    }

    #[test]
    fn a_belt_that_stops_or_leaves_the_timed_speed_starts_the_steps_over() {
        let t = Instant::now();
        let mut s = StepRun::new(belt());
        steps(&mut s, t, 0.6, 5, 3.0, &[]);
        s.sample(3.5);
        let v = s.view(t);
        assert_eq!(v.steps, 0);
        assert!(v.note.unwrap().contains("timed 3.0 km/h"));
        s.tap(t, 0.0, 3.0, &[]);
        assert!(s.view(t).note.unwrap().contains("not moving"));
        assert_eq!(s.view(t).steps, 0);
    }
}
