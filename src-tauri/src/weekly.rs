//! Weekly summary: last week against the week before, plus this week so far.
//!
//! Weeks are local calendar weeks, Monday 00:00 to Sunday 23:59 local time, so
//! each one is an ISO week. Sessions are bucketed by the local calendar date
//! they started on (not by subtracting 7 x 24 h), which keeps a week that
//! contains a DST change seven days long.
//!
//! The maths is a pure function of a list of session rows, "now" and a time
//! zone, so it can be unit-tested. The saved-sessions query is cached like the
//! daily distance and refreshed off-thread (startup, session change, week
//! rollover, a session deleted); the snapshot only clones the cache.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use crate::util::now_ms;
use chrono::{DateTime, Datelike, Days, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::AppHandle;

/// Settings key: the ISO week id (e.g. "2026-W45") of the last-week summary the
/// user acknowledged.
const KEY_ACK: &str = "weekly_summary_ack";

/// A day counts as active when one of its sessions went further than this.
const ACTIVE_SESSION_MIN_M: f64 = 50.0;

/// One saved session, as far as the summary cares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionRow {
    /// Epoch ms (`sessions.start_time`).
    pub start_ms: i64,
    pub distance_m: f64,
    pub duration_s: i64,
    pub calories: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct WeekStats {
    pub distance_km: f64,
    /// Total walking time.
    #[ts(type = "number")]
    pub duration_s: u64,
    /// Days with at least one session over 50 m.
    pub active_days: u32,
    pub sessions: u32,
    pub calories: f64,
    /// The most distance covered on any one day.
    pub longest_day_km: f64,
}

/// How a value moved against the week before.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Change {
    /// Whole percent, rounded; negative is down. `None` when there is nothing
    /// to compare against (the week before was zero).
    pub pct: Option<i32>,
    /// There was nothing the week before and something now: show "new".
    pub is_new: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct WeeklySummary {
    /// The current week so far (Monday to now).
    pub this_week: WeekStats,
    /// The most recent completed week.
    pub last_week: WeekStats,
    pub week_before: WeekStats,
    /// ISO week of `last_week`, e.g. "2026-W45". The acknowledgement is stored
    /// under this id.
    pub last_week_id: String,
    /// Last week's distance against the week before.
    pub distance_change: Change,
    /// The HUD should show the last-week card.
    pub show_card: bool,
}

// Pure logic.

/// Percent change from `previous` to `current`. From zero there is no
/// percentage: `is_new` is set when there is something now, and nothing is set
/// when both are zero.
pub fn change(current: f64, previous: f64) -> Change {
    const EPS: f64 = 1e-9;
    if previous <= EPS {
        return Change {
            pct: None,
            is_new: current > EPS,
        };
    }
    Change {
        pct: Some((((current - previous) / previous) * 100.0).round() as i32),
        is_new: false,
    }
}

fn local_date<Tz: TimeZone>(ms: i64, tz: &Tz) -> Option<NaiveDate> {
    DateTime::<Utc>::from_timestamp_millis(ms).map(|d| d.with_timezone(tz).date_naive())
}

fn shift(date: NaiveDate, days: i64) -> NaiveDate {
    let delta = Days::new(days.unsigned_abs());
    let out = if days >= 0 {
        date.checked_add_days(delta)
    } else {
        date.checked_sub_days(delta)
    };
    out.unwrap_or(date)
}

fn monday_of(date: NaiveDate) -> NaiveDate {
    shift(date, -(date.weekday().num_days_from_monday() as i64))
}

/// "2026-W45" for the ISO week starting on `monday`.
fn week_id(monday: NaiveDate) -> String {
    let w = monday.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

/// The ISO week id of the local week containing `now_ms`.
fn current_week_id<Tz: TimeZone>(now_ms: i64, tz: &Tz) -> String {
    week_id(monday_of(local_date(now_ms, tz).unwrap_or_default()))
}

/// First instant of a local date as epoch ms. A DST gap at midnight starts the
/// day later, so try the next few hours.
fn day_start_ms<Tz: TimeZone>(tz: &Tz, date: NaiveDate) -> Option<i64> {
    // DST gaps are one hour (two at most in a few zones); four hours clears any.
    (0..=3).find_map(|hour| {
        let naive = date.and_hms_opt(hour, 0, 0)?;
        tz.from_local_datetime(&naive)
            .earliest()
            .map(|dt| dt.timestamp_millis())
    })
}

/// Epoch ms to query sessions from: the start of the week before last week,
/// less a day of slack. `compute` does the exact bucketing.
fn query_since_ms<Tz: TimeZone>(now_ms: i64, tz: &Tz) -> i64 {
    let today = local_date(now_ms, tz).unwrap_or_default();
    let before_monday = shift(monday_of(today), -14);
    day_start_ms(tz, before_monday)
        .map(|ms| ms - 24 * 60 * 60 * 1000)
        .unwrap_or(now_ms - 22 * 24 * 60 * 60 * 1000)
}

#[derive(Default)]
struct Acc {
    duration_s: i64,
    calories: f64,
    sessions: u32,
    /// Per local day: (metres, had a session over the active threshold).
    days: HashMap<NaiveDate, (f64, bool)>,
}

impl Acc {
    fn add(&mut self, date: NaiveDate, row: &SessionRow) {
        // Bad rows (NaN, negative) count as zero rather than poisoning a total.
        let dist = if row.distance_m.is_finite() {
            row.distance_m.max(0.0)
        } else {
            0.0
        };
        let kcal = if row.calories.is_finite() {
            row.calories.max(0.0)
        } else {
            0.0
        };
        self.duration_s += row.duration_s.max(0);
        self.calories += kcal;
        self.sessions += 1;
        let day = self.days.entry(date).or_insert((0.0, false));
        day.0 += dist;
        day.1 |= dist > ACTIVE_SESSION_MIN_M;
    }

    fn finish(self) -> WeekStats {
        let meters: f64 = self.days.values().map(|d| d.0).sum();
        let longest = self.days.values().map(|d| d.0).fold(0.0, f64::max);
        WeekStats {
            distance_km: meters / 1000.0,
            duration_s: self.duration_s.max(0) as u64,
            active_days: self.days.values().filter(|d| d.1).count() as u32,
            sessions: self.sessions,
            calories: self.calories,
            longest_day_km: longest / 1000.0,
        }
    }
}

/// Summarise this week so far, last week and the week before. `show_card` is
/// left false; the cache decides that.
pub fn compute<Tz: TimeZone>(rows: &[SessionRow], now_ms: i64, tz: &Tz) -> WeeklySummary {
    let today = local_date(now_ms, tz).unwrap_or_default();
    let this_monday = monday_of(today);
    let last_monday = shift(this_monday, -7);
    let before_monday = shift(this_monday, -14);
    let next_monday = shift(this_monday, 7);

    let (mut before, mut last, mut this) = (Acc::default(), Acc::default(), Acc::default());
    for row in rows {
        let Some(date) = local_date(row.start_ms, tz) else {
            continue;
        };
        if date >= next_monday || date < before_monday {
            continue;
        }
        if date >= this_monday {
            this.add(date, row);
        } else if date >= last_monday {
            last.add(date, row);
        } else {
            before.add(date, row);
        }
    }

    let (this_week, last_week, week_before) = (this.finish(), last.finish(), before.finish());
    WeeklySummary {
        distance_change: change(last_week.distance_km, week_before.distance_km),
        last_week_id: week_id(last_monday),
        this_week,
        last_week,
        week_before,
        show_card: false,
    }
}

/// The card is triggered by a session starting when last week had at least one
/// active day (a session over 50 m) and
/// that week's summary hasn't been acknowledged.
fn card_trigger(summary: &WeeklySummary, acked: Option<&str>) -> bool {
    summary.last_week.active_days > 0 && acked != Some(summary.last_week_id.as_str())
}

// Cached state.

struct State {
    summary: WeeklySummary,
    /// ISO week the summary was computed in; a mismatch means the week rolled over.
    this_week_id: String,
    /// `last_week_id` of the summary the user acknowledged.
    acked: Option<String>,
    /// `last_week_id` the card was triggered for. Holds the card on screen
    /// after the session ends, until it is acknowledged.
    latched: Option<String>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn parse_ack(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
}

/// Load the acknowledgement and the first summary. Blocking; call once after
/// the DB is initialised.
pub fn init(app: &AppHandle) {
    let acked = parse_ack(persistence::read_setting(app, KEY_ACK));
    *lock() = Some(State {
        summary: WeeklySummary::default(),
        this_week_id: String::new(),
        acked,
        latched: None,
    });
    refresh(app, None);
}

/// Re-read the saved sessions and rebuild the summary, skipping the live
/// session `active_id` (its row is still partial). On a query error the old
/// summary stays. Blocking; never called per snapshot tick.
pub fn refresh(app: &AppHandle, active_id: Option<i64>) {
    let now = now_ms();
    let tz = chrono::Local;
    let rows = match persistence::session_rows_since(app, query_since_ms(now, &tz), active_id) {
        Ok(rows) => rows,
        Err(e) => {
            log::warn!("weekly summary query failed: {}", e);
            return;
        }
    };
    let summary = compute(&rows, now, &tz);
    let this_week_id = current_week_id(now, &tz);
    if let Some(st) = lock().as_mut() {
        st.summary = summary;
        st.this_week_id = this_week_id;
    }
}

/// The local week changed since the last refresh. Pure of I/O; cheap enough
/// for the once-a-minute check.
pub fn rolled_over() -> bool {
    let now = current_week_id(now_ms(), &chrono::Local);
    lock().as_ref().is_some_and(|st| st.this_week_id != now)
}

/// A session just started: show the card if last week's summary is due.
pub fn on_session_start() {
    if let Some(st) = lock().as_mut() {
        if card_trigger(&st.summary, st.acked.as_deref()) {
            st.latched = Some(st.summary.last_week_id.clone());
        }
    }
}

/// The summary for the snapshot. No I/O.
pub fn snapshot() -> WeeklySummary {
    let guard = lock();
    let Some(st) = guard.as_ref() else {
        return WeeklySummary::default();
    };
    let mut out = st.summary.clone();
    out.show_card = st.latched.as_deref() == Some(out.last_week_id.as_str())
        && st.acked.as_deref() != Some(out.last_week_id.as_str());
    out
}

/// The user pressed OK. Blocking (SQLite).
pub fn acknowledge(app: &AppHandle) -> Result<(), String> {
    let id = match lock().as_ref() {
        Some(st) => st.summary.last_week_id.clone(),
        None => return Ok(()),
    };
    if id.is_empty() {
        return Ok(());
    }
    persistence::write_setting(app, KEY_ACK, &id)?;
    if let Some(st) = lock().as_mut() {
        st.acked = Some(id);
        st.latched = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, LocalResult, NaiveDateTime, Offset};
    use std::fmt;

    // ---- A tiny US Eastern zone for 2026, so DST weeks can be tested without
    // a tz database: EDT from 2026-03-08 07:00Z to 2026-11-01 06:00Z.

    const DST_START_UTC_MS: i64 = 1_772_953_200_000; // 2026-03-08T07:00:00Z
    const DST_END_UTC_MS: i64 = 1_793_512_800_000; // 2026-11-01T06:00:00Z

    #[derive(Clone, Copy, Debug)]
    struct EasternOffset(i32);

    impl fmt::Display for EasternOffset {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl Offset for EasternOffset {
        fn fix(&self) -> FixedOffset {
            FixedOffset::east_opt(self.0).unwrap()
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct Eastern;

    const EST: EasternOffset = EasternOffset(-5 * 3600);
    const EDT: EasternOffset = EasternOffset(-4 * 3600);

    fn in_dst(utc_ms: i64) -> bool {
        (DST_START_UTC_MS..DST_END_UTC_MS).contains(&utc_ms)
    }

    impl TimeZone for Eastern {
        type Offset = EasternOffset;

        fn from_offset(_: &EasternOffset) -> Self {
            Eastern
        }

        fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<EasternOffset> {
            self.offset_from_local_datetime(&local.and_hms_opt(0, 0, 0).unwrap())
        }

        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<EasternOffset> {
            let as_utc = local.and_utc().timestamp_millis();
            let edt_ok = in_dst(as_utc + 4 * 3_600_000);
            let est_ok = !in_dst(as_utc + 5 * 3_600_000);
            match (edt_ok, est_ok) {
                (true, true) => LocalResult::Ambiguous(EDT, EST),
                (true, false) => LocalResult::Single(EDT),
                (false, true) => LocalResult::Single(EST),
                (false, false) => LocalResult::None,
            }
        }

        fn offset_from_utc_date(&self, utc: &NaiveDate) -> EasternOffset {
            self.offset_from_utc_datetime(&utc.and_hms_opt(0, 0, 0).unwrap())
        }

        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> EasternOffset {
            if in_dst(utc.and_utc().timestamp_millis()) {
                EDT
            } else {
                EST
            }
        }
    }

    /// Local wall-clock time in `tz` to epoch ms (first instant if ambiguous).
    fn at<Tz: TimeZone>(tz: &Tz, y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        tz.with_ymd_and_hms(y, mo, d, h, mi, 0)
            .earliest()
            .expect("valid local time")
            .timestamp_millis()
    }

    fn row(start_ms: i64, distance_m: f64, duration_s: i64, calories: f64) -> SessionRow {
        SessionRow {
            start_ms,
            distance_m,
            duration_s,
            calories,
        }
    }

    #[test]
    fn eastern_test_zone_behaves() {
        // Sanity-check the fake zone before relying on it.
        let z = Eastern;
        assert_eq!(at(&z, 2026, 3, 8, 1, 59), DST_START_UTC_MS - 60_000);
        // 02:30 on spring-forward day does not exist; 03:00 EDT is the start instant.
        assert_eq!(z.with_ymd_and_hms(2026, 3, 8, 2, 30, 0), LocalResult::None);
        assert_eq!(at(&z, 2026, 3, 8, 3, 0), DST_START_UTC_MS);
        // 01:30 on fall-back day happens twice.
        assert!(matches!(
            z.with_ymd_and_hms(2026, 11, 1, 1, 30, 0),
            LocalResult::Ambiguous(_, _)
        ));
    }

    #[test]
    fn no_sessions_gives_zeroes_and_no_change() {
        let now = at(&Utc, 2026, 11, 11, 10, 0); // a Wednesday
        let s = compute(&[], now, &Utc);
        assert_eq!(s.this_week, WeekStats::default());
        assert_eq!(s.last_week, WeekStats::default());
        assert_eq!(s.week_before, WeekStats::default());
        assert_eq!(s.last_week_id, "2026-W45");
        assert_eq!(
            s.distance_change,
            Change {
                pct: None,
                is_new: false
            }
        );
        assert!(!s.show_card);
        assert!(!card_trigger(&s, None));
    }

    #[test]
    fn sums_each_week_and_finds_active_days_and_longest_day() {
        // Wed 2026-11-11. This week from Mon Nov 9; last week Nov 2-8; before Oct 26-Nov 1.
        let now = at(&Utc, 2026, 11, 11, 10, 0);
        let rows = [
            // Week before: one 2 km day.
            row(at(&Utc, 2026, 10, 28, 7, 0), 2000.0, 1200, 100.0),
            // Last week: Tue 3 km + 1 km (same day), Thu 2 km, Sat 30 m (not active).
            row(at(&Utc, 2026, 11, 3, 7, 0), 3000.0, 1800, 150.0),
            row(at(&Utc, 2026, 11, 3, 18, 0), 1000.0, 600, 50.0),
            row(at(&Utc, 2026, 11, 5, 12, 0), 1970.0, 1500, 90.0),
            row(at(&Utc, 2026, 11, 7, 12, 0), 30.0, 40, 1.0),
            // This week so far.
            row(at(&Utc, 2026, 11, 10, 7, 0), 1500.0, 900, 60.0),
        ];
        let s = compute(&rows, now, &Utc);

        assert_eq!(s.week_before.sessions, 1);
        assert_eq!(s.week_before.distance_km, 2.0);

        let last = &s.last_week;
        assert_eq!(last.sessions, 4);
        assert_eq!(last.distance_km, 6.0);
        assert_eq!(last.duration_s, 1800 + 600 + 1500 + 40);
        assert_eq!(last.active_days, 2); // Tue and Thu; the 30 m Saturday walk is not enough
        assert_eq!(last.longest_day_km, 4.0);
        assert!((last.calories - 291.0).abs() < 1e-9);

        assert_eq!(s.this_week.sessions, 1);
        assert_eq!(s.this_week.active_days, 1);
        assert_eq!(s.this_week.distance_km, 1.5);

        // 6 km against 2 km: +200%.
        assert_eq!(
            s.distance_change,
            Change {
                pct: Some(200),
                is_new: false
            }
        );
        assert!(card_trigger(&s, None));
        assert!(card_trigger(&s, Some("2026-W44")));
        assert!(!card_trigger(&s, Some("2026-W45")));
    }

    #[test]
    fn active_day_needs_more_than_fifty_metres() {
        let now = at(&Utc, 2026, 11, 11, 10, 0);
        let day = |m: f64| [row(at(&Utc, 2026, 11, 3, 7, 0), m, 60, 1.0)];
        assert_eq!(compute(&day(50.0), now, &Utc).last_week.active_days, 0);
        assert_eq!(compute(&day(50.1), now, &Utc).last_week.active_days, 1);
        // Two short walks on one day don't add up to an active day on their own.
        let two = [
            row(at(&Utc, 2026, 11, 3, 7, 0), 40.0, 30, 1.0),
            row(at(&Utc, 2026, 11, 3, 8, 0), 40.0, 30, 1.0),
        ];
        let s = compute(&two, now, &Utc);
        assert_eq!(s.last_week.active_days, 0);
        assert_eq!(s.last_week.sessions, 2);
        assert_eq!(s.last_week.distance_km, 0.08);
    }

    #[test]
    fn week_boundaries_are_local_midnight_monday() {
        let now = at(&Utc, 2026, 11, 11, 10, 0);
        let rows = [
            row(at(&Utc, 2026, 11, 1, 23, 59) + 59_999, 1000.0, 60, 1.0), // Sun 23:59:59.999, before
            row(at(&Utc, 2026, 11, 2, 0, 0), 2000.0, 60, 1.0), // Mon 00:00 starts last week
            row(at(&Utc, 2026, 11, 8, 23, 59) + 59_999, 4000.0, 60, 1.0), // Sun 23:59:59.999, last
            row(at(&Utc, 2026, 11, 9, 0, 0), 8000.0, 60, 1.0), // Mon 00:00 starts this week
        ];
        let s = compute(&rows, now, &Utc);
        assert_eq!(s.week_before.distance_km, 1.0);
        assert_eq!(s.last_week.distance_km, 6.0);
        assert_eq!(s.this_week.distance_km, 8.0);
    }

    #[test]
    fn weeks_follow_the_local_zone_not_utc() {
        // UTC-8: Sunday 7 pm local is already Monday in UTC.
        let tz = FixedOffset::west_opt(8 * 3600).unwrap();
        let now = at(&tz, 2026, 11, 11, 10, 0);
        let sunday_evening = at(&tz, 2026, 11, 8, 19, 0);
        assert_eq!(
            DateTime::<Utc>::from_timestamp_millis(sunday_evening)
                .unwrap()
                .weekday(),
            chrono::Weekday::Mon
        );
        let s = compute(&[row(sunday_evening, 1000.0, 60, 1.0)], now, &tz);
        assert_eq!(s.last_week.sessions, 1);
        assert_eq!(s.this_week.sessions, 0);
    }

    #[test]
    fn spring_forward_week_keeps_its_edges() {
        // The week of Mon Mar 2 to Sun Mar 8 2026 loses an hour on Sunday.
        let z = Eastern;
        let now = at(&z, 2026, 3, 11, 9, 0);
        let rows = [
            row(at(&z, 2026, 3, 1, 23, 59), 1000.0, 60, 1.0), // before
            row(at(&z, 2026, 3, 2, 0, 0), 2000.0, 60, 1.0),   // last week, Monday 00:00 EST
            row(at(&z, 2026, 3, 8, 3, 0), 4000.0, 60, 1.0),   // last week, right after the jump
            row(at(&z, 2026, 3, 8, 23, 30), 8000.0, 60, 1.0), // last week, Sunday 23:30 EDT
            row(at(&z, 2026, 3, 9, 0, 10), 16000.0, 60, 1.0), // this week, Monday 00:10 EDT
        ];
        // Sunday 23:30 EDT is Monday in UTC; a UTC (or 7 x 24 h) split would get it wrong.
        assert_eq!(rows[2].start_ms, DST_START_UTC_MS);
        let s = compute(&rows, now, &z);
        assert_eq!(s.week_before.distance_km, 1.0);
        assert_eq!(s.last_week.distance_km, 14.0);
        assert_eq!(s.last_week.sessions, 3);
        assert_eq!(s.last_week.active_days, 2); // Mon Mar 2 and Sun Mar 8
        assert_eq!(s.this_week.distance_km, 16.0);
        assert_eq!(s.last_week_id, "2026-W10");

        // Seen from the Monday after, "now" in the DST week itself still lands right.
        let monday_early = at(&z, 2026, 3, 9, 0, 5);
        assert_eq!(current_week_id(monday_early, &z), "2026-W11");
        assert_eq!(current_week_id(at(&z, 2026, 3, 8, 23, 59), &z), "2026-W10");
    }

    #[test]
    fn fall_back_week_keeps_its_edges() {
        // Mon Oct 26 to Sun Nov 1 2026 gains an hour, and 01:30 on Sunday happens twice.
        let z = Eastern;
        let now = at(&z, 2026, 11, 4, 9, 0);
        let first_130 = at(&z, 2026, 11, 1, 1, 30); // EDT
        let second_130 = first_130 + 3_600_000; // EST
        assert_eq!(
            z.with_ymd_and_hms(2026, 11, 1, 1, 30, 0)
                .latest()
                .unwrap()
                .timestamp_millis(),
            second_130
        );
        let rows = [
            row(at(&z, 2026, 10, 25, 23, 59), 1000.0, 60, 1.0), // before
            row(at(&z, 2026, 10, 26, 0, 0), 2000.0, 60, 1.0),   // last week
            row(first_130, 4000.0, 60, 1.0),                    // last week, Sunday
            row(second_130, 8000.0, 60, 1.0),                   // same Sunday, an hour later
            row(at(&z, 2026, 11, 1, 23, 30), 16000.0, 60, 1.0), // last week, Sunday EST
            row(at(&z, 2026, 11, 2, 0, 10), 32000.0, 60, 1.0),  // this week
        ];
        let s = compute(&rows, now, &z);
        assert_eq!(s.week_before.distance_km, 1.0);
        assert_eq!(s.last_week.sessions, 4);
        assert_eq!(s.last_week.distance_km, 30.0);
        assert_eq!(s.last_week.active_days, 2); // Monday and the one Sunday
        assert_eq!(s.last_week.longest_day_km, 28.0);
        assert_eq!(s.this_week.distance_km, 32.0);
        assert_eq!(s.last_week_id, "2026-W44");
    }

    #[test]
    fn query_start_covers_the_week_before_last() {
        let z = Eastern;
        let now = at(&z, 2026, 3, 11, 9, 0); // Wed in the DST week's aftermath
        let since = query_since_ms(now, &z);
        let before_monday = at(&z, 2026, 2, 23, 0, 0);
        assert_eq!(since, before_monday - 86_400_000);
        // A session at the very start of the week before is inside the query.
        assert!(at(&z, 2026, 2, 23, 0, 0) >= since);
    }

    #[test]
    fn iso_week_ids_cross_the_year_boundary() {
        // Thu 2026-01-01 is in ISO week 2026-W01 (Monday Dec 29 2025).
        let now = at(&Utc, 2026, 1, 1, 12, 0);
        let s = compute(&[], now, &Utc);
        assert_eq!(current_week_id(now, &Utc), "2026-W01");
        assert_eq!(s.last_week_id, "2025-W52");
        // Last week spans the year end and still buckets by date.
        let rows = [
            row(at(&Utc, 2025, 12, 22, 8, 0), 1000.0, 60, 1.0), // Mon of 2025-W52
            row(at(&Utc, 2025, 12, 31, 20, 0), 2000.0, 60, 1.0),
        ];
        let s = compute(&rows, now, &Utc);
        assert_eq!(s.last_week.sessions, 1); // only Dec 22 is last week...
        assert_eq!(s.this_week.sessions, 1); // ...Dec 31 is already this week (W01 starts Dec 29)
    }

    #[test]
    fn change_percentages() {
        assert_eq!(
            change(150.0, 100.0),
            Change {
                pct: Some(50),
                is_new: false
            }
        );
        assert_eq!(
            change(100.0, 150.0),
            Change {
                pct: Some(-33),
                is_new: false
            }
        );
        assert_eq!(
            change(100.0, 100.0),
            Change {
                pct: Some(0),
                is_new: false
            }
        );
        assert_eq!(
            change(0.0, 5.0),
            Change {
                pct: Some(-100),
                is_new: false
            }
        );
        assert_eq!(
            change(5.0, 3.0),
            Change {
                pct: Some(67),
                is_new: false
            }
        );
        // From zero there is no percentage.
        assert_eq!(
            change(5.0, 0.0),
            Change {
                pct: None,
                is_new: true
            }
        );
        assert_eq!(
            change(0.0, 0.0),
            Change {
                pct: None,
                is_new: false
            }
        );
    }

    #[test]
    fn bad_rows_do_not_break_totals() {
        let now = at(&Utc, 2026, 11, 11, 10, 0);
        let rows = [
            row(at(&Utc, 2026, 11, 3, 7, 0), f64::NAN, -5, f64::INFINITY),
            row(at(&Utc, 2026, 11, 3, 8, 0), -40.0, 10, -2.0),
            row(i64::MAX, 1000.0, 60, 1.0), // unrepresentable date: skipped
            row(at(&Utc, 2026, 11, 16, 8, 0), 1000.0, 60, 1.0), // next week: skipped
            row(at(&Utc, 2026, 10, 1, 8, 0), 1000.0, 60, 1.0), // too old: skipped
        ];
        let s = compute(&rows, now, &Utc);
        assert_eq!(s.last_week.sessions, 2);
        assert_eq!(s.last_week.distance_km, 0.0);
        assert_eq!(s.last_week.duration_s, 10);
        assert_eq!(s.last_week.calories, 0.0);
        assert_eq!(s.this_week.sessions, 0);
        assert_eq!(s.week_before.sessions, 0);
    }

    #[test]
    fn card_needs_sessions_last_week_and_no_ack() {
        let now = at(&Utc, 2026, 11, 11, 10, 0);
        let busy = compute(
            &[row(at(&Utc, 2026, 11, 3, 7, 0), 1000.0, 60, 1.0)],
            now,
            &Utc,
        );
        assert!(card_trigger(&busy, None));
        assert!(card_trigger(&busy, Some("2026-W44")));
        assert!(!card_trigger(&busy, Some("2026-W45")));
        let idle = compute(&[], now, &Utc);
        assert!(!card_trigger(&idle, None));
        // Only tiny sessions (test starts, 20 m) last week: no active day, no card.
        let tiny = compute(
            &[row(at(&Utc, 2026, 11, 3, 7, 0), 20.0, 30, 1.0)],
            now,
            &Utc,
        );
        assert!(!card_trigger(&tiny, None));
    }

    #[test]
    fn ack_setting_parses_plain_and_quoted() {
        assert_eq!(parse_ack(Some("2026-W45".into())), Some("2026-W45".into()));
        assert_eq!(
            parse_ack(Some("\"2026-W45\"\n".into())),
            Some("2026-W45".into())
        );
        assert_eq!(parse_ack(Some("  ".into())), None);
        assert_eq!(parse_ack(None), None);
    }
}
