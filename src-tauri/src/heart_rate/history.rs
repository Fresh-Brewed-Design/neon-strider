//! The PULSE_RATE spark chart's data: a rolling window of heart rate, one
//! point per step, kept in Rust so every window draws the same line from the
//! snapshot. Pure: the caller passes a monotonic clock in milliseconds.

use std::collections::VecDeque;

/// One point every 5 s: heart rate on a walk moves over tens of seconds, so
/// 5 s shows a climb or a recovery without the beat-to-beat jitter.
pub const STEP_MS: u64 = 5_000;
/// Ten minutes: long enough to see a climb or a recovery from start to end,
/// short enough that the latest change still fills a good part of a small
/// card. At one point per step that is 120 numbers, so the snapshot stays small.
pub const WINDOW_MS: u64 = 10 * 60 * 1_000;
/// Points in the window.
pub const POINTS: usize = (WINDOW_MS / STEP_MS) as usize;

/// The rolling window. A step with no reading is a gap (`None`), so the line
/// breaks there instead of joining across it.
#[derive(Debug, Default)]
pub struct History {
    /// Finished steps, oldest first; never more than [`POINTS`].
    points: VecDeque<Option<u16>>,
    /// The step being filled now, as `at_ms / STEP_MS`.
    slot: Option<u64>,
    sum: u32,
    count: u32,
}

impl History {
    /// Record one reading (or none) at `at_ms`. Steps that passed with no
    /// call at all (the tick stalled) are gaps.
    pub fn record(&mut self, at_ms: u64, bpm: Option<u16>) {
        let slot = at_ms / STEP_MS;
        match self.slot {
            None => self.slot = Some(slot),
            Some(current) if slot > current => {
                self.close();
                // More than a window of missed steps is a full window of gaps.
                let skipped = (slot - current - 1).min(POINTS as u64);
                for _ in 0..skipped {
                    self.push(None);
                }
                self.slot = Some(slot);
            }
            // The same step, or a clock that went back: keep filling it.
            Some(_) => {}
        }
        if let Some(b) = bpm {
            self.sum += u32::from(b);
            self.count += 1;
        }
    }

    /// Finish the current step: the rounded mean of its readings, or a gap.
    fn close(&mut self) {
        let point = (self.count > 0).then(|| ((self.sum + self.count / 2) / self.count) as u16);
        self.push(point);
        self.sum = 0;
        self.count = 0;
    }

    fn push(&mut self, point: Option<u16>) {
        self.points.push_back(point);
        while self.points.len() > POINTS {
            self.points.pop_front();
        }
    }

    /// The finished steps, oldest first, padded at the start with gaps to a
    /// full window, so the newest point is always at the right edge.
    pub fn points(&self) -> Vec<Option<u32>> {
        let pad = POINTS - self.points.len();
        std::iter::repeat_n(None, pad)
            .chain(self.points.iter().map(|p| p.map(u32::from)))
            .collect()
    }

    /// Start again (a new sensor, or none).
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One reading a second, like the worker's tick, from `from_s` for `secs`.
    fn feed(h: &mut History, from_s: u64, secs: u64, bpm: Option<u16>) {
        for s in from_s..from_s + secs {
            h.record(s * 1_000, bpm);
        }
    }

    fn finished(h: &History) -> Vec<Option<u32>> {
        h.points().into_iter().skip_while(Option::is_none).collect()
    }

    #[test]
    fn the_window_is_ten_minutes_of_five_second_steps() {
        assert_eq!(POINTS, 120);
        let h = History::default();
        assert_eq!(h.points().len(), POINTS);
        assert!(h.points().iter().all(Option::is_none));
    }

    #[test]
    fn each_step_is_the_mean_of_its_readings() {
        let mut h = History::default();
        for (s, bpm) in [100, 101, 102, 103, 104].into_iter().enumerate() {
            h.record(s as u64 * 1_000, Some(bpm));
        }
        // The step is still open: nothing finished yet.
        assert!(h.points().iter().all(Option::is_none));
        h.record(5_000, Some(90));
        assert_eq!(finished(&h), vec![Some(102)]);
        // Rounded, not truncated: 90 and 91 make 91.
        h.record(6_000, Some(91));
        h.record(10_000, None);
        assert_eq!(finished(&h), vec![Some(102), Some(91)]);
    }

    #[test]
    fn a_step_with_no_reading_is_a_gap() {
        let mut h = History::default();
        feed(&mut h, 0, 5, Some(110));
        feed(&mut h, 5, 10, None);
        feed(&mut h, 15, 5, Some(120));
        h.record(20_000, None);
        assert_eq!(finished(&h), vec![Some(110), None, None, Some(120)]);
    }

    #[test]
    fn one_reading_is_enough_for_a_step() {
        let mut h = History::default();
        h.record(0, None);
        h.record(3_000, Some(95));
        h.record(5_000, None);
        assert_eq!(finished(&h), vec![Some(95)]);
    }

    #[test]
    fn steps_the_tick_missed_are_gaps() {
        let mut h = History::default();
        h.record(0, Some(100));
        // The tick stalled for 17 s: steps 1 and 2 had no call at all.
        h.record(17_000, Some(130));
        h.record(20_000, None);
        assert_eq!(finished(&h), vec![Some(100), None, None, Some(130)]);
    }

    #[test]
    fn the_window_rolls_and_keeps_the_newest() {
        let mut h = History::default();
        let steps = POINTS as u64 + 30;
        for i in 0..=steps {
            h.record(i * STEP_MS, Some(60 + (i % 100) as u16));
        }
        let p = h.points();
        assert_eq!(p.len(), POINTS);
        assert!(p.iter().all(Option::is_some));
        // The last finished step is the one before `steps`.
        assert_eq!(p[POINTS - 1], Some(60 + ((steps - 1) % 100) as u32));
        assert_eq!(p[0], Some(60 + ((steps - POINTS as u64) % 100) as u32));
    }

    #[test]
    fn a_long_stall_is_a_full_window_of_gaps() {
        let mut h = History::default();
        h.record(0, Some(100));
        h.record(60 * 60 * 1_000, Some(100));
        assert_eq!(h.points().len(), POINTS);
        assert!(h.points().iter().all(Option::is_none));
    }

    #[test]
    fn a_clock_going_back_stays_in_the_step() {
        let mut h = History::default();
        h.record(9_000, Some(100));
        h.record(2_000, Some(110));
        h.record(10_000, None);
        assert_eq!(finished(&h), vec![Some(105)]);
    }

    #[test]
    fn clear_starts_again() {
        let mut h = History::default();
        feed(&mut h, 0, 30, Some(100));
        h.clear();
        assert!(h.points().iter().all(Option::is_none));
    }
}
