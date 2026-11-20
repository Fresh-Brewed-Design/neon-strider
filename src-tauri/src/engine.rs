//! The control engine: what the treadmill should be doing, and the next packet
//! that gets it there.
//!
//! [`OrchestratorState`] is the one shared state, behind
//! `bluetooth::ORCHESTRATOR`: the deck's last telemetry, the manual and program
//! intents, the session being recorded, and the start and stop bookkeeping.
//! [`ProtocolEngine`] turns it into at most one packet per tick of the protocol
//! worker.
//!
//! Speeds: intents are calibrated (physical) km/h; the deck speaks hardware
//! units of 0.1 km/h. [`calculate_hardware_bytes`] and
//! [`OrchestratorState::to_physical_speed`] convert between the two using the
//! calibration points.

use crate::calibration;
use crate::persistence::{self, Session};
use crate::programs::{Program, ProgramPhase, ProgramRun};
use crate::protocol::{
    self, CalibrationPoint, Control, ControlIntent, HandshakeStep, MachineStatus,
};
use std::fmt;
use std::time::{Duration, Instant};

/// How long the deck may sit paused before the session is saved (and the user
/// told so). The session stays open: a resume continues it.
const PAUSE_SAVE_AFTER: Duration = Duration::from_secs(300);

/// Below this a speed means "belt still" (km/h, physical or hardware).
pub(crate) const STILL_KMH: f32 = 0.05;
/// A stopped deck is only auto-started for a target above this.
const AUTO_START_MIN_KMH: f32 = 0.1;
/// Manual input wins over a running program for this long.
pub(crate) const MANUAL_LOCKOUT: Duration = Duration::from_secs(5);
/// Least time between the last write and the next command packet.
const COMMAND_GAP: Duration = Duration::from_millis(150);
/// Least time between the deck's Ready (or first Pause) and the Start (or
/// second Pause) of the handshake.
const HANDSHAKE_PRIME_GAP: Duration = Duration::from_millis(150);
const HANDSHAKE_ACT_GAP: Duration = Duration::from_millis(400);
/// How often the deck is asked for a status frame.
/// A nack or echo this soon after a target packet is the deck's answer to it.
const TARGET_REPLY_WINDOW: Duration = Duration::from_secs(2);
/// How long the note on an adjusted or refused target stays up.
const TARGET_NOTE_SHOWN: Duration = Duration::from_secs(8);
const POLL_EVERY: Duration = Duration::from_millis(600);
/// While a stop is in flight it is repeated this often...
const STOP_REISSUE_INTERVAL: Duration = Duration::from_secs(1);
/// ...for at most this long, if the deck never reports the belt at rest.
const STOP_TIMEOUT: Duration = Duration::from_secs(20);
/// Incline levels above this are never sent, whatever the settings say.
const INCLINE_LEVEL_LIMIT: u8 = 20;
/// A sample is recorded at most this often...
const SAMPLE_EVERY: Duration = Duration::from_secs(1);
/// ...and the session row rewritten at most this often, so a crash loses little.
const SESSION_FLUSH_EVERY: Duration = Duration::from_secs(5);
/// A deck timer at or under this on connect is an idle deck, not a workout.
const RESUME_MARGIN_S: u16 = 3;
/// A link lost within this long of the last telemetry, with the belt moving,
/// is a dropout: the intents and session are kept for the reconnect.
const DROPOUT_GRACE: Duration = Duration::from_secs(30);
/// Physics steps longer than this are a stall (sleep, lost link), not walking.
const MAX_PHYSICS_STEP_S: f64 = 5.0;
/// Stride used when there is no calibration, cm.
const DEFAULT_STRIDE_CM: f32 = 74.0;
/// Slack on top of the deck's timer when matching a session to the deck's
/// current workout: the deck holds its clock while paused.
const RESUME_SLACK_MS: i64 = 2 * 60 * 60 * 1000;

/// `now` minus `ago`, or `now` itself if the clock doesn't reach back that far.
fn before(now: Instant, ago: Duration) -> Instant {
    now.checked_sub(ago).unwrap_or(now)
}

/// A control verb from a window, the HUD or a keyboard shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckAction {
    SpeedUp,
    SpeedDown,
    InclineUp,
    InclineDown,
    /// Start or resume the belt.
    Start,
    Pause,
    /// Pause a moving belt, otherwise start it.
    ToggleStart,
    /// Stop the belt and end the session.
    Stop,
}

impl DeckAction {
    const ALL: [DeckAction; 8] = [
        DeckAction::SpeedUp,
        DeckAction::SpeedDown,
        DeckAction::InclineUp,
        DeckAction::InclineDown,
        DeckAction::Start,
        DeckAction::Pause,
        DeckAction::ToggleStart,
        DeckAction::Stop,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DeckAction::SpeedUp => "speed-up",
            DeckAction::SpeedDown => "speed-down",
            DeckAction::InclineUp => "incline-up",
            DeckAction::InclineDown => "incline-down",
            DeckAction::Start => "start",
            DeckAction::Pause => "pause",
            DeckAction::ToggleStart => "toggle-start",
            DeckAction::Stop => "stop",
        }
    }

    /// The action named `s` (the names `as_str` gives).
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.as_str() == s)
    }
}

impl fmt::Display for DeckAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which intent [`OrchestratorState::resolve_base`] picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSource {
    /// The manual intent, within the lockout after a manual change.
    Manual,
    /// The manual intent, after the lockout, with no program to defer to.
    ManualIdle,
    Program,
    /// No intent at all: the engine leaves the deck alone.
    Unset,
}

impl fmt::Display for TargetSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TargetSource::Manual => "MANUAL",
            TargetSource::ManualIdle => "MANUAL_IDLE",
            TargetSource::Program => "PROGRAM",
            TargetSource::Unset => "NONE",
        })
    }
}

/// The session being recorded, as live totals.
#[derive(Default)]
pub struct SessionState {
    pub active_session_id: Option<i64>,
    /// Mirrors the deck's own workout timer.
    pub duration_seconds: i64,
    pub distance_meters: f64,
    pub steps: f64,
    pub calories: f32,
    pub last_physics_update: Option<Instant>,
    pub last_db_flush: Option<Instant>,
    pub last_sample_time: Option<Instant>,
    /// The deck's timer when the session began.
    pub hardware_time_offset: Option<u16>,
    // Accumulators for the saved averages (see `session_averages`).
    pub raw_distance_meters: f64,
    pub moving_seconds: f64,
    pub incline_pct_seconds: f64,
}

pub struct OrchestratorState {
    pub manual_intent: Option<ControlIntent>,
    pub last_manual_interaction: Instant,
    pub program_intent: Option<ControlIntent>,

    // The deck's latest telemetry, raw.
    pub last_status: MachineStatus,
    pub hardware_speed: f32,
    pub hardware_incline: u8,
    pub hardware_time: u16,

    /// The target packet last sent (speed byte, incline byte, when) and not yet
    /// echoed or nacked.
    pub(crate) target_pending: Option<(u8, u8, Instant)>,
    /// What the deck did with the last target when it was not what we asked:
    /// plain words and when, for the windows to show for a while.
    pub(crate) target_note: Option<(String, Instant)>,

    pub handshake_step: HandshakeStep,
    /// Whether the handshake under way is a pause (true) or a start. Decided
    /// once, at its first packet: the deck can report Paused between the two
    /// packets, and re-deciding then would turn a pause into a start.
    pub(crate) handshake_is_pause: bool,
    pub last_handshake_tx: Option<Instant>,

    /// When the last packet was written.
    pub last_cmd_time: Instant,
    pub last_poll_time: Instant,
    /// Bumped for each protocol worker; an older worker that sees another id stops.
    pub worker_id: u64,

    pub calibration_points: Vec<CalibrationPoint>,
    pub min_speed: f32,
    pub max_speed: f32,
    pub is_connecting: bool,
    /// No telemetry yet on this connection: nothing is sent until there is.
    pub awaiting_first_packet: bool,
    /// When the last status frame arrived. The link can stay up for a while
    /// after the deck loses power, leaving `last_status` stale.
    pub last_frame_at: Option<Instant>,
    /// The resume-vs-new-session decision for this connection is made (it
    /// needs the deck's first telemetry). Reset on disconnect.
    pub session_evaluated_on_connect: bool,
    /// When the deck entered its current pause; `None` while not paused.
    pub paused_since: Option<Instant>,
    /// The long-pause save already happened for this pause.
    pub pause_saved: bool,
    /// A stop is in flight: the stop is repeated, and the automatic start,
    /// pause and adoption logic held off, until the deck reports the belt at rest.
    pub stop_requested: bool,
    pub tilt_multiplier: f32,
    pub incline_offset: f32,
    /// Highest incline level the deck supports (`AppConfig::max_incline`).
    pub max_incline: u8,
    /// Body weight (kg) for the calorie estimate (`AppConfig::body_mass_kg`).
    pub body_mass_kg: f32,

    pub session: SessionState,

    // Neural flow (typing-aware pace, see neural.rs). `focus_stability_enabled`
    // is its on switch; the name follows the stored setting.
    pub focus_stability_enabled: bool,
    pub neural_floor_pct: f32,
    pub neural_sensitivity: crate::neural::NeuralSensitivity,
    pub neural: crate::neural::NeuralFlow,
    /// Bumped on every speed the user sets by hand (not programs, not quiet
    /// mode): Neural flow takes it as the new base.
    pub speed_hand_epoch: u64,
    /// When Neural flow last changed the target, so the protocol worker
    /// doesn't toast the deck's speed change that follows.
    pub neural_changed_at: Option<Instant>,

    /// An Engage/start is waiting for the deck to report Starting or Running.
    pub start_requested_at: Option<Instant>,
    /// The last start went unanswered: stop re-sending it (auto-start and
    /// auto-resume) until the user asks again.
    pub start_blocked: bool,
    /// When the engine last sent the deck a Start (the handshake's second
    /// half) that the deck hasn't answered yet: motion then is the app's own
    /// start, not one at the treadmill (`programs::watch_for_deck_start`).
    pub start_sent_at: Option<Instant>,
    /// The raw 0x51 status byte when it is not one we know; None when known.
    pub unknown_status_code: Option<u8>,
    /// The deck reports standby (`protocol::STATUS_STANDBY`): off or asleep.
    /// It refuses every start, so none is sent (see `start_refusal`).
    pub deck_standby: bool,
    /// The belt never starts by itself after a restart, reconnect or update:
    /// set for each new connection, this holds off the automatic start and
    /// resume until the deck is seen moving on this connection or the user asks
    /// for a start (Engage, GO, a program, a speed set by hand). A belt that
    /// never stopped carries on; a still one waits for the user. A program
    /// whose next step would start a still belt sets it again while it waits
    /// for GO (`programs::ProgramWait`).
    pub hold_auto_start: bool,
    /// A program ended (END, or its last step) while the belt was not moving:
    /// the speed from before the program, for the next Engage or RE-JACK to
    /// start at. Speed only: the incline lives in the manual intent (which
    /// holds the belt still at speed 0 until then), so the tilt shown, the
    /// tilt nudged and the tilt Engage starts at are one value.
    /// Dropped by a speed set by hand, a stop, a new program or a full reset.
    pub engage_speed: Option<f32>,

    // Saved programs and the one that is running (see programs.rs).
    pub programs: Vec<Program>,
    pub program_next_id: u32,
    pub program_last_run_id: Option<u32>,
    /// A program toast raised between ticks (a start at the treadmill taken
    /// as GO), for the worker to send at once (`take_program_toast`).
    pub program_toast: Option<crate::programs::ProgramToast>,
    pub program: Option<ProgramRun>,

    /// `calibration_points` sorted by hardware speed.
    pub sorted_points: Vec<CalibrationPoint>,
    /// Physical speed for each hardware speed, indexed by `hw_speed * 10`.
    pub hw_to_physical_lookup: Vec<f32>,

    /// A Pause or Stop to send as soon as the command gap allows.
    pub pending_control: Option<Control>,

    /// The speed byte the deck set by itself (its remote or its own buttons)
    /// since the app last sent a target, if it did
    /// (`ProtocolEngine::notice_deck_changes`): the next speed or incline
    /// press steps from it.
    pub deck_set_speed: Option<u8>,
    /// The incline level the deck set by itself, as for `deck_set_speed`.
    pub deck_set_incline: Option<u8>,
    /// A press stepped from the deck's own setting: send the next target even
    /// when its bytes match the last ones sent, since the deck runs others.
    pub resend_target: bool,

    /// Tests only: the simulated instant `now()` reads instead of the real
    /// clock, so ticked time and the engine's own clock reads agree.
    #[cfg(test)]
    pub(crate) test_now: Option<Instant>,
}

impl Default for OrchestratorState {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            manual_intent: None,
            last_manual_interaction: before(now, Duration::from_secs(3600)),
            program_intent: None,
            last_status: MachineStatus::Stopped,
            hardware_speed: 0.0,
            hardware_incline: 0,
            hardware_time: 0,
            handshake_step: HandshakeStep::Idle,
            handshake_is_pause: false,
            last_handshake_tx: None,
            last_cmd_time: before(now, Duration::from_secs(1)),
            last_poll_time: before(now, Duration::from_secs(2)),
            worker_id: 0,
            calibration_points: Vec::new(),
            min_speed: 0.6,
            max_speed: 12.0,
            is_connecting: false,
            awaiting_first_packet: true,
            last_frame_at: None,
            session_evaluated_on_connect: false,
            paused_since: None,
            pause_saved: false,
            stop_requested: false,
            tilt_multiplier: 1.0,
            incline_offset: 0.0,
            max_incline: 15,
            body_mass_kg: persistence::DEFAULT_BODY_MASS_KG,
            session: SessionState::default(),
            focus_stability_enabled: false,
            neural_floor_pct: crate::neural::DEFAULT_FLOOR_PCT,
            neural_sensitivity: crate::neural::NeuralSensitivity::Medium,
            neural: crate::neural::NeuralFlow::default(),
            speed_hand_epoch: 0,
            neural_changed_at: None,
            start_requested_at: None,
            start_blocked: false,
            start_sent_at: None,
            unknown_status_code: None,
            target_pending: None,
            target_note: None,
            deck_standby: false,
            hold_auto_start: true,
            engage_speed: None,
            programs: Vec::new(),
            program_next_id: 1,
            program_last_run_id: None,
            program_toast: None,
            program: None,
            sorted_points: Vec::new(),
            hw_to_physical_lookup: Vec::new(),
            pending_control: None,
            deck_set_speed: None,
            deck_set_incline: None,
            resend_target: false,
            #[cfg(test)]
            test_now: None,
        }
    }
}

/// Whether a saved session can be the one the deck's running workout belongs to.
fn resumable(s: &Session, deck_time_s: u16, now_ms: i64, local_midnight_ms: i64) -> bool {
    let workout_began = now_ms - deck_time_s as i64 * 1000 - RESUME_SLACK_MS;
    s.strava_id.is_none() && s.start_time >= local_midnight_ms && s.end_time >= workout_began
}

/// Gross energy (kcal) burned walking for `dt_s` seconds at a calibrated
/// `speed_kmh` and calibrated `grade_pct` (negative grade counts as level).
///
/// ACSM walking equation (ACSM's Guidelines for Exercise Testing and
/// Prescription): VO2 (mL/kg/min) = 0.1*S + 1.8*S*G + 3.5, with S in m/min and
/// G as a fraction. Then kcal/min = VO2 * kg / 1000 * 5 (about 5 kcal per litre
/// of O2). This is gross energy, including the resting 3.5 term, which matches
/// what fitness apps and Strava report. The equation is validated for roughly
/// 1.9 to 6 km/h; it is applied outside that range too.
fn walking_kcal(speed_kmh: f64, grade_pct: f64, body_mass_kg: f64, dt_s: f64) -> f64 {
    // Written as negations so NaN counts as "not positive".
    if !(speed_kmh > 0.0 && dt_s > 0.0 && body_mass_kg > 0.0) {
        return 0.0;
    }
    let s = speed_kmh * 1000.0 / 60.0; // m/min
    let g = (grade_pct / 100.0).max(0.0);
    let vo2 = 0.1 * s + 1.8 * s * g + 3.5;
    vo2 * body_mass_kg / 1000.0 * 5.0 * dt_s / 60.0
}

/// Linear interpolation of `y` over calibration points ordered by `x`.
/// Beyond either end it holds the end point's value. `None` when `at` falls in
/// no segment (no points, or NaN).
fn interpolate(
    points: &[CalibrationPoint],
    x: impl Fn(&CalibrationPoint) -> f32,
    y: impl Fn(&CalibrationPoint) -> f32,
    at: f32,
) -> Option<f32> {
    let (first, last) = (points.first()?, points.last()?);
    if at <= x(first) {
        return Some(y(first));
    }
    if at >= x(last) {
        return Some(y(last));
    }
    points.windows(2).find_map(|pair| {
        let (p1, p2) = (&pair[0], &pair[1]);
        let (x1, x2) = (x(p1), x(p2));
        (at >= x1 && at <= x2).then(|| {
            let t = if x2 > x1 { (at - x1) / (x2 - x1) } else { 0.0 };
            y(p1) + t * (y(p2) - y(p1))
        })
    })
}

/// A calibration point's physical speed: its hardware speed over its scale.
/// The scale is clamped the same way the speed conversions clamp it, so the
/// points sort on the axis they are applied on.
fn physical_axis_velocity(p: &CalibrationPoint) -> f32 {
    let scale = if p.distance_scale > 0.0 {
        p.distance_scale
            .clamp(calibration::MIN_SCALE, calibration::MAX_SCALE)
    } else {
        1.0
    };
    p.speed / scale
}

/// Tests: a calibration whose scale isn't 1 at the deck's floor. The default
/// `min_speed` of 0.6 (hardware) runs the belt at 1.2 km/h (scale 0.5 there,
/// 0.8 at hw 6.0 = 7.5 km/h).
#[cfg(test)]
pub(crate) fn slow_floor_config() -> persistence::AppConfig {
    let point = |speed: f32, distance_scale: f32| CalibrationPoint {
        speed,
        stride_length_cm: 60.0,
        distance_scale,
    };
    persistence::AppConfig {
        calibration_points: vec![point(0.6, 0.5), point(6.0, 0.8)],
        ..Default::default()
    }
}

/// Calibration points in hardware-speed order, the order both speed lookups
/// walk the curve in.
fn sorted_by_hw_speed(points: &[CalibrationPoint]) -> Vec<CalibrationPoint> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| {
        a.speed
            .partial_cmp(&b.speed)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
}

/// Calibrated km/h for a hardware speed, over points sorted by hardware speed
/// (`sorted_by_hw_speed`): the hardware speed over the scale interpolated at it.
fn hw_to_physical(sorted: &[CalibrationPoint], hw_speed: f32) -> f32 {
    if sorted.is_empty() {
        return hw_speed;
    }
    let scale = interpolate(sorted, |p| p.speed, |p| p.distance_scale, hw_speed)
        .unwrap_or(1.0)
        .clamp(calibration::MIN_SCALE, calibration::MAX_SCALE);
    hw_speed / scale.max(0.1)
}

/// Hardware speed and incline bytes for a physical target, clamped to the
/// deck's `max_hw_speed`. Standalone, so the transmit path, the manual nudges
/// and the tests share it.
///
/// The exact inverse of `to_physical_speed`: the byte whose calibrated km/h is
/// nearest the target (the lowest such byte on a tie). A calibration whose km/h
/// doesn't rise with every byte has no formula inverse, and one worked out on
/// a different curve would send a byte's own km/h as some other byte.
pub(crate) fn calculate_hardware_bytes(
    physical_speed: f32,
    incline: u8,
    points: &[CalibrationPoint],
    max_hw_speed: f32,
) -> (u8, u8) {
    let max_byte = max_hw_speed * 10.0;
    if points.is_empty() {
        let hw_speed = (physical_speed * 10.0).round().clamp(0.0, max_byte) as u8;
        return (hw_speed, incline);
    }

    let sorted = sorted_by_hw_speed(points);
    let top = max_byte.round().clamp(0.0, u8::MAX as f32) as u8;
    let gap = |b: u8| (hw_to_physical(&sorted, b as f32 / 10.0) - physical_speed).abs();
    let hw_speed = (0..=top)
        .min_by(|a, b| {
            gap(*a)
                .partial_cmp(&gap(*b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0);
    (hw_speed, incline)
}

impl OrchestratorState {
    /// The engine's clock: every read of "now" that doesn't come from the
    /// tick goes through here. The real clock, except in tests, which can set
    /// a simulated instant (`set_test_now`).
    pub(crate) fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(now) = self.test_now {
            return now;
        }
        Instant::now()
    }

    /// Tests only: make `now()` return `now` from here on.
    #[cfg(test)]
    pub(crate) fn set_test_now(&mut self, now: Instant) {
        self.test_now = Some(now);
    }

    /// Apply the stored settings. The one way settings reach the engine: at
    /// startup, on every settings change, and on each new connection.
    ///
    /// Intents hold hardware levels, so a new tilt or offset never changes the
    /// incline byte being sent: the deck stays where it is and only its
    /// percentage is recomputed (`incline_pct`). A program step's level is
    /// worked out from its percentage when the step starts.
    pub fn sync_config(&mut self, config: &persistence::AppConfig) {
        self.calibration_points = config.calibration_points.clone();
        self.min_speed = config.min_speed;
        self.max_speed = config.max_speed;
        self.max_incline = config.max_incline;
        self.tilt_multiplier = config.tilt_multiplier;
        self.incline_offset = config.incline_offset;
        self.body_mass_kg = config
            .body_mass_kg
            .clamp(persistence::MIN_BODY_MASS_KG, persistence::MAX_BODY_MASS_KG);
        self.focus_stability_enabled = config.focus_stability_enabled;
        self.neural_floor_pct = crate::neural::clamp_floor_pct(config.neural_floor_pct);
        self.neural_sensitivity = config.neural_sensitivity;

        self.sorted_points = sorted_by_hw_speed(&self.calibration_points);
        self.rebuild_lookup_table();
    }

    /// Every field `sync_config` sets, for tests that compare two states.
    #[cfg(test)]
    pub(crate) fn config_fingerprint(&self) -> String {
        format!(
            "{:?}",
            (
                (&self.calibration_points, &self.sorted_points),
                &self.hw_to_physical_lookup,
                (self.min_speed, self.max_speed, self.max_incline),
                (self.tilt_multiplier, self.incline_offset),
                self.body_mass_kg,
                (
                    self.focus_stability_enabled,
                    self.neural_floor_pct,
                    self.neural_sensitivity,
                ),
            )
        )
    }

    fn rebuild_lookup_table(&mut self) {
        // A little headroom past the top speed.
        let len = (self.max_speed * 10.0).round() as usize + 20;
        self.hw_to_physical_lookup = (0..len)
            .map(|i| self.calculate_physical_speed_uncached(i as f32 / 10.0))
            .collect();
        log::debug!(
            "Speed lookup table rebuilt (0 to {:.1} km/h)",
            self.max_speed
        );
    }

    fn calculate_physical_speed_uncached(&self, hw_speed: f32) -> f32 {
        hw_to_physical(&self.sorted_points, hw_speed)
    }

    /// The slowest calibrated km/h the belt runs at: `min_speed` is the deck's
    /// own floor in hardware units, so every physical speed is bounded by this.
    pub fn min_physical_speed(&self) -> f32 {
        self.to_physical_speed(self.min_speed)
    }

    /// Calibrated km/h for a hardware speed.
    pub fn to_physical_speed(&self, hw_speed: f32) -> f32 {
        let idx = (hw_speed * 10.0).round() as usize;
        match self.hw_to_physical_lookup.get(idx) {
            Some(v) => *v,
            None => self.calculate_physical_speed_uncached(hw_speed),
        }
    }

    /// Stride length (cm) at a physical speed, from the calibration points.
    pub fn calculate_stride_length(&self, physical_speed: f32) -> f32 {
        if self.sorted_points.is_empty() {
            return DEFAULT_STRIDE_CM;
        }
        interpolate(
            &self.sorted_points,
            physical_axis_velocity,
            |p| p.stride_length_cm,
            physical_speed,
        )
        .map_or(DEFAULT_STRIDE_CM, |cm| {
            cm.clamp(calibration::MIN_STRIDE_CM, calibration::MAX_STRIDE_CM)
        })
    }

    /// Calibrated metres per uncalibrated metre: from the resumed session's own
    /// totals when it has them, else from the calibration curve at the current
    /// (or a typical walking) speed.
    fn distance_calibration_ratio(&self, resumed: Option<&Session>) -> f64 {
        if let Some(s) = resumed {
            if s.distance_meters > 0.0 && s.raw_distance_meters > 0.0 {
                return s.distance_meters / s.raw_distance_meters;
            }
        }
        let hw = if self.hardware_speed > 0.1 {
            self.hardware_speed
        } else {
            3.0
        };
        let ratio = (self.to_physical_speed(hw) / hw) as f64;
        if ratio.is_finite() && ratio > 0.0 {
            ratio
        } else {
            1.0
        }
    }

    /// Calibrated incline percent for a hardware level.
    pub fn incline_pct(&self, level: u8) -> f32 {
        level as f32 * self.tilt_multiplier + self.incline_offset
    }

    // -----------------------------------------------------------------------
    // Session recording
    // -----------------------------------------------------------------------

    pub fn start_session(&mut self, session_id: i64) {
        let now = self.now();
        self.session = SessionState {
            active_session_id: Some(session_id),
            last_physics_update: Some(now),
            last_sample_time: Some(now),
            // Only what the deck counts from here on belongs to this session.
            hardware_time_offset: Some(self.hardware_time),
            last_db_flush: self.session.last_db_flush,
            ..SessionState::default()
        };
        log::info!(
            "Physical session started: ID {} (Offset: {})",
            session_id,
            self.hardware_time
        );
    }

    pub fn stop_session(&mut self) {
        log::info!(
            "Physical session stopped: ID {:?}",
            self.session.active_session_id
        );
        self.session.active_session_id = None;
        // Without intents the engine has nothing to start the belt again with.
        self.manual_intent = None;
        self.program_intent = None;
    }

    pub fn should_start_session(&self) -> bool {
        self.session.active_session_id.is_none() && self.last_status.is_moving()
    }

    pub fn should_stop_session(&self) -> bool {
        self.session.active_session_id.is_some() && self.last_status == MachineStatus::Stopped
    }

    /// Whether a sample is due (at most one per `SAMPLE_EVERY`); marks it taken.
    pub fn should_record_sample(&mut self) -> bool {
        if self.session.active_session_id.is_none() {
            return false;
        }
        let now = self.now();
        let due = self
            .session
            .last_sample_time
            .is_none_or(|last| now.duration_since(last) >= SAMPLE_EVERY);
        if due {
            self.session.last_sample_time = Some(now);
        }
        due
    }

    /// (avg speed, raw avg speed, avg incline %) for the open session. Speeds
    /// are distance over the deck's session time, which holds while paused, so
    /// they match how merged walks and Strava uploads compute them. Incline is
    /// weighted by time spent moving.
    pub fn session_averages(&self) -> (f32, f32, f32) {
        let s = &self.session;
        let secs = s.duration_seconds.max(0) as f64;
        let per_hour = |m: f64| {
            if secs > 0.0 {
                (m / secs * 3.6) as f32
            } else {
                0.0
            }
        };
        let incline = if s.moving_seconds > 0.0 {
            (s.incline_pct_seconds / s.moving_seconds) as f32
        } else {
            0.0
        };
        (
            per_hour(s.distance_meters),
            per_hour(s.raw_distance_meters),
            incline,
        )
    }

    /// Advance the live session totals to now.
    pub fn update_physics(&mut self) {
        let now = self.now();

        // Duration mirrors the deck's own workout timer: it holds on pause,
        // resets on stop, and is already running when we connect mid-walk. A
        // stopped deck reads 0; the last non-zero value is what gets saved.
        if self.session.active_session_id.is_some() && self.hardware_time > 0 {
            self.session.duration_seconds = self.hardware_time as i64;
        }

        if let Some(last) = self.session.last_physics_update {
            let dt = now.duration_since(last).as_secs_f64();
            if dt > 0.0 && dt < MAX_PHYSICS_STEP_S {
                self.integrate(dt);
            }
        }
        self.session.last_physics_update = Some(now);
    }

    /// Add `dt` seconds at the current belt speed and incline.
    fn integrate(&mut self, dt: f64) {
        let p_speed = self.to_physical_speed(self.hardware_speed);
        let moving = self.last_status.is_moving() && p_speed > STILL_KMH;
        if !moving {
            return;
        }
        let stride_cm = self.calculate_stride_length(p_speed);
        let dist_m = (p_speed as f64 / 3.6) * dt;
        let steps = if stride_cm > 0.0 {
            (dist_m * 100.0) / stride_cm as f64
        } else {
            0.0
        };
        let grade_pct = self.incline_pct(self.hardware_incline);

        let s = &mut self.session;
        s.distance_meters += dist_m;
        s.raw_distance_meters += (self.hardware_speed as f64 / 3.6) * dt;
        s.moving_seconds += dt;
        s.incline_pct_seconds += grade_pct as f64 * dt;
        s.steps += steps;
        s.calories += walking_kcal(
            p_speed as f64,
            grade_pct as f64,
            self.body_mass_kg as f64,
            dt,
        ) as f32;
    }

    /// The open session's totals as a row for `db_update_session`.
    fn session_record(&self, session_id: i64) -> Session {
        let (avg_speed, raw_avg_speed, avg_incline) = self.session_averages();
        Session {
            id: Some(session_id),
            // Not written by an update.
            start_time: 0,
            end_time: 0,
            duration_seconds: self.session.duration_seconds,
            distance_meters: self.session.distance_meters,
            raw_distance_meters: self.session.raw_distance_meters,
            calories: self.session.calories,
            steps: self.session.steps,
            raw_steps: self.session.steps,
            avg_speed,
            raw_avg_speed,
            avg_incline,
            strava_id: None,
            neural_heat: 0.0,
            neural_ticks: 0,
            last_hardware_time: self.hardware_time as i64,
            hardware_time_offset: self.session.hardware_time_offset.unwrap_or(0) as i64,
        }
    }

    /// Decide, on the first telemetry after a connect, whether the deck's current
    /// workout continues a saved session (resume) or is fresh. Runs once per
    /// connection. Distance and steps depend on calibration and never match the
    /// deck exactly, so the deck's timer decides: a running timer means a
    /// workout is under way, so the latest session is resumed if it can be the
    /// same walk (see [`resumable`]), or a new one opened, and its totals are
    /// synced to what the deck reports. An idle deck (timer about 0) leaves the
    /// new session to `should_start_session` once walking starts.
    pub fn evaluate_session_on_connect(
        &mut self,
        app: &tauri::AppHandle,
        deck_time: u16,
        deck_distance_m: f64,
        deck_steps: f64,
        deck_calories: f32,
    ) {
        if self.session_evaluated_on_connect {
            return;
        }
        self.session_evaluated_on_connect = true;

        // A reconnect that kept the session: nothing to decide.
        if self.session.active_session_id.is_some() {
            return;
        }
        if deck_time <= RESUME_MARGIN_S {
            log::info!(
                "Connect: deck idle (t={}s) — fresh session pending.",
                deck_time
            );
            return;
        }

        let now_ms = crate::util::now_ms();
        let midnight_ms = persistence::local_midnight_ms(chrono::Local::now());
        let latest = persistence::get_latest_session(app)
            .filter(|s| resumable(s, deck_time, now_ms, midnight_ms));
        let sid = match latest.as_ref().and_then(|s| s.id) {
            Some(id) => {
                log::info!(
                    "Connect: resuming session {} onto active deck (t={}s).",
                    id,
                    deck_time
                );
                id
            }
            None => match persistence::db_create_session(app) {
                Ok(id) => {
                    log::info!(
                        "Connect: active deck (t={}s), no prior session — new session {}.",
                        deck_time,
                        id
                    );
                    id
                }
                Err(e) => {
                    log::error!("Connect: failed to create session: {}", e);
                    return;
                }
            },
        };

        let now = self.now();
        // The deck reports uncalibrated distance; scale it like live distance.
        let distance_meters = deck_distance_m * self.distance_calibration_ratio(latest.as_ref());
        // Carry the resumed session's incline average forward instead of
        // restarting it from whatever is walked after the reconnect.
        let resumed = latest.filter(|s| s.id == Some(sid));
        let prior_secs = resumed
            .as_ref()
            .map_or(0.0, |s| s.duration_seconds.max(0) as f64);
        let s = &mut self.session;
        s.active_session_id = Some(sid);
        s.duration_seconds = deck_time as i64;
        s.distance_meters = distance_meters;
        s.steps = deck_steps;
        s.calories = deck_calories;
        s.raw_distance_meters = deck_distance_m;
        s.moving_seconds = prior_secs;
        s.incline_pct_seconds = resumed.map_or(0.0, |r| r.avg_incline as f64) * prior_secs;
        s.last_physics_update = Some(now);
        s.last_sample_time = Some(now);
        // Write the synced totals promptly.
        s.last_db_flush = None;
    }

    /// After each telemetry frame: open, sample, checkpoint and close the
    /// session as the deck's state requires.
    pub fn manage_session_lifecycle(&mut self, app: &tauri::AppHandle) {
        let now = self.now();

        if self.should_start_session() {
            match persistence::db_create_session(app) {
                Ok(new_id) => {
                    self.start_session(new_id);
                    log::info!("HEADLESS START: Created Session {}", new_id);
                }
                Err(e) => log::error!("Failed to create session: {}", e),
            }
        }

        if self.should_record_sample() {
            self.record_sample(app, now);
        }

        // A long pause saves the session but keeps it open: the deck holds its
        // clock across a pause, so a new session on resume would re-adopt the
        // deck's whole time and count it twice. Only a stop (which resets the
        // deck's clock) ends it.
        if self.last_status == MachineStatus::Paused {
            if self.session.active_session_id.is_some() {
                let started = *self.paused_since.get_or_insert(now);
                if !self.pause_saved && now.duration_since(started) >= PAUSE_SAVE_AFTER {
                    self.save_and_notify(app, "extended pause", false);
                    self.pause_saved = true;
                }
            }
        } else {
            self.paused_since = None;
            self.pause_saved = false;
        }

        if self.should_stop_session() {
            self.save_and_notify(app, "stopped", true);
        }
    }

    /// Write one sample, and the session row when a flush is due, off the
    /// async runtime.
    fn record_sample(&mut self, app: &tauri::AppHandle, now: Instant) {
        let Some(session_id) = self.session.active_session_id else {
            return;
        };
        let sample = persistence::Sample {
            timestamp: None,
            speed: self.to_physical_speed(self.hardware_speed),
            // A calibrated grade: uploads, TCX altitude and the History plot
            // all read it as one.
            incline: self.incline_pct(self.hardware_incline),
            distance: self.session.distance_meters as f32,
            raw_distance: self.session.distance_meters as f32,
            steps: self.session.steps as f32,
            raw_steps: self.session.steps as f32,
            raw_speed: self.hardware_speed,
        };
        let flush = self
            .session
            .last_db_flush
            .is_none_or(|last| now.duration_since(last) >= SESSION_FLUSH_EVERY);
        let record = flush.then(|| self.session_record(session_id));
        if flush {
            self.session.last_db_flush = Some(now);
        }

        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = persistence::db_add_sample(&app, session_id, sample) {
                log::debug!("Sample not saved: {}", e);
            }
            if let Some(record) = record {
                if let Err(e) = persistence::db_update_session(&app, session_id, record) {
                    log::debug!("Session {} not updated: {}", session_id, e);
                }
            }
        });
    }

    /// Persist the active session and show "session saved" on the HUD. With
    /// `end` (a stop) the session is also closed, so the next walk starts a new
    /// one; without (a long pause) it stays open for the resume.
    fn save_and_notify(&mut self, app: &tauri::AppHandle, reason: &str, end: bool) {
        let Some(session_id) = self.session.active_session_id else {
            return;
        };
        let duration = crate::util::format_duration(self.session.duration_seconds);
        let km = self.session.distance_meters / 1000.0;
        log::info!(
            "Finalizing Session {} ({}) — {} · {:.2} km",
            session_id,
            reason,
            duration,
            km
        );

        let record = self.session_record(session_id);
        let app_c = app.clone();
        // A checkpoint keeps the session live, and the snapshot adds the live
        // distance itself, so it must stay out of the cached daily base.
        let exclude = if end { None } else { Some(session_id) };
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = persistence::db_update_session(&app_c, session_id, record) {
                log::warn!("Session {} not saved: {}", session_id, e);
            }
            // After the write, so the day's total includes what was just saved.
            crate::snapshot::refresh_daily_base(&app_c, exclude);
        });

        crate::hud::toast(
            app,
            serde_json::json!({
                "type": "STATUS",
                "message": format!("SESSION SAVED · {} · {:.2} km", duration, km),
                "isActionable": true,
            }),
        );

        if end {
            self.stop_session();
            // Let the Strava worker look soon (no-op unless auto-sync is on).
            crate::strava::notify_session_ended();
        }
    }

    // -----------------------------------------------------------------------
    // Intents and actions
    // -----------------------------------------------------------------------

    /// Set the manual speed and/or incline. `by_hand`: the user chose this
    /// speed, so Neural flow takes it as the new base (quiet mode on a call
    /// passes false). While a program walks, this edits its live step target.
    pub(crate) fn set_intent(&mut self, speed: Option<f32>, incline: Option<u8>, by_hand: bool) {
        if by_hand && speed.is_some() {
            self.note_hand_speed(self.now());
        }
        if self.adjust_program_target(speed, incline) {
            return;
        }
        let mut current = self.manual_intent.unwrap_or(ControlIntent {
            speed: 0.0,
            incline: 0,
        });
        if let Some(s) = speed {
            current.speed = s;
        }
        if let Some(i) = incline {
            current.incline = i;
        }
        self.manual_intent = Some(current);
        self.last_manual_interaction = self.now();
    }

    pub fn trigger_action(&mut self, action: DeckAction) {
        let now = self.now();
        log::info!("TRIGGER ACTION: {}", action);
        match action {
            DeckAction::SpeedUp => self.nudge_manual_speed(1),
            DeckAction::SpeedDown => self.nudge_manual_speed(-1),
            DeckAction::InclineUp => self.nudge_incline(true, now),
            DeckAction::InclineDown => self.nudge_incline(false, now),
            DeckAction::Start => self.start_or_go(now),
            DeckAction::Pause => self.do_pause(),
            DeckAction::ToggleStart => {
                if self.last_status.is_moving() {
                    self.do_pause();
                } else {
                    self.start_or_go(now);
                }
            }
            DeckAction::Stop => self.do_stop(now),
        }
    }

    /// Nudge the manual speed by one hardware unit in `dir` (+1 or -1).
    ///
    /// The intent is physical km/h, but the deck only acts on the hardware
    /// byte, and the engine only sends when that byte changes. With a
    /// calibration scale below 1, a 0.1 km/h step can round to the same byte
    /// and do nothing; stepping the byte itself guarantees every press reaches
    /// the deck. The byte's km/h comes from the curve the transmit path
    /// inverts exactly, so the stored intent is sent as that byte and the next
    /// press steps on from it.
    fn nudge_manual_speed(&mut self, dir: i32) {
        // After a program ended on a still belt the target shown is the speed
        // Engage will use: a nudge steps from that, as a speed set by hand.
        // Otherwise it steps from what the deck runs (`nudge_base`).
        let (cur, deck_byte) = match self.engage_preview() {
            Some(preview) => (preview, None),
            None => self.nudge_base(),
        };
        let hw_byte = |phys: f32| {
            calculate_hardware_bytes(phys, cur.incline, &self.calibration_points, self.max_speed).0
        };
        let cur_byte = deck_byte.unwrap_or_else(|| hw_byte(cur.speed)) as i32;
        let max_byte = (self.max_speed * 10.0).round() as i32;
        let mut target_byte = (cur_byte + dir).clamp(0, max_byte);
        // Two bytes with exactly the same km/h both send as the lower one:
        // step past it so the press still moves the deck.
        while target_byte > 0
            && target_byte < max_byte
            && target_byte != cur_byte
            && hw_byte(self.physical_for_byte(target_byte as u8, cur.incline)) as i32 != target_byte
        {
            target_byte += dir;
        }

        // A press at the top speed still takes the deck's own setting.
        if target_byte != cur_byte || deck_byte.is_some() {
            let phys = self.physical_for_byte(target_byte as u8, cur.incline);
            self.store_editable_intent(ControlIntent {
                speed: phys,
                incline: cur.incline,
            });
            self.note_hand_speed(self.now());
        }
        log::debug!(
            "SPEED NUDGE dir={} hw_byte {} -> {} (status={:?})",
            dir,
            cur_byte,
            target_byte,
            self.last_status
        );
        self.stamp_manual_interaction(self.now());
    }

    /// The physical speed of a hardware `byte`, which the transmit path
    /// (`calculate_hardware_bytes`, its exact inverse) turns back into that
    /// byte. The incline doesn't change the speed curve.
    fn physical_for_byte(&self, byte: u8, _incline: u8) -> f32 {
        self.to_physical_speed(byte as f32 / 10.0).max(0.0)
    }

    fn nudge_incline(&mut self, up: bool, now: Instant) {
        let (mut cur, _) = self.nudge_base();
        cur.incline = if up {
            (cur.incline + 1).min(self.max_incline)
        } else {
            cur.incline.saturating_sub(1)
        };
        self.store_editable_intent(cur);
        self.stamp_manual_interaction(now);
    }

    /// What a speed or incline press steps from: the editable intent, with
    /// the speed and incline the deck set by itself (its remote or buttons)
    /// since the app last sent a target in place of the intent's stale ones.
    /// The press then steps from what the deck runs, and doesn't put the
    /// other value back to the one from before the remote. Takes the deck's
    /// settings, since the press stores them in the intent; also returns the
    /// deck's speed byte when the speed came from the deck.
    fn nudge_base(&mut self) -> (ControlIntent, Option<u8>) {
        let mut cur = self.editable_intent();
        let speed = self.deck_set_speed.take();
        let incline = self.deck_set_incline.take();
        if let Some(byte) = speed {
            cur.speed = self.physical_for_byte(byte, cur.incline);
        }
        if let Some(level) = incline {
            cur.incline = level.min(self.max_incline);
        }
        if speed.is_some() || incline.is_some() {
            self.resend_target = true;
            log::info!(
                "Press steps from the deck's own setting: {:.2} km/h, incline {}",
                cur.speed,
                cur.incline
            );
        }
        (cur, speed)
    }

    /// The intent manual nudges act on: the live step target while a program
    /// is walking, otherwise the manual intent.
    fn editable_intent(&self) -> ControlIntent {
        match &self.program {
            Some(r) if r.phase == ProgramPhase::Walking => r.target,
            _ => self.manual_intent.unwrap_or(ControlIntent {
                speed: 0.0,
                incline: 0,
            }),
        }
    }

    fn store_editable_intent(&mut self, intent: ControlIntent) {
        if !self.adjust_program_target(Some(intent.speed), Some(intent.incline)) {
            self.manual_intent = Some(intent);
        }
    }

    /// Manual input resets the manual lockout, except while a program is
    /// walking: then it edited the program target and must not mask it.
    fn stamp_manual_interaction(&mut self, now: Instant) {
        if !self.program_walking() {
            self.last_manual_interaction = now;
        }
    }

    /// Pause the belt. The deck holds its clock, so a later start resumes the
    /// same session.
    fn do_pause(&mut self) {
        self.pending_control = Some(Control::Pause);
    }

    /// Engage (or the start shortcut). On a paused program (one waiting for GO
    /// included) it is the program's GO, so the belt starts at the step's speed
    /// and the program doesn't pause it again a moment later.
    fn start_or_go(&mut self, now: Instant) {
        if self.program_walking() && self.program.as_ref().is_some_and(|r| r.paused) {
            if let Err(e) = self.resume_program(now) {
                log::warn!("Engage on a paused program: {}", e);
            }
            return;
        }
        self.do_start(now);
    }

    /// Start or resume: prime the handshake, at no less than the belt's floor
    /// (`min_physical_speed`) so the belt actually moves.
    /// After a program ended on a still belt, it starts at the speed from
    /// before the program (`engage_speed`), at the incline shown.
    fn do_start(&mut self, now: Instant) {
        if !self.arm_start_watch(now) {
            return;
        }
        let current = self.manual_intent.unwrap_or(ControlIntent {
            speed: 0.0,
            incline: 0,
        });
        let engage = self.engage_speed.take();
        let floor = self.min_physical_speed();
        if current.speed < floor {
            self.manual_intent = Some(ControlIntent {
                speed: engage.unwrap_or(floor).max(floor),
                incline: current.incline,
            });
        }
        self.handshake_step = HandshakeStep::Priming;
        self.last_manual_interaction = now;
    }

    /// The speed the next Engage will start the still belt at, when it is the
    /// speed a program left for it (`engage_speed`, see `do_start`), with the
    /// incline the manual intent holds (the one Engage uses). For display (the
    /// snapshot's target) and for a speed nudge to step from; the engine never
    /// acts on it.
    pub(crate) fn engage_preview(&self) -> Option<ControlIntent> {
        if self.program_walking() || self.last_status.is_moving() {
            return None;
        }
        let current = self.manual_intent.unwrap_or(ControlIntent {
            speed: 0.0,
            incline: 0,
        });
        let floor = self.min_physical_speed();
        if current.speed >= floor {
            return None;
        }
        self.engage_speed.map(|speed| ControlIntent {
            speed: speed.max(floor),
            incline: current.incline,
        })
    }

    /// Stop the deck and clear every intent, so nothing restarts it. The session
    /// is saved by `manage_session_lifecycle` once the deck reports Stopped.
    fn do_stop(&mut self, now: Instant) {
        self.pending_control = Some(Control::Stop);
        self.stop_requested = true;
        self.manual_intent = None;
        self.program_intent = None;
        // Stop means stop: end any program without restoring.
        self.program = None;
        self.engage_speed = None;
        self.handshake_step = HandshakeStep::Idle;
        self.last_manual_interaction = now;
        self.start_requested_at = None;
        self.start_blocked = false;
        log::info!("STOP requested (manual).");
    }

    /// The link went down. A dropout mid-walk keeps the intents and the session
    /// for the reconnect; anything else starts over.
    /// Take the deck's echo of a target, or a nack. The echo is what the deck
    /// took: when it differs from what was sent, say so (log and note). A nack
    /// shortly after a target is that target refused.
    pub fn on_deck_reply(&mut self, reply: protocol::DeckReply) {
        let now = self.now();
        let pending = self
            .target_pending
            .take()
            .filter(|(_, _, at)| now.saturating_duration_since(*at) < TARGET_REPLY_WINDOW);
        let Some((asked_speed, asked_incline, _)) = pending else {
            return;
        };
        let kmh = |b: u8| self.to_physical_speed(b as f32 / 10.0);
        let tilt = |level: u8| self.incline_pct(level);
        let note = match reply {
            protocol::DeckReply::Nack => {
                log::warn!(
                    "Deck refused target: speed byte {asked_speed}, incline {asked_incline}"
                );
                Some(format!(
                    "DECK_REFUSED // {:.1} KM/H, TILT {:.1} %",
                    kmh(asked_speed),
                    tilt(asked_incline)
                ))
            }
            protocol::DeckReply::TargetEcho { speed, incline }
                if (speed, incline) != (asked_speed, asked_incline) =>
            {
                log::info!(
                    "Deck took a different target: asked speed byte {asked_speed}, incline {asked_incline}; got {speed}, {incline}"
                );
                let mut parts = vec![];
                if speed != asked_speed {
                    parts.push(format!(
                        "{:.1} KM/H -> {:.1} KM/H",
                        kmh(asked_speed),
                        kmh(speed)
                    ));
                }
                if incline != asked_incline {
                    parts.push(format!(
                        "TILT {:.1} % -> {:.1} %",
                        tilt(asked_incline),
                        tilt(incline)
                    ));
                }
                Some(format!("DECK_ADJUSTED // {}", parts.join(", ")))
            }
            protocol::DeckReply::TargetEcho { .. } => None,
        };
        match note {
            Some(n) => self.target_note = Some((n, now)),
            // The deck took it as asked: an older complaint is stale.
            None => self.target_note = None,
        }
    }

    /// The note on the last target, while it is fresh.
    pub fn current_target_note(&self) -> Option<String> {
        let (note, at) = self.target_note.as_ref()?;
        (self.now().saturating_duration_since(*at) < TARGET_NOTE_SHOWN).then(|| note.clone())
    }

    pub fn reset_connection(&mut self) {
        self.awaiting_first_packet = true;
        self.session_evaluated_on_connect = false;
        // A stop in flight is void without a link.
        self.stop_requested = false;
        self.start_requested_at = None;
        self.start_blocked = false;
        self.start_sent_at = None;
        self.unknown_status_code = None;
        self.target_pending = None;
        self.target_note = None;
        self.deck_standby = false;
        self.last_handshake_tx = None;
        self.deck_set_speed = None;
        self.deck_set_incline = None;
        self.resend_target = false;
        // Nothing starts the belt on this connection until it is seen moving
        // or the user asks; a start half-sent before the drop is void too.
        self.hold_auto_start = true;
        self.handshake_step = HandshakeStep::Idle;
        // A program waiting for GO can't move the belt, so it outlives the
        // link (the treadmill switches itself off when idle, say over lunch):
        // like a run restored after a restart, it re-applies its paused target
        // after the reconnect instead of reading the cleared intents as a stop.
        if let Some(run) = self.program.as_mut().filter(|r| r.wait.is_some()) {
            run.restored = true;
        }

        let now = self.now();
        let was_moving = self.last_status.is_moving();
        let recent = self
            .session
            .last_physics_update
            .is_some_and(|t| now.duration_since(t) < DROPOUT_GRACE);

        if was_moving && recent {
            log::info!("Connection Reset: Preserving Intent for Auto-Resume (Status was Running, recent update)");
            // Still Running, so the engine doesn't start a handshake on the
            // reconnect; the first frame re-seeds speed and incline.
            self.last_status = MachineStatus::Running;
            self.hardware_speed = 0.0;
            self.hardware_incline = 0;
            self.handshake_step = HandshakeStep::Idle;
        } else {
            self.manual_intent = None;
            self.program_intent = None;
            self.engage_speed = None;
            log::info!("Connection Reset: Full State Clear");
            self.last_status = MachineStatus::Stopped;
            self.last_manual_interaction = before(now, Duration::from_secs(3600));
            self.session = SessionState::default();
            log::debug!("Physics/Session CLEARED");
        }
    }

    /// The user set a speed by hand: Neural flow takes it as the new base, and
    /// a deck that went quiet on a start may be asked again.
    pub(crate) fn note_hand_speed(&mut self, now: Instant) {
        self.speed_hand_epoch = self.speed_hand_epoch.wrapping_add(1);
        // The speed change that follows is the user's: toast it as usual.
        self.neural_changed_at = None;
        // A speed set by hand may start a still belt, as it always has, and
        // replaces the speed a program left for the next Engage.
        self.hold_auto_start = false;
        self.engage_speed = None;
        if self.start_blocked {
            self.arm_start_watch(now);
        }
    }

    /// An explicit start (Engage, a program run): watch for the deck to answer.
    /// False when the deck can't take a start now (see [`Self::start_refusal`]):
    /// nothing is sent, the user is told at once, and, as after an unanswered
    /// Engage, nothing starts the belt later by itself (`start_blocked`).
    pub(crate) fn arm_start_watch(&mut self, now: Instant) -> bool {
        if let Some(why) = self.start_refusal() {
            self.refuse_start(why);
            return false;
        }
        self.start_blocked = false;
        self.hold_auto_start = false;
        if !self.last_status.is_moving() {
            self.start_requested_at = Some(now);
            crate::link::start_requested();
        }
        true
    }

    /// Why the deck can't take a start right now, if it can't: it is in
    /// standby (off or asleep; it refuses every start and the app can't wake
    /// it), or winding the belt down after a stop (it refuses anything but
    /// polls until it has stopped).
    pub(crate) fn start_refusal(&self) -> Option<&'static str> {
        if self.deck_standby {
            Some(crate::link::START_STANDBY)
        } else if self.last_status == MachineStatus::Stopping {
            Some(crate::link::START_SPINNING_DOWN)
        } else {
            None
        }
    }

    /// Send no start, say why at once, and, as after an unanswered Engage,
    /// start nothing later by itself (`start_blocked`) until the user asks again.
    fn refuse_start(&mut self, why: &str) {
        log::info!("Start refused: {}", why);
        self.start_requested_at = None;
        self.start_blocked = true;
        self.handshake_step = HandshakeStep::Idle;
        crate::link::start_refused(why);
    }

    /// Follow up on an Engage: clear the watch once the deck reports Starting or
    /// Running, or give up after `link::START_TIMEOUT` and say so.
    pub(crate) fn check_start_watch(&mut self, now: Instant) {
        // The deck started some other way (its own buttons, the remote): the
        // block after an unanswered Engage no longer applies, or auto-resume
        // after a later pause would stay off for the rest of the session.
        if self.start_blocked && self.last_status.is_moving() {
            self.start_blocked = false;
        }
        let Some(at) = self.start_requested_at else {
            return;
        };
        match crate::link::start_check(now.saturating_duration_since(at), self.last_status) {
            crate::link::StartCheck::Waiting => {}
            crate::link::StartCheck::Started => {
                self.start_requested_at = None;
                crate::link::start_succeeded();
            }
            crate::link::StartCheck::TimedOut => {
                self.start_requested_at = None;
                self.start_blocked = true;
                self.handshake_step = HandshakeStep::Idle;
                crate::link::start_timed_out(self.last_status, self.unknown_status_code);
            }
        }
    }

    /// Physical km/h of one hardware speed step near `base`.
    fn hw_step_kmh(&self, base: f32) -> f32 {
        let (b, _) = calculate_hardware_bytes(base, 0, &self.calibration_points, self.max_speed);
        let b = b.max(1);
        let step =
            self.to_physical_speed(b as f32 / 10.0) - self.to_physical_speed((b - 1) as f32 / 10.0);
        if step.is_finite() && step > 0.0 {
            step.clamp(0.05, 0.5)
        } else {
            0.1
        }
    }

    /// Neural flow, once a second. `keys` is the key presses counted since the
    /// last tick (a count only); `quiet` is quiet mode holding the belt for a call.
    pub fn update_neural_flow(&mut self, keys: u32, quiet: bool, now: Instant, now_ms: u64) {
        let (base, _, _) = self.resolve_base(now);
        let tick = crate::neural::Tick {
            now_ms,
            keys,
            base_kmh: base,
            min_kmh: self.min_physical_speed(),
            step_kmh: self.hw_step_kmh(base),
            running: self.last_status == MachineStatus::Running && !self.stop_requested,
            quiet,
            hand_epoch: self.speed_hand_epoch,
        };
        let cfg = crate::neural::Config {
            enabled: self.focus_stability_enabled,
            floor_pct: self.neural_floor_pct,
            sensitivity: self.neural_sensitivity,
        };
        let was = self.neural.target_kmh();
        if self.neural.tick(&cfg, &tick) {
            self.neural_changed_at = Some(now);
            let kind = match self.neural.last_change_kind() {
                Some(crate::neural::ChangeKind::Recover) => "recovery",
                Some(crate::neural::ChangeKind::Floor) => "floor moved",
                _ => "slowing",
            };
            log::info!(
                "Neural flow {}: {:.2} -> {:.2} km/h (base {:.2}, {:.0} keys/min, typing level {:.0} keys/min)",
                kind,
                was,
                self.neural.target_kmh(),
                self.neural.base_kmh(),
                self.neural.keys_per_min(),
                self.neural.level_kpm()
            );
        }
    }

    /// What the manual and program intents ask for, before Neural flow: the
    /// manual intent within the lockout after a manual change, else a running
    /// program's, else the manual one.
    pub(crate) fn resolve_base(&self, now: Instant) -> (f32, u8, TargetSource) {
        let manual_locked =
            now.saturating_duration_since(self.last_manual_interaction) < MANUAL_LOCKOUT;
        match (self.manual_intent, self.program_intent) {
            (Some(m), _) if manual_locked => (m.speed, m.incline, TargetSource::Manual),
            (_, Some(p)) => (p.speed, p.incline, TargetSource::Program),
            (Some(m), None) => (m.speed, m.incline, TargetSource::ManualIdle),
            (None, None) => (0.0, 0, TargetSource::Unset),
        }
    }

    /// The speed and incline to send now: the base eased by Neural flow, held
    /// to at least the belt's floor (`min_physical_speed`) while moving.
    pub fn resolve_target(&self) -> (f32, u8, TargetSource) {
        let (speed, incline, source) = self.resolve_base(self.now());
        // A speed set by hand since Neural flow's last tick wins at once; quiet
        // mode on a call takes the lower speed and is never slowed further.
        let speed = self.neural.speed_for(
            speed,
            self.speed_hand_epoch,
            crate::snapshot::calls::quiet_active(),
        );
        // The top speed is enforced in `calculate_hardware_bytes`.
        let floor = self.min_physical_speed();
        let speed = if speed > 0.0 && speed < floor {
            floor
        } else {
            speed
        };
        (speed, incline.min(INCLINE_LEVEL_LIMIT), source)
    }

    /// The engine may drive the deck: it has reported, and no stop is in flight.
    fn may_drive(&self) -> bool {
        !self.awaiting_first_packet && !self.stop_requested
    }

    /// The belt is turning but the manual intent sits at zero (we connected to
    /// an idle deck and the user then started it with its own buttons): take
    /// the deck's speed as the manual intent, or the auto-pause would fight the
    /// walk. Only after the manual lockout, so a deliberate pause from the app
    /// is never overridden.
    fn adopt_physical_start(&mut self, now: Instant) {
        if !self.may_drive() || self.program_walking() {
            return;
        }
        let manual_stale = now.duration_since(self.last_manual_interaction) >= MANUAL_LOCKOUT;
        let belt_moving = self.hardware_speed > STILL_KMH && self.last_status.is_moving();
        let intent_zero = self.manual_intent.is_none_or(|i| i.speed < STILL_KMH);
        if manual_stale && belt_moving && intent_zero {
            let deck_byte = (self.hardware_speed * 10.0).round() as u8;
            let phys = self.physical_for_byte(deck_byte, self.hardware_incline);
            self.manual_intent = Some(ControlIntent {
                speed: phys,
                incline: self.hardware_incline,
            });
            self.last_manual_interaction = now;
            // A start at the treadmill is a press: it replaces the speed a
            // program left for the next Engage.
            self.engage_speed = None;
            log::info!(
                "Adopted physical start: manual intent -> {:.2} km/h (inc {})",
                phys,
                self.hardware_incline
            );
        }
    }

    /// The deck has reported the belt moving on this connection: from here on
    /// the automatic start and resume work as usual (a pause later in this walk
    /// can be resumed by a program step, say). Not while a stop is in flight:
    /// that belt is on its way to still, and a hold set with the stop (lights
    /// out) stays until a press.
    fn release_hold_if_moving(&mut self) {
        if self.hold_auto_start
            && !self.awaiting_first_packet
            && self.last_status.is_moving()
            && !self.stop_requested
        {
            self.hold_auto_start = false;
            log::info!("Belt seen moving on this connection: automatic start allowed");
        }
    }

    /// The handshake runs as a pause, not a start: the belt is running and the
    /// target is zero.
    fn handshake_pauses(&self) -> bool {
        self.last_status == MachineStatus::Running && self.resolve_target().0 < STILL_KMH
    }

    /// The command gap since the last write has passed.
    fn command_gap_passed(&self, now: Instant) -> bool {
        now.duration_since(self.last_cmd_time) >= COMMAND_GAP
    }
}

/// What a packet is, for the log and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketKind {
    /// A queued Pause or Stop.
    AtomicAction,
    HandshakeReady,
    HandshakeStart,
    HandshakePausePrime,
    HandshakePause,
    ControlSet,
    PollStatus,
}

impl PacketKind {
    pub fn label(self) -> &'static str {
        match self {
            PacketKind::AtomicAction => "ATOMIC_ACTION",
            PacketKind::HandshakeReady => "HANDSHAKE_READY",
            PacketKind::HandshakeStart => "HANDSHAKE_START",
            PacketKind::HandshakePausePrime => "HANDSHAKE_PAUSE_PRIME",
            PacketKind::HandshakePause => "HANDSHAKE_PAUSE",
            PacketKind::ControlSet => "CONTROL_SET",
            PacketKind::PollStatus => "POLL_STATUS",
        }
    }
}

/// A framed packet ready to write.
#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub bytes: Vec<u8>,
    pub kind: PacketKind,
}

impl Packet {
    fn control(kind: PacketKind, control: Control) -> Self {
        Self {
            bytes: protocol::control_packet(control, 0, 0),
            kind,
        }
    }
}

/// How long the deck reports the same speed or incline, with no target sent
/// in that time, before a value short of the app's target counts as the
/// deck's own setting (`DeckWatch::check`).
const DECK_SETTLED: Duration = Duration::from_secs(3);

/// One tick of `ProtocolEngine::notice_deck_changes`.
struct DeckWatch {
    /// The deck is running, with no stop or handshake under way.
    running: bool,
    /// `DECK_SETTLED` has passed since the app last sent a target.
    settled_since_set: bool,
    now: Instant,
}

impl DeckWatch {
    /// Take one reported `value` (a speed byte or incline level) against the
    /// one `seen` before it, the app's last `sent` one (255: none yet) and the
    /// deck's own setting `own`. The deck is headed for `own`, else `sent`. A
    /// change that brings the value closer is the deck getting there (a ramp
    /// toward the app's target is the app's own change); any other change is
    /// the deck's own setting, and so is a value short of the app's target
    /// that holds for `DECK_SETTLED` with no target sent meanwhile.
    fn check(&self, seen: &mut Option<(u8, Instant)>, value: u8, sent: u8, own: &mut Option<u8>) {
        let prev = *seen;
        if prev.is_none_or(|(v, _)| v != value) {
            *seen = Some((value, self.now));
        }
        if !self.running || sent == 255 {
            return;
        }
        let expected = own.unwrap_or(sent);
        let deck_chose = match prev {
            None => false,
            Some((v, _)) if v != value => value.abs_diff(expected) >= v.abs_diff(expected),
            Some((_, since)) => {
                value != expected
                    && self.settled_since_set
                    && self.now.saturating_duration_since(since) >= DECK_SETTLED
            }
        };
        if deck_chose {
            *own = Some(value);
        }
    }
}

/// Decides the next packet. One per connection: what it last sent lives here.
pub struct ProtocolEngine {
    pub last_sent_speed: u8,
    pub last_sent_incline: u8,
    /// The target last sent while moving. Auto-resume needs the target to
    /// rise above it.
    pub last_physical_speed: f32,
    /// The "last sent" values have been seeded from the deck's first frame.
    pub has_synced_reality: bool,
    stop_started: Option<Instant>,
    last_stop_issue: Option<Instant>,
    /// The deck's last reported speed byte and incline, each with when it
    /// was first reported, for `notice_deck_changes`.
    seen_speed: Option<(u8, Instant)>,
    seen_incline: Option<(u8, Instant)>,
    /// When the last target was sent.
    last_set_at: Option<Instant>,
}

impl Default for ProtocolEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolEngine {
    pub fn new() -> Self {
        Self {
            last_sent_speed: 255,
            last_sent_incline: 255,
            last_physical_speed: 0.0,
            has_synced_reality: false,
            stop_started: None,
            last_stop_issue: None,
            seen_speed: None,
            seen_incline: None,
            last_set_at: None,
        }
    }

    /// The packet to write now, if any. In order of priority: a queued Pause or
    /// Stop, the start/pause handshake, a target change (or the automatic
    /// start, resume and pause that arm the handshake), and the status poll.
    pub fn determine_next_packet(
        &mut self,
        state: &mut OrchestratorState,
        now: Instant,
    ) -> Option<Packet> {
        state.check_start_watch(now);
        state.release_hold_if_moving();
        self.drive_stop(state, now);
        self.sync_with_deck(state, now);
        self.notice_deck_changes(state, now);
        self.forget_sent_when_stopped(state);
        // A start at the treadmill on a program holding the belt still is a
        // press; while it is being confirmed the target isn't followed (no
        // pause, no zero speed sent to the starting belt).
        let deck_start_pending = state.watch_for_deck_start(now);
        state.adopt_physical_start(now);

        // The deck is winding the belt down after a stop: it refuses anything
        // but polls until it reports the belt still, so nothing else is sent
        // (a start or pause asked for now is dropped, not kept for later).
        if state.last_status == MachineStatus::Stopping {
            if let Some(control) = state.pending_control.take() {
                log::info!("Belt spinning down: {:?} not sent", control);
            }
            state.handshake_step = HandshakeStep::Idle;
            return Self::poll_packet(state, now);
        }

        if let Some(control) = state.pending_control.take() {
            if state.command_gap_passed(now) {
                return Some(Packet::control(PacketKind::AtomicAction, control));
            }
            // Too soon after the last write: keep it for the next tick.
            state.pending_control = Some(control);
        }

        let mut packet = Self::handshake_packet(state, now);
        if packet.is_none() && state.handshake_step == HandshakeStep::Idle && !deck_start_pending {
            packet = self.control_packet(state, now);
        }
        packet.or_else(|| Self::poll_packet(state, now))
    }

    /// While a stop is in flight nothing else drives the deck. The stop is
    /// repeated at most once a second (any faster starves the status poll, so
    /// the deck's "at rest" never arrives), until the belt is at rest or
    /// `STOP_TIMEOUT` passes.
    fn drive_stop(&mut self, state: &mut OrchestratorState, now: Instant) {
        if !state.stop_requested {
            self.stop_started = None;
            self.last_stop_issue = None;
            return;
        }
        let started = *self.stop_started.get_or_insert(now);
        // Stopping: the deck took the stop and winds the belt down itself;
        // another stop would only be refused.
        let at_rest = state.hardware_speed < STILL_KMH
            || matches!(
                state.last_status,
                MachineStatus::Stopped | MachineStatus::Paused | MachineStatus::Stopping
            );
        let timed_out = now.duration_since(started) >= STOP_TIMEOUT;
        if at_rest || timed_out {
            if timed_out && !at_rest {
                log::warn!("Stop not confirmed by the deck; giving up re-issuing.");
            }
            state.stop_requested = false;
            self.stop_started = None;
            self.last_stop_issue = None;
        } else if self
            .last_stop_issue
            .is_none_or(|t| now.duration_since(t) >= STOP_REISSUE_INTERVAL)
        {
            state.pending_control = Some(Control::Stop);
            self.last_stop_issue = Some(now);
        }
    }

    /// On the first frame of a connection, take the deck's speed and incline
    /// as already sent (so nothing is sent just because the defaults differ),
    /// and as the manual intent if there is none (so calibration rounding
    /// doesn't nudge the belt).
    fn sync_with_deck(&mut self, state: &mut OrchestratorState, now: Instant) {
        if state.awaiting_first_packet || self.has_synced_reality {
            return;
        }
        self.last_sent_incline = state.hardware_incline;
        self.last_sent_speed = (state.hardware_speed * 10.0).round() as u8;
        self.has_synced_reality = true;

        if state.manual_intent.is_none() {
            // The speed that maps back to the deck's own byte, so adopting a
            // moving belt never sends it a slightly different speed.
            let physical_speed =
                state.physical_for_byte(self.last_sent_speed, state.hardware_incline);
            state.manual_intent = Some(ControlIntent {
                speed: physical_speed,
                incline: state.hardware_incline,
            });
            state.last_manual_interaction = now;
            log::info!(
                "First-Sync: Seeded Manual Intent from Physical Reality (Spd: {:.2}, Inc: {})",
                physical_speed,
                state.hardware_incline
            );
        }
    }

    /// Notice a speed or incline the deck set by itself (its remote or its own
    /// buttons), for the next press to step from (`OrchestratorState::nudge_base`).
    /// Nothing is sent for it. Only while running: a stopped deck forgets its
    /// speed, and a paused or starting one winds the belt down or up.
    fn notice_deck_changes(&mut self, state: &mut OrchestratorState, now: Instant) {
        if state.last_status == MachineStatus::Stopped {
            state.deck_set_speed = None;
            state.deck_set_incline = None;
            state.resend_target = false;
            self.seen_speed = None;
            self.seen_incline = None;
            return;
        }
        if !self.has_synced_reality {
            return;
        }
        let watch = DeckWatch {
            running: state.last_status == MachineStatus::Running
                && !state.stop_requested
                && state.handshake_step == HandshakeStep::Idle,
            settled_since_set: self
                .last_set_at
                .is_none_or(|t| now.saturating_duration_since(t) >= DECK_SETTLED),
            now,
        };
        let speed = (state.hardware_speed * 10.0).round().clamp(0.0, 255.0) as u8;
        watch.check(
            &mut self.seen_speed,
            speed,
            self.last_sent_speed,
            &mut state.deck_set_speed,
        );
        watch.check(
            &mut self.seen_incline,
            state.hardware_incline,
            self.last_sent_incline,
            &mut state.deck_set_incline,
        );
    }

    /// A stopped deck forgets its speed: the next start runs at the deck's own
    /// start speed. Forget what was sent too, so the target goes out again once
    /// the belt runs, even when it is the same speed as last time.
    fn forget_sent_when_stopped(&mut self, state: &OrchestratorState) {
        if self.has_synced_reality && state.last_status == MachineStatus::Stopped {
            self.last_sent_speed = 255;
            self.last_sent_incline = 255;
        }
    }

    /// The next handshake packet when one is due: Ready then Start, or Pause
    /// twice when the handshake is pausing a running belt.
    fn handshake_packet(state: &mut OrchestratorState, now: Instant) -> Option<Packet> {
        if !state.may_drive() {
            return None;
        }
        // Standby refuses every start; nothing is kept for when it wakes.
        if state.deck_standby && state.handshake_step != HandshakeStep::Idle {
            state.handshake_step = HandshakeStep::Idle;
            return None;
        }
        let waited = |gap: Duration| {
            state
                .last_handshake_tx
                .is_none_or(|t| now.duration_since(t) >= gap)
        };
        let (packet, next) = match state.handshake_step {
            HandshakeStep::Idle => return None,
            HandshakeStep::Priming if waited(HANDSHAKE_PRIME_GAP) => {
                state.handshake_is_pause = state.handshake_pauses();
                let packet = if state.handshake_is_pause {
                    Packet::control(PacketKind::HandshakePausePrime, Control::Pause)
                } else {
                    Packet::control(PacketKind::HandshakeReady, Control::Ready)
                };
                (packet, HandshakeStep::Starting)
            }
            HandshakeStep::Starting if waited(HANDSHAKE_ACT_GAP) => {
                let packet = if state.handshake_is_pause {
                    Packet::control(PacketKind::HandshakePause, Control::Pause)
                } else {
                    state.start_sent_at = Some(now);
                    Packet::control(PacketKind::HandshakeStart, Control::Start)
                };
                (packet, HandshakeStep::Idle)
            }
            _ => return None,
        };
        state.last_handshake_tx = Some(now);
        state.handshake_step = next;
        Some(packet)
    }

    /// Follow the target: arm the handshake to start a stopped deck, resume a
    /// paused one or pause a running one at target zero; otherwise send the
    /// target when its bytes change.
    fn control_packet(&mut self, state: &mut OrchestratorState, now: Instant) -> Option<Packet> {
        if !state.may_drive() {
            return None;
        }
        let (target_speed, target_incline, source) = state.resolve_target();
        let has_target = source != TargetSource::Unset;

        match state.last_status {
            // Not after an unanswered Engage (`start_blocked`): the deck is
            // asleep, and sending start again does nothing. Not on a
            // connection that hasn't seen the belt move or a user start
            // (`hold_auto_start`): a stale intent never starts the belt.
            MachineStatus::Stopped
                if has_target
                    && target_speed > AUTO_START_MIN_KMH
                    && !state.start_blocked
                    && !state.hold_auto_start =>
            {
                // A speed set by hand on a deck in standby: it would only
                // refuse the start, so say so instead.
                match state.start_refusal() {
                    Some(why) => state.refuse_start(why),
                    None => state.handshake_step = HandshakeStep::Priming,
                }
            }
            MachineStatus::Paused
                if target_speed > self.last_physical_speed + STILL_KMH
                    && has_target
                    && !state.start_blocked
                    && !state.hold_auto_start =>
            {
                state.handshake_step = HandshakeStep::Priming;
            }
            MachineStatus::Running if target_speed < STILL_KMH => {
                state.handshake_step = HandshakeStep::Priming;
                // Auto-resume needs the target above this, so it drops with the
                // belt, or resuming at the same speed would never trigger.
                self.last_physical_speed = 0.0;
                log::info!("Triggering Pause (Source: {})", source);
            }
            // The deck's countdown after a start or resume: it is already
            // starting, and refuses another Ready/Start or a target until it
            // runs. Stopping: nothing but polls (see `determine_next_packet`).
            MachineStatus::Starting | MachineStatus::Stopping => {}
            status if status.is_moving() && has_target && state.command_gap_passed(now) => {
                let (h_speed, h_inc) = calculate_hardware_bytes(
                    target_speed,
                    target_incline,
                    &state.calibration_points,
                    state.max_speed,
                );
                self.last_physical_speed = target_speed;
                if h_speed != self.last_sent_speed
                    || h_inc != self.last_sent_incline
                    || state.resend_target
                {
                    log::debug!(
                        "SET -> Speed: {:.1}kmh (HW: {}), Incline: Level {} (HW: {}) [Source: {}]",
                        target_speed,
                        h_speed,
                        target_incline,
                        h_inc,
                        source
                    );
                    self.last_sent_speed = h_speed;
                    self.last_sent_incline = h_inc;
                    state.target_pending = Some((h_speed, h_inc, now));
                    // The deck takes the app's speed and incline from here.
                    state.deck_set_speed = None;
                    state.deck_set_incline = None;
                    state.resend_target = false;
                    self.last_set_at = Some(now);
                    return Some(Packet {
                        bytes: protocol::control_packet(Control::Target, h_speed, h_inc),
                        kind: PacketKind::ControlSet,
                    });
                }
            }
            _ => {}
        }
        None
    }

    /// A status request when one is due. The session totals are brought up to
    /// date first.
    fn poll_packet(state: &mut OrchestratorState, now: Instant) -> Option<Packet> {
        if now.duration_since(state.last_poll_time) < POLL_EVERY || !state.command_gap_passed(now) {
            return None;
        }
        state.update_physics();
        state.last_poll_time = now;
        Some(Packet {
            bytes: protocol::status_request(),
            kind: PacketKind::PollStatus,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand calculation, 75 kg at 4.0 km/h (S = 4000/60 = 66.667 m/min):
    //   level:   VO2 = 0.1*66.667 + 3.5            = 10.1667 mL/kg/min
    //            kcal/min = 10.1667 * 75 / 1000 * 5 = 3.8125  (228.75 kcal/h)
    //   10 %:    VO2 = 6.6667 + 1.8*66.667*0.10 + 3.5 = 6.6667 + 12.0 + 3.5 = 22.1667
    //            kcal/min = 22.1667 * 75 / 1000 * 5 = 8.3125  (498.75 kcal/h)
    #[test]
    fn walking_kcal_level_and_graded() {
        let level = walking_kcal(4.0, 0.0, 75.0, 60.0);
        assert!((level - 3.8125).abs() < 1e-6, "level: {level}");
        let graded = walking_kcal(4.0, 10.0, 75.0, 60.0);
        assert!((graded - 8.3125).abs() < 1e-6, "10%: {graded}");
        // Integrates linearly in time: one hour at 4 km/h level is 228.75 kcal.
        let hour = walking_kcal(4.0, 0.0, 75.0, 3600.0);
        assert!((hour - 228.75).abs() < 1e-4, "hour: {hour}");
    }

    #[test]
    fn walking_kcal_zero_and_clamps() {
        assert_eq!(walking_kcal(0.0, 5.0, 75.0, 1.0), 0.0);
        assert_eq!(walking_kcal(4.0, 5.0, 75.0, 0.0), 0.0);
        assert_eq!(walking_kcal(4.0, 5.0, 0.0, 1.0), 0.0);
        assert_eq!(walking_kcal(f64::NAN, 5.0, 75.0, 1.0), 0.0);
        // A negative grade counts as level.
        assert_eq!(
            walking_kcal(4.0, -8.0, 75.0, 60.0),
            walking_kcal(4.0, 0.0, 75.0, 60.0)
        );
        // Scales with body mass: 50 kg is two thirds of 75 kg.
        let light = walking_kcal(4.0, 0.0, 50.0, 60.0);
        assert!((light - 3.8125 * 2.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn update_physics_counts_calories_only_while_moving() {
        let mut state = OrchestratorState {
            hardware_speed: 4.0,
            hardware_incline: 0,
            last_status: MachineStatus::Running,
            ..Default::default()
        };
        let t0 = Instant::now();
        state.session.last_physics_update = Some(t0);
        state.set_test_now(t0 + Duration::from_secs(1));
        state.update_physics();
        // About one second at 3.8125 kcal/min = 0.0635 kcal.
        assert!(
            (state.session.calories - 0.0635).abs() < 0.005,
            "{}",
            state.session.calories
        );

        let mut paused = OrchestratorState {
            hardware_speed: 4.0,
            last_status: MachineStatus::Paused,
            ..Default::default()
        };
        paused.session.last_physics_update = Some(t0);
        paused.set_test_now(t0 + Duration::from_secs(1));
        paused.update_physics();
        assert_eq!(paused.session.calories, 0.0);
    }

    /// A state the engine may drive: the deck has reported.
    fn reported() -> OrchestratorState {
        OrchestratorState {
            awaiting_first_packet: false,
            ..Default::default()
        }
    }

    /// An engine whose first sync has happened.
    fn synced() -> ProtocolEngine {
        ProtocolEngine {
            has_synced_reality: true,
            ..ProtocolEngine::new()
        }
    }

    #[test]
    fn test_handshake_progression() {
        let mut engine = synced();
        let mut state = reported();
        state.handshake_step = HandshakeStep::Priming;
        let now = Instant::now();

        let packet = engine
            .determine_next_packet(&mut state, now)
            .expect("ready");
        assert_eq!(packet.kind, PacketKind::HandshakeReady);
        assert_eq!(packet.bytes, protocol::control_packet(Control::Ready, 0, 0));
        assert_eq!(state.handshake_step, HandshakeStep::Starting);

        let packet = engine
            .determine_next_packet(&mut state, now + Duration::from_millis(500))
            .expect("start");
        assert_eq!(packet.kind, PacketKind::HandshakeStart);
        assert_eq!(packet.bytes, protocol::control_packet(Control::Start, 0, 0));
        assert_eq!(state.handshake_step, HandshakeStep::Idle);
    }

    #[test]
    fn the_handshake_waits_between_its_packets() {
        let mut engine = synced();
        let mut state = reported();
        state.handshake_step = HandshakeStep::Priming;
        let now = Instant::now();
        state.last_handshake_tx = Some(now);
        state.last_poll_time = now;
        // 100 ms after the last handshake packet: too soon to prime.
        assert_eq!(
            engine.determine_next_packet(&mut state, now + Duration::from_millis(100)),
            None
        );
        assert_eq!(state.handshake_step, HandshakeStep::Priming);
        let t = now + Duration::from_millis(150);
        assert_eq!(
            engine.determine_next_packet(&mut state, t).map(|p| p.kind),
            Some(PacketKind::HandshakeReady)
        );
        // The start needs 400 ms after the Ready.
        assert_eq!(
            engine.determine_next_packet(&mut state, t + Duration::from_millis(399)),
            None
        );
        assert_eq!(
            engine
                .determine_next_packet(&mut state, t + Duration::from_millis(400))
                .map(|p| p.kind),
            Some(PacketKind::HandshakeStart)
        );
    }

    #[test]
    fn a_handshake_on_a_running_belt_at_zero_target_pauses() {
        let mut engine = synced();
        let mut state = reported();
        state.last_status = MachineStatus::Running;
        state.hardware_speed = 3.0;
        state.manual_intent = Some(ControlIntent {
            speed: 0.0,
            incline: 0,
        });
        state.last_manual_interaction = Instant::now();
        let now = Instant::now();
        state.last_poll_time = now;
        // The zero target arms the handshake (no packet on that tick)...
        assert_eq!(engine.determine_next_packet(&mut state, now), None);
        assert_eq!(state.handshake_step, HandshakeStep::Priming);
        // ...which then pauses twice.
        let first = engine
            .determine_next_packet(&mut state, now + Duration::from_millis(50))
            .unwrap();
        assert_eq!(first.kind, PacketKind::HandshakePausePrime);
        assert_eq!(first.bytes, protocol::control_packet(Control::Pause, 0, 0));
        let second = engine
            .determine_next_packet(&mut state, now + Duration::from_millis(500))
            .unwrap();
        assert_eq!(second.kind, PacketKind::HandshakePause);
        assert_eq!(second.bytes, protocol::control_packet(Control::Pause, 0, 0));
    }

    #[test]
    fn test_first_sync_authority() {
        let mut engine = ProtocolEngine::new();
        let mut state = reported();
        // The deck is already at incline 12 and 5.0 km/h; no intent yet.
        state.hardware_incline = 12;
        state.hardware_speed = 5.0;
        state.last_status = MachineStatus::Running;
        let now = Instant::now();
        state.last_poll_time = now; // no poll this tick

        // The first tick only seeds.
        assert_eq!(engine.determine_next_packet(&mut state, now), None);
        assert!(engine.has_synced_reality);
        assert_eq!(engine.last_sent_incline, 12);
        assert_eq!(engine.last_sent_speed, 50);
        assert_eq!(state.manual_intent.unwrap().incline, 12);

        // The target now matches the deck, so nothing is sent.
        assert_eq!(
            engine.determine_next_packet(&mut state, now + Duration::from_millis(200)),
            None
        );
    }

    #[test]
    fn a_new_target_is_sent_once_as_a_control_set() {
        let mut engine = synced();
        engine.last_sent_speed = 30;
        engine.last_sent_incline = 0;
        let mut state = reported();
        state.last_status = MachineStatus::Running;
        state.hardware_speed = 3.0;
        state.manual_intent = Some(ControlIntent {
            speed: 3.5,
            incline: 4,
        });
        state.last_manual_interaction = Instant::now();
        let now = Instant::now();
        state.last_poll_time = now;
        let p = engine.determine_next_packet(&mut state, now).unwrap();
        assert_eq!(p.kind, PacketKind::ControlSet);
        assert_eq!(p.bytes, protocol::control_packet(Control::Target, 35, 4));
        state.last_cmd_time = now;
        // Unchanged: not sent again.
        assert_eq!(
            engine.determine_next_packet(&mut state, now + Duration::from_millis(200)),
            None
        );
        assert!((engine.last_physical_speed - 3.5).abs() < 1e-6);
    }

    #[test]
    fn a_queued_pause_waits_for_the_command_gap() {
        let mut engine = synced();
        let mut state = reported();
        let now = Instant::now();
        state.last_cmd_time = now;
        state.last_poll_time = now;
        state.trigger_action(DeckAction::Pause);
        assert_eq!(
            engine.determine_next_packet(&mut state, now + Duration::from_millis(100)),
            None
        );
        assert_eq!(state.pending_control, Some(Control::Pause));
        let p = engine
            .determine_next_packet(&mut state, now + Duration::from_millis(150))
            .unwrap();
        assert_eq!(p.kind, PacketKind::AtomicAction);
        assert_eq!(p.bytes, protocol::control_packet(Control::Pause, 0, 0));
        assert_eq!(state.pending_control, None);
    }

    #[test]
    fn the_status_poll_runs_every_600_ms() {
        let mut engine = synced();
        let mut state = reported();
        let now = Instant::now();
        state.last_poll_time = now;
        assert_eq!(
            engine.determine_next_packet(&mut state, now + Duration::from_millis(599)),
            None
        );
        let p = engine
            .determine_next_packet(&mut state, now + Duration::from_millis(600))
            .unwrap();
        assert_eq!(p.kind, PacketKind::PollStatus);
        assert_eq!(p.bytes, protocol::status_request());
    }

    #[test]
    fn nothing_but_polls_before_the_first_frame() {
        let mut engine = ProtocolEngine::new();
        let mut state = OrchestratorState {
            manual_intent: Some(ControlIntent {
                speed: 3.0,
                incline: 0,
            }),
            handshake_step: HandshakeStep::Priming,
            ..Default::default()
        };
        let sent = kinds(&mut engine, &mut state, Instant::now(), 40);
        assert!(
            sent.iter().all(|k| *k == PacketKind::PollStatus),
            "{sent:?}"
        );
        assert!(!engine.has_synced_reality);
    }

    /// Drive the engine like the protocol worker does (50 ms ticks, stamping
    /// last_cmd_time on every send) and count what goes out.
    fn run_ticks(
        engine: &mut ProtocolEngine,
        state: &mut OrchestratorState,
        start: Instant,
        ticks: u64,
    ) -> (usize, usize) {
        let sent = kinds(engine, state, start, ticks);
        let count = |k: PacketKind| sent.iter().filter(|s| **s == k).count();
        (
            count(PacketKind::AtomicAction),
            count(PacketKind::PollStatus),
        )
    }

    /// Kinds of the packets sent over `ticks` 50 ms ticks from `start`.
    #[test]
    fn a_pause_stays_a_pause_when_the_deck_reports_paused_mid_handshake() {
        let mut engine = ProtocolEngine::new();
        engine.has_synced_reality = true;
        let mut state = OrchestratorState {
            awaiting_first_packet: false,
            hold_auto_start: false,
            last_status: MachineStatus::Running,
            hardware_speed: 3.0,
            handshake_step: HandshakeStep::Priming,
            ..Default::default()
        };
        let t0 = Instant::now();
        // A pause just asked for by hand: target zero, inside the lockout.
        state.manual_intent = Some(ControlIntent {
            speed: 0.0,
            incline: 0,
        });
        state.last_manual_interaction = t0;
        let first = engine.determine_next_packet(&mut state, t0).map(|p| p.kind);
        assert_eq!(first, Some(PacketKind::HandshakePausePrime));
        state.last_cmd_time = t0;
        // The deck answers the first packet before the second goes out.
        state.last_status = MachineStatus::Paused;
        let rest = kinds(&mut engine, &mut state, t0 + Duration::from_millis(50), 20);
        assert!(rest.contains(&PacketKind::HandshakePause), "{rest:?}");
        assert!(!rest.contains(&PacketKind::HandshakeStart), "{rest:?}");
    }

    fn kinds(
        engine: &mut ProtocolEngine,
        state: &mut OrchestratorState,
        start: Instant,
        ticks: u64,
    ) -> Vec<PacketKind> {
        let mut out = vec![];
        for i in 0..ticks {
            let now = start + Duration::from_millis(50 * i);
            state.set_test_now(now);
            if let Some(packet) = engine.determine_next_packet(state, now) {
                state.last_cmd_time = now;
                out.push(packet.kind);
            }
        }
        out
    }

    #[test]
    fn an_unanswered_engage_gives_up_after_eight_seconds() {
        let mut engine = synced();
        let mut state = reported();
        state.last_status = MachineStatus::Stopped;

        let t0 = Instant::now();
        state.trigger_action(DeckAction::Start);
        assert!(state.start_requested_at.is_some());

        // The deck stays Stopped (screen off): start is tried while we wait...
        let sent = kinds(&mut engine, &mut state, t0, 170);
        assert!(sent.contains(&PacketKind::HandshakeStart), "{sent:?}");
        // ...then given up, and never re-sent by the auto-start.
        assert!(state.start_requested_at.is_none());
        assert!(state.start_blocked);
        let later = kinds(
            &mut engine,
            &mut state,
            t0 + Duration::from_millis(8_500),
            400,
        );
        assert!(
            !later.iter().any(|k| k.label().starts_with("HANDSHAKE")),
            "{later:?}"
        );
        assert_eq!(state.handshake_step, HandshakeStep::Idle);
        assert!(later.contains(&PacketKind::PollStatus));

        // Engage again, 30 s in: tried again.
        state.set_test_now(t0 + Duration::from_secs(30));
        state.trigger_action(DeckAction::Start);
        assert!(!state.start_blocked);
        assert_eq!(state.start_requested_at, Some(t0 + Duration::from_secs(30)));
        let again = kinds(&mut engine, &mut state, t0 + Duration::from_secs(30), 40);
        assert!(again.contains(&PacketKind::HandshakeStart), "{again:?}");
    }

    #[test]
    fn a_deck_started_by_hand_clears_the_block() {
        let mut state = reported();
        state.start_blocked = true;
        state.last_status = MachineStatus::Running;
        state.check_start_watch(Instant::now());
        assert!(!state.start_blocked);
    }

    #[test]
    fn an_answered_engage_clears_the_watch() {
        let mut engine = synced();
        let mut state = reported();
        state.last_status = MachineStatus::Stopped;
        let t0 = Instant::now();
        state.trigger_action(DeckAction::Start);
        kinds(&mut engine, &mut state, t0, 20);
        state.last_status = MachineStatus::Starting;
        kinds(&mut engine, &mut state, t0 + Duration::from_secs(1), 2);
        assert!(state.start_requested_at.is_none());
        assert!(!state.start_blocked);
    }

    #[test]
    fn neural_flow_eases_a_manual_walk_and_a_hand_change_wins() {
        let mut state = OrchestratorState {
            focus_stability_enabled: true,
            neural_floor_pct: 80.0,
            last_status: MachineStatus::Running,
            manual_intent: Some(ControlIntent {
                speed: 4.0,
                incline: 0,
            }),
            ..Default::default()
        };
        let t0 = Instant::now();
        let ms0 = 1_000_000u64;
        for s in 0..60u64 {
            state.update_neural_flow(4, false, t0 + Duration::from_secs(s), ms0 + s * 1000);
        }
        let (slowed, _, _) = state.resolve_target();
        assert!((3.2 - 1e-3..4.0).contains(&slowed), "{slowed}");
        assert!(state.neural_changed_at.is_some());
        // The user sets 4.5: it applies at once, unslowed.
        crate::bluetooth::apply_manual_intent(&mut state, Some(4.5), None);
        assert!((state.resolve_target().0 - 4.5).abs() < 1e-4);
        assert!(state.neural_changed_at.is_none());
        // Quiet mode moves the base without counting as a hand change.
        let epoch = state.speed_hand_epoch;
        state.set_intent(Some(2.0), None, false);
        assert_eq!(state.speed_hand_epoch, epoch);
    }

    #[test]
    fn stop_is_reissued_slowly_and_does_not_starve_polling() {
        let mut engine = synced();
        let mut state = reported();
        state.last_status = MachineStatus::Running;
        state.hardware_speed = 3.0;

        let start = Instant::now() + Duration::from_secs(1);
        state.trigger_action(DeckAction::Stop);

        // 5 s of wind-down with the deck still reporting Running.
        let (stops, polls) = run_ticks(&mut engine, &mut state, start, 100);
        assert!((4..=6).contains(&stops), "stop sent {} times in 5s", stops);
        assert!(polls >= 4, "only {} status polls in 5s", polls);
        assert!(state.stop_requested);

        // Deck reports the belt at rest: the stop is released and not re-sent.
        state.last_status = MachineStatus::Stopped;
        state.hardware_speed = 0.0;
        let later = start + Duration::from_secs(5);
        let (stops, _) = run_ticks(&mut engine, &mut state, later, 40);
        assert_eq!(stops, 0);
        assert!(!state.stop_requested);
    }

    #[test]
    fn a_stop_sends_the_stop_packet_and_clears_every_intent() {
        let mut engine = synced();
        let mut state = reported();
        state.last_status = MachineStatus::Running;
        state.hardware_speed = 3.0;
        state.manual_intent = Some(ControlIntent {
            speed: 3.0,
            incline: 2,
        });
        state.program_intent = Some(ControlIntent {
            speed: 3.0,
            incline: 2,
        });
        state.handshake_step = HandshakeStep::Priming;
        state.trigger_action(DeckAction::Stop);
        assert_eq!((state.manual_intent, state.program_intent), (None, None));
        assert_eq!(state.handshake_step, HandshakeStep::Idle);
        let p = engine
            .determine_next_packet(&mut state, Instant::now() + Duration::from_secs(1))
            .unwrap();
        assert_eq!(p.kind, PacketKind::AtomicAction);
        assert_eq!(p.bytes, protocol::control_packet(Control::Stop, 0, 0));
    }

    #[test]
    fn stop_gives_up_if_the_deck_never_confirms() {
        let mut engine = synced();
        let mut state = reported();
        state.last_status = MachineStatus::Running;
        state.hardware_speed = 3.0;

        let start = Instant::now() + Duration::from_secs(1);
        state.trigger_action(DeckAction::Stop);
        run_ticks(&mut engine, &mut state, start, 20 * 25); // 25 s
        assert!(!state.stop_requested);
    }

    #[test]
    fn toggle_start_pauses_a_moving_belt_and_starts_a_still_one() {
        let mut state = reported();
        state.last_status = MachineStatus::Running;
        state.trigger_action(DeckAction::ToggleStart);
        assert_eq!(state.pending_control, Some(Control::Pause));
        assert_eq!(state.handshake_step, HandshakeStep::Idle);

        let mut state = reported();
        state.last_status = MachineStatus::Paused;
        state.trigger_action(DeckAction::ToggleStart);
        assert_eq!(state.pending_control, None);
        assert_eq!(state.handshake_step, HandshakeStep::Priming);
        // A start is never below the slowest speed.
        assert_eq!(
            state.manual_intent.map(|i| i.speed),
            Some(state.min_physical_speed())
        );
    }

    /// The belt's floor is `min_speed` in hardware units; with a calibration
    /// whose scale isn't 1 the physical floor differs, and every clamp must
    /// land on the deck's own floor byte, not below it (issue #61).
    #[test]
    fn the_speed_floor_is_the_deck_floor_through_the_calibration() {
        let mut state = reported();
        state.sync_config(&slow_floor_config());
        let floor = state.min_physical_speed();
        assert!((floor - 1.2).abs() < 1e-4, "{floor}");
        let floor_byte = (state.min_speed * 10.0).round() as u8;
        assert_eq!(floor_byte, 6);
        let byte = |s: &OrchestratorState, kmh: f32| {
            calculate_hardware_bytes(kmh, 0, &s.calibration_points, s.max_speed).0
        };
        assert_eq!(byte(&state, floor), floor_byte);

        // A positive speed under the floor is sent as the floor byte.
        state.manual_intent = Some(ControlIntent {
            speed: 0.8,
            incline: 0,
        });
        let (target, _, _) = state.resolve_target();
        assert_eq!(target, floor);
        assert_eq!(byte(&state, target), floor_byte);

        // Engage on a still belt starts at the floor, and the engage preview
        // never shows less.
        state.manual_intent = Some(ControlIntent {
            speed: 1.0,
            incline: 0,
        });
        state.last_status = MachineStatus::Paused;
        state.trigger_action(DeckAction::ToggleStart);
        assert_eq!(state.manual_intent.map(|i| i.speed), Some(floor));
        state.manual_intent = None;
        state.engage_speed = Some(0.9);
        assert_eq!(state.engage_preview().map(|i| i.speed), Some(floor));
    }

    #[test]
    fn deck_actions_round_trip_their_names() {
        for a in DeckAction::ALL {
            assert_eq!(DeckAction::parse(a.as_str()), Some(a));
        }
        assert_eq!(
            DeckAction::parse("toggle-start"),
            Some(DeckAction::ToggleStart)
        );
        assert_eq!(DeckAction::parse("toggle-automation"), None);
        assert_eq!(DeckAction::parse(""), None);
    }

    #[test]
    fn the_manual_intent_wins_only_during_the_lockout() {
        let now = Instant::now();
        let manual = ControlIntent {
            speed: 3.0,
            incline: 1,
        };
        let program = ControlIntent {
            speed: 4.0,
            incline: 2,
        };
        let mut state = OrchestratorState {
            manual_intent: Some(manual),
            program_intent: Some(program),
            ..reported()
        };
        state.last_manual_interaction = now;
        assert_eq!(
            state.resolve_base(now + Duration::from_millis(4_999)),
            (3.0, 1, TargetSource::Manual)
        );
        assert_eq!(
            state.resolve_base(now + MANUAL_LOCKOUT),
            (4.0, 2, TargetSource::Program)
        );
        state.program_intent = None;
        assert_eq!(
            state.resolve_base(now + MANUAL_LOCKOUT),
            (3.0, 1, TargetSource::ManualIdle)
        );
        state.manual_intent = None;
        assert_eq!(state.resolve_base(now), (0.0, 0, TargetSource::Unset));
        state.program_intent = Some(program);
        assert_eq!(state.resolve_base(now), (4.0, 2, TargetSource::Program));
        assert_eq!(TargetSource::Unset.to_string(), "NONE");
    }

    #[test]
    fn a_dropout_mid_walk_keeps_the_session() {
        let mut state = reported();
        state.last_status = MachineStatus::Running;
        state.manual_intent = Some(ControlIntent {
            speed: 3.0,
            incline: 0,
        });
        state.session.active_session_id = Some(9);
        state.session.last_physics_update = Some(Instant::now());
        state.reset_connection();
        assert!(state.awaiting_first_packet);
        assert_eq!(state.last_status, MachineStatus::Running);
        assert_eq!(state.session.active_session_id, Some(9));
        assert!(state.manual_intent.is_some());

        // Not moving: everything starts over.
        let mut state = reported();
        state.last_status = MachineStatus::Paused;
        state.manual_intent = Some(ControlIntent {
            speed: 3.0,
            incline: 0,
        });
        state.session.active_session_id = Some(9);
        state.session.last_physics_update = Some(Instant::now());
        state.reset_connection();
        assert_eq!(state.last_status, MachineStatus::Stopped);
        assert_eq!(state.session.active_session_id, None);
        assert!(state.manual_intent.is_none());
    }

    fn saved(start: i64, end: i64, strava: Option<&str>) -> Session {
        Session {
            id: Some(1),
            start_time: start,
            end_time: end,
            duration_seconds: 0,
            distance_meters: 0.0,
            raw_distance_meters: 0.0,
            calories: 0.0,
            steps: 0.0,
            raw_steps: 0.0,
            avg_speed: 0.0,
            raw_avg_speed: 0.0,
            avg_incline: 0.0,
            strava_id: strava.map(String::from),
            neural_heat: 0.0,
            neural_ticks: 0,
            last_hardware_time: 0,
            hardware_time_offset: 0,
        }
    }

    #[test]
    fn resume_only_matches_the_decks_current_workout() {
        let hour = 3_600_000;
        let midnight = 1_000 * hour;
        let now = midnight + 15 * hour;
        // Saved 10 min ago, deck has been going 20 min: same walk.
        assert!(resumable(
            &saved(now - hour, now - 600_000, None),
            1200,
            now,
            midnight
        ));
        // Last saved yesterday evening: new walk even though the deck is counting.
        assert!(!resumable(
            &saved(midnight - 3 * hour, midnight - 2 * hour, None),
            1200,
            now,
            midnight
        ));
        // Earlier today but long before this workout (+2 h pause slack) began.
        assert!(!resumable(
            &saved(now - 6 * hour, now - 5 * hour, None),
            1200,
            now,
            midnight
        ));
        // Already on Strava: never extend it.
        assert!(!resumable(
            &saved(now - hour, now - 600_000, Some("42")),
            1200,
            now,
            midnight
        ));
    }

    #[test]
    fn session_averages_use_session_time_and_moving_incline() {
        let mut state = OrchestratorState::default();
        state.session.duration_seconds = 1800;
        state.session.distance_meters = 1500.0;
        state.session.raw_distance_meters = 1650.0;
        // 10 min at 4% and 20 min at 7% of moving time -> 6%.
        state.session.moving_seconds = 1800.0;
        state.session.incline_pct_seconds = 600.0 * 4.0 + 1200.0 * 7.0;
        let (speed, raw, incline) = state.session_averages();
        assert!((speed - 3.0).abs() < 1e-4);
        assert!((raw - 3.3).abs() < 1e-4);
        assert!((incline - 6.0).abs() < 1e-4);

        // Nothing walked yet: zeros, not NaN.
        assert_eq!(
            OrchestratorState::default().session_averages(),
            (0.0, 0.0, 0.0)
        );
    }

    fn point(speed: f32, stride: f32, scale: f32) -> CalibrationPoint {
        CalibrationPoint {
            speed,
            stride_length_cm: stride,
            distance_scale: scale,
        }
    }

    #[test]
    fn test_speed_scaling_clamped() {
        let points = vec![point(1.0, 0.0, 1.0), point(10.0, 0.0, 2.0)];
        // Physical 10.0 sits past the last point (physical 5.0), so scale 2.0:
        // hardware 20.0, clamped to the 12.0 maximum, byte 120.
        let (raw, _) = calculate_hardware_bytes(10.0, 0, &points, 12.0);
        assert_eq!(raw, 120);
    }

    #[test]
    fn hardware_bytes_interpolate_the_scale() {
        // Hardware 2.0 is 2.0 km/h (scale 1.0), hardware 5.0 is 4.0 (5.0 / 1.25).
        let points = vec![point(5.0, 70.0, 1.25), point(2.0, 60.0, 1.0)];
        assert_eq!(calculate_hardware_bytes(2.0, 3, &points, 12.0), (20, 3));
        assert_eq!(calculate_hardware_bytes(4.0, 3, &points, 12.0), (50, 3));
        // Between them the scale runs along the hardware axis, as in
        // `to_physical_speed`: hardware 3.3 has scale 1.1, so 3.0 km/h.
        assert_eq!(calculate_hardware_bytes(3.0, 3, &points, 12.0).0, 33);
        // Below the first point the first scale holds.
        assert_eq!(calculate_hardware_bytes(1.0, 0, &points, 12.0).0, 10);
        // No calibration: tenths of km/h, clamped.
        assert_eq!(calculate_hardware_bytes(3.46, 0, &[], 12.0).0, 35);
        assert_eq!(calculate_hardware_bytes(30.0, 0, &[], 12.0).0, 120);
        assert_eq!(calculate_hardware_bytes(-1.0, 0, &[], 12.0).0, 0);
    }

    #[test]
    fn physical_speed_and_stride_follow_the_calibration() {
        let mut state = OrchestratorState::default();
        let config = persistence::AppConfig {
            calibration_points: vec![point(5.0, 70.0, 1.25), point(2.0, 60.0, 1.0)],
            ..Default::default()
        };
        state.sync_config(&config);
        assert_eq!(state.sorted_points[0].speed, 2.0);
        assert!((state.to_physical_speed(2.0) - 2.0).abs() < 1e-5);
        assert!((state.to_physical_speed(5.0) - 4.0).abs() < 1e-5);
        // Past the table (12.0 + headroom) the curve is computed directly.
        assert!((state.to_physical_speed(20.0) - 16.0).abs() < 1e-4);
        // Stride: by physical speed, held at the ends, interpolated between.
        assert_eq!(state.calculate_stride_length(1.0), 60.0);
        assert_eq!(state.calculate_stride_length(9.0), 70.0);
        assert!((state.calculate_stride_length(3.0) - 65.0).abs() < 1e-4);
        assert_eq!(
            OrchestratorState::default().calculate_stride_length(3.0),
            DEFAULT_STRIDE_CM
        );
        assert_eq!(OrchestratorState::default().to_physical_speed(3.3), 3.3);
    }

    #[test]
    fn speed_nudges_move_the_hardware_byte_by_one() {
        let mut state = OrchestratorState::default();
        let config = persistence::AppConfig {
            calibration_points: vec![point(2.0, 60.0, 0.9), point(8.0, 70.0, 0.9)],
            ..Default::default()
        };
        state.sync_config(&config);
        state.manual_intent = Some(ControlIntent {
            speed: 3.0,
            incline: 0,
        });
        let byte = |s: &OrchestratorState| {
            calculate_hardware_bytes(
                s.manual_intent.unwrap().speed,
                0,
                &s.calibration_points,
                s.max_speed,
            )
            .0
        };
        let before = byte(&state);
        state.trigger_action(DeckAction::SpeedUp);
        assert_eq!(byte(&state), before + 1);
        state.trigger_action(DeckAction::SpeedDown);
        state.trigger_action(DeckAction::SpeedDown);
        assert_eq!(byte(&state), before - 1);
        state.trigger_action(DeckAction::InclineUp);
        assert_eq!(state.manual_intent.unwrap().incline, 1);
        state.trigger_action(DeckAction::InclineDown);
        state.trigger_action(DeckAction::InclineDown);
        assert_eq!(state.manual_intent.unwrap().incline, 0);
    }

    /// The curve from issue #60: its physical speeds go up, then down, then
    /// up again (hardware 1.5 reads faster than hardware 2.0).
    fn non_monotonic_calibration(max_speed: f32) -> persistence::AppConfig {
        persistence::AppConfig {
            calibration_points: vec![
                point(0.6, 50.0, 0.6 / 1.46),
                point(1.5, 60.0, 1.5 / 4.84),
                point(2.0, 65.0, 2.0 / 2.97),
            ],
            max_speed,
            ..Default::default()
        }
    }

    fn monotonic_calibration(max_speed: f32) -> persistence::AppConfig {
        persistence::AppConfig {
            calibration_points: vec![point(1.0, 50.0, 0.8), point(4.0, 70.0, 0.95)],
            max_speed,
            ..Default::default()
        }
    }

    /// The byte the transmit path sends for the current manual intent.
    fn manual_byte(s: &OrchestratorState) -> u8 {
        calculate_hardware_bytes(
            s.manual_intent.map_or(0.0, |i| i.speed),
            0,
            &s.calibration_points,
            s.max_speed,
        )
        .0
    }

    /// Every speed-up press moves the deck up a byte until the top speed, and
    /// every speed-down press down a byte until zero, whatever the curve.
    #[test]
    fn speed_nudges_step_every_byte_from_zero_to_max_and_back() {
        for config in [
            non_monotonic_calibration(5.0),
            monotonic_calibration(5.0),
            persistence::AppConfig {
                max_speed: 5.0,
                ..Default::default()
            },
        ] {
            let mut state = OrchestratorState::default();
            state.sync_config(&config);
            let max_byte = (state.max_speed * 10.0).round() as u8;
            let mut byte = manual_byte(&state);
            assert_eq!(byte, 0);
            for _ in 0..max_byte {
                state.trigger_action(DeckAction::SpeedUp);
                let next = manual_byte(&state);
                assert_eq!(next, byte + 1, "up from byte {byte}: {config:?}");
                byte = next;
            }
            state.trigger_action(DeckAction::SpeedUp);
            assert_eq!(manual_byte(&state), max_byte, "held at the top");
            for _ in 0..max_byte {
                state.trigger_action(DeckAction::SpeedDown);
                let next = manual_byte(&state);
                assert_eq!(next + 1, byte, "down from byte {byte}: {config:?}");
                byte = next;
            }
            state.trigger_action(DeckAction::SpeedDown);
            assert_eq!(manual_byte(&state), 0, "held at zero");
        }
    }

    /// Forward (hardware to km/h) and inverse (km/h to hardware) lookups
    /// agree: a byte's km/h is sent as that byte.
    #[test]
    fn the_inverse_speed_lookup_undoes_the_forward_one() {
        for config in [non_monotonic_calibration(5.0), monotonic_calibration(12.0)] {
            let mut state = OrchestratorState::default();
            state.sync_config(&config);
            let max_byte = (state.max_speed * 10.0).round() as u8;
            for byte in 0..=max_byte {
                let phys = state.to_physical_speed(byte as f32 / 10.0);
                let back =
                    calculate_hardware_bytes(phys, 0, &state.calibration_points, state.max_speed).0;
                assert_eq!(back, byte, "byte {byte} -> {phys} km/h -> {back}");
            }
        }
    }

    #[test]
    fn a_tilt_calibration_change_applies_without_a_reconnect() {
        let mut state = reported();
        state.sync_config(&persistence::AppConfig::default());
        assert_eq!(state.incline_pct(4), 4.0);
        let config = persistence::AppConfig {
            tilt_multiplier: 0.5,
            incline_offset: 1.0,
            ..Default::default()
        };
        state.sync_config(&config);
        assert_eq!(state.tilt_multiplier, 0.5);
        assert_eq!(state.incline_offset, 1.0);
        assert_eq!(state.incline_pct(4), 3.0);
    }

    /// Mid-walk, a new tilt or offset relabels the deck's level; the incline
    /// byte (and so the deck) stays where it is.
    #[test]
    fn a_tilt_change_mid_walk_sends_nothing_new() {
        let mut engine = synced();
        engine.last_sent_speed = 30;
        engine.last_sent_incline = 4;
        let mut state = reported();
        state.sync_config(&persistence::AppConfig::default());
        state.last_status = MachineStatus::Running;
        state.hardware_speed = 3.0;
        state.hardware_incline = 4;
        state.manual_intent = Some(ControlIntent {
            speed: 3.0,
            incline: 4,
        });
        let now = Instant::now();
        state.last_poll_time = now;
        assert_eq!(engine.determine_next_packet(&mut state, now), None);

        state.sync_config(&persistence::AppConfig {
            tilt_multiplier: 0.5,
            incline_offset: 1.0,
            ..Default::default()
        });
        let later = now + Duration::from_millis(200);
        state.last_poll_time = later;
        assert_eq!(engine.determine_next_packet(&mut state, later), None);
        assert_eq!(engine.last_sent_incline, 4);
        let (_, level, _) = state.resolve_target();
        assert_eq!(level, 4);
        // The same level, shown in the new calibration: 4 * 0.5 + 1.0.
        assert_eq!(state.incline_pct(level), 3.0);
    }

    // -----------------------------------------------------------------------
    // The belt never starts by itself after a restart or reconnect
    // -----------------------------------------------------------------------

    /// A calibration whose two directions disagree by a byte between points.
    fn skewed_calibration() -> persistence::AppConfig {
        persistence::AppConfig {
            calibration_points: vec![point(1.0, 50.0, 0.6), point(4.0, 70.0, 0.9)],
            ..Default::default()
        }
    }

    /// Take one status frame, as the status listener does.
    fn frame(state: &mut OrchestratorState, status: MachineStatus, hw_speed: f32, incline: u8) {
        state.last_status = status;
        state.hardware_speed = hw_speed;
        state.hardware_incline = incline;
        state.awaiting_first_packet = false;
    }

    /// Anything sent that would move the belt: a start or resume handshake,
    /// or a new speed or incline.
    fn moves_belt(sent: &[PacketKind]) -> bool {
        sent.iter().any(|k| {
            matches!(
                k,
                PacketKind::HandshakeReady | PacketKind::HandshakeStart | PacketKind::ControlSet
            )
        })
    }

    #[test]
    fn the_physical_speed_for_a_byte_maps_back_to_that_byte() {
        for config in [persistence::AppConfig::default(), skewed_calibration()] {
            let mut state = OrchestratorState::default();
            state.sync_config(&config);
            for byte in 0..=120u8 {
                let phys = state.physical_for_byte(byte, 0);
                let back = calculate_hardware_bytes(phys, 0, &state.calibration_points, 12.0).0;
                assert_eq!(back, byte, "byte {byte} -> {phys} km/h -> {back}");
            }
        }
    }

    /// The app restarts (an update, say) while the belt keeps going: the new
    /// connection adopts the belt and sends it nothing but status polls, past
    /// the manual lockout, whatever the calibration.
    #[test]
    fn a_restart_adopts_a_moving_belt_without_sending_it_anything() {
        for config in [persistence::AppConfig::default(), skewed_calibration()] {
            for hw in [1.2_f32, 2.5, 3.0, 4.4] {
                let mut state = OrchestratorState::default();
                state.sync_config(&config);
                state.reset_connection();
                assert!(state.hold_auto_start);
                frame(&mut state, MachineStatus::Running, hw, 3);
                let mut engine = ProtocolEngine::new();
                let sent = kinds(&mut engine, &mut state, Instant::now(), 200);
                assert!(
                    sent.iter().all(|k| *k == PacketKind::PollStatus),
                    "{hw} km/h: {sent:?}"
                );
                assert!(!state.hold_auto_start, "a moving belt releases the hold");
                let intent = state.manual_intent.unwrap();
                assert_eq!(intent.incline, 3);
                let byte = calculate_hardware_bytes(
                    intent.speed,
                    3,
                    &state.calibration_points,
                    state.max_speed,
                )
                .0;
                assert_eq!(byte, (hw * 10.0).round() as u8);
            }
        }
    }

    /// A dropout mid-walk keeps the intent; the belt is still running at it on
    /// the reconnect, so nothing is sent.
    #[test]
    fn a_dropout_reconnect_to_the_same_walk_sends_nothing() {
        let mut state = reported();
        state.sync_config(&skewed_calibration());
        let speed = state.physical_for_byte(30, 2);
        state.manual_intent = Some(ControlIntent { speed, incline: 2 });
        state.last_status = MachineStatus::Running;
        state.hardware_speed = 3.0;
        state.session.last_physics_update = Some(Instant::now());
        state.reset_connection();
        frame(&mut state, MachineStatus::Running, 3.0, 2);
        let mut engine = ProtocolEngine::new();
        let sent = kinds(&mut engine, &mut state, Instant::now(), 200);
        assert!(
            sent.iter().all(|k| *k == PacketKind::PollStatus),
            "{sent:?}"
        );
    }

    /// The belt stopped or paused while the link was down (the treadmill's own
    /// buttons): the intent kept from the dropout must not start it again, and
    /// neither may a paused deck that still reports its speed. Engage does.
    #[test]
    fn a_stale_intent_never_starts_a_still_belt_after_a_reconnect() {
        for (status, hw) in [
            (MachineStatus::Paused, 0.0),
            (MachineStatus::Paused, 3.0),
            (MachineStatus::Stopped, 0.0),
        ] {
            let mut state = reported();
            state.manual_intent = Some(ControlIntent {
                speed: 3.0,
                incline: 0,
            });
            state.last_status = MachineStatus::Running;
            state.session.last_physics_update = Some(Instant::now());
            state.reset_connection();
            assert!(state.manual_intent.is_some(), "the dropout kept the intent");
            frame(&mut state, status, hw, 0);
            let mut engine = ProtocolEngine::new();
            let t0 = Instant::now();
            let sent = kinds(&mut engine, &mut state, t0, 200);
            assert!(!moves_belt(&sent), "{status:?} {hw}: {sent:?}");
            assert_eq!(state.handshake_step, HandshakeStep::Idle);

            state.trigger_action(DeckAction::Start);
            state.start_requested_at = Some(t0 + Duration::from_secs(10));
            let sent = kinds(&mut engine, &mut state, t0 + Duration::from_secs(10), 20);
            assert!(
                sent.contains(&PacketKind::HandshakeStart),
                "{status:?}: {sent:?}"
            );
        }

        // A fresh start (no intent kept) on a paused deck reporting its speed.
        let mut state = OrchestratorState::default();
        state.reset_connection();
        frame(&mut state, MachineStatus::Paused, 3.0, 0);
        let mut engine = ProtocolEngine::new();
        let sent = kinds(&mut engine, &mut state, Instant::now(), 200);
        assert!(!moves_belt(&sent), "{sent:?}");
    }

    /// A start half-sent when the link dropped is not finished on the reconnect.
    #[test]
    fn a_handshake_in_flight_is_dropped_by_a_reconnect() {
        let mut state = reported();
        state.handshake_step = HandshakeStep::Starting;
        state.reset_connection();
        assert_eq!(state.handshake_step, HandshakeStep::Idle);
        frame(&mut state, MachineStatus::Stopped, 0.0, 0);
        let sent = kinds(&mut synced(), &mut state, Instant::now(), 40);
        assert!(!moves_belt(&sent), "{sent:?}");
    }

    /// Setting a speed by hand on a still belt starts it, as before.
    #[test]
    fn a_speed_set_by_hand_after_a_reconnect_starts_the_belt() {
        let mut state = OrchestratorState::default();
        state.reset_connection();
        frame(&mut state, MachineStatus::Stopped, 0.0, 0);
        let mut engine = ProtocolEngine::new();
        let t0 = Instant::now();
        assert!(!moves_belt(&kinds(&mut engine, &mut state, t0, 40)));
        crate::bluetooth::apply_manual_intent(&mut state, Some(3.0), None);
        assert!(!state.hold_auto_start);
        let sent = kinds(&mut engine, &mut state, t0 + Duration::from_secs(3), 40);
        assert!(sent.contains(&PacketKind::HandshakeStart), "{sent:?}");
    }

    /// A speed nudge on a still belt is a speed set by hand: it starts the
    /// belt, one step up from the target shown (here 0, so at min speed).
    #[test]
    fn a_speed_nudge_on_a_still_belt_starts_it() {
        for status in [MachineStatus::Stopped, MachineStatus::Paused] {
            let mut state = reported();
            frame(&mut state, status, 0.0, 1);
            state.manual_intent = Some(ControlIntent {
                speed: 0.0,
                incline: 1,
            });
            state.hold_auto_start = true;
            let mut engine = synced();
            let t0 = Instant::now();
            assert!(!moves_belt(&kinds(&mut engine, &mut state, t0, 40)));
            state.trigger_action(DeckAction::SpeedUp);
            assert!(!state.hold_auto_start, "{status:?}");
            let m = state.manual_intent.unwrap();
            assert!((m.speed - 0.1).abs() < 0.01, "{status:?}: {m:?}");
            assert_eq!(
                state.resolve_target().0,
                state.min_physical_speed(),
                "{status:?}"
            );
            let sent = kinds(&mut engine, &mut state, t0 + Duration::from_secs(3), 40);
            assert!(
                sent.contains(&PacketKind::HandshakeStart),
                "{status:?}: {sent:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Replays of a traffic recording (Jogwell Ares 11 Elite, 2026-11-15)
    // -----------------------------------------------------------------------

    /// A full 0x51 frame as the deck sends it (17 bytes, counters filled in).
    fn deck_frame(status: u8, speed: u8, time_s: u16) -> Vec<u8> {
        let [lo, hi] = time_s.to_le_bytes();
        let mut f = vec![
            0x02, 0x51, status, speed, 0x00, lo, hi, 0x0b, 0x00, 0x0f, 0x00, 0x2d, 0x00, 0x00, 0x00,
        ];
        f.push(f.iter().skip(1).fold(0, |a, b| a ^ b));
        f.push(0x03);
        f
    }

    /// The short countdown frame `02 51 02 <n>`.
    fn countdown_frame(n: u8) -> Vec<u8> {
        vec![0x02, 0x51, 0x02, n, 0x51 ^ 0x02 ^ n, 0x03]
    }

    const IDLE_FRAME: [u8; 5] = [0x02, 0x51, 0x00, 0x51, 0x03];
    /// Standby, as recorded after a power-off held on the remote.
    const STANDBY_FRAME: [u8; 6] = [0x02, 0x51, 0x06, 0x02, 0x55, 0x03];

    enum Ev {
        /// A status frame arrives (and keeps arriving every 600 ms until the next).
        Rx(Vec<u8>),
        Press(DeckAction),
    }

    /// The real engine fed a recorded sequence: frames go through
    /// `bluetooth::apply_telemetry`, the session opens and closes as
    /// `manage_session_lifecycle` does, and every packet sent is kept.
    struct Replay {
        engine: ProtocolEngine,
        state: OrchestratorState,
        t0: Instant,
        sent: Vec<(u64, Packet)>,
        sessions_started: u32,
        /// The duration each finalised session was saved with.
        finalised: Vec<i64>,
    }

    impl Replay {
        fn new() -> Self {
            let mut state = reported();
            // The user's own Engage releases the hold; a deck seen moving too.
            state.hold_auto_start = false;
            Self {
                engine: synced(),
                state,
                t0: Instant::now(),
                sent: vec![],
                sessions_started: 0,
                finalised: vec![],
            }
        }

        fn rx(&mut self, bytes: &[u8]) {
            let t = protocol::parse_status(bytes).expect("a status frame");
            crate::bluetooth::apply_telemetry(&mut self.state, &t);
            self.state.update_physics();
            if self.state.should_start_session() {
                self.sessions_started += 1;
                self.state.start_session(self.sessions_started as i64);
            }
            if self.state.should_stop_session() {
                self.finalised.push(self.state.session.duration_seconds);
                self.state.stop_session();
            }
        }

        /// Run `events` (ms from now, in order) until `until_ms`, ticking the
        /// engine every 50 ms and repeating the latest frame every 600 ms as
        /// the deck answers polls.
        fn play(&mut self, events: Vec<(u64, Ev)>, until_ms: u64) {
            let mut events = events.into_iter().peekable();
            let mut last_frame: Option<(u64, Vec<u8>)> = None;
            let mut ms = 0;
            while ms <= until_ms {
                let now = self.t0 + Duration::from_millis(ms);
                self.state.set_test_now(now);
                while let Some((_, ev)) = events.next_if(|(at, _)| *at <= ms) {
                    match ev {
                        Ev::Rx(bytes) => {
                            self.rx(&bytes);
                            last_frame = Some((ms, bytes));
                        }
                        Ev::Press(action) => self.state.trigger_action(action),
                    }
                }
                if let Some((at, bytes)) = last_frame.clone() {
                    if ms > at && (ms - at) % 600 == 0 {
                        self.rx(&bytes);
                    }
                }
                if let Some(packet) = self.engine.determine_next_packet(&mut self.state, now) {
                    self.state.last_cmd_time = now;
                    self.sent.push((ms, packet));
                }
                ms += 50;
            }
            self.t0 += Duration::from_millis(until_ms + 50);
        }

        fn count(&self, kind: PacketKind) -> usize {
            self.sent.iter().filter(|(_, p)| p.kind == kind).count()
        }

        fn controls(&self, control: Control) -> usize {
            let bytes = protocol::control_packet(control, 0, 0);
            self.sent.iter().filter(|(_, p)| p.bytes == bytes).count()
        }

        /// Everything sent but status polls.
        fn commands(&self) -> Vec<PacketKind> {
            self.sent
                .iter()
                .map(|(_, p)| p.kind)
                .filter(|k| *k != PacketKind::PollStatus)
                .collect()
        }
    }

    /// Engage on an idle deck as recorded: READY, the deck counts down 3-2-1
    /// in status 2, then runs (3) at its own 0.6 km/h.
    fn engage_events() -> Vec<(u64, Ev)> {
        vec![
            (0, Ev::Rx(IDLE_FRAME.to_vec())),
            (300, Ev::Press(DeckAction::ToggleStart)),
            (660, Ev::Rx(countdown_frame(3))),
            (1620, Ev::Rx(countdown_frame(2))),
            (2585, Ev::Rx(countdown_frame(1))),
            (4140, Ev::Rx(deck_frame(3, 0x06, 0))),
        ]
    }

    /// Running frames, one a second, from `from_s` to `to_s` of deck time.
    fn walking(at_ms: u64, from_s: u16, to_s: u16) -> Vec<(u64, Ev)> {
        (from_s..=to_s)
            .map(|s| {
                (
                    at_ms + (s - from_s) as u64 * 1000,
                    Ev::Rx(deck_frame(3, 0x0b, s)),
                )
            })
            .collect()
    }

    #[test]
    fn engage_sends_one_ready_and_one_start_through_the_countdown() {
        let mut r = Replay::new();
        r.play(engage_events(), 6000);
        assert_eq!(r.count(PacketKind::HandshakeReady), 1, "{:?}", r.commands());
        assert_eq!(r.count(PacketKind::HandshakeStart), 1, "{:?}", r.commands());
        assert!(
            r.state.start_requested_at.is_none(),
            "the countdown answered"
        );
        assert!(!r.state.start_blocked);
        // Nothing but polls during the countdown; the target once running.
        let first_running_ms = 4140;
        assert!(r
            .sent
            .iter()
            .filter(|(_, p)| p.kind == PacketKind::ControlSet)
            .all(|(t, _)| *t >= first_running_ms));
        assert_eq!(r.state.last_status, MachineStatus::Running);
        assert_eq!(r.sessions_started, 1);
    }

    #[test]
    fn a_deck_that_never_counts_down_still_gets_the_engage_retries_and_timeout() {
        let mut r = Replay::new();
        r.play(
            vec![
                (0, Ev::Rx(IDLE_FRAME.to_vec())),
                (300, Ev::Press(DeckAction::ToggleStart)),
            ],
            9000,
        );
        assert!(
            r.count(PacketKind::HandshakeReady) > 1,
            "{:?}",
            r.commands()
        );
        assert!(r.state.start_blocked, "gave up after 8 s");
        let late = r.sent.iter().filter(|(t, p)| {
            *t > 8_400
                && matches!(
                    p.kind,
                    PacketKind::HandshakeReady | PacketKind::HandshakeStart
                )
        });
        assert_eq!(late.count(), 0);
    }

    #[test]
    fn dump_sends_one_stop_and_no_pause_while_the_deck_winds_down() {
        let mut r = Replay::new();
        r.play(engage_events(), 4500);
        r.play(walking(0, 1, 30), 30_000);
        assert_eq!(r.sessions_started, 1);
        r.sent.clear();
        // As recorded: stop, the belt winds down in 4, the workout ends (1)
        // about 1.3 s after it is still, and 0 three seconds later.
        r.play(
            vec![
                (0, Ev::Rx(deck_frame(3, 0x0b, 36))),
                (100, Ev::Press(DeckAction::Stop)),
                (520, Ev::Rx(deck_frame(4, 0x08, 36))),
                (1120, Ev::Rx(deck_frame(4, 0x05, 36))),
                (1840, Ev::Rx(deck_frame(4, 0x01, 36))),
                (2680, Ev::Rx(deck_frame(4, 0x00, 36))),
                (4000, Ev::Rx(deck_frame(1, 0x00, 36))),
                (7120, Ev::Rx(IDLE_FRAME.to_vec())),
            ],
            10_000,
        );
        assert_eq!(r.controls(Control::Stop), 1, "{:?}", r.commands());
        assert_eq!(r.commands(), vec![PacketKind::AtomicAction]);
        assert_eq!(r.controls(Control::Pause), 0);
        // Saved once, when the deck said the workout ended, with its time.
        assert_eq!(r.finalised, vec![36]);
        assert_eq!(r.sessions_started, 1);
        assert!(!r.state.stop_requested);
        assert_eq!(r.state.last_status, MachineStatus::Stopped);
    }

    /// Status 1 (workout ended) is not a running belt: no session opens, no
    /// key counting for Neural flow, and no pause is triggered.
    #[test]
    fn a_workout_ended_frame_is_a_still_belt() {
        let mut r = Replay::new();
        r.state.focus_stability_enabled = true;
        r.play(vec![(0, Ev::Rx(deck_frame(1, 0x00, 36)))], 3_000);
        assert!(!r.state.neural_monitor_wanted());
        assert_eq!(r.sessions_started, 0);
        assert_eq!(r.commands(), Vec::<PacketKind>::new());
    }

    #[test]
    fn nothing_is_sent_while_the_belt_spins_down_even_on_a_press() {
        let mut r = Replay::new();
        r.play(engage_events(), 4500);
        r.play(walking(0, 1, 5), 5_000);
        r.sent.clear();
        r.play(
            vec![
                (100, Ev::Press(DeckAction::Stop)),
                (520, Ev::Rx(deck_frame(4, 0x08, 6))),
                (700, Ev::Press(DeckAction::Stop)),
                (900, Ev::Press(DeckAction::Pause)),
                (1000, Ev::Press(DeckAction::ToggleStart)),
                (1120, Ev::Rx(deck_frame(4, 0x05, 6))),
                (2000, Ev::Rx(deck_frame(1, 0x00, 6))),
                (5000, Ev::Rx(IDLE_FRAME.to_vec())),
            ],
            9_000,
        );
        assert_eq!(r.commands(), vec![PacketKind::AtomicAction], "{:?}", r.sent);
        // The Engage was refused (and said so), not kept for later: the deck
        // reaching 0 doesn't start the belt.
        assert!(r.state.start_requested_at.is_none());
        assert!(r.state.start_blocked);
        assert_eq!(r.state.last_status, MachineStatus::Stopped);
    }

    #[test]
    fn pause_then_resume_through_the_countdown() {
        let mut r = Replay::new();
        r.play(engage_events(), 4500);
        r.play(walking(0, 1, 30), 30_000);
        r.sent.clear();
        r.play(
            vec![
                (0, Ev::Rx(deck_frame(3, 0x0b, 31))),
                (100, Ev::Press(DeckAction::ToggleStart)),
                (520, Ev::Rx(deck_frame(10, 0x08, 31))),
                (1120, Ev::Rx(deck_frame(10, 0x05, 31))),
                (1720, Ev::Rx(deck_frame(10, 0x02, 31))),
                (2500, Ev::Rx(deck_frame(10, 0x00, 31))),
            ],
            5_000,
        );
        assert_eq!(r.controls(Control::Pause), 1, "{:?}", r.commands());
        assert_eq!(r.commands(), vec![PacketKind::AtomicAction]);
        r.sent.clear();
        // Resume as recorded: READY, the countdown (2), then running (3).
        r.play(
            vec![
                (0, Ev::Press(DeckAction::ToggleStart)),
                (360, Ev::Rx(countdown_frame(3))),
                (1560, Ev::Rx(countdown_frame(2))),
                (2280, Ev::Rx(countdown_frame(1))),
                (3480, Ev::Rx(deck_frame(3, 0x0b, 31))),
            ],
            6_000,
        );
        assert_eq!(r.count(PacketKind::HandshakeReady), 1, "{:?}", r.commands());
        assert_eq!(r.count(PacketKind::HandshakeStart), 1, "{:?}", r.commands());
        assert_eq!(r.sessions_started, 1, "a resume continues the session");
        assert!(r.finalised.is_empty());
    }

    #[test]
    fn a_pause_that_ends_on_the_deck_saves_the_session_once_and_sends_nothing() {
        let mut r = Replay::new();
        r.play(engage_events(), 4500);
        r.play(walking(0, 1, 20), 20_000);
        r.play(
            vec![
                (0, Ev::Press(DeckAction::Pause)),
                (400, Ev::Rx(deck_frame(10, 0x05, 21))),
                (1500, Ev::Rx(deck_frame(10, 0x00, 21))),
            ],
            3_000,
        );
        r.sent.clear();
        // The deck's own pause timeout (or a stop at the treadmill): 10, 1, 0.
        r.play(
            vec![
                (0, Ev::Rx(deck_frame(1, 0x00, 21))),
                (3000, Ev::Rx(IDLE_FRAME.to_vec())),
            ],
            8_000,
        );
        assert_eq!(r.commands(), Vec::<PacketKind>::new());
        assert_eq!(r.finalised, vec![21]);
        assert_eq!(r.sessions_started, 1);
    }

    /// Everything that starts the belt, pressed on a deck in standby: no
    /// start goes out, the refusal is at once, and the deck waking (6, then 0)
    /// starts nothing by itself.
    #[test]
    fn a_start_on_a_deck_in_standby_sends_nothing_and_says_so() {
        let presses: [fn(&mut OrchestratorState); 4] = [
            |s| s.trigger_action(DeckAction::ToggleStart),
            |s| s.trigger_action(DeckAction::Start),
            |s| s.set_intent(Some(3.0), None, true),
            |s| {
                s.programs = vec![Program {
                    id: 1,
                    name: "Walk".into(),
                    steps: vec![crate::programs::ProgramStep {
                        duration_minutes: Some(10.0),
                        speed_kmh: 3.0,
                        incline_pct: 0.0,
                    }],
                    repeat_every_minutes: None,
                    note: None,
                }];
                let now = s.now();
                s.run_program(1, now).unwrap();
            },
        ];
        for (i, press) in presses.into_iter().enumerate() {
            let mut r = Replay::new();
            r.state.hold_auto_start = true;
            r.play(vec![(0, Ev::Rx(STANDBY_FRAME.to_vec()))], 1_000);
            assert!(r.state.deck_standby);
            press(&mut r.state);
            // Refused before any packet: nothing waits for an answer.
            assert!(r.state.start_requested_at.is_none(), "press {i}");
            assert_eq!(r.state.start_refusal(), Some(crate::link::START_STANDBY));
            r.play(vec![], 9_000);
            assert!(r.state.start_blocked, "press {i}");
            // The deck wakes: still nothing starts the belt.
            r.play(
                vec![
                    (0, Ev::Rx(IDLE_FRAME.to_vec())),
                    (6_000, Ev::Rx(IDLE_FRAME.to_vec())),
                ],
                12_000,
            );
            assert!(!r.state.deck_standby);
            assert_eq!(r.commands(), Vec::<PacketKind>::new(), "press {i}");
            // Engage again, now it is awake: a real start.
            r.state.trigger_action(DeckAction::Start);
            r.play(vec![], 1_000);
            assert!(
                r.commands().contains(&PacketKind::HandshakeReady),
                "press {i}"
            );
        }
    }

    #[test]
    fn a_start_at_the_treadmill_counts_down_and_opens_one_session() {
        let mut r = Replay::new();
        r.state.hold_auto_start = true;
        r.play(
            vec![
                (0, Ev::Rx(IDLE_FRAME.to_vec())),
                (1000, Ev::Rx(countdown_frame(3))),
                (2000, Ev::Rx(countdown_frame(2))),
                (3000, Ev::Rx(countdown_frame(1))),
                (4000, Ev::Rx(deck_frame(3, 0x06, 0))),
            ],
            6_000,
        );
        // The app sends no start of its own (the deck's speed may be echoed
        // back as the target once it runs).
        assert!(!r.commands().iter().any(|k| matches!(
            k,
            PacketKind::HandshakeReady | PacketKind::HandshakeStart | PacketKind::HandshakePause
        )));
        assert!(!r.state.hold_auto_start, "seen moving");
        assert_eq!(r.sessions_started, 1);
    }
}
