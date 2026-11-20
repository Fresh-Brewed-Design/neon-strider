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
        }
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
        let accepted = match (self.phase, control) {
            // Standby refuses every start; the countdown and the stop's
            // wind-down refuse everything but polls (docs/protocol.md).
            (Standby | Countdown | Stopping, _) => false,
            (Idle | Ended | Paused, Control::Ready | Control::Start) => {
                self.begin_countdown(t);
                true
            }
            (Running, Control::Target) => {
                self.target = speed as f32 / 10.0;
                self.incline = incline;
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
            Control::Target => bytes.to_vec(),
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
            return; // an echo or nack: the app ignores these
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
    use crate::programs::{Program, ProgramPhase, ProgramStep, ProgramToast, ProgramWait};

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
                },
                ProgramStep {
                    duration_minutes: Some(2.0),
                    speed_kmh: 3.0,
                    incline_pct: 0.0,
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
                },
                protocol::CalibrationPoint {
                    speed: 1.5,
                    stride_length_cm: 60.0,
                    distance_scale: 1.5 / 4.84,
                },
                protocol::CalibrationPoint {
                    speed: 2.0,
                    stride_length_cm: 65.0,
                    distance_scale: 2.0 / 2.97,
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
}
