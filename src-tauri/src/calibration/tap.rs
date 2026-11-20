//! Timing taps: the pure maths behind GROUND_TRUTH's tap runs.
//!
//! The user taps each time something repeats (the belt mark passing, and in
//! a later step each footstep). Rust timestamps every tap; this turns the
//! timestamps into one typical interval. The typical interval is the median
//! of the gaps between taps: a missed tap (one gap twice as long) or a doubled
//! one (a gap split in two) barely moves a median, and is then dropped as an
//! outlier against it. A run is done once enough kept gaps agree closely.

/// Two taps closer than this are one press: a key or button that bounced, or
/// a finger that hit twice. The later one is ignored, so the first keeps its
/// timing. Shorter than any real gap: a 2 m belt loop at 20 km/h passes every
/// 0.36 s, and footsteps at a brisk walk come about every 0.5 s.
pub const DOUBLE_TAP_S: f64 = 0.25;

/// A gap further than this fraction from the median is an outlier. A tap
/// lands within about 50 ms of the moment it marks, so an honest gap of 1 s
/// (a fast pass) wanders by up to about 10 %; a missed tap is +100 % and a tap
/// split in two leaves parts far short of the median.
pub const OUTLIER_FRACTION: f32 = 0.15;

/// Kept gaps a run needs before it can finish. With about 50 ms of jitter on
/// each tap, eight gaps of a 3 s belt loop put the median within about 1 %.
pub const MIN_AGREEING: usize = 8;

/// The kept gaps must also agree this closely: their standard error, as a
/// fraction of the median. 1 % is half the 2 % accuracy the calibration aims
/// for (issue #140), so tap timing is never the larger part of the error. A
/// fast belt with short gaps simply needs a few more passes to get there.
pub const MAX_STANDARD_ERROR: f32 = 0.01;

/// At most this share of all gaps may be dropped. A missed or doubled tap is
/// the odd one out; once as many gaps disagree as agree (taps that keep
/// landing at two different points of the loop), the median picks one side by
/// chance and the run waits for steadier taps. A quarter still lets one slip
/// in every four passes through.
pub const MAX_DROPPED_SHARE: f32 = 0.25;

/// What a set of taps says so far.
#[derive(Debug, Clone, PartialEq)]
pub struct TapStats {
    /// Gaps between taps that agree with the median.
    pub kept: usize,
    /// Gaps dropped as a missed or doubled tap.
    pub dropped: usize,
    /// The median of the kept gaps, s; none until there is a gap.
    pub median_s: Option<f64>,
    /// Standard error of the kept gaps over their median; none below two.
    pub standard_error: Option<f32>,
    /// Enough kept gaps agree closely, and few enough were dropped: the run
    /// can finish.
    pub settled: bool,
}

/// The gaps between consecutive tap times, s.
pub fn gaps(taps_s: &[f64]) -> Vec<f64> {
    taps_s.windows(2).map(|w| w[1] - w[0]).collect()
}

/// The median of `v`; none when empty.
pub fn median(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let n = s.len();
    Some(if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    })
}

/// Gaps within `OUTLIER_FRACTION` of the median of all of them.
pub fn kept_gaps(gaps: &[f64]) -> Vec<f64> {
    let Some(m) = median(gaps) else {
        return vec![];
    };
    gaps.iter()
        .copied()
        .filter(|g| ((g - m) / m).abs() as f32 <= OUTLIER_FRACTION)
        .collect()
}

/// Everything the taps at `taps_s` (seconds on one clock, in order) say.
pub fn analyse(taps_s: &[f64]) -> TapStats {
    let all = gaps(taps_s);
    let kept = kept_gaps(&all);
    let median_s = median(&kept);
    let standard_error = match (median_s, kept.len()) {
        (Some(m), n) if n >= 2 => {
            let mean = kept.iter().sum::<f64>() / n as f64;
            let var = kept.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
            Some((var.sqrt() / (n as f64).sqrt() / m) as f32)
        }
        _ => None,
    };
    TapStats {
        kept: kept.len(),
        dropped: all.len() - kept.len(),
        median_s,
        standard_error,
        settled: kept.len() >= MIN_AGREEING
            && standard_error.is_some_and(|se| se <= MAX_STANDARD_ERROR)
            && (all.len() - kept.len()) as f32 <= MAX_DROPPED_SHARE * all.len() as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Taps every `gap` s from 0, with `jitter[i]` added to tap i.
    fn taps(gap: f64, n: usize, jitter: &[f64]) -> Vec<f64> {
        (0..n)
            .map(|i| i as f64 * gap + jitter.get(i).copied().unwrap_or(0.0))
            .collect()
    }

    #[test]
    fn the_median_is_the_middle_value_or_the_mean_of_the_two_middle_ones() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3.0]), Some(3.0));
        assert_eq!(median(&[5.0, 1.0, 3.0]), Some(3.0));
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
    }

    #[test]
    fn steady_taps_settle_after_eight_gaps() {
        let s = analyse(&taps(3.0, 8, &[]));
        assert_eq!((s.kept, s.dropped, s.settled), (7, 0, false), "seven gaps");
        let s = analyse(&taps(3.0, 9, &[]));
        assert_eq!((s.kept, s.dropped), (8, 0));
        assert_eq!(s.median_s, Some(3.0));
        assert!(s.settled);
    }

    #[test]
    fn a_missed_tap_is_dropped_and_the_median_holds() {
        // Ten passes at 3 s, but the tap at 15 s never came: one 6 s gap.
        let mut t = taps(3.0, 11, &[]);
        t.remove(5);
        let s = analyse(&t);
        assert_eq!((s.kept, s.dropped), (8, 1));
        assert_eq!(s.median_s, Some(3.0));
        assert!(s.settled);
    }

    #[test]
    fn a_doubled_tap_is_dropped_too() {
        // An extra tap 1.2 s after the one at 9 s splits that gap in two.
        let mut t = taps(3.0, 11, &[]);
        t.insert(4, 10.2);
        let s = analyse(&t);
        assert_eq!((s.kept, s.dropped), (9, 2));
        assert_eq!(s.median_s, Some(3.0));
    }

    #[test]
    fn honest_jitter_is_kept_and_wild_taps_are_not() {
        // ±50 ms on each tap at a 3 s pass: all kept, settled, close to 3 s.
        let jitter = [0.0, 0.05, -0.04, 0.03, -0.05, 0.02, 0.04, -0.03, 0.01];
        let s = analyse(&taps(3.0, 9, &jitter));
        assert_eq!(s.dropped, 0);
        assert!(s.settled, "{:?}", s.standard_error);
        assert!((s.median_s.unwrap() - 3.0).abs() < 0.05);
        // Taps all over the place never agree, however many there are.
        let wild: Vec<f64> = (0..30)
            .scan(0.0, |t, i| {
                *t += if i % 2 == 0 { 2.0 } else { 3.3 };
                Some(*t)
            })
            .collect();
        assert!(!analyse(&wild).settled);
    }

    #[test]
    fn short_gaps_need_closer_agreement_before_settling() {
        // A 0.9 s pass with ±20 ms jitter: eight gaps are not yet within 1 %.
        let jitter: Vec<f64> = (0..40)
            .map(|i| if i % 2 == 0 { 0.02 } else { -0.02 })
            .collect();
        assert!(!analyse(&taps(0.9, 9, &jitter)).settled);
        assert!(analyse(&taps(0.9, 40, &jitter)).settled);
    }

    #[test]
    fn no_taps_or_one_tap_say_nothing_yet() {
        for t in [vec![], vec![1.0]] {
            let s = analyse(&t);
            assert_eq!(
                (s.kept, s.dropped, s.median_s, s.settled),
                (0, 0, None, false)
            );
        }
    }
}
