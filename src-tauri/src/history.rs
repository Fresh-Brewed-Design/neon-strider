//! The History page's reads: walks by date range, loaded a few months at a
//! time, and its totals and calendar as SQL aggregates over every walk, so a
//! long archive never drops out of the figures.
//!
//! Walks that start within the merge threshold of the previous one's end are
//! one walk on the page (`sessionMerge.ts`, `strava_plan::merge_sessions`).
//! The aggregates group the same way in SQL, and a range read reaches back
//! past its start for the earlier parts of a walk that straddles it.

use crate::persistence::{self, session_from_row, Session, SESSION_COLS};
use crate::util::blocking;
use chrono::{Datelike, Months, NaiveDate, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::AppHandle;

/// Calendar months in one "load older" step. Three months at three walks a
/// day is under 300 rows, quick to read and draw, and a few presses reach a
/// year back.
const OLDER_MONTHS: u32 = 3;

/// Totals of the walks in a range, each joined walk counted once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HistoryTotals {
    pub meters: f64,
    #[ts(type = "number")]
    pub seconds: i64,
    pub kcal: f64,
    pub sessions: u32,
}

/// Metres walked on one local day (`YYYY-MM-DD`), for the calendar.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DayMeters {
    pub day: String,
    pub meters: f64,
}

/// What History shows above its list.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HistoryStats {
    pub totals: HistoryTotals,
    pub meters_by_day: Vec<DayMeters>,
    /// Start of the oldest saved walk; `None` with nothing saved.
    #[ts(type = "number | null")]
    pub oldest_start_ms: Option<i64>,
}

/// An older page of walks and the start of the window it now reaches.
#[derive(Debug, Clone, Serialize)]
pub struct OlderWalks {
    pub sessions: Vec<Session>,
    /// `None` when no walk is older than the one asked about.
    pub from_ms: Option<i64>,
}

/// Every walk folded into joined walks: `start` of the first part and the
/// summed counters. A walk starts a new group unless it begins within
/// `?1` ms of the previous one's end, the rule `mergeSessions` applies.
/// It reads the whole table; the window functions take a few milliseconds
/// even over years of walks, and the page asks once per refresh.
const GROUPS_SQL: &str = "
    WITH flagged AS (
        SELECT id, start_time, distance_meters, duration_seconds, calories,
               CASE WHEN start_time - LAG(end_time) OVER (ORDER BY start_time, id) < ?1
                    THEN 0 ELSE 1 END AS fresh
        FROM sessions
    ), numbered AS (
        SELECT *, SUM(fresh) OVER (ORDER BY start_time, id ROWS UNBOUNDED PRECEDING) AS grp
        FROM flagged
    )
    SELECT MIN(start_time) AS start, TOTAL(distance_meters) AS meters,
           CAST(TOTAL(duration_seconds) AS INTEGER) AS seconds, TOTAL(calories) AS kcal
    FROM numbered GROUP BY grp";

fn merge_ms(minutes: u32) -> i64 {
    i64::from(minutes) * 60_000
}

/// Totals of the joined walks starting in `[from, to)`; open ends are `None`.
pub fn totals_in(
    conn: &Connection,
    from: Option<i64>,
    to: Option<i64>,
    merge_minutes: u32,
) -> rusqlite::Result<HistoryTotals> {
    conn.query_row(
        &format!(
            "SELECT COUNT(*), TOTAL(meters), CAST(TOTAL(seconds) AS INTEGER), TOTAL(kcal)
             FROM ({GROUPS_SQL}) WHERE start >= ?2 AND start < ?3"
        ),
        params![
            merge_ms(merge_minutes),
            from.unwrap_or(i64::MIN),
            to.unwrap_or(i64::MAX)
        ],
        |r| {
            Ok(HistoryTotals {
                sessions: r.get(0)?,
                meters: r.get(1)?,
                seconds: r.get(2)?,
                kcal: r.get(3)?,
            })
        },
    )
}

/// Metres per local day of the joined walks starting at or after `from`,
/// oldest day first. A joined walk counts on the day it started, as the list
/// files it.
pub fn meters_by_day<Tz: TimeZone>(
    conn: &Connection,
    from: i64,
    merge_minutes: u32,
    tz: &Tz,
) -> rusqlite::Result<Vec<DayMeters>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT start, meters FROM ({GROUPS_SQL}) WHERE start >= ?2 ORDER BY start"
    ))?;
    let rows = stmt.query_map(params![merge_ms(merge_minutes), from], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
    })?;
    let mut out: Vec<DayMeters> = Vec::new();
    for row in rows {
        let (start, meters) = row?;
        let Some(at) = tz.timestamp_millis_opt(start).single() else {
            continue;
        };
        let day = at.date_naive().format("%Y-%m-%d").to_string();
        match out.last_mut() {
            Some(last) if last.day == day => last.meters += meters,
            _ => out.push(DayMeters { day, meters }),
        }
    }
    Ok(out)
}

pub fn oldest_start(conn: &Connection) -> rusqlite::Result<Option<i64>> {
    conn.query_row("SELECT MIN(start_time) FROM sessions", [], |r| r.get(0))
}

/// Walks starting in `[from, before)`, newest first, plus any earlier parts of
/// a joined walk that the range cuts, so the page never shows half a walk.
pub fn sessions_between(
    conn: &Connection,
    from: i64,
    before: Option<i64>,
    merge_minutes: u32,
) -> rusqlite::Result<Vec<Session>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SESSION_COLS} FROM sessions WHERE start_time >= ?1 AND start_time < ?2
         ORDER BY start_time DESC, id DESC"
    ))?;
    let mut out = stmt
        .query_map(params![from, before.unwrap_or(i64::MAX)], session_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut prev = conn.prepare(&format!(
        "SELECT {SESSION_COLS} FROM sessions WHERE (start_time, id) < (?1, ?2)
         ORDER BY start_time DESC, id DESC LIMIT 1"
    ))?;
    while let Some(first) = out.last() {
        let earlier = prev
            .query_row(
                params![first.start_time, first.id.unwrap_or(0)],
                session_from_row,
            )
            .optional()?;
        match earlier {
            Some(e) if first.start_time - e.end_time < merge_ms(merge_minutes) => out.push(e),
            _ => break,
        }
    }
    Ok(out)
}

/// Local midnight on the first of the month `back` months before the one
/// holding `at_ms`.
fn month_start_before<Tz: TimeZone>(at_ms: i64, back: u32, tz: &Tz) -> Option<i64> {
    let day = tz.timestamp_millis_opt(at_ms).single()?.date_naive();
    let first = NaiveDate::from_ymd_opt(day.year(), day.month(), 1)?
        .checked_sub_months(Months::new(back))?;
    let midnight = tz
        .from_local_datetime(&first.and_hms_opt(0, 0, 0)?)
        .earliest()?;
    Some(midnight.timestamp_millis())
}

/// The next page before `before`: the three calendar months ending with the
/// month of the newest walk older than `before`, so a long break never gives
/// an empty page.
pub fn older_walks<Tz: TimeZone>(
    conn: &Connection,
    before: i64,
    merge_minutes: u32,
    tz: &Tz,
) -> rusqlite::Result<OlderWalks> {
    let newest: Option<i64> = conn.query_row(
        "SELECT MAX(start_time) FROM sessions WHERE start_time < ?1",
        params![before],
        |r| r.get(0),
    )?;
    let Some(from) = newest.and_then(|at| month_start_before(at, OLDER_MONTHS - 1, tz)) else {
        return Ok(OlderWalks {
            sessions: Vec::new(),
            from_ms: None,
        });
    };
    Ok(OlderWalks {
        sessions: sessions_between(conn, from, Some(before), merge_minutes)?,
        from_ms: Some(from),
    })
}

/// Walks starting at or after `from_ms` (and before `before_ms`, when given),
/// newest first.
#[tauri::command]
pub async fn history_sessions(
    app: AppHandle,
    from_ms: i64,
    before_ms: Option<i64>,
) -> Result<Vec<Session>, String> {
    blocking(move || {
        let minutes = crate::strava::merge_minutes(&app);
        persistence::with_conn(&app, |conn| {
            sessions_between(conn, from_ms, before_ms, minutes)
        })
    })
    .await
}

/// The next few months of walks older than `before_ms`.
#[tauri::command]
pub async fn history_older(app: AppHandle, before_ms: i64) -> Result<OlderWalks, String> {
    blocking(move || {
        let minutes = crate::strava::merge_minutes(&app);
        persistence::with_conn(&app, |conn| {
            older_walks(conn, before_ms, minutes, &chrono::Local)
        })
    })
    .await
}

/// Totals for `[from_ms, to_ms)` over every saved walk, the calendar from
/// `calendar_from_ms`, and where the archive starts.
#[tauri::command]
pub async fn history_stats(
    app: AppHandle,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    calendar_from_ms: i64,
) -> Result<HistoryStats, String> {
    blocking(move || {
        let minutes = crate::strava::merge_minutes(&app);
        persistence::with_conn(&app, |conn| {
            Ok(HistoryStats {
                totals: totals_in(conn, from_ms, to_ms, minutes)?,
                meters_by_day: meters_by_day(conn, calendar_from_ms, minutes, &chrono::Local)?,
                oldest_start_ms: oldest_start(conn)?,
            })
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Utc};

    const DAY: i64 = 86_400_000;
    const MIN: i64 = 60_000;
    /// 2025-01-01T00:00:00Z.
    const T0: i64 = 1_735_689_600_000;

    fn add(conn: &Connection, start: i64, minutes: i64, meters: f64) {
        conn.execute(
            "INSERT INTO sessions (start_time, end_time, duration_seconds, distance_meters, calories)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![start, start + minutes * MIN, minutes * 60, meters, meters / 10.0],
        )
        .unwrap();
    }

    /// Three walks a day at 08:00, 12:00 and 17:00 UTC for `days` days from T0.
    fn seeded(days: i64) -> Connection {
        let conn = persistence::memory_db();
        for d in 0..days {
            for h in [8, 12, 17] {
                add(&conn, T0 + d * DAY + h * 60 * MIN, 30, 2000.0);
            }
        }
        conn
    }

    #[test]
    fn totals_cover_every_walk_beyond_a_thousand() {
        let conn = seeded(400); // 1200 walks
        let all = totals_in(&conn, None, None, 10).unwrap();
        assert_eq!(all.sessions, 1200);
        assert_eq!(all.meters, 1200.0 * 2000.0);
        assert_eq!(all.seconds, 1200 * 30 * 60);
        assert!((all.kcal - 1200.0 * 200.0).abs() < 1e-6);
        assert_eq!(oldest_start(&conn).unwrap(), Some(T0 + 8 * 60 * MIN));
    }

    #[test]
    fn totals_of_a_range_count_walks_starting_in_it() {
        let conn = seeded(400);
        let week = totals_in(&conn, Some(T0 + 393 * DAY), Some(T0 + 400 * DAY), 10).unwrap();
        assert_eq!(week.sessions, 21);
        let first_day = totals_in(&conn, None, Some(T0 + DAY), 10).unwrap();
        assert_eq!(first_day.sessions, 3);
    }

    #[test]
    fn walks_close_together_count_once() {
        let conn = persistence::memory_db();
        add(&conn, T0, 20, 1000.0);
        add(&conn, T0 + 25 * MIN, 20, 1000.0); // 5 min after the first ends
        add(&conn, T0 + 120 * MIN, 20, 500.0);
        let t = totals_in(&conn, None, None, 10).unwrap();
        assert_eq!(t.sessions, 2);
        assert_eq!(t.meters, 2500.0);
        // With a 2-minute threshold none join.
        assert_eq!(totals_in(&conn, None, None, 2).unwrap().sessions, 3);
        // A range starting inside the joined walk leaves it out, as the list does.
        assert_eq!(
            totals_in(&conn, Some(T0 + MIN), None, 10).unwrap().sessions,
            1
        );
    }

    #[test]
    fn calendar_sums_each_local_day() {
        let conn = seeded(400);
        let days = meters_by_day(&conn, T0 + 35 * DAY, 10, &Utc).unwrap();
        assert_eq!(days.len(), 365);
        assert_eq!(
            days[0],
            DayMeters {
                day: "2025-02-05".into(),
                meters: 6000.0
            }
        );
        // At UTC-10 the 08:00 UTC walk is the evening before.
        let hst = FixedOffset::west_opt(10 * 3600).unwrap();
        let shifted = meters_by_day(&conn, T0, 10, &hst).unwrap();
        assert_eq!(
            shifted[0],
            DayMeters {
                day: "2024-12-31".into(),
                meters: 2000.0
            }
        );
        assert_eq!(shifted[1].meters, 6000.0);
    }

    #[test]
    fn a_range_reads_only_its_walks_newest_first() {
        let conn = seeded(400);
        let rows = sessions_between(&conn, T0 + 390 * DAY, None, 10).unwrap();
        assert_eq!(rows.len(), 30);
        assert!(rows.windows(2).all(|w| w[0].start_time > w[1].start_time));
        let page = sessions_between(&conn, T0 + 10 * DAY, Some(T0 + 12 * DAY), 10).unwrap();
        assert_eq!(page.len(), 6);
    }

    #[test]
    fn a_range_reaches_back_for_a_joined_walk_it_cuts() {
        let conn = persistence::memory_db();
        add(&conn, T0 - 30 * MIN, 25, 1000.0); // ends 5 min before T0
        add(&conn, T0 + MIN, 20, 1000.0);
        add(&conn, T0 - DAY, 20, 1000.0);
        let rows = sessions_between(&conn, T0, None, 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].start_time, T0 - 30 * MIN);
        assert_eq!(sessions_between(&conn, T0, None, 2).unwrap().len(), 1);
    }

    #[test]
    fn older_pages_walk_back_through_the_archive() {
        let conn = seeded(400); // 2025-01-01 to 2026-02-04
        let mut before = T0 + 400 * DAY;
        let mut total = 0;
        let mut pages = 0;
        loop {
            let page = older_walks(&conn, before, 10, &Utc).unwrap();
            let Some(from) = page.from_ms else { break };
            assert!(!page.sessions.is_empty());
            assert!(page
                .sessions
                .iter()
                .all(|s| s.start_time >= from && s.start_time < before));
            total += page.sessions.len();
            before = from;
            pages += 1;
        }
        assert_eq!(total, 1200);
        assert_eq!(pages, 5); // Dec-Feb, Sep-Nov, Jun-Aug, Mar-May, Jan-Feb
    }

    #[test]
    fn an_older_page_skips_a_long_break() {
        let conn = persistence::memory_db();
        add(&conn, T0, 20, 1000.0); // January 2025
        let page = older_walks(&conn, T0 + 300 * DAY, 10, &Utc).unwrap();
        assert_eq!(page.sessions.len(), 1);
        // November, December and January: the month of the walk and two before.
        assert_eq!(page.from_ms, Some(T0 - 61 * DAY));
        assert!(older_walks(&conn, T0, 10, &Utc).unwrap().from_ms.is_none());
    }
}
