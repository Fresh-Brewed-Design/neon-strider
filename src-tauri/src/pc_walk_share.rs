//! PC time walked: the share of today's active PC time (keyboard or mouse
//! input within the last minute, DESK_SYNC's `ACTIVE_INPUT` rule) during which
//! the belt was running, i.e. how much of today's desk time was spent walking.
//!
//! The desk loop calls `tick` once a second. A second with recent input adds
//! one to today's active seconds, and one to its walked seconds as well when
//! the belt is Running (not paused, in countdown or winding down). A day the PC
//! isn't used has no seconds at all, so it shows nothing and breaks nothing.
//! There are no bests or targets: the share is only today's.
//!
//! Counts are per local day and saved in `desk_days` (migration 12) at most
//! once a minute, so a crash or an update's reinstall loses under a minute of
//! counting, which is too little to move the share; startup loads today's
//! counts back. Nothing here sends anything to the treadmill: it only reads
//! whether the belt runs.
//!
//! The counting is pure (`count`, `share_pct`, `view`). This file is declared
//! from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use crate::snapshot::desk_sync::ACTIVE_INPUT;
use crate::util::now_ms;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// No share until today has this much active PC time.
/// After one minute at the desk the share could only read 0 % or 100 %; ten
/// minutes is enough desk time for a part of it walked to mean something.
pub const MIN_ACTIVE_S: u32 = 10 * 60;
/// Today's counts are written at most this often.
/// Losing under a minute after a crash or a reinstall can't move the share by
/// a whole percent once it shows, and one write a minute keeps the disk quiet.
const SAVE_EVERY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PcWalkShare {
    /// Today's active PC seconds: seconds with keyboard or mouse input in the
    /// minute before.
    pub active_s: u32,
    /// The active PC seconds the belt was running.
    pub walked_s: u32,
    /// `walked_s` as a whole percent of `active_s`; None until today has 10
    /// minutes of active PC time.
    pub share_pct: Option<u32>,
}

/// One local day's counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub day: NaiveDate,
    pub active_s: u32,
    pub walked_s: u32,
}

// Pure logic.

/// One second: a new local day starts from zero; an active second counts, and
/// counts as walked too while the belt runs. A running belt with no input
/// (away from the PC) counts nothing.
pub fn count(tally: Tally, today: NaiveDate, active: bool, running: bool) -> Tally {
    let mut t = if tally.day == today {
        tally
    } else {
        Tally {
            day: today,
            ..Tally::default()
        }
    };
    if active {
        t.active_s = t.active_s.saturating_add(1);
        if running {
            t.walked_s = t.walked_s.saturating_add(1);
        }
    }
    t
}

/// The walked share as a whole percent, once there is enough desk time.
pub fn share_pct(active_s: u32, walked_s: u32) -> Option<u32> {
    if active_s < MIN_ACTIVE_S {
        return None;
    }
    let walked = u64::from(walked_s.min(active_s));
    Some(((walked * 100 + u64::from(active_s) / 2) / u64::from(active_s)) as u32)
}

/// The snapshot for `today`. A tally from another day (no tick since
/// midnight) is today's zero.
pub fn view(tally: Tally, today: NaiveDate) -> PcWalkShare {
    if tally.day != today {
        return PcWalkShare::default();
    }
    PcWalkShare {
        active_s: tally.active_s,
        walked_s: tally.walked_s,
        share_pct: share_pct(tally.active_s, tally.walked_s),
    }
}

/// Today's tally after a restart: the saved counts when there are any.
pub fn resume(today: NaiveDate, saved: Option<(u32, u32)>) -> Tally {
    let (active_s, walked_s) = saved.unwrap_or_default();
    Tally {
        day: today,
        active_s,
        walked_s,
    }
}

fn local_today() -> NaiveDate {
    DateTime::<Utc>::from_timestamp_millis(now_ms())
        .map(|d| d.with_timezone(&chrono::Local).date_naive())
        .unwrap_or_default()
}

fn day_key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

// Runtime state.

struct State {
    tally: Tally,
    /// The counts last written for `tally.day`.
    saved: Tally,
    /// The last write started then.
    last_write: Option<Instant>,
    /// A write is in flight.
    writing: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Load today's saved counts. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let today = local_today();
    let saved = match persistence::pc_walk_share_on(app, &day_key(today)) {
        Ok(saved) => saved,
        Err(e) => {
            // Counting starts from zero; a save never lowers the stored counts.
            log::warn!("could not load today's PC walk share counts: {}", e);
            None
        }
    };
    let tally = resume(today, saved);
    *lock() = Some(State {
        tally,
        saved: tally,
        last_write: None,
        writing: false,
    });
}

/// Write `t` off-thread. `regular` writes mark the in-flight flag; the last
/// write of a day that just ended doesn't, so it never waits.
fn save(app: &AppHandle, t: Tally, regular: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = persistence::save_pc_walk_share(
            &app,
            &day_key(t.day),
            now_ms(),
            t.active_s,
            t.walked_s,
        );
        if let Err(e) = &result {
            log::warn!("could not save PC walk share counts: {}", e);
        }
        if let Some(st) = lock().as_mut() {
            if regular {
                st.writing = false;
            }
            if result.is_ok() && st.saved.day == t.day && t.active_s >= st.saved.active_s {
                st.saved = t;
            }
        }
    });
}

/// Once a second from the desk loop. `idle` is the PC's input idle time (None
/// when it can't be read: nothing counts), `running` whether the connected
/// belt is Running. Saves at most once a minute, plus once when a day ends.
pub fn tick(app: &AppHandle, idle: Option<Duration>, running: bool) {
    let active = idle.is_some_and(|d| d <= ACTIVE_INPUT);
    let today = local_today();
    let mut guard = lock();
    let Some(st) = guard.as_mut() else {
        return;
    };
    let before = st.tally;
    st.tally = count(before, today, active, running);
    if before.day != today {
        if before != st.saved {
            save(app, before, false);
        }
        st.saved = Tally {
            day: today,
            ..Tally::default()
        };
    }
    let due = st.last_write.is_none_or(|t| t.elapsed() >= SAVE_EVERY);
    if st.tally != st.saved && due && !st.writing {
        st.writing = true;
        // A failed write waits the same minute before the next try.
        st.last_write = Some(Instant::now());
        save(app, st.tally, true);
    }
}

/// The PC walk share for the snapshot. No I/O.
pub fn snapshot() -> PcWalkShare {
    let today = local_today();
    lock()
        .as_ref()
        .map(|st| view(st.tally, today))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// Run `n` seconds of the same input and belt.
    fn run(mut t: Tally, today: NaiveDate, n: u32, active: bool, running: bool) -> Tally {
        for _ in 0..n {
            t = count(t, today, active, running);
        }
        t
    }

    #[test]
    fn only_active_seconds_count_and_walked_needs_a_running_belt() {
        let today = d("2026-11-22");
        let mut t = resume(today, None);
        // At the desk, belt running.
        t = run(t, today, 600, true, true);
        // At the desk, belt paused or still.
        t = run(t, today, 900, true, false);
        // Away from the PC with the belt running: neither counts.
        t = run(t, today, 300, false, true);
        // Away, belt still.
        t = run(t, today, 300, false, false);
        assert_eq!((t.active_s, t.walked_s), (1_500, 600));
        assert_eq!(view(t, today).share_pct, Some(40));
    }

    #[test]
    fn no_share_until_ten_minutes_at_the_desk() {
        let today = d("2026-11-22");
        let t = run(resume(today, None), today, MIN_ACTIVE_S - 1, true, true);
        let v = view(t, today);
        assert_eq!((v.active_s, v.walked_s, v.share_pct), (599, 599, None));
        let t = count(t, today, true, false);
        assert_eq!(view(t, today).share_pct, Some(100));
        assert_eq!(share_pct(600, 0), Some(0));
        // Rounded to the nearest whole percent; walked never above active.
        assert_eq!(share_pct(3_000, 1_015), Some(34));
        assert_eq!(share_pct(3_000, 1_005), Some(34));
        assert_eq!(share_pct(3_000, 985), Some(33));
        assert_eq!(share_pct(600, 9_999), Some(100));
        assert_eq!(share_pct(0, 0), None);
    }

    #[test]
    fn a_new_local_day_starts_from_zero() {
        let mon = d("2026-11-23");
        let sun = d("2026-11-22");
        let t = run(resume(sun, None), sun, 2_000, true, true);
        // No tick since midnight: the snapshot shows today's zero.
        assert_eq!(view(t, mon), PcWalkShare::default());
        let t = count(t, mon, true, false);
        assert_eq!((t.day, t.active_s, t.walked_s), (mon, 1, 0));
        // An idle first second still moves the day on.
        let t = count(
            run(resume(sun, None), sun, 5, true, true),
            mon,
            false,
            false,
        );
        assert_eq!((t.day, t.active_s, t.walked_s), (mon, 0, 0));
    }

    #[test]
    fn a_restart_carries_on_from_the_saved_counts() {
        let today = d("2026-11-22");
        let t = resume(today, Some((3_600, 1_200)));
        let t = run(t, today, 400, true, true);
        assert_eq!((t.active_s, t.walked_s), (4_000, 1_600));
        assert_eq!(view(t, today).share_pct, Some(40));
        // Nothing saved today: from zero.
        assert_eq!(
            resume(today, None),
            Tally {
                day: today,
                active_s: 0,
                walked_s: 0
            }
        );
    }
}
