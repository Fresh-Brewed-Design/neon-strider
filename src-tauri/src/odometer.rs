//! Lifetime odometer: total distance and belt running hours across every saved
//! session plus the live one, and the date tracking began (the earliest session).
//!
//! Like the daily and weekly totals, the saved-sessions part is cached and
//! refreshed off-thread (startup, session change, a session deleted, once a
//! minute); the snapshot only adds the live session on top. The sum is a pure
//! function so it can be unit-tested. "Belt hours" is the sessions' duration,
//! which is the deck's own timer and runs while the belt does.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct OdometerSnapshot {
    /// Everything ever walked, including the live session.
    pub distance_km: f64,
    /// Belt running time, including the live session.
    pub belt_hours: f64,
    /// Start of the earliest session (epoch ms); `None` before the first walk.
    #[ts(type = "number | null")]
    pub since_ms: Option<i64>,
}

/// Totals from the saved sessions (the live one excluded).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Saved {
    meters: f64,
    seconds: i64,
    first_start_ms: Option<i64>,
}

// Pure logic.

/// Add the live session (`live_m`, `live_s`; zero when none is open) to the saved
/// totals. Bad numbers count as zero. With no saved session the date falls back
/// to when the live one must have started, and with neither it is `None`.
fn compute(
    saved: &Saved,
    live_m: f64,
    live_s: i64,
    live_open: bool,
    now_ms: i64,
) -> OdometerSnapshot {
    let clean = |m: f64| if m.is_finite() { m.max(0.0) } else { 0.0 };
    let meters = clean(saved.meters) + clean(live_m);
    let seconds = saved.seconds.max(0) + live_s.max(0);

    let live_start = live_open.then(|| now_ms - live_s.max(0) * 1000);
    let since_ms = match (saved.first_start_ms, live_start) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };

    OdometerSnapshot {
        distance_km: meters / 1000.0,
        belt_hours: seconds as f64 / 3600.0,
        since_ms,
    }
}

// Cached state.

struct State {
    saved: Saved,
    /// Live session id at the last refresh.
    active_id: Option<i64>,
}

static STATE: Mutex<State> = Mutex::new(State {
    saved: Saved {
        meters: 0.0,
        seconds: 0,
        first_start_ms: None,
    },
    active_id: None,
});

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Load the saved totals. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    refresh(app, None);
}

/// Re-read the saved totals from SQLite, skipping the live session `active_id`
/// (the snapshot adds it). On a query error the old totals stay. Blocking; never
/// called per snapshot tick.
pub fn refresh(app: &AppHandle, active_id: Option<i64>) {
    match persistence::lifetime_totals(app, active_id) {
        Ok((meters, seconds, first_start_ms)) => {
            let mut st = lock();
            st.saved = Saved {
                meters,
                seconds,
                first_start_ms,
            };
            st.active_id = active_id;
        }
        Err(e) => log::warn!("odometer query failed: {}", e),
    }
}

/// Refresh with the live session as of the last refresh (for a deleted session).
pub fn refresh_current(app: &AppHandle) {
    let active = lock().active_id;
    refresh(app, active);
}

/// Snapshot from the cache plus the live session (distance and belt seconds; pass
/// zeros and `false` when no session is open). No I/O.
pub fn snapshot(live_m: f64, live_s: i64, live_open: bool) -> OdometerSnapshot {
    let saved = lock().saved;
    compute(&saved, live_m, live_s, live_open, crate::util::now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    fn saved(m: f64, s: i64, first: Option<i64>) -> Saved {
        Saved {
            meters: m,
            seconds: s,
            first_start_ms: first,
        }
    }

    #[test]
    fn empty_database_is_zero_with_no_date() {
        let o = compute(&Saved::default(), 0.0, 0, false, NOW);
        assert_eq!(o.distance_km, 0.0);
        assert_eq!(o.belt_hours, 0.0);
        assert_eq!(o.since_ms, None);
    }

    #[test]
    fn totals_add_the_live_session() {
        let first = NOW - 100 * 86_400_000;
        let o = compute(
            &saved(412_600.0, 138 * 3600 + 1800, Some(first)),
            1_500.0,
            1800,
            true,
            NOW,
        );
        assert!((o.distance_km - 414.1).abs() < 1e-9);
        assert!((o.belt_hours - 139.0).abs() < 1e-9);
        assert_eq!(o.since_ms, Some(first));
    }

    #[test]
    fn a_closed_session_leftover_is_not_counted_twice() {
        // The caller passes zeros when no session is open (it is already saved).
        let o = compute(&saved(1000.0, 600, Some(NOW - 1000)), 0.0, 0, false, NOW);
        assert_eq!(o.distance_km, 1.0);
        assert!((o.belt_hours - 600.0 / 3600.0).abs() < 1e-12);
    }

    #[test]
    fn first_walk_in_progress_dates_from_its_start() {
        let o = compute(&Saved::default(), 200.0, 120, true, NOW);
        assert_eq!(o.since_ms, Some(NOW - 120_000));
        assert!((o.distance_km - 0.2).abs() < 1e-12);
        // A saved session earlier than the live one wins.
        let o = compute(&saved(0.0, 0, Some(NOW - 9_999_999)), 0.0, 60, true, NOW);
        assert_eq!(o.since_ms, Some(NOW - 9_999_999));
    }

    #[test]
    fn bad_numbers_count_as_zero() {
        let o = compute(&saved(f64::NAN, -50, None), f64::INFINITY, -3, false, NOW);
        assert_eq!(o.distance_km, 0.0);
        assert_eq!(o.belt_hours, 0.0);
        assert_eq!(o.since_ms, None);
    }
}
