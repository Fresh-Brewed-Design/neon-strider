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
//! The same seconds also go into today's 5-minute slots (`SLOT_MIN`), saved
//! with the counts (migration 13), so `snapshot.today_timeline` can lay out
//! today from the first input to now: when the PC was in use and when the belt
//! walked during it. A day with no PC input has no timeline.
//!
//! The counting is pure (`count`, `share_pct`, `view`, `timeline`). This file
//! is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use crate::snapshot::desk_sync::ACTIVE_INPUT;
use crate::util::now_ms;
use chrono::{DateTime, NaiveDate, Timelike, Utc};
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
/// Minutes in one timeline slot.
/// Five minutes shows a short walk or a coffee break as its own mark, and a
/// whole day is 288 slots: a 10-hour desk day is 120 marks, about 5 px each
/// across the History page, still wide enough to see.
pub const SLOT_MIN: u32 = 5;
/// Slots in a local day. A day the clocks go back folds its repeated hour into
/// the same slots (a slot can then hold up to ten minutes); a day they go
/// forward leaves the skipped hour empty.
pub const SLOTS_PER_DAY: usize = (24 * 60 / SLOT_MIN) as usize;

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

/// One timeline slot's counts: active PC seconds and how many of them walked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Slot {
    pub active_s: u16,
    pub walked_s: u16,
}

/// What a stretch of today's timeline was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum TimelineMark {
    /// No keyboard or mouse input.
    Away,
    /// At the PC, with the belt still for most of that time.
    Desk,
    /// At the PC with the belt running for at least half of that time.
    Walked,
}

/// A run of slots with the same mark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TimelineRun {
    /// Minutes after local midnight the run starts.
    pub start_min: u32,
    /// Minutes after local midnight the run ends (the last one ends at now).
    pub end_min: u32,
    pub mark: TimelineMark,
}

/// Today laid out from the first slot with PC input to now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TodayTimeline {
    /// Minutes after local midnight the first slot with input starts.
    pub start_min: u32,
    /// Minutes after local midnight now is (the end of the current minute).
    pub now_min: u32,
    /// Minutes in one slot.
    pub slot_min: u32,
    /// Back-to-back runs from `start_min` to `now_min`.
    pub runs: Vec<TimelineRun>,
}

pub type Slots = [Slot; SLOTS_PER_DAY];

/// One local day's counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub day: NaiveDate,
    pub active_s: u32,
    pub walked_s: u32,
    pub slots: Slots,
}

impl Default for Tally {
    fn default() -> Self {
        Tally {
            day: NaiveDate::default(),
            active_s: 0,
            walked_s: 0,
            slots: [Slot::default(); SLOTS_PER_DAY],
        }
    }
}

/// What the database keeps for a day: (active, walked) seconds and the slots.
pub type Saved = (u32, u32, Slots);

// Pure logic.

/// The slot a minute of the local day falls in.
pub fn slot_of(minute: u32) -> usize {
    ((minute / SLOT_MIN) as usize).min(SLOTS_PER_DAY - 1)
}

/// One second at `minute` (minutes after local midnight): a new local day
/// starts from zero; an active second counts, in the day and in its slot, and
/// counts as walked too while the belt runs. A running belt with no input
/// (away from the PC) counts nothing.
pub fn count(tally: Tally, today: NaiveDate, minute: u32, active: bool, running: bool) -> Tally {
    let mut t = if tally.day == today {
        tally
    } else {
        Tally {
            day: today,
            ..Tally::default()
        }
    };
    if active {
        let slot = &mut t.slots[slot_of(minute)];
        t.active_s = t.active_s.saturating_add(1);
        slot.active_s = slot.active_s.saturating_add(1);
        if running {
            t.walked_s = t.walked_s.saturating_add(1);
            slot.walked_s = slot.walked_s.saturating_add(1);
        }
    }
    t
}

/// A slot's mark. Walked needs the belt running for at least half of the
/// slot's PC time, so a minute's walk in a slot of sitting doesn't paint it.
fn mark(slot: Slot) -> TimelineMark {
    if slot.active_s == 0 {
        TimelineMark::Away
    } else if u32::from(slot.walked_s) * 2 >= u32::from(slot.active_s) {
        TimelineMark::Walked
    } else {
        TimelineMark::Desk
    }
}

/// Today's timeline at `minute` (minutes after local midnight): runs from the
/// first slot with input to the end of the current minute. None when today
/// has no input yet (a tally from another day has none).
pub fn timeline(tally: &Tally, today: NaiveDate, minute: u32) -> Option<TodayTimeline> {
    if tally.day != today {
        return None;
    }
    let now = slot_of(minute);
    let first = tally.slots[..=now].iter().position(|s| s.active_s > 0)?;
    let now_min = (minute + 1).min(24 * 60);
    let mut runs: Vec<TimelineRun> = Vec::new();
    for (i, slot) in tally.slots.iter().enumerate().take(now + 1).skip(first) {
        let start_min = i as u32 * SLOT_MIN;
        let end_min = (start_min + SLOT_MIN).min(now_min);
        let m = mark(*slot);
        match runs.last_mut() {
            Some(run) if run.mark == m => run.end_min = end_min,
            _ => runs.push(TimelineRun {
                start_min,
                end_min,
                mark: m,
            }),
        }
    }
    Some(TodayTimeline {
        start_min: first as u32 * SLOT_MIN,
        now_min,
        slot_min: SLOT_MIN,
        runs,
    })
}

/// The slots as saved text: `slot:active:walked` for each slot with input,
/// comma-separated (a full desk day is a few kilobytes at most).
pub fn encode_slots(slots: &Slots) -> String {
    slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.active_s > 0)
        .map(|(i, s)| format!("{}:{}:{}", i, s.active_s, s.walked_s))
        .collect::<Vec<_>>()
        .join(",")
}

/// The slots from saved text. A part that doesn't read is skipped, so a damaged
/// row loses those slots and keeps the rest.
pub fn decode_slots(text: &str) -> Slots {
    let mut slots = [Slot::default(); SLOTS_PER_DAY];
    for part in text.split(',') {
        let mut it = part.split(':').map(|n| n.trim().parse::<u16>().ok());
        let (Some(Some(i)), Some(Some(active_s)), Some(Some(walked_s)), None) =
            (it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        if let Some(slot) = slots.get_mut(usize::from(i)) {
            *slot = Slot {
                active_s,
                walked_s: walked_s.min(active_s),
            };
        }
    }
    slots
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

/// Today's tally after a restart: the saved counts and slots when there are any.
pub fn resume(today: NaiveDate, saved: Option<Saved>) -> Tally {
    match saved {
        Some((active_s, walked_s, slots)) => Tally {
            day: today,
            active_s,
            walked_s,
            slots,
        },
        None => Tally {
            day: today,
            ..Tally::default()
        },
    }
}

/// The local day and the minute of it, from one clock reading.
fn local_now() -> (NaiveDate, u32) {
    DateTime::<Utc>::from_timestamp_millis(now_ms())
        .map(|d| {
            let local = d.with_timezone(&chrono::Local);
            (local.date_naive(), local.hour() * 60 + local.minute())
        })
        .unwrap_or_default()
}

fn local_today() -> NaiveDate {
    local_now().0
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
        Ok(saved) => {
            saved.map(|(active_s, walked_s, slots)| (active_s, walked_s, decode_slots(&slots)))
        }
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
            &encode_slots(&t.slots),
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
    let (today, minute) = local_now();
    let mut guard = lock();
    let Some(st) = guard.as_mut() else {
        return;
    };
    let before = st.tally;
    st.tally = count(before, today, minute, active, running);
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

/// Today's timeline for the snapshot. No I/O.
pub fn today_timeline() -> Option<TodayTimeline> {
    let (today, minute) = local_now();
    lock()
        .as_ref()
        .and_then(|st| timeline(&st.tally, today, minute))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// Run `n` seconds of the same input and belt.
    fn run(mut t: Tally, today: NaiveDate, n: u32, active: bool, running: bool) -> Tally {
        run_at(t, today, 9 * 60, n, active, running)
    }

    /// Run `n` seconds from `minute` on, a minute every 60 seconds.
    fn run_at(
        mut t: Tally,
        today: NaiveDate,
        minute: u32,
        n: u32,
        active: bool,
        running: bool,
    ) -> Tally {
        for i in 0..n {
            t = count(t, today, minute + i / 60, active, running);
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
        let t = count(t, today, 9 * 60, true, false);
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
        let t = count(t, mon, 0, true, false);
        assert_eq!((t.day, t.active_s, t.walked_s), (mon, 1, 0));
        // An idle first second still moves the day on.
        let t = count(
            run(resume(sun, None), sun, 5, true, true),
            mon,
            0,
            false,
            false,
        );
        assert_eq!((t.day, t.active_s, t.walked_s), (mon, 0, 0));
    }

    #[test]
    fn a_restart_carries_on_from_the_saved_counts() {
        let today = d("2026-11-22");
        let t = resume(
            today,
            Some((3_600, 1_200, [Slot::default(); SLOTS_PER_DAY])),
        );
        let t = run(t, today, 400, true, true);
        assert_eq!((t.active_s, t.walked_s), (4_000, 1_600));
        assert_eq!(view(t, today).share_pct, Some(40));
        // Nothing saved today: from zero.
        assert_eq!(
            resume(today, None),
            Tally {
                day: today,
                ..Tally::default()
            }
        );
    }

    fn marks(tl: &TodayTimeline) -> Vec<(u32, u32, TimelineMark)> {
        tl.runs
            .iter()
            .map(|r| (r.start_min, r.end_min, r.mark))
            .collect()
    }

    #[test]
    fn the_timeline_runs_from_the_first_input_to_now() {
        use TimelineMark::*;
        let today = d("2026-11-23");
        let mut t = resume(today, None);
        // 08:02 to 08:30 at the desk, 08:30 to 09:00 walking, away until
        // 09:20, then at the desk again.
        t = run_at(t, today, 8 * 60 + 2, 28 * 60, true, false);
        t = run_at(t, today, 8 * 60 + 30, 30 * 60, true, true);
        t = run_at(t, today, 9 * 60, 20 * 60, false, true);
        t = run_at(t, today, 9 * 60 + 20, 7 * 60, true, false);
        let tl = timeline(&t, today, 9 * 60 + 26).unwrap();
        assert_eq!((tl.start_min, tl.now_min, tl.slot_min), (480, 567, 5));
        assert_eq!(
            marks(&tl),
            vec![
                (480, 510, Desk),
                (510, 540, Walked),
                (540, 560, Away),
                (560, 567, Desk)
            ]
        );
        // Later with no input: the strip reaches now as away.
        let tl = timeline(&t, today, 10 * 60 + 3).unwrap();
        assert_eq!(
            tl.runs.last().unwrap(),
            &TimelineRun {
                start_min: 570,
                end_min: 604,
                mark: Away
            }
        );
    }

    #[test]
    fn walked_needs_half_the_slots_pc_time() {
        use TimelineMark::*;
        let today = d("2026-11-23");
        // 10:00 slot: 150 s walked of 300. 10:05 slot: 149 of 300.
        let mut t = run_at(resume(today, None), today, 600, 150, true, true);
        t = run_at(t, today, 602, 150, true, false);
        t = run_at(t, today, 605, 149, true, true);
        t = run_at(t, today, 607, 151, true, false);
        let tl = timeline(&t, today, 609).unwrap();
        assert_eq!(marks(&tl), vec![(600, 605, Walked), (605, 610, Desk)]);
    }

    #[test]
    fn no_timeline_without_input_today() {
        let today = d("2026-11-23");
        let t = run_at(resume(today, None), today, 480, 600, false, true);
        assert_eq!(timeline(&t, today, 500), None);
        // Yesterday's input doesn't count for today.
        let sun = d("2026-11-22");
        let t = run_at(resume(sun, None), sun, 480, 600, true, true);
        assert_eq!(timeline(&t, today, 500), None);
        // The last minute of the day ends at midnight.
        let t = run_at(resume(today, None), today, 1_439, 30, true, false);
        let tl = timeline(&t, today, 1_439).unwrap();
        assert_eq!((tl.start_min, tl.now_min), (1_435, 1_440));
    }

    #[test]
    fn slots_survive_a_save_and_a_restart() {
        let today = d("2026-11-23");
        let t = run_at(resume(today, None), today, 480, 600, true, true);
        let t = run_at(t, today, 490, 300, true, false);
        let text = encode_slots(&t.slots);
        assert_eq!(text, "96:300:300,97:300:300,98:300:0");
        let back = resume(today, Some((t.active_s, t.walked_s, decode_slots(&text))));
        assert_eq!(back, t);
        // A damaged part is skipped; walked never above active; out of range dropped.
        let slots = decode_slots("96:300:300,x,97:10:99,400:5:5,98:1:1:1,");
        assert_eq!(
            slots[96],
            Slot {
                active_s: 300,
                walked_s: 300
            }
        );
        assert_eq!(
            slots[97],
            Slot {
                active_s: 10,
                walked_s: 10
            }
        );
        assert_eq!(slots[98], Slot::default());
        assert_eq!(decode_slots(""), [Slot::default(); SLOTS_PER_DAY]);
    }
}
