//! Eye breaks.
//!
//! Optional (off by default). After a set time of screen use, the HUD says a
//! break is a minute away, then runs a break: a card with a countdown, a tone
//! every 10 seconds to blink, and a tone at the end. The defaults follow the
//! research in the roadmap issue: a break of 90 seconds, because the focusing
//! system needs about that long to relax (Boulet 2016), every 20 minutes.
//! Longer breaks (5 minutes an hour, Pucker & Gawne 2023) are one setting away.
//!
//! The belt: with "Pause the belt" on (the default once eye breaks are on), a
//! break that starts while the belt is walking pauses it, the same pause as the
//! HUD's (a running program pauses at its step, it doesn't end). The machine
//! remembers that the pause is the break's own, and gives it up as soon as the
//! user does something with the belt (GO, Engage, a start at the treadmill, a
//! stop, END). When the break ends, or is skipped, and its pause still holds,
//! the HUD shows `OCULAR_IMPLANT // RECALIBRATED` with RESUME (a press) and
//! STAY STILL. Nothing here ever resumes the belt. A belt that was still when
//! the break began is left alone. Fitting breaks into program rests is still to
//! come.
//!
//! "Screen use" is keyboard or mouse input in the last 5 minutes, with the
//! session unlocked. That stands in for the screen being on, which Windows does
//! not tell us cheaply. Being away (locked, or no input for 15 minutes) starts
//! the count again, and so does no input for as long as a break lasts: the eyes
//! were off the screen anyway.
//!
//! The state machine is pure: it takes epoch-millisecond times as arguments, so
//! the tests drive it with a fake clock. `desk.rs` runs it once a second. The
//! tones are played by the HUD, which watches `blink_seq` and `end_seq` change.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

const KEY_ENABLED: &str = "eye_break_enabled";
const KEY_MINUTES: &str = "eye_break_minutes";
const KEY_SECONDS: &str = "eye_break_seconds";
const KEY_SOUND: &str = "eye_break_sound";
const KEY_PAUSE_BELT: &str = "eye_break_pause_belt";

const DEFAULT_MINUTES: u32 = 20;
const MIN_MINUTES: u32 = 10;
const MAX_MINUTES: u32 = 120;
const DEFAULT_SECONDS: u32 = 90;
const MIN_SECONDS: u32 = 20;
const MAX_SECONDS: u32 = 300;

const SECOND: u64 = 1000;
const MINUTE: u64 = 60 * SECOND;
/// Input within this long counts as using the screen.
const ACTIVE_WINDOW_MS: u64 = 5 * MINUTE;
/// No input for longer than this means the user left.
const AWAY_MS: u64 = 15 * MINUTE;
/// The heads-up comes this long before the break.
const WARN_MS: u64 = MINUTE;
/// A blink tone this often during a break.
const BLINK_EVERY_MS: u64 = 10 * SECOND;
/// A longer gap between two ticks is a stall or sleep, not time passing.
const MAX_STEP_MS: u64 = 10 * SECOND;
/// The deck takes a moment to report a pause. If the belt still walks this long
/// after the pause was sent, it is sent again.
const REISSUE_MS: u64 = 5 * SECOND;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub minutes: u32,
    pub seconds: u32,
    pub sound: bool,
    /// A break that starts while the belt walks pauses it.
    pub pause_belt: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            minutes: DEFAULT_MINUTES,
            seconds: DEFAULT_SECONDS,
            sound: true,
            pause_belt: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum EyeBreakPhase {
    /// Counting screen time, nothing to show.
    #[default]
    Idle,
    /// A break is about a minute away.
    Upcoming,
    /// Look away, blink.
    Break,
    /// The break is over and the belt it paused is still paused: RESUME or
    /// STAY STILL. Only in the snapshot; the machine itself is counting again.
    Complete,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct EyeBreakSnapshot {
    pub enabled: bool,
    /// Minutes of screen use between breaks.
    pub minutes: u32,
    /// How long a break lasts.
    pub seconds: u32,
    /// The HUD plays tones.
    pub sound: bool,
    /// A break that starts while the belt walks pauses it.
    pub pause_belt: bool,
    /// The belt is paused by the break (during the break, or on the resume card).
    pub holds_belt: bool,
    pub phase: EyeBreakPhase,
    /// Seconds until the break (upcoming) or until it ends (break).
    pub remaining_s: u32,
    /// Counts up at each blink prompt; the HUD plays a tone when it changes.
    pub blink_seq: u32,
    /// Counts up when a break runs its full length; the HUD plays the end tone.
    pub end_seq: u32,
}

/// What the desk loop tells the machine each second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Time since the last keyboard or mouse input; `None` if none was seen yet.
    pub idle_ms: Option<u64>,
    pub locked: bool,
    /// The belt, so a break can pause it and know the pause is still its own.
    pub belt: Belt,
}

/// Which pause: the treadmill's own (a manual walk), or a program's (by id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    Manual,
    Program(u32),
}

/// The belt as the desk loop sees it each second.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Belt {
    /// Anything else: still or stopped, offline, starting, stopping, resting,
    /// or waiting for GO. Nothing to pause, and no pause of the break's.
    #[default]
    Other,
    /// Walking, manual or a program's walking step not paused.
    Walking(Hold),
    /// Paused this way: the treadmill reports its pause with no program, or
    /// this program is paused at a walking step.
    Paused(Hold),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Pause the belt: the program's pause, or the treadmill's.
    Pause(Hold),
}

/// A pause the break made and still owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Owned {
    hold: Hold,
    /// When the pause was (last) sent.
    sent_ms: u64,
    /// The belt has been seen paused since.
    took: bool,
}

#[derive(Debug, Default)]
pub struct EyeBreak {
    active_ms: u64,
    break_ms: u64,
    phase: EyeBreakPhase,
    last_tick_ms: Option<u64>,
    blink_seq: u32,
    end_seq: u32,
    /// The belt is paused by a break. Outlives the break: while it holds after
    /// one, the HUD shows the resume card.
    owned: Option<Owned>,
}

impl EyeBreak {
    /// The count starts again. A pause the break owns is kept: the belt is
    /// still paused, and the resume card still applies.
    fn reset(&mut self) {
        self.active_ms = 0;
        self.break_ms = 0;
        self.phase = EyeBreakPhase::Idle;
    }

    /// Check the break's pause against the belt. Gives it up when the belt is
    /// anything but paused that way (the user resumed, stopped, ended the
    /// program, the link went), and sends the pause again if it hasn't taken.
    fn follow(&mut self, now_ms: u64, belt: Belt) -> Option<Effect> {
        let own = self.owned.as_mut()?;
        match belt {
            Belt::Paused(h) if h == own.hold => {
                own.took = true;
                None
            }
            Belt::Walking(h) if h == own.hold && !own.took => {
                if now_ms.saturating_sub(own.sent_ms) < REISSUE_MS {
                    return None;
                }
                log::info!("Eye break: the belt is still moving; sending the pause again");
                own.sent_ms = now_ms;
                Some(Effect::Pause(h))
            }
            _ => {
                log::info!("Eye break: the belt was moved by hand; the break no longer holds it");
                self.owned = None;
                None
            }
        }
    }

    /// Advance to `now_ms`. Returns a pause to apply, if any. Never resumes.
    pub fn tick(&mut self, cfg: &Config, now_ms: u64, input: &Tick) -> Option<Effect> {
        let step = self
            .last_tick_ms
            .map_or(0, |prev| now_ms.saturating_sub(prev));
        self.last_tick_ms = Some(now_ms);
        let dt = if step > MAX_STEP_MS { 0 } else { step };
        if !cfg.enabled {
            self.reset();
            self.owned = None;
            return None;
        }
        let effect = self.follow(now_ms, input.belt);
        let away = input.locked || step >= AWAY_MS || input.idle_ms.is_some_and(|i| i > AWAY_MS);
        if away {
            self.reset();
            return effect;
        }
        let break_ms = cfg.seconds as u64 * SECOND;

        if self.phase == EyeBreakPhase::Break {
            let before = self.break_ms;
            self.break_ms += dt;
            if self.break_ms >= break_ms {
                self.reset();
                self.end_seq = self.end_seq.wrapping_add(1);
            } else if self.break_ms / BLINK_EVERY_MS > before / BLINK_EVERY_MS {
                self.blink_seq = self.blink_seq.wrapping_add(1);
            }
            return effect;
        }

        match input.idle_ms {
            // Off the screen for a whole break already: that was one.
            Some(i) if i >= break_ms => {
                self.reset();
                return effect;
            }
            Some(i) if i <= ACTIVE_WINDOW_MS => self.active_ms += dt,
            _ => {}
        }

        let interval = cfg.minutes as u64 * MINUTE;
        if self.active_ms >= interval {
            self.phase = EyeBreakPhase::Break;
            self.break_ms = 0;
            self.blink_seq = self.blink_seq.wrapping_add(1);
            if let (true, None, Belt::Walking(hold)) = (cfg.pause_belt, self.owned, input.belt) {
                log::info!("Eye break: pausing the belt for the break");
                self.owned = Some(Owned {
                    hold,
                    sent_ms: now_ms,
                    took: false,
                });
                return Some(Effect::Pause(hold));
            }
        } else if self.active_ms + WARN_MS >= interval {
            self.phase = EyeBreakPhase::Upcoming;
        } else {
            self.phase = EyeBreakPhase::Idle;
        }
        effect
    }

    /// SKIP on the card: no tone, and the count starts again. A belt the break
    /// paused stays paused, and the resume card shows.
    pub fn skip(&mut self) {
        self.reset();
    }

    /// RESUME (once the caller has started the belt) or STAY STILL on the
    /// resume card: the break lets go of the pause and the card goes. Returns
    /// whether it held one.
    pub fn release(&mut self) -> bool {
        self.owned.take().is_some()
    }

    /// The belt is paused by a break.
    pub fn holds_belt(&self) -> bool {
        self.owned.is_some()
    }

    pub fn phase(&self) -> EyeBreakPhase {
        self.phase
    }

    /// How many breaks ran their full length.
    #[cfg(test)]
    pub fn end_seq(&self) -> u32 {
        self.end_seq
    }

    /// How many blink prompts (and the start of each break) there were.
    #[cfg(test)]
    pub fn blink_seq(&self) -> u32 {
        self.blink_seq
    }

    /// The phase the HUD shows: after a break, `Complete` while its pause holds.
    pub fn shown_phase(&self) -> EyeBreakPhase {
        match self.phase {
            EyeBreakPhase::Idle if self.owned.is_some() => EyeBreakPhase::Complete,
            p => p,
        }
    }

    fn remaining_s(&self, cfg: &Config) -> u32 {
        let ms = match self.phase {
            EyeBreakPhase::Idle | EyeBreakPhase::Complete => return 0,
            EyeBreakPhase::Upcoming => (cfg.minutes as u64 * MINUTE).saturating_sub(self.active_ms),
            EyeBreakPhase::Break => (cfg.seconds as u64 * SECOND).saturating_sub(self.break_ms),
        };
        ms.div_ceil(SECOND) as u32
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Build the config from raw stored values. Bad values fall back to the
/// defaults (off, 20 minutes, 90 seconds, sound on, pause the belt).
pub fn config_from_raw(
    enabled: Option<&str>,
    minutes: Option<&str>,
    seconds: Option<&str>,
    sound: Option<&str>,
    pause_belt: Option<&str>,
) -> Config {
    Config {
        enabled: persistence::parse_setting(enabled).unwrap_or(false),
        minutes: persistence::setting_in(minutes, MIN_MINUTES..=MAX_MINUTES, DEFAULT_MINUTES),
        seconds: persistence::setting_in(seconds, MIN_SECONDS..=MAX_SECONDS, DEFAULT_SECONDS),
        sound: persistence::parse_setting(sound).unwrap_or(true),
        pause_belt: persistence::parse_setting(pause_belt).unwrap_or(true),
    }
}

fn validate(minutes: u32, seconds: u32) -> Result<(), String> {
    if !(MIN_MINUTES..=MAX_MINUTES).contains(&minutes) {
        return Err(format!(
            "Time between breaks must be between {} and {} minutes",
            MIN_MINUTES, MAX_MINUTES
        ));
    }
    if !(MIN_SECONDS..=MAX_SECONDS).contains(&seconds) {
        return Err(format!(
            "A break must last between {} and {} seconds",
            MIN_SECONDS, MAX_SECONDS
        ));
    }
    Ok(())
}

#[derive(Default)]
pub struct Runtime {
    pub cfg: Config,
    pub machine: EyeBreak,
}

static STATE: Mutex<Option<Runtime>> = Mutex::new(None);

/// Run `f` on the shared config and machine.
pub fn with<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Runtime::default))
}

/// Load the settings. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let read = |k: &str| persistence::read_setting(app, k);
    let cfg = config_from_raw(
        read(KEY_ENABLED).as_deref(),
        read(KEY_MINUTES).as_deref(),
        read(KEY_SECONDS).as_deref(),
        read(KEY_SOUND).as_deref(),
        read(KEY_PAUSE_BELT).as_deref(),
    );
    with(|rt| rt.cfg = cfg);
}

/// Blocking (SQLite).
pub fn set_config(app: &AppHandle, cfg: Config) -> Result<(), String> {
    let Config {
        enabled,
        minutes,
        seconds,
        sound,
        pause_belt,
    } = cfg;
    validate(minutes, seconds)?;
    persistence::write_setting(app, KEY_ENABLED, &enabled.to_string())?;
    persistence::write_setting(app, KEY_MINUTES, &minutes.to_string())?;
    persistence::write_setting(app, KEY_SECONDS, &seconds.to_string())?;
    persistence::write_setting(app, KEY_SOUND, &sound.to_string())?;
    persistence::write_setting(app, KEY_PAUSE_BELT, &pause_belt.to_string())?;
    with(|rt| {
        rt.cfg = cfg;
        if !enabled {
            // Off: no card at all. A paused belt stays paused for the HUD's GO.
            rt.machine.reset();
            rt.machine.owned = None;
        }
    });
    Ok(())
}

/// For the snapshot. No I/O.
pub fn snapshot() -> EyeBreakSnapshot {
    with(|rt| EyeBreakSnapshot {
        enabled: rt.cfg.enabled,
        minutes: rt.cfg.minutes,
        seconds: rt.cfg.seconds,
        sound: rt.cfg.sound,
        pause_belt: rt.cfg.pause_belt,
        holds_belt: rt.cfg.enabled && rt.machine.holds_belt(),
        phase: if rt.cfg.enabled {
            rt.machine.shown_phase()
        } else {
            EyeBreakPhase::Idle
        },
        remaining_s: rt.machine.remaining_s(&rt.cfg),
        blink_seq: rt.machine.blink_seq,
        end_seq: rt.machine.end_seq,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;

    fn cfg() -> Config {
        Config {
            enabled: true,
            minutes: 20,
            seconds: 90,
            sound: true,
            pause_belt: true,
        }
    }

    fn typing() -> Tick {
        Tick {
            idle_ms: Some(0),
            locked: false,
            belt: Belt::Other,
        }
    }

    /// One tick a second from `from` to `to` (seconds after T0). Returns the
    /// effects, with the second each came at.
    fn run(m: &mut EyeBreak, c: &Config, from: u64, to: u64, input: Tick) -> Vec<(u64, Effect)> {
        let mut out = vec![];
        for s in from..=to {
            if let Some(e) = m.tick(c, T0 + s * SECOND, &input) {
                out.push((s, e));
            }
        }
        out
    }

    fn belt(belt: Belt) -> Tick {
        Tick { belt, ..typing() }
    }

    /// Looking away (no input for a while) with the belt in `b`.
    fn away_with(b: Belt) -> Tick {
        Tick {
            idle_ms: Some(30 * SECOND),
            locked: false,
            belt: b,
        }
    }

    const WALK: Belt = Belt::Walking(Hold::Manual);
    const PAUSED: Belt = Belt::Paused(Hold::Manual);

    /// Walking until the break starts at 1200 s; returns the effects.
    fn walk_into_a_break(m: &mut EyeBreak, c: &Config) -> Vec<(u64, Effect)> {
        let fx = run(m, c, 0, 1200, belt(WALK));
        assert_eq!(m.phase(), EyeBreakPhase::Break);
        fx
    }

    #[test]
    fn a_break_pauses_a_walking_belt_once_and_shows_the_resume_card_after() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        let fx = walk_into_a_break(&mut m, &c);
        assert_eq!(fx, vec![(1200, Effect::Pause(Hold::Manual))]);
        assert!(m.holds_belt());
        // The deck takes a second to report the pause.
        assert_eq!(run(&mut m, &c, 1201, 1202, away_with(WALK)), vec![]);
        assert_eq!(run(&mut m, &c, 1203, 1289, away_with(PAUSED)), vec![]);
        assert_eq!(m.shown_phase(), EyeBreakPhase::Break);
        // The break runs out: the end tone, and the resume card.
        assert_eq!(run(&mut m, &c, 1290, 1291, away_with(PAUSED)), vec![]);
        assert_eq!(m.end_seq, 1);
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(m.shown_phase(), EyeBreakPhase::Complete);
        // It waits, sending nothing, for as long as the belt stays paused.
        assert_eq!(run(&mut m, &c, 1292, 1800, belt(PAUSED)), vec![]);
        assert_eq!(m.shown_phase(), EyeBreakPhase::Complete);
        // RESUME or STAY STILL lets go.
        assert!(m.release());
        assert_eq!(m.shown_phase(), EyeBreakPhase::Idle);
        assert!(!m.release());
    }

    #[test]
    fn a_program_is_paused_with_its_own_pause() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        let fx = run(&mut m, &c, 0, 1200, belt(Belt::Walking(Hold::Program(2))));
        assert_eq!(fx, vec![(1200, Effect::Pause(Hold::Program(2)))]);
        run(
            &mut m,
            &c,
            1201,
            1300,
            away_with(Belt::Paused(Hold::Program(2))),
        );
        assert_eq!(m.shown_phase(), EyeBreakPhase::Complete);
    }

    #[test]
    fn a_still_belt_is_left_alone() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        let fx = run(&mut m, &c, 0, 1200, belt(Belt::Other));
        assert_eq!(fx, vec![]);
        assert!(!m.holds_belt());
        // Paused by the user before the break: not the break's pause either.
        let (mut m, c) = (EyeBreak::default(), cfg());
        assert_eq!(run(&mut m, &c, 0, 1300, belt(PAUSED)), vec![]);
        assert_eq!(m.shown_phase(), EyeBreakPhase::Idle);
    }

    /// Jeremy, 2026-11-19: eye breaks show whenever he uses the PC, with the
    /// treadmill off or unlinked, the belt still and no program (all
    /// `Belt::Other` to the machine). The heads-up, the break with its blink
    /// tones and the end tone all come; nothing is sent to the belt.
    #[test]
    fn with_nothing_running_the_whole_break_still_comes() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        let mut fx = run(&mut m, &c, 0, 1141, belt(Belt::Other));
        assert_eq!(m.shown_phase(), EyeBreakPhase::Upcoming);
        assert!(m.remaining_s(&c) > 0);
        fx.extend(run(&mut m, &c, 1142, 1200, belt(Belt::Other)));
        assert_eq!(m.shown_phase(), EyeBreakPhase::Break);
        assert_eq!(m.blink_seq, 1, "the tone at the start of the break");
        fx.extend(run(&mut m, &c, 1201, 1245, away_with(Belt::Other)));
        assert_eq!(m.blink_seq, 5, "a blink tone every ten seconds");
        fx.extend(run(&mut m, &c, 1246, 1295, away_with(Belt::Other)));
        assert_eq!(m.end_seq, 1, "the end tone");
        assert_eq!(fx, vec![], "nothing for the belt");
        assert!(!m.holds_belt());
        // No resume card: the break paused nothing.
        assert_eq!(m.shown_phase(), EyeBreakPhase::Idle);
    }

    #[test]
    fn with_pause_belt_off_the_break_never_touches_the_belt() {
        let mut m = EyeBreak::default();
        let c = Config {
            pause_belt: false,
            ..cfg()
        };
        assert_eq!(run(&mut m, &c, 0, 1300, belt(WALK)), vec![]);
        assert!(!m.holds_belt());
        assert_eq!(m.shown_phase(), EyeBreakPhase::Idle);
    }

    #[test]
    fn a_pause_that_does_not_take_is_sent_again() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        walk_into_a_break(&mut m, &c);
        let fx = run(&mut m, &c, 1201, 1212, away_with(WALK));
        assert_eq!(
            fx,
            vec![
                (1205, Effect::Pause(Hold::Manual)),
                (1210, Effect::Pause(Hold::Manual))
            ]
        );
        assert!(m.holds_belt());
    }

    #[test]
    fn resuming_by_hand_during_the_break_gives_the_pause_up() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        walk_into_a_break(&mut m, &c);
        run(&mut m, &c, 1201, 1210, away_with(PAUSED));
        // Engage, GO or a start at the treadmill: the countdown, then walking.
        run(&mut m, &c, 1211, 1213, away_with(Belt::Other));
        assert!(!m.holds_belt());
        assert_eq!(run(&mut m, &c, 1214, 1300, away_with(WALK)), vec![]);
        // The break still ends with its tone, but no resume card.
        assert_eq!(m.end_seq, 1);
        assert_eq!(m.shown_phase(), EyeBreakPhase::Idle);
    }

    #[test]
    fn walking_again_straight_from_the_pause_gives_it_up_too() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        walk_into_a_break(&mut m, &c);
        run(&mut m, &c, 1201, 1210, away_with(PAUSED));
        assert_eq!(run(&mut m, &c, 1211, 1220, away_with(WALK)), vec![]);
        assert!(!m.holds_belt());
    }

    #[test]
    fn a_stop_an_end_or_the_link_going_gives_the_pause_up() {
        for after in [Belt::Other, Belt::Paused(Hold::Program(1))] {
            let (mut m, c) = (EyeBreak::default(), cfg());
            walk_into_a_break(&mut m, &c);
            run(&mut m, &c, 1201, 1210, away_with(PAUSED));
            run(&mut m, &c, 1211, 1300, away_with(after));
            assert!(!m.holds_belt(), "{:?}", after);
            assert_eq!(m.shown_phase(), EyeBreakPhase::Idle);
        }
        // A program ended with END leaves a manual pause: not the program's.
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 1200, belt(Belt::Walking(Hold::Program(2))));
        run(
            &mut m,
            &c,
            1201,
            1210,
            away_with(Belt::Paused(Hold::Program(2))),
        );
        run(&mut m, &c, 1211, 1212, away_with(PAUSED));
        assert!(!m.holds_belt());
    }

    #[test]
    fn skip_shows_the_resume_card_at_once_without_a_tone() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        walk_into_a_break(&mut m, &c);
        run(&mut m, &c, 1201, 1205, belt(PAUSED));
        m.skip();
        assert_eq!(m.shown_phase(), EyeBreakPhase::Complete);
        assert_eq!(m.end_seq, 0);
        assert_eq!(run(&mut m, &c, 1206, 1400, belt(PAUSED)), vec![]);
        assert_eq!(m.shown_phase(), EyeBreakPhase::Complete);
    }

    #[test]
    fn locking_the_pc_keeps_the_pause_and_its_card() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        walk_into_a_break(&mut m, &c);
        let locked = Tick {
            locked: true,
            ..away_with(PAUSED)
        };
        run(&mut m, &c, 1201, 1260, locked);
        assert_eq!(m.shown_phase(), EyeBreakPhase::Complete);
    }

    #[test]
    fn turning_eye_breaks_off_drops_the_card_but_never_resumes() {
        let (mut m, mut c) = (EyeBreak::default(), cfg());
        walk_into_a_break(&mut m, &c);
        c.enabled = false;
        assert_eq!(run(&mut m, &c, 1201, 1300, belt(PAUSED)), vec![]);
        assert!(!m.holds_belt());
    }

    #[test]
    fn a_belt_left_paused_after_the_card_gets_no_new_pause_from_the_next_break() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        walk_into_a_break(&mut m, &c);
        // The card is ignored through the next whole interval and break.
        let fx = run(&mut m, &c, 1201, 2700, belt(PAUSED));
        assert_eq!(fx, vec![]);
        assert!(m.holds_belt());
    }

    #[test]
    fn warns_a_minute_ahead_then_breaks() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 1138, typing());
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        run(&mut m, &c, 1139, 1141, typing());
        assert_eq!(m.phase(), EyeBreakPhase::Upcoming);
        assert!(m.remaining_s(&c) <= 60);
        run(&mut m, &c, 1142, 1200, typing());
        assert_eq!(m.phase(), EyeBreakPhase::Break);
        assert_eq!(m.blink_seq, 1);
    }

    #[test]
    fn a_break_blinks_every_ten_seconds_then_ends_with_a_tone() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 1200, typing());
        assert_eq!(m.phase(), EyeBreakPhase::Break);
        let start = m.blink_seq;
        // Looking away: no input during the break.
        let away = Tick {
            idle_ms: Some(30 * SECOND),
            locked: false,
            belt: Belt::Other,
        };
        run(&mut m, &c, 1201, 1245, away);
        assert_eq!(m.blink_seq, start + 4);
        assert_eq!(m.end_seq, 0);
        run(&mut m, &c, 1246, 1295, away);
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(m.end_seq, 1);
        assert_eq!(m.remaining_s(&c), 0);
    }

    #[test]
    fn skip_restarts_the_count_without_a_tone() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 1200, typing());
        m.skip();
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(m.end_seq, 0);
        run(&mut m, &c, 1201, 1300, typing());
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
    }

    #[test]
    fn being_away_or_locked_starts_the_count_again() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 1150, typing());
        assert_eq!(m.phase(), EyeBreakPhase::Upcoming);
        let locked = Tick {
            idle_ms: Some(0),
            locked: true,
            belt: Belt::Other,
        };
        m.tick(&c, T0 + 1151 * SECOND, &locked);
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(m.active_ms, 0);
    }

    #[test]
    fn a_pause_in_input_as_long_as_a_break_counts_as_one() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 600, typing());
        let paused = Tick {
            idle_ms: Some(91 * SECOND),
            locked: false,
            belt: Belt::Other,
        };
        m.tick(&c, T0 + 601 * SECOND, &paused);
        assert_eq!(m.active_ms, 0);
    }

    #[test]
    fn a_stall_counts_for_nothing() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 10, typing());
        let before = m.active_ms;
        m.tick(&c, T0 + 70 * SECOND, &typing());
        assert_eq!(m.active_ms, before);
    }

    #[test]
    fn off_means_nothing_happens() {
        let mut m = EyeBreak::default();
        let c = Config {
            enabled: false,
            ..cfg()
        };
        run(&mut m, &c, 0, 1300, typing());
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(m.blink_seq, 0);
    }

    #[test]
    fn bad_settings_fall_back_to_defaults() {
        let c = config_from_raw(Some("garbage"), Some("1"), Some("9999"), None, Some("x"));
        assert_eq!(c, Config::default());
        let c = config_from_raw(
            Some("true"),
            Some("30"),
            Some("300"),
            Some("false"),
            Some("false"),
        );
        assert_eq!(
            c,
            Config {
                enabled: true,
                minutes: 30,
                seconds: 300,
                sound: false,
                pause_belt: false,
            }
        );
        assert!(validate(5, 90).is_err());
        assert!(validate(20, 10).is_err());
        assert!(validate(20, 90).is_ok());
    }
}
