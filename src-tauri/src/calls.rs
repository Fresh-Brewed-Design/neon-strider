//! Slow down on calls.
//!
//! While any app is using the microphone (a call), a running belt eases to a
//! quiet speed, so the walking is less noticeable to the people on the call, and
//! goes back when the call ends. Three parts:
//!
//! - `mic`: polls whether another app uses the microphone (`platform`);
//! - `CallMachine`: the pure decision logic, driven with injected times;
//! - the settings and the snapshot.
//!
//! `desk.rs` polls the microphone, runs the machine once a second and applies
//! the actions to the orchestrator.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;

// Settings keys (raw values in the `settings` table).
pub const KEY_ENABLED: &str = "calls_enabled";
const KEY_QUIET_KMH: &str = "calls_quiet_kmh";

/// About the 1.8 km/h office workers chose while working (Levine and Miller 2007,
/// docs/research.md, Focus): slow enough to talk without sounding out of breath.
const DEFAULT_QUIET_KMH: f32 = 2.0;
/// Below a typical deck's slowest speed (0.5 to 1 km/h) the speed would only be
/// raised to the floor. Above 6 km/h is a brisk walk, not quiet.
pub(crate) const MIN_QUIET_KMH: f32 = 0.5;
pub(crate) const MAX_QUIET_KMH: f32 = 6.0;

/// The microphone must be free this long before a call counts as over.
/// Call apps release the mic for a moment when muting, switching devices or
/// between calls; 10 s rides that out. DeskSection's text quotes it.
const CALL_END_HOLD_MS: u64 = 10_000;
/// ...and in use this long before it counts as a call, so an app that opens
/// the mic for a moment doesn't slow the belt.
const CALL_START_HOLD_MS: u64 = 4_000;
/// Two speeds this close are the same one.
/// Half the deck's 0.1 km/h step.
const SAME_KMH: f32 = 0.05;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub quiet_kmh: f32,
    /// How long the mic must be in use before it counts as a call.
    pub start_hold_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            quiet_kmh: DEFAULT_QUIET_KMH,
            start_hold_ms: CALL_START_HOLD_MS,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CallsSnapshot {
    pub enabled: bool,
    pub quiet_kmh: f32,
    /// Some app is using the microphone right now.
    pub mic_in_use: bool,
    /// The belt is currently held at the quiet speed because of a call.
    pub quiet_active: bool,
}

/// What the machine needs to know about the belt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Belt {
    pub running: bool,
    /// A program step is walking, so the speed lives in the program's live target.
    pub program: bool,
    /// The current target speed: the program's live target, or the manual one.
    pub target_kmh: f32,
    /// The slowest speed the treadmill accepts; a quiet speed below it is raised.
    pub min_kmh: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Input {
    pub mic_in_use: bool,
    pub belt: Belt,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// A call began: ease to this speed.
    Quiet { to_kmh: f32 },
    /// The call ended and nobody touched the speed: put it back.
    Restore { to_kmh: f32 },
}

// ---------------------------------------------------------------------------
// The decision logic
// ---------------------------------------------------------------------------

/// A speed change the machine made, remembered for the restore.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Adjusted {
    saved_kmh: f32,
    set_kmh: f32,
    program: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Phase {
    #[default]
    Idle,
    /// A call is going on. `adjusted` is set when we slowed the belt for it.
    Call {
        adjusted: Option<Adjusted>,
        /// When the microphone last became free; `None` while it is in use.
        free_since_ms: Option<u64>,
    },
}

#[derive(Debug, Default)]
pub struct CallMachine {
    phase: Phase,
    /// While idle: when the microphone was first seen in use.
    mic_since_ms: Option<u64>,
}

impl CallMachine {
    /// We are holding the belt at the quiet speed for a call.
    pub fn quiet_active(&self) -> bool {
        matches!(
            self.phase,
            Phase::Call {
                adjusted: Some(_),
                ..
            }
        )
    }

    /// Advance to `now_ms`. Returns a speed change to apply, if any.
    pub fn tick(&mut self, cfg: &Config, now_ms: u64, input: &Input) -> Option<Action> {
        match self.phase {
            Phase::Idle => {
                if !(cfg.enabled && input.mic_in_use) {
                    self.mic_since_ms = None;
                    return None;
                }
                let since = *self.mic_since_ms.get_or_insert(now_ms);
                if now_ms.saturating_sub(since) < cfg.start_hold_ms {
                    return None;
                }
                self.mic_since_ms = None;
                let belt = &input.belt;
                let quiet = cfg.quiet_kmh.max(belt.min_kmh);
                // Only ever slower: a belt already at or under the quiet speed
                // (or not running) is left alone, for this whole call.
                let adjusted =
                    (belt.running && belt.target_kmh > quiet + SAME_KMH).then_some(Adjusted {
                        saved_kmh: belt.target_kmh,
                        set_kmh: quiet,
                        program: belt.program,
                    });
                self.phase = Phase::Call {
                    adjusted,
                    free_since_ms: None,
                };
                adjusted.map(|a| Action::Quiet { to_kmh: a.set_kmh })
            }
            Phase::Call {
                adjusted,
                free_since_ms,
            } => {
                let free_since_ms = if input.mic_in_use {
                    None
                } else {
                    Some(free_since_ms.unwrap_or(now_ms))
                };
                let over =
                    free_since_ms.is_some_and(|t| now_ms.saturating_sub(t) >= CALL_END_HOLD_MS);
                // Turning the feature off puts the belt back at once.
                if over || !cfg.enabled {
                    self.phase = Phase::Idle;
                    return adjusted.and_then(|a| restore(&a, &input.belt));
                }
                self.phase = Phase::Call {
                    adjusted,
                    free_since_ms,
                };
                None
            }
        }
    }
}

/// Put the speed back, unless the user (or a program step change) already
/// moved it: it must still be the quiet speed we set, in the same mode, on a
/// belt that is still running. A stopped or paused belt is never restarted by a
/// restore.
fn restore(a: &Adjusted, belt: &Belt) -> Option<Action> {
    let untouched = belt.running
        && belt.program == a.program
        && (belt.target_kmh - a.set_kmh).abs() <= SAME_KMH;
    untouched.then_some(Action::Restore {
        to_kmh: a.saved_kmh,
    })
}

// ---------------------------------------------------------------------------
// Microphone detection
// ---------------------------------------------------------------------------

/// Whether another app is using the microphone, polled in the background (the
/// OS side is `platform::apps_using_microphone`).
pub mod mic {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// Half of `CALL_START_HOLD_MS`, so a call is seen 4 to 6 s after it starts;
    /// each poll walks the consent store in the registry, so not much more often.
    const POLL_EVERY: Duration = Duration::from_secs(2);

    /// Latest answer from the poller.
    static IN_USE: AtomicBool = AtomicBool::new(false);

    pub fn in_use_now() -> bool {
        IN_USE.load(Ordering::Relaxed)
    }

    /// Poll on a thread of its own (the OS query blocks; this keeps it off the
    /// async runtime), and log when the set of apps using it changes.
    pub fn spawn_poller() {
        let own_exe = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
        let spawned = std::thread::Builder::new()
            .name("mic-poll".to_string())
            .spawn(move || {
                let mut last: Vec<String> = Vec::new();
                loop {
                    let mut names = crate::platform::apps_using_microphone(own_exe.as_deref());
                    IN_USE.store(!names.is_empty(), Ordering::Relaxed);
                    names.sort();
                    if names != last {
                        if names.is_empty() {
                            log::info!("Microphone free");
                        } else {
                            log::info!("Microphone in use by {}", names.join(", "));
                        }
                        last = names;
                    }
                    std::thread::sleep(POLL_EVERY);
                }
            });
        if let Err(e) = spawned {
            log::warn!("Could not start the microphone poller: {}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// Settings and shared state
// ---------------------------------------------------------------------------

static CONFIG: Mutex<Config> = Mutex::new(Config {
    enabled: true,
    quiet_kmh: DEFAULT_QUIET_KMH,
    start_hold_ms: CALL_START_HOLD_MS,
});
/// Published by the desk loop for the snapshot.
static QUIET_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn config() -> Config {
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_quiet_active(active: bool) {
    QUIET_ACTIVE.store(active, Ordering::Relaxed);
}

/// The belt is held at the quiet speed for a call (Neural flow defers to it).
pub fn quiet_active() -> bool {
    QUIET_ACTIVE.load(Ordering::Relaxed)
}

/// Build the config from raw stored values. Bad values fall back to the defaults
/// (on, 2.0 km/h).
pub fn config_from_raw(enabled: Option<&str>, quiet_kmh: Option<&str>) -> Config {
    Config {
        enabled: persistence::parse_setting(enabled).unwrap_or(true),
        // The range also rules out NaN and infinity.
        quiet_kmh: persistence::setting_in(
            quiet_kmh,
            MIN_QUIET_KMH..=MAX_QUIET_KMH,
            DEFAULT_QUIET_KMH,
        ),
        start_hold_ms: CALL_START_HOLD_MS,
    }
}

/// 0.5 to 6 km/h.
fn validate_quiet_kmh(kmh: f32) -> Result<f32, String> {
    if !kmh.is_finite() || !(MIN_QUIET_KMH..=MAX_QUIET_KMH).contains(&kmh) {
        return Err(format!(
            "Quiet speed must be between {} and {} km/h",
            MIN_QUIET_KMH, MAX_QUIET_KMH
        ));
    }
    Ok(kmh)
}

/// Load the settings. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let read = |k: &str| persistence::read_setting(app, k);
    let cfg = config_from_raw(read(KEY_ENABLED).as_deref(), read(KEY_QUIET_KMH).as_deref());
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = cfg;
}

/// Blocking (SQLite).
pub fn set_config(app: &AppHandle, enabled: bool, quiet_kmh: f32) -> Result<(), String> {
    let quiet_kmh = validate_quiet_kmh(quiet_kmh)?;
    persistence::write_setting(app, KEY_ENABLED, &enabled.to_string())?;
    persistence::write_setting(app, KEY_QUIET_KMH, &quiet_kmh.to_string())?;
    *CONFIG.lock().unwrap_or_else(|e| e.into_inner()) = Config {
        enabled,
        quiet_kmh,
        start_hold_ms: CALL_START_HOLD_MS,
    };
    Ok(())
}

/// For the snapshot. No I/O.
pub fn snapshot() -> CallsSnapshot {
    let cfg = config();
    CallsSnapshot {
        enabled: cfg.enabled,
        quiet_kmh: cfg.quiet_kmh,
        mic_in_use: mic::in_use_now(),
        quiet_active: quiet_active(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;
    const S: u64 = 1000;

    /// On, 2.0 km/h, and no start hold so each test controls timing itself.
    fn cfg() -> Config {
        Config {
            start_hold_ms: 0,
            ..Config::default()
        }
    }

    #[test]
    fn the_mic_must_be_in_use_for_four_seconds_before_slowing() {
        let c = Config::default();
        let mut m = CallMachine::default();
        // A one-second blip does nothing.
        assert_eq!(m.tick(&c, T0, &call(running(4.0))), None);
        assert_eq!(m.tick(&c, T0 + S, &quiet_call(running(4.0))), None);
        assert!(!m.quiet_active());
        // A real call: slows once it has lasted 4 s.
        let actions = run(&mut m, &c, 10, 20, call(running(4.0)));
        assert_eq!(actions, vec![(14, Action::Quiet { to_kmh: 2.0 })]);
    }

    fn running(target: f32) -> Belt {
        Belt {
            running: true,
            program: false,
            target_kmh: target,
            min_kmh: 0.6,
        }
    }

    fn program(target: f32) -> Belt {
        Belt {
            program: true,
            ..running(target)
        }
    }

    fn call(belt: Belt) -> Input {
        Input {
            mic_in_use: true,
            belt,
        }
    }

    fn quiet_call(belt: Belt) -> Input {
        Input {
            mic_in_use: false,
            belt,
        }
    }

    /// Ticks once a second from `from` to `to` (seconds after T0) with a fixed input.
    fn run(
        m: &mut CallMachine,
        c: &Config,
        from: u64,
        to: u64,
        input: Input,
    ) -> Vec<(u64, Action)> {
        (from..=to)
            .filter_map(|t| m.tick(c, T0 + t * S, &input).map(|a| (t, a)))
            .collect()
    }

    #[test]
    fn call_start_while_running_eases_to_the_quiet_speed() {
        let mut m = CallMachine::default();
        assert_eq!(m.tick(&cfg(), T0, &quiet_call(running(4.0))), None);
        assert!(!m.quiet_active());
        assert_eq!(
            m.tick(&cfg(), T0 + S, &call(running(4.0))),
            Some(Action::Quiet { to_kmh: 2.0 })
        );
        assert!(m.quiet_active());
        // The same call never slows it again.
        assert_eq!(m.tick(&cfg(), T0 + 2 * S, &call(running(2.0))), None);
    }

    #[test]
    fn call_end_restores_after_ten_free_seconds() {
        let mut m = CallMachine::default();
        m.tick(&cfg(), T0, &call(running(4.0)));
        // The belt is now at the quiet speed (the orchestrator applied it).
        let at_quiet = running(2.0);
        let fx = run(&mut m, &cfg(), 1, 30, call(at_quiet));
        assert!(fx.is_empty());
        // Mic goes free at t = 31 s; restore exactly 10 s later.
        let fx = run(&mut m, &cfg(), 31, 45, quiet_call(at_quiet));
        assert_eq!(fx, vec![(41, Action::Restore { to_kmh: 4.0 })]);
        assert!(!m.quiet_active());
        // And only once.
        assert!(run(&mut m, &cfg(), 46, 60, quiet_call(running(4.0))).is_empty());
    }

    #[test]
    fn a_mic_blip_does_not_end_the_call() {
        let mut m = CallMachine::default();
        m.tick(&cfg(), T0, &call(running(4.0)));
        let b = running(2.0);
        // Free for 9 s, then back in use: the 10 s hold starts over.
        assert!(run(&mut m, &cfg(), 1, 9, quiet_call(b)).is_empty());
        assert!(run(&mut m, &cfg(), 10, 11, call(b)).is_empty());
        assert!(m.quiet_active());
        assert!(run(&mut m, &cfg(), 12, 20, quiet_call(b)).is_empty());
        let fx = run(&mut m, &cfg(), 21, 30, quiet_call(b));
        assert_eq!(fx, vec![(22, Action::Restore { to_kmh: 4.0 })]);
    }

    #[test]
    fn a_speed_the_user_set_during_the_call_is_left_alone() {
        let mut m = CallMachine::default();
        m.tick(&cfg(), T0, &call(running(4.0)));
        // They turn it up to 3.0 mid-call.
        run(&mut m, &cfg(), 1, 20, call(running(3.0)));
        let fx = run(&mut m, &cfg(), 21, 40, quiet_call(running(3.0)));
        assert!(fx.is_empty());
        assert!(!m.quiet_active());
        // Turning it even slightly different counts (a nudge of 0.1).
        let mut m = CallMachine::default();
        m.tick(&cfg(), T0, &call(running(4.0)));
        let fx = run(&mut m, &cfg(), 1, 30, quiet_call(running(2.1)));
        assert!(fx.is_empty());
    }

    #[test]
    fn a_belt_already_slower_is_never_sped_up_or_restored() {
        let mut m = CallMachine::default();
        // 1.5 km/h is under the 2.0 quiet speed: nothing happens at the start...
        assert_eq!(m.tick(&cfg(), T0, &call(running(1.5))), None);
        assert!(!m.quiet_active());
        // ...and nothing at the end, even though the belt is still running.
        assert!(run(&mut m, &cfg(), 1, 30, quiet_call(running(1.5))).is_empty());
        // Exactly at the quiet speed counts as already quiet.
        let mut m = CallMachine::default();
        assert_eq!(m.tick(&cfg(), T0, &call(running(2.0))), None);
        assert_eq!(m.tick(&cfg(), T0 + S, &call(running(2.04))), None);
    }

    #[test]
    fn a_belt_that_is_not_running_is_left_alone_for_the_whole_call() {
        let mut m = CallMachine::default();
        let stopped = Belt {
            running: false,
            ..running(4.0)
        };
        assert_eq!(m.tick(&cfg(), T0, &call(stopped)), None);
        // Starting mid-call is the user's choice; no quieting later.
        assert_eq!(m.tick(&cfg(), T0 + S, &call(running(4.0))), None);
        assert!(run(&mut m, &cfg(), 2, 30, quiet_call(running(4.0))).is_empty());
    }

    #[test]
    fn a_belt_stopped_during_the_call_is_not_restarted_by_the_restore() {
        let mut m = CallMachine::default();
        m.tick(&cfg(), T0, &call(running(4.0)));
        let stopped = Belt {
            running: false,
            ..running(2.0)
        };
        assert!(run(&mut m, &cfg(), 1, 30, quiet_call(stopped)).is_empty());
        assert!(!m.quiet_active());
    }

    #[test]
    fn programs_are_quieted_and_restored_through_their_live_target() {
        let mut m = CallMachine::default();
        assert_eq!(
            m.tick(&cfg(), T0, &call(program(3.5))),
            Some(Action::Quiet { to_kmh: 2.0 })
        );
        let fx = run(&mut m, &cfg(), 1, 40, quiet_call(program(2.0)));
        assert_eq!(fx, vec![(11, Action::Restore { to_kmh: 3.5 })]);
    }

    #[test]
    fn a_program_step_change_during_the_call_is_not_overwritten() {
        let mut m = CallMachine::default();
        m.tick(&cfg(), T0, &call(program(3.5)));
        // The next step starts at 4.0: the target is no longer our quiet speed.
        let fx = run(&mut m, &cfg(), 1, 40, quiet_call(program(4.0)));
        assert!(fx.is_empty());
    }

    #[test]
    fn switching_between_program_and_manual_during_the_call_is_not_restored() {
        let mut m = CallMachine::default();
        m.tick(&cfg(), T0, &call(running(4.0)));
        // A program took over at what happens to be the quiet speed.
        let fx = run(&mut m, &cfg(), 1, 40, quiet_call(program(2.0)));
        assert!(fx.is_empty());
    }

    #[test]
    fn disabled_does_nothing() {
        let off = Config {
            enabled: false,
            ..cfg()
        };
        let mut m = CallMachine::default();
        assert!(run(&mut m, &off, 0, 30, call(running(4.0))).is_empty());
        assert!(!m.quiet_active());
    }

    #[test]
    fn turning_the_feature_off_mid_call_restores_at_once() {
        let mut m = CallMachine::default();
        m.tick(&cfg(), T0, &call(running(4.0)));
        let off = Config {
            enabled: false,
            ..cfg()
        };
        assert_eq!(
            m.tick(&off, T0 + 5 * S, &call(running(2.0))),
            Some(Action::Restore { to_kmh: 4.0 })
        );
        assert!(!m.quiet_active());
    }

    #[test]
    fn the_quiet_speed_is_never_below_what_the_treadmill_accepts() {
        let mut m = CallMachine::default();
        let c = Config {
            quiet_kmh: 0.5,
            ..cfg()
        };
        assert_eq!(
            m.tick(&c, T0, &call(running(4.0))),
            Some(Action::Quiet { to_kmh: 0.6 })
        );
        // And the restore still recognises it as ours.
        let fx = run(&mut m, &c, 1, 30, quiet_call(running(0.6)));
        assert_eq!(fx, vec![(11, Action::Restore { to_kmh: 4.0 })]);
    }

    #[test]
    fn the_quiet_speed_setting_is_used() {
        let mut m = CallMachine::default();
        let c = Config {
            quiet_kmh: 3.0,
            ..cfg()
        };
        assert_eq!(
            m.tick(&c, T0, &call(running(4.5))),
            Some(Action::Quiet { to_kmh: 3.0 })
        );
    }

    #[test]
    fn a_second_call_after_the_first_is_handled_again() {
        let mut m = CallMachine::default();
        m.tick(&cfg(), T0, &call(running(4.0)));
        run(&mut m, &cfg(), 1, 30, quiet_call(running(2.0)));
        assert!(!m.quiet_active());
        // The belt is back at 4.0; a new call quiets it again.
        assert_eq!(
            m.tick(&cfg(), T0 + 100 * S, &call(running(4.0))),
            Some(Action::Quiet { to_kmh: 2.0 })
        );
    }

    // ---- settings ----

    #[test]
    fn settings_default_to_on_two_kmh() {
        let c = config_from_raw(None, None);
        assert!(c.enabled);
        assert_eq!(c.quiet_kmh, 2.0);
    }

    #[test]
    fn settings_parse_and_bad_values_fall_back() {
        let c = config_from_raw(Some("false"), Some("1.5"));
        assert_eq!(
            c,
            Config {
                enabled: false,
                quiet_kmh: 1.5,
                start_hold_ms: CALL_START_HOLD_MS,
            }
        );
        assert_eq!(
            config_from_raw(Some("nope"), Some("0.1")),
            Config::default()
        );
        assert_eq!(config_from_raw(None, Some("6.1")).quiet_kmh, 2.0);
        assert_eq!(config_from_raw(None, Some("nan")).quiet_kmh, 2.0);
        assert_eq!(config_from_raw(None, Some("\"2.5\"")).quiet_kmh, 2.5);
    }

    #[test]
    fn quiet_speed_validation() {
        assert_eq!(validate_quiet_kmh(2.0), Ok(2.0));
        assert_eq!(validate_quiet_kmh(0.5), Ok(0.5));
        assert_eq!(validate_quiet_kmh(6.0), Ok(6.0));
        assert!(validate_quiet_kmh(0.4).is_err());
        assert!(validate_quiet_kmh(6.1).is_err());
        assert!(validate_quiet_kmh(f32::NAN).is_err());
    }
}
