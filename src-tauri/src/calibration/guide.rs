//! GROUND_TRUTH's guided walk: the speeds come from one answer, "What's the
//! fastest you'll walk?". It times the deck's slowest speed and that top
//! speed (one tap run each), then checks the speed halfway between them. The
//! engine draws a straight line between two points (`engine::interpolate`), so
//! the check run measures how far that line is off at the midpoint: the
//! factor the two ends predict there against the factor the taps measure.
//!
//! Within `TOLERANCE` the span is done. Otherwise the midpoint is a point of
//! its own (its run already saved it) and each half is checked the same way,
//! down to `MAX_DEPTH` levels. A span with no deck speed between its ends (in
//! 0.1 km/h steps) needs no check.
//!
//! A check runs at a speed the deck runs: when DECK_PROFILE says the deck
//! skips the halfway speed, the check moves to the nearest one it runs inside
//! the span, and the view says so (`GuideCheck::halfway_kmh`).
//!
//! This is pure planning. Every move to a new speed is the user's RUN press
//! (`calibration::guide_run`); nothing here touches the belt.

use serde::{Deserialize, Serialize};

/// The error a span may have at its midpoint: the owner's 2 % target, about
/// 200 m over 10 km. Each tap run is held to 1 % (`tap::MAX_STANDARD_ERROR`),
/// so a 2 % gap is more than tap noise.
pub const TOLERANCE: f32 = 0.02;

/// How many levels of halving the walk goes down. Three levels split the
/// range into at most eight spans (1 to 6 km/h in steps of about 0.6 km/h,
/// finer than the old 1 km/h list) with at most nine runs. At about three
/// minutes a run that is under the half hour the dashboard pin lasts
/// (`guided_task::TASK_TIMEOUT`). A span still off at the last level is
/// reported as it is.
pub const MAX_DEPTH: u32 = 3;

/// The deck runs and reports speeds in tenths of a km/h, so two speeds closer
/// than this are one speed.
const STEP_KMH: f32 = 0.1;

fn tenths(kmh: f32) -> f32 {
    (kmh * 10.0).round() / 10.0
}

fn same(a: f32, b: f32) -> bool {
    (a - b).abs() < STEP_KMH / 2.0
}

/// The deck speed halfway between `lo` and `hi`, if one fits between them.
pub fn midpoint(lo: f32, hi: f32) -> Option<f32> {
    let mid = tenths((lo + hi) / 2.0);
    (mid - lo >= STEP_KMH - 1e-3 && hi - mid >= STEP_KMH - 1e-3).then_some(mid)
}

/// The speed a check between `lo` and `hi` runs at: the halfway speed when
/// the deck `runs` it, else the nearest speed inside the span it runs (the
/// slower one on a tie, as DECK_PROFILE snaps), with the halfway speed it
/// moved off. None when no speed inside the span runs.
///
/// No cap on how far it moves: any speed inside the span still measures how
/// far the straight line is off there, and the nearest one is where it errs
/// most.
pub fn check_speed(lo: f32, hi: f32, runs: impl Fn(f32) -> bool) -> Option<(f32, Option<f32>)> {
    let half = midpoint(lo, hi)?;
    if runs(half) {
        return Some((half, None));
    }
    let inside = |k: f32| k - lo >= STEP_KMH - 1e-3 && hi - k >= STEP_KMH - 1e-3;
    (1u16..)
        .map(|d| f32::from(d) * STEP_KMH)
        .take_while(|d| inside(tenths(half - d)) || inside(tenths(half + d)))
        .flat_map(|d| [tenths(half - d), tenths(half + d)])
        .find(|k| inside(*k) && runs(*k))
        .map(|k| (k, Some(half)))
}

/// The factor a straight line through `(lo_kmh, lo)` and `(hi_kmh, hi)` gives
/// at `at_kmh`, as the engine interpolates between two points.
pub fn predicted(lo_kmh: f32, lo: f32, hi_kmh: f32, hi: f32, at_kmh: f32) -> f32 {
    if hi_kmh - lo_kmh <= f32::EPSILON {
        return lo;
    }
    let t = ((at_kmh - lo_kmh) / (hi_kmh - lo_kmh)).clamp(0.0, 1.0);
    lo + t * (hi - lo)
}

/// How far off distance is with the `predicted` factor where `measured` is
/// true: real distance is reported distance over the factor, so the error is
/// `measured / predicted - 1`, as a fraction.
pub fn distance_error(predicted: f32, measured: f32) -> f32 {
    if !(predicted.is_finite() && predicted > 0.0 && measured.is_finite()) {
        return f32::INFINITY;
    }
    (measured / predicted - 1.0).abs()
}

/// What one guided speed gave.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// A tap run measured it: the deck's speed during the run and the factor.
    Measured { at_kmh: f32, scale: f32 },
    /// SKIP: no run at the speed.
    Skipped,
}

/// What became of one midpoint check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum CheckVerdict {
    /// Not run yet.
    Pending,
    /// Within `TOLERANCE`: the span is done.
    Agreed,
    /// Off by more: the midpoint is a point, and each half is checked.
    Split,
    /// Off by more at the last level: reported as it is.
    AtLimit,
    /// SKIP on the check run: the span's error is not known.
    Skipped,
}

/// One midpoint check, as the windows see it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct GuideCheck {
    pub lo_kmh: f32,
    pub hi_kmh: f32,
    /// The speed the check's RUN sets: halfway, or the nearest speed the deck
    /// runs when it skips halfway.
    pub mid_kmh: f32,
    /// The halfway speed, when the deck skips it and the check moved off it.
    pub halfway_kmh: Option<f32>,
    pub verdict: CheckVerdict,
    /// The measured distance error at the midpoint, %, once run.
    pub error_pct: Option<f32>,
}

/// One speed of the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum GuideMark {
    Pending,
    Saved,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct GuideSpeed {
    pub kmh: f32,
    pub mark: GuideMark,
    /// The slowest or the top speed; false for a midpoint check.
    pub end: bool,
}

/// The guided walk as the windows see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationGuideView {
    pub slow_kmh: f32,
    pub top_kmh: f32,
    /// Slowest first.
    pub speeds: Vec<GuideSpeed>,
    /// In the order they were opened.
    pub checks: Vec<GuideCheck>,
    /// The speed the next RUN sets; none once the walk is done.
    pub next_kmh: Option<f32>,
    /// The largest error a finished check measured, %; none before any.
    pub remaining_error_pct: Option<f32>,
    /// The last run measured another deck speed than the one asked, and
    /// what the walk did about it.
    pub note: Option<String>,
    /// `TOLERANCE`, %.
    pub tolerance_pct: f32,
}

/// The walk: the two ends and what each speed gave so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Guide {
    slow_kmh: f32,
    top_kmh: f32,
    outcomes: Vec<(f32, Outcome)>,
    note: Option<String>,
    /// The speeds DECK_PROFILE knows the deck skips, in tenths, taken when
    /// the walk opens. Kept fixed for the walk: a check's outcome is filed
    /// under its speed, so the plan must pick the same speeds each time.
    skipped: Vec<f32>,
}

struct Plan {
    speeds: Vec<GuideSpeed>,
    checks: Vec<GuideCheck>,
    next: Option<f32>,
}

impl Guide {
    /// A walk from `slow_kmh` to `top_kmh`, in tenths. Refused unless the top
    /// is above the slowest.
    pub fn new(slow_kmh: f32, top_kmh: f32) -> Result<Self, String> {
        let (slow, top) = (tenths(slow_kmh), tenths(top_kmh));
        if !(top.is_finite() && top > slow + 1e-3) {
            return Err(format!(
                "Pick a top speed above the slowest, {slow:.1} km/h."
            ));
        }
        Ok(Self {
            slow_kmh: slow,
            top_kmh: top,
            outcomes: vec![],
            note: None,
            skipped: vec![],
        })
    }

    /// The walk, picking its checks only from speeds the deck runs:
    /// `skipped_kmh` are the speeds DECK_PROFILE knows it skips.
    pub fn with_skipped(mut self, skipped_kmh: impl IntoIterator<Item = f32>) -> Self {
        self.skipped = skipped_kmh.into_iter().map(tenths).collect();
        self
    }

    fn runs(&self, kmh: f32) -> bool {
        !self.skipped.iter().any(|s| same(*s, kmh))
    }

    fn outcome(&self, kmh: f32) -> Option<Outcome> {
        self.outcomes
            .iter()
            .find(|(k, _)| same(*k, kmh))
            .map(|(_, o)| *o)
    }

    fn set(&mut self, kmh: f32, outcome: Option<Outcome>) {
        self.outcomes.retain(|(k, _)| !same(*k, kmh));
        if let Some(o) = outcome {
            self.outcomes.push((kmh, o));
        }
    }

    /// The run at `target_kmh` was measured and saved: the deck ran
    /// `at_kmh` and the taps gave `scale`.
    ///
    /// A start at the treadmill or its remote can leave the deck at another
    /// speed than RUN asked for. An end then moves to the speed the deck
    /// ran, so the midpoint is planned from it; a check keeps its place and
    /// its prediction uses the speed the deck ran.
    pub fn record(&mut self, target_kmh: f32, at_kmh: f32, scale: f32) {
        if !self.is_speed(target_kmh) {
            return;
        }
        self.note = None;
        let ran = tenths(at_kmh);
        let mut key = target_kmh;
        if !same(ran, target_kmh) {
            let asked = tenths(target_kmh);
            if same(target_kmh, self.slow_kmh) && ran < self.top_kmh - 1e-3 {
                self.set(target_kmh, None);
                self.slow_kmh = ran;
                key = ran;
                self.note = Some(format!(
                    "The deck ran {ran:.1}, not {asked:.1}, so {ran:.1} is the slowest speed."
                ));
            } else if same(target_kmh, self.top_kmh) && ran > self.slow_kmh + 1e-3 {
                self.set(target_kmh, None);
                self.top_kmh = ran;
                key = ran;
                self.note = Some(format!(
                    "The deck ran {ran:.1}, not {asked:.1}, so {ran:.1} is the top speed."
                ));
            } else {
                self.note = Some(format!(
                    "The deck ran {ran:.1}, not {asked:.1}, so the check uses {ran:.1}."
                ));
            }
        }
        self.set(key, Some(Outcome::Measured { at_kmh, scale }));
    }

    /// UNDO or TAP AGAIN took the run at `target_kmh` back: it is up again.
    /// `ran_kmh` is the speed the deck ran, where a moved end now sits.
    pub fn forget(&mut self, target_kmh: f32, ran_kmh: f32) {
        self.note = None;
        self.set(target_kmh, None);
        self.set(tenths(ran_kmh), None);
    }

    /// SKIP on the next speed. Refused once the walk is done.
    pub fn skip_next(&mut self) -> Result<(), String> {
        let next = self.next().ok_or("Every speed is done.")?;
        self.note = None;
        self.set(next, Some(Outcome::Skipped));
        Ok(())
    }

    /// `kmh` is one of the walk's speeds so far.
    fn is_speed(&self, kmh: f32) -> bool {
        self.plan().speeds.iter().any(|s| same(s.kmh, kmh))
    }

    /// The speed the next RUN sets.
    pub fn next(&self) -> Option<f32> {
        self.plan().next
    }

    fn mark(&self, kmh: f32) -> GuideMark {
        match self.outcome(kmh) {
            None => GuideMark::Pending,
            Some(Outcome::Measured { .. }) => GuideMark::Saved,
            Some(Outcome::Skipped) => GuideMark::Skipped,
        }
    }

    fn plan(&self) -> Plan {
        let mut plan = Plan {
            speeds: vec![],
            checks: vec![],
            next: None,
        };
        for end in [self.slow_kmh, self.top_kmh] {
            plan.speeds.push(GuideSpeed {
                kmh: end,
                mark: self.mark(end),
                end: true,
            });
            if self.outcome(end).is_none() && plan.next.is_none() {
                plan.next = Some(end);
            }
        }
        if let (Some(lo), Some(hi)) = (self.measured(self.slow_kmh), self.measured(self.top_kmh)) {
            self.walk((self.slow_kmh, lo), (self.top_kmh, hi), 1, &mut plan);
        }
        plan.speeds.sort_by(|a, b| a.kmh.total_cmp(&b.kmh));
        plan
    }

    fn measured(&self, kmh: f32) -> Option<(f32, f32)> {
        match self.outcome(kmh) {
            Some(Outcome::Measured { at_kmh, scale }) => Some((at_kmh, scale)),
            _ => None,
        }
    }

    /// Check the span from `lo` to `hi` (each its target and `(at_kmh,
    /// scale)`), `depth` levels down, and the halves it splits into.
    fn walk(&self, lo: (f32, (f32, f32)), hi: (f32, (f32, f32)), depth: u32, plan: &mut Plan) {
        let Some((mid, halfway)) = check_speed(lo.0, hi.0, |k| self.runs(k)) else {
            return;
        };
        plan.speeds.push(GuideSpeed {
            kmh: mid,
            mark: self.mark(mid),
            end: false,
        });
        let mut check = GuideCheck {
            lo_kmh: lo.0,
            hi_kmh: hi.0,
            mid_kmh: mid,
            halfway_kmh: halfway,
            verdict: CheckVerdict::Pending,
            error_pct: None,
        };
        match self.outcome(mid) {
            None => {
                plan.next = plan.next.or(Some(mid));
                plan.checks.push(check);
            }
            Some(Outcome::Skipped) => {
                check.verdict = CheckVerdict::Skipped;
                plan.checks.push(check);
            }
            Some(Outcome::Measured { at_kmh, scale }) => {
                let (lo_at, lo_scale) = lo.1;
                let (hi_at, hi_scale) = hi.1;
                let p = predicted(lo_at, lo_scale, hi_at, hi_scale, at_kmh);
                let err = distance_error(p, scale);
                check.error_pct = Some(err * 100.0);
                check.verdict = if err <= TOLERANCE {
                    CheckVerdict::Agreed
                } else if depth >= MAX_DEPTH {
                    CheckVerdict::AtLimit
                } else {
                    CheckVerdict::Split
                };
                plan.checks.push(check);
                if check.verdict == CheckVerdict::Split {
                    let m = (mid, (at_kmh, scale));
                    self.walk(lo, m, depth + 1, plan);
                    self.walk(m, hi, depth + 1, plan);
                }
            }
        }
    }

    pub fn view(&self) -> CalibrationGuideView {
        let plan = self.plan();
        // A split span's error is gone once its halves are points, so only
        // the checks that ended a span count.
        let remaining = plan
            .checks
            .iter()
            .filter(|c| matches!(c.verdict, CheckVerdict::Agreed | CheckVerdict::AtLimit))
            .filter_map(|c| c.error_pct)
            .reduce(f32::max);
        CalibrationGuideView {
            slow_kmh: self.slow_kmh,
            top_kmh: self.top_kmh,
            speeds: plan.speeds,
            checks: plan.checks,
            next_kmh: plan.next,
            remaining_error_pct: remaining,
            tolerance_pct: TOLERANCE * 100.0,
            note: self.note.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deck whose factor at `kmh` is `f(kmh)`: walk the guide, recording
    /// each next speed until it is done. Gives the speeds run, in order.
    fn walk_all(g: &mut Guide, f: impl Fn(f32) -> f32) -> Vec<f32> {
        let mut ran = vec![];
        while let Some(next) = g.next() {
            ran.push(next);
            g.record(next, next, f(next));
            assert!(ran.len() < 20, "the walk ends");
        }
        ran
    }

    #[test]
    fn the_midpoint_is_a_deck_speed_strictly_between() {
        assert_eq!(midpoint(1.0, 6.0), Some(3.5));
        assert_eq!(midpoint(1.0, 1.3), Some(1.2));
        assert_eq!(midpoint(1.0, 1.2), Some(1.1));
        assert_eq!(midpoint(1.0, 1.1), None, "no deck speed between");
    }

    #[test]
    fn the_prediction_is_the_straight_line_and_the_error_is_in_distance() {
        assert!((predicted(1.0, 0.8, 5.0, 1.2, 3.0) - 1.0).abs() < 1e-6);
        assert!((distance_error(1.0, 1.02) - 0.02).abs() < 1e-6);
        assert!((distance_error(1.0, 0.97) - 0.03).abs() < 1e-6);
        assert!(distance_error(0.0, 1.0).is_infinite());
    }

    #[test]
    fn a_top_speed_must_be_above_the_slowest() {
        assert!(Guide::new(1.0, 1.0).is_err());
        assert!(Guide::new(1.0, 0.5).is_err());
        assert!(Guide::new(1.0, f32::NAN).is_err());
        assert!(Guide::new(1.0, 1.1).is_ok());
    }

    #[test]
    fn the_ends_run_first_then_the_midpoint() {
        let mut g = Guide::new(1.0, 6.0).unwrap();
        assert_eq!(g.next(), Some(1.0));
        g.record(1.0, 1.0, 0.9);
        assert_eq!(g.next(), Some(6.0));
        assert_eq!(g.view().checks, vec![], "no check before both ends");
        g.record(6.0, 6.0, 1.1);
        assert_eq!(g.next(), Some(3.5));
        let c = g.view().checks[0];
        assert_eq!(
            (c.lo_kmh, c.hi_kmh, c.mid_kmh, c.verdict),
            (1.0, 6.0, 3.5, CheckVerdict::Pending)
        );
    }

    #[test]
    fn a_straight_deck_is_done_after_one_check() {
        let mut g = Guide::new(1.0, 6.0).unwrap();
        let ran = walk_all(&mut g, |k| 0.8 + 0.05 * k);
        assert_eq!(ran, vec![1.0, 6.0, 3.5]);
        let v = g.view();
        assert_eq!(v.checks.len(), 1);
        assert_eq!(v.checks[0].verdict, CheckVerdict::Agreed);
        assert!(v.remaining_error_pct.unwrap() < 1e-3);
        assert_eq!(v.next_kmh, None);
        assert!(v.speeds.iter().all(|s| s.mark == GuideMark::Saved));
    }

    #[test]
    fn within_two_percent_is_done_and_past_it_splits() {
        // 1.9 % off at the midpoint: done.
        let mut g = Guide::new(1.0, 6.0).unwrap();
        g.record(1.0, 1.0, 1.0);
        g.record(6.0, 6.0, 1.0);
        g.record(3.5, 3.5, 1.019);
        assert_eq!(g.view().checks[0].verdict, CheckVerdict::Agreed);
        assert_eq!(g.next(), None);
        // 2.5 % off: the midpoint is a point and both halves are checked.
        g.record(3.5, 3.5, 1.025);
        let v = g.view();
        assert_eq!(v.checks[0].verdict, CheckVerdict::Split);
        assert_eq!(g.next(), Some(2.3), "the slower half first");
        let mids: Vec<f32> = v.checks.iter().map(|c| c.mid_kmh).collect();
        assert_eq!(mids, vec![3.5, 2.3, 4.8]);
    }

    #[test]
    fn a_bent_deck_is_split_where_it_bends_and_converges() {
        // Straight from 1 to 3.5 km/h, then a knee: flat after 3.5.
        let f = |k: f32| {
            if k <= 3.5 {
                0.8 + 0.08 * (k - 1.0)
            } else {
                1.0
            }
        };
        let mut g = Guide::new(1.0, 6.0).unwrap();
        let ran = walk_all(&mut g, f);
        // The ends, the midpoint (off), then each half, both straight now.
        assert_eq!(ran, vec![1.0, 6.0, 3.5, 2.3, 4.8]);
        let v = g.view();
        let verdicts: Vec<CheckVerdict> = v.checks.iter().map(|c| c.verdict).collect();
        assert_eq!(
            verdicts,
            vec![
                CheckVerdict::Split,
                CheckVerdict::Agreed,
                CheckVerdict::Agreed
            ]
        );
        assert!(v.remaining_error_pct.unwrap() <= 2.0);
    }

    #[test]
    fn a_curve_that_never_settles_stops_at_the_last_level() {
        // Bent everywhere: every check is off by far more than 2 %.
        let f = |k: f32| 0.5 + 0.3 * (k * 2.0).sin().abs() + 0.05 * k * k;
        let mut g = Guide::new(1.0, 6.0).unwrap();
        let ran = walk_all(&mut g, f);
        assert!(ran.len() <= 2 + 1 + 2 + 4, "at most nine runs, got {ran:?}");
        let v = g.view();
        assert!(v
            .checks
            .iter()
            .all(|c| c.verdict != CheckVerdict::Pending && c.verdict != CheckVerdict::Skipped));
        assert_eq!(v.next_kmh, None);
        if v.checks.iter().any(|c| c.verdict == CheckVerdict::AtLimit) {
            assert!(v.remaining_error_pct.unwrap() > 2.0, "reported as it is");
        }
    }

    #[test]
    fn a_narrow_range_has_no_check() {
        let mut g = Guide::new(1.0, 1.1).unwrap();
        let ran = walk_all(&mut g, |_| 1.0);
        assert_eq!(ran, vec![1.0, 1.1]);
        let v = g.view();
        assert!(v.checks.is_empty());
        assert_eq!(v.remaining_error_pct, None);
    }

    /// #169 follow-up: RUN asked 0.6, a start at the treadmill ran 1.2. The
    /// slowest end moves to 1.2, the 0.6 row is gone, and the midpoint is
    /// planned from 1.2. UNDO puts 1.2 back up as the slowest.
    #[test]
    fn an_end_the_deck_ran_at_another_speed_moves_to_that_speed() {
        let mut g = Guide::new(0.6, 4.0).unwrap();
        g.record(0.6, 1.2, 0.8);
        let v = g.view();
        assert_eq!(v.slow_kmh, 1.2);
        assert!(!v.speeds.iter().any(|s| same(s.kmh, 0.6)));
        assert!(v
            .speeds
            .iter()
            .any(|s| same(s.kmh, 1.2) && s.mark == GuideMark::Saved));
        assert_eq!(v.next_kmh, Some(4.0));
        assert!(v.note.as_deref().unwrap().contains("1.2 is the slowest"));
        g.record(4.0, 4.0, 0.8);
        assert_eq!(g.next(), Some(midpoint(1.2, 4.0).unwrap()));
        assert_eq!(g.view().note, None);
        g.forget(1.2, 1.2);
        assert_eq!(g.next(), Some(1.2));
        // A top end the deck ran lower moves down; a check keeps its place.
        let mut g = Guide::new(1.0, 6.0).unwrap();
        g.record(1.0, 1.0, 1.0);
        g.record(6.0, 5.8, 1.0);
        assert_eq!(g.view().top_kmh, 5.8);
        let mid = g.next().unwrap();
        g.record(mid, mid + 0.1, 1.0);
        let v = g.view();
        assert_eq!(v.checks[0].verdict, CheckVerdict::Agreed);
        assert!(v.note.unwrap().contains("the check uses"));
    }

    #[test]
    fn undo_puts_a_speed_back_and_skip_leaves_the_span_unknown() {
        let mut g = Guide::new(1.0, 6.0).unwrap();
        g.record(1.0, 1.0, 1.0);
        g.record(6.0, 6.0, 1.0);
        g.forget(6.0, 6.0);
        assert_eq!(g.next(), Some(6.0), "UNDO: the top speed is up again");
        g.record(6.0, 6.0, 1.0);
        g.skip_next().unwrap();
        let v = g.view();
        assert_eq!(v.checks[0].verdict, CheckVerdict::Skipped);
        assert_eq!(v.next_kmh, None);
        assert_eq!(v.remaining_error_pct, None);
        assert!(g.skip_next().is_err());
        // A speed the walk never asked for is not recorded.
        g.record(2.0, 2.0, 0.5);
        assert!(!g.view().speeds.iter().any(|s| s.kmh == 2.0));
    }

    #[test]
    fn the_prediction_uses_the_speeds_the_deck_really_ran() {
        // The deck ran 3.4 for the 3.5 check: the line is read at 3.4.
        let mut g = Guide::new(1.0, 6.0).unwrap();
        g.record(1.0, 1.0, 1.0);
        g.record(6.0, 6.0, 1.5);
        g.record(3.5, 3.4, 1.24);
        let c = g.view().checks[0];
        assert_eq!(c.verdict, CheckVerdict::Agreed);
        assert!(c.error_pct.unwrap() < 0.1, "{:?}", c.error_pct);
    }
    /// #166: a check runs at a speed the deck runs. Halfway when it runs,
    /// else the nearest inside the span (the slower on a tie), none when no
    /// speed inside runs.
    #[test]
    fn a_check_picks_a_speed_the_deck_runs() {
        let all = |_: f32| true;
        assert_eq!(check_speed(1.0, 6.0, all), Some((3.5, None)));
        let skips = |s: &'static [f32]| move |k: f32| !s.iter().any(|x| same(*x, k));
        assert_eq!(check_speed(1.0, 6.0, skips(&[3.5])), Some((3.4, Some(3.5))));
        assert_eq!(
            check_speed(1.0, 6.0, skips(&[3.4, 3.5])),
            Some((3.6, Some(3.5)))
        );
        // Only the ends' neighbours are left: still inside the span.
        assert_eq!(
            check_speed(1.0, 1.4, skips(&[1.2, 1.1])),
            Some((1.3, Some(1.2)))
        );
        assert_eq!(check_speed(1.0, 1.3, skips(&[1.1, 1.2])), None);
        assert_eq!(check_speed(1.0, 1.1, all), None, "no deck speed between");
    }

    #[test]
    fn the_walk_moves_a_check_off_a_speed_the_deck_skips_and_says_so() {
        let mut g = Guide::new(1.0, 6.0).unwrap().with_skipped([3.5, 2.2]);
        g.record(1.0, 1.0, 1.0);
        g.record(6.0, 6.0, 1.0);
        assert_eq!(g.next(), Some(3.4));
        let c = g.view().checks[0];
        assert_eq!((c.mid_kmh, c.halfway_kmh), (3.4, Some(3.5)));
        assert!(g.view().speeds.iter().any(|s| same(s.kmh, 3.4)));
        assert!(!g.view().speeds.iter().any(|s| same(s.kmh, 3.5)));
        // Split: each half is checked at a speed the deck runs too.
        g.record(3.4, 3.4, 1.05);
        assert_eq!(g.view().checks[0].verdict, CheckVerdict::Split);
        let c = g.view().checks[1];
        assert_eq!((c.lo_kmh, c.hi_kmh), (1.0, 3.4));
        assert_eq!((c.mid_kmh, c.halfway_kmh), (2.1, Some(2.2)));
        assert_eq!(g.view().checks[2].halfway_kmh, None);
        // A whole walk on such a deck never asks for a skipped speed.
        let mut g = Guide::new(0.6, 5.0).unwrap().with_skipped([2.8, 1.7, 1.5]);
        let bent = |k: f32| if k < 2.8 { 0.8 + 0.06 * k } else { 1.0 };
        let ran = walk_all(&mut g, bent);
        assert!(
            ran.iter()
                .all(|k| ![2.8, 1.7, 1.5].iter().any(|s| same(*s, *k))),
            "{ran:?}"
        );
        assert!(ran.len() > 3, "the bend still splits: {ran:?}");
    }
}
