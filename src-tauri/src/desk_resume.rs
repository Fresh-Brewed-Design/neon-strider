//! Desk timers across a restart.
//!
//! The sit-too-long nudge, eye breaks and calls keep their state in memory
//! (`seated.rs`, `eyebreak.rs`, `calls.rs`). An app restart (an auto-install,
//! a crash, Quit and open again) would zero it: the seated time and the
//! eye-break count would start over, and a call that slowed the belt would
//! forget the speed to put back. So the desk loop saves what each machine
//! keeps (`Kept`) every few seconds, update.rs saves it right before the
//! installer runs, and startup puts it back.
//!
//! A save is used only while it is fresh: up to `MAX_AGE_MS` old, the time
//! being away from the PC already resets the seated time and the eye-break
//! count (`seated::AWAY_MS`, `eyebreak::AWAY_MS`). The seated time and the
//! eye-break count are also dropped when the PC has had no input for that long
//! by the time the app is back. The time the app was closed is not counted as
//! seated or screen time. Nothing here touches the belt: a restored call only
//! remembers the speed its end puts back, and that restore only lowers or puts
//! back the speed of a belt that is running.
//!
//! The save is one JSON row in the `automation_state` table (`KEY`).
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use crate::snapshot::{calls, card_queue, eyebreak, seated};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// The `automation_state` key the timers are saved under.
pub const KEY: &str = "desk_timers";
/// A save older than this is stale.
/// Fifteen minutes away from the PC resets both timers anyway.
pub const MAX_AGE_MS: u64 = 15 * 60 * 1000;
/// How often the desk loop saves while something changed.
/// A crash loses at most this much seated or screen time.
pub const SAVE_EVERY: Duration = Duration::from_secs(10);

/// What the desk timers keep across a restart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    /// Epoch ms of the save.
    pub saved_at_ms: u64,
    #[serde(default)]
    pub seated: seated::Kept,
    #[serde(default)]
    pub eye: eyebreak::Kept,
    #[serde(default)]
    pub call: calls::Kept,
    /// ATROPHY_WARNING was folded into the strip (`card_queue`).
    #[serde(default)]
    pub seated_folded: bool,
}

impl Saved {
    /// The kept state without the time of the save, to tell a change.
    fn same_state(&self, other: &Saved) -> bool {
        self.seated == other.seated
            && self.eye == other.eye
            && self.call == other.call
            && self.seated_folded == other.seated_folded
    }
}

/// The part of `saved` to put back at `now_ms`, with the PC idle for
/// `idle_ms` (none when unknown). `None` when the save is stale or from the
/// future (a clock set back).
pub fn usable(saved: Saved, now_ms: u64, idle_ms: Option<u64>) -> Option<Saved> {
    let age = now_ms.checked_sub(saved.saved_at_ms)?;
    if age > MAX_AGE_MS {
        return None;
    }
    let mut out = saved;
    // Away from the PC long enough to have reset the timers if the app had
    // been running. A call is kept: listening goes without input.
    if idle_ms.is_some_and(|i| i >= MAX_AGE_MS) {
        out.seated = seated::Kept::default();
        out.eye = eyebreak::Kept::default();
        out.seated_folded = false;
    }
    Some(out)
}

/// The three machines' state now.
pub fn capture(now_ms: u64) -> Saved {
    Saved {
        saved_at_ms: now_ms,
        seated: seated::with(|rt| rt.machine.kept()),
        eye: eyebreak::with(|rt| rt.machine.kept()),
        call: calls::with_machine(|m| m.kept()),
        seated_folded: card_queue::seated_folded(),
    }
}

/// Put a usable save back into the machines.
pub fn apply(saved: Saved) {
    seated::with(|rt| rt.machine.restore(saved.seated));
    eyebreak::with(|rt| rt.machine.restore(saved.eye));
    calls::with_machine(|m| m.restore(saved.call));
    if saved.seated_folded && saved.seated.showing {
        card_queue::restore_seated_folded();
    }
}

/// Read the save and put it back. Blocking; call once after the machines'
/// settings are loaded and before the desk loop starts.
pub fn restore(app: &AppHandle) {
    let Some(raw) = persistence::read_state(app, KEY) else {
        return;
    };
    let Ok(saved) = serde_json::from_str::<Saved>(&raw) else {
        log::warn!("Desk timers: the saved state could not be read");
        return;
    };
    let now = crate::util::now_ms_u64();
    let idle = crate::platform::input_idle_for().map(|d| d.as_millis() as u64);
    match usable(saved, now, idle) {
        Some(s) => {
            log::info!(
                "Desk timers: restored after restart ({} min seated, {} min screen time, call {})",
                s.seated.seated_ms / 60_000,
                s.eye.active_ms / 60_000,
                if s.call.call { "on" } else { "off" }
            );
            apply(s);
        }
        None => log::info!("Desk timers: the saved state is stale; starting fresh"),
    }
}

/// Write `saved`. Blocking (SQLite).
pub fn write(app: &AppHandle, saved: &Saved) {
    let value = serde_json::to_string(saved).unwrap_or_default();
    if let Err(e) = persistence::write_state(app, KEY, &value) {
        log::warn!("Desk timers: could not save: {}", e);
    }
}

/// Save the timers now. Blocking; for the moment before the app closes (an
/// update's installer, Quit).
pub fn save_now(app: &AppHandle) {
    write(app, &capture(crate::util::now_ms_u64()));
}

/// When the desk loop saves next.
#[derive(Debug, Default)]
pub struct Saver {
    last: Option<(Saved, Instant)>,
}

impl Saver {
    /// Whether `current` should be written at `now`: the first time, then at
    /// most every `SAVE_EVERY` while it changed. Marks it written.
    pub fn due(&mut self, current: Saved, now: Instant) -> bool {
        let due = match &self.last {
            None => true,
            Some((last, at)) => !last.same_state(&current) && now.duration_since(*at) >= SAVE_EVERY,
        };
        if due {
            self.last = Some((current, now));
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;
    const MIN: u64 = 60_000;

    fn saved() -> Saved {
        Saved {
            saved_at_ms: T0,
            seated: seated::Kept {
                seated_ms: 22 * MIN,
                quiet_until_ms: Some(T0 + 5 * MIN),
                snoozes: 1,
                showing: false,
            },
            eye: eyebreak::Kept {
                active_ms: 14 * MIN,
                ..Default::default()
            },
            call: calls::Kept {
                call: true,
                ..Default::default()
            },
            seated_folded: true,
        }
    }

    #[test]
    fn a_fresh_save_is_used_whole() {
        assert_eq!(usable(saved(), T0 + MIN, Some(0)), Some(saved()));
        assert_eq!(usable(saved(), T0 + MAX_AGE_MS, None), Some(saved()));
    }

    #[test]
    fn a_stale_save_or_one_from_the_future_is_dropped() {
        assert_eq!(usable(saved(), T0 + MAX_AGE_MS + 1, Some(0)), None);
        assert_eq!(usable(saved(), T0 - 1, Some(0)), None);
    }

    #[test]
    fn being_away_drops_the_timers_but_keeps_the_call() {
        let out = usable(saved(), T0 + MIN, Some(MAX_AGE_MS)).unwrap();
        assert_eq!(out.seated, seated::Kept::default());
        assert_eq!(out.eye, eyebreak::Kept::default());
        assert!(!out.seated_folded);
        assert!(out.call.call);
    }

    #[test]
    fn the_save_reads_back_and_an_old_one_without_fields_still_reads() {
        let json = serde_json::to_string(&saved()).unwrap();
        assert_eq!(serde_json::from_str::<Saved>(&json).unwrap(), saved());
        let old: Saved = serde_json::from_str(r#"{"saved_at_ms":5}"#).unwrap();
        assert_eq!(old.saved_at_ms, 5);
        assert_eq!(old.seated, seated::Kept::default());
    }

    #[test]
    fn the_saver_writes_first_then_only_changes_every_ten_seconds() {
        let mut s = Saver::default();
        let t = Instant::now();
        let a = saved();
        assert!(s.due(a, t));
        assert!(!s.due(a, t + Duration::from_secs(60)), "unchanged");
        let mut b = a;
        b.seated.seated_ms += 1000;
        assert!(!s.due(b, t + Duration::from_secs(5)), "too soon");
        assert!(s.due(b, t + Duration::from_secs(10)));
    }

    /// Each machine carries its state through a restart: seated time and a
    /// snooze, the eye-break count, and a call with the speed to put back.
    #[test]
    fn each_machine_comes_back_where_it_was() {
        // Sit-too-long: 20 minutes seated, then a restart, then 10 more.
        let cfg = seated::Config::default();
        let tick = seated::Tick {
            idle_ms: Some(0),
            belt_running: false,
            ready: true,
        };
        let mut before = seated::Seated::default();
        let mut t = 0;
        while t <= 20 * MIN {
            before.tick(&cfg, T0 + t, &tick);
            t += 1000;
        }
        assert_eq!(before.seated_minutes(), 20);
        let mut after = seated::Seated::default();
        after.restore(before.kept());
        // A minute closed: not counted.
        let back = T0 + 21 * MIN;
        let mut t = 0;
        while t <= 10 * MIN {
            after.tick(&cfg, back + t, &tick);
            t += 1000;
        }
        assert_eq!(after.seated_minutes(), 30);
        assert!(
            after.show_card(),
            "the card comes at 30 minutes, as without the restart"
        );

        // Eye breaks: the count goes on from where it was.
        let mut eye = eyebreak::EyeBreak::default();
        eye.restore(eyebreak::Kept {
            active_ms: 19 * MIN + 30_000,
            ..Default::default()
        });
        assert_eq!(eye.kept().active_ms, 19 * MIN + 30_000);

        // A call: the restored machine still knows the speed to put back.
        let mut call = calls::CallMachine::default();
        let ccfg = calls::Config::default();
        let belt = calls::Belt {
            running: true,
            paused: false,
            program: false,
            target_kmh: 4.0,
            min_kmh: 0.6,
        };
        let on = calls::Input {
            mic_in_use: true,
            belt,
        };
        let mut t = T0;
        let mut quieted = None;
        while t <= T0 + 10_000 {
            if let Some(a) = call.tick(&ccfg, t, &on) {
                quieted = Some(a);
            }
            t += 1000;
        }
        assert!(matches!(quieted, Some(calls::Action::Quiet { .. })));
        let mut restarted = calls::CallMachine::default();
        restarted.restore(call.kept());
        assert!(restarted.quiet_active());
        let quiet = calls::Belt {
            target_kmh: ccfg.quiet_kmh,
            ..belt
        };
        let off = calls::Input {
            mic_in_use: false,
            belt: quiet,
        };
        let mut restored = None;
        let mut t = T0 + 60_000;
        while t <= T0 + 80_000 {
            if let Some(a) = restarted.tick(&ccfg, t, &off) {
                restored = Some(a);
            }
            t += 1000;
        }
        assert_eq!(restored, Some(calls::Action::Restore { to_kmh: 4.0 }));
    }

    /// A call that started with the belt still left it alone; after a
    /// restart it still does, so Engage during the call keeps its speed.
    #[test]
    fn a_restored_call_that_left_the_belt_alone_still_does() {
        let mut m = calls::CallMachine::default();
        m.restore(calls::Kept {
            call: true,
            ..Default::default()
        });
        let input = calls::Input {
            mic_in_use: true,
            belt: calls::Belt {
                running: true,
                paused: false,
                program: false,
                target_kmh: 4.0,
                min_kmh: 0.6,
            },
        };
        let cfg = calls::Config::default();
        for s in 0..30 {
            assert_eq!(m.tick(&cfg, T0 + s * 1000, &input), None);
        }
    }

    /// Snoozes lengthen (15, 30, 60 minutes) until the seated time resets; a
    /// restart in between keeps the count, so the next snooze is as long.
    #[test]
    fn a_restart_keeps_how_many_snoozes_there_were() {
        let cfg = seated::Config::default();
        let tick = seated::Tick {
            idle_ms: Some(0),
            belt_running: false,
            ready: true,
        };
        let mut m = seated::Seated::default();
        let mut t = 0;
        while t <= 31 * MIN {
            m.tick(&cfg, T0 + t, &tick);
            t += 1000;
        }
        assert!(m.show_card());
        m.snooze(T0 + t);
        m.snooze(T0 + t);
        assert_eq!(m.snooze_minutes(), 60);
        let mut after = seated::Seated::default();
        after.restore(m.kept());
        assert_eq!(
            after.snooze_minutes(),
            60,
            "the third snooze is still an hour"
        );
        assert_eq!(after.seated_minutes(), m.seated_minutes());
    }

    /// A break due mid-walk waits for the walk's end, at most 10 minutes
    /// past its time. A restart while it waits keeps it waiting through the
    /// reconnect (no break begins on a belt the app can't see yet) and keeps
    /// its lateness, so it still pauses the walk on time, not 10 minutes on.
    #[test]
    fn a_break_waiting_for_the_walk_keeps_waiting_through_a_restart() {
        let cfg = eyebreak::Config {
            enabled: true,
            ..eyebreak::Config::default()
        };
        let tick = |belt| eyebreak::Tick {
            idle_ms: Some(0),
            locked: false,
            display_off: false,
            held: false,
            belt,
            rest_in_ms: None,
            rest_len_ms: None,
            mic_in_use: false,
        };
        let walking = eyebreak::Belt::Walking(eyebreak::Hold::Manual);
        let mut m = eyebreak::EyeBreak::default();
        let mut t = 0;
        while t <= 25 * MIN {
            assert_eq!(m.tick(&cfg, T0 + t, &tick(walking)), None);
            t += 1000;
        }
        assert_eq!(m.phase(), eyebreak::EyeBreakPhase::Waiting);

        // The app restarts; the treadmill takes 10 s to come back.
        let mut after = eyebreak::EyeBreak::default();
        after.restore(m.kept());
        let back = 25 * MIN + 30_000;
        for s in 0..10 {
            let now = T0 + back + s * 1000;
            assert_eq!(after.tick(&cfg, now, &tick(eyebreak::Belt::Other)), None);
            assert_eq!(after.phase(), eyebreak::EyeBreakPhase::Waiting, "{s} s");
        }
        // Walking again: the wait runs out about 10 minutes past the break's
        // time (the seconds the app was closed are not counted), not 10
        // minutes after the reconnect.
        let mut paused_at = None;
        let mut t = back + 10_000;
        while t <= 32 * MIN && paused_at.is_none() {
            if after.tick(&cfg, T0 + t, &tick(walking)).is_some() {
                paused_at = Some(t);
            }
            t += 1000;
        }
        let paused_at = paused_at.expect("the walk is paused for the break");
        assert!(
            (30 * MIN..=31 * MIN).contains(&paused_at),
            "paused at {} s",
            paused_at / 1000
        );
    }
}
