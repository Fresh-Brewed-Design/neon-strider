//! Eye breaks.
//!
//! Optional (off by default). After a set time of screen use, the HUD says a
//! break is a minute away, then runs a break: a card with a countdown, guided
//! full blinks, and a tone at the end. The defaults follow `docs/research.md`:
//! a break of 90 seconds, because the focusing system needs about that long to
//! relax (Boulet 2016), every 20 minutes. Longer breaks (5 minutes an hour,
//! Pucker & Gawne 2023) are one setting away.
//!
//! Guided blinks: screen reading makes more blinks incomplete (Chu et al. 2014,
//! Argilés et al. 2015), and blinking exercises cut incomplete blinks. Through
//! the break the card repeats the exercise from Wolffsohn et al. (2025): close
//! the eyes and hold, squeeze gently, open, each step 2 seconds. At the default
//! 90 seconds that is 15 full blinks, the trial's best routine. Each step is a
//! cue (`blink` and `blink_seq` in the snapshot): text on the card, and a tone
//! when Tones is on.
//!
//! The belt: with "Pause the belt" on (the default once eye breaks are on), a
//! break that starts while the belt is walking pauses it, the same pause as the
//! HUD's (a running program pauses at its step, it doesn't end). The machine
//! remembers that the pause is the break's own, and gives it up as soon as the
//! user does something with the belt (GO, Engage, a start at the treadmill, a
//! stop, END). When the break ends, or is skipped, and its pause still holds,
//! the HUD shows `OCULAR_IMPLANT // RECALIBRATED` with RESUME (a press) and
//! STAY STILL. Nothing here ever resumes the belt. A belt that was still when
//! the break began is left alone.
//!
//! Program rests: with "Take breaks at program rests" on (the default), a
//! break fits into the moments a running program holds the belt still anyway
//! (the rest between cycles, a 0 km/h step, or a wait for GO), so it doesn't
//! interrupt a walk. At such a rest a break due within half an interval starts
//! now. While a program walks, a break that comes due waits for the program's
//! next rest if that rest is at most half an interval (and never more than 10
//! minutes) late; the heads-up says so. A rest out of reach, or a manual walk,
//! gets the pause as before. A still program never starts the belt by itself
//! (its next step waits for GO), so a break at a rest needs no pause of its own.
//!
//! "Screen use" is keyboard or mouse input in the last 5 minutes, with the
//! session unlocked and the display on (`platform::display_off`). Being away
//! (locked, the display off, or no input for 15 minutes) starts the count again, and so does no input for as long as a break lasts: the eyes
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
const KEY_FIT_RESTS: &str = "eye_break_fit_rests";

/// Defaults and ranges: docs/research.md, Eye breaks (20 min, 90 s; breaks up to
/// 300 s for the myopia data). DeskSection reads the ranges from `SettingLimits`.
const DEFAULT_MINUTES: u32 = 20;
pub(crate) const MIN_MINUTES: u32 = 10;
pub(crate) const MAX_MINUTES: u32 = 120;
const DEFAULT_SECONDS: u32 = 90;
pub(crate) const MIN_SECONDS: u32 = 20;
pub(crate) const MAX_SECONDS: u32 = 300;

const SECOND: u64 = 1000;
const MINUTE: u64 = 60 * SECOND;
/// Input within this long counts as using the screen.
/// Same as the idle nudge (`seated::ACTIVE_WINDOW_MS`): reading a page can go
/// minutes without a key press and is still screen use.
const ACTIVE_WINDOW_MS: u64 = 5 * MINUTE;
/// No input for longer than this means the user left.
/// Same as the idle nudge: a quarter of an hour away rests the eyes as well as a break.
const AWAY_MS: u64 = 15 * MINUTE;
/// The heads-up comes this long before the break.
/// A minute finishes a sentence or a call reply before the break starts.
const WARN_MS: u64 = MINUTE;
/// Each step of a guided blink (close and hold, squeeze, open) lasts this
/// long: the 2 s per step of the blinking-exercise trial (Wolffsohn et al. 2025).
const BLINK_STEP_MS: u64 = 2 * SECOND;
/// The steps of one full blink: close, squeeze, open.
const BLINK_STEPS: u64 = 3;
/// A step starts this much early, so it lands on the nearest one-second tick
/// and not a whole tick late when a tick runs a few milliseconds early.
const BLINK_SLACK_MS: u64 = SECOND / 2;
/// A longer gap between two ticks is a stall or sleep, not time passing.
/// Ten 1 s ticks: longer is a sleeping PC or a stalled runtime.
const MAX_STEP_MS: u64 = 10 * SECOND;
/// The deck takes a moment to report a pause. If the belt still walks this long
/// after the pause was sent, it is sent again.
/// About eight polls: the deck reports a pause within a frame or two.
const REISSUE_MS: u64 = 5 * SECOND;
/// A break never waits longer than this for a program's rest.
/// Half the default 20 minute interval: a break moved further than that is a skipped break.
const MAX_WAIT_FOR_REST_MS: u64 = 10 * MINUTE;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub minutes: u32,
    pub seconds: u32,
    pub sound: bool,
    /// A break that starts while the belt walks pauses it.
    pub pause_belt: bool,
    /// Breaks fit into a running program's rests.
    pub fit_rests: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            minutes: DEFAULT_MINUTES,
            seconds: DEFAULT_SECONDS,
            sound: true,
            pause_belt: true,
            fit_rests: true,
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
    /// Look away, and blink fully with the cues.
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
    /// Breaks fit into a running program's rests.
    pub fit_rests: bool,
    /// The belt is paused by the break (during the break, or on the resume card).
    pub holds_belt: bool,
    /// Upcoming: the break is set for the program's next rest, so the belt
    /// won't be paused for it.
    pub at_rest: bool,
    pub phase: EyeBreakPhase,
    /// Seconds until the break (upcoming) or until it ends (break).
    pub remaining_s: u32,
    /// During a break: the step of the guided blink to do now.
    pub blink: Option<BlinkStep>,
    /// Counts up at each blink cue; the HUD plays the step's tone when it changes.
    pub blink_seq: u32,
    /// Counts up when a break runs its full length; the HUD plays the end tone.
    pub end_seq: u32,
}

/// One step of a guided full blink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum BlinkStep {
    /// Close the eyes and hold them closed.
    Close,
    /// Squeeze the eyelids gently.
    Squeeze,
    /// Open the eyes and look far away.
    Open,
}

/// The step `break_ms` into a break.
fn blink_index(break_ms: u64) -> u64 {
    (break_ms + BLINK_SLACK_MS) / BLINK_STEP_MS
}

fn blink_step(break_ms: u64) -> BlinkStep {
    match blink_index(break_ms) % BLINK_STEPS {
        0 => BlinkStep::Close,
        1 => BlinkStep::Squeeze,
        _ => BlinkStep::Open,
    }
}

/// What the desk loop tells the machine each second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Time since the last keyboard or mouse input; `None` if none was seen yet.
    pub idle_ms: Option<u64>,
    pub locked: bool,
    /// The PC's display is off.
    pub display_off: bool,
    /// The HUD holds its non-urgent cards back (`presenting`, a presentation
    /// or a full-screen app): no heads-up or break begins, and the count stops
    /// a heads-up short of the break, so the break comes with its warning once
    /// the hold ends.
    pub held: bool,
    /// The belt, so a break can pause it and know the pause is still its own.
    pub belt: Belt,
    /// While a program walks: walking time until it next holds the belt still
    /// (`ProgramRun::next_still_s`). None when no such rest is coming.
    pub rest_in_ms: Option<u64>,
    /// How long that rest lasts or, at a rest (`Belt::Rest`), what is left of
    /// it. None: until a press (a wait for GO, an open-ended 0 km/h step).
    pub rest_len_ms: Option<u64>,
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
    /// A running program holds the belt still by itself: its rest between
    /// cycles, a 0 km/h step, or a wait for GO. A break fits here.
    Rest,
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
    /// Upcoming: milliseconds until the break (until the rest when it waits
    /// for one).
    next_ms: u64,
    /// The next break is set for the program's next rest.
    at_rest: bool,
}

impl EyeBreak {
    /// The count starts again. A pause the break owns is kept: the belt is
    /// still paused, and the resume card still applies.
    fn reset(&mut self) {
        self.active_ms = 0;
        self.break_ms = 0;
        self.phase = EyeBreakPhase::Idle;
        self.at_rest = false;
    }

    /// When the next break comes, and whether it is set for a program's rest
    /// (the next one, or the one the belt is in). A break waits for a rest at
    /// most half an interval (and `MAX_WAIT_FOR_REST_MS`) past its time. It
    /// comes early, by up to half an interval, to a rest that would be over
    /// before the break ended at its own time. A rest that holds it anyway
    /// leaves it on time.
    fn plan(&self, cfg: &Config, interval: u64, input: &Tick) -> (u64, bool) {
        let due_in = interval.saturating_sub(self.active_ms);
        if !cfg.fit_rests {
            return (due_in, false);
        }
        let rest_in = match (input.belt, input.rest_in_ms) {
            (Belt::Rest, _) => 0,
            (Belt::Walking(Hold::Program(_)), Some(r)) => r,
            _ => return (due_in, false),
        };
        // Moving a break by up to half an interval keeps the gaps between breaks within
        // 0.5 to 1.5 intervals.
        let half = interval / 2;
        let fits = if rest_in >= due_in {
            rest_in - due_in <= half.min(MAX_WAIT_FOR_REST_MS)
        } else {
            let ends_first = input
                .rest_len_ms
                .is_some_and(|len| due_in + cfg.seconds as u64 * SECOND > rest_in + len);
            due_in - rest_in <= half && ends_first
        };
        if fits {
            (rest_in, true)
        } else {
            (due_in, false)
        }
    }

    /// The break begins: the first blink cue, and the pause if the belt walks.
    fn begin(&mut self, cfg: &Config, now_ms: u64, belt: Belt) -> Option<Effect> {
        self.phase = EyeBreakPhase::Break;
        self.break_ms = 0;
        self.at_rest = false;
        self.blink_seq = self.blink_seq.wrapping_add(1);
        if let (true, None, Belt::Walking(hold)) = (cfg.pause_belt, self.owned, belt) {
            log::info!("Eye break: pausing the belt for the break");
            self.owned = Some(Owned {
                hold,
                sent_ms: now_ms,
                took: false,
            });
            return Some(Effect::Pause(hold));
        }
        None
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
        let away = input.locked
            || input.display_off
            || step >= AWAY_MS
            || input.idle_ms.is_some_and(|i| i > AWAY_MS);
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
            } else if blink_index(self.break_ms) > blink_index(before) {
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
        if input.held {
            // The heads-up's minute is kept in reserve while held: the break
            // never starts unannounced right after a presentation ends.
            self.active_ms = self.active_ms.min(interval.saturating_sub(WARN_MS));
        }
        let (next_ms, at_rest) = self.plan(cfg, interval, input);
        self.next_ms = next_ms;
        self.at_rest = at_rest;
        // Set for a rest: it begins once the belt is at it. Never while held:
        // its card would not show, and a pause with no card is a surprise.
        if next_ms == 0 && !input.held && (!at_rest || input.belt == Belt::Rest) {
            if at_rest {
                log::info!("Eye break: taking the break at the program's rest");
            }
            return self.begin(cfg, now_ms, input.belt).or(effect);
        }
        self.phase = if next_ms <= WARN_MS {
            EyeBreakPhase::Upcoming
        } else {
            EyeBreakPhase::Idle
        };
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

    /// During a break, the blink step to do now.
    pub fn blink(&self) -> Option<BlinkStep> {
        (self.phase == EyeBreakPhase::Break).then(|| blink_step(self.break_ms))
    }

    /// What a restart keeps (`desk_resume.rs`): the screen time counted
    /// towards the next break. A break under way is not kept as such; its
    /// count is already full, so it starts again after the restart.
    pub fn kept(&self) -> Kept {
        Kept {
            active_ms: self.active_ms,
        }
    }

    /// Put back what a restart kept.
    pub fn restore(&mut self, kept: Kept) {
        self.active_ms = kept.active_ms;
    }

    fn remaining_s(&self, cfg: &Config) -> u32 {
        let ms = match self.phase {
            EyeBreakPhase::Idle | EyeBreakPhase::Complete => return 0,
            EyeBreakPhase::Upcoming => self.next_ms,
            EyeBreakPhase::Break => (cfg.seconds as u64 * SECOND).saturating_sub(self.break_ms),
        };
        ms.div_ceil(SECOND) as u32
    }
}

/// The eye-break state a restart keeps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    pub active_ms: u64,
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
    fit_rests: Option<&str>,
) -> Config {
    Config {
        enabled: persistence::parse_setting(enabled).unwrap_or(false),
        minutes: persistence::setting_in(minutes, MIN_MINUTES..=MAX_MINUTES, DEFAULT_MINUTES),
        seconds: persistence::setting_in(seconds, MIN_SECONDS..=MAX_SECONDS, DEFAULT_SECONDS),
        sound: persistence::parse_setting(sound).unwrap_or(true),
        pause_belt: persistence::parse_setting(pause_belt).unwrap_or(true),
        fit_rests: persistence::parse_setting(fit_rests).unwrap_or(true),
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
        read(KEY_FIT_RESTS).as_deref(),
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
        fit_rests,
    } = cfg;
    validate(minutes, seconds)?;
    persistence::write_setting(app, KEY_ENABLED, &enabled.to_string())?;
    persistence::write_setting(app, KEY_MINUTES, &minutes.to_string())?;
    persistence::write_setting(app, KEY_SECONDS, &seconds.to_string())?;
    persistence::write_setting(app, KEY_SOUND, &sound.to_string())?;
    persistence::write_setting(app, KEY_PAUSE_BELT, &pause_belt.to_string())?;
    persistence::write_setting(app, KEY_FIT_RESTS, &fit_rests.to_string())?;
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
    with(|rt| snapshot_of(rt))
}

/// The snapshot of one runtime.
pub(crate) fn snapshot_of(rt: &Runtime) -> EyeBreakSnapshot {
    EyeBreakSnapshot {
        enabled: rt.cfg.enabled,
        minutes: rt.cfg.minutes,
        seconds: rt.cfg.seconds,
        sound: rt.cfg.sound,
        pause_belt: rt.cfg.pause_belt,
        fit_rests: rt.cfg.fit_rests,
        holds_belt: rt.cfg.enabled && rt.machine.holds_belt(),
        at_rest: rt.cfg.enabled
            && rt.machine.phase == EyeBreakPhase::Upcoming
            && rt.machine.at_rest,
        phase: if rt.cfg.enabled {
            rt.machine.shown_phase()
        } else {
            EyeBreakPhase::Idle
        },
        remaining_s: rt.machine.remaining_s(&rt.cfg),
        blink: if rt.cfg.enabled {
            rt.machine.blink()
        } else {
            None
        },
        blink_seq: rt.machine.blink_seq,
        end_seq: rt.machine.end_seq,
    }
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
            fit_rests: true,
        }
    }

    fn typing() -> Tick {
        Tick {
            idle_ms: Some(0),
            locked: false,
            display_off: false,
            held: false,
            belt: Belt::Other,
            rest_in_ms: None,
            rest_len_ms: None,
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
            display_off: false,
            held: false,
            belt: b,
            rest_in_ms: None,
            rest_len_ms: None,
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

    /// Fresh Brewed Design, 2026-11-19: eye breaks show whenever the PC is in use, with the
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
        assert_eq!(m.blink_seq, 23, "a guided blink cue every 2 seconds");
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

    fn away() -> Tick {
        Tick {
            idle_ms: Some(30 * SECOND),
            locked: false,
            display_off: false,
            held: false,
            belt: Belt::Other,
            rest_in_ms: None,
            rest_len_ms: None,
        }
    }

    #[test]
    fn a_break_guides_full_blinks_then_ends_with_a_tone() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 1200, typing());
        assert_eq!(m.phase(), EyeBreakPhase::Break);
        assert_eq!(m.blink(), Some(BlinkStep::Close));
        let start = m.blink_seq;
        // Close and hold for 2 s, squeeze for 2 s, open for 2 s.
        run(&mut m, &c, 1201, 1201, away());
        assert_eq!((m.blink(), m.blink_seq), (Some(BlinkStep::Close), start));
        run(&mut m, &c, 1202, 1203, away());
        assert_eq!(
            (m.blink(), m.blink_seq),
            (Some(BlinkStep::Squeeze), start + 1)
        );
        run(&mut m, &c, 1204, 1205, away());
        assert_eq!((m.blink(), m.blink_seq), (Some(BlinkStep::Open), start + 2));
        run(&mut m, &c, 1206, 1206, away());
        assert_eq!(
            (m.blink(), m.blink_seq),
            (Some(BlinkStep::Close), start + 3)
        );
        // 90 seconds hold 15 full blinks: 45 cues, the last one OPEN.
        run(&mut m, &c, 1207, 1289, away());
        assert_eq!(m.blink(), Some(BlinkStep::Open));
        assert_eq!(m.blink_seq, start + 44);
        assert_eq!(m.end_seq, 0);
        run(&mut m, &c, 1290, 1295, away());
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(m.blink(), None);
        assert_eq!(m.blink_seq, start + 44);
        assert_eq!(m.end_seq, 1);
        assert_eq!(m.remaining_s(&c), 0);
    }

    #[test]
    fn blink_steps_stay_two_ticks_long_when_ticks_run_early_or_late() {
        for tick_ms in [990, 999, 1003, 1010] {
            let (mut m, c) = (EyeBreak::default(), cfg());
            run(&mut m, &c, 0, 1200, typing());
            assert_eq!(m.phase(), EyeBreakPhase::Break);
            let t0 = T0 + 1200 * SECOND;
            let mut changes = vec![];
            let mut seq = m.blink_seq;
            for k in 1..=40u64 {
                m.tick(&c, t0 + k * tick_ms, &away());
                if m.blink_seq != seq {
                    seq = m.blink_seq;
                    changes.push(k);
                }
            }
            let gaps: Vec<u64> = changes.windows(2).map(|w| w[1] - w[0]).collect();
            assert!(gaps.iter().all(|&g| g == 2), "{tick_ms} ms ticks: {gaps:?}");
        }
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
            display_off: false,
            held: false,
            belt: Belt::Other,
            rest_in_ms: None,
            rest_len_ms: None,
        };
        m.tick(&c, T0 + 1151 * SECOND, &locked);
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(m.active_ms, 0);
    }

    #[test]
    fn no_break_begins_while_held_and_the_heads_up_comes_after() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        let walking = belt(Belt::Walking(Hold::Manual));
        let held = Tick {
            held: true,
            ..walking
        };
        // Presenting through the whole interval and well past it.
        let effects = run(&mut m, &c, 0, 1800, held);
        assert!(effects.is_empty(), "the belt is never paused while held");
        assert_ne!(m.phase(), EyeBreakPhase::Break);
        assert_eq!(m.blink_seq(), 0);
        // Once the hold ends the heads-up's minute is still ahead.
        run(&mut m, &c, 1801, 1802, walking);
        assert_eq!(m.phase(), EyeBreakPhase::Upcoming);
        let effects = run(&mut m, &c, 1803, 1870, walking);
        assert_eq!(m.phase(), EyeBreakPhase::Break);
        // The break's own pause, a whole heads-up after the hold ended.
        assert!(effects.first().is_some_and(|(s, _)| *s >= 1801 + 59));
    }

    #[test]
    fn a_pause_in_input_as_long_as_a_break_counts_as_one() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 600, typing());
        let paused = Tick {
            idle_ms: Some(91 * SECOND),
            locked: false,
            display_off: false,
            held: false,
            belt: Belt::Other,
            rest_in_ms: None,
            rest_len_ms: None,
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

    // -- Program rests ------------------------------------------------------

    const PROGRAM: Belt = Belt::Walking(Hold::Program(2));

    /// One tick a second, the input made from the second.
    fn run_by(
        m: &mut EyeBreak,
        c: &Config,
        from: u64,
        to: u64,
        input: impl Fn(u64) -> Tick,
    ) -> Vec<(u64, Effect)> {
        let mut out = vec![];
        for s in from..=to {
            if let Some(e) = m.tick(c, T0 + s * SECOND, &input(s)) {
                out.push((s, e));
            }
        }
        out
    }

    /// A program walking at the keyboard until its rest at `rest_at` seconds,
    /// then resting there for `len` seconds.
    fn program_resting_at(rest_at: u64, len: u64) -> impl Fn(u64) -> Tick {
        move |s| {
            if s < rest_at {
                Tick {
                    rest_in_ms: Some((rest_at - s) * SECOND),
                    rest_len_ms: Some(len * SECOND),
                    ..belt(PROGRAM)
                }
            } else {
                Tick {
                    rest_len_ms: Some((rest_at + len).saturating_sub(s) * SECOND),
                    ..belt(Belt::Rest)
                }
            }
        }
    }

    /// The Workday shape: the rest is 25 minutes.
    const LONG: u64 = 1500;
    /// A rest over before a break due 500 s after it starts could end.
    const SHORT: u64 = 300;

    #[test]
    fn a_break_due_while_a_program_walks_waits_for_its_rest_and_pauses_nothing() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        // Due at 1200 s; the rest comes 200 s later.
        let walk = program_resting_at(1400, LONG);
        assert_eq!(run_by(&mut m, &c, 0, 1339, &walk), vec![]);
        assert_eq!(
            m.phase(),
            EyeBreakPhase::Idle,
            "no heads-up at the due time"
        );
        run_by(&mut m, &c, 1340, 1399, &walk);
        assert_eq!(m.phase(), EyeBreakPhase::Upcoming);
        assert!(m.at_rest);
        assert_eq!(m.remaining_s(&c), 1);
        assert_eq!(run_by(&mut m, &c, 1400, 1400, &walk), vec![]);
        assert_eq!(m.phase(), EyeBreakPhase::Break);
        assert!(!m.holds_belt());
        // It runs its length at the rest, and no resume card follows.
        run_by(&mut m, &c, 1401, 1500, |_| away_with(Belt::Rest));
        assert_eq!(m.end_seq, 1);
        assert_eq!(m.shown_phase(), EyeBreakPhase::Idle);
    }

    #[test]
    fn a_rest_too_late_for_the_break_gets_the_pause_on_time() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        // Half an interval is 600 s; this rest would be 700 s late.
        let fx = run_by(&mut m, &c, 0, 1200, program_resting_at(1900, LONG));
        assert_eq!(fx, vec![(1200, Effect::Pause(Hold::Program(2)))]);
    }

    #[test]
    fn a_break_never_waits_more_than_ten_minutes_for_a_rest() {
        let c = Config {
            minutes: 60,
            ..cfg()
        };
        // Due at 3600 s; a rest 11 minutes later is out of reach.
        let mut m = EyeBreak::default();
        let fx = run_by(&mut m, &c, 0, 3600, program_resting_at(3600 + 660, LONG));
        assert_eq!(fx, vec![(3600, Effect::Pause(Hold::Program(2)))]);
        // Nine minutes is in reach.
        let mut m = EyeBreak::default();
        let fx = run_by(&mut m, &c, 0, 4200, program_resting_at(3600 + 540, LONG));
        assert_eq!(fx, vec![]);
        assert_eq!(m.phase(), EyeBreakPhase::Break);
    }

    #[test]
    fn a_short_rest_takes_a_break_due_within_half_an_interval_early() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        // At 700 s the break is 500 s away and the rest is over at 1000 s.
        let walk = program_resting_at(700, SHORT);
        run_by(&mut m, &c, 0, 645, &walk);
        assert_eq!(
            m.phase(),
            EyeBreakPhase::Upcoming,
            "the heads-up comes first"
        );
        assert!(m.at_rest);
        assert_eq!(run_by(&mut m, &c, 646, 700, &walk), vec![]);
        assert_eq!(m.phase(), EyeBreakPhase::Break);
        assert!(!m.holds_belt());
        run_by(&mut m, &c, 701, 795, |_| away_with(Belt::Rest));
        assert_eq!(m.end_seq, 1);
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
    }

    #[test]
    fn a_long_rest_leaves_the_break_on_time() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        // The rest from 700 s to 2200 s holds the break at 1200 s anyway.
        let walk = program_resting_at(700, LONG);
        run_by(&mut m, &c, 0, 1100, &walk);
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(run_by(&mut m, &c, 1101, 1200, &walk), vec![]);
        assert_eq!(m.phase(), EyeBreakPhase::Break);
        assert!(!m.holds_belt());
    }

    #[test]
    fn a_rest_more_than_half_an_interval_early_leaves_the_break_for_later() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        // At 300 s the break is 900 s away; the short rest ends at 600 s.
        let walk = program_resting_at(300, SHORT);
        run_by(&mut m, &c, 0, 599, &walk);
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(m.end_seq, 0);
    }

    #[test]
    fn with_fit_rests_off_a_break_pauses_a_program_on_time() {
        let mut m = EyeBreak::default();
        let c = Config {
            fit_rests: false,
            ..cfg()
        };
        let fx = run_by(&mut m, &c, 0, 1200, program_resting_at(1300, LONG));
        assert_eq!(fx, vec![(1200, Effect::Pause(Hold::Program(2)))]);
        // Nor does a short rest take one early.
        let mut m = EyeBreak::default();
        run_by(&mut m, &c, 0, 800, program_resting_at(700, SHORT));
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
    }

    #[test]
    fn a_manual_walk_never_waits_for_a_rest() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        let walk = |_| Tick {
            rest_in_ms: Some(100 * SECOND),
            ..belt(WALK)
        };
        let fx = run_by(&mut m, &c, 0, 1200, walk);
        assert_eq!(fx, vec![(1200, Effect::Pause(Hold::Manual))]);
    }

    #[test]
    fn a_program_paused_while_its_break_waits_gets_the_break_at_once() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run_by(&mut m, &c, 0, 1250, program_resting_at(1400, LONG));
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        // The user pauses the program: the break starts, and isn't the pause's owner.
        let fx = run(&mut m, &c, 1251, 1251, belt(Belt::Paused(Hold::Program(2))));
        assert_eq!(fx, vec![]);
        assert_eq!(m.phase(), EyeBreakPhase::Break);
        assert!(!m.holds_belt());
    }

    #[test]
    fn the_display_off_starts_the_count_again() {
        let (mut m, c) = (EyeBreak::default(), cfg());
        run(&mut m, &c, 0, 1150, typing());
        assert_eq!(m.phase(), EyeBreakPhase::Upcoming);
        let dark = Tick {
            display_off: true,
            held: false,
            ..typing()
        };
        m.tick(&c, T0 + 1151 * SECOND, &dark);
        assert_eq!(m.phase(), EyeBreakPhase::Idle);
        assert_eq!(m.active_ms, 0);
    }

    #[test]
    fn bad_settings_fall_back_to_defaults() {
        let c = config_from_raw(
            Some("garbage"),
            Some("1"),
            Some("9999"),
            None,
            Some("x"),
            Some("y"),
        );
        assert_eq!(c, Config::default());
        let c = config_from_raw(
            Some("true"),
            Some("30"),
            Some("300"),
            Some("false"),
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
                fit_rests: false,
            }
        );
        assert!(validate(5, 90).is_err());
        assert!(validate(20, 10).is_err());
        assert!(validate(20, 90).is_ok());
    }
}
