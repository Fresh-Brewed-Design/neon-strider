//! DESK_SYNC: how many of the recent desk days were also walked on (roadmap #28),
//! e.g. "walked on 9 of your last 10 desk days".
//!
//! A desk day is a local calendar day the PC had keyboard or mouse input while
//! the app ran. The desk loop calls `note_input` once a second; the first input
//! of a day writes one row to `desk_days` (migration 5) and later ticks that day
//! only compare a date. A day the PC wasn't used is simply not a desk day: it
//! neither counts nor breaks anything.
//!
//! A walked day is a local day whose sessions add up to more than 50 m. A day
//! with a walk but no desk-day row (before this feature existed, or with the
//! app's input reading unavailable) counts as a desk day too: he walked at the
//! desk. Today counts once it is walked; until then it is left out of the
//! count and shown as open, so a fresh morning doesn't read as a miss.
//!
//! The counting is a pure function (`compute`) of the two day sets, today and
//! the window size. The saved data is cached and refreshed off-thread (startup,
//! a session starting or ending, a session deleted); the snapshot only adds
//! today's live distance.
//!
//! WALL_CLOCK (`wall_clock.rs`) counts walked hours over the same window. It
//! is cached here with the saved sessions it reads, and worked out again only
//! after a refresh or when the local day rolls over, never per tick.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use crate::snapshot::wall_clock::{self, WallClock};
use crate::snapshot::weekly::SessionRow;
use crate::util::now_ms;
use chrono::{DateTime, Days, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// How many desk days the card looks back over.
/// Two working weeks: enough days for a pattern to show.
pub const WINDOW: usize = 14;
/// A day counts as walked when its sessions went further than this in total.
const WALKED_MIN_M: f64 = crate::strava_plan::MIN_WALK_DISTANCE_M;
/// Saved data older than this is not read; 14 desk days fit easily.
/// Even with weeks of holiday, 14 desk days are inside four months.
const LOOKBACK_DAYS: u64 = 120;
/// Input within this long counts as the PC being used now.
/// A minute covers reading or thinking between key presses.
const ACTIVE_INPUT: Duration = Duration::from_secs(60);
/// After a failed write, wait this long before trying again.
/// A failed write is a locked or full disk; five minutes keeps the log quiet and
/// the day is still recorded long before it ends.
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

/// One day in the window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DeskDay {
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    pub walked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DeskSync {
    /// Desk days in the window that were walked on.
    pub walked: u32,
    /// Desk days in the window (at most `window`).
    pub desk_days: u32,
    /// The most desk days the card looks back over.
    pub window: u32,
    /// The window's days, oldest first. Today is not in it until walked.
    pub days: Vec<DeskDay>,
    /// Today is a desk day not walked on yet; it isn't counted.
    pub today_open: bool,
}

// Pure logic.

fn local_date<Tz: TimeZone>(ms: i64, tz: &Tz) -> Option<NaiveDate> {
    DateTime::<Utc>::from_timestamp_millis(ms).map(|d| d.with_timezone(tz).date_naive())
}

fn day_key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn parse_day(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok()
}

/// Local days whose sessions went further than 50 m in total.
pub fn walk_days<Tz: TimeZone>(rows: &[SessionRow], tz: &Tz) -> BTreeSet<NaiveDate> {
    let mut per_day: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for row in rows {
        let Some(date) = local_date(row.start_ms, tz) else {
            continue;
        };
        let m = if row.distance_m.is_finite() {
            row.distance_m.max(0.0)
        } else {
            0.0
        };
        *per_day.entry(date).or_default() += m;
    }
    per_day
        .into_iter()
        .filter(|(_, m)| *m > WALKED_MIN_M)
        .map(|(d, _)| d)
        .collect()
}

/// Count the walked days among the last `window` desk days up to `today`.
///
/// Desk days are `desk` plus every walked day (a walk happens at the desk).
/// Days after `today` are ignored. Today is in the window only when walked
/// (`walked` contains it or `today_walked`); a desk day today without a walk
/// yet sets `today_open` instead.
pub fn compute(
    desk: &BTreeSet<NaiveDate>,
    walked: &BTreeSet<NaiveDate>,
    today: NaiveDate,
    today_walked: bool,
    window: usize,
) -> DeskSync {
    let today_walked = today_walked || walked.contains(&today);
    let all: BTreeSet<NaiveDate> = desk.union(walked).copied().collect();
    let mut days: Vec<DeskDay> = all
        .range(..today)
        .rev()
        .take(window.saturating_sub(usize::from(today_walked)))
        .map(|&d| DeskDay {
            date: day_key(d),
            walked: walked.contains(&d),
        })
        .collect();
    days.reverse();
    if today_walked && window > 0 {
        days.push(DeskDay {
            date: day_key(today),
            walked: true,
        });
    }
    DeskSync {
        walked: days.iter().filter(|d| d.walked).count() as u32,
        desk_days: days.len() as u32,
        window: window as u32,
        days,
        today_open: !today_walked && desk.contains(&today),
    }
}

fn shift_back(date: NaiveDate, days: u64) -> NaiveDate {
    date.checked_sub_days(Days::new(days)).unwrap_or(date)
}

// Cached state.

#[derive(Default)]
struct State {
    desk: BTreeSet<NaiveDate>,
    /// Walked days from saved sessions, without the live one.
    walked: BTreeSet<NaiveDate>,
    /// The saved sessions `walked` came from, for WALL_CLOCK.
    rows: Vec<SessionRow>,
    /// WALL_CLOCK as last worked out, and the local day it was for. Cleared
    /// by `refresh`.
    wall: Option<(NaiveDate, WallClock)>,
    /// The last day `note_input` recorded (or found already recorded).
    recorded: Option<NaiveDate>,
    /// A write failed at this time; don't retry before `RETRY_AFTER`.
    failed_at: Option<Instant>,
    /// A write is in flight.
    writing: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Load the saved days. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    *lock() = Some(State::default());
    refresh(app, None);
}

/// Re-read desk days and saved sessions, skipping the live session
/// `active_id` (the snapshot adds today's live distance itself). On a query
/// error the old data stays. Blocking; never called per snapshot tick.
pub fn refresh(app: &AppHandle, active_id: Option<i64>) {
    let tz = chrono::Local;
    let now = now_ms();
    let today = local_date(now, &tz).unwrap_or_default();
    let since = shift_back(today, LOOKBACK_DAYS);
    let desk: BTreeSet<NaiveDate> = match persistence::desk_days_since(app, &day_key(since)) {
        Ok(rows) => rows.iter().filter_map(|r| parse_day(r)).collect(),
        Err(e) => {
            log::warn!("desk days query failed: {}", e);
            return;
        }
    };
    let since_ms = since
        .and_hms_opt(0, 0, 0)
        .and_then(|n| tz.from_local_datetime(&n).earliest())
        .map(|d| d.timestamp_millis() - 24 * 60 * 60 * 1000)
        .unwrap_or(now - (LOOKBACK_DAYS as i64 + 1) * 24 * 60 * 60 * 1000);
    let rows = match persistence::session_rows_since(app, since_ms, active_id) {
        Ok(rows) => rows,
        Err(e) => {
            log::warn!("desk sync session query failed: {}", e);
            return;
        }
    };
    let walked = walk_days(&rows, &tz);
    if let Some(st) = lock().as_mut() {
        if desk.contains(&today) {
            st.recorded = Some(today);
        }
        // Keep a day recorded in memory whose write is still in flight.
        st.desk = desk;
        if let Some(d) = st.recorded {
            st.desk.insert(d);
        }
        st.walked = walked;
        st.rows = rows;
        st.wall = None;
    }
}

/// Once a second from the desk loop: with recent input, make sure today is a
/// recorded desk day. Writes at most once a day (plus retries after a failure).
pub fn note_input(app: &AppHandle, idle: Option<Duration>) {
    if idle.is_none_or(|d| d > ACTIVE_INPUT) {
        return;
    }
    let now = now_ms();
    let Some(today) = local_date(now, &chrono::Local) else {
        return;
    };
    {
        let mut guard = lock();
        let Some(st) = guard.as_mut() else {
            return;
        };
        if st.recorded == Some(today) || st.writing {
            return;
        }
        if st.failed_at.is_some_and(|t| t.elapsed() < RETRY_AFTER) {
            return;
        }
        st.writing = true;
        // Counted from now; the write follows.
        st.desk.insert(today);
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = persistence::record_desk_day(&app, &day_key(today), now);
        if let Some(st) = lock().as_mut() {
            st.writing = false;
            match result {
                Ok(()) => {
                    st.recorded = Some(today);
                    st.failed_at = None;
                }
                Err(e) => {
                    log::warn!("could not record today's desk day: {}", e);
                    st.failed_at = Some(Instant::now());
                }
            }
        }
    });
}

/// The card for the snapshot. `today_m` is today's distance including the
/// live session. No I/O.
pub fn snapshot(today_m: f64) -> DeskSync {
    let today = local_date(now_ms(), &chrono::Local).unwrap_or_default();
    let guard = lock();
    let Some(st) = guard.as_ref() else {
        return DeskSync::default();
    };
    compute(&st.desk, &st.walked, today, today_m > WALKED_MIN_M, WINDOW)
}

/// WALL_CLOCK for the snapshot, over DESK_SYNC's window of saved days. The
/// live session joins once it is saved (the refresh when it ends). Works the
/// counts out again only after a refresh or a new local day; otherwise a copy.
pub fn wall_clock() -> WallClock {
    let tz = chrono::Local;
    let today = local_date(now_ms(), &tz).unwrap_or_default();
    let mut guard = lock();
    let Some(st) = guard.as_mut() else {
        return WallClock::default();
    };
    if let Some((day, clock)) = &st.wall {
        if *day == today {
            return clock.clone();
        }
    }
    let clock = wall_clock_for(&st.desk, &st.walked, &st.rows, today, &tz);
    st.wall = Some((today, clock.clone()));
    clock
}

/// WALL_CLOCK over the same desk days `compute` puts in DESK_SYNC's window.
/// Today is in it only once a saved walk makes it a walked day.
fn wall_clock_for<Tz: TimeZone>(
    desk: &BTreeSet<NaiveDate>,
    walked: &BTreeSet<NaiveDate>,
    rows: &[SessionRow],
    today: NaiveDate,
    tz: &Tz,
) -> WallClock {
    let sync = compute(desk, walked, today, false, WINDOW);
    let days: BTreeSet<NaiveDate> = sync
        .days
        .iter()
        .filter_map(|x| parse_day(&x.date))
        .collect();
    wall_clock::compute(rows, &days, WINDOW, tz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn d(s: &str) -> NaiveDate {
        parse_day(s).unwrap()
    }

    fn set(days: &[&str]) -> BTreeSet<NaiveDate> {
        days.iter().map(|s| d(s)).collect()
    }

    fn row(start_ms: i64, distance_m: f64) -> SessionRow {
        SessionRow {
            start_ms,
            end_ms: start_ms + 600_000,
            distance_m,
            duration_s: 600,
            calories: 30.0,
        }
    }

    fn dates(sync: &DeskSync) -> Vec<(&str, bool)> {
        sync.days
            .iter()
            .map(|x| (x.date.as_str(), x.walked))
            .collect()
    }

    #[test]
    fn days_off_the_pc_are_ignored() {
        // Mon..Fri at the desk, a weekend away, then Mon and Tue.
        let desk = set(&[
            "2026-11-09",
            "2026-11-10",
            "2026-11-11",
            "2026-11-12",
            "2026-11-13",
            "2026-11-16",
            "2026-11-17",
        ]);
        let walked = set(&["2026-11-09", "2026-11-10", "2026-11-12", "2026-11-16"]);
        let s = compute(&desk, &walked, d("2026-11-18"), false, 14);
        assert_eq!((s.walked, s.desk_days, s.window), (4, 7, 14));
        // The weekend is not in the list at all, so it breaks nothing.
        assert!(!s.days.iter().any(|x| x.date == "2026-11-14"));
        assert_eq!(s.days.first().unwrap().date, "2026-11-09");
        assert_eq!(
            s.days.last().unwrap(),
            &DeskDay {
                date: "2026-11-17".into(),
                walked: false,
            }
        );
        assert!(!s.today_open);
    }

    #[test]
    fn the_window_keeps_only_the_most_recent_desk_days() {
        let desk: BTreeSet<NaiveDate> = (1..=20).map(|n| d(&format!("2026-10-{:02}", n))).collect();
        // Only the first five were walked: outside the last 10.
        let walked = set(&[
            "2026-10-01",
            "2026-10-02",
            "2026-10-03",
            "2026-10-04",
            "2026-10-05",
            "2026-10-19",
        ]);
        let s = compute(&desk, &walked, d("2026-10-25"), false, 10);
        assert_eq!((s.walked, s.desk_days), (1, 10));
        assert_eq!(s.days.first().unwrap().date, "2026-10-11");
        assert_eq!(s.days.last().unwrap().date, "2026-10-20");
    }

    #[test]
    fn a_walk_without_a_desk_record_counts_as_a_desk_day() {
        // Before desk days were recorded only the walks are known.
        let s = compute(
            &BTreeSet::new(),
            &set(&["2026-11-02", "2026-11-05"]),
            d("2026-11-18"),
            false,
            14,
        );
        assert_eq!((s.walked, s.desk_days), (2, 2));
    }

    #[test]
    fn today_counts_once_walked_and_is_open_until_then() {
        let desk = set(&["2026-11-16", "2026-11-17", "2026-11-18"]);
        let walked = set(&["2026-11-16"]);
        let open = compute(&desk, &walked, d("2026-11-18"), false, 14);
        assert_eq!((open.walked, open.desk_days, open.today_open), (1, 2, true));

        let done = compute(&desk, &walked, d("2026-11-18"), true, 14);
        assert_eq!(
            (done.walked, done.desk_days, done.today_open),
            (2, 3, false)
        );
        assert_eq!(
            dates(&done),
            vec![
                ("2026-11-16", true),
                ("2026-11-17", false),
                ("2026-11-18", true)
            ]
        );
    }

    #[test]
    fn a_walked_today_still_fits_the_window() {
        let desk: BTreeSet<NaiveDate> = (1..=5).map(|n| d(&format!("2026-11-{:02}", n))).collect();
        let s = compute(&desk, &BTreeSet::new(), d("2026-11-06"), true, 3);
        assert_eq!(
            dates(&s),
            vec![
                ("2026-11-04", false),
                ("2026-11-05", false),
                ("2026-11-06", true)
            ]
        );
    }

    #[test]
    fn future_days_and_an_empty_history_count_nothing() {
        let s = compute(
            &set(&["2026-11-20"]),
            &BTreeSet::new(),
            d("2026-11-18"),
            false,
            14,
        );
        assert_eq!((s.walked, s.desk_days, s.today_open), (0, 0, false));
        assert!(s.days.is_empty());
        let zero = compute(&set(&["2026-11-17"]), &set(&[]), d("2026-11-18"), true, 0);
        assert!(zero.days.is_empty());
    }

    #[test]
    fn wall_clock_counts_only_the_last_14_desk_days() {
        let tz = FixedOffset::east_opt(3600).unwrap();
        let at9 = |day: u32| {
            tz.with_ymd_and_hms(2026, 10, day, 9, 15, 0)
                .unwrap()
                .timestamp_millis()
        };
        // 20 desk days, each walked at 09:15 for 20 minutes.
        let desk: BTreeSet<NaiveDate> = (1..=20).map(|n| d(&format!("2026-10-{:02}", n))).collect();
        let mut rows: Vec<SessionRow> = (1..=20)
            .map(|n| {
                let mut r = row(at9(n), 900.0);
                r.end_ms = r.start_ms + 20 * 60_000;
                r
            })
            .collect();
        // On the 27th a 30 m walk alone is not a walked day and the PC was not
        // used, so the 27th is not a desk day and its 14:00 stays dark.
        let mut short = row(at9(27) + 5 * 3_600_000, 30.0);
        short.end_ms = short.start_ms + 600_000;
        rows.push(short);
        let walked = walk_days(&rows, &tz);
        let c = wall_clock_for(&desk, &walked, &rows, d("2026-10-28"), &tz);
        assert_eq!((c.desk_days, c.window), (14, 14));
        assert_eq!(c.hours[9], 14);
        assert_eq!(c.hours[14], 0);
        // Today walked and saved: in the window, the oldest day drops out.
        let c = wall_clock_for(&desk, &walked, &rows, d("2026-10-20"), &tz);
        assert_eq!((c.desk_days, c.hours[9]), (14, 14));
    }

    #[test]
    fn walk_days_use_the_local_date_and_a_daily_total_over_50_m() {
        // 2026-11-17 23:30 at UTC+10 is 13:30Z: the local day is the 17th.
        let tz = FixedOffset::east_opt(10 * 3600).unwrap();
        let late = tz
            .with_ymd_and_hms(2026, 11, 17, 23, 30, 0)
            .unwrap()
            .timestamp_millis();
        // 00:10 on the 18th local is still the 17th in UTC.
        let early = tz
            .with_ymd_and_hms(2026, 11, 18, 0, 10, 0)
            .unwrap()
            .timestamp_millis();
        let rows = [
            row(late, 400.0),
            row(early, 30.0),
            row(early + 60_000, 30.0),
        ];
        // Two 30 m sessions on the 18th add up to 60 m: walked.
        assert_eq!(walk_days(&rows, &tz), set(&["2026-11-17", "2026-11-18"]));
        // In UTC all three fall on the 17th.
        assert_eq!(walk_days(&rows, &Utc), set(&["2026-11-17"]));
        // A short or broken row alone is not a walk.
        assert!(walk_days(&[row(late, 50.0), row(early, f64::NAN)], &tz).is_empty());
    }
}
