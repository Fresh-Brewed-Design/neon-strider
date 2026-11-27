//! Walk-away safety.
//!
//! While the belt is walking, leaving the PC pauses it:
//!
//! - **Lock:** locking Windows (Win+L) pauses at once.
//! - **Idle:** after N minutes without keyboard or mouse input, the HUD warns
//!   (`WALK-AWAY // PAUSING IN 30s`, with STAY); any input or STAY cancels it,
//!   and when the 30 seconds run out the belt pauses. The timeout is long and
//!   the warning visible because watching a video while walking is normal.
//! - **Steps** (Labs, off by default, `labs_walkaway_steps`): the deck's own
//!   step counter (bytes 11 and 12 of its 0x51 status frame, never the steps
//!   the app works out from belt distance and stride) counts as presence once
//!   it is seen moving while the belt walks (`StepCounter`). A step counts as
//!   input, and with the counter live, no step and no input for
//!   `STEPS_AWAY_MS` starts the same warning, so an empty belt pauses about a
//!   minute after the last step. A deck that sends no count, or one that
//!   never moves, leaves walk-away as it was. Whether a deck counts feet or
//!   works its count out from the belt is only known on the hardware, hence
//!   the switch; `STEPS_HOLD_MAX_MS` keeps the idle warning in reach either way.
//!
//! The treadmill's pause is used, never stop, so the session (and a running
//! program) carries on after RESUME. Nothing here ever resumes the belt by
//! itself: after a pause the HUD shows `WALK-AWAY // BELT PAUSED` with RESUME
//! and END until the user picks one (or starts the belt some other way).
//!
//! Two parts: `WalkAway`, the pure decision logic driven with injected epoch-ms
//! times, and the settings and snapshot. The session lock comes from
//! `platform::session_locked`.
//!
//! `desk.rs` runs the machine once a second, using the same last-input time as
//! the sit-too-long nudge (`hooks::input_idle_ms`).
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

// Settings keys (raw values in the `settings` table).
const KEY_LOCK_ENABLED: &str = "walkaway_lock_enabled";
const KEY_IDLE_ENABLED: &str = "walkaway_idle_enabled";
const KEY_IDLE_MINUTES: &str = "walkaway_idle_minutes";
/// Labs: the deck's step count as presence. Off by default.
const KEY_STEPS_PRESENCE: &str = "labs_walkaway_steps";

/// Defaults and range: docs/research.md, Walk-away timings. DeskSection reads
/// the range from `SettingLimits`.
const DEFAULT_IDLE_MINUTES: u32 = 10;
pub(crate) const MIN_IDLE_MINUTES: u32 = 2;
pub(crate) const MAX_IDLE_MINUTES: u32 = 30;

const SECOND: u64 = 1000;
const MINUTE: u64 = 60 * SECOND;
/// How long the idle warning shows before the belt pauses.
/// Time to notice the card and touch the mouse (docs/research.md). DeskSection's text quotes it.
const WARNING_MS: u64 = 30 * SECOND;
/// The deck takes a moment to report the pause. If it still reports the belt
/// walking this long after a pause was sent, the pause is sent again.
/// As for eye breaks: about eight polls.
const REISSUE_MS: u64 = 5 * SECOND;

/// The deck's step counter counts as a sensor once it has moved this many
/// times while the belt walked on this connection. One move can be the count
/// resetting as a walk starts; three moves while walking is a counter that
/// counts. A deck that always sends 0 (or one fixed value) never gets there,
/// so its walk-away stays as it was.
const STEPS_LIVE_MOVES: u32 = 3;
/// With a live counter, no step and no keyboard or mouse input for this long
/// while the belt walks starts the warning. A slow desk walk (about 1 km/h)
/// still takes a step every second or two and the deck reports about twice a
/// second, so 45 s without one is feet off the belt, not a slow stride. With
/// the 30 s warning the belt pauses about 75 s after the last step instead of
/// after the idle minutes. The HUD and Labs text quote it.
pub(crate) const STEPS_AWAY_MS: u64 = 45 * SECOND;
/// Steps alone hold the idle warning off for at most this long after the
/// last keyboard or mouse input, STAY or the belt starting. If the deck's
/// counter turns out to follow the belt instead of feet, an empty belt still
/// warns within the hour. An hour covers a long video walked without
/// touching the PC, the case the switch is for.
const STEPS_HOLD_MAX_MS: u64 = 60 * MINUTE;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub lock_enabled: bool,
    pub idle_enabled: bool,
    pub idle_minutes: u32,
    /// Labs: the deck's step count as presence (needs `idle_enabled`).
    pub steps_presence: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lock_enabled: true,
            idle_enabled: true,
            idle_minutes: DEFAULT_IDLE_MINUTES,
            steps_presence: false,
        }
    }
}

/// Why the belt was paused, or why the warning is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum WalkAwayReason {
    Lock,
    Idle,
    /// Labs: the deck's live step count stopped, with no input either.
    Steps,
}

/// Which HUD card shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum WalkAwayCard {
    #[default]
    None,
    /// `WALK-AWAY // PAUSING IN 30s` with STAY.
    Warning,
    /// `WALK-AWAY // BELT PAUSED` with RESUME and END.
    Paused,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct WalkAwaySnapshot {
    pub lock_enabled: bool,
    pub idle_enabled: bool,
    /// Minutes without input before the warning, 2 to 30.
    pub idle_minutes: u32,
    pub card: WalkAwayCard,
    /// While `card` is `warning`: whole seconds left before the pause.
    pub pause_in_s: u32,
    /// While `card` is `warning` or `paused`: why.
    pub reason: Option<WalkAwayReason>,
    /// Labs: the deck's step count counts as presence.
    pub steps_presence: bool,
    /// With `steps_presence` and a live count: seconds with no step and no
    /// input before the warning (`STEPS_AWAY_MS`).
    pub steps_away_s: u32,
    /// The deck's step counter as walk-away sees it, for Diagnostics.
    pub deck_steps: DeckStepsReading,
}

/// The deck's own step counter on this connection, switch on or off.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DeckStepsReading {
    /// The count in the deck's latest status frame; none with no fresh frame
    /// on this connection.
    pub count: Option<u32>,
    /// Times the count moved while the belt walked, on this connection.
    pub moves: u32,
    /// The count has moved enough to count as a sensor (`STEPS_LIVE_MOVES`).
    pub live: bool,
    /// While the belt walks: whole seconds since the count last moved (or
    /// since the belt started, if later).
    pub still_s: Option<u32>,
}

/// What the desk loop tells the machine each second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Input {
    /// The treadmill link is up.
    pub connected: bool,
    /// The belt is walking and may be paused: running, no stop in flight, and
    /// either manual or a program's walking phase that is not paused already.
    pub walking: bool,
    /// The user's session is locked.
    pub locked: bool,
    /// Time since the last keyboard or mouse input; `None` if none was seen yet.
    pub idle_ms: Option<u64>,
    /// A call is going on (an app holds the microphone). People talk on calls
    /// without touching the PC, so the idle timer does not run.
    pub call_active: bool,
    /// The step count in the deck's latest status frame on this connection,
    /// raw; `None` with no fresh frame. Never the app's own derived steps.
    pub deck_steps: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Pause the belt (the treadmill's pause, or the program's pause).
    Pause(WalkAwayReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Phase {
    #[default]
    Watching,
    /// The idle warning is up; the belt pauses at `pause_at_ms`.
    Warning {
        pause_at_ms: u64,
        reason: WalkAwayReason,
    },
    Paused {
        reason: WalkAwayReason,
        /// When the pause was (last) sent.
        sent_ms: u64,
        /// The deck has since reported the belt not walking.
        took: bool,
        /// Where the walk-away began: the last sign of the user (input, STAY
        /// or the belt starting) for an idle pause, the pause itself for a
        /// lock. The belt ran empty from here to `paused_at_ms`.
        away_since_ms: u64,
        /// When the pause was first sent.
        paused_at_ms: u64,
    },
}

// ---------------------------------------------------------------------------
// The state machine
// ---------------------------------------------------------------------------

/// The deck's step counter on this connection: its last value, when it last
/// moved and how often it moved while the belt walked.
#[derive(Debug, Default, Clone, Copy)]
struct StepCounter {
    last: Option<u16>,
    moved_ms: Option<u64>,
    moves: u32,
}

impl StepCounter {
    /// One reading. Any change counts as a move (a reset or a wrap at 65535
    /// is the counter working too); only moves while walking make it live.
    fn observe(&mut self, now_ms: u64, count: Option<u16>, walking: bool) {
        let Some(count) = count else { return };
        if self.last.is_some_and(|prev| prev != count) {
            self.moved_ms = Some(now_ms);
            if walking {
                self.moves = self.moves.saturating_add(1);
            }
        }
        self.last = Some(count);
    }

    fn live(&self) -> bool {
        self.moves >= STEPS_LIVE_MOVES
    }
}

#[derive(Debug, Default)]
pub struct WalkAway {
    phase: Phase,
    /// When the belt started walking (this stretch).
    walking_since_ms: Option<u64>,
    /// STAY or RESUME: counts as input.
    stay_ms: Option<u64>,
    /// The belt was started again (at the treadmill, say) while the PC was
    /// still locked from the lock that paused it: that lock has had its say,
    /// so it doesn't pause the belt again. Cleared on unlock.
    lock_spent: bool,
    /// The deck's step counter, watched whether or not the switch is on.
    steps: StepCounter,
    /// This tick's step count was fresh.
    steps_fresh: bool,
    /// This tick the belt walked on a ready link.
    walking_now: bool,
}

impl WalkAway {
    fn pause(&mut self, reason: WalkAwayReason, now_ms: u64, away_since_ms: u64) -> Option<Effect> {
        self.phase = Phase::Paused {
            reason,
            sent_ms: now_ms,
            took: false,
            away_since_ms: away_since_ms.min(now_ms),
            paused_at_ms: now_ms,
        };
        Some(Effect::Pause(reason))
    }

    /// Advance to `now_ms`. Returns a pause to apply, if any. Never resumes.
    pub fn tick(&mut self, cfg: &Config, now_ms: u64, input: &Input) -> Option<Effect> {
        if !input.locked {
            self.lock_spent = false;
        }
        if input.connected {
            self.steps.observe(now_ms, input.deck_steps, input.walking);
        } else {
            // A new connection may be another deck: watch it afresh.
            self.steps = StepCounter::default();
        }
        self.steps_fresh = input.connected && input.deck_steps.is_some();
        self.walking_now = input.connected && input.walking;
        if let Phase::Paused {
            reason,
            sent_ms,
            took,
            away_since_ms,
            paused_at_ms,
        } = self.phase
        {
            if !input.connected {
                // The treadmill went away (it powers itself off when idle):
                // there is nothing left to resume.
                self.phase = Phase::Watching;
                self.walking_since_ms = None;
                return None;
            }
            if !input.walking {
                self.phase = Phase::Paused {
                    reason,
                    sent_ms,
                    took: true,
                    away_since_ms,
                    paused_at_ms,
                };
                return None;
            }
            if took {
                // Walking again after the pause took: the user started the belt
                // some other way (at the treadmill, while the PC is still
                // locked, say). The card has done its job, and the person is
                // there: the same lock doesn't pause it again.
                self.phase = Phase::Watching;
                self.walking_since_ms = Some(now_ms);
                self.lock_spent = input.locked;
                return None;
            }
            if now_ms.saturating_sub(sent_ms) >= REISSUE_MS {
                log::info!("Walk-away: the belt is still moving; sending the pause again");
                self.phase = Phase::Paused {
                    reason,
                    sent_ms: now_ms,
                    took: false,
                    away_since_ms,
                    paused_at_ms,
                };
                return Some(Effect::Pause(reason));
            }
            return None;
        }

        if !input.connected || !input.walking {
            self.phase = Phase::Watching;
            self.walking_since_ms = None;
            return None;
        }
        let walking_since = *self.walking_since_ms.get_or_insert(now_ms);

        if cfg.lock_enabled && input.locked && !self.lock_spent {
            return self.pause(WalkAwayReason::Lock, now_ms, now_ms);
        }

        if !cfg.idle_enabled || input.call_active {
            self.phase = Phase::Watching;
            return None;
        }

        // Idle since the latest of: the last input, STAY, the belt starting.
        // `None` (no input seen since launch) counts from the belt starting.
        let last_input = input.idle_ms.map(|i| now_ms.saturating_sub(i));
        let human_at = [last_input, self.stay_ms, Some(walking_since)]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(now_ms);
        // Labs: a live deck step count is presence too, and its stopping
        // warns sooner; within `STEPS_HOLD_MAX_MS` of the last input only.
        let steps_watch = cfg.steps_presence && self.steps_fresh && self.steps.live();
        let hold_end = human_at + STEPS_HOLD_MAX_MS;
        let (active_at, warn_at, reason) = if steps_watch && now_ms < hold_end {
            let at = human_at.max(self.steps.moved_ms.unwrap_or(0));
            (at, at + STEPS_AWAY_MS, WalkAwayReason::Steps)
        } else {
            let idle_warn = human_at + cfg.idle_minutes as u64 * MINUTE;
            // Steps held the warning until the hour ran out: warn now, with
            // the full 30 s, never a pause out of the blue.
            let warn_at = if steps_watch {
                idle_warn.max(hold_end)
            } else {
                idle_warn
            };
            (human_at, warn_at, WalkAwayReason::Idle)
        };
        let pause_at = warn_at + WARNING_MS;
        if now_ms >= pause_at {
            return self.pause(reason, now_ms, active_at);
        }
        self.phase = if now_ms >= warn_at {
            Phase::Warning {
                pause_at_ms: pause_at,
                reason,
            }
        } else {
            Phase::Watching
        };
        None
    }

    /// STAY on the warning card.
    pub fn stay(&mut self, now_ms: u64) {
        self.stay_ms = Some(now_ms);
        if matches!(self.phase, Phase::Warning { .. }) {
            self.phase = Phase::Watching;
        }
    }

    /// RESUME or END on the paused card: the card goes. The caller restarts or
    /// stops the belt. Returns whether the card was showing.
    pub fn acknowledge(&mut self, now_ms: u64) -> bool {
        let was = matches!(self.phase, Phase::Paused { .. });
        self.phase = Phase::Watching;
        self.stay_ms = Some(now_ms);
        was
    }

    /// The settings changed: a warning the new settings no longer allow goes.
    /// A paused card stays (the belt is still paused).
    pub fn reconfigure(&mut self, cfg: &Config) {
        let stale = match self.phase {
            Phase::Warning { reason, .. } => {
                !cfg.idle_enabled || (reason == WalkAwayReason::Steps && !cfg.steps_presence)
            }
            _ => false,
        };
        if stale {
            self.phase = Phase::Watching;
        }
    }

    pub fn card(&self) -> WalkAwayCard {
        match self.phase {
            Phase::Watching => WalkAwayCard::None,
            Phase::Warning { .. } => WalkAwayCard::Warning,
            Phase::Paused { .. } => WalkAwayCard::Paused,
        }
    }

    /// Whole seconds until the pause while warning (rounded up), else 0.
    pub fn pause_in_s(&self, now_ms: u64) -> u32 {
        match self.phase {
            Phase::Warning { pause_at_ms, .. } => {
                pause_at_ms.saturating_sub(now_ms).div_ceil(SECOND) as u32
            }
            _ => 0,
        }
    }

    /// While the paused card shows: the stretch the belt ran with nobody on
    /// it, from where the walk-away began to the pause (epoch ms). DUMP cuts
    /// the walk there. None for a lock (it pauses at once) or no card.
    pub fn empty_stretch(&self) -> Option<(u64, u64)> {
        match self.phase {
            Phase::Paused {
                away_since_ms,
                paused_at_ms,
                ..
            } if away_since_ms < paused_at_ms => Some((away_since_ms, paused_at_ms)),
            _ => None,
        }
    }

    pub fn reason(&self) -> Option<WalkAwayReason> {
        match self.phase {
            Phase::Paused { reason, .. } | Phase::Warning { reason, .. } => Some(reason),
            Phase::Watching => None,
        }
    }

    /// The deck's step counter for Diagnostics.
    pub fn deck_steps(&self, now_ms: u64) -> DeckStepsReading {
        let s = &self.steps;
        DeckStepsReading {
            count: s.last.filter(|_| self.steps_fresh).map(u32::from),
            moves: s.moves,
            live: s.live(),
            still_s: self
                .walking_since_ms
                .filter(|_| self.walking_now)
                .map(|since| {
                    let from = s.moved_ms.unwrap_or(since).max(since);
                    (now_ms.saturating_sub(from) / SECOND) as u32
                }),
        }
    }
}

// ---------------------------------------------------------------------------
// Settings and shared state
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Runtime {
    pub cfg: Config,
    pub machine: WalkAway,
}

static STATE: Mutex<Option<Runtime>> = Mutex::new(None);

/// Run `f` on the shared config and machine.
pub fn with<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Runtime::default))
}

/// Build the config from raw stored values. Bad values fall back to the
/// defaults (both on, 10 minutes).
/// The steps switch (`steps`) is off unless saved on.
pub fn config_from_raw(
    lock: Option<&str>,
    idle: Option<&str>,
    minutes: Option<&str>,
    steps: Option<&str>,
) -> Config {
    Config {
        lock_enabled: persistence::parse_setting(lock).unwrap_or(true),
        idle_enabled: persistence::parse_setting(idle).unwrap_or(true),
        idle_minutes: persistence::setting_in(
            minutes,
            MIN_IDLE_MINUTES..=MAX_IDLE_MINUTES,
            DEFAULT_IDLE_MINUTES,
        ),
        steps_presence: persistence::parse_setting(steps).unwrap_or(false),
    }
}

/// 2 to 30 minutes.
fn validate_minutes(minutes: u32) -> Result<u32, String> {
    if !(MIN_IDLE_MINUTES..=MAX_IDLE_MINUTES).contains(&minutes) {
        return Err(format!(
            "Idle time must be between {} and {} minutes",
            MIN_IDLE_MINUTES, MAX_IDLE_MINUTES
        ));
    }
    Ok(minutes)
}

/// Load the settings. Blocking; call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let read = |k: &str| persistence::read_setting(app, k);
    let cfg = config_from_raw(
        read(KEY_LOCK_ENABLED).as_deref(),
        read(KEY_IDLE_ENABLED).as_deref(),
        read(KEY_IDLE_MINUTES).as_deref(),
        read(KEY_STEPS_PRESENCE).as_deref(),
    );
    with(|rt| rt.cfg = cfg);
}

/// Blocking (SQLite).
pub fn set_config(
    app: &AppHandle,
    lock_enabled: bool,
    idle_enabled: bool,
    idle_minutes: u32,
) -> Result<(), String> {
    let idle_minutes = validate_minutes(idle_minutes)?;
    persistence::write_setting(app, KEY_LOCK_ENABLED, &lock_enabled.to_string())?;
    persistence::write_setting(app, KEY_IDLE_ENABLED, &idle_enabled.to_string())?;
    persistence::write_setting(app, KEY_IDLE_MINUTES, &idle_minutes.to_string())?;
    with(|rt| {
        rt.cfg = Config {
            lock_enabled,
            idle_enabled,
            idle_minutes,
            ..rt.cfg
        };
        let cfg = rt.cfg;
        rt.machine.reconfigure(&cfg);
    });
    Ok(())
}

/// Labs: the deck's step count as presence on or off. Blocking (SQLite).
pub fn set_steps_presence(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_STEPS_PRESENCE, &on.to_string())?;
    with(|rt| {
        rt.cfg.steps_presence = on;
        let cfg = rt.cfg;
        rt.machine.reconfigure(&cfg);
    });
    Ok(())
}

/// For the snapshot. No I/O.
pub fn snapshot(now_ms: u64) -> WalkAwaySnapshot {
    with(|rt| WalkAwaySnapshot {
        lock_enabled: rt.cfg.lock_enabled,
        idle_enabled: rt.cfg.idle_enabled,
        idle_minutes: rt.cfg.idle_minutes,
        card: rt.machine.card(),
        pause_in_s: rt.machine.pause_in_s(now_ms),
        reason: rt.machine.reason(),
        steps_presence: rt.cfg.steps_presence,
        steps_away_s: (STEPS_AWAY_MS / SECOND) as u32,
        deck_steps: rt.machine.deck_steps(now_ms),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;

    fn cfg() -> Config {
        Config::default()
    }

    /// Walking, unlocked, input `idle_ms` ago.
    fn walking(idle_ms: u64) -> Input {
        Input {
            connected: true,
            walking: true,
            locked: false,
            idle_ms: Some(idle_ms),
            call_active: false,
            deck_steps: None,
        }
    }

    /// Run one tick per second from `from` (inclusive) to `to` (exclusive),
    /// with the last input at `input_at`. Returns the effects seen.
    fn run(m: &mut WalkAway, c: &Config, from: u64, to: u64, input_at: u64) -> Vec<(u64, Effect)> {
        let mut out = Vec::new();
        let mut t = from;
        while t < to {
            if let Some(e) = m.tick(c, t, &walking(t - input_at)) {
                out.push((t, e));
            }
            t += SECOND;
        }
        out
    }

    #[test]
    fn lock_while_running_pauses_at_once() {
        let mut m = WalkAway::default();
        let c = cfg();
        assert_eq!(m.tick(&c, T0, &walking(0)), None);
        let locked = Input {
            locked: true,
            ..walking(1000)
        };
        assert_eq!(
            m.tick(&c, T0 + 1000, &locked),
            Some(Effect::Pause(WalkAwayReason::Lock))
        );
        assert_eq!(m.card(), WalkAwayCard::Paused);
        assert_eq!(m.reason(), Some(WalkAwayReason::Lock));
        // The deck reports the pause: nothing more, the card stays.
        let still = Input {
            walking: false,
            ..locked
        };
        assert_eq!(m.tick(&c, T0 + 2000, &still), None);
        assert_eq!(m.card(), WalkAwayCard::Paused);
        assert_eq!(
            m.empty_stretch(),
            None,
            "a lock pauses at once: nothing ran empty"
        );
    }

    #[test]
    fn lock_while_stopped_or_paused_does_nothing() {
        let mut m = WalkAway::default();
        let c = cfg();
        let stopped = Input {
            walking: false,
            locked: true,
            ..walking(0)
        };
        for i in 0..10 {
            assert_eq!(m.tick(&c, T0 + i * SECOND, &stopped), None);
        }
        assert_eq!(m.card(), WalkAwayCard::None);
        // Treadmill offline and locked: nothing either.
        let offline = Input {
            connected: false,
            ..stopped
        };
        assert_eq!(m.tick(&c, T0 + 20 * SECOND, &offline), None);
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    #[test]
    fn idle_warning_then_input_cancels() {
        let mut m = WalkAway::default();
        let c = cfg();
        let start = T0;
        // No input for 10 minutes: no pause yet, the warning is up.
        assert!(run(&mut m, &c, start, start + 10 * MINUTE + SECOND, start).is_empty());
        assert_eq!(m.card(), WalkAwayCard::Warning);
        assert_eq!(m.pause_in_s(start + 10 * MINUTE), 30);
        assert_eq!(m.pause_in_s(start + 10 * MINUTE + 12_500), 18);
        // Input: the warning goes.
        let now = start + 10 * MINUTE + 10 * SECOND;
        assert_eq!(m.tick(&c, now, &walking(0)), None);
        assert_eq!(m.card(), WalkAwayCard::None);
        assert_eq!(m.pause_in_s(now), 0);
    }

    #[test]
    fn idle_warning_then_stay_cancels() {
        let mut m = WalkAway::default();
        let c = cfg();
        run(&mut m, &c, T0, T0 + 10 * MINUTE + 5 * SECOND, T0);
        assert_eq!(m.card(), WalkAwayCard::Warning);
        let now = T0 + 10 * MINUTE + 5 * SECOND;
        m.stay(now);
        assert_eq!(m.card(), WalkAwayCard::None);
        // Still no keyboard or mouse input, but STAY counts as input: the next
        // warning is a full 10 minutes later.
        assert!(run(&mut m, &c, now, now + 10 * MINUTE, T0).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
        run(
            &mut m,
            &c,
            now + 10 * MINUTE,
            now + 10 * MINUTE + SECOND,
            T0,
        );
        assert_eq!(m.card(), WalkAwayCard::Warning);
    }

    #[test]
    fn idle_warning_then_pause() {
        let mut m = WalkAway::default();
        let c = cfg();
        let effects = run(&mut m, &c, T0, T0 + 10 * MINUTE + WARNING_MS + SECOND, T0);
        assert_eq!(
            effects,
            vec![(
                T0 + 10 * MINUTE + WARNING_MS,
                Effect::Pause(WalkAwayReason::Idle)
            )]
        );
        assert_eq!(m.card(), WalkAwayCard::Paused);
        assert_eq!(m.reason(), Some(WalkAwayReason::Idle));
        // The belt ran empty from the last input to the pause; a reissued
        // pause keeps both ends.
        let paused_at = T0 + 10 * MINUTE + WARNING_MS;
        assert_eq!(m.empty_stretch(), Some((T0, paused_at)));
        run(&mut m, &c, paused_at + SECOND, paused_at + 10 * SECOND, T0);
        assert_eq!(m.empty_stretch(), Some((T0, paused_at)));
        m.acknowledge(paused_at + 20 * SECOND);
        assert_eq!(m.empty_stretch(), None, "gone with the card");
    }

    #[test]
    fn idle_counts_from_the_belt_starting() {
        // Input was 20 minutes ago (a program started on its own when the
        // treadmill connected): the timer starts with the walk, not before.
        let mut m = WalkAway::default();
        let c = cfg();
        assert_eq!(m.tick(&c, T0, &walking(20 * MINUTE)), None);
        assert_eq!(m.card(), WalkAwayCard::None);
        assert!(run(&mut m, &c, T0, T0 + 10 * MINUTE, T0 - 20 * MINUTE).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    #[test]
    fn never_auto_resumes() {
        let mut m = WalkAway::default();
        let c = cfg();
        let locked = Input {
            locked: true,
            ..walking(0)
        };
        assert!(m.tick(&c, T0, &locked).is_some());
        let paused = Input {
            walking: false,
            ..walking(0)
        };
        // Unlock, fresh input, hours pass: it only ever stays paused.
        for i in 1..3600 {
            let input = Input {
                locked: i < 10,
                ..paused
            };
            assert_eq!(m.tick(&c, T0 + i * SECOND, &input), None);
        }
        assert_eq!(m.card(), WalkAwayCard::Paused);
        // RESUME clears the card; the caller restarts the belt.
        assert!(m.acknowledge(T0 + 3600 * SECOND));
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    #[test]
    fn a_pause_that_does_not_take_is_sent_again() {
        let mut m = WalkAway::default();
        let c = cfg();
        let locked = Input {
            locked: true,
            ..walking(0)
        };
        assert!(m.tick(&c, T0, &locked).is_some());
        assert_eq!(m.tick(&c, T0 + 2 * SECOND, &locked), None);
        assert_eq!(
            m.tick(&c, T0 + 5 * SECOND, &locked),
            Some(Effect::Pause(WalkAwayReason::Lock))
        );
    }

    #[test]
    fn resumed_elsewhere_clears_the_card() {
        let mut m = WalkAway::default();
        let c = cfg();
        let locked = Input {
            locked: true,
            ..walking(0)
        };
        m.tick(&c, T0, &locked);
        m.tick(
            &c,
            T0 + SECOND,
            &Input {
                walking: false,
                ..walking(0)
            },
        );
        // Started from the dashboard: walking again, unlocked.
        assert_eq!(m.tick(&c, T0 + 60 * SECOND, &walking(0)), None);
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    /// Was `pin_started_at_the_treadmill_while_still_locked_pauses_again`:
    /// the person is at the treadmill, so the lock that paused the belt doesn't
    /// pause it again. The next lock does.
    #[test]
    fn started_at_the_treadmill_while_still_locked_carries_on() {
        let mut m = WalkAway::default();
        let c = cfg();
        let locked = Input {
            locked: true,
            ..walking(0)
        };
        assert!(m.tick(&c, T0, &locked).is_some());
        let paused = Input {
            walking: false,
            ..locked
        };
        assert_eq!(m.tick(&c, T0 + SECOND, &paused), None);
        // The PC is still locked; the user starts the belt at the treadmill.
        for i in 60..600 {
            assert_eq!(m.tick(&c, T0 + i * SECOND, &locked), None, "{i}");
        }
        assert_eq!(m.card(), WalkAwayCard::None);
        // Unlocked, then locked again: that lock pauses as usual.
        assert_eq!(m.tick(&c, T0 + 600 * SECOND, &walking(0)), None);
        assert_eq!(
            m.tick(&c, T0 + 601 * SECOND, &locked),
            Some(Effect::Pause(WalkAwayReason::Lock))
        );
    }

    #[test]
    fn started_at_the_treadmill_after_an_idle_pause_gets_a_full_idle_period() {
        let mut m = WalkAway::default();
        let c = cfg();
        // No input since T0: the idle pause.
        let effects = run(&mut m, &c, T0, T0 + 10 * MINUTE + WARNING_MS + SECOND, T0);
        assert_eq!(effects.len(), 1);
        let at = T0 + 10 * MINUTE + WARNING_MS + SECOND;
        let paused = Input {
            walking: false,
            ..walking(at - T0)
        };
        assert_eq!(m.tick(&c, at, &paused), None);
        // Started at the treadmill, still no keyboard or mouse input: the
        // card goes, and the idle timer counts from the belt starting.
        let start = at + 5 * SECOND;
        assert!(run(&mut m, &c, start, start + 10 * MINUTE, T0).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
        assert!(run(
            &mut m,
            &c,
            start + 10 * MINUTE,
            start + 10 * MINUTE + SECOND,
            T0
        )
        .is_empty());
        assert_eq!(m.card(), WalkAwayCard::Warning);
    }

    #[test]
    fn treadmill_going_offline_clears_the_card() {
        let mut m = WalkAway::default();
        let c = cfg();
        m.tick(
            &c,
            T0,
            &Input {
                locked: true,
                ..walking(0)
            },
        );
        m.tick(
            &c,
            T0 + SECOND,
            &Input {
                connected: false,
                walking: false,
                ..walking(0)
            },
        );
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    #[test]
    fn disabled_settings_do_nothing() {
        let off = Config {
            lock_enabled: false,
            idle_enabled: false,
            idle_minutes: 10,
            steps_presence: false,
        };
        let mut m = WalkAway::default();
        let locked = Input {
            locked: true,
            ..walking(0)
        };
        for i in 0..(40 * 60) {
            assert_eq!(
                m.tick(
                    &off,
                    T0 + i * SECOND,
                    &Input {
                        idle_ms: Some(i * SECOND),
                        ..locked
                    }
                ),
                None
            );
        }
        assert_eq!(m.card(), WalkAwayCard::None);

        // Lock off, idle on: locking alone does nothing; the idle timer still runs.
        let idle_only = Config {
            lock_enabled: false,
            ..cfg()
        };
        let mut m = WalkAway::default();
        assert_eq!(m.tick(&idle_only, T0, &locked), None);
        assert_eq!(m.card(), WalkAwayCard::None);

        // Idle off, lock on: no warning ever, but a lock still pauses.
        let lock_only = Config {
            idle_enabled: false,
            ..cfg()
        };
        let mut m = WalkAway::default();
        assert!(run(&mut m, &lock_only, T0, T0 + 40 * MINUTE, T0).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
        assert!(m.tick(&lock_only, T0 + 40 * MINUTE, &locked).is_some());
    }

    #[test]
    fn turning_idle_off_drops_a_warning() {
        let mut m = WalkAway::default();
        run(&mut m, &cfg(), T0, T0 + 10 * MINUTE + SECOND, T0);
        assert_eq!(m.card(), WalkAwayCard::Warning);
        m.reconfigure(&Config {
            idle_enabled: false,
            ..cfg()
        });
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    #[test]
    fn calls_hold_the_idle_timer() {
        let mut m = WalkAway::default();
        let c = cfg();
        for i in 0..(20 * 60) {
            let t = T0 + i * SECOND;
            let input = Input {
                call_active: true,
                ..walking(t - T0)
            };
            assert_eq!(m.tick(&c, t, &input), None);
        }
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    #[test]
    fn not_walking_never_warns() {
        // Stopped, paused, a stop in flight or a resting program all reach the
        // machine as `walking: false`.
        let mut m = WalkAway::default();
        let c = cfg();
        for i in 0..(20 * 60) {
            let t = T0 + i * SECOND;
            let input = Input {
                walking: false,
                ..walking(t - T0)
            };
            assert_eq!(m.tick(&c, t, &input), None);
        }
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    #[test]
    fn settings_parse_and_validate() {
        assert_eq!(config_from_raw(None, None, None, None), Config::default());
        assert_eq!(
            config_from_raw(Some("false"), Some("\"false\""), Some("25"), Some("true")),
            Config {
                lock_enabled: false,
                idle_enabled: false,
                idle_minutes: 25,
                steps_presence: true,
            }
        );
        assert_eq!(
            config_from_raw(None, None, Some("1"), None).idle_minutes,
            DEFAULT_IDLE_MINUTES
        );
        assert_eq!(
            config_from_raw(None, None, Some("31"), None).idle_minutes,
            DEFAULT_IDLE_MINUTES
        );
        assert!(config_from_raw(Some("junk"), None, None, None).lock_enabled);
        assert!(!config_from_raw(None, None, None, Some("junk")).steps_presence);
        assert!(!Config::default().steps_presence, "off by default");
        assert!(validate_minutes(1).is_err());
        assert!(validate_minutes(31).is_err());
        assert_eq!(validate_minutes(2), Ok(2));
        assert_eq!(validate_minutes(30), Ok(30));
    }

    // ---- Labs: the deck's step count as presence ----

    /// A running 0x51 frame from the deck with `steps` in bytes 11 and 12,
    /// framed and parsed the way `bluetooth.rs` reads one.
    fn frame(steps: u16) -> Vec<u8> {
        let mut body = vec![0x51, 3, 30, 0];
        for v in [600u16, 500, 300, steps] {
            body.extend_from_slice(&v.to_le_bytes());
        }
        body.extend_from_slice(&[0, 0]);
        crate::protocol::raw_packet(&body).unwrap()
    }

    /// A short running frame with no counters, as some decks send.
    fn short_frame() -> Vec<u8> {
        crate::protocol::raw_packet(&[0x51, 3]).unwrap()
    }

    /// The step count walk-away gets from a frame (`None`: no fresh frame).
    fn steps_of(frame: Option<Vec<u8>>) -> Option<u16> {
        frame.map(|f| {
            crate::protocol::parse_status(&f)
                .expect("a status frame")
                .steps
        })
    }

    fn steps_cfg() -> Config {
        Config {
            steps_presence: true,
            ..cfg()
        }
    }

    /// One tick per second from `from` to `to`, the last input at `input_at`
    /// and the deck's frame at each second from `deck`.
    fn run_deck(
        m: &mut WalkAway,
        c: &Config,
        from: u64,
        to: u64,
        input_at: u64,
        deck: impl Fn(u64) -> Option<Vec<u8>>,
    ) -> Vec<(u64, Effect)> {
        let mut out = Vec::new();
        let mut t = from;
        while t < to {
            let input = Input {
                deck_steps: steps_of(deck(t)),
                ..walking(t - input_at)
            };
            if let Some(e) = m.tick(c, t, &input) {
                out.push((t, e));
            }
            t += SECOND;
        }
        out
    }

    /// A walker's steps: about one a second from `start`.
    fn stepping(start: u64) -> impl Fn(u64) -> Option<Vec<u8>> {
        move |t| Some(frame(((t - start) / SECOND) as u16))
    }

    #[test]
    fn advancing_steps_hold_the_warning_off_through_a_video_walk() {
        // 40 minutes of a video walk: no keyboard or mouse, feet moving.
        let mut m = WalkAway::default();
        let c = steps_cfg();
        assert!(run_deck(&mut m, &c, T0, T0 + 40 * MINUTE, T0, stepping(T0)).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
        let r = m.deck_steps(T0 + 40 * MINUTE - SECOND);
        assert!(r.live);
        assert_eq!(r.count, Some(40 * 60 - 1));
        assert_eq!(r.still_s, Some(0));
    }

    #[test]
    fn frozen_steps_warn_and_pause_sooner() {
        let mut m = WalkAway::default();
        let c = steps_cfg();
        // Two minutes of walking, then the walker leaves: the count stops.
        let left = T0 + 2 * MINUTE;
        let deck = move |t: u64| Some(frame(((t.min(left) - T0) / SECOND) as u16));
        let last_move = left;
        let warn_at = last_move + STEPS_AWAY_MS;
        assert!(run_deck(&mut m, &c, T0, warn_at, T0, deck).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
        assert!(run_deck(&mut m, &c, warn_at, warn_at + SECOND, T0, deck).is_empty());
        assert_eq!(m.card(), WalkAwayCard::Warning);
        assert_eq!(m.reason(), Some(WalkAwayReason::Steps));
        assert_eq!(m.pause_in_s(warn_at), 30);
        let r = m.deck_steps(warn_at);
        assert_eq!(r.still_s, Some((STEPS_AWAY_MS / SECOND) as u32));
        let effects = run_deck(
            &mut m,
            &c,
            warn_at + SECOND,
            warn_at + WARNING_MS + SECOND,
            T0,
            deck,
        );
        assert_eq!(
            effects,
            vec![(warn_at + WARNING_MS, Effect::Pause(WalkAwayReason::Steps))]
        );
        assert_eq!(m.card(), WalkAwayCard::Paused);
        // DUMP cuts the walk at the last step.
        assert_eq!(m.empty_stretch(), Some((last_move, warn_at + WARNING_MS)));
        // The same walk-away with the switch off warns only at 10 minutes.
        let mut off = WalkAway::default();
        let effects = run_deck(&mut off, &cfg(), T0, T0 + 10 * MINUTE, T0, deck);
        assert!(effects.is_empty());
        assert_eq!(off.card(), WalkAwayCard::None);
    }

    #[test]
    fn steps_coming_back_cancel_the_warning() {
        let mut m = WalkAway::default();
        let c = steps_cfg();
        let stop = T0 + MINUTE;
        let back = stop + STEPS_AWAY_MS + 10 * SECOND;
        let deck = move |t: u64| {
            let n = if t < stop {
                (t - T0) / SECOND
            } else if t < back {
                60
            } else {
                60 + (t - back) / SECOND + 1
            };
            Some(frame(n as u16))
        };
        run_deck(&mut m, &c, T0, back, T0, deck);
        assert_eq!(m.card(), WalkAwayCard::Warning);
        assert!(run_deck(&mut m, &c, back, back + 5 * MINUTE, T0, deck).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    #[test]
    fn input_still_counts_while_the_steps_are_frozen() {
        // Typing with the count frozen (standing on the side rails, say):
        // the steps never pause a walker who is at the keyboard.
        let mut m = WalkAway::default();
        let c = steps_cfg();
        let deck = |t: u64| Some(frame(((t.min(T0 + MINUTE) - T0) / SECOND) as u16));
        let mut t = T0;
        while t < T0 + 20 * MINUTE {
            let input = Input {
                deck_steps: steps_of(deck(t)),
                // A key every 20 s.
                ..walking((t - T0) % (20 * SECOND))
            };
            assert_eq!(m.tick(&c, t, &input), None);
            assert_eq!(m.card(), WalkAwayCard::None, "{}", t - T0);
            t += SECOND;
        }
    }

    #[test]
    fn absent_steps_leave_walk_away_as_it_was() {
        // Short frames (no counters) and full frames that always say 0.
        for deck in [
            Box::new(|_: u64| Some(short_frame())) as Box<dyn Fn(u64) -> Option<Vec<u8>>>,
            Box::new(|_: u64| Some(frame(0))),
            Box::new(|_: u64| Some(frame(1234))),
            Box::new(|_: u64| None),
        ] {
            let mut m = WalkAway::default();
            let c = steps_cfg();
            assert!(run_deck(&mut m, &c, T0, T0 + 10 * MINUTE, T0, &deck).is_empty());
            assert_eq!(m.card(), WalkAwayCard::None);
            run_deck(
                &mut m,
                &c,
                T0 + 10 * MINUTE,
                T0 + 10 * MINUTE + SECOND,
                T0,
                &deck,
            );
            assert_eq!(m.card(), WalkAwayCard::Warning);
            assert_eq!(m.reason(), Some(WalkAwayReason::Idle));
            assert!(!m.deck_steps(T0).live);
        }
    }

    #[test]
    fn a_count_that_moves_only_once_or_twice_is_not_a_sensor() {
        // A reset as the walk starts, then nothing: not live, so no sooner
        // warning either.
        let mut m = WalkAway::default();
        let c = steps_cfg();
        let deck = |t: u64| {
            Some(frame(if t < T0 + 5 * SECOND {
                900
            } else if t < T0 + 6 * SECOND {
                0
            } else {
                1
            }))
        };
        assert!(run_deck(&mut m, &c, T0, T0 + 10 * MINUTE, T0, deck).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
        assert_eq!(m.deck_steps(T0).moves, 2);
        assert!(!m.deck_steps(T0).live);
    }

    #[test]
    fn switch_off_ignores_a_live_count() {
        // Feet moving, no input: the warning comes at the idle minutes, as
        // with no steps at all.
        let mut m = WalkAway::default();
        let c = cfg();
        assert!(run_deck(&mut m, &c, T0, T0 + 10 * MINUTE, T0, stepping(T0)).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
        run_deck(
            &mut m,
            &c,
            T0 + 10 * MINUTE,
            T0 + 10 * MINUTE + SECOND,
            T0,
            stepping(T0),
        );
        assert_eq!(m.card(), WalkAwayCard::Warning);
        assert_eq!(m.reason(), Some(WalkAwayReason::Idle));
        // The readout still shows the count moving, for the hardware check.
        assert!(m.deck_steps(T0 + 10 * MINUTE).live);
    }

    #[test]
    fn steps_hold_the_warning_off_for_an_hour_at_most() {
        // A count that follows the belt instead of feet must not keep an
        // empty belt going for ever.
        let mut m = WalkAway::default();
        let c = steps_cfg();
        let hour = T0 + STEPS_HOLD_MAX_MS;
        assert!(run_deck(&mut m, &c, T0, hour, T0, stepping(T0)).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
        run_deck(&mut m, &c, hour, hour + SECOND, T0, stepping(T0));
        assert_eq!(m.card(), WalkAwayCard::Warning);
        assert_eq!(m.reason(), Some(WalkAwayReason::Idle));
        // STAY gives the steps another hour.
        m.stay(hour + SECOND);
        assert!(run_deck(
            &mut m,
            &c,
            hour + SECOND,
            hour + 30 * MINUTE,
            T0,
            stepping(T0)
        )
        .is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
    }

    #[test]
    fn a_stalled_link_falls_back_to_the_idle_minutes() {
        // Frames stop (no fresh count) after the count went live: a frozen
        // reading from a stalled link is not a walker who left.
        let mut m = WalkAway::default();
        let c = steps_cfg();
        let stall = T0 + MINUTE;
        let deck = move |t: u64| (t < stall).then(|| frame(((t - T0) / SECOND) as u16));
        assert!(run_deck(&mut m, &c, T0, T0 + 10 * MINUTE, T0, deck).is_empty());
        assert_eq!(m.card(), WalkAwayCard::None);
        assert_eq!(m.deck_steps(T0 + 5 * MINUTE).count, None);
    }

    #[test]
    fn a_new_connection_watches_the_count_afresh() {
        let mut m = WalkAway::default();
        let c = steps_cfg();
        run_deck(&mut m, &c, T0, T0 + MINUTE, T0, stepping(T0));
        assert!(m.deck_steps(T0 + MINUTE).live);
        let offline = Input {
            connected: false,
            walking: false,
            ..walking(0)
        };
        m.tick(&c, T0 + MINUTE, &offline);
        let r = m.deck_steps(T0 + MINUTE);
        assert!(!r.live);
        assert_eq!(r.count, None);
        assert_eq!(r.still_s, None, "not walking");
    }

    #[test]
    fn turning_steps_off_drops_a_steps_warning() {
        let mut m = WalkAway::default();
        let left = T0 + MINUTE;
        let deck = move |t: u64| Some(frame(((t.min(left) - T0) / SECOND) as u16));
        run_deck(
            &mut m,
            &steps_cfg(),
            T0,
            left + STEPS_AWAY_MS + SECOND,
            T0,
            deck,
        );
        assert_eq!(m.reason(), Some(WalkAwayReason::Steps));
        m.reconfigure(&cfg());
        assert_eq!(m.card(), WalkAwayCard::None);
        // An idle warning stays when only the steps switch goes off.
        let mut m = WalkAway::default();
        run(&mut m, &steps_cfg(), T0, T0 + 10 * MINUTE + SECOND, T0);
        assert_eq!(m.reason(), Some(WalkAwayReason::Idle));
        m.reconfigure(&cfg());
        assert_eq!(m.card(), WalkAwayCard::Warning);
    }

    #[test]
    fn a_steps_pause_never_resumes_the_belt() {
        let mut m = WalkAway::default();
        let c = steps_cfg();
        let left = T0 + MINUTE;
        let deck = move |t: u64| Some(frame(((t.min(left) - T0) / SECOND) as u16));
        let effects = run_deck(&mut m, &c, T0, left + 2 * MINUTE, T0, deck);
        assert!(effects
            .iter()
            .all(|(_, e)| *e == Effect::Pause(WalkAwayReason::Steps)));
        // The belt reports paused for an hour; the count even moves (the
        // walker back on the deck): nothing but the card.
        let paused = left + 2 * MINUTE;
        for i in 0..3600 {
            let t = paused + i * SECOND;
            let input = Input {
                walking: false,
                deck_steps: Some(60 + i as u16),
                ..walking(t - T0)
            };
            assert_eq!(m.tick(&c, t, &input), None);
        }
        assert_eq!(m.card(), WalkAwayCard::Paused);
    }
}
