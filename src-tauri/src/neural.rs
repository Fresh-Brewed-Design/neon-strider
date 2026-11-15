//! Neural flow: eases the belt off while you type a lot, and brings it back
//! once you stop.
//!
//! Pure logic, driven once a second with injected times (`now_ms`) so it can be
//! tested. The keyboard hook (`hooks.rs`) only counts key presses (never which
//! key); the protocol worker drains that count each second and calls
//! [`NeuralFlow::tick`]. The engine reads the result through
//! [`NeuralFlow::speed_for`] inside `resolve_target`, so the manual and program
//! intents themselves are never changed: a speed set by hand (or by a program
//! step, or by quiet mode on a call) stays the base the slowdown is measured from.
//!
//! The shape of it:
//! - Score: keys per minute over a rolling 20 s window.
//! - Slowing starts only after about 10 s of sustained typing (a streak of key
//!   presses with no gap over 8 s), so a short burst never slows the belt.
//! - The depth follows a wide, linear band of typing rates per sensitivity,
//!   down to the floor (a % of the base speed).
//! - Recovery starts only after 45 s with little typing, then ramps up in
//!   small steps, so a thinking pause never speeds the belt straight back up.
//! - Targets sit on the treadmill's own speed steps, change at most once every
//!   15 s, never go below the floor or the treadmill's minimum, never above the
//!   base, and never change while the belt is stopped, paused or spinning up.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// How readily typing slows the belt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum NeuralSensitivity {
    Low,
    #[default]
    Medium,
    High,
}

impl NeuralSensitivity {
    /// Keys per minute where slowing begins, and where it reaches the floor.
    ///
    /// Realistic typing: bursts run at 200 to 300 kpm, but the average over a
    /// working stretch, pauses included, sits around 60 to 120 kpm. Every level
    /// starts at moderate sustained typing and spreads the slowdown over a wide
    /// band, so a typical pace lands part-way down and only intense typing
    /// reaches the floor. Medium is about halfway at 110 kpm.
    pub fn band(self) -> (f32, f32) {
        match self {
            NeuralSensitivity::High => (20.0, 150.0),
            NeuralSensitivity::Medium => (30.0, 190.0),
            NeuralSensitivity::Low => (45.0, 240.0),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            NeuralSensitivity::Low => "low",
            NeuralSensitivity::Medium => "medium",
            NeuralSensitivity::High => "high",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().trim_matches('"').to_ascii_lowercase().as_str() {
            "low" => Some(NeuralSensitivity::Low),
            "medium" => Some(NeuralSensitivity::Medium),
            "high" => Some(NeuralSensitivity::High),
            _ => None,
        }
    }
}

/// "Slowest speed", in % of the base speed.
pub const DEFAULT_FLOOR_PCT: f32 = 90.0;
pub const MIN_FLOOR_PCT: f32 = 50.0;
pub const MAX_FLOOR_PCT: f32 = 95.0;

/// Settings keys (raw values in the `settings` table).
pub const KEY_FLOOR_PCT: &str = "neuralFloorPct";
pub const KEY_SENSITIVITY: &str = "neuralSensitivity";
/// The old four-knob settings; read once to migrate, then deleted.
pub const LEGACY_KEYS: [&str; 4] = [
    "glideThreshold",
    "glideSaturation",
    "glideBiasPercentage",
    "glideSensitivity",
];

pub fn clamp_floor_pct(pct: f32) -> f32 {
    if pct.is_finite() {
        pct.clamp(MIN_FLOOR_PCT, MAX_FLOOR_PCT)
    } else {
        DEFAULT_FLOOR_PCT
    }
}

/// Map the old settings onto the new two, for a database that has no new keys
/// yet. The old score was "heat" per second with 5 per key press (so heat x 12
/// is roughly keys per minute), and mouse movement counted too, so only the
/// start threshold says much about how sensitive it was meant to be.
///
/// - Floor: 100 minus the old bias. The old default (bias 1, a 99% floor) did
///   almost nothing, and floors below 90% proved too drastic, so anything
///   outside 90 to 95 becomes 90.
/// - Sensitivity: the old default threshold (0.25) or none is Medium;
///   otherwise the threshold in keys per minute picks the nearest level.
pub fn migrate_legacy(bias_pct: Option<f32>, threshold: Option<f32>) -> (f32, NeuralSensitivity) {
    let floor = match bias_pct.filter(|b| b.is_finite()) {
        Some(b) => {
            let old = 100.0 - b;
            if (DEFAULT_FLOOR_PCT..=MAX_FLOOR_PCT).contains(&old) {
                old
            } else {
                DEFAULT_FLOOR_PCT
            }
        }
        None => DEFAULT_FLOOR_PCT,
    };
    let sensitivity = match threshold.filter(|t| t.is_finite() && *t > 0.0) {
        None => NeuralSensitivity::Medium,
        Some(t) if (t - 0.25).abs() < 1e-3 => NeuralSensitivity::Medium,
        Some(t) => {
            let kpm = t * 12.0;
            if kpm < 25.0 {
                NeuralSensitivity::High
            } else if kpm <= 400.0 {
                // Old thresholds of 3 to 30 (36 to 360 kpm) are where nearly every
                // saved setting sits; with the mouse no longer counting, Medium
                // is the closest match for all of them.
                NeuralSensitivity::Medium
            } else {
                NeuralSensitivity::Low
            }
        }
    };
    (floor, sensitivity)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub floor_pct: f32,
    pub sensitivity: NeuralSensitivity,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            floor_pct: DEFAULT_FLOOR_PCT,
            sensitivity: NeuralSensitivity::Medium,
        }
    }
}

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------

/// Rolling window for the keys-per-minute score.
const WINDOW_MS: u64 = 20_000;
/// Short window that decides "little typing" for recovery.
const QUIET_WINDOW_MS: u64 = 10_000;
/// Typing must span this long (no gap over STREAK_GAP_MS) before slowing.
const SLOW_AFTER_MS: u64 = 10_000;
/// A gap this long ends a typing streak.
const STREAK_GAP_MS: u64 = 8_000;
/// Little typing for this long before the belt starts to recover.
const RECOVER_AFTER_MS: u64 = 45_000;
/// At most one speed change in this long.
const MIN_CHANGE_GAP_MS: u64 = 15_000;
/// After a speed set by hand: no slowing for this long.
const HAND_HOLD_MS: u64 = 30_000;
/// Key presses counted per second at most (a held key auto-repeats ~30/s).
const MAX_KEYS_PER_TICK: u32 = 12;
/// Speed steps per change: down, and back up.
const SLOW_STEPS_PER_CHANGE: u32 = 2;
const RECOVER_STEPS_PER_CHANGE: u32 = 2;
/// Slow only once the depth asks for this many steps more than now: keeps a
/// rate hovering at a boundary from toggling a step.
const SLOW_MARGIN_STEPS: f32 = 0.3;

/// What the engine knows this second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    pub now_ms: u64,
    /// Key presses since the last tick (counts only).
    pub keys: u32,
    /// The speed the user, a program step or quiet mode asked for.
    pub base_kmh: f32,
    /// The treadmill's slowest speed.
    pub min_kmh: f32,
    /// Physical km/h of one hardware speed step near the base.
    pub step_kmh: f32,
    /// The belt is running (not stopped, paused or spinning up).
    pub running: bool,
    /// Quiet mode is holding the belt for a call.
    pub quiet: bool,
    /// Bumped whenever the user sets a speed by hand.
    pub hand_epoch: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NeuralFlow {
    window: VecDeque<(u64, u32)>,
    streak_start: Option<u64>,
    last_key: Option<u64>,
    quiet_since: Option<u64>,
    hold_until: Option<u64>,
    last_change: Option<u64>,
    /// The base the current slowdown is measured from.
    base_kmh: f32,
    step_kmh: f32,
    /// Hardware steps below the base; 0 = no slowdown.
    steps_down: u32,
    hand_epoch: u64,
    /// While quiet mode holds the belt: the speed it was at when the call
    /// began. The belt runs at the lower of this and the quiet speed.
    quiet_cap: Option<f32>,
    keys_per_min: f32,
    initialized: bool,
}

impl NeuralFlow {
    pub fn keys_per_min(&self) -> f32 {
        self.keys_per_min
    }

    /// The base the slowdown is measured from.
    pub fn base_kmh(&self) -> f32 {
        self.base_kmh
    }

    /// The speed this module wants right now for its own base (no slowdown = the base).
    pub fn target_kmh(&self) -> f32 {
        let t = self.base_kmh - self.steps_down as f32 * self.step_kmh;
        match self.quiet_cap {
            Some(cap) => t.min(cap),
            None => t,
        }
    }

    /// The belt is being eased off right now.
    pub fn active(&self) -> bool {
        self.base_kmh > 0.0 && self.target_kmh() < self.base_kmh - 1e-4 && self.quiet_cap.is_none()
    }

    /// Target / base, 1.0 when not slowing.
    pub fn factor(&self) -> f32 {
        if self.active() {
            self.target_kmh() / self.base_kmh
        } else {
            1.0
        }
    }

    /// The speed to send for `base` (what resolve_target picked). Pure read,
    /// valid between ticks: a speed set by hand since the last tick wins at
    /// once; a base changed by a program step or quiet mode gets the current
    /// slowdown straight away.
    /// `quiet` is quiet mode holding the belt for a call: then the lower of
    /// its speed and ours wins, and nothing slows on top of it.
    pub fn speed_for(&self, base: f32, hand_epoch: u64, quiet: bool) -> f32 {
        if !self.initialized || hand_epoch != self.hand_epoch || base <= 0.0 {
            return base;
        }
        if quiet {
            return base.min(self.quiet_cap.unwrap_or_else(|| self.target_kmh()));
        }
        if (base - self.base_kmh).abs() < 1e-3 {
            return self.target_kmh().min(base);
        }
        if self.steps_down == 0 || self.base_kmh <= 0.0 {
            return base;
        }
        let factor = (self.base_kmh - self.steps_down as f32 * self.step_kmh) / self.base_kmh;
        base - rescaled_steps(base, factor, self.step_kmh) as f32 * self.step_kmh
    }

    /// Clear everything (feature switched off, belt stopped).
    pub fn reset(&mut self) {
        *self = NeuralFlow::default();
    }

    /// The depth (in steps below base) the current typing rate asks for.
    fn desired_steps(&self, cfg: &Config, base: f32, min_kmh: f32, step: f32) -> u32 {
        let max = max_steps(base, cfg.floor_pct, min_kmh, step);
        let depth =
            depth_for_kpm(cfg.sensitivity, self.keys_per_min) * (1.0 - cfg.floor_pct / 100.0);
        let steps = (base * depth / step + 1e-4).floor().max(0.0) as u32;
        steps.min(max)
    }

    /// One second. Returns true when the target speed changed.
    pub fn tick(&mut self, cfg: &Config, t: &Tick) -> bool {
        let before = self.target_kmh();
        let before_base = self.base_kmh;
        let step = if t.step_kmh.is_finite() && t.step_kmh > 0.0 {
            t.step_kmh
        } else {
            0.1
        };

        if !self.initialized {
            self.initialized = true;
            self.hand_epoch = t.hand_epoch;
            self.base_kmh = t.base_kmh.max(0.0);
            self.step_kmh = step;
        }

        // Not running: hold everything as it is, so nothing moves the belt and
        // a changed target can't trigger the engine's auto-resume.
        if !t.running {
            if t.base_kmh <= 0.0 {
                // Stopped (intents cleared) or a program on HOLD: start over.
                let epoch = t.hand_epoch;
                self.reset();
                self.initialized = true;
                self.hand_epoch = epoch;
            } else if t.hand_epoch != self.hand_epoch {
                self.hand_reset(t);
            }
            return false;
        }

        if !cfg.enabled {
            let epoch = t.hand_epoch;
            self.reset();
            self.initialized = true;
            self.hand_epoch = epoch;
            self.base_kmh = t.base_kmh.max(0.0);
            self.step_kmh = step;
            return (self.target_kmh() - before).abs() > 1e-4
                && (before_base - self.base_kmh).abs() < 1e-3;
        }

        // Score.
        let keys = t.keys.min(MAX_KEYS_PER_TICK);
        self.window.push_back((t.now_ms, keys));
        while let Some(&(at, _)) = self.window.front() {
            if t.now_ms.saturating_sub(at) >= WINDOW_MS {
                self.window.pop_front();
            } else {
                break;
            }
        }
        let total: u32 = self.window.iter().map(|&(_, k)| k).sum();
        self.keys_per_min = total as f32 * 60_000.0 / WINDOW_MS as f32;
        let recent: u32 = self
            .window
            .iter()
            .filter(|&&(at, _)| t.now_ms.saturating_sub(at) < QUIET_WINDOW_MS)
            .map(|&(_, k)| k)
            .sum();
        let recent_kpm = recent as f32 * 60_000.0 / QUIET_WINDOW_MS as f32;

        // Typing streak.
        if keys > 0 {
            let gap_ok = self
                .last_key
                .is_some_and(|k| t.now_ms.saturating_sub(k) <= STREAK_GAP_MS);
            if !gap_ok {
                self.streak_start = Some(t.now_ms);
            }
            self.last_key = Some(t.now_ms);
        } else if self
            .last_key
            .is_some_and(|k| t.now_ms.saturating_sub(k) > STREAK_GAP_MS)
        {
            self.streak_start = None;
        }

        // Little typing.
        let (start_kpm, _) = cfg.sensitivity.band();
        if recent_kpm < start_kpm / 2.0 {
            self.quiet_since.get_or_insert(t.now_ms);
        } else {
            self.quiet_since = None;
        }

        // A speed set by hand is the new base: start over and hold off.
        if t.hand_epoch != self.hand_epoch {
            // The user's own change, not ours.
            self.hand_reset(t);
            return false;
        }

        // Quiet mode on a call wins: never slow on top of it, never speed past
        // what the belt was doing when the call began.
        if t.quiet {
            if self.quiet_cap.is_none() {
                self.quiet_cap = Some(before.max(0.0));
            }
            return false;
        }
        if self.quiet_cap.take().is_some() {
            // The call ended. A restored base equal to ours carries on; any
            // other base is handled below.
            self.last_change = Some(t.now_ms);
        }

        // The base moved without the user (a program step, a call restore):
        // the slowdown carries over to the new base.
        if (t.base_kmh - self.base_kmh).abs() >= 1e-3 || (step - self.step_kmh).abs() > 1e-4 {
            let factor = if self.base_kmh > 0.0 {
                before.min(self.base_kmh) / self.base_kmh
            } else {
                1.0
            };
            self.base_kmh = t.base_kmh.max(0.0);
            self.step_kmh = step;
            self.steps_down = if self.steps_down > 0 {
                rescaled_steps(self.base_kmh, factor, step).min(max_steps(
                    self.base_kmh,
                    cfg.floor_pct,
                    t.min_kmh,
                    step,
                ))
            } else {
                0
            };
            if self.steps_down > 0 {
                self.last_change = Some(t.now_ms);
            }
            return false;
        }

        let base = self.base_kmh;
        let max = max_steps(base, cfg.floor_pct, t.min_kmh, step);
        if self.steps_down > max {
            // The floor or the treadmill minimum moved up.
            self.steps_down = max;
            self.last_change = Some(t.now_ms);
            return true;
        }

        let held = self.hold_until.is_some_and(|h| t.now_ms < h);
        let gap_ok = self
            .last_change
            .is_none_or(|c| t.now_ms.saturating_sub(c) >= MIN_CHANGE_GAP_MS);
        if !gap_ok {
            return false;
        }

        let sustained = match (self.streak_start, self.last_key) {
            (Some(s), Some(k)) => k.saturating_sub(s) >= SLOW_AFTER_MS,
            _ => false,
        };
        let desired = self.desired_steps(cfg, base, t.min_kmh, step);
        let depth_wanted = {
            let d =
                depth_for_kpm(cfg.sensitivity, self.keys_per_min) * (1.0 - cfg.floor_pct / 100.0);
            base * d / step
        };

        if !held
            && sustained
            && desired > self.steps_down
            && depth_wanted >= self.steps_down as f32 + 1.0 + SLOW_MARGIN_STEPS - 1e-4
        {
            self.steps_down = (self.steps_down + SLOW_STEPS_PER_CHANGE).min(desired);
            self.last_change = Some(t.now_ms);
            return true;
        }

        let quiet_long = self
            .quiet_since
            .is_some_and(|q| t.now_ms.saturating_sub(q) >= RECOVER_AFTER_MS);
        if quiet_long && self.steps_down > 0 {
            self.steps_down = self.steps_down.saturating_sub(RECOVER_STEPS_PER_CHANGE);
            self.last_change = Some(t.now_ms);
            return true;
        }
        false
    }

    fn hand_reset(&mut self, t: &Tick) {
        self.window.clear();
        self.streak_start = None;
        self.last_key = None;
        self.quiet_since = None;
        self.quiet_cap = None;
        self.steps_down = 0;
        self.keys_per_min = 0.0;
        self.hand_epoch = t.hand_epoch;
        self.base_kmh = t.base_kmh.max(0.0);
        if t.step_kmh.is_finite() && t.step_kmh > 0.0 {
            self.step_kmh = t.step_kmh;
        }
        self.hold_until = Some(t.now_ms + HAND_HOLD_MS);
        self.last_change = None;
    }
}

/// Slowdown depth, 0 to 1 of the way to the floor, for a typing rate. Linear
/// across the sensitivity's band: monotonic, and a small change in rate only
/// moves it a little.
fn depth_for_kpm(sensitivity: NeuralSensitivity, kpm: f32) -> f32 {
    let (start, full) = sensitivity.band();
    // NaN counts as no typing.
    if kpm.is_nan() || kpm <= start {
        return 0.0;
    }
    ((kpm - start) / (full - start)).clamp(0.0, 1.0)
}

/// Most steps below `base` allowed by the floor and the treadmill minimum.
fn max_steps(base: f32, floor_pct: f32, min_kmh: f32, step: f32) -> u32 {
    let lowest = (base * clamp_floor_pct(floor_pct) / 100.0).max(min_kmh);
    if base <= lowest || step <= 0.0 {
        return 0;
    }
    ((base - lowest) / step + 1e-4).floor().max(0.0) as u32
}

/// Steps below `base` that keep about `factor` of it.
fn rescaled_steps(base: f32, factor: f32, step: f32) -> u32 {
    if step <= 0.0 || factor.is_nan() || factor >= 1.0 {
        return 0;
    }
    ((base * (1.0 - factor)) / step).round().max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_000_000;

    fn cfg() -> Config {
        Config {
            enabled: true,
            floor_pct: 80.0,
            sensitivity: NeuralSensitivity::Medium,
        }
    }

    fn tick_at(s: u64, keys: u32, base: f32) -> Tick {
        Tick {
            now_ms: T0 + s * 1000,
            keys,
            base_kmh: base,
            min_kmh: 0.6,
            step_kmh: 0.1,
            running: true,
            quiet: false,
            hand_epoch: 0,
        }
    }

    /// Run seconds `from..to`, with `keys(s)` presses each second. Returns the
    /// seconds at which the target changed and the target after each.
    fn run(
        n: &mut NeuralFlow,
        c: &Config,
        from: u64,
        to: u64,
        base: f32,
        keys: impl Fn(u64) -> u32,
        tweak: impl Fn(&mut Tick),
    ) -> Vec<(u64, f32)> {
        let mut out = vec![];
        for s in from..to {
            let mut t = tick_at(s, keys(s), base);
            tweak(&mut t);
            if n.tick(c, &t) {
                out.push((s, n.target_kmh()));
            }
        }
        out
    }

    /// Steady typing at about `kpm`, spread evenly.
    fn steady(kpm: u32) -> impl Fn(u64) -> u32 {
        move |s| {
            let before = (s as u32 * kpm) / 60;
            let after = ((s as u32 + 1) * kpm) / 60;
            after - before
        }
    }

    /// Bursty real typing: 4 s at ~240 kpm, then a 3 s pause (~137 kpm average).
    fn bursty(s: u64) -> u32 {
        if s % 7 < 4 {
            4
        } else {
            0
        }
    }

    fn none(_: &mut Tick) {}

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn steady_typing_slows_within_expected_time() {
        let mut n = NeuralFlow::default();
        let fx = run(&mut n, &cfg(), 0, 40, 3.0, steady(150), none);
        let first = fx.first().expect("slowed").0;
        assert!((10..=20).contains(&first), "first slowdown at {first}s");
        assert!(n.target_kmh() < 3.0);
        assert!(n.active());
        assert!(n.factor() < 1.0);

        let mut n = NeuralFlow::default();
        let fx = run(&mut n, &cfg(), 0, 40, 3.0, bursty, none);
        assert!(
            fx.first().is_some_and(|f| (10..=20).contains(&f.0)),
            "{fx:?}"
        );
    }

    #[test]
    fn a_short_burst_does_not_slow() {
        let mut n = NeuralFlow::default();
        // Five seconds at 300 kpm, then nothing.
        let fx = run(
            &mut n,
            &cfg(),
            0,
            90,
            3.0,
            |s| if s < 5 { 5 } else { 0 },
            none,
        );
        assert!(fx.is_empty(), "{fx:?}");
        // Two short bursts 12 s apart are still not sustained typing.
        let mut n = NeuralFlow::default();
        let fx = run(
            &mut n,
            &cfg(),
            0,
            60,
            3.0,
            |s| if s < 5 || (17..22).contains(&s) { 5 } else { 0 },
            none,
        );
        assert!(fx.is_empty(), "{fx:?}");
    }

    #[test]
    fn a_ten_second_pause_does_not_restore() {
        let mut n = NeuralFlow::default();
        run(&mut n, &cfg(), 0, 60, 3.0, steady(200), none);
        let slowed = n.target_kmh();
        assert!(slowed < 3.0);
        // Pause 10 s, then keep typing.
        let fx = run(&mut n, &cfg(), 60, 70, 3.0, |_| 0, none);
        assert!(fx.iter().all(|&(_, v)| v <= slowed + 1e-4), "{fx:?}");
        run(&mut n, &cfg(), 70, 100, 3.0, steady(200), none);
        assert!(n.target_kmh() <= slowed + 1e-4);
    }

    #[test]
    fn about_sixty_seconds_idle_restores_gradually() {
        let mut n = NeuralFlow::default();
        run(&mut n, &cfg(), 0, 90, 3.0, steady(250), none);
        let slowed = n.target_kmh();
        assert!(
            close(slowed, 2.4),
            "reached the floor at intense typing: {slowed}"
        );
        // Idle: nothing for the first 45 s.
        let early = run(&mut n, &cfg(), 90, 135, 3.0, |_| 0, none);
        assert!(early.is_empty(), "{early:?}");
        let fx = run(&mut n, &cfg(), 135, 240, 3.0, |_| 0, none);
        let first = fx.first().expect("recovers").0;
        assert!(
            first <= 90 + 60,
            "first recovery step at {}s after stopping",
            first - 90
        );
        // In steps, upwards, ending at the base.
        assert!(fx.len() >= 2, "{fx:?}");
        assert!(fx.windows(2).all(|w| w[1].1 > w[0].1));
        assert!(fx[0].1 < 3.0);
        assert!(close(n.target_kmh(), 3.0));
        assert!(!n.active());
    }

    #[test]
    fn changes_are_spaced_and_quantized() {
        let mut n = NeuralFlow::default();
        let c = Config {
            floor_pct: 50.0,
            ..cfg()
        };
        let keys = |s: u64| {
            if s < 120 {
                steady(250)(s)
            } else if s < 300 {
                0
            } else {
                steady(100)(s)
            }
        };
        let fx = run(&mut n, &c, 0, 500, 5.0, keys, none);
        assert!(fx.len() >= 4, "{fx:?}");
        for w in fx.windows(2) {
            assert!(w[1].0 - w[0].0 >= 15, "changes {}s apart", w[1].0 - w[0].0);
        }
        for &(_, v) in &fx {
            let steps = (5.0 - v) / 0.1;
            assert!(
                (steps - steps.round()).abs() < 1e-3,
                "{v} is not on a 0.1 step"
            );
            assert!((2.5 - 1e-4..=5.0 + 1e-4).contains(&v));
        }
    }

    #[test]
    fn a_hand_change_becomes_the_base_and_holds_off() {
        let mut n = NeuralFlow::default();
        run(&mut n, &cfg(), 0, 60, 3.0, steady(200), none);
        assert!(n.active());
        // The user sets 3.5 by hand: the slowdown is gone at once.
        assert!(close(n.speed_for(3.5, 1, false), 3.5));
        let fx = run(&mut n, &cfg(), 60, 90, 3.5, steady(200), |t| {
            t.hand_epoch = 1
        });
        assert!(fx.is_empty(), "held off for 30 s: {fx:?}");
        assert!(close(n.target_kmh(), 3.5));
        assert!(close(n.base_kmh(), 3.5));
        // After the hold, sustained typing slows from the new base.
        let fx = run(&mut n, &cfg(), 90, 120, 3.5, steady(200), |t| {
            t.hand_epoch = 1
        });
        assert!(!fx.is_empty());
        assert!(n.target_kmh() < 3.5);
    }

    #[test]
    fn the_floor_and_minimum_are_respected() {
        for floor in [50.0, 80.0, 90.0, 95.0] {
            let c = Config {
                floor_pct: floor,
                ..cfg()
            };
            let mut n = NeuralFlow::default();
            run(&mut n, &c, 0, 300, 4.0, steady(400), none);
            let lowest = 4.0 * floor / 100.0;
            assert!(
                n.target_kmh() >= lowest - 1e-3,
                "floor {floor}: {}",
                n.target_kmh()
            );
            assert!(
                n.target_kmh() < lowest + 0.1 + 1e-3,
                "floor {floor} reached: {}",
                n.target_kmh()
            );
        }
        // Never below the treadmill's minimum.
        let mut n = NeuralFlow::default();
        let c = Config {
            floor_pct: 50.0,
            ..cfg()
        };
        run(&mut n, &c, 0, 300, 1.0, steady(400), none);
        assert!(n.target_kmh() >= 0.6 - 1e-4);
        // A base at the minimum is never slowed.
        let mut n = NeuralFlow::default();
        assert!(run(&mut n, &c, 0, 120, 0.6, steady(400), none).is_empty());
    }

    #[test]
    fn quiet_mode_wins_and_never_slows_twice() {
        let mut n = NeuralFlow::default();
        run(&mut n, &cfg(), 0, 60, 4.0, steady(250), none);
        let slowed = n.target_kmh();
        assert!(slowed < 4.0);
        // A call: quiet mode sets the base to 2.0, below our target. The quiet speed wins as is.
        assert!(close(n.speed_for(2.0, 0, true), 2.0));
        let fx = run(&mut n, &cfg(), 60, 120, 2.0, steady(250), |t| {
            t.quiet = true
        });
        assert!(fx.is_empty());
        assert!(
            close(n.speed_for(2.0, 0, true), 2.0),
            "not slowed on top of quiet mode"
        );
        // A quiet speed above our slowed speed: the lower one wins.
        let mut m = NeuralFlow::default();
        run(&mut m, &cfg(), 0, 60, 4.0, steady(250), none);
        let ours = m.target_kmh();
        run(&mut m, &cfg(), 60, 70, 3.9, steady(250), |t| t.quiet = true);
        assert!(close(m.speed_for(3.9, 0, true), ours.min(3.9)));
        // The call ends and the base comes back: the slowdown carries on.
        run(&mut n, &cfg(), 120, 121, 4.0, steady(250), none);
        assert!(
            close(n.speed_for(4.0, 0, false), slowed),
            "{} vs {}",
            n.speed_for(4.0, 0, false),
            slowed
        );
    }

    #[test]
    fn a_program_step_carries_the_slowdown_to_the_new_base() {
        let mut n = NeuralFlow::default();
        run(&mut n, &cfg(), 0, 60, 4.0, steady(250), none);
        let f = n.target_kmh() / 4.0;
        // Between ticks the new base already gets the slowdown.
        let v = n.speed_for(5.0, 0, false);
        assert!((v / 5.0 - f).abs() < 0.03, "{v}");
        run(&mut n, &cfg(), 60, 61, 5.0, steady(250), none);
        assert!(close(n.target_kmh(), v));
        assert!(n.target_kmh() >= 4.0 - 1e-4);
    }

    #[test]
    fn nothing_changes_while_paused_and_a_stop_resets() {
        let mut n = NeuralFlow::default();
        run(&mut n, &cfg(), 0, 60, 3.0, steady(250), none);
        let slowed = n.target_kmh();
        let fx = run(&mut n, &cfg(), 60, 200, 3.0, |_| 0, |t| t.running = false);
        assert!(fx.is_empty());
        assert!(close(n.speed_for(3.0, 0, false), slowed));
        // Switching off while paused also leaves the target alone until it runs.
        let off = Config {
            enabled: false,
            ..cfg()
        };
        assert!(run(&mut n, &off, 200, 210, 3.0, |_| 0, |t| t.running = false).is_empty());
        assert!(close(n.speed_for(3.0, 0, false), slowed));
        // A stop clears the intents: everything starts over.
        run(&mut n, &cfg(), 210, 211, 0.0, |_| 0, |t| t.running = false);
        assert!(close(n.speed_for(3.0, 0, false), 3.0));
    }

    #[test]
    fn the_curve_is_monotonic_and_gentle() {
        for s in [
            NeuralSensitivity::Low,
            NeuralSensitivity::Medium,
            NeuralSensitivity::High,
        ] {
            let mut prev = 0.0;
            for kpm in 0..400 {
                let d = depth_for_kpm(s, kpm as f32);
                assert!(d >= prev, "{s:?} not monotonic at {kpm}");
                // 5 kpm never moves a 5 km/h belt a whole 0.1 step, even at a 50% floor.
                let d5 = depth_for_kpm(s, kpm as f32 + 5.0);
                assert!(5.0 * (d5 - d) * 0.5 < 0.1, "{s:?} steep at {kpm}");
                prev = d;
            }
            // Moderate sustained typing starts it.
            assert!(depth_for_kpm(s, 60.0) > 0.0, "{s:?}");
            // A typical pace lands part-way, not at the floor.
            let typical = depth_for_kpm(s, 100.0);
            assert!(typical > 0.0 && typical < 0.75, "{s:?}: {typical}");
        }
        // Medium is about halfway at a normal steady pace, and full only when intense.
        let mid = depth_for_kpm(NeuralSensitivity::Medium, 110.0);
        assert!((0.4..=0.6).contains(&mid), "{mid}");
        assert!(depth_for_kpm(NeuralSensitivity::Medium, 150.0) < 1.0);
        assert!(close(depth_for_kpm(NeuralSensitivity::Medium, 250.0), 1.0));
        // Higher sensitivity slows more at the same pace.
        assert!(
            depth_for_kpm(NeuralSensitivity::High, 90.0)
                > depth_for_kpm(NeuralSensitivity::Medium, 90.0)
        );
        assert!(
            depth_for_kpm(NeuralSensitivity::Medium, 90.0)
                > depth_for_kpm(NeuralSensitivity::Low, 90.0)
        );
    }

    #[test]
    fn a_typical_pace_lands_between_full_speed_and_the_floor() {
        let c = Config {
            floor_pct: 90.0,
            ..cfg()
        };
        let mut n = NeuralFlow::default();
        run(&mut n, &c, 0, 120, 5.0, steady(120), none);
        let v = n.target_kmh();
        assert!(v < 5.0 && v > 4.5 + 1e-3, "{v}");
    }

    #[test]
    fn a_held_key_is_capped() {
        let mut n = NeuralFlow::default();
        run(&mut n, &cfg(), 0, 25, 3.0, |_| 30, none);
        assert!(n.keys_per_min() <= MAX_KEYS_PER_TICK as f32 * 60.0 + 1e-3);
    }

    #[test]
    fn disabled_does_nothing() {
        let mut n = NeuralFlow::default();
        let off = Config {
            enabled: false,
            ..cfg()
        };
        assert!(run(&mut n, &off, 0, 120, 3.0, steady(300), none).is_empty());
        assert!(close(n.speed_for(3.0, 0, false), 3.0));
    }

    #[test]
    fn migration_from_the_old_settings() {
        // The old default (99% floor, threshold 0.25): the new defaults.
        assert_eq!(
            migrate_legacy(Some(1.0), Some(0.25)),
            (90.0, NeuralSensitivity::Medium)
        );
        // Nothing saved.
        assert_eq!(
            migrate_legacy(None, None),
            (90.0, NeuralSensitivity::Medium)
        );
        // An 80% floor proved too drastic: 90%.
        assert_eq!(
            migrate_legacy(Some(20.0), Some(30.0)),
            (90.0, NeuralSensitivity::Medium)
        );
        // 90 to 95 is kept.
        assert_eq!(migrate_legacy(Some(7.0), Some(30.0)).0, 93.0);
        // Thresholds pick the nearest level.
        assert_eq!(
            migrate_legacy(Some(10.0), Some(1.0)).1,
            NeuralSensitivity::High
        );
        assert_eq!(
            migrate_legacy(Some(10.0), Some(100.0)).1,
            NeuralSensitivity::Low
        );
        assert_eq!(
            migrate_legacy(Some(f32::NAN), Some(f32::NAN)),
            (90.0, NeuralSensitivity::Medium)
        );
    }

    #[test]
    fn sensitivity_strings_round_trip() {
        for s in [
            NeuralSensitivity::Low,
            NeuralSensitivity::Medium,
            NeuralSensitivity::High,
        ] {
            assert_eq!(NeuralSensitivity::parse(s.as_str()), Some(s));
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                format!("\"{}\"", s.as_str())
            );
        }
        assert_eq!(
            NeuralSensitivity::parse("\"High\""),
            Some(NeuralSensitivity::High)
        );
        assert_eq!(NeuralSensitivity::parse("x"), None);
    }
}
