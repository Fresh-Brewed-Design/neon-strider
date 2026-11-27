//! The VELOCITY spark chart's data (#362): the belt's calibrated speed over the
//! last 10 minutes, so after a call or a typing burst the HUD and the Live page
//! show what the belt did meanwhile (Neural flow easing off, a call's quiet
//! speed, a program's rest, the user's own presses).
//!
//! The window is PULSE_RATE's (`heart_rate/history.rs`, reused as is): one
//! point per 5 s, 120 points, each the mean of its seconds. The desk loop calls
//! `record` once a second with the belt's speed while the treadmill is linked
//! and a walk is open, and with none otherwise, so a dropped link or the time
//! between walks is a gap and the line never joins across it. A paused belt
//! inside a walk is a real 0 km/h and draws as one.
//!
//! It only reads the belt. This file is declared from `snapshot.rs`
//! (`#[path]`), not `lib.rs`.

use crate::heart_rate::history::History;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

/// The history keeps whole numbers: speed goes in as hundredths of a km/h,
/// fine enough that a 0.1 km/h step shows and a 5 s mean is not rounded away.
const SCALE: f32 = 100.0;

/// The step clock. Monotonic, so a change to the PC's clock never moves the line.
static CLOCK: LazyLock<Instant> = LazyLock::new(Instant::now);
static TRACE: LazyLock<Mutex<History>> = LazyLock::new(|| Mutex::new(History::default()));

fn with<R>(f: impl FnOnce(&mut History) -> R) -> R {
    let mut h = TRACE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut h)
}

/// What the desk loop records this second: the speed while the treadmill is
/// linked and a walk is open, none (a gap) otherwise.
pub fn reading(linked: bool, walk_open: bool, speed_kmh: f32) -> Option<f32> {
    (linked && walk_open).then_some(speed_kmh)
}

/// Add this second's speed (or a gap). Called once a second by the desk loop.
pub fn record(kmh: Option<f32>) {
    let at_ms = u64::try_from(CLOCK.elapsed().as_millis()).unwrap_or(u64::MAX);
    with(|h| record_at(h, at_ms, kmh));
}

fn record_at(h: &mut History, at_ms: u64, kmh: Option<f32>) {
    h.record(at_ms, kmh.map(to_scaled));
}

fn to_scaled(kmh: f32) -> u16 {
    // NaN and below zero are 0; the deck's top speed is far under 655 km/h.
    (kmh.max(0.0) * SCALE).round().min(f32::from(u16::MAX)) as u16
}

/// The chart for the snapshot: km/h, oldest first, a full window with `None`
/// for a gap. Empty while the window holds no speed at all (no walk in the
/// last 10 minutes), so the windows draw no chart then and the snapshot stays
/// small.
pub fn snapshot() -> Vec<Option<f32>> {
    with(|h| points(h))
}

fn points(h: &History) -> Vec<Option<f32>> {
    let raw = h.points();
    if raw.iter().all(Option::is_none) {
        return Vec::new();
    }
    raw.into_iter()
        .map(|p| p.map(|v| v as f32 / SCALE))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heart_rate::history::{POINTS, STEP_MS};

    /// One call a second, like the desk loop, from `from_s` for `secs`.
    fn feed(h: &mut History, from_s: u64, secs: u64, kmh: Option<f32>) {
        for s in from_s..from_s + secs {
            record_at(h, s * 1_000, kmh);
        }
    }

    fn finished(h: &History) -> Vec<Option<f32>> {
        points(h).into_iter().skip_while(Option::is_none).collect()
    }

    #[test]
    fn no_speed_in_the_window_is_no_chart() {
        let mut h = History::default();
        assert!(points(&h).is_empty());
        // Gaps only, the link down or no walk: still no chart.
        feed(&mut h, 0, 30, None);
        assert!(points(&h).is_empty());
    }

    #[test]
    fn a_walk_fills_a_full_window_of_five_second_steps() {
        let mut h = History::default();
        feed(&mut h, 0, 10, Some(3.5));
        record_at(&mut h, 10_000, None);
        let p = points(&h);
        assert_eq!(p.len(), POINTS);
        assert_eq!(POINTS, 120);
        assert_eq!(STEP_MS, 5_000);
        assert_eq!(finished(&h), vec![Some(3.5), Some(3.5)]);
    }

    #[test]
    fn each_step_is_the_mean_speed() {
        let mut h = History::default();
        for (s, kmh) in [3.0, 3.0, 3.2, 3.4, 3.4].into_iter().enumerate() {
            record_at(&mut h, s as u64 * 1_000, Some(kmh));
        }
        record_at(&mut h, 5_000, None);
        let got = finished(&h);
        assert_eq!(got.len(), 1);
        assert!((got[0].unwrap() - 3.2).abs() < 1e-4);
    }

    #[test]
    fn a_dropped_link_or_no_walk_is_a_gap() {
        let mut h = History::default();
        feed(&mut h, 0, 5, reading(true, true, 4.0));
        feed(&mut h, 5, 5, reading(false, true, 4.0));
        feed(&mut h, 10, 5, reading(true, false, 0.0));
        feed(&mut h, 15, 5, reading(true, true, 2.0));
        record_at(&mut h, 20_000, None);
        assert_eq!(finished(&h), vec![Some(4.0), None, None, Some(2.0)]);
    }

    #[test]
    fn a_paused_belt_in_a_walk_is_zero_not_a_gap() {
        assert_eq!(reading(true, true, 0.0), Some(0.0));
        let mut h = History::default();
        feed(&mut h, 0, 5, Some(0.0));
        record_at(&mut h, 5_000, None);
        assert_eq!(finished(&h), vec![Some(0.0)]);
    }

    #[test]
    fn a_tenth_of_a_kmh_survives_the_scale() {
        assert_eq!(to_scaled(3.1), 310);
        assert_eq!(to_scaled(-1.0), 0);
        assert_eq!(to_scaled(f32::NAN), 0);
    }

    #[test]
    fn the_window_rolls_to_the_last_ten_minutes() {
        let mut h = History::default();
        feed(&mut h, 0, 60, Some(2.0));
        // Eleven minutes later the first minute has rolled out.
        feed(&mut h, 60, 11 * 60, None);
        assert!(points(&h).is_empty());
    }
}
