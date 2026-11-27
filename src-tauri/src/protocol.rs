//! The FitShow wire format: packet framing, the control commands we send and
//! the 0x51 status frame the deck answers with. Pure, no I/O.
//!
//! Every packet is `HEADER, payload..., checksum, TAIL`, where the checksum is
//! the XOR of every byte after the header. See `docs/protocol.md`.

use serde::{Deserialize, Serialize};

/// One point of the user's calibration. Near hardware `speed`, the real belt
/// speed is the deck's speed divided by `distance_scale`, and one step is
/// `stride_length_cm` long.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CalibrationPoint {
    pub speed: f32,
    #[serde(alias = "strideLengthCm")]
    pub stride_length_cm: f32,
    #[serde(alias = "distanceScale")]
    pub distance_scale: f32,
    /// How many timed runs the point averages; older points count as one.
    #[serde(default = "one_run")]
    pub runs: u32,
    /// How many footstep runs (GAIT_SYNC) the stride averages, counted apart
    /// from the belt runs in `runs`. 0 means the stride was never measured:
    /// it was borrowed from the curve when a belt run saved the point. A point
    /// stored before strides were counted has no such field and reads as 0,
    /// so its first measured stride replaces the stored one.
    #[serde(default)]
    pub stride_runs: u32,
}

fn one_run() -> u32 {
    1
}

/// A wanted belt state: calibrated (physical) km/h and a hardware incline level.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct ControlIntent {
    pub speed: f32,
    pub incline: u8,
}

/// The two-packet start (or pause) sequence the deck needs: `Ready` (or a
/// first pause) primes it, then `Start` (or a second pause) acts.
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum HandshakeStep {
    Idle,
    /// The priming packet is due.
    Priming,
    /// The priming packet went out; the start (or pause) packet is due.
    Starting,
}

const HEADER: u8 = 0x02;
const TAIL: u8 = 0x03;

/// Byte 1: the PC asks for a status frame (and the deck answers with one).
const CMD_STATUS: u8 = 0x51;
/// Byte 1: a control packet; byte 2 says which (see [`Control`]).
const CMD_CONTROL: u8 = 0x53;

/// Control packets are this long before the checksum and tail.
/// 15-byte control packets (docs/protocol.md) less the checksum and tail.
const CONTROL_BODY_LEN: usize = 13;

/// Byte 2 of a control packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Control {
    /// Ready to start: the first half of the start handshake.
    Ready = 0x01,
    /// Set the target speed (byte 3, 0.1 km/h) and incline level (byte 4).
    Target = 0x02,
    Stop = 0x03,
    /// Start or resume: the second half of the start handshake.
    Start = 0x09,
    Pause = 0x0A,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum MachineStatus {
    Stopped = 0,
    /// The deck's 3-2-1 countdown after a start or resume: the belt is about
    /// to move.
    Starting = 1,
    Running = 2,
    Paused = 3,
    /// The deck took a stop and is winding the belt down to 0. It answers
    /// anything but a status poll with a nack, so nothing else is sent.
    Stopping = 4,
}

impl MachineStatus {
    /// The belt is turning or about to: Running or Starting. Not Stopping: the
    /// belt is on its way down, and nothing (a session, a GO) begins there.
    pub fn is_moving(self) -> bool {
        matches!(self, MachineStatus::Running | MachineStatus::Starting)
    }

    /// The HUD's word for this state, reached from `from`: the header's words
    /// (`src/hud/status.ts` `linkLabel`), and RE-JACK for a belt moving again
    /// after STASIS. The DECK toast says it only while no HUD panel shows.
    pub fn hud_word(self, from: MachineStatus) -> &'static str {
        match self {
            MachineStatus::Stopped => "STANDBY",
            MachineStatus::Starting => "SPIN-UP",
            MachineStatus::Running if from == MachineStatus::Paused => "RE-JACK",
            MachineStatus::Running => "RUNNING",
            MachineStatus::Paused => "STASIS",
            MachineStatus::Stopping => "SPIN-DOWN",
        }
    }
}

/// Status byte for standby (screen off, or switched off on the remote): the
/// deck stays connected and answers polls, but refuses every start.
pub const STATUS_STANDBY: u8 = 6;

/// One decoded 0x51 status frame from the deck. Speed, distance and calories are
/// the deck's own raw values, before calibration.
#[derive(Debug, Clone)]
pub struct Telemetry {
    /// What the engine acts on. An unknown status byte maps to Stopped here,
    /// with `status_known` false, and the engine then keeps the last known
    /// status instead (`OrchestratorState::take_status`).
    pub status: MachineStatus,
    /// The raw status byte (index 2 of the 0x51 frame).
    pub status_code: u8,
    /// `status_code` is one we recognise.
    pub status_known: bool,
    pub speed_kmh: f32,
    pub incline: u8,
    pub time_sec: u16,
    pub distance_km: f32,
    pub calories: f32,
    pub steps: u16,
}

/// XOR of every byte after the header.
fn calculate_checksum(data: &[u8]) -> u8 {
    data.iter().skip(1).fold(0, |acc, b| acc ^ b)
}

/// Frame `body` (header first) with its checksum and tail.
fn serialize_command(body: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(body.len() + 2);
    result.extend_from_slice(body);
    result.push(calculate_checksum(body));
    result.push(TAIL);
    result
}

/// A framed control packet. `speed` and `incline` only mean something for
/// [`Control::Target`]; the deck expects zeros otherwise.
pub fn control_packet(control: Control, speed: u8, incline: u8) -> Vec<u8> {
    let mut body = [0u8; CONTROL_BODY_LEN];
    body[0] = HEADER;
    body[1] = CMD_CONTROL;
    body[2] = control as u8;
    body[3] = speed;
    body[4] = incline;
    serialize_command(&body)
}

/// The longest body the diagnostics command cycler may send: with the header,
/// checksum and tail it still fits one 20-byte BLE write.
pub const RAW_BODY_MAX: usize = 17;

/// Frame a body from the command cycler (`53 01 00 ...`, no header) as
/// `HEADER, body..., checksum, TAIL`.
pub fn raw_packet(body: &[u8]) -> Result<Vec<u8>, String> {
    if body.is_empty() || body.len() > RAW_BODY_MAX {
        return Err(format!(
            "A raw command needs 1 to {RAW_BODY_MAX} bytes, not {}.",
            body.len()
        ));
    }
    let mut framed = Vec::with_capacity(body.len() + 1);
    framed.push(HEADER);
    framed.extend_from_slice(body);
    Ok(serialize_command(&framed))
}

/// What a non-status notification from the deck says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckReply {
    /// `02 53 53 03`: the deck refused the packet.
    Nack,
    /// The deck took a target and echoes the speed and incline byte it took,
    /// which may differ from what was asked (it clamps to its floor).
    TargetEcho { speed: u8, incline: u8 },
}

/// Decode a nack or a target echo; anything else (a status frame, another
/// control's echo) is None.
pub fn parse_reply(bytes: &[u8]) -> Option<DeckReply> {
    if bytes == [HEADER, CMD_CONTROL, CMD_CONTROL, TAIL] {
        return Some(DeckReply::Nack);
    }
    if bytes.len() != CONTROL_BODY_LEN + 2 || bytes[0] != HEADER || bytes[1] != CMD_CONTROL {
        return None;
    }
    if bytes[2] != Control::Target as u8 {
        return None;
    }
    let (speed, incline) = (bytes[3], bytes[4]);
    (control_packet(Control::Target, speed, incline) == bytes)
        .then_some(DeckReply::TargetEcho { speed, incline })
}

/// The framed status request.
pub fn status_request() -> Vec<u8> {
    serialize_command(&[HEADER, CMD_STATUS])
}

/// The status byte values we have seen and understood, from traffic
/// recordings of the Jogwell Ares 11 Elite. Anything else is unknown (see
/// docs/protocol.md, "Status Byte").
fn status_from_byte(b: u8) -> Option<MachineStatus> {
    match b {
        0 => Some(MachineStatus::Stopped),
        // The workout just ended (after a stop, or when a pause ends): belt
        // still, totals still shown for about 3 s, then 0. The session ends
        // here, with the deck's totals still in the frame.
        1 => Some(MachineStatus::Stopped),
        // The 3-2-1 countdown after a start or a resume (`02 51 02 <n>`).
        2 => Some(MachineStatus::Starting),
        3 => Some(MachineStatus::Running),
        // After a stop: the belt winds down to 0, then 1.
        4 => Some(MachineStatus::Stopping),
        // Standby (screen off, or off on the remote): seen about 14 minutes
        // after a stop and on connecting to a sleeping deck, never during a walk.
        STATUS_STANDBY => Some(MachineStatus::Stopped),
        10 => Some(MachineStatus::Paused),
        _ => None,
    }
}

fn le_u16(lo: u8, hi: u8) -> u16 {
    u16::from_le_bytes([lo, hi])
}

/// Decode a 0x51 status frame. A frame too short for the counters (5 to 12
/// bytes) still carries the status; the counters read as zero.
pub fn parse_status(bytes: &[u8]) -> Option<Telemetry> {
    if bytes.len() < 5 || bytes[0] != HEADER || bytes[1] != CMD_STATUS {
        return None;
    }

    let status_code = bytes[2];
    let known = status_from_byte(status_code);
    let mut t = Telemetry {
        status: known.unwrap_or(MachineStatus::Stopped),
        status_code,
        status_known: known.is_some(),
        speed_kmh: 0.0,
        incline: 0,
        time_sec: 0,
        distance_km: 0.0,
        calories: 0.0,
        steps: 0,
    };
    if bytes.len() < 13 {
        return Some(t);
    }

    // Counters are little-endian u16 pairs: time (s), distance (m),
    // calories (0.1 kcal) and steps.
    t.speed_kmh = bytes[3] as f32 / 10.0;
    t.incline = bytes[4];
    t.time_sec = le_u16(bytes[5], bytes[6]);
    t.distance_km = le_u16(bytes[7], bytes[8]) as f32 / 1000.0;
    t.calories = le_u16(bytes[9], bytes[10]) as f32 / 10.0;
    t.steps = le_u16(bytes[11], bytes[12]);
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_decode_a_nack_and_a_target_echo_only() {
        assert_eq!(
            parse_reply(&[0x02, 0x53, 0x53, 0x03]),
            Some(DeckReply::Nack)
        );
        let echo = control_packet(Control::Target, 6, 2);
        assert_eq!(
            parse_reply(&echo),
            Some(DeckReply::TargetEcho {
                speed: 6,
                incline: 2
            })
        );
        let mut bad = echo.clone();
        bad[13] ^= 1;
        assert_eq!(parse_reply(&bad), None, "checksum");
        assert_eq!(parse_reply(&control_packet(Control::Stop, 0, 0)), None);
        assert_eq!(parse_reply(&[0x02, 0x51, 0x03, 0x03]), None);
    }

    #[test]
    fn test_telemetry_parsing_le() {
        // Status 3 (Running), speed 30 (3.0 km/h), incline 5, time 120 s,
        // distance 150 m, calories 4.5, steps 1000.
        let bytes = vec![
            0x02, 0x51, 0x03, 30, 5, 0x78, 0x00, // Time (index 5, 6)
            0x96, 0x00, // Distance (index 7, 8)
            0x2D, 0x00, // Calories (index 9, 10)
            0xE8, 0x03, // Steps (index 11, 12)
            0x00, 0x03, // Checksum, tail
        ];

        let t = parse_status(&bytes).unwrap();
        assert_eq!(t.status, MachineStatus::Running);
        assert_eq!(t.speed_kmh, 3.0);
        assert_eq!(t.incline, 5);
        assert_eq!(t.time_sec, 120);
        assert_eq!(t.distance_km, 0.150);
        assert_eq!(t.calories, 4.5);
        assert_eq!(t.steps, 1000);
    }

    #[test]
    fn unknown_status_bytes_stay_distinguishable() {
        let t = parse_status(&[0x02, 0x51, 0x07, 0x00, 0x03]).unwrap();
        assert_eq!(t.status, MachineStatus::Stopped);
        assert_eq!(t.status_code, 7);
        assert!(!t.status_known);
        for (b, s) in [
            (0, MachineStatus::Stopped),
            (1, MachineStatus::Stopped),
            (2, MachineStatus::Starting),
            (3, MachineStatus::Running),
            (4, MachineStatus::Stopping),
            (6, MachineStatus::Stopped),
            (10, MachineStatus::Paused),
        ] {
            let t = parse_status(&[0x02, 0x51, b, 0x00, 0x03]).unwrap();
            assert_eq!((t.status, t.status_known, t.status_code), (s, true, b));
        }
    }

    #[test]
    fn a_captured_frame_decodes() {
        // A frame captured from the deck; bytes 5 to 12 are (low, high) pairs.
        let bytes = vec![
            0x02, 0x51, 0x01, 35, // Speed: 3.5 km/h
            10, // Incline: 10
            0xDE, 0x00, // Time: 222 s
            0x40, 0x06, // Distance: 1600 m
            0x96, 0x00, // Calories: 15.0 (raw 150)
            0xD0, 0x07, // Steps: 2000
            0x00, 0x03,
        ];

        let t = parse_status(&bytes).unwrap();
        assert_eq!(t.time_sec, 222, "Time LE Parsing Failed");
        assert_eq!(t.distance_km, 1.6, "Distance Meter Scaling Failed");
        assert_eq!(t.calories, 15.0, "Calories 0.1 Scaling Failed");
        assert_eq!(t.steps, 2000, "Step Integer Parsing Failed");
    }

    #[test]
    fn short_or_foreign_frames() {
        assert!(parse_status(&[0x02, 0x51, 0x03, 0x00]).is_none());
        assert!(parse_status(&[0x01, 0x51, 0x03, 0x00, 0x03]).is_none());
        assert!(parse_status(&[0x02, 0x53, 0x03, 0x00, 0x03]).is_none());
        // Status only: the counters read as zero.
        let t = parse_status(&[0x02, 0x51, 0x03, 30, 5, 0x78, 0x00, 0x03]).unwrap();
        assert_eq!(
            (t.status, t.speed_kmh, t.incline, t.time_sec),
            (MachineStatus::Running, 0.0, 0, 0)
        );
    }

    /// The exact bytes on the wire. These are what the deck was tested with; any
    /// change here is a protocol change.
    #[test]
    fn packets_are_byte_exact() {
        let zeros = [0u8; 10];
        let control = |code: u8, checksum: u8| {
            let mut v = vec![0x02, 0x53, code];
            v.extend_from_slice(&zeros);
            v.extend_from_slice(&[checksum, 0x03]);
            v
        };
        assert_eq!(control_packet(Control::Ready, 0, 0), control(0x01, 0x52));
        assert_eq!(control_packet(Control::Stop, 0, 0), control(0x03, 0x50));
        assert_eq!(control_packet(Control::Start, 0, 0), control(0x09, 0x5A));
        assert_eq!(control_packet(Control::Pause, 0, 0), control(0x0A, 0x59));
        assert_eq!(
            control_packet(Control::Target, 35, 4),
            vec![
                0x02,
                0x53,
                0x02,
                35,
                4,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0x53 ^ 0x02 ^ 35 ^ 4,
                0x03
            ]
        );
        assert_eq!(control_packet(Control::Target, 35, 4).len(), 15);
        assert_eq!(status_request(), vec![0x02, 0x51, 0x51, 0x03]);
        // The checksum skips the header.
        assert_eq!(calculate_checksum(&[0xFF, 0x01, 0x02]), 0x03);
        assert_eq!(calculate_checksum(&[0x02]), 0);
    }

    /// The cycler's raw bodies frame exactly like the app's own packets.
    #[test]
    fn raw_bodies_are_framed() {
        let body = |code: u8| {
            let mut v = vec![0x53, code];
            v.extend_from_slice(&[0u8; 10]);
            v
        };
        assert_eq!(
            raw_packet(&body(0x01)),
            Ok(control_packet(Control::Ready, 0, 0))
        );
        assert_eq!(
            raw_packet(&body(0x09)),
            Ok(control_packet(Control::Start, 0, 0))
        );
        assert_eq!(
            raw_packet(&body(0x03)),
            Ok(control_packet(Control::Stop, 0, 0))
        );
        assert_eq!(raw_packet(&[0x51]), Ok(vec![0x02, 0x51, 0x51, 0x03]));
        assert!(raw_packet(&[]).is_err());
        assert!(raw_packet(&[0; RAW_BODY_MAX + 1]).is_err());
        assert_eq!(raw_packet(&[0; RAW_BODY_MAX]).map(|p| p.len()), Ok(20));
    }

    #[test]
    fn moving_means_running_or_starting() {
        assert!(MachineStatus::Running.is_moving());
        assert!(MachineStatus::Starting.is_moving());
        assert!(!MachineStatus::Paused.is_moving());
        assert!(!MachineStatus::Stopped.is_moving());
        assert!(!MachineStatus::Stopping.is_moving());
    }

    /// Frames from a traffic recording of the Jogwell Ares 11 Elite.
    #[test]
    fn recorded_frames_decode() {
        let status = |b: &[u8]| parse_status(b).map(|t| (t.status, t.status_code));
        // Engage: the 3-2-1 countdown comes in short frames, then running.
        for n in [3, 2, 1] {
            let frame = [0x02, 0x51, 0x02, n, 0x51 ^ 0x02 ^ n, 0x03];
            assert_eq!(status(&frame), Some((MachineStatus::Starting, 2)));
        }
        let running = [
            0x02, 0x51, 0x03, 0x0d, 0x00, 0x05, 0x00, 0x01, 0x00, 0x01, 0x00, 0x05, 0x00, 0x00,
            0x00, 0x5f, 0x03,
        ];
        assert_eq!(status(&running), Some((MachineStatus::Running, 3)));
        // Stop: the belt winds down in status 4, the workout ends (1), then 0.
        let winding = [
            0x02, 0x51, 0x04, 0x08, 0x00, 0x24, 0x00, 0x0b, 0x00, 0x0f, 0x00, 0x2d, 0x00, 0x00,
            0x00, 0x50, 0x03,
        ];
        let t = parse_status(&winding).unwrap();
        assert_eq!(
            (t.status, t.speed_kmh, t.time_sec),
            (MachineStatus::Stopping, 0.8, 36)
        );
        let ended = [
            0x02, 0x51, 0x01, 0x00, 0x00, 0x24, 0x00, 0x0b, 0x00, 0x0f, 0x00, 0x2d, 0x00, 0x00,
            0x00, 0x5d, 0x03,
        ];
        let t = parse_status(&ended).unwrap();
        assert_eq!((t.status, t.time_sec), (MachineStatus::Stopped, 36));
        assert_eq!(
            status(&[0x02, 0x51, 0x00, 0x51, 0x03]),
            Some((MachineStatus::Stopped, 0))
        );
        // Standby, after a power-off held on the remote.
        assert_eq!(
            status(&[0x02, 0x51, 0x06, 0x02, 0x55, 0x03]),
            Some((MachineStatus::Stopped, STATUS_STANDBY))
        );
        // A pause winds the belt down in status 10.
        let pausing = [
            0x02, 0x51, 0x0a, 0x08, 0x00, 0x1f, 0x00, 0x0a, 0x00, 0x0e, 0x00, 0x27, 0x00, 0x00,
            0x00, 0x6f, 0x03,
        ];
        assert_eq!(status(&pausing), Some((MachineStatus::Paused, 10)));
    }

    #[test]
    fn hud_words_are_the_apps_not_the_protocols() {
        use MachineStatus::*;
        assert_eq!(Stopping.hud_word(Running), "SPIN-DOWN");
        assert_eq!(Starting.hud_word(Stopped), "SPIN-UP");
        assert_eq!(Running.hud_word(Starting), "RUNNING");
        assert_eq!(Running.hud_word(Paused), "RE-JACK");
        assert_eq!(Paused.hud_word(Running), "STASIS");
        assert_eq!(Stopped.hud_word(Stopping), "STANDBY");
    }
}
