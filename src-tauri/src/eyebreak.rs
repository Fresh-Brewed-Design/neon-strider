//! Eye breaks.
//!
//! Optional (off by default). After a set time of screen use, the HUD says a
//! break is a minute away, then runs a break: a card with a countdown, a tone
//! every 10 seconds to blink, and a tone at the end. The defaults follow the
//! research in the roadmap issue: a break of 90 seconds, because the focusing
//! system needs about that long to relax (Boulet 2016), every 20 minutes.
//! Longer breaks (5 minutes an hour, Pucker & Gawne 2023) are one setting away.
//!
//! This step never touches the belt: you can look away while you walk. Pausing
//! the belt for a break and folding breaks into programs are still to come.
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

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub minutes: u32,
    pub seconds: u32,
    pub sound: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            minutes: DEFAULT_MINUTES,
            seconds: DEFAULT_SECONDS,
            sound: true,
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
}

#[derive(Debug, Default)]
pub struct EyeBreak {
    active_ms: u64,
    break_ms: u64,
    phase: EyeBreakPhase,
    last_tick_ms: Option<u64>,
    blink_seq: u32,
    end_seq: u32,
}

impl EyeBreak {
    fn reset(&mut self) {
        self.active_ms = 0;
        self.break_ms = 0;
        self.phase = EyeBreakPhase::Idle;
    }

    pub fn tick(&mut self, cfg: &Config, now_ms: u64, input: &Tick) {
        let step = self
            .last_tick_ms
            .map_or(0, |prev| now_ms.saturating_sub(prev));
        self.last_tick_ms = Some(now_ms);
        let dt = if step > MAX_STEP_MS { 0 } else { step };
        let away = input.locked || step >= AWAY_MS || input.idle_ms.is_some_and(|i| i > AWAY_MS);
        if !cfg.enabled || away {
            self.reset();
            return;
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
            return;
        }

        match input.idle_ms {
            // Off the screen for a whole break already: that was one.
            Some(i) if i >= break_ms => {
                self.reset();
                return;
            }
            Some(i) if i <= ACTIVE_WINDOW_MS => self.active_ms += dt,
            _ => {}
        }

        let interval = cfg.minutes as u64 * MINUTE;
        if self.active_ms >= interval {
            self.phase = EyeBreakPhase::Break;
            self.break_ms = 0;
            self.blink_seq = self.blink_seq.wrapping_add(1);
        } else if self.active_ms + WARN_MS >= interval {
            self.phase = EyeBreakPhase::Upcoming;
        } else {
            self.phase = EyeBreakPhase::Idle;
        }
    }

    /// SKIP on the card: no tone, and the count starts again.
    pub fn skip(&mut self) {
        self.reset();
    }

    pub fn phase(&self) -> EyeBreakPhase {
        self.phase
    }

    fn remaining_s(&self, cfg: &Config) -> u32 {
        let ms = match self.phase {
            EyeBreakPhase::Idle => return 0,
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
/// defaults (off, 20 minutes, 90 seconds, sound on).
pub fn config_from_raw(
    enabled: Option<&str>,
    minutes: Option<&str>,
    seconds: Option<&str>,
    sound: Option<&str>,
) -> Config {
    Config {
        enabled: persistence::parse_setting(enabled).unwrap_or(false),
        minutes: persistence::setting_in(minutes, MIN_MINUTES..=MAX_MINUTES, DEFAULT_MINUTES),
        seconds: persistence::setting_in(seconds, MIN_SECONDS..=MAX_SECONDS, DEFAULT_SECONDS),
        sound: persistence::parse_setting(sound).unwrap_or(true),
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
    );
    with(|rt| rt.cfg = cfg);
}

/// Blocking (SQLite).
pub fn set_config(
    app: &AppHandle,
    enabled: bool,
    minutes: u32,
    seconds: u32,
    sound: bool,
) -> Result<(), String> {
    validate(minutes, seconds)?;
    persistence::write_setting(app, KEY_ENABLED, &enabled.to_string())?;
    persistence::write_setting(app, KEY_MINUTES, &minutes.to_string())?;
    persistence::write_setting(app, KEY_SECONDS, &seconds.to_string())?;
    persistence::write_setting(app, KEY_SOUND, &sound.to_string())?;
    with(|rt| {
        rt.cfg = Config {
            enabled,
            minutes,
            seconds,
            sound,
        };
        if !enabled {
            rt.machine.reset();
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
        phase: if rt.cfg.enabled {
            rt.machine.phase
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
        }
    }

    fn typing() -> Tick {
        Tick {
            idle_ms: Some(0),
            locked: false,
        }
    }

    /// One tick a second from `from` to `to` (seconds after T0).
    fn run(m: &mut EyeBreak, c: &Config, from: u64, to: u64, input: Tick) {
        for s in from..=to {
            m.tick(c, T0 + s * SECOND, &input);
        }
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
        let c = config_from_raw(Some("garbage"), Some("1"), Some("9999"), None);
        assert_eq!(c, Config::default());
        let c = config_from_raw(Some("true"), Some("30"), Some("300"), Some("false"));
        assert_eq!(
            c,
            Config {
                enabled: true,
                minutes: 30,
                seconds: 300,
                sound: false
            }
        );
        assert!(validate(5, 90).is_err());
        assert!(validate(20, 10).is_err());
        assert!(validate(20, 90).is_ok());
    }
}
