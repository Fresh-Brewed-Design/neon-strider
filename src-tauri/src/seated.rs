//! Sit-too-long nudge.
//!
//! After a set time of PC use without walking, a persistent HUD card asks for a
//! walk: ENGAGE (a plain walk, Engage's own path), then one-tap programs, then
//! SNOOZE 15 MIN. Nothing here starts the belt without that press. The
//! treadmill powers itself off when idle: ENGAGE then only says so, while a
//! program picked while it is off is remembered and started as soon as it
//! connects (within 15 minutes).
//!
//! "PC use" is any keyboard or mouse input seen by the global input hooks
//! (`hooks::input_idle_ms`; only the time of the last one is kept). Seated
//! time builds while there was input in the last 5 minutes and the belt is not
//! running. Walking (belt running for 2 minutes), or the PC being idle for 15
//! minutes (away, locked, asleep), resets it.
//!
//! The state machine is pure: it takes epoch-millisecond times as arguments, so
//! the tests drive it with a fake clock. Wall-clock time, not `Instant`, so that
//! a sleeping PC counts as time away. `desk.rs` runs it once a second.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use crate::programs::Program;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

// Settings keys (raw values in the `settings` table).
pub const KEY_ENABLED: &str = "seated_enabled";
const KEY_MINUTES: &str = "seated_minutes";
/// JSON array of program ids; absent means the first three saved programs.
pub const KEY_PROGRAMS: &str = "seated_program_ids";

const DEFAULT_MINUTES: u32 = 60;
const MIN_MINUTES: u32 = 15;
const MAX_MINUTES: u32 = 240;
/// Program buttons on the card.
const MAX_PROGRAM_BUTTONS: usize = 3;

const SECOND: u64 = 1000;
const MINUTE: u64 = 60 * SECOND;
/// Input within this long counts as using the PC.
const ACTIVE_WINDOW_MS: u64 = 5 * MINUTE;
/// No input for longer than this means the user left.
const AWAY_MS: u64 = 15 * MINUTE;
/// The belt must run this long to count as a walk.
const WALK_MS: u64 = 2 * MINUTE;
/// What SNOOZE 15 MIN snoozes for.
const SNOOZE_MS: u64 = 15 * MINUTE;
/// Quiet time after starting a program, so the card does not flash back if the
/// belt takes a moment to spin up.
const START_GRACE_MS: u64 = 2 * MINUTE;
/// A remembered program is dropped after this long.
const PENDING_TTL_MS: u64 = 15 * MINUTE;
/// A longer gap between two ticks is a stall or sleep, not time passing seated.
const MAX_STEP_MS: u64 = 10 * SECOND;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub minutes: u32,
    /// `None` = the first three saved programs.
    pub program_ids: Option<Vec<u32>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            minutes: DEFAULT_MINUTES,
            program_ids: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SeatedSnapshot {
    pub enabled: bool,
    /// Minutes of sitting before the card appears.
    pub minutes: u32,
    /// Programs the card offers (existing ones, at most three).
    pub program_ids: Vec<u32>,
    /// `program_ids` was chosen by the user, not the default first three.
    pub custom_programs: bool,
    /// The HUD should show the card.
    pub show_card: bool,
    /// Whole minutes seated; what the card says.
    pub seated_minutes: u32,
    /// A program that starts as soon as the treadmill connects.
    pub pending_program_id: Option<u32>,
}

/// What the desk loop tells the machine each second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// Time since the last keyboard or mouse input; `None` if none was seen yet.
    pub idle_ms: Option<u64>,
    pub belt_running: bool,
    /// The treadmill is connected and has reported once, so a program can start.
    pub ready: bool,
    /// A program is paused waiting for GO. That is not walking, so seated time
    /// still builds, but the card stays back: the HUD is already asking for
    /// GO, and one prompt at a time is enough.
    pub program_waiting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    StartProgram(u32),
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    program_id: u32,
    at_ms: u64,
}

// ---------------------------------------------------------------------------
// The state machine
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct Seated {
    seated_ms: u64,
    walking_ms: u64,
    last_tick_ms: Option<u64>,
    showing: bool,
    /// The card stays hidden until this time (snooze, or just started a program).
    quiet_until_ms: Option<u64>,
    pending: Option<Pending>,
}

impl Seated {
    fn clear_timer(&mut self) {
        self.seated_ms = 0;
        self.showing = false;
        self.quiet_until_ms = None;
    }

    /// Hide the card and keep it away for a moment after a program started.
    fn started(&mut self, now_ms: u64) {
        self.showing = false;
        self.quiet_until_ms = Some(now_ms + START_GRACE_MS);
    }

    /// Advance to `now_ms`. Returns a program to start, if a remembered one is
    /// now possible.
    pub fn tick(&mut self, cfg: &Config, now_ms: u64, input: &Tick) -> Option<Effect> {
        let step = match self.last_tick_ms {
            Some(prev) => now_ms.saturating_sub(prev),
            None => 0,
        };
        self.last_tick_ms = Some(now_ms);
        // A long gap means the PC slept or the loop stalled. Nothing is counted for
        // it, and one as long as being away means the user was.
        let gap_is_away = step >= AWAY_MS;
        let dt = if step > MAX_STEP_MS { 0 } else { step };

        let mut effect = None;
        if let Some(p) = self.pending {
            if now_ms.saturating_sub(p.at_ms) > PENDING_TTL_MS {
                log::info!(
                    "Remembered program {} expired without a connection",
                    p.program_id
                );
                self.pending = None;
            } else if input.ready {
                self.pending = None;
                if input.belt_running {
                    // Started by hand in the meantime: nothing to do.
                    log::info!(
                        "Treadmill already running; dropping remembered program {}",
                        p.program_id
                    );
                } else {
                    effect = Some(Effect::StartProgram(p.program_id));
                    self.started(now_ms);
                }
            }
        }

        if !cfg.enabled || gap_is_away || input.idle_ms.is_some_and(|i| i > AWAY_MS) {
            self.clear_timer();
            self.walking_ms = 0;
            return effect;
        }

        if input.belt_running {
            // The card makes no sense while walking; a real walk (2 minutes) resets.
            self.showing = false;
            self.walking_ms += dt;
            if self.walking_ms >= WALK_MS {
                self.clear_timer();
            }
        } else {
            self.walking_ms = 0;
            if input.idle_ms.is_some_and(|i| i <= ACTIVE_WINDOW_MS) {
                self.seated_ms += dt;
            }
        }

        let threshold = cfg.minutes as u64 * MINUTE;
        if self.seated_ms < threshold || input.program_waiting {
            self.showing = false;
        } else if !input.belt_running && !self.showing {
            let quiet = self.quiet_until_ms.is_some_and(|q| now_ms < q);
            if !quiet {
                self.showing = true;
            }
        }
        effect
    }

    /// SNOOZE 15 MIN: hide the card for 15 minutes. The time already seated is
    /// kept, so it comes back at once when the snooze ends.
    pub fn snooze(&mut self, now_ms: u64) {
        self.showing = false;
        self.quiet_until_ms = Some(now_ms + SNOOZE_MS);
        self.pending = None;
    }

    /// The nudge was switched off: forget the time seated and any remembered
    /// program.
    pub fn reset(&mut self) {
        self.clear_timer();
        self.pending = None;
    }

    /// ENGAGE was pressed and the belt was asked to start (the caller sent
    /// Engage's own start): hide the card as after a program start. A plain walk
    /// is never remembered for later, so a remembered program is dropped too.
    pub fn engaged(&mut self, now_ms: u64) {
        self.pending = None;
        self.started(now_ms);
    }

    /// A program button. `ready`: the treadmill is connected, so start it now;
    /// otherwise remember it for when it connects.
    pub fn press(&mut self, program_id: u32, ready: bool, now_ms: u64) -> Option<Effect> {
        if ready {
            self.pending = None;
            self.started(now_ms);
            Some(Effect::StartProgram(program_id))
        } else {
            self.pending = Some(Pending {
                program_id,
                at_ms: now_ms,
            });
            None
        }
    }

    pub fn show_card(&self) -> bool {
        self.showing
    }

    pub fn seated_minutes(&self) -> u32 {
        (self.seated_ms / MINUTE).min(u32::MAX as u64) as u32
    }

    pub fn pending_program_id(&self) -> Option<u32> {
        self.pending.map(|p| p.program_id)
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Build the config from raw stored values. Bad values fall back to the
/// defaults (on, 60 minutes, the first three programs).
pub fn config_from_raw(
    enabled: Option<&str>,
    minutes: Option<&str>,
    program_ids: Option<&str>,
) -> Config {
    let program_ids = program_ids
        .and_then(|raw| serde_json::from_str::<Vec<u32>>(raw).ok())
        .map(|ids| {
            let mut ids = dedup(ids);
            ids.truncate(MAX_PROGRAM_BUTTONS);
            ids
        })
        .filter(|ids| !ids.is_empty());
    Config {
        enabled: persistence::parse_setting(enabled).unwrap_or(true),
        minutes: persistence::setting_in(minutes, MIN_MINUTES..=MAX_MINUTES, DEFAULT_MINUTES),
        program_ids,
    }
}

/// `ids` in order, each once.
fn dedup(ids: Vec<u32>) -> Vec<u32> {
    let mut out = Vec::new();
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Minutes 15 to 240; at most three distinct programs (empty or `None` = the
/// first three saved).
fn validate(
    minutes: u32,
    program_ids: Option<Vec<u32>>,
) -> Result<(u32, Option<Vec<u32>>), String> {
    if !(MIN_MINUTES..=MAX_MINUTES).contains(&minutes) {
        return Err(format!(
            "Sitting time must be between {} and {} minutes",
            MIN_MINUTES, MAX_MINUTES
        ));
    }
    let ids = match program_ids {
        None => None,
        Some(ids) => {
            let unique = dedup(ids);
            if unique.len() > MAX_PROGRAM_BUTTONS {
                return Err(format!("Pick at most {} programs", MAX_PROGRAM_BUTTONS));
            }
            Some(unique).filter(|u| !u.is_empty())
        }
    };
    Ok((minutes, ids))
}

/// The programs the card offers: the chosen ones that still exist, or the first
/// three saved when none were chosen (or every chosen one was deleted).
fn effective_program_ids(chosen: Option<&[u32]>, programs: &[Program]) -> Vec<u32> {
    let chosen: Vec<u32> = chosen
        .unwrap_or(&[])
        .iter()
        .copied()
        .filter(|id| programs.iter().any(|p| p.id == *id))
        .take(MAX_PROGRAM_BUTTONS)
        .collect();
    if chosen.is_empty() {
        programs
            .iter()
            .take(MAX_PROGRAM_BUTTONS)
            .map(|p| p.id)
            .collect()
    } else {
        chosen
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Runtime {
    pub cfg: Config,
    pub machine: Seated,
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
        read(KEY_PROGRAMS).as_deref(),
    );
    with(|rt| rt.cfg = cfg);
}

/// Blocking (SQLite).
pub fn set_config(
    app: &AppHandle,
    enabled: bool,
    minutes: u32,
    program_ids: Option<Vec<u32>>,
) -> Result<(), String> {
    let (minutes, program_ids) = validate(minutes, program_ids)?;
    persistence::write_setting(app, KEY_ENABLED, &enabled.to_string())?;
    persistence::write_setting(app, KEY_MINUTES, &minutes.to_string())?;
    match &program_ids {
        Some(ids) => persistence::write_setting(
            app,
            KEY_PROGRAMS,
            &serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string()),
        )?,
        None => persistence::delete_setting(app, KEY_PROGRAMS)?,
    }
    with(|rt| {
        rt.cfg = Config {
            enabled,
            minutes,
            program_ids,
        };
        if !enabled {
            rt.machine.reset();
        }
    });
    Ok(())
}

/// For the snapshot. No I/O.
pub fn snapshot(programs: &[Program]) -> SeatedSnapshot {
    with(|rt| SeatedSnapshot {
        enabled: rt.cfg.enabled,
        minutes: rt.cfg.minutes,
        program_ids: effective_program_ids(rt.cfg.program_ids.as_deref(), programs),
        custom_programs: rt.cfg.program_ids.is_some(),
        show_card: rt.cfg.enabled && rt.machine.show_card(),
        seated_minutes: rt.machine.seated_minutes(),
        pending_program_id: rt.machine.pending_program_id(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;
    const S: u64 = SECOND;
    const M: u64 = MINUTE;

    fn cfg(minutes: u32) -> Config {
        Config {
            enabled: true,
            minutes,
            program_ids: None,
        }
    }

    /// Typing right now, belt off, treadmill off.
    fn typing() -> Tick {
        Tick {
            idle_ms: Some(0),
            belt_running: false,
            ready: false,
            program_waiting: false,
        }
    }

    /// Runs the machine once a second from `from` (ms after T0) to `to`, with the
    /// input produced by `input(ms)`. Returns the effects seen.
    fn run(
        m: &mut Seated,
        c: &Config,
        from: u64,
        to: u64,
        input: impl Fn(u64) -> Tick,
    ) -> Vec<(u64, Effect)> {
        let mut out = Vec::new();
        let mut t = from;
        while t <= to {
            if let Some(e) = m.tick(c, T0 + t, &input(t)) {
                out.push((t, e));
            }
            t += S;
        }
        out
    }

    #[test]
    fn card_appears_after_the_set_time_of_pc_use() {
        let c = cfg(60);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 59 * M, |_| typing());
        assert!(!m.show_card());
        assert_eq!(m.seated_minutes(), 59);
        run(&mut m, &c, 59 * M + S, 60 * M + 5 * S, |_| typing());
        assert!(m.show_card());
        assert_eq!(m.seated_minutes(), 60);
    }

    #[test]
    fn a_program_waiting_for_go_counts_as_seated_but_holds_the_card_back() {
        let c = cfg(60);
        let mut m = Seated::default();
        let waiting = |_| Tick {
            program_waiting: true,
            ..typing()
        };
        run(&mut m, &c, 0, 61 * M, waiting);
        assert_eq!(m.seated_minutes(), 61, "not walking: the time counts");
        assert!(!m.show_card(), "the HUD is asking for GO instead");
        // Ended without walking: the card comes up at once.
        run(&mut m, &c, 61 * M + S, 61 * M + 2 * S, |_| typing());
        assert!(m.show_card());
        // A program that starts waiting while the card is up hides it.
        run(&mut m, &c, 61 * M + 3 * S, 61 * M + 3 * S, waiting);
        assert!(!m.show_card());
    }

    #[test]
    fn a_shorter_setting_triggers_sooner() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 15 * M + 2 * S, |_| typing());
        assert!(m.show_card());
    }

    #[test]
    fn time_only_builds_while_there_was_input_in_the_last_five_minutes() {
        let c = cfg(60);
        let mut m = Seated::default();
        // Input stops at 10 min; the next 5 minutes still count, then it stops.
        let input = |t: u64| Tick {
            idle_ms: Some(t.saturating_sub(10 * M)),
            ..typing()
        };
        run(&mut m, &c, 0, 14 * M, input);
        // Idle 4 minutes at the end: still counted up to the 5-minute mark (t = 15 min).
        assert_eq!(m.seated_minutes(), 14);
        run(&mut m, &c, 14 * M + S, 24 * M, input);
        // From 15 to 25 minutes idle is over 5 minutes but under 15: held, not counted.
        assert_eq!(m.seated_minutes(), 15);
        assert!(!m.show_card());
    }

    #[test]
    fn no_input_seen_yet_counts_nothing() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 30 * M, |_| Tick {
            idle_ms: None,
            ..typing()
        });
        assert_eq!(m.seated_minutes(), 0);
        assert!(!m.show_card());
    }

    #[test]
    fn belt_running_does_not_accumulate_and_hides_the_card() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        assert!(m.show_card());
        // Walking for a minute: card hidden, nothing added, not yet a reset.
        let walking = |_| Tick {
            belt_running: true,
            ..typing()
        };
        run(&mut m, &c, 16 * M + S, 17 * M, walking);
        assert!(!m.show_card());
        assert_eq!(m.seated_minutes(), 16);
        // Belt stops after a minute: the card comes back, the time was kept.
        run(&mut m, &c, 17 * M + S, 17 * M + 3 * S, |_| typing());
        assert!(m.show_card());
    }

    #[test]
    fn walking_two_minutes_resets_the_timer() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        assert!(m.show_card());
        run(&mut m, &c, 16 * M + S, 18 * M + 2 * S, |_| Tick {
            belt_running: true,
            ..typing()
        });
        assert_eq!(m.seated_minutes(), 0);
        assert!(!m.show_card());
        // Sitting again starts from zero.
        run(&mut m, &c, 18 * M + 3 * S, 30 * M, |_| typing());
        assert_eq!(m.seated_minutes(), 11);
        assert!(!m.show_card());
    }

    #[test]
    fn a_stop_start_walk_under_two_minutes_at_a_time_does_not_reset() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 10 * M, |_| typing());
        // 90 s walking, 30 s sitting, repeated: the walking run never reaches 2 minutes.
        for i in 0..4u64 {
            let base = 10 * M + S + i * 2 * M;
            run(&mut m, &c, base, base + 89 * S, |_| Tick {
                belt_running: true,
                ..typing()
            });
            run(&mut m, &c, base + 90 * S, base + 119 * S, |_| typing());
        }
        assert!(m.seated_minutes() >= 10);
    }

    #[test]
    fn being_idle_for_fifteen_minutes_resets() {
        let c = cfg(30);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 20 * M, |_| typing());
        assert_eq!(m.seated_minutes(), 20);
        // Walk away: idle grows from 0 at 20 min.
        run(&mut m, &c, 20 * M + S, 36 * M, |t| Tick {
            idle_ms: Some(t - 20 * M),
            ..typing()
        });
        assert_eq!(m.seated_minutes(), 0);
        assert!(!m.show_card());
    }

    #[test]
    fn a_sleep_gap_counts_as_away() {
        let c = cfg(30);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 25 * M, |_| typing());
        assert_eq!(m.seated_minutes(), 25);
        // The PC sleeps for an hour, and a key press wakes it: idle is 0 again.
        assert_eq!(m.tick(&c, T0 + 85 * M, &typing()), None);
        assert_eq!(m.seated_minutes(), 0);
        // A short stall (30 s) is skipped without resetting.
        run(&mut m, &c, 85 * M + S, 90 * M, |_| typing());
        let before = m.seated_minutes();
        m.tick(&c, T0 + 90 * M + 30 * S, &typing());
        assert_eq!(m.seated_minutes(), before);
    }

    #[test]
    fn disabled_never_shows_and_counts_nothing() {
        let c = Config {
            enabled: false,
            ..cfg(15)
        };
        let mut m = Seated::default();
        run(&mut m, &c, 0, 60 * M, |_| typing());
        assert!(!m.show_card());
        assert_eq!(m.seated_minutes(), 0);
    }

    #[test]
    fn snooze_hides_for_fifteen_minutes_then_returns() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        assert!(m.show_card());
        m.snooze(T0 + 16 * M);
        assert!(!m.show_card());
        run(&mut m, &c, 16 * M + S, 31 * M - S, |_| typing());
        assert!(!m.show_card(), "still snoozed");
        run(&mut m, &c, 31 * M, 31 * M + 2 * S, |_| typing());
        assert!(m.show_card(), "snooze over");
        assert_eq!(m.seated_minutes(), 31);
    }

    #[test]
    fn walking_cancels_a_snooze() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        m.snooze(T0 + 16 * M);
        run(&mut m, &c, 16 * M + S, 19 * M, |_| Tick {
            belt_running: true,
            ..typing()
        });
        // The timer restarted, so the card is not owed for another 15 minutes.
        run(&mut m, &c, 19 * M + S, 33 * M, |_| typing());
        assert!(!m.show_card());
        run(&mut m, &c, 33 * M + S, 35 * M, |_| typing());
        assert!(m.show_card());
    }

    #[test]
    fn snooze_keeps_the_time_seated() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 20 * M, |_| typing());
        m.snooze(T0 + 20 * M);
        // The count goes on under the snooze: the card says how long, truthfully.
        run(&mut m, &c, 20 * M + S, 34 * M, |_| typing());
        assert!(!m.show_card());
        assert_eq!(m.seated_minutes(), 34);
    }

    #[test]
    fn turning_the_nudge_off_resets_the_timer() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        m.press(3, false, T0 + 16 * M);
        m.reset();
        assert!(!m.show_card());
        assert_eq!(m.seated_minutes(), 0);
        assert_eq!(m.pending_program_id(), None);
    }

    #[test]
    fn engage_hides_the_card_like_a_program_start() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        m.engaged(T0 + 16 * M);
        assert!(!m.show_card());
        // The belt takes a moment to report running: the card stays away.
        let fx = run(&mut m, &c, 16 * M + S, 17 * M + 30 * S, |_| Tick {
            ready: true,
            ..typing()
        });
        assert!(fx.is_empty());
        assert!(!m.show_card());
    }

    #[test]
    fn engage_is_never_remembered_and_drops_a_remembered_program() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        m.press(3, false, T0 + 16 * M);
        m.engaged(T0 + 16 * M + S);
        assert_eq!(m.pending_program_id(), None);
        // The treadmill connecting later starts nothing.
        let fx = run(&mut m, &c, 16 * M + 2 * S, 20 * M, |_| Tick {
            ready: true,
            ..typing()
        });
        assert!(fx.is_empty());
    }

    #[test]
    fn nothing_starts_without_a_press() {
        // Hours of sitting with the card up and the treadmill ready: the
        // machine only ever shows the card.
        let c = cfg(15);
        let mut m = Seated::default();
        let fx = run(&mut m, &c, 0, 3 * 60 * M, |_| Tick {
            ready: true,
            ..typing()
        });
        assert!(fx.is_empty());
        assert!(m.show_card());
        assert_eq!(m.pending_program_id(), None);
    }

    #[test]
    fn raising_the_time_while_showing_hides_the_card() {
        let mut m = Seated::default();
        run(&mut m, &cfg(15), 0, 16 * M, |_| typing());
        assert!(m.show_card());
        run(&mut m, &cfg(60), 16 * M + S, 16 * M + 3 * S, |_| typing());
        assert!(!m.show_card());
    }

    #[test]
    fn pressing_a_program_with_the_treadmill_ready_starts_it_now() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        let e = m.press(7, true, T0 + 16 * M);
        assert_eq!(e, Some(Effect::StartProgram(7)));
        assert!(!m.show_card());
        assert_eq!(m.pending_program_id(), None);
        // The belt takes a minute to show as running: the card stays away.
        run(&mut m, &c, 16 * M + S, 17 * M + 30 * S, |_| typing());
        assert!(!m.show_card());
    }

    #[test]
    fn pressing_with_the_treadmill_off_remembers_and_starts_on_connect() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        assert_eq!(m.press(3, false, T0 + 16 * M), None);
        assert_eq!(m.pending_program_id(), Some(3));
        assert!(
            m.show_card(),
            "the card stays, saying to switch the treadmill on"
        );
        // Five minutes later the treadmill connects.
        let fx = run(&mut m, &c, 16 * M + S, 21 * M, |t| Tick {
            ready: t >= 21 * M,
            ..typing()
        });
        assert_eq!(fx, vec![(21 * M, Effect::StartProgram(3))]);
        assert_eq!(m.pending_program_id(), None);
        assert!(!m.show_card());
    }

    #[test]
    fn a_remembered_program_expires_after_fifteen_minutes() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        m.press(3, false, T0 + 16 * M);
        // Exactly 15 minutes later it is still valid.
        let fx = run(&mut m, &c, 16 * M + S, 31 * M, |t| Tick {
            ready: t == 31 * M,
            ..typing()
        });
        assert_eq!(fx, vec![(31 * M, Effect::StartProgram(3))]);

        // One second past: dropped, nothing starts, the card is back to its buttons.
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        m.press(3, false, T0 + 16 * M);
        let fx = run(&mut m, &c, 16 * M + S, 31 * M + S, |t| Tick {
            ready: t > 31 * M,
            ..typing()
        });
        assert!(fx.is_empty());
        assert_eq!(m.pending_program_id(), None);
        assert!(m.show_card());
    }

    #[test]
    fn a_remembered_program_is_dropped_if_the_belt_is_already_running() {
        let c = cfg(15);
        let mut m = Seated::default();
        run(&mut m, &c, 0, 16 * M, |_| typing());
        m.press(3, false, T0 + 16 * M);
        let fx = run(&mut m, &c, 16 * M + S, 17 * M, |_| Tick {
            ready: true,
            belt_running: true,
            ..typing()
        });
        assert!(fx.is_empty());
        assert_eq!(m.pending_program_id(), None);
    }

    #[test]
    fn a_new_press_replaces_the_remembered_program() {
        let mut m = Seated::default();
        m.press(1, false, T0);
        m.press(2, false, T0 + S);
        assert_eq!(m.pending_program_id(), Some(2));
    }

    #[test]
    fn snooze_forgets_a_remembered_program() {
        let mut m = Seated::default();
        m.press(1, false, T0);
        m.snooze(T0 + S);
        assert_eq!(m.pending_program_id(), None);
    }

    #[test]
    fn a_remembered_program_starts_even_if_the_nudge_was_turned_off() {
        let c = Config {
            enabled: false,
            ..cfg(15)
        };
        let mut m = Seated::default();
        m.press(4, false, T0);
        let fx = run(&mut m, &c, S, 2 * S, |_| Tick {
            ready: true,
            ..typing()
        });
        assert_eq!(fx, vec![(S, Effect::StartProgram(4))]);
    }

    // ---- settings ----

    #[test]
    fn settings_default_to_on_sixty_minutes_first_three() {
        let c = config_from_raw(None, None, None);
        assert_eq!(c, Config::default());
        assert!(c.enabled);
        assert_eq!(c.minutes, 60);
        assert_eq!(c.program_ids, None);
    }

    #[test]
    fn settings_parse_and_bad_values_fall_back() {
        let c = config_from_raw(Some("false"), Some("90"), Some("[3,1,2]"));
        assert_eq!(
            c,
            Config {
                enabled: false,
                minutes: 90,
                program_ids: Some(vec![3, 1, 2])
            }
        );
        let c = config_from_raw(Some("maybe"), Some("5"), Some("not json"));
        assert_eq!(c, Config::default());
        assert_eq!(config_from_raw(None, Some("241"), None).minutes, 60);
        assert_eq!(config_from_raw(None, Some("\"45\""), None).minutes, 45);
        // Duplicates and extras are trimmed; an empty list means the default.
        assert_eq!(
            config_from_raw(None, None, Some("[2,2,5,6,7]")).program_ids,
            Some(vec![2, 5, 6])
        );
        assert_eq!(config_from_raw(None, None, Some("[]")).program_ids, None);
    }

    #[test]
    fn validation() {
        assert_eq!(validate(60, None), Ok((60, None)));
        assert_eq!(
            validate(15, Some(vec![1, 2, 3])),
            Ok((15, Some(vec![1, 2, 3])))
        );
        assert_eq!(validate(240, Some(vec![])), Ok((240, None)));
        assert_eq!(
            validate(60, Some(vec![1, 1, 2])),
            Ok((60, Some(vec![1, 2])))
        );
        assert!(validate(14, None).is_err());
        assert!(validate(241, None).is_err());
        assert!(validate(60, Some(vec![1, 2, 3, 4])).is_err());
    }

    fn prog(id: u32) -> Program {
        Program {
            id,
            name: format!("P{id}"),
            steps: Vec::new(),
            repeat_every_minutes: None,
        }
    }

    #[test]
    fn card_programs_default_to_the_first_three_saved() {
        let programs: Vec<Program> = [4, 5, 6, 7].iter().map(|i| prog(*i)).collect();
        assert_eq!(effective_program_ids(None, &programs), vec![4, 5, 6]);
        assert_eq!(effective_program_ids(None, &programs[..2]), vec![4, 5]);
        assert!(effective_program_ids(None, &[]).is_empty());
    }

    #[test]
    fn card_programs_skip_deleted_ones_and_fall_back_when_none_remain() {
        let programs: Vec<Program> = [4, 5, 6, 7].iter().map(|i| prog(*i)).collect();
        assert_eq!(
            effective_program_ids(Some(&[7, 99, 5]), &programs),
            vec![7, 5]
        );
        assert_eq!(effective_program_ids(Some(&[99]), &programs), vec![4, 5, 6]);
        assert_eq!(
            effective_program_ids(Some(&[7, 6, 5, 4]), &programs),
            vec![7, 6, 5]
        );
    }
}
