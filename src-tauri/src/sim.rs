//! A simulated FitShow deck (the Jogwell Ares 11 Elite) at the byte level,
//! and scenario tests that run the real engine and programs against it.
//! Test and dev builds only: `lib.rs` has
//! `#[cfg(any(test, debug_assertions))] mod sim;`, so release builds never
//! contain it. Dev builds drive the same deck through `sim_link.rs` (the
//! "simulated treadmill" in the Bluetooth scan list); the [`Rig`] and the
//! scenarios are test-only.
//!
//! [`SimDeck`] takes the exact packets the engine writes and answers the way
//! the deck does (docs/protocol.md): a status frame for each poll, an echo or
//! a nack for each control packet. It models the 3-2-1 countdown, the speed
//! ramp, the pause and stop wind-downs, standby (screen off), power-off and
//! the link dropping. [`Rig`] wires the real `ProtocolEngine`,
//! `OrchestratorState` and program tick to it on the test clock, the way
//! `bluetooth::spawn_protocol_worker` and the status listener do.
//!
//! Add a scenario at the bottom of this file for any change to how the app
//! drives the belt.
//!
//! Timings come from the traffic recording the engine's replay tests use
//! (engine.rs, "Replays of a traffic recording", 2026-11-15) and from
//! docs/protocol.md. Ones nobody has measured say "assumed".

#[cfg(test)]
use crate::bluetooth::apply_telemetry;
#[cfg(test)]
use crate::engine::{OrchestratorState, Packet, PacketKind, ProtocolEngine};
use crate::protocol::{self, Control};
use std::time::{Duration, Instant};

/// A write is answered this long after it: the poll at 600 ms and its frame
/// at 655 ms in the docs/protocol.md sample recording.
const REPLY_DELAY: Duration = Duration::from_millis(55);
/// Each count of the 3-2-1 countdown: recorded frames for 3, 2, 1 at 660,
/// 1620 and 2585 ms, about a second apart.
const COUNT_STEP: Duration = Duration::from_millis(1000);
/// READY to running: recorded READY at about 300 ms, first running frame at
/// 4140 ms; the polls lag the deck by up to 600 ms, so 3.5 s.
const COUNTDOWN: Duration = Duration::from_millis(3500);
/// The belt's speed when a countdown from stopped ends: the recorded first
/// running frame says 0x06.
const START_KMH: f32 = 0.6;
/// The wind-down in a pause or a stop: recorded 1.1 to 0 km/h in about
/// 2.2 s (0x0b, 08, 05, 01, 00 at 0, 520, 1120, 1840, 2680 ms).
const DECEL_KMH_PER_S: f32 = 0.5;
/// Speeding up toward a new target. Assumed: the same rate as the wind-down.
const ACCEL_KMH_PER_S: f32 = 0.5;
/// After a stop the belt sits still in status 4 this long before 1: recorded
/// still at 2680 ms, status 1 at 4000 ms.
const STOP_SETTLE: Duration = Duration::from_millis(1300);
/// Status 1 (workout ended) shows "for about 3 s, then 0" (docs; recorded
/// 4000 to 7120 ms).
const ENDED_FOR: Duration = Duration::from_millis(3000);
/// An idle deck goes to standby after this long: docs/protocol.md, the log
/// showed byte 6 "about 14 minutes after a stop".
const STANDBY_AFTER: Duration = Duration::from_secs(14 * 60);
/// Assumed counters: a 74 cm stride (the engine's default) and about
/// 0.057 kcal a metre (4 km/h at 75 kg), in the deck's 0.1 kcal units.
const STRIDE_M: f64 = 0.74;
const CALORIES_RAW_PER_M: f64 = 0.57;
/// The physics step, so a long `advance` still ramps smoothly.
const SIM_STEP: Duration = Duration::from_millis(50);

/// The deck's answer to a READY, as docs/protocol.md gives it (no checksum).
const READY_ACK: [u8; 4] = [0x02, 0x53, 0x01, 0x03];
/// The answer to a refused control packet.
const NACK: [u8; 4] = [0x02, 0x53, 0x53, 0x03];

/// What the deck is doing; each maps to one status byte (docs/protocol.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// 0: stopped, idle.
    Idle,
    /// 1: workout ended, totals still shown.
    Ended,
    /// 2: the 3-2-1 countdown.
    Countdown,
    /// 3: running.
    Running,
    /// 4: winding down after a stop.
    Stopping,
    /// 6: standby, screen off.
    Standby,
    /// 10: paused (the belt winds down here too).
    Paused,
}

impl Phase {
    fn code(self) -> u8 {
        match self {
            Phase::Idle => 0,
            Phase::Ended => 1,
            Phase::Countdown => 2,
            Phase::Running => 3,
            Phase::Stopping => 4,
            Phase::Standby => protocol::STATUS_STANDBY,
            Phase::Paused => 10,
        }
    }
}

/// A full 0x51 frame (17 bytes) with every counter, framed by protocol.rs.
fn status_frame(
    code: u8,
    speed: u8,
    incline: u8,
    time_s: u16,
    dist_m: u16,
    cal: u16,
    steps: u16,
) -> Vec<u8> {
    let mut body = vec![0x51, code, speed, incline];
    for v in [time_s, dist_m, cal, steps] {
        body.extend_from_slice(&v.to_le_bytes());
    }
    body.extend_from_slice(&[0, 0]);
    protocol::raw_packet(&body).expect("a status body fits one write")
}

/// The control packet in `bytes`, if it is one protocol.rs would build.
fn decode_control(bytes: &[u8]) -> Option<(Control, u8, u8)> {
    if bytes.len() != 15 || bytes[1] != 0x53 {
        return None;
    }
    let control = [
        Control::Ready,
        Control::Target,
        Control::Stop,
        Control::Start,
        Control::Pause,
    ]
    .into_iter()
    .find(|c| *c as u8 == bytes[2])?;
    (protocol::control_packet(control, bytes[3], bytes[4]) == bytes)
        .then_some((control, bytes[3], bytes[4]))
}

pub(crate) struct SimDeck {
    phase: Phase,
    since: Instant,
    clock: Instant,
    speed: f32,
    /// What the belt runs at; kept across a pause, so a resume returns to it.
    target: f32,
    incline: u8,
    workout_s: f64,
    distance_m: f64,
    still_since: Option<Instant>,
    powered: bool,
    linked: bool,
    outbox: Vec<(Instant, Vec<u8>)>,
    /// Every status byte the deck has been in, each change once.
    pub(crate) history: Vec<u8>,
    /// Control packets refused.
    pub(crate) nacks: u32,
    /// The slowest and fastest speed byte a target is taken at: outside, the
    /// deck clamps and echoes the byte it took.
    floor: u8,
    ceiling: u8,
    /// In-between speeds the deck skips: (asked byte, byte it runs instead).
    skips: Vec<(u8, u8)>,
    /// The highest incline level a target is taken at, and in-between levels
    /// the deck skips: (asked level, level it runs instead).
    incline_ceiling: u8,
    incline_skips: Vec<(u8, u8)>,
}

impl SimDeck {
    /// A deck switched on, idle, not yet connected.
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            phase: Phase::Idle,
            since: now,
            clock: now,
            speed: 0.0,
            target: 0.0,
            incline: 0,
            workout_s: 0.0,
            distance_m: 0.0,
            still_since: None,
            powered: true,
            linked: false,
            outbox: vec![],
            history: vec![0],
            nacks: 0,
            floor: 0,
            ceiling: u8::MAX,
            skips: vec![],
            incline_ceiling: u8::MAX,
            incline_skips: vec![],
        }
    }

    /// A deck that runs only `floor`..=`ceiling` (speed bytes, 0.1 km/h).
    #[cfg(test)]
    pub(crate) fn set_limits(&mut self, floor: u8, ceiling: u8) {
        self.floor = floor;
        self.ceiling = ceiling;
    }

    /// A deck whose top incline level is `ceiling`.
    #[cfg(test)]
    pub(crate) fn set_incline_ceiling(&mut self, ceiling: u8) {
        self.incline_ceiling = ceiling;
    }

    /// A deck that runs incline `took` when asked for `asked`.
    #[cfg(test)]
    pub(crate) fn set_incline_skip(&mut self, asked: u8, took: u8) {
        self.incline_skips.push((asked, took));
    }

    /// A deck that runs `took` when asked for `asked` (speed bytes).
    #[cfg(test)]
    pub(crate) fn set_skip(&mut self, asked: u8, took: u8) {
        self.skips.push((asked, took));
    }

    #[cfg(test)]
    pub(crate) fn status_code(&self) -> u8 {
        self.phase.code()
    }

    #[cfg(test)]
    pub(crate) fn speed_kmh(&self) -> f32 {
        self.speed
    }

    #[cfg(test)]
    pub(crate) fn workout_s(&self) -> u16 {
        self.workout_s as u16
    }

    /// Running at `kmh` (to the 0.1 km/h the deck speaks).
    #[cfg(test)]
    pub(crate) fn running_at(&self, kmh: f32) -> bool {
        self.phase == Phase::Running && (self.speed - kmh).abs() < 0.05
    }

    #[cfg(test)]
    pub(crate) fn belt_still(&self) -> bool {
        self.speed == 0.0 && self.phase != Phase::Running
    }

    fn set(&mut self, phase: Phase, t: Instant) {
        self.phase = phase;
        self.since = t;
        if self.history.last() != Some(&phase.code()) {
            self.history.push(phase.code());
        }
    }

    fn begin_countdown(&mut self, t: Instant) {
        if self.phase != Phase::Paused {
            // A new workout: the deck's own start speed and fresh counters.
            self.target = START_KMH;
            self.workout_s = 0.0;
            self.distance_m = 0.0;
        }
        self.set(Phase::Countdown, t);
    }

    /// Run the deck's own physics up to `now`.
    pub(crate) fn advance(&mut self, now: Instant) {
        while self.clock < now {
            let t = (self.clock + SIM_STEP).min(now);
            let dt = t.duration_since(self.clock).as_secs_f32();
            self.clock = t;
            self.step(t, dt);
        }
    }

    fn step(&mut self, t: Instant, dt: f32) {
        let in_phase = t.saturating_duration_since(self.since);
        match self.phase {
            Phase::Countdown if in_phase >= COUNTDOWN => {
                self.set(Phase::Running, t);
                self.speed = START_KMH.min(self.target);
            }
            Phase::Running => {
                let d = self.target - self.speed;
                let rate = if d > 0.0 {
                    ACCEL_KMH_PER_S
                } else {
                    DECEL_KMH_PER_S
                };
                let step = rate * dt;
                self.speed = if d.abs() <= step {
                    self.target
                } else {
                    self.speed + step.copysign(d)
                };
                self.workout_s += dt as f64;
            }
            Phase::Paused | Phase::Stopping => {
                self.speed = (self.speed - DECEL_KMH_PER_S * dt).max(0.0);
                if self.phase == Phase::Stopping && self.speed == 0.0 {
                    let still = *self.still_since.get_or_insert(t);
                    if t.saturating_duration_since(still) >= STOP_SETTLE {
                        self.set(Phase::Ended, t);
                    }
                }
            }
            Phase::Ended if in_phase >= ENDED_FOR => {
                self.set(Phase::Idle, t);
                self.workout_s = 0.0;
                self.distance_m = 0.0;
                self.target = 0.0;
            }
            Phase::Idle if in_phase >= STANDBY_AFTER => self.set(Phase::Standby, t),
            _ => {}
        }
        self.distance_m += (self.speed / 3.6 * dt) as f64;
    }

    /// The frame the deck answers a status poll with now.
    fn current_frame(&self) -> Vec<u8> {
        let code = self.phase.code();
        let short = |b: &[u8]| protocol::raw_packet(b).expect("short frame");
        match self.phase {
            Phase::Idle => short(&[0x51, code]),
            Phase::Countdown => {
                let counts = self.clock.saturating_duration_since(self.since).as_millis()
                    / COUNT_STEP.as_millis();
                short(&[0x51, code, 3 - counts.min(2) as u8])
            }
            // As recorded after a power-off held on the remote.
            Phase::Standby => short(&[0x51, code, 0x02]),
            _ => status_frame(
                code,
                (self.speed * 10.0).round() as u8,
                self.incline,
                self.workout_s as u16,
                self.distance_m as u16,
                (self.distance_m * CALORIES_RAW_PER_M) as u16,
                (self.distance_m / STRIDE_M) as u16,
            ),
        }
    }

    /// A packet the engine wrote. Nothing reaches a deck that isn't linked.
    pub(crate) fn receive(&mut self, bytes: &[u8], now: Instant) {
        if !self.linked {
            return;
        }
        self.advance(now);
        let reply = if bytes == protocol::status_request() {
            self.current_frame()
        } else {
            self.control(bytes, now)
        };
        self.outbox.push((now + REPLY_DELAY, reply));
    }

    /// Act on a control packet; the echo or nack to send back.
    fn control(&mut self, bytes: &[u8], t: Instant) -> Vec<u8> {
        use Phase::*;
        let Some((control, speed, incline)) = decode_control(bytes) else {
            self.nacks += 1;
            return NACK.to_vec();
        };
        let mut echo_speed = None;
        let mut echo_incline = None;
        let accepted = match (self.phase, control) {
            // Standby refuses every start; the countdown and the stop's
            // wind-down refuse everything but polls (docs/protocol.md).
            (Standby | Countdown | Stopping, _) => false,
            (Idle | Ended | Paused, Control::Ready | Control::Start) => {
                self.begin_countdown(t);
                true
            }
            (Running, Control::Target) => {
                // A speed outside the deck's range is clamped, and the echo
                // below says which byte it took.
                let taken = self
                    .skips
                    .iter()
                    .find(|(asked, _)| *asked == speed)
                    .map_or(speed.clamp(self.floor, self.ceiling), |(_, took)| *took);
                self.target = taken as f32 / 10.0;
                let took_incline = self
                    .incline_skips
                    .iter()
                    .find(|(asked, _)| *asked == incline)
                    .map_or(incline.min(self.incline_ceiling), |(_, took)| *took);
                self.incline = took_incline;
                echo_speed = Some(taken);
                echo_incline = Some(took_incline);
                true
            }
            (Running, Control::Pause) => {
                self.set(Paused, t);
                true
            }
            // Assumed: the handshake's second Pause is a no-op.
            (Paused, Control::Pause) => true,
            (Running, Control::Stop) => {
                self.still_since = None;
                self.set(Stopping, t);
                true
            }
            // A pause that ends: 10, 1, 0.
            (Paused, Control::Stop) => {
                self.set(Ended, t);
                true
            }
            // Assumed: a stop on a still deck is a no-op.
            (Idle | Ended, Control::Stop) => true,
            // Assumed: a target or pause with the belt not running is refused.
            _ => false,
        };
        if !accepted {
            self.nacks += 1;
            return NACK.to_vec();
        }
        match control {
            Control::Ready => READY_ACK.to_vec(),
            Control::Target => protocol::control_packet(
                Control::Target,
                echo_speed.unwrap_or(speed),
                echo_incline.unwrap_or(incline),
            ),
            // `02 53 03 50 03` for a Stop (docs); the same shape assumed for the rest.
            other => protocol::raw_packet(&[0x53, other as u8]).expect("echo"),
        }
    }

    /// The notifications due by `now`.
    pub(crate) fn take_notifications(&mut self, now: Instant) -> Vec<Vec<u8>> {
        self.advance(now);
        let (due, later) = std::mem::take(&mut self.outbox)
            .into_iter()
            .partition(|(at, _)| *at <= now);
        self.outbox = later;
        due.into_iter().map(|(_, b)| b).collect()
    }

    /// Start pressed on the deck's own buttons or remote.
    #[cfg(test)]
    pub(crate) fn press_start(&mut self, now: Instant) {
        self.advance(now);
        if matches!(self.phase, Phase::Idle | Phase::Ended | Phase::Paused) {
            self.begin_countdown(now);
        }
    }

    /// Pause pressed on the deck's own buttons or remote.
    #[cfg(test)]
    pub(crate) fn press_pause(&mut self, now: Instant) {
        self.advance(now);
        if self.phase == Phase::Running {
            self.set(Phase::Paused, now);
        }
    }

    /// Stop pressed on the deck's own buttons or remote, while running.
    #[cfg(test)]
    pub(crate) fn press_stop(&mut self, now: Instant) {
        self.advance(now);
        if self.phase == Phase::Running {
            self.still_since = None;
            self.set(Phase::Stopping, now);
        }
    }

    /// One speed press on the deck's own buttons or remote (`up` or down),
    /// 0.1 km/h, while running: the deck takes it as its own target and ramps
    /// to it, as it does for a target the app sends.
    #[cfg(test)]
    pub(crate) fn press_speed(&mut self, up: bool, now: Instant) {
        self.advance(now);
        if self.phase == Phase::Running {
            let byte = (self.target * 10.0).round() as i32 + if up { 1 } else { -1 };
            self.target = byte.clamp(0, u8::MAX as i32) as f32 / 10.0;
        }
    }

    /// One incline press on the deck's own buttons or remote, while running.
    #[cfg(test)]
    pub(crate) fn press_incline(&mut self, up: bool, now: Instant) {
        self.advance(now);
        if self.phase == Phase::Running {
            self.incline = if up {
                self.incline.saturating_add(1)
            } else {
                self.incline.saturating_sub(1)
            };
        }
    }

    /// The screen goes off (standby), as it does by itself after idling.
    #[cfg(test)]
    pub(crate) fn enter_standby(&mut self, now: Instant) {
        self.advance(now);
        self.set(Phase::Standby, now);
    }

    /// Woken at the deck: back to idle.
    pub(crate) fn wake(&mut self, now: Instant) {
        self.advance(now);
        if self.phase == Phase::Standby {
            self.set(Phase::Idle, now);
        }
    }

    /// Switched off at the wall: the belt stops dead, the link drops, and
    /// nothing answers until it is switched on and connected again.
    #[cfg(test)]
    pub(crate) fn power_off(&mut self, now: Instant) {
        self.advance(now);
        self.powered = false;
        self.linked = false;
        self.outbox.clear();
        self.speed = 0.0;
        self.target = 0.0;
        self.workout_s = 0.0;
        self.distance_m = 0.0;
    }

    /// Switched on again: it boots idle (assumed: counters at zero).
    #[cfg(test)]
    pub(crate) fn power_on(&mut self, now: Instant) {
        self.advance(now);
        self.powered = true;
        self.set(Phase::Idle, now);
    }

    /// The Bluetooth link drops; the deck carries on doing what it was doing.
    pub(crate) fn drop_link(&mut self) {
        self.linked = false;
        self.outbox.clear();
    }

    /// Linked to the app.
    pub(crate) fn linked(&self) -> bool {
        self.linked
    }

    /// Connect; only a deck that is switched on answers.
    pub(crate) fn connect(&mut self) -> bool {
        self.linked = self.powered;
        self.linked
    }
}

/// The real engine and programs driven against a [`SimDeck`] on the test
/// clock: 50 ms worker ticks (200 ms while stopped), the program tick once a
/// second, every packet written to the deck, every notification taken in as
/// the status listener does. Sessions open and close as in the engine's
/// replay tests (no database).
#[cfg(test)]
pub(crate) struct Rig {
    pub(crate) deck: SimDeck,
    pub(crate) state: OrchestratorState,
    engine: ProtocolEngine,
    linked: bool,
    pub(crate) t: Instant,
    last_second: Instant,
    pub(crate) sent: Vec<Packet>,
    pub(crate) sessions_started: u32,
    /// The duration each closed session was saved with.
    pub(crate) finalised: Vec<i64>,
    pub(crate) toasts: Vec<crate::programs::ProgramToast>,
}

#[cfg(test)]
impl Rig {
    /// A fresh app connected to an idle deck, with the seeded programs.
    pub(crate) fn new() -> Self {
        let t = Instant::now();
        let mut state = OrchestratorState {
            programs: crate::programs::classic_programs(),
            program_next_id: 4,
            ..Default::default()
        };
        state.set_test_now(t);
        let mut rig = Self {
            deck: SimDeck::new(t),
            state,
            engine: ProtocolEngine::new(),
            linked: false,
            t,
            last_second: t,
            sent: vec![],
            sessions_started: 0,
            finalised: vec![],
            toasts: vec![],
        };
        assert!(rig.reconnect());
        rig
    }

    /// Connect again, as `bluetooth::connect_to` and a new worker do: reset
    /// the connection state, apply the settings, a new engine. False if the
    /// deck is off.
    pub(crate) fn reconnect(&mut self) -> bool {
        if !self.deck.connect() {
            return false;
        }
        self.state.set_test_now(self.t);
        self.state.reset_connection();
        self.state
            .sync_config(&crate::persistence::AppConfig::default());
        self.engine = ProtocolEngine::new();
        self.linked = true;
        true
    }

    pub(crate) fn drop_link(&mut self) {
        self.deck.drop_link();
        self.linked = false;
    }

    /// Take one notification, as `bluetooth::listen_for_status` does.
    fn rx(&mut self, bytes: &[u8]) {
        let Some(t) = protocol::parse_status(bytes) else {
            if let Some(reply) = protocol::parse_reply(bytes) {
                self.state.set_test_now(self.t);
                self.state.on_deck_reply(reply);
            }
            return;
        };
        apply_telemetry(&mut self.state, &t);
        self.state.update_physics();
        self.open_session_if_due();
        if self.state.should_stop_session() {
            self.finalised.push(self.state.session.duration_seconds);
            self.state.stop_session();
        }
    }

    fn open_session_if_due(&mut self) {
        if self.state.should_start_session() {
            self.sessions_started += 1;
            self.state.start_session(self.sessions_started as i64);
        }
    }

    /// One worker tick at `self.t`.
    fn tick(&mut self) {
        let now = self.t;
        self.state.set_test_now(now);
        self.deck.advance(now);
        if !self.linked {
            return; // no worker without a link; the deck carries on
        }
        for frame in self.deck.take_notifications(now) {
            self.rx(&frame);
        }
        if now.duration_since(self.last_second) >= Duration::from_secs(1) {
            self.last_second = now;
            self.state.tick_profile_sweep(now);
            self.toasts.extend(self.state.tick_program(now));
        }
        if let Some(packet) = self.engine.determine_next_packet(&mut self.state, now) {
            self.state.last_cmd_time = now;
            self.deck.receive(&packet.bytes, now);
            self.sent.push(packet);
        }
        self.toasts.extend(self.state.take_program_toast());
        self.open_session_if_due();
    }

    /// Run for `secs` seconds of simulated time.
    pub(crate) fn run(&mut self, secs: f64) {
        let end = self.t + Duration::from_secs_f64(secs);
        while self.t < end {
            self.tick();
            let stopped = self.linked && self.state.last_status == protocol::MachineStatus::Stopped;
            self.t += Duration::from_millis(if stopped { 200 } else { 50 });
        }
    }

    pub(crate) fn press(&mut self, action: crate::engine::DeckAction) {
        self.state.set_test_now(self.t);
        self.state.trigger_action(action);
    }

    /// A speed set by hand (the dashboard slider, a typed value).
    pub(crate) fn set_speed(&mut self, kmh: f32) {
        self.state.set_test_now(self.t);
        crate::bluetooth::apply_manual_intent(&mut self.state, Some(kmh), None);
    }

    /// `n` speed presses on the deck's remote, `up` or down, 0.3 s apart.
    pub(crate) fn remote_speed(&mut self, up: bool, n: u32) {
        for _ in 0..n {
            self.deck.press_speed(up, self.t);
            self.run(0.3);
        }
    }

    /// `n` incline presses on the deck's remote, `up` or down, 0.3 s apart.
    pub(crate) fn remote_incline(&mut self, up: bool, n: u32) {
        for _ in 0..n {
            self.deck.press_incline(up, self.t);
            self.run(0.3);
        }
    }

    pub(crate) fn run_program(&mut self, id: u32) {
        self.state.set_test_now(self.t);
        self.state.run_program(id, self.t).unwrap();
    }

    /// GO on the HUD or dashboard (`Command::ProgramResume`).
    pub(crate) fn go(&mut self) {
        self.state.set_test_now(self.t);
        self.state.resume_program(self.t).unwrap();
    }

    /// Where `commands_since` starts counting.
    pub(crate) fn mark(&self) -> usize {
        self.sent.len()
    }

    /// Everything sent since `mark` but status polls.
    pub(crate) fn commands_since(&self, mark: usize) -> Vec<PacketKind> {
        self.sent[mark..]
            .iter()
            .map(|p| p.kind)
            .filter(|k| *k != PacketKind::PollStatus)
            .collect()
    }

    /// How many `control` packets went out since `mark`.
    pub(crate) fn controls_since(&self, mark: usize, control: Control) -> usize {
        self.sent[mark..]
            .iter()
            .filter(|p| p.bytes.len() == 15 && p.bytes[1] == 0x53 && p.bytes[2] == control as u8)
            .count()
    }

    pub(crate) fn program(&self) -> crate::programs::ActiveProgram {
        self.state.active_program().expect("a program is running")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::DeckAction;
    use crate::programs::{
        HeartZone, Program, ProgramPhase, ProgramStep, ProgramToast, ProgramWait,
    };

    const POST_MEAL: u32 = 1; // 10 min at 4.0 km/h, 10 %
    const WORKDAY: u32 = 2; // 5 min at 3.0 km/h, every 30 min

    /// Anything the app sends that would start or speed up the belt.
    fn moves_belt(sent: &[PacketKind]) -> bool {
        sent.iter().any(|k| {
            matches!(
                k,
                PacketKind::HandshakeReady | PacketKind::HandshakeStart | PacketKind::ControlSet
            )
        })
    }

    fn app_starts(sent: &[PacketKind]) -> bool {
        sent.iter()
            .any(|k| matches!(k, PacketKind::HandshakeReady | PacketKind::HandshakeStart))
    }

    /// The deck's bytes against literal ones from docs/protocol.md and the
    /// recording, so the sim can't drift along with a protocol.rs bug.
    #[test]
    fn the_sim_speaks_the_recorded_bytes() {
        let t = Instant::now();
        let ms = |n: u64| t + Duration::from_millis(n);
        let mut deck = SimDeck::new(t);
        assert!(deck.connect());
        let poll = [0x02, 0x51, 0x51, 0x03];
        let ready = [0x02, 0x53, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x52, 0x03];
        let start = [0x02, 0x53, 0x09, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x5A, 0x03];
        let stop = [0x02, 0x53, 0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x50, 0x03];
        let ask = |deck: &mut SimDeck, bytes: &[u8], at: u64| {
            deck.receive(bytes, ms(at));
            deck.take_notifications(ms(at + 100)).pop().unwrap()
        };
        assert_eq!(ask(&mut deck, &poll, 0), [0x02, 0x51, 0x00, 0x51, 0x03]);
        assert_eq!(ask(&mut deck, &ready, 200), READY_ACK);
        assert_eq!(
            ask(&mut deck, &poll, 400),
            [0x02, 0x51, 0x02, 0x03, 0x50, 0x03]
        );
        // The Start after a READY is refused: the deck is counting down.
        assert_eq!(ask(&mut deck, &start, 600), [0x02, 0x53, 0x53, 0x03]);
        assert_eq!(ask(&mut deck, &poll, 1400)[3], 2);
        assert_eq!(ask(&mut deck, &poll, 2400)[3], 1);
        assert_eq!(ask(&mut deck, &poll, 4000)[2], 3);
        assert_eq!(ask(&mut deck, &stop, 4200), [0x02, 0x53, 0x03, 0x50, 0x03]);
        deck.enter_standby(ms(20_000));
        assert_eq!(
            ask(&mut deck, &poll, 20_000),
            [0x02, 0x51, 0x06, 0x02, 0x55, 0x03]
        );
        assert_eq!(ask(&mut deck, &ready, 20_200), [0x02, 0x53, 0x53, 0x03]);
        // A recorded running frame: 1.3 km/h, 5 s, 1 m, 0.1 kcal, 5 steps.
        assert_eq!(
            status_frame(3, 0x0d, 0, 5, 1, 1, 5),
            [
                0x02, 0x51, 0x03, 0x0d, 0x00, 0x05, 0x00, 0x01, 0x00, 0x01, 0x00, 0x05, 0x00, 0x00,
                0x00, 0x5f, 0x03
            ]
        );
        assert_eq!(deck.history, vec![0, 2, 3, 4, 1, 0, 6]);
    }

    #[test]
    fn engage_counts_down_ramps_and_runs_at_the_target() {
        let mut r = Rig::new();
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(6.0);
        assert_eq!(r.deck.history, vec![0, 2, 3], "countdown, then running");
        assert!(r.deck.running_at(START_KMH));
        r.set_speed(3.0);
        let mut speeds = vec![];
        for _ in 0..8 {
            r.run(1.0);
            speeds.push(r.deck.speed_kmh());
        }
        assert!(speeds.windows(2).all(|w| w[0] <= w[1]), "{speeds:?}");
        assert!(speeds[0] < 3.0, "a ramp, not a jump: {speeds:?}");
        assert!(r.deck.running_at(3.0));
        assert_eq!(r.controls_since(0, Control::Pause), 0);
        assert_eq!(r.sessions_started, 1);
        assert!(r.state.start_requested_at.is_none() && !r.state.start_blocked);
    }

    /// The deck counts down for 3.5 s after READY, but the engine only learns
    /// it from the next poll. Whatever the poll phase, the app should send one
    /// READY and one Start per Engage.
    #[test]
    #[ignore = "engine bug: a READY sent just after a poll re-primes the start before the countdown frame arrives"]
    fn engage_sends_one_ready_and_one_start_whatever_the_poll_phase() {
        for offset_ms in (0..600).step_by(50) {
            let mut r = Rig::new();
            r.run(1.0 + offset_ms as f64 / 1000.0);
            let mark = r.mark();
            r.press(DeckAction::Start);
            r.run(6.0);
            let ready = r.controls_since(mark, Control::Ready);
            let start = r.controls_since(mark, Control::Start);
            assert_eq!(
                (ready, start),
                (1, 1),
                "offset {offset_ms} ms: {:?}",
                r.commands_since(mark)
            );
        }
    }

    /// A top speed stored past the setting's range (40 km/h, which the dashboard
    /// can't type) loads as 25 km/h, so a speed asked for by hand runs the belt
    /// at 25, never 40, on a deck that would take either.
    #[test]
    fn a_stored_top_speed_past_its_range_holds_the_belt_to_the_range() {
        let mut r = Rig::new();
        let stored = |key: &str| match key {
            "maxSpeed" => Some("40".to_string()),
            _ => None,
        };
        let config = crate::persistence::config_from_settings(stored, None);
        assert_eq!(config.max_speed, crate::persistence::MAX_SPEED_SETTING_KMH);
        r.state.sync_config(&config);
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(6.0);
        r.set_speed(40.0);
        r.run(120.0);
        assert!(r.deck.running_at(25.0), "{} km/h", r.deck.speed_kmh());
    }

    /// Fixture: Post-meal run from a still deck, walking at 4.0 km/h.
    fn post_meal_walking() -> Rig {
        let mut r = Rig::new();
        r.run(1.0);
        r.run_program(POST_MEAL);
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        r
    }

    #[test]
    fn a_program_taken_to_zero_by_hand_and_re_jacked_keeps_running() {
        let mut r = post_meal_walking();
        r.set_speed(0.0);
        r.run(12.0);
        assert_eq!(r.deck.status_code(), 10);
        assert!(r.deck.belt_still());
        assert!(r.program().paused);
        // RE-JACK: Engage on the paused deck.
        r.press(DeckAction::Start);
        r.run(15.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        r.run(30.0);
        assert!(r.deck.running_at(4.0), "still running 30 s later");
        assert_eq!(r.commands_since(mark), vec![], "nothing pauses it again");
        let a = r.program();
        assert_eq!(
            (a.phase, a.paused, a.step_index),
            (ProgramPhase::Walking, false, 0)
        );
        assert_eq!(r.sessions_started, 1);
    }

    #[test]
    fn pause_and_resume_by_the_user() {
        let mut r = Rig::new();
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(5.0);
        r.set_speed(3.0);
        r.run(20.0);
        let mark = r.mark();
        r.press(DeckAction::Pause);
        r.run(10.0);
        assert_eq!(r.deck.status_code(), 10);
        assert!(r.deck.belt_still());
        let held = r.deck.workout_s();
        r.run(20.0);
        assert_eq!(r.deck.workout_s(), held, "the deck holds its clock");
        assert_eq!(r.commands_since(mark), vec![PacketKind::AtomicAction]);
        assert_eq!(r.controls_since(mark, Control::Pause), 1);
        // Resume: Engage again, the countdown, back at the speed from before.
        r.press(DeckAction::Start);
        r.run(10.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        assert!(r.deck.workout_s() > held, "the clock runs again");
        assert_eq!(r.controls_since(mark, Control::Stop), 0);
        assert_eq!(r.sessions_started, 1, "a resume continues the session");
        assert!(r.finalised.is_empty());
    }

    #[test]
    fn a_reconnect_while_walking_carries_on_and_sends_nothing() {
        let mut r = post_meal_walking();
        r.run(10.0);
        let left = r.program().seconds_remaining.unwrap();
        r.drop_link();
        r.run(5.0);
        assert!(
            r.deck.running_at(4.0),
            "the belt carries on without the app"
        );
        assert!(r.reconnect());
        let mark = r.mark();
        r.run(20.0);
        assert_eq!(r.commands_since(mark), vec![], "only polls");
        assert!(r.deck.running_at(4.0));
        let a = r.program();
        assert_eq!(
            (a.phase, a.paused, a.wait),
            (ProgramPhase::Walking, false, None)
        );
        assert!(
            a.seconds_remaining.unwrap() <= left - 20,
            "the step timer ran on"
        );
        assert_eq!(
            (r.sessions_started, r.finalised.len()),
            (1, 0),
            "same session"
        );
    }

    #[test]
    fn a_reconnect_to_a_still_belt_sends_no_start_and_the_program_waits_for_go() {
        let mut r = post_meal_walking();
        r.drop_link();
        // Paused at the treadmill while the app wasn't connected.
        let t = r.t;
        r.deck.press_pause(t);
        r.run(10.0);
        assert!(r.reconnect());
        let mark = r.mark();
        r.run(30.0);
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        assert_eq!(r.deck.status_code(), 10);
        assert!(r.deck.belt_still());
        assert!(r.state.hold_auto_start);
        let a = r.program();
        assert_eq!((a.paused, a.wait), (true, Some(ProgramWait::BeltStill)));
        assert_eq!(
            r.toasts,
            vec![ProgramToast::WaitingForGo(ProgramWait::BeltStill)]
        );
        // GO starts it.
        r.go();
        r.run(15.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(r.program().wait, None);
    }

    #[test]
    fn a_rest_that_ends_waits_for_go_and_go_starts_the_belt() {
        let mut r = Rig::new();
        r.run(1.0);
        r.run_program(WORKDAY);
        r.run(5.0 * 60.0 + 20.0);
        assert_eq!(r.program().phase, ProgramPhase::Resting);
        assert_eq!(r.deck.status_code(), 10, "the rest pauses the belt");
        let mark = r.mark();
        // The rest is 25 minutes; then a minute more of nobody pressing GO.
        r.run(26.0 * 60.0);
        let a = r.program();
        assert_eq!(
            (a.phase, a.cycle, a.paused, a.wait),
            (
                ProgramPhase::Walking,
                2,
                true,
                Some(ProgramWait::RestComplete)
            )
        );
        assert!(r.deck.belt_still() && r.deck.status_code() == 10);
        assert!(
            !app_starts(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        assert!(r
            .toasts
            .contains(&ProgramToast::WaitingForGo(ProgramWait::RestComplete)));
        r.go();
        r.run(15.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(r.program().wait, None);
    }

    /// A 20 minute step at 4.0 km/h holding 85 to 95 % of 200 bpm (170 to 190).
    const ZONE: u32 = 9;
    fn zone_rig() -> Rig {
        let mut r = Rig::new();
        r.state.max_heart_rate = 200;
        r.state.programs.push(Program {
            id: ZONE,
            name: "Zone".into(),
            steps: vec![ProgramStep {
                duration_minutes: Some(20.0),
                speed_kmh: 4.0,
                incline_pct: 0.0,
                zone: Some(HeartZone {
                    low_pct: 85.0,
                    high_pct: 95.0,
                }),
            }],
            repeat_every_minutes: None,
            note: None,
        });
        r.run(1.0);
        r
    }

    fn controls(sent: &[PacketKind]) -> Vec<PacketKind> {
        sent.iter()
            .copied()
            .filter(|k| {
                matches!(
                    k,
                    PacketKind::HandshakeReady
                        | PacketKind::HandshakeStart
                        | PacketKind::HandshakePausePrime
                        | PacketKind::HandshakePause
                        | PacketKind::AtomicAction
                )
            })
            .collect()
    }

    /// A zone step trims a running belt's speed down above the zone and up
    /// below it, by targets alone: no start, pause or stop.
    #[test]
    fn a_heart_rate_zone_trims_a_running_belt_and_sends_only_targets() {
        let mut r = zone_rig();
        r.run_program(ZONE);
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        r.state.heart_bpm = Some(195);
        r.run(70.0);
        assert!(
            r.deck.speed_kmh() < 3.95,
            "slowed: {} km/h",
            r.deck.speed_kmh()
        );
        let slowed = r.deck.speed_kmh();
        r.state.heart_bpm = Some(150);
        r.run(70.0);
        assert!(
            r.deck.speed_kmh() > slowed + 0.05,
            "sped up: {} km/h",
            r.deck.speed_kmh()
        );
        assert_eq!(controls(&r.commands_since(mark)), vec![]);
    }

    /// The deck is paused at the treadmill just as the zone trims the speed up
    /// (before the Paused status arrives): the higher target never resumes it.
    #[test]
    fn a_zone_trim_racing_a_pause_at_the_treadmill_never_resumes_the_belt() {
        let mut r = zone_rig();
        r.run_program(ZONE);
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        let t = r.t;
        r.deck.press_pause(t);
        r.state.heart_bpm = Some(120);
        r.state.set_test_now(t);
        r.state.tick_program(t);
        assert!(
            r.state.program_intent.expect("intent").speed > 4.05,
            "the trim raised the target"
        );
        // The Paused frame lands before the engine sends the raised target.
        let frame = r.deck.current_frame();
        r.rx(&frame);
        assert_eq!(r.state.last_status, protocol::MachineStatus::Paused);
        r.run(60.0);
        assert_eq!(r.deck.status_code(), 10, "still paused");
        assert!(
            !app_starts(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
    }

    /// A zone step on a still belt (the program waits for GO) with a low heart
    /// rate leaves the belt still.
    #[test]
    fn a_zone_never_starts_a_still_belt() {
        let mut r = zone_rig();
        r.run_program(ZONE);
        r.run(20.0);
        r.press(DeckAction::Pause);
        r.run(10.0);
        assert_eq!(r.deck.status_code(), 10, "paused");
        let mark = r.mark();
        r.state.heart_bpm = Some(120);
        r.run(120.0);
        assert_eq!(r.deck.status_code(), 10, "still paused");
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
    }

    /// The seeded VO2 max against the deck. With no heart rate reading its
    /// climb runs the set 6.0 km/h. With a pulse above the zone the warm-up
    /// (no zone) is left alone and the climb is trimmed down by targets alone;
    /// paused at the treadmill mid-climb with a low pulse, the belt stays
    /// paused until a start press.
    #[test]
    fn the_seeded_vo2_max_trims_its_climbs_and_never_starts_a_still_belt() {
        let seeded = || {
            crate::programs::seed_programs()
                .into_iter()
                .find(|p| p.name == "VO2 max")
                .expect("seeded")
        };
        let rig = || {
            let mut r = Rig::new();
            r.state.programs.push(seeded());
            r.state.max_heart_rate = 190; // The zone is 161.5 to 180.5 bpm.
            r.run(1.0);
            r.run_program(seeded().id);
            r
        };

        // No reading: the climb holds its set speed.
        let mut r = rig();
        r.run(330.0);
        assert_eq!(r.program().step_index, 1);
        r.run(90.0);
        assert!(r.deck.running_at(6.0), "{} km/h", r.deck.speed_kmh());

        let mut r = rig();
        r.run(20.0);
        assert!(r.deck.running_at(4.5), "{} km/h", r.deck.speed_kmh());
        r.state.heart_bpm = Some(185);
        r.run(60.0);
        assert!(
            r.deck.running_at(4.5),
            "warm-up: {} km/h",
            r.deck.speed_kmh()
        );

        // Into the first climb (6.0 km/h at 10 %), still above the zone.
        r.run(230.0);
        assert_eq!(r.program().step_index, 1);
        let mark = r.mark();
        r.run(70.0);
        let speed = r.deck.speed_kmh();
        assert!((5.0..5.95).contains(&speed), "trimmed down: {} km/h", speed);
        assert_eq!(controls(&r.commands_since(mark)), vec![]);

        // Paused at the treadmill, a pulse below the zone never resumes it.
        let t = r.t;
        r.deck.press_pause(t);
        r.run(10.0);
        assert_eq!(r.deck.status_code(), 10, "paused");
        let mark = r.mark();
        r.state.heart_bpm = Some(120);
        r.run(120.0);
        assert_eq!(r.deck.status_code(), 10, "still paused");
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );

        // A start press (Engage or RE-JACK) moves it again.
        r.press(DeckAction::Start);
        r.run(15.0);
        assert!(r.deck.speed_kmh() > 4.0, "{} km/h", r.deck.speed_kmh());
    }

    /// The seeded Interval walk against the deck: one press runs the warm-up
    /// and all five brisk and easy rounds with the belt moving throughout, no
    /// step waits for GO, and the end leaves a belt that was still before
    /// the program still.
    #[test]
    fn the_seeded_interval_walk_runs_every_round_without_waiting_for_go() {
        let seeded = crate::programs::seed_programs()
            .into_iter()
            .find(|p| p.name == "Interval walk")
            .expect("seeded");
        let mut r = Rig::new();
        r.state.programs.push(seeded.clone());
        r.run(1.0);
        r.run_program(seeded.id);
        r.run(60.0);
        assert!(
            r.deck.running_at(4.0),
            "warm-up: {} km/h",
            r.deck.speed_kmh()
        );

        // Each step, 60 s in: the belt runs at the step's speed, no wait.
        r.run(240.0);
        for (i, step) in seeded.steps.iter().enumerate().skip(1) {
            r.run(60.0);
            let a = r.program();
            assert_eq!((a.step_index as usize, a.wait), (i, None), "step {}", i);
            assert!(
                r.deck.running_at(step.speed_kmh),
                "step {}: {} km/h",
                i,
                r.deck.speed_kmh()
            );
            r.run(120.0);
        }

        // 35 minutes in, the program is over and the belt is still again.
        r.run(30.0);
        assert!(r.state.active_program().is_none());
        let mark = r.mark();
        r.run(30.0);
        assert!(r.deck.belt_still(), "{} km/h", r.deck.speed_kmh());
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
    }

    #[test]
    fn a_start_at_the_treadmill_on_a_waiting_program_is_its_go() {
        let mut r = Rig::new();
        r.state.programs.push(Program {
            id: 5,
            name: "Hold then walk".into(),
            steps: vec![
                ProgramStep {
                    duration_minutes: Some(1.0),
                    speed_kmh: 0.0,
                    incline_pct: 0.0,
                    zone: None,
                },
                ProgramStep {
                    duration_minutes: Some(2.0),
                    speed_kmh: 3.0,
                    incline_pct: 0.0,
                    zone: None,
                },
            ],
            repeat_every_minutes: None,
            note: None,
        });
        r.run(1.0);
        r.run_program(5);
        r.run(65.0);
        let a = r.program();
        assert_eq!((a.step_index, a.wait), (1, Some(ProgramWait::HoldComplete)));
        assert!(r.deck.belt_still() && r.deck.status_code() == 0);
        assert_eq!(r.commands_since(0), vec![], "the hold sent nothing");
        let mark = r.mark();
        let t = r.t;
        r.deck.press_start(t);
        r.run(15.0);
        let sent = r.commands_since(mark);
        assert!(!app_starts(&sent), "the deck started it: {sent:?}");
        assert!(sent.contains(&PacketKind::ControlSet), "{sent:?}");
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        let a = r.program();
        assert_eq!((a.paused, a.wait), (false, None));
        assert!(r.toasts.contains(&ProgramToast::deck_go("Hold then walk")));
    }

    /// Post-meal walking at 4.0 km/h for a minute, then paused with the
    /// treadmill's own button and left for 5 s.
    fn post_meal_paused_at_the_treadmill() -> Rig {
        let mut r = Rig::new();
        r.run(1.0);
        r.run_program(POST_MEAL);
        r.run(60.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        let t = r.t;
        r.deck.press_pause(t);
        r.run(5.0);
        assert_eq!(r.deck.status_code(), 10, "paused");
        r
    }

    /// #158: a pause at the treadmill mid-step pauses the program with it.
    /// Its step timer stops, and nothing restarts the belt by itself.
    #[test]
    fn a_pause_at_the_treadmill_mid_step_pauses_the_program() {
        let mut r = post_meal_paused_at_the_treadmill();
        let a = r.program();
        assert_eq!((a.step_index, a.paused, a.wait), (0, true, None));
        let left = a.seconds_remaining;
        let mark = r.mark();
        r.run(300.0);
        assert_eq!(r.deck.status_code(), 10, "still paused");
        let sent = r.commands_since(mark);
        assert!(!moves_belt(&sent), "{sent:?}");
        let a = r.program();
        assert!(a.paused);
        assert_eq!(a.seconds_remaining, left, "the step timer held");
    }

    /// #158: GO after a pause at the treadmill resumes the same step at its
    /// speed, and its timer runs again.
    #[test]
    fn go_resumes_the_step_after_a_pause_at_the_treadmill() {
        let mut r = post_meal_paused_at_the_treadmill();
        let left = r.program().seconds_remaining.expect("timed step");
        r.go();
        r.run(15.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        let a = r.program();
        assert_eq!((a.step_index, a.paused, a.wait), (0, false, None));
        r.run(30.0);
        let now_left = r.program().seconds_remaining.expect("timed step");
        assert!(now_left < left, "the timer runs: {now_left} vs {left}");
    }

    /// #158: a start at the treadmill on a program paused there stays its GO.
    #[test]
    fn a_start_at_the_treadmill_after_its_pause_is_the_programs_go() {
        let mut r = post_meal_paused_at_the_treadmill();
        let mark = r.mark();
        let t = r.t;
        r.deck.press_start(t);
        r.run(15.0);
        let sent = r.commands_since(mark);
        assert!(!app_starts(&sent), "the deck started it: {sent:?}");
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        let a = r.program();
        assert_eq!((a.paused, a.wait), (false, None));
        assert!(r.toasts.contains(&ProgramToast::deck_go("Post-meal")));
    }

    #[test]
    fn engage_in_standby_is_refused_at_once_and_sends_no_start() {
        let mut r = Rig::new();
        let t = r.t;
        r.deck.enter_standby(t);
        r.run(2.0);
        assert!(r.state.deck_standby);
        r.press(DeckAction::Start);
        assert!(
            r.state.start_requested_at.is_none(),
            "refused before any packet"
        );
        assert!(r.state.start_blocked);
        r.run(10.0);
        assert_eq!(r.commands_since(0), vec![]);
        assert_eq!(r.deck.nacks, 0, "nothing reached the deck to refuse");
        // Woken at the deck: still nothing starts by itself.
        let t = r.t;
        r.deck.wake(t);
        r.run(10.0);
        assert_eq!(r.commands_since(0), vec![]);
        assert_eq!(r.deck.status_code(), 0);
    }

    /// The deck clamps a target to its floor and says so in the echo; the app
    /// shows "asked X, got Y" and keeps the note only for a while.
    #[test]
    fn a_target_below_the_decks_floor_is_noted_from_the_echo() {
        let mut r = Rig::new();
        r.deck.set_limits(8, 40);
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(5.0);
        r.set_speed(2.0);
        r.run(5.0);
        assert!(r.deck.running_at(2.0));
        assert_eq!(r.state.current_target_note(), None, "taken as asked");
        // Engage started at 0.6 and the deck took 0.8: DECK_PROFILE knows the
        // floor, so 0.6 now goes out as 0.8 and needs no note.
        assert_eq!(r.state.deck_profile.floor(), Some(8));
        let mark = r.mark();
        r.set_speed(0.6);
        r.run(3.0);
        assert!(r.deck.running_at(0.8), "{} km/h", r.deck.speed_kmh());
        assert_eq!(target_bytes_since(&r, mark), vec![8]);
        assert_eq!(r.state.current_target_note(), None);
        r.set_speed(5.0);
        r.run(2.0);
        let note = r.state.current_target_note().expect("a note");
        assert!(note.contains("5.0") && note.contains("4.0"), "{note}");
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        r.run(10.0);
        assert_eq!(r.state.current_target_note(), None, "the note fades");
        r.set_speed(2.0);
        r.run(3.0);
        assert_eq!(r.state.current_target_note(), None);
    }

    #[test]
    fn a_nack_just_after_a_target_is_that_target_refused() {
        let mut r = Rig::new();
        r.state.set_test_now(r.t);
        r.state.target_pending = Some((30, 0, r.t));
        r.state.on_deck_reply(protocol::DeckReply::Nack);
        let note = r.state.current_target_note().expect("a note");
        assert!(note.starts_with("DECK_REFUSED"), "{note}");
        assert!(
            note.contains("KM/H, TILT") && note.ends_with(" %"),
            "{note}"
        );
        // A nack with no target in flight says nothing.
        r.state.target_note = None;
        r.state.on_deck_reply(protocol::DeckReply::Nack);
        assert_eq!(r.state.current_target_note(), None);
    }

    /// Target bytes sent since `mark`.
    fn target_bytes_since(r: &Rig, mark: usize) -> Vec<u8> {
        r.sent[mark..]
            .iter()
            .filter(|p| {
                p.bytes.len() == 15 && p.bytes[1] == 0x53 && p.bytes[2] == Control::Target as u8
            })
            .map(|p| p.bytes[3])
            .collect()
    }

    fn walking_rig(floor: u8, ceiling: u8) -> Rig {
        let mut r = Rig::new();
        r.deck.set_limits(floor, ceiling);
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(5.0);
        r.set_speed(2.0);
        r.run(3.0);
        assert!(r.deck.running_at(2.0));
        r
    }

    /// DECK_PROFILE learns the floor and ceiling from the echoes alone: the
    /// app sends only the targets it was asked for, and afterwards the limits
    /// shown, the stepper and a typed speed keep to them.
    #[test]
    fn the_deck_profile_learns_the_floor_and_ceiling_from_echoes() {
        // Engage starts at the 0.6 setting; the deck takes 0.8 and says so.
        let mut r = walking_rig(8, 40);
        assert_eq!(r.state.deck_profile.floor(), Some(8));
        assert_eq!(r.state.deck_profile.ceiling(), None);
        let mark = r.mark();
        r.set_speed(5.0);
        r.run(3.0);
        assert_eq!(
            target_bytes_since(&r, mark),
            vec![50],
            "nothing sent to learn"
        );
        assert_eq!(r.state.deck_profile.ceiling(), Some(40));
        assert!((r.state.min_physical_speed() - 0.8).abs() < 1e-5);
        assert!((r.state.top_hw_speed() - 4.0).abs() < 1e-5);
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());

        // The deck already runs 4.0: asking 5.0 again sends nothing.
        let mark = r.mark();
        r.set_speed(4.5);
        r.run(2.0);
        assert!(target_bytes_since(&r, mark).is_empty());
        r.press(DeckAction::SpeedUp);
        r.run(2.0);
        assert!(
            target_bytes_since(&r, mark).is_empty(),
            "the stepper stops at 4.0"
        );

        let steps = r.state.deck_speeds();
        assert_eq!(steps.first().map(|s| s.0), Some(0.8));
        assert_eq!(steps.last().map(|s| s.0), Some(4.0));

        // Down to the floor and no further: a down press there sends nothing.
        r.set_speed(0.9);
        r.run(8.0);
        r.press(DeckAction::SpeedDown);
        r.run(5.0);
        assert!(r.deck.running_at(0.8), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        r.press(DeckAction::SpeedDown);
        r.run(2.0);
        assert!(target_bytes_since(&r, mark).is_empty());
        assert!(r.deck.running_at(0.8));

        // The profile outlives the connection.
        r.drop_link();
        r.run(1.0);
        assert!(r.reconnect());
        assert_eq!(r.state.deck_profile.ceiling(), Some(40));
    }

    /// A skipped in-between speed is learned from its echo, and the stepper
    /// then steps over it both ways.
    #[test]
    fn the_stepper_steps_over_a_speed_the_deck_skips() {
        let mut r = walking_rig(6, 40);
        r.deck.set_skip(15, 16);
        r.set_speed(1.4);
        r.run(3.0);
        r.press(DeckAction::SpeedUp);
        r.run(3.0);
        assert!(r.deck.running_at(1.6), "{} km/h", r.deck.speed_kmh());
        assert!(r.state.deck_profile.skips(15));
        assert!(
            r.state.current_target_note().is_some(),
            "the first time is noted"
        );
        let shown = r.state.snap_speed(1.5);
        assert!((shown - 1.6).abs() < 1e-5, "{shown}");

        let mark = r.mark();
        r.press(DeckAction::SpeedDown);
        r.run(3.0);
        r.press(DeckAction::SpeedUp);
        r.run(3.0);
        assert_eq!(
            target_bytes_since(&r, mark),
            vec![14, 16],
            "1.5 never asked again"
        );
        assert!(r.deck.running_at(1.6));
        assert!(!r
            .state
            .deck_speeds()
            .iter()
            .any(|s| (s.0 - 1.5).abs() < 1e-5));
    }

    /// After the remote moves the belt, a press steps from the deck's speed
    /// and still steps over a speed the deck skips, and keeps to the floor.
    #[test]
    fn a_press_after_the_remote_steps_from_the_deck_and_over_a_skipped_speed() {
        let mut r = walking_rig(6, 40);
        r.deck.set_skip(15, 16);
        r.set_speed(1.4);
        r.run(3.0);
        r.press(DeckAction::SpeedUp);
        r.run(3.0);
        assert!(r.deck.running_at(1.6), "{} km/h", r.deck.speed_kmh());
        assert!(r.state.deck_profile.skips(15));

        // Down to 1.4 on the remote; up on the HUD lands on the skipped 1.5,
        // so 1.6 goes out, though the app last sent 1.6 itself.
        r.remote_speed(false, 2);
        r.run(5.0);
        assert!(r.deck.running_at(1.4), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        r.press(DeckAction::SpeedUp);
        r.run(3.0);
        assert_eq!(target_bytes_since(&r, mark), vec![16], "1.5 skipped");
        assert!(r.deck.running_at(1.6), "{} km/h", r.deck.speed_kmh());

        // Up to 1.7 on the remote; down twice on the HUD steps 1.6, then
        // over 1.5 to 1.4.
        r.remote_speed(true, 1);
        r.run(5.0);
        assert!(r.deck.running_at(1.7), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        r.press(DeckAction::SpeedDown);
        r.run(3.0);
        r.press(DeckAction::SpeedDown);
        r.run(3.0);
        assert_eq!(target_bytes_since(&r, mark), vec![16, 14]);
        assert!(r.deck.running_at(1.4), "{} km/h", r.deck.speed_kmh());

        // Down to the floor on the remote; down on the HUD stays there.
        r.remote_speed(false, 8);
        r.run(5.0);
        assert!(r.deck.running_at(0.6), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        r.press(DeckAction::SpeedDown);
        r.run(3.0);
        assert!(target_bytes_since(&r, mark).iter().all(|b| *b >= 6));
        assert!(r.deck.running_at(0.6), "{} km/h", r.deck.speed_kmh());
        assert_eq!(r.controls_since(0, Control::Pause), 0);
    }

    /// DECK_PROFILE never moves a still belt: with a profile learned, a
    /// reconnect to a still deck, a down press and a 0 speed send no start.
    #[test]
    fn a_learned_profile_never_starts_a_still_belt() {
        let mut r = walking_rig(8, 40);
        assert_eq!(r.state.deck_profile.floor(), Some(8));
        r.press(DeckAction::Stop);
        r.run(15.0);
        assert!(r.deck.belt_still());
        r.drop_link();
        r.run(1.0);
        assert!(r.reconnect());
        let mark = r.mark();
        r.run(5.0);
        r.press(DeckAction::SpeedDown);
        r.run(5.0);
        r.set_speed(0.0);
        r.run(5.0);
        assert!(!app_starts(&r.commands_since(mark)));
        assert!(r.deck.belt_still());
        assert_eq!(r.state.snap_speed(0.0), 0.0);
    }

    /// The simulated deck in dev builds teaches the profile nothing.
    #[test]
    fn replies_are_not_learned_while_learning_is_off() {
        let mut r = Rig::new();
        r.state.profile_learning = false;
        r.deck.set_limits(8, 40);
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(5.0);
        r.set_speed(5.0);
        r.run(2.0);
        assert!(r.state.deck_profile.is_empty());
        assert!(r.state.current_target_note().is_some(), "still noted");
    }

    /// Incline levels are learned from the echoes like speeds: the ceiling
    /// and a skipped level, then the stepper and every send keep to them.
    #[test]
    fn the_deck_profile_learns_incline_levels_and_the_stepper_keeps_to_them() {
        let mut r = walking_rig(6, 40);
        r.deck.set_incline_ceiling(10);
        r.deck.set_incline_skip(4, 5);
        for level in [4, 15] {
            r.state.set_intent(None, Some(level), true);
            r.run(3.0);
        }
        assert!(r.state.deck_profile.skips_level(4));
        assert_eq!(r.state.deck_profile.incline_ceiling(), Some(10));
        assert_eq!(
            r.state.deck_incline_levels(),
            vec![0, 1, 2, 3, 5, 6, 7, 8, 9, 10]
        );
        assert_eq!(r.state.snap_incline_level(4), 5);
        assert_eq!(r.state.snap_incline_level(14), 10);

        // Up from 3 lands on 5, and a press at the top stays there.
        r.state.set_intent(None, Some(3), true);
        r.run(3.0);
        r.press(DeckAction::InclineUp);
        r.run(3.0);
        assert_eq!(r.state.manual_intent.map(|i| i.incline), Some(5));
        for _ in 0..8 {
            r.press(DeckAction::InclineUp);
            r.run(3.0);
        }
        assert_eq!(r.state.manual_intent.map(|i| i.incline), Some(10));
    }

    #[test]
    fn an_incline_the_deck_changed_is_noted_in_percent() {
        let mut r = Rig::new();
        r.state.set_test_now(r.t);
        r.state.target_pending = Some((30, 3, r.t));
        r.state.on_deck_reply(protocol::DeckReply::TargetEcho {
            speed: 30,
            incline: 4,
        });
        let note = r.state.current_target_note().expect("a note");
        let (a, b) = (r.state.incline_pct(3), r.state.incline_pct(4));
        assert_eq!(note, format!("DECK_ADJUSTED // TILT {a:.1} % -> {b:.1} %"));
    }

    #[test]
    fn a_stop_spins_down_to_stopped_and_sends_nothing_after() {
        let mut r = Rig::new();
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(5.0);
        r.set_speed(3.0);
        r.run(30.0);
        let mark = r.mark();
        r.press(DeckAction::Stop);
        r.run(15.0);
        assert_eq!(r.commands_since(mark), vec![PacketKind::AtomicAction]);
        assert_eq!(r.controls_since(mark, Control::Stop), 1);
        assert!(
            r.deck.history.ends_with(&[3, 4, 1, 0]),
            "{:?}",
            r.deck.history
        );
        assert!(r.deck.belt_still());
        assert_eq!(r.finalised.len(), 1, "saved once");
        assert!(
            r.finalised[0] >= 30,
            "with the deck's time: {:?}",
            r.finalised
        );
        let mark = r.mark();
        r.run(20.0);
        assert_eq!(r.commands_since(mark), vec![]);
        assert_eq!(r.sessions_started, 1);
    }

    /// The deck forgets the speed when it stops (it starts again at its own
    /// start speed), so the same speed has to be sent again after the next
    /// start on the same connection.
    #[test]
    fn a_program_run_again_after_a_stop_reaches_its_speed() {
        let mut r = post_meal_walking();
        r.press(DeckAction::Stop);
        r.run(20.0);
        assert!(r.deck.belt_still() && r.deck.status_code() == 0);
        r.run_program(POST_MEAL);
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
    }

    /// The same after a pause: a paused deck resumes at its own speed only
    /// if that is what it was told last.
    #[test]
    fn engage_after_a_stop_reaches_the_speed_from_before() {
        let mut r = Rig::new();
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(5.0);
        r.set_speed(3.0);
        r.run(20.0);
        assert!(r.deck.running_at(3.0));
        r.press(DeckAction::Stop);
        r.run(20.0);
        assert!(r.deck.belt_still());
        r.set_speed(3.0);
        r.run(20.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
    }

    /// Issue #58/#60: on a calibration whose km/h dips between points, the
    /// HUD's speed-up stuck at one byte (about 3.8 km/h). Every press now
    /// reaches the deck as one more 0.1 hardware step, up to the max speed,
    /// and speed-down walks it back the same way.
    #[test]
    fn speed_up_presses_reach_the_max_speed_on_a_dipping_calibration() {
        let mut r = Rig::new();
        r.state.sync_config(&crate::persistence::AppConfig {
            calibration_points: vec![
                protocol::CalibrationPoint {
                    speed: 0.6,
                    stride_length_cm: 50.0,
                    distance_scale: 0.6 / 1.46,
                    runs: 1,
                    stride_runs: 0,
                },
                protocol::CalibrationPoint {
                    speed: 1.5,
                    stride_length_cm: 60.0,
                    distance_scale: 1.5 / 4.84,
                    runs: 1,
                    stride_runs: 0,
                },
                protocol::CalibrationPoint {
                    speed: 2.0,
                    stride_length_cm: 65.0,
                    distance_scale: 2.0 / 2.97,
                    runs: 1,
                    stride_runs: 0,
                },
            ],
            max_speed: 5.0,
            ..Default::default()
        });
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(6.0);
        assert_eq!(r.deck.status_code(), 3, "running");
        r.set_speed(r.state.to_physical_speed(1.0));
        r.run(5.0);
        assert!(r.deck.running_at(1.0), "{} km/h", r.deck.speed_kmh());

        let tenths = |kmh: f32| (kmh * 10.0).round() as i32;
        let mut targets = vec![tenths(r.deck.target)];
        for _ in 0..45 {
            r.press(DeckAction::SpeedUp);
            r.run(0.5);
            targets.push(tenths(r.deck.target));
        }
        let climb: Vec<i32> = (10..=50).chain([50; 5]).collect();
        assert_eq!(targets, climb, "one byte a press, held at the top");
        r.run(10.0);
        assert!(r.deck.running_at(5.0), "{} km/h", r.deck.speed_kmh());

        let mut targets = vec![];
        for _ in 0..10 {
            r.press(DeckAction::SpeedDown);
            r.run(0.5);
            targets.push(tenths(r.deck.target));
        }
        assert_eq!(targets, (40..50).rev().collect::<Vec<_>>());
    }

    #[test]
    fn a_power_cut_mid_walk_comes_back_still_and_nothing_starts_it() {
        let mut r = Rig::new();
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(5.0);
        r.set_speed(3.0);
        r.run(15.0);
        let t = r.t;
        r.deck.power_off(t);
        r.drop_link();
        r.run(10.0);
        assert!(!r.reconnect(), "a deck that is off doesn't connect");
        let t = r.t;
        r.deck.power_on(t);
        assert!(r.reconnect());
        let mark = r.mark();
        r.run(20.0);
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        assert!(r.deck.belt_still() && r.deck.status_code() == 0);
        assert_eq!(r.finalised.len(), 1, "the walk was saved once");
    }

    /// Guided calibration: RUN on a still belt is his press. It starts the
    /// belt and brings the deck to the target's hardware speed through the
    /// current calibration; the reconnect hold never lets it start by itself.
    #[test]
    fn a_calibration_run_starts_the_belt_at_the_target_deck_speed() {
        let mut r = Rig::new();
        r.state.sync_config(&crate::persistence::AppConfig {
            calibration_points: vec![protocol::CalibrationPoint {
                speed: 4.0,
                stride_length_cm: 70.0,
                distance_scale: 0.8,
                runs: 1,
                stride_runs: 0,
            }],
            ..Default::default()
        });
        r.run(5.0);
        assert!(r.deck.belt_still(), "nothing starts before RUN");
        r.state.set_test_now(r.t);
        crate::calibration::begin_run(&mut r.state, 4.0).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(r.state.hardware_speed, 4.0);
        // A repeat at another speed changes the belt without stopping it.
        let mark = r.mark();
        r.state.set_test_now(r.t);
        crate::calibration::begin_run(&mut r.state, 2.5).unwrap();
        r.run(10.0);
        assert!(r.deck.running_at(2.5), "{} km/h", r.deck.speed_kmh());
        assert_eq!(r.controls_since(mark, Control::Stop), 0);
        assert_eq!(r.controls_since(mark, Control::Pause), 0);
    }

    /// A calibration run's belt paused at the treadmill's own buttons.
    fn paused_at_treadmill_during_a_run(kmh: f32) -> Rig {
        let mut r = Rig::new();
        r.run(5.0);
        r.state.set_test_now(r.t);
        crate::calibration::begin_run(&mut r.state, kmh).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(kmh), "{} km/h", r.deck.speed_kmh());
        let t = r.t;
        r.deck.press_pause(t);
        r.run(5.0);
        assert_eq!(r.state.last_status, protocol::MachineStatus::Paused);
        r
    }

    /// GROUND_TRUTH (#169): the belt paused at the treadmill stays paused
    /// with no press, and RUN at the speed it paused at resumes it, as GO
    /// does, though the target is no higher than the paused speed.
    #[test]
    fn a_calibration_run_at_the_same_speed_resumes_a_belt_paused_at_the_treadmill() {
        let mut r = paused_at_treadmill_during_a_run(3.0);
        let mark = r.mark();
        r.run(60.0);
        assert_eq!(r.deck.status_code(), 10, "no resume without the press");
        assert!(!app_starts(&r.commands_since(mark)));

        r.state.set_test_now(r.t);
        crate::calibration::begin_run(&mut r.state, 3.0).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
    }

    /// GROUND_TRUTH (#169): RUN at another speed, slower than the paused
    /// one, resumes the paused belt at the run's speed.
    #[test]
    fn a_calibration_run_at_another_speed_resumes_a_paused_belt_at_that_speed() {
        let mut r = paused_at_treadmill_during_a_run(3.0);
        r.run(10.0);
        r.state.set_test_now(r.t);
        crate::calibration::begin_run(&mut r.state, 2.0).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(2.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(r.state.hardware_speed, 2.0);
    }

    /// GROUND_TRUTH (#169): after STOP the belt stays still with no press,
    /// and RUN starts it at the run's speed.
    #[test]
    fn a_calibration_run_starts_a_stopped_belt() {
        let mut r = Rig::new();
        r.run(5.0);
        r.state.set_test_now(r.t);
        crate::calibration::begin_run(&mut r.state, 3.0).unwrap();
        r.run(20.0);
        r.press(crate::engine::DeckAction::Stop);
        r.run(15.0);
        assert!(r.deck.belt_still());
        let mark = r.mark();
        r.run(60.0);
        assert!(r.deck.belt_still(), "no start without the press");
        assert!(!app_starts(&r.commands_since(mark)));

        r.state.set_test_now(r.t);
        crate::calibration::begin_run(&mut r.state, 3.0).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
    }

    /// GROUND_TRUTH: tapping the passes of the belt mark only reads the
    /// deck. Nine taps measure the belt without one packet that could move
    /// it; the belt keeps the run's speed; after STOP a tap on the still belt
    /// is not taken and nothing starts the belt again.
    #[test]
    fn tapping_a_calibration_run_never_moves_the_belt() {
        let mut r = Rig::new();
        r.run(5.0);
        r.state.set_test_now(r.t);
        crate::calibration::begin_run(&mut r.state, 3.0).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());

        // A 3 m loop at 3.0 km/h passes every 3.6 s.
        let mut run = crate::calibration::Run::new(3.0, 300.0);
        let mark = r.mark();
        for _ in 0..9 {
            run.tap(r.t, r.state.hardware_speed, |_| 70.0);
            r.run(3.6);
            run.sample(r.state.hardware_speed);
        }
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        let v = run.view(r.t);
        assert_eq!(v.phase, crate::calibration::CalibrationPhase::Measured);
        let res = v.result.unwrap();
        assert!((res.real_kmh - 3.0).abs() < 0.05, "{}", res.real_kmh);
        assert!((res.point.distance_scale - 1.0).abs() < 0.02);

        // STOP, then a tap on the still belt: not taken, nothing moves it.
        r.press(crate::engine::DeckAction::Stop);
        r.run(10.0);
        assert!(r.deck.belt_still());
        let mut run = crate::calibration::Run::new(3.0, 300.0);
        let mark = r.mark();
        for _ in 0..5 {
            run.tap(r.t, r.state.hardware_speed, |_| 70.0);
            r.run(3.6);
        }
        assert_eq!(run.view(r.t).passes, 0);
        assert!(!moves_belt(&r.commands_since(mark)));
        assert!(r.deck.belt_still(), "a tap never starts the belt");
    }

    /// GROUND_TRUTH's footstep phase: on a measured belt, TAP STEPS and 21
    /// footsteps only read the deck. Not one packet that could move the belt;
    /// the belt keeps the timed speed; the stride is the belt over the
    /// cadence. STOP mid-steps ends the steps, keeps the belt result, and a
    /// footstep on the still belt never starts it again.
    #[test]
    fn tapping_footsteps_never_moves_the_belt() {
        let mut r = Rig::new();
        r.run(5.0);
        r.state.set_test_now(r.t);
        crate::calibration::begin_run(&mut r.state, 3.0).unwrap();
        r.run(20.0);
        let mut run = crate::calibration::Run::new(3.0, 300.0);
        for _ in 0..9 {
            run.tap(r.t, r.state.hardware_speed, |_| 70.0);
            r.run(3.6);
            run.sample(r.state.hardware_speed);
        }
        assert_eq!(
            run.view(r.t).phase,
            crate::calibration::CalibrationPhase::Measured
        );

        // A step every 0.6 s at 3.0 km/h: 100 a minute, 50 cm. The footstep
        // that settles it saves the stride by itself, in the points alone.
        run.autosave(&mut r.state).unwrap();
        let mark = r.mark();
        run.start_steps(&mut r.state).unwrap();
        assert!(run.listening(), "footsteps are taps too");
        let mut saved = None;
        for _ in 0..21 {
            saved = saved.or(run.step_tap(r.t, &mut r.state));
            r.run(0.6);
            run.sample(r.state.hardware_speed);
        }
        assert!(saved.is_some(), "saved by itself");
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        let v = run.view(r.t);
        let steps = v.steps.unwrap();
        assert_eq!(steps.phase, crate::calibration::CalibrationPhase::Measured);
        let res = steps.result.unwrap();
        assert!((res.cadence_spm.unwrap() - 100.0).abs() < 1.0);
        assert!((res.point.stride_length_cm - 50.0).abs() < 1.0);
        assert!(!run.listening());

        // UNDO and TAP AGAIN on the steps change only the points.
        let mark = r.mark();
        run.steps_undo(&mut r.state).unwrap();
        run.start_steps(&mut r.state).unwrap();
        for _ in 0..5 {
            run.step_tap(r.t, &mut r.state);
            r.run(0.6);
        }
        assert!(!moves_belt(&r.commands_since(mark)));
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());

        // STOP mid-steps, then footsteps on the still belt.
        r.press(crate::engine::DeckAction::Stop);
        r.run(10.0);
        assert!(r.deck.belt_still());
        let mark = r.mark();
        for _ in 0..5 {
            run.step_tap(r.t, &mut r.state);
            r.run(0.6);
            run.sample(r.state.hardware_speed);
        }
        assert_eq!(
            run.view(r.t).steps.unwrap().steps,
            0,
            "no step on a still belt"
        );
        assert!(!moves_belt(&r.commands_since(mark)));
        assert!(r.deck.belt_still(), "a footstep never starts the belt");
    }

    /// Tap the open GROUND_TRUTH run until it measures, as if the belt really
    /// ran at the deck's speed over `factor`: a 300 cm loop's passes come
    /// that far apart. Gives what the taps saved.
    fn tap_until_measured(
        r: &mut Rig,
        factor: f32,
    ) -> Option<Vec<crate::protocol::CalibrationPoint>> {
        let mut saved = None;
        for _ in 0..12 {
            let real_ms = r.state.hardware_speed / factor / 3.6;
            saved = saved.or(crate::calibration::tap(&mut r.state, r.t).unwrap());
            r.run(3.0 / real_ms as f64);
        }
        saved
    }

    /// The whole guided walk: "What's the fastest you'll walk?" plans the
    /// speeds and moves nothing. Each speed's RUN is the only thing that
    /// moves the belt: the taps, the saves and the plan never do, STOP stops
    /// it and nothing starts it again until his next RUN. The deck here is
    /// straight, so the midpoint check agrees and the walk ends there.
    #[test]
    fn a_guided_calibration_only_moves_the_belt_on_his_run() {
        let _globals = crate::calibration::TEST_GLOBALS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::calibration::close_all();
        crate::calibration::set_belt_for_test(Some(300.0));
        let mut r = Rig::new();
        r.run(5.0);
        let factor = |kmh: f32| 0.9 + 0.02 * kmh;

        // The question plans; the belt stays still.
        let mark = r.mark();
        crate::calibration::guide_start(&r.state, 5.0).unwrap();
        r.run(20.0);
        assert!(!moves_belt(&r.commands_since(mark)));
        assert!(
            r.deck.belt_still(),
            "picking a top speed never starts the belt"
        );
        let slow = crate::calibration::snapshot(&r.state)
            .guide
            .unwrap()
            .slow_kmh;

        // RUN on the slowest speed, his press: the belt starts there.
        r.state.set_test_now(r.t);
        crate::calibration::guide_run(&mut r.state).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(slow), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        assert!(
            tap_until_measured(&mut r, factor(slow)).is_some(),
            "saved by itself"
        );
        r.run(20.0);
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        assert!(r.deck.running_at(slow), "the belt waits at {slow} for RUN");

        // RUN on the top speed, then STOP mid-tapping: the run closes, the
        // walk waits on the same speed, and nothing restarts the belt.
        r.state.set_test_now(r.t);
        crate::calibration::guide_run(&mut r.state).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(5.0), "{} km/h", r.deck.speed_kmh());
        crate::calibration::tap(&mut r.state, r.t).unwrap();
        crate::calibration::on_stop();
        r.press(crate::engine::DeckAction::Stop);
        r.run(20.0);
        assert!(r.deck.belt_still());
        let mark = r.mark();
        r.run(30.0);
        assert!(!moves_belt(&r.commands_since(mark)));
        assert!(r.deck.belt_still(), "nothing restarts the belt after STOP");
        let snap = crate::calibration::snapshot(&r.state);
        assert_eq!(snap.run, None);
        assert_eq!(snap.guide.as_ref().unwrap().next_kmh, Some(5.0));

        // His RUN again, then the taps.
        r.state.set_test_now(r.t);
        crate::calibration::guide_run(&mut r.state).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(5.0), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        assert!(tap_until_measured(&mut r, factor(5.0)).is_some());
        r.run(20.0);
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?} at {} km/h",
            r.commands_since(mark),
            r.deck.speed_kmh()
        );
        let guide = crate::calibration::snapshot(&r.state).guide.unwrap();
        let mid = guide.next_kmh.expect("the midpoint check is next");
        assert_eq!(guide.checks.len(), 1);
        assert!(r.deck.running_at(5.0), "the belt waits at 5.0 for RUN");

        // RUN on the midpoint: the check agrees and the walk is done.
        r.state.set_test_now(r.t);
        crate::calibration::guide_run(&mut r.state).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(mid), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        assert!(tap_until_measured(&mut r, factor(mid)).is_some());
        r.run(30.0);
        assert!(!moves_belt(&r.commands_since(mark)));
        let guide = crate::calibration::snapshot(&r.state).guide.unwrap();
        assert_eq!(guide.next_kmh, None, "done: {guide:?}");
        assert_eq!(
            guide.checks[0].verdict,
            crate::calibration::CheckVerdict::Agreed
        );
        assert!(guide.remaining_error_pct.unwrap() < 2.0);
        assert_eq!(r.state.calibration_points.len(), 3);
        assert!(crate::calibration::guide_run(&mut r.state).is_err());
        crate::calibration::close_all();
    }

    /// The owner's walk on hardware (#169): the belt paused at the treadmill,
    /// RUN on the slowest speed resumes it there, then the remote takes the
    /// deck 0.6 faster before the taps. The run is measured and saved at the
    /// speed the deck ran, the walk's slowest end moves there and says so,
    /// the belt keeps that speed, and the walk carries on to the top speed
    /// and the midpoint check.
    #[test]
    fn a_guided_run_the_deck_ran_faster_moves_the_walk_to_that_speed() {
        let _globals = crate::calibration::TEST_GLOBALS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::calibration::close_all();
        crate::calibration::set_belt_for_test(Some(300.0));
        let mut r = Rig::new();
        r.run(5.0);
        crate::calibration::guide_start(&r.state, 4.0).unwrap();
        let slow = crate::calibration::snapshot(&r.state)
            .guide
            .unwrap()
            .slow_kmh;
        r.state.set_test_now(r.t);
        crate::calibration::guide_run(&mut r.state).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(slow), "{} km/h", r.deck.speed_kmh());
        let t = r.t;
        r.deck.press_pause(t);
        r.run(5.0);
        assert_eq!(r.state.last_status, protocol::MachineStatus::Paused);

        // RUN on the paused belt at the same speed resumes it.
        r.state.set_test_now(r.t);
        crate::calibration::guide_run(&mut r.state).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(slow), "{} km/h", r.deck.speed_kmh());

        // The remote takes the deck faster, then the taps.
        r.remote_speed(true, 6);
        r.run(10.0);
        let ran = ((slow + 0.6) * 10.0).round() / 10.0;
        assert!(r.deck.running_at(ran), "{} km/h", r.deck.speed_kmh());
        assert!(tap_until_measured(&mut r, 0.9).is_some(), "saved");
        r.run(20.0);
        assert!(
            r.deck.running_at(ran),
            "the belt keeps {ran}: {}",
            r.deck.speed_kmh()
        );

        let snap = crate::calibration::snapshot(&r.state);
        let result = snap.run.as_ref().unwrap().result.as_ref().unwrap();
        assert!((result.point.speed - ran).abs() < 0.05);
        let speeds: Vec<f32> = r.state.calibration_points.iter().map(|p| p.speed).collect();
        assert_eq!(speeds.len(), 1);
        assert!((speeds[0] - ran).abs() < 0.05, "{speeds:?}");
        let guide = snap.guide.unwrap();
        assert!((guide.slow_kmh - ran).abs() < 0.05, "{guide:?}");
        assert!(!guide.speeds.iter().any(|s| (s.kmh - slow).abs() < 0.05));
        assert!(guide.note.as_deref().unwrap().contains("slowest"));
        assert_eq!(guide.next_kmh, Some(4.0));

        // The walk carries on: RUN on the top speed, then the midpoint.
        r.state.set_test_now(r.t);
        crate::calibration::guide_run(&mut r.state).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        assert!(tap_until_measured(&mut r, 0.9).is_some());
        r.run(10.0);
        let guide = crate::calibration::snapshot(&r.state).guide.unwrap();
        let mid = guide.next_kmh.expect("the midpoint is next");
        assert!(mid > ran && mid < 4.0, "{mid}");
        assert_eq!(guide.note, None);
        crate::calibration::close_all();
    }

    /// RUN ALL: the press runs the slowest speed. Taps and saving never move
    /// the belt; STOP stops it, closes the open run, and nothing starts the
    /// belt again until his RUN on the next speed.
    #[test]
    fn a_calibration_sweep_only_moves_the_belt_on_his_run() {
        let _globals = crate::calibration::TEST_GLOBALS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut r = Rig::new();
        r.run(5.0);
        r.state.set_test_now(r.t);
        crate::calibration::set_belt_for_test(None);
        let refused = crate::calibration::sweep_start_over(&mut r.state, &[4.0, 3.0]).unwrap_err();
        assert!(refused.contains("belt loop"), "{refused}");
        r.run(5.0);
        assert!(r.deck.belt_still(), "no belt length, no RUN");
        crate::calibration::set_belt_for_test(Some(300.0));
        r.state.set_test_now(r.t);
        crate::calibration::sweep_start_over(&mut r.state, &[4.0, 3.0]).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());

        // Taps through the shared command path, as the TAP button and
        // Alt+Space send them: the run listens, and nothing reaches the belt.
        assert!(crate::calibration::tap_listening());
        let mark = r.mark();
        for _ in 0..4 {
            crate::calibration::tap(&mut r.state, r.t).unwrap();
            r.run(3.6);
        }
        assert!(!moves_belt(&r.commands_since(mark)));
        let snap = crate::calibration::snapshot(&r.state);
        assert_eq!(snap.run.as_ref().map(|v| v.passes), Some(4));
        assert_eq!(snap.belt_cm, Some(300.0));

        // SAVE: the belt keeps 3.0 until RUN.
        crate::calibration::sweep_next(true).unwrap();
        r.run(10.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());

        // RUN on 4.0, then STOP mid-settle: the run closes, the sweep waits.
        r.state.set_test_now(r.t);
        crate::calibration::sweep_run(&mut r.state).unwrap();
        r.run(10.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        crate::calibration::on_stop();
        r.press(crate::engine::DeckAction::Stop);
        r.run(10.0);
        assert!(r.deck.belt_still());
        let snap = crate::calibration::snapshot(&r.state);
        assert_eq!(snap.run, None);
        assert_eq!(snap.sweep.as_ref().and_then(|s| s.current), Some(1));

        let mark = r.mark();
        r.run(20.0);
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        assert!(r.deck.belt_still(), "nothing restarts the belt after STOP");

        r.state.set_test_now(r.t);
        crate::calibration::sweep_run(&mut r.state).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        crate::calibration::sweep_next(false).unwrap();
        let snap = crate::calibration::snapshot(&r.state);
        assert_eq!(snap.sweep.unwrap().current, None, "both speeds done");
        crate::calibration::close_all();

        // RUN ALL from the dashboard walks the stored list (the defaults
        // here), and its press runs the slowest.
        r.state.set_test_now(r.t);
        crate::calibration::sweep_start(&mut r.state).unwrap();
        r.run(20.0);
        assert!(r.deck.running_at(2.0), "{} km/h", r.deck.speed_kmh());
        let snap = crate::calibration::snapshot(&r.state);
        assert_eq!(
            snap.sweep.unwrap().targets_kmh,
            crate::calibration::targets()
        );
        crate::calibration::close_all();
    }

    /// DECK_SWEEP: on a moving belt it asks every unlearned speed in turn,
    /// learns the deck's floor, ceiling and skipped speeds from the replies,
    /// sends no start, pause or stop, and puts the belt back at its speed.
    #[test]
    fn a_deck_sweep_learns_every_speed_and_returns_the_belt() {
        let mut r = walking_rig(8, 30);
        r.deck.set_skip(15, 16);
        r.state.max_speed = 3.5;
        r.state.profile_sweep_enabled = true;
        r.run(3.0);
        let mark = r.mark();
        r.state.profile_sweep_start().unwrap();
        r.run(120.0);
        assert!(r.state.profile_sweep.is_none(), "the sweep finished");
        assert_eq!(r.state.deck_profile.floor(), Some(8));
        assert_eq!(r.state.deck_profile.ceiling(), Some(30));
        assert!(r.state.deck_profile.skips(15));
        assert!(r.state.deck_profile.knows(22));
        assert_eq!(r.controls_since(mark, Control::Stop), 0);
        assert_eq!(r.controls_since(mark, Control::Pause), 0);
        assert_eq!(r.controls_since(mark, Control::Start), 0);
        assert!(r.deck.running_at(2.0), "{} km/h", r.deck.speed_kmh());
    }

    /// DECK_SWEEP only runs on a moving belt, and anything else that takes the
    /// belt (STOP, a speed set by hand) ends it without starting anything.
    #[test]
    fn a_deck_sweep_needs_a_moving_belt_and_ends_when_the_belt_is_taken() {
        let mut r = Rig::new();
        r.run(5.0);
        assert!(r.state.profile_sweep_start().is_err(), "a still belt");
        assert!(r.deck.belt_still());

        let mut r = walking_rig(0, u8::MAX);
        r.state.profile_sweep_enabled = true;
        r.state.profile_sweep_start().unwrap();
        r.run(6.0);
        assert!(r.state.profile_sweep.is_some());
        r.set_speed(3.0);
        r.run(5.0);
        assert!(r.state.profile_sweep.is_none(), "a hand speed ends it");
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());

        r.state.profile_sweep_start().unwrap();
        r.run(4.0);
        r.press(DeckAction::Stop);
        r.run(10.0);
        assert!(r.state.profile_sweep.is_none());
        assert!(r.deck.belt_still(), "nothing restarts the belt");
    }

    /// A walking rig with the DECK_SWEEP switch on and a sweep running.
    fn sweeping_rig() -> Rig {
        let mut r = walking_rig(0, u8::MAX);
        r.state.max_speed = 6.0;
        r.state.profile_sweep_enabled = true;
        r.state.profile_sweep_start().unwrap();
        r.run(9.0);
        assert!(r.state.profile_sweep.is_some());
        r
    }

    /// A pause at the treadmill mid-sweep: the next sweep step lands before
    /// the deck's paused status arrives, and still nothing resumes the belt.
    /// The speed goes back to the one from before the sweep, and GO resumes
    /// the belt there.
    #[test]
    fn a_pause_at_the_treadmill_mid_sweep_is_never_resumed_by_the_sweep() {
        let mut r = sweeping_rig();
        let mark = r.mark();
        r.deck.press_pause(r.t);
        // Force the next step at once, while the engine still reads Running.
        assert!(r.state.last_status.is_moving());
        let before = r.state.manual_intent.expect("an intent").speed;
        r.state.profile_sweep_step_due_for_test();
        r.state.tick_profile_sweep(r.t);
        let stepped = r.state.manual_intent.expect("an intent").speed;
        assert!(
            stepped > before,
            "the sweep stepped up: {before} -> {stepped}"
        );
        // Then the paused status arrives before the engine sends that step.
        let frame = r.deck.current_frame();
        r.rx(&frame);
        assert_eq!(r.state.last_status, protocol::MachineStatus::Paused);
        // The engine runs before the next once-a-second sweep tick.
        r.last_second = r.t;
        r.run(30.0);
        assert!(r.state.profile_sweep.is_none(), "the pause ends the sweep");
        assert!(!r.deck.running_at(0.1), "{} km/h", r.deck.speed_kmh());
        assert!(!app_starts(&r.commands_since(mark)), "no start or resume");
        assert_eq!(r.controls_since(mark, Control::Start), 0);
        assert!(
            r.state.hold_auto_start,
            "automatic starts held until a press"
        );
        let back = r.state.manual_intent.expect("an intent").speed;
        assert!((back - 2.0).abs() < 0.06, "back at the start speed: {back}");

        r.press(DeckAction::Start);
        r.run(10.0);
        assert!(
            r.deck.running_at(2.0),
            "GO resumes at {} km/h",
            r.deck.speed_kmh()
        );
    }

    /// A speed press on the deck's own remote takes the belt: the sweep ends
    /// and the remote's speed stands.
    #[test]
    fn a_remote_speed_press_ends_the_deck_sweep() {
        let mut r = sweeping_rig();
        r.remote_speed(true, 2);
        let deck_speed = r.deck.speed_kmh();
        r.run(5.0);
        assert!(r.state.profile_sweep.is_none(), "the remote ends it");
        r.run(10.0);
        assert!(
            (r.deck.speed_kmh() - deck_speed).abs() < 0.15,
            "{} km/h after {deck_speed}",
            r.deck.speed_kmh()
        );
    }

    /// DECK_SWEEP stays off until its Labs switch is on.
    #[test]
    fn a_deck_sweep_is_refused_while_its_switch_is_off() {
        let mut r = walking_rig(0, u8::MAX);
        let mark = r.mark();
        let err = r.state.profile_sweep_start().unwrap_err();
        assert!(err.contains("DECK_SWEEP"), "{err}");
        r.run(10.0);
        assert!(r.state.profile_sweep.is_none());
        assert!(r
            .commands_since(mark)
            .iter()
            .all(|k| *k != PacketKind::ControlSet));
        assert!(r.deck.running_at(2.0));
    }

    /// DECK_SWEEP over the incline levels: on a moving belt it asks every
    /// unlearned level in turn, learns the deck's top level and a skipped
    /// level from the replies, leaves the speed alone, sends no start, pause
    /// or stop, and puts the incline back where it was.
    #[test]
    fn an_incline_sweep_learns_every_level_and_returns_the_incline() {
        let mut r = walking_rig(0, u8::MAX);
        r.deck.set_incline_ceiling(10);
        r.deck.set_incline_skip(4, 5);
        r.state.max_incline = 15;
        r.press(DeckAction::InclineUp);
        r.press(DeckAction::InclineUp);
        r.run(5.0);
        assert_eq!(r.deck.incline, 2);
        r.state.profile_sweep_enabled = true;
        let mark = r.mark();
        r.state.profile_sweep_incline_start().unwrap();
        assert_eq!(
            r.state.profile_sweep_kind(),
            Some(crate::profile_sweep::SweepKind::Incline)
        );
        let view = r.state.profile_sweep_view().expect("a view");
        assert_eq!(view.level, Some(1), "lowest first");
        r.run(20.0);
        assert!(r.state.profile_sweep.is_some());
        assert!(r.deck.running_at(2.0), "the speed stays");
        r.run(100.0);
        assert!(r.state.profile_sweep.is_none(), "the sweep finished");
        let p = &r.state.deck_profile;
        assert_eq!(p.incline_ceiling(), Some(10));
        assert!(p.skips_level(4));
        assert!(p.knows_level(1) && p.knows_level(9));
        assert!(
            !p.knows_level(14),
            "nothing asked above the top level once seen"
        );
        assert_eq!(r.controls_since(mark, Control::Stop), 0);
        assert_eq!(r.controls_since(mark, Control::Pause), 0);
        assert_eq!(r.controls_since(mark, Control::Start), 0);
        assert_eq!(r.deck.incline, 2, "back at the start incline");
        assert!(r.deck.running_at(2.0), "{} km/h", r.deck.speed_kmh());
        let err = r.state.profile_sweep_incline_start().unwrap_err();
        assert!(err.contains("already learned"), "{err}");
    }

    /// The incline sweep never goes above the Highest incline level setting.
    #[test]
    fn an_incline_sweep_stops_at_the_highest_incline_level_setting() {
        let mut r = walking_rig(0, u8::MAX);
        r.state.max_incline = 5;
        r.state.profile_sweep_enabled = true;
        r.state.profile_sweep_incline_start().unwrap();
        assert_eq!(r.state.profile_sweep_view().map(|v| v.total), Some(5));
        let mut highest = 0;
        for _ in 0..60 {
            r.run(1.0);
            highest = highest.max(r.deck.incline);
        }
        assert!(r.state.profile_sweep.is_none());
        assert_eq!(highest, 5);
        assert!(!r.state.deck_profile.knows_level(6));
    }

    /// The incline sweep needs the switch and a moving belt, one sweep runs at
    /// a time, and an incline set by hand, a speed set by hand or STOP on the
    /// sweep ends it. STOP on the sweep puts the incline back.
    #[test]
    fn an_incline_sweep_needs_a_moving_belt_and_ends_when_the_belt_is_taken() {
        let mut r = Rig::new();
        r.state.profile_sweep_enabled = true;
        r.run(5.0);
        assert!(
            r.state.profile_sweep_incline_start().is_err(),
            "a still belt"
        );
        assert!(r.deck.belt_still());

        let mut r = walking_rig(0, u8::MAX);
        r.state.max_incline = 15;
        let err = r.state.profile_sweep_incline_start().unwrap_err();
        assert!(err.contains("DECK_SWEEP"), "{err}");
        r.state.profile_sweep_enabled = true;
        r.state.profile_sweep_incline_start().unwrap();
        assert!(
            r.state.profile_sweep_start().is_err(),
            "one sweep at a time"
        );
        r.run(9.0);
        assert!(r.state.profile_sweep.is_some());
        r.press(DeckAction::InclineDown);
        r.run(3.0);
        assert!(r.state.profile_sweep.is_none(), "an incline press ends it");
        let pressed = r.deck.incline;
        r.run(10.0);
        assert_eq!(r.deck.incline, pressed, "the press stands");

        r.state.profile_sweep_incline_start().unwrap();
        r.run(9.0);
        r.set_speed(2.5);
        r.run(3.0);
        assert!(r.state.profile_sweep.is_none(), "a hand speed ends it");
        assert!(r.deck.running_at(2.5), "{} km/h", r.deck.speed_kmh());

        let start = r.deck.incline;
        r.state.profile_sweep_incline_start().unwrap();
        r.run(9.0);
        assert_ne!(r.deck.incline, start, "the sweep moved the incline");
        let mark = r.mark();
        r.state.profile_sweep_stop();
        r.run(5.0);
        assert_eq!(r.deck.incline, start, "STOP on the sweep puts it back");
        assert!(r.deck.running_at(2.5), "the belt keeps going");
        assert!(!app_starts(&r.commands_since(mark)));
    }

    /// A walking rig with an incline sweep running, from flat.
    fn incline_sweeping_rig() -> Rig {
        let mut r = walking_rig(0, u8::MAX);
        r.state.max_incline = 15;
        r.state.profile_sweep_enabled = true;
        r.state.profile_sweep_incline_start().unwrap();
        r.run(9.0);
        assert!(r.state.profile_sweep.is_some());
        assert!(r.deck.incline > 0);
        r
    }

    /// A pause at the treadmill mid incline sweep: the next sweep step lands
    /// before the deck's paused status arrives, and still nothing resumes the
    /// belt. The incline goes back to the one from before the sweep, and GO
    /// resumes the belt there.
    #[test]
    fn a_pause_at_the_treadmill_mid_incline_sweep_is_never_resumed() {
        let mut r = incline_sweeping_rig();
        let mark = r.mark();
        r.deck.press_pause(r.t);
        // Force the next step at once, while the engine still reads Running.
        assert!(r.state.last_status.is_moving());
        let before = r.state.manual_intent.expect("an intent").incline;
        r.state.profile_sweep_step_due_for_test();
        r.state.tick_profile_sweep(r.t);
        let stepped = r.state.manual_intent.expect("an intent").incline;
        assert!(
            stepped > before,
            "the sweep stepped up: {before} -> {stepped}"
        );
        // Then the paused status arrives before the engine sends that step.
        let frame = r.deck.current_frame();
        r.rx(&frame);
        assert_eq!(r.state.last_status, protocol::MachineStatus::Paused);
        r.last_second = r.t;
        r.run(30.0);
        assert!(r.state.profile_sweep.is_none(), "the pause ends the sweep");
        assert!(!r.deck.running_at(0.1), "{} km/h", r.deck.speed_kmh());
        assert!(!app_starts(&r.commands_since(mark)), "no start or resume");
        assert_eq!(r.controls_since(mark, Control::Start), 0);
        assert!(
            r.state.hold_auto_start,
            "automatic starts held until a press"
        );
        let back = r.state.manual_intent.expect("an intent");
        assert_eq!(back.incline, 0, "back at the start incline");
        assert!((back.speed - 2.0).abs() < 0.06, "and speed: {}", back.speed);

        r.press(DeckAction::Start);
        r.run(10.0);
        assert!(
            r.deck.running_at(2.0),
            "GO resumes at {} km/h",
            r.deck.speed_kmh()
        );
        assert_eq!(r.deck.incline, 0, "GO resumes flat");
    }

    /// The belt stopped at the treadmill mid incline sweep: nothing restarts
    /// it, and the incline waits at the start one.
    #[test]
    fn a_stop_at_the_treadmill_mid_incline_sweep_restores_the_incline() {
        let mut r = incline_sweeping_rig();
        let mark = r.mark();
        r.deck.press_stop(r.t);
        r.run(30.0);
        assert!(r.state.profile_sweep.is_none());
        assert!(r.deck.belt_still());
        assert!(!app_starts(&r.commands_since(mark)), "nothing restarts it");
        assert!(r.state.hold_auto_start);
        if let Some(i) = r.state.manual_intent {
            assert_eq!(i.incline, 0, "the incline waits at the start one");
        }
    }

    /// An incline press on the deck's own remote takes the belt: the incline
    /// sweep ends and the remote's incline stands.
    #[test]
    fn a_remote_incline_press_ends_the_incline_sweep() {
        let mut r = incline_sweeping_rig();
        r.remote_incline(true, 2);
        let deck_incline = r.deck.incline;
        r.run(5.0);
        assert!(r.state.profile_sweep.is_none(), "the remote ends it");
        r.run(10.0);
        assert_eq!(r.deck.incline, deck_incline);
    }

    /// Stop sweep on a speed sweep puts the belt back at the speed it had
    /// before the sweep, and starts or stops nothing.
    #[test]
    fn stop_sweep_on_a_speed_sweep_returns_to_the_start_speed() {
        let mut r = sweeping_rig();
        assert!(
            !r.deck.running_at(2.0),
            "the sweep moved the belt: {} km/h",
            r.deck.speed_kmh()
        );
        let mark = r.mark();
        r.state.profile_sweep_stop();
        assert!(r.state.profile_sweep.is_none());
        r.run(8.0);
        assert!(
            r.deck.running_at(2.0),
            "back at the start speed: {} km/h",
            r.deck.speed_kmh()
        );
        assert_eq!(r.controls_since(mark, Control::Stop), 0);
        assert_eq!(r.controls_since(mark, Control::Pause), 0);
        assert!(!app_starts(&r.commands_since(mark)));
    }

    /// Stop sweep pressed just after a pause at the treadmill, before the
    /// sweep's tick saw it: the start speed is put back without resuming
    /// the belt, and automatic starts wait for a press.
    #[test]
    fn stop_sweep_on_a_paused_belt_never_resumes_it() {
        let mut r = sweeping_rig();
        let mark = r.mark();
        r.deck.press_pause(r.t);
        let frame = r.deck.current_frame();
        r.rx(&frame);
        assert_eq!(r.state.last_status, protocol::MachineStatus::Paused);
        r.state.profile_sweep_stop();
        r.run(30.0);
        assert!(!r.deck.running_at(0.1), "{} km/h", r.deck.speed_kmh());
        assert!(!app_starts(&r.commands_since(mark)), "no start or resume");
        assert!(r.state.hold_auto_start);
        let back = r.state.manual_intent.expect("an intent").speed;
        assert!((back - 2.0).abs() < 0.06, "back at the start speed: {back}");
    }

    /// An incline press on the remote ends a speed sweep too, so the sweep's
    /// next speed never puts the incline back to the app's.
    #[test]
    fn a_remote_incline_press_ends_the_speed_sweep() {
        let mut r = sweeping_rig();
        r.remote_incline(true, 3);
        r.run(5.0);
        assert!(r.state.profile_sweep.is_none(), "the remote ends it");
        r.run(10.0);
        assert_eq!(r.deck.incline, 3, "the remote's incline stands");
    }

    #[test]
    fn a_calibration_run_is_refused_while_a_program_runs() {
        let mut r = post_meal_walking();
        r.state.set_test_now(r.t);
        assert!(crate::calibration::begin_run(&mut r.state, 3.0).is_err());
        r.run(5.0);
        assert!(r.deck.running_at(4.0), "the program keeps its speed");
    }

    /// Issue #61: `min_speed` is the deck's floor in hardware units. With a
    /// calibration at scale 0.5 there, 0.6 hardware km/h is 1.2 km/h on the
    /// belt; a speed set under that ends at the floor byte (0.6 on the deck),
    /// never below it.
    #[test]
    fn a_speed_set_under_the_calibrated_floor_runs_at_the_floor_byte() {
        let mut r = Rig::new();
        r.state.sync_config(&crate::engine::slow_floor_config());
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(6.0);
        assert!(r.deck.running_at(START_KMH), "{} km/h", r.deck.speed_kmh());
        r.set_speed(3.0);
        r.run(15.0);
        // 3.0 km/h: the nearest deck byte is 1.7 (3.03 km/h at scale 0.56).
        assert!(r.deck.running_at(1.7), "{} km/h", r.deck.speed_kmh());
        // 0.8 km/h is above the raw 0.6 but under the belt's 1.2 km/h floor.
        r.set_speed(0.8);
        let mut lowest = f32::MAX;
        for _ in 0..20 {
            r.run(1.0);
            lowest = lowest.min(r.deck.speed_kmh());
        }
        assert!(r.deck.running_at(0.6), "{} km/h", r.deck.speed_kmh());
        assert!(lowest >= 0.6 - 0.05, "dipped to {lowest} km/h");
        assert_eq!(r.controls_since(0, Control::Pause), 0);
    }

    /// Lights out (`snapshot/lights_out.rs`) as `desk.rs` runs it: once a
    /// second, with the display `off` or on, for `secs` seconds. Returns the
    /// effects it applied.
    fn lights_out_for(
        r: &mut Rig,
        m: &mut crate::snapshot::lights_out::LightsOut,
        off: bool,
        secs: u32,
    ) -> Vec<crate::snapshot::lights_out::Effect> {
        use crate::snapshot::lights_out;
        let mut applied = vec![];
        for _ in 0..secs {
            r.run(1.0);
            let input = lights_out::Input {
                connected: r.linked && !r.state.awaiting_first_packet,
                display_off: off,
                belt: lights_out::belt(&r.state),
            };
            if let Some(effect) = m.tick(true, &input) {
                r.state.set_test_now(r.t);
                lights_out::apply(&mut r.state, effect);
                applied.push(effect);
            }
        }
        applied
    }

    /// Display off: one Stop, the deck winds down, the session is saved once
    /// and the deck goes to its own sleep. Display back on: nothing is sent,
    /// the belt stays still and the hold stands.
    fn lights_out_stops_and_sleeps(r: &mut Rig) {
        use crate::snapshot::lights_out::{Effect, LightsOut};
        let mut m = LightsOut::default();
        assert!(lights_out_for(r, &mut m, false, 5).is_empty());
        let mark = r.mark();
        assert_eq!(lights_out_for(r, &mut m, true, 20), vec![Effect::Stop]);
        assert_eq!(r.commands_since(mark), vec![PacketKind::AtomicAction]);
        assert_eq!(r.controls_since(mark, Control::Stop), 1);
        assert!(
            r.deck.history.ends_with(&[3, 4, 1, 0]),
            "{:?}",
            r.deck.history
        );
        assert_eq!(r.finalised.len(), 1, "the walk was saved once");
        assert!(r.state.hold_auto_start);

        assert!(lights_out_for(r, &mut m, true, 15 * 60).is_empty());
        assert_eq!(r.deck.status_code(), 6, "standby, screen off");

        let mark = r.mark();
        assert_eq!(lights_out_for(r, &mut m, false, 1), vec![Effect::Back]);
        assert!(lights_out_for(r, &mut m, false, 5 * 60).is_empty());
        assert_eq!(r.commands_since(mark), vec![]);
        assert!(r.deck.belt_still());
        assert!(r.state.hold_auto_start);
        assert_eq!(r.sessions_started, 1);
        assert_eq!(r.finalised.len(), 1);
    }

    #[test]
    fn lights_out_stops_a_walk_and_the_display_waking_starts_nothing() {
        let mut r = Rig::new();
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(5.0);
        r.set_speed(3.0);
        r.run(30.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        lights_out_stops_and_sleeps(&mut r);
    }

    #[test]
    fn lights_out_ends_a_walking_program() {
        let mut r = post_meal_walking();
        lights_out_stops_and_sleeps(&mut r);
        assert!(r.state.active_program().is_none(), "the program ended");
    }

    #[test]
    fn lights_out_ends_a_resting_program() {
        let mut r = Rig::new();
        r.run(1.0);
        r.run_program(WORKDAY);
        r.run(5.0 * 60.0 + 20.0);
        assert_eq!(r.program().phase, ProgramPhase::Resting);
        assert_eq!(r.deck.status_code(), 10, "the rest pauses the belt");
        let mark = r.mark();
        let mut m = crate::snapshot::lights_out::LightsOut::default();
        assert_eq!(
            lights_out_for(&mut r, &mut m, true, 20),
            vec![crate::snapshot::lights_out::Effect::Stop]
        );
        assert_eq!(r.controls_since(mark, Control::Stop), 1);
        assert!(r.state.active_program().is_none(), "the program ended");
        assert_eq!(r.finalised.len(), 1, "saved once");
        let mark = r.mark();
        assert!(lights_out_for(&mut r, &mut m, false, 30 * 60).len() == 1);
        assert!(!moves_belt(&r.commands_since(mark)));
        assert!(r.deck.belt_still());
    }

    #[test]
    fn lights_out_sends_one_stop_to_a_still_awake_deck() {
        use crate::snapshot::lights_out::{Effect, LightsOut};
        let mut r = Rig::new();
        r.run(5.0);
        assert_eq!(r.deck.status_code(), 0);
        let mark = r.mark();
        let mut m = LightsOut::default();
        assert_eq!(lights_out_for(&mut r, &mut m, true, 60), vec![Effect::Stop]);
        assert_eq!(r.controls_since(mark, Control::Stop), 1, "sent once");
        assert!(r.deck.belt_still());
        assert!(!r.state.stop_requested, "nothing left waiting");
        let mark = r.mark();
        assert_eq!(
            lights_out_for(&mut r, &mut m, false, 60),
            vec![Effect::Back]
        );
        assert_eq!(r.commands_since(mark), vec![]);
    }

    // -- Eye breaks ----------------------------------------------------------

    /// The eye-break machine on a fake epoch clock that follows the rig's
    /// time, ticked once a second as `desk.rs` does.
    struct Eyes {
        rt: crate::snapshot::eyebreak::Runtime,
        t0: Instant,
        /// The treadmill is linked (`desk.rs`'s `ready`).
        linked: bool,
    }

    impl Eyes {
        /// Breaks every 10 minutes of screen use, 20 seconds long, pausing the belt.
        fn new(r: &Rig) -> Self {
            let rt = crate::snapshot::eyebreak::Runtime {
                cfg: crate::snapshot::eyebreak::Config {
                    enabled: true,
                    minutes: 10,
                    seconds: 20,
                    sound: true,
                    pause_belt: true,
                    fit_rests: true,
                },
                ..Default::default()
            };
            Self {
                rt,
                t0: r.t,
                linked: true,
            }
        }

        /// Run the rig `secs` seconds with the user at the keyboard, ticking the
        /// machine each second.
        fn run(&mut self, r: &mut Rig, secs: u32) {
            for _ in 0..secs {
                r.run(1.0);
                let now = 1_800_000_000_000 + r.t.duration_since(self.t0).as_millis() as u64;
                crate::snapshot::desk::eye_break_tick(
                    &mut r.state,
                    &mut self.rt,
                    self.linked,
                    now,
                    Some(0),
                    false,
                    false,
                );
            }
        }

        fn shown(&self) -> crate::snapshot::eyebreak::EyeBreakPhase {
            self.rt.machine.shown_phase()
        }

        /// RESUME on the card.
        fn resume(&mut self, r: &mut Rig) {
            r.state.set_test_now(r.t);
            crate::snapshot::desk::eye_break_resume_on(&mut r.state, true, &mut self.rt.machine)
                .unwrap();
        }
    }

    #[test]
    fn an_eye_break_pauses_a_walk_and_only_resume_starts_it_again() {
        use crate::snapshot::eyebreak::EyeBreakPhase;
        let mut r = Rig::new();
        let mut eyes = Eyes::new(&r);
        // Nine and a half minutes at the screen, then a walk at 3 km/h.
        eyes.run(&mut r, 570);
        r.press(DeckAction::Start);
        eyes.run(&mut r, 5);
        r.set_speed(3.0);
        eyes.run(&mut r, 20);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(eyes.shown(), EyeBreakPhase::Upcoming);
        let mark = r.mark();
        // The break starts at ten minutes and pauses the belt.
        eyes.run(&mut r, 15);
        assert_eq!(eyes.shown(), EyeBreakPhase::Break);
        assert!(r.deck.belt_still() && r.deck.status_code() == 10);
        assert_eq!(r.controls_since(mark, Control::Pause), 1);
        assert_eq!(r.controls_since(mark, Control::Stop), 0);
        // It ends: the resume card, and the belt stays still for minutes.
        eyes.run(&mut r, 20);
        assert_eq!(eyes.shown(), EyeBreakPhase::Complete);
        eyes.run(&mut r, 180);
        assert_eq!(eyes.shown(), EyeBreakPhase::Complete);
        assert!(r.deck.belt_still() && r.deck.status_code() == 10);
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        // RESUME is the press: the countdown, back at 3 km/h, same session.
        eyes.resume(&mut r);
        eyes.run(&mut r, 10);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(eyes.shown(), EyeBreakPhase::Idle);
        assert_eq!(r.sessions_started, 1);
        assert!(r.finalised.is_empty());
    }

    #[test]
    fn an_eye_break_pauses_a_program_at_its_step_and_resume_is_its_go() {
        use crate::snapshot::eyebreak::EyeBreakPhase;
        let mut r = Rig::new();
        let mut eyes = Eyes::new(&r);
        eyes.run(&mut r, 570);
        r.run_program(POST_MEAL);
        eyes.run(&mut r, 25);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        let mark = r.mark();
        eyes.run(&mut r, 15);
        assert_eq!(eyes.shown(), EyeBreakPhase::Break);
        let a = r.program();
        assert_eq!(
            (a.phase, a.paused, a.wait, a.step_index),
            (ProgramPhase::Walking, true, None, 0)
        );
        assert!(r.deck.belt_still());
        // The break ends and nobody presses anything for three minutes.
        eyes.run(&mut r, 200);
        assert_eq!(eyes.shown(), EyeBreakPhase::Complete);
        assert!(r.program().paused, "the program is still paused");
        assert!(r.deck.belt_still());
        assert!(
            !app_starts(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        assert_eq!(r.controls_since(mark, Control::Stop), 0);
        eyes.resume(&mut r);
        eyes.run(&mut r, 15);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        let a = r.program();
        assert_eq!((a.paused, a.step_index), (false, 0));
        assert_eq!(eyes.shown(), EyeBreakPhase::Idle);
        assert_eq!(r.sessions_started, 1);
    }

    #[test]
    fn go_during_an_eye_break_takes_the_belt_back_and_no_card_follows() {
        use crate::snapshot::eyebreak::EyeBreakPhase;
        let mut r = Rig::new();
        let mut eyes = Eyes::new(&r);
        eyes.run(&mut r, 570);
        r.run_program(POST_MEAL);
        eyes.run(&mut r, 40);
        assert_eq!(eyes.shown(), EyeBreakPhase::Break);
        assert!(r.program().paused);
        // GO on the HUD in the middle of the break.
        r.go();
        eyes.run(&mut r, 15);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        assert!(!eyes.rt.machine.holds_belt());
        let mark = r.mark();
        eyes.run(&mut r, 30);
        assert_eq!(eyes.shown(), EyeBreakPhase::Idle, "no resume card");
        assert_eq!(
            r.controls_since(mark, Control::Pause),
            0,
            "nothing pauses it again"
        );
        assert!(r.deck.running_at(4.0));
    }

    #[test]
    fn an_eye_break_due_mid_walk_waits_for_the_program_rest_and_never_pauses_the_walk() {
        use crate::snapshot::eyebreak::EyeBreakPhase;
        let mut r = Rig::new();
        let mut eyes = Eyes::new(&r);
        // Seven minutes at the screen, then Workday: 5 min at 3 km/h, rest 25.
        eyes.run(&mut r, 420);
        r.run_program(WORKDAY);
        // Past the break's time (10 min of screen use), still walking.
        eyes.run(&mut r, 200);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        assert!(!r.program().paused);
        assert_ne!(eyes.shown(), EyeBreakPhase::Break);
        assert!(eyes.rt.machine.phase() == EyeBreakPhase::Idle);
        let mark = r.mark();
        // The heads-up, then the rest: the break starts there.
        eyes.run(&mut r, 50);
        assert_eq!(eyes.shown(), EyeBreakPhase::Upcoming);
        assert!(crate::snapshot::eyebreak::snapshot_of(&eyes.rt).at_rest);
        assert!(r.deck.running_at(3.0));
        eyes.run(&mut r, 70);
        assert_eq!(r.program().phase, ProgramPhase::Resting);
        assert_eq!(eyes.shown(), EyeBreakPhase::Break);
        assert!(!eyes.rt.machine.holds_belt());
        assert!(r.deck.belt_still());
        // Only the rest's own pauses: the same as Workday with no eye breaks.
        let mut plain = Rig::new();
        let mut off = Eyes::new(&plain);
        off.rt.cfg.enabled = false;
        off.run(&mut plain, 420);
        plain.run_program(WORKDAY);
        off.run(&mut plain, 200);
        let plain_mark = plain.mark();
        off.run(&mut plain, 120);
        assert_eq!(
            r.controls_since(mark, Control::Pause),
            plain.controls_since(plain_mark, Control::Pause)
        );
        // No resume card after the break.
        eyes.run(&mut r, 30);
        assert_eq!(eyes.shown(), EyeBreakPhase::Idle);
        // The rest runs out and the next cycle waits for GO; nothing starts it.
        eyes.run(&mut r, 26 * 60);
        let a = r.program();
        assert_eq!((a.paused, a.wait), (true, Some(ProgramWait::RestComplete)));
        assert!(r.deck.belt_still());
        assert!(
            !app_starts(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
        assert!(!eyes.rt.machine.holds_belt());
    }

    #[test]
    fn an_eye_break_leaves_a_still_belt_alone() {
        use crate::snapshot::eyebreak::EyeBreakPhase;
        let mut r = Rig::new();
        let mut eyes = Eyes::new(&r);
        eyes.run(&mut r, 5);
        let mark = r.mark();
        eyes.run(&mut r, 635);
        assert_eq!(eyes.rt.machine.end_seq(), 1, "a break came and went");
        assert_eq!(r.commands_since(mark), vec![]);
        assert_eq!(eyes.shown(), EyeBreakPhase::Idle);
    }

    /// Fresh Brewed Design, 2026-11-19: eye breaks show even when nothing is running. With
    /// no treadmill linked, the heads-up, the break with its blink tones and
    /// the end tone come at the screen-time interval, and nothing is sent.
    #[test]
    fn an_eye_break_comes_with_no_treadmill_linked() {
        use crate::snapshot::eyebreak::EyeBreakPhase;
        let mut r = Rig::new();
        let mut eyes = Eyes::new(&r);
        eyes.linked = false;
        let mark = r.mark();
        eyes.run(&mut r, 560);
        assert_eq!(eyes.shown(), EyeBreakPhase::Upcoming);
        eyes.run(&mut r, 45);
        assert_eq!(eyes.shown(), EyeBreakPhase::Break);
        let blinks = eyes.rt.machine.blink_seq();
        eyes.run(&mut r, 20);
        assert!(eyes.rt.machine.blink_seq() > blinks, "blink tones");
        assert_eq!(eyes.rt.machine.end_seq(), 1, "the end tone");
        assert_eq!(eyes.shown(), EyeBreakPhase::Idle, "no resume card");
        assert_eq!(r.commands_since(mark), vec![]);
    }

    /// The link drops mid-walk, so the last status still reads Running. A break
    /// then still comes, and sends nothing: unlinked, the belt is not the
    /// break's to pause.
    #[test]
    fn an_eye_break_after_the_link_drops_mid_walk_sends_nothing() {
        use crate::snapshot::eyebreak::EyeBreakPhase;
        let mut r = Rig::new();
        let mut eyes = Eyes::new(&r);
        eyes.run(&mut r, 5);
        r.press(DeckAction::Start);
        eyes.run(&mut r, 5);
        r.set_speed(3.0);
        eyes.run(&mut r, 20);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        eyes.linked = false;
        let mark = r.mark();
        eyes.run(&mut r, 580);
        assert_eq!(eyes.shown(), EyeBreakPhase::Break);
        assert!(!eyes.rt.machine.holds_belt());
        assert_eq!(r.controls_since(mark, Control::Pause), 0);
        assert!(
            !moves_belt(&r.commands_since(mark)),
            "{:?}",
            r.commands_since(mark)
        );
    }

    /// A manual walk at 3.0 km/h, level, settled.
    fn manual_walk_at_3() -> Rig {
        let mut r = Rig::new();
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(5.0);
        r.set_speed(3.0);
        r.run(10.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
        r
    }

    fn deck_byte(r: &Rig) -> i32 {
        (r.deck.target * 10.0).round() as i32
    }

    #[test]
    fn a_speed_nudge_after_the_remote_steps_from_the_remote_speed() {
        let mut r = manual_walk_at_3();
        let mark = r.mark();
        r.remote_speed(true, 10);
        r.run(5.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(
            r.commands_since(mark),
            vec![],
            "the app leaves the remote alone"
        );
        r.press(DeckAction::SpeedUp);
        r.run(2.0);
        assert_eq!(deck_byte(&r), 41, "up from the remote's 4.0");
        assert!(r.deck.running_at(4.1), "{} km/h", r.deck.speed_kmh());
        r.press(DeckAction::SpeedDown);
        r.run(1.0);
        r.press(DeckAction::SpeedDown);
        r.run(2.0);
        assert_eq!(deck_byte(&r), 39, "down from 4.1, twice");

        // Down on the remote, then down on the HUD.
        r.remote_speed(false, 5);
        r.run(3.0);
        assert!(r.deck.running_at(3.4), "{} km/h", r.deck.speed_kmh());
        r.press(DeckAction::SpeedDown);
        r.run(2.0);
        assert_eq!(deck_byte(&r), 33, "down from the remote's 3.4");

        // One up on the remote, one down on the HUD: back to the speed the
        // app last sent, which still goes out.
        r.remote_speed(true, 1);
        r.run(3.0);
        assert!(r.deck.running_at(3.4), "{} km/h", r.deck.speed_kmh());
        r.press(DeckAction::SpeedDown);
        r.run(2.0);
        assert!(r.deck.running_at(3.3), "{} km/h", r.deck.speed_kmh());
        assert_eq!(r.controls_since(0, Control::Pause), 0);
    }

    #[test]
    fn an_incline_nudge_after_the_remote_steps_from_the_remote_incline() {
        let mut r = manual_walk_at_3();
        let mark = r.mark();
        r.remote_incline(true, 5);
        r.run(3.0);
        assert_eq!(r.deck.incline, 5);
        assert_eq!(
            r.commands_since(mark),
            vec![],
            "the app leaves the remote alone"
        );
        r.press(DeckAction::InclineUp);
        r.run(2.0);
        assert_eq!(r.deck.incline, 6, "up from the remote's 5");
        r.press(DeckAction::InclineDown);
        r.run(1.0);
        r.press(DeckAction::InclineDown);
        r.run(2.0);
        assert_eq!(r.deck.incline, 4, "down from 6, twice");
        assert!(r.deck.running_at(3.0), "the speed stays put");

        r.remote_incline(false, 3);
        r.run(2.0);
        assert_eq!(r.deck.incline, 1);
        r.press(DeckAction::InclineUp);
        r.run(2.0);
        assert_eq!(r.deck.incline, 2, "up from the remote's 1");
    }

    #[test]
    fn a_nudge_of_one_keeps_what_the_remote_set_on_the_other() {
        let mut r = manual_walk_at_3();
        r.remote_speed(true, 5);
        r.remote_incline(true, 4);
        r.run(3.0);
        assert!(r.deck.running_at(3.5), "{} km/h", r.deck.speed_kmh());
        r.press(DeckAction::InclineUp);
        r.run(2.0);
        assert_eq!(r.deck.incline, 5);
        assert!(
            r.deck.running_at(3.5),
            "an incline press keeps the remote's speed"
        );
        r.remote_incline(false, 2);
        r.run(2.0);
        r.press(DeckAction::SpeedUp);
        r.run(2.0);
        assert!(r.deck.running_at(3.6), "{} km/h", r.deck.speed_kmh());
        assert_eq!(
            r.deck.incline, 3,
            "a speed press keeps the remote's incline"
        );
    }

    #[test]
    fn a_nudge_during_a_program_steps_from_the_remote_speed() {
        let mut r = post_meal_walking();
        r.remote_speed(true, 5);
        r.run(3.0);
        assert!(r.deck.running_at(4.5), "{} km/h", r.deck.speed_kmh());
        r.press(DeckAction::SpeedUp);
        r.run(2.0);
        assert!(r.deck.running_at(4.6), "{} km/h", r.deck.speed_kmh());
        let a = r.program();
        assert_eq!(
            (a.phase, a.paused, a.step_index),
            (ProgramPhase::Walking, false, 0)
        );
    }

    #[test]
    fn app_presses_mid_ramp_still_step_from_the_app_target() {
        let mut r = manual_walk_at_3();
        // A slider to 5.0, and + while the belt is still ramping up to it.
        r.set_speed(5.0);
        r.run(1.0);
        assert!(r.deck.speed_kmh() < 4.5, "{} km/h", r.deck.speed_kmh());
        r.press(DeckAction::SpeedUp);
        r.run(8.0);
        assert!(r.deck.running_at(5.1), "{} km/h", r.deck.speed_kmh());
        // Quick presses before the deck reports any of them all count.
        r.press(DeckAction::SpeedUp);
        r.run(0.05);
        r.press(DeckAction::SpeedUp);
        r.run(0.05);
        r.press(DeckAction::SpeedUp);
        r.run(3.0);
        assert!(r.deck.running_at(5.4), "{} km/h", r.deck.speed_kmh());
        r.press(DeckAction::InclineUp);
        r.run(0.05);
        r.press(DeckAction::InclineUp);
        r.run(3.0);
        assert_eq!(r.deck.incline, 2);
    }

    #[test]
    fn a_remote_press_against_an_app_ramp_is_the_base_once_the_deck_settles() {
        let mut r = manual_walk_at_3();
        r.set_speed(5.0);
        r.run(1.0);
        // Down on the remote while the belt ramps up to the app's 5.0: the
        // deck still climbs, but stops at its own 4.9.
        r.remote_speed(false, 1);
        r.run(10.0);
        assert!(r.deck.running_at(4.9), "{} km/h", r.deck.speed_kmh());
        r.press(DeckAction::SpeedUp);
        r.run(2.0);
        assert_eq!(deck_byte(&r), 50, "up from the deck's 4.9");
    }

    /// The owner's calibration from 2026-11-20: deck 1.2 runs 1.86 km/h, 2.3
    /// runs 3.62 and 4.0 runs 6.20.
    fn his_calibration() -> crate::persistence::AppConfig {
        let point = |hw: f32, real: f32| protocol::CalibrationPoint {
            speed: hw,
            stride_length_cm: 60.0,
            distance_scale: hw / real,
            runs: 1,
            stride_runs: 0,
        };
        crate::persistence::AppConfig {
            calibration_points: vec![point(1.2, 1.86), point(2.3, 3.62), point(4.0, 6.20)],
            max_speed: 6.0,
            ..Default::default()
        }
    }

    /// 2026-11-20 on the owner's treadmill: started at the treadmill on its
    /// 0.6 floor, an incline sweep, then a down and up presses. Before
    /// DECK_PROFILE knew the floor, the down press stored 0.6's step below
    /// the Min speed setting, so the first up press only came back to 0.6
    /// and sent nothing. Every up press now reaches the deck.
    #[test]
    fn a_down_press_at_the_min_speed_leaves_every_up_press_sending() {
        let mut r = Rig::new();
        r.state.sync_config(&his_calibration());
        r.deck.set_limits(6, 60);
        r.deck.set_incline_ceiling(6);
        r.state.max_incline = 6;
        r.run(10.0);
        r.deck.press_start(r.t);
        r.run(8.0);
        assert!(r.deck.running_at(0.6), "{} km/h", r.deck.speed_kmh());
        assert_eq!(r.state.deck_profile.floor(), None, "floor not learned yet");
        r.state.profile_sweep_enabled = true;
        r.state.profile_sweep_incline_start().unwrap();
        r.run(60.0);
        assert!(r.state.profile_sweep.is_none(), "the sweep finished");
        assert_eq!(r.deck.incline, 0, "back at the start incline");
        r.run(10.0);

        let mark = r.mark();
        r.press(DeckAction::SpeedDown);
        r.run(4.0);
        assert!(target_bytes_since(&r, mark).is_empty(), "nothing below 0.6");
        assert!(r.deck.running_at(0.6), "{} km/h", r.deck.speed_kmh());
        let shown = r.state.resolve_target().0;
        assert!((shown - 0.93).abs() < 0.01, "shown {shown}");
        for want in [7u8, 8, 9] {
            let mark = r.mark();
            r.press(DeckAction::SpeedUp);
            r.run(4.0);
            assert_eq!(target_bytes_since(&r, mark), vec![want], "one byte a press");
            assert!(
                r.deck.running_at(want as f32 / 10.0),
                "{} km/h",
                r.deck.speed_kmh()
            );
        }
        assert_eq!(r.controls_since(0, Control::Start), 0);
        assert_eq!(r.controls_since(0, Control::Ready), 0);
    }

    /// One point at 4.0 hardware km/h reading 0.8 of it: 3.2 km/h runs as
    /// byte 40, and without the point it would be byte 32.
    fn one_point() -> crate::persistence::AppConfig {
        crate::persistence::AppConfig {
            calibration_points: vec![protocol::CalibrationPoint {
                speed: 4.0,
                stride_length_cm: 70.0,
                distance_scale: 0.8,
                runs: 1,
                stride_runs: 0,
            }],
            ..Default::default()
        }
    }

    /// Issue #167: a points change through the settings sync (a row deleted in
    /// the old point panel) while the belt runs leaves the deck where it is.
    /// The new curve applies at the next speed set by hand, and a press steps
    /// from the byte the deck runs.
    #[test]
    fn a_calibration_sync_never_moves_a_running_belt() {
        let mut r = Rig::new();
        r.state.sync_config(&one_point());
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(6.0);
        r.set_speed(r.state.to_physical_speed(4.0));
        r.run(15.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());

        let mark = r.mark();
        r.state.set_test_now(r.t);
        r.state
            .sync_config(&crate::persistence::AppConfig::default());
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(target_bytes_since(&r, mark), Vec::<u8>::new());
        assert!(!moves_belt(&r.commands_since(mark)));

        // His press: one step up from the byte the deck runs.
        r.press(DeckAction::SpeedUp);
        r.run(3.0);
        assert!(r.deck.running_at(4.1), "{} km/h", r.deck.speed_kmh());
        // A speed set by hand goes through the new curve.
        r.set_speed(3.0);
        r.run(5.0);
        assert!(r.deck.running_at(3.0), "{} km/h", r.deck.speed_kmh());
    }

    /// The same for a program's belt: a new curve mid-step keeps the deck at
    /// the step's byte until the next step.
    #[test]
    fn a_calibration_sync_never_moves_a_programs_belt() {
        let mut r = post_meal_walking();
        let mark = r.mark();
        r.state.set_test_now(r.t);
        r.state.sync_config(&one_point());
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(target_bytes_since(&r, mark), Vec::<u8>::new());
        assert_eq!(r.controls_since(mark, Control::Pause), 0);
        assert_eq!(r.controls_since(mark, Control::Stop), 0);
    }

    /// #164: START FRESH empties the points and RESTORE brings them back,
    /// both on a running belt; neither moves the deck off its byte, and
    /// RESTORE ending a walk leaves the belt going.
    #[test]
    fn start_fresh_and_restore_never_move_a_running_belt() {
        let _globals = crate::calibration::TEST_GLOBALS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::calibration::close_all();
        crate::calibration::set_belt_for_test(Some(300.0));
        let mut r = Rig::new();
        r.state.sync_config(&one_point());
        r.run(1.0);
        r.press(DeckAction::Start);
        r.run(6.0);
        r.set_speed(r.state.to_physical_speed(4.0));
        r.run(15.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());

        let mark = r.mark();
        r.state.set_test_now(r.t);
        crate::calibration::start_fresh(&mut r.state).unwrap();
        assert!(r.state.calibration_points.is_empty());
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());

        crate::calibration::guide_start(&r.state, 5.0).unwrap();
        r.state.set_test_now(r.t);
        crate::calibration::restore(&mut r.state).unwrap();
        assert_eq!(r.state.calibration_points, one_point().calibration_points);
        r.run(20.0);
        assert!(r.deck.running_at(4.0), "{} km/h", r.deck.speed_kmh());
        assert_eq!(target_bytes_since(&r, mark), Vec::<u8>::new());
        assert!(!moves_belt(&r.commands_since(mark)));
        assert_eq!(r.controls_since(mark, Control::Stop), 0);
        assert_eq!(r.controls_since(mark, Control::Pause), 0);
    }
}
