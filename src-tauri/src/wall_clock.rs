//! WALL_CLOCK: which hours of the desk day are usually walked (issue #218).
//!
//! For each local hour of the day, how many of the last desk days (the same
//! window DESK_SYNC counts, `desk_sync::compute`) had walking in that hour. A
//! session counts in every local hour it touches, from its start to its end,
//! so a walk from 09:40 to 10:20 lights 09:00 and 10:00 once each. Days away
//! from the PC are not desk days, so they never dim the clock.
//!
//! Hours are local wall-clock hours, read through the time zone for each
//! instant, so a DST change is handled the way a wall clock shows it: on the
//! spring day the skipped hour simply has no walking, and on the autumn day
//! the hour that repeats is one hour, counted once per day. A walk past
//! midnight counts its later part on the next day, which counts only if that
//! day is in the window too.
//!
//! Pure logic only; `desk_sync` caches the result and recomputes it when the
//! saved sessions or desk days change, or the local day rolls over.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::snapshot::weekly::SessionRow;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The card shows at least these hours (inclusive cell starts): 07:00 to 20:00.
/// That spans a usual desk day; an hour outside it shows only once walked in.
pub const SHOWN_FIRST_HOUR: u32 = 7;
/// The last cell starts at 19:00 and ends at 20:00.
pub const SHOWN_LAST_HOUR: u32 = 19;
/// A session longer than this is cut to it.
/// No desk walk runs half a day; a broken end time must not light every hour.
const MAX_SPAN_MS: i64 = 12 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct WallClock {
    /// Desk days with walking in each local hour, index 0 is 00:00 to 01:00.
    /// Always 24 entries.
    pub hours: Vec<u32>,
    /// Desk days the counts are out of (at most `window`).
    pub desk_days: u32,
    /// The most desk days the card looks back over (DESK_SYNC's window).
    pub window: u32,
    /// The first hour cell to show: 07 or an earlier walked hour.
    pub first_hour: u32,
    /// The last hour cell to show: 19 or a later walked hour.
    pub last_hour: u32,
}

impl Default for WallClock {
    fn default() -> Self {
        WallClock {
            hours: vec![0; 24],
            desk_days: 0,
            window: 0,
            first_hour: SHOWN_FIRST_HOUR,
            last_hour: SHOWN_LAST_HOUR,
        }
    }
}

/// The local (date, hour) slots a span from `start_ms` to `end_ms` touches.
fn slots<Tz: TimeZone>(start_ms: i64, end_ms: i64, tz: &Tz, out: &mut BTreeSet<(NaiveDate, u32)>) {
    let Some(mut cursor) = DateTime::<Utc>::from_timestamp_millis(start_ms) else {
        return;
    };
    let end_ms = end_ms.min(start_ms.saturating_add(MAX_SPAN_MS));
    while cursor.timestamp_millis() < end_ms {
        let local = cursor.with_timezone(tz).naive_local();
        out.insert((local.date(), local.hour()));
        let hour_start: NaiveDateTime = local
            .date()
            .and_hms_opt(local.hour(), 0, 0)
            .unwrap_or(local);
        // The next local hour that exists: in a spring-forward gap the
        // skipped hour has no instant, so try the one after it.
        let next = (1..=3).find_map(|step| {
            tz.from_local_datetime(&(hour_start + Duration::hours(step)))
                .earliest()
                .map(|d| d.with_timezone(&Utc))
        });
        match next {
            Some(n) if n > cursor => cursor = n,
            // Never expected; step an hour so the loop always ends.
            _ => cursor += Duration::hours(1),
        }
    }
}

/// The span a session row covers: start to its end time, or start plus its
/// moving time when the end is missing or not after the start.
fn span(row: &SessionRow) -> (i64, i64) {
    let end = if row.end_ms > row.start_ms {
        row.end_ms
    } else {
        row.start_ms
            .saturating_add(row.duration_s.max(0).saturating_mul(1000))
    };
    (row.start_ms, end)
}

/// Count, per local hour, the `days` that had walking in that hour.
///
/// A session with no distance is a belt start with no walk and is skipped.
/// Slots on dates outside `days` are ignored. `window` is passed through.
pub fn compute<Tz: TimeZone>(
    rows: &[SessionRow],
    days: &BTreeSet<NaiveDate>,
    window: usize,
    tz: &Tz,
) -> WallClock {
    let mut touched = BTreeSet::new();
    for row in rows {
        if !(row.distance_m.is_finite() && row.distance_m > 0.0) {
            continue;
        }
        let (start, end) = span(row);
        slots(start, end, tz, &mut touched);
    }
    let mut hours = vec![0u32; 24];
    for (date, hour) in touched {
        if days.contains(&date) {
            if let Some(h) = hours.get_mut(hour as usize) {
                *h += 1;
            }
        }
    }
    let lit = |h: &usize| hours[*h] > 0;
    let first_hour = (0..24)
        .find(lit)
        .map_or(SHOWN_FIRST_HOUR, |h| (h as u32).min(SHOWN_FIRST_HOUR));
    let last_hour = (0..24)
        .rev()
        .find(lit)
        .map_or(SHOWN_LAST_HOUR, |h| (h as u32).max(SHOWN_LAST_HOUR));
    WallClock {
        hours,
        desk_days: days.len() as u32,
        window: window as u32,
        first_hour,
        last_hour,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::weekly::tests::{at, Eastern};
    use chrono::FixedOffset;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn set(days: &[&str]) -> BTreeSet<NaiveDate> {
        days.iter().map(|s| d(s)).collect()
    }

    fn walk(start_ms: i64, end_ms: i64) -> SessionRow {
        SessionRow {
            start_ms,
            end_ms,
            distance_m: 800.0,
            duration_s: (end_ms - start_ms) / 1000,
            calories: 40.0,
        }
    }

    fn utc2() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).unwrap()
    }

    fn lit(clock: &WallClock) -> Vec<(usize, u32)> {
        clock
            .hours
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .map(|(h, n)| (h, *n))
            .collect()
    }

    #[test]
    fn a_session_counts_in_every_hour_it_touches_once_per_day() {
        let tz = utc2();
        let rows = [
            // 09:40 to 11:05 touches 09, 10 and 11.
            walk(at(&tz, 2026, 11, 16, 9, 40), at(&tz, 2026, 11, 16, 11, 5)),
            // A second walk in 10:00 the same day adds nothing to 10.
            walk(at(&tz, 2026, 11, 16, 10, 30), at(&tz, 2026, 11, 16, 10, 50)),
            // Another day, 10:00 to 10:30 only.
            walk(at(&tz, 2026, 11, 17, 10, 0), at(&tz, 2026, 11, 17, 10, 30)),
        ];
        let c = compute(&rows, &set(&["2026-11-16", "2026-11-17"]), 14, &tz);
        assert_eq!(lit(&c), vec![(9, 1), (10, 2), (11, 1)]);
        assert_eq!((c.desk_days, c.window), (2, 14));
        assert_eq!(c.hours.len(), 24);
        assert_eq!((c.first_hour, c.last_hour), (7, 19));
    }

    #[test]
    fn days_outside_the_window_are_ignored() {
        let tz = utc2();
        let rows = [
            walk(at(&tz, 2026, 11, 1, 8, 0), at(&tz, 2026, 11, 1, 8, 30)),
            walk(at(&tz, 2026, 11, 16, 14, 0), at(&tz, 2026, 11, 16, 14, 30)),
        ];
        // The 1st is older than the last desk days, or a day not at the desk:
        // either way it is not in the window and lights nothing.
        let c = compute(&rows, &set(&["2026-11-16", "2026-11-17"]), 14, &tz);
        assert_eq!(lit(&c), vec![(14, 1)]);
        assert_eq!(c.desk_days, 2);
    }

    #[test]
    fn a_walk_past_midnight_counts_its_later_part_on_the_next_day() {
        let tz = utc2();
        let rows = [walk(
            at(&tz, 2026, 11, 16, 23, 30),
            at(&tz, 2026, 11, 17, 0, 40),
        )];
        let both = compute(&rows, &set(&["2026-11-16", "2026-11-17"]), 14, &tz);
        assert_eq!(lit(&both), vec![(0, 1), (23, 1)]);
        assert_eq!((both.first_hour, both.last_hour), (0, 23));
        // The next day not in the window: only 23:00 on the 16th counts.
        let first = compute(&rows, &set(&["2026-11-16"]), 14, &tz);
        assert_eq!(lit(&first), vec![(23, 1)]);
    }

    #[test]
    fn dst_days_follow_the_wall_clock() {
        let tz = Eastern;
        // Spring: 2026-03-08 02:00 EST jumps to 03:00 EDT. 01:30 to 03:30 is
        // one hour of real time and touches 01 and 03; 02 never happens.
        let spring = [walk(at(&tz, 2026, 3, 8, 1, 30), at(&tz, 2026, 3, 8, 3, 30))];
        let c = compute(&spring, &set(&["2026-03-08"]), 14, &tz);
        assert_eq!(lit(&c), vec![(1, 1), (3, 1)]);
        // Autumn: 2026-11-01 01:00 to 02:00 happens twice. A walk through
        // both is hour 01 once, then 02.
        let start = at(&tz, 2026, 11, 1, 0, 50); // EDT
        let end = start + 2 * 3_600_000 + 20 * 60_000; // 02:10 EST
        let c = compute(&[walk(start, end)], &set(&["2026-11-01"]), 14, &tz);
        assert_eq!(lit(&c), vec![(0, 1), (1, 1), (2, 1)]);
    }

    #[test]
    fn broken_rows_light_little_or_nothing() {
        let tz = utc2();
        let start = at(&tz, 2026, 11, 16, 9, 0);
        let mut no_walk = walk(start, start + 3_600_000);
        no_walk.distance_m = 0.0;
        // An end before the start falls back to the moving time (20 min).
        let mut no_end = walk(start, start - 1);
        no_end.duration_s = 20 * 60;
        let c = compute(&[no_walk, no_end], &set(&["2026-11-16"]), 14, &tz);
        assert_eq!(lit(&c), vec![(9, 1)]);
        // An end days later is cut to 12 hours.
        let runaway = walk(start, start + 5 * 24 * 3_600_000);
        let c = compute(&[runaway], &set(&["2026-11-16", "2026-11-17"]), 14, &tz);
        assert_eq!(lit(&c).len(), 12);
        assert_eq!(c.hours[21], 0);
    }

    #[test]
    fn no_desk_days_is_an_empty_clock() {
        let c = compute(&[], &BTreeSet::new(), 14, &utc2());
        assert_eq!(c.desk_days, 0);
        assert!(c.hours.iter().all(|n| *n == 0));
        assert_eq!((c.first_hour, c.last_hour), (7, 19));
    }
}
