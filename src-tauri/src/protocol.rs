//! The FitShow wire format: packet framing, the control commands we send and
//! the 0x51 status frame the deck answers with. Pure, no I/O.
//!
//! Every packet is `HEADER, payload..., checksum, TAIL`, where the checksum is
//! the XOR of every byte after the header. See `docs/protocol.md`.

use serde::{Deserialize, Serialize};

/// One point of the user's calibration. Near hardware `speed`, the real belt
/// speed is the deck's speed divided by `distance_scale`, and one step is
/// `stride_length_cm` long.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CalibrationPoint {
    pub speed: f32,
    #[serde(alias = "strideLengthCm")]
    pub stride_length_cm: f32,
    #[serde(alias = "distanceScale")]
    pub distance_scale: f32,
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
    Starting = 1,
    Running = 2,
    Paused = 3,
}

impl MachineStatus {
    /// The belt is turning or about to: Running or Starting.
    pub fn is_moving(self) -> bool {
        matches!(self, MachineStatus::Running | MachineStatus::Starting)
    }
}

/// One decoded 0x51 status frame from the deck. Speed, distance and calories are
/// the deck's own raw values, before calibration.
#[derive(Debug, Clone)]
pub struct Telemetry {
    /// What the engine acts on. An unknown status byte maps to Stopped here (so
    /// nothing starts or ends a session on a guess); `status_known` says so.
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

/// The framed status request.
pub fn status_request() -> Vec<u8> {
    serialize_command(&[HEADER, CMD_STATUS])
}

/// The status byte values we have seen and understood. Anything else is
/// unknown (see docs/protocol.md, "Capturing the standby state").
fn status_from_byte(b: u8) -> Option<MachineStatus> {
    match b {
        1 | 3 => Some(MachineStatus::Running),
        2 | 10 => Some(MachineStatus::Paused),
        4 => Some(MachineStatus::Starting),
        0 => Some(MachineStatus::Stopped),
        // Deck asleep, screen off: seen about 14 minutes after a stop and on
        // connecting to a sleeping deck, never during a walk.
        6 => Some(MachineStatus::Stopped),
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
    fn test_telemetry_parsing_le() {
        // Status 1 (Running), speed 30 (3.0 km/h), incline 5, time 120 s,
        // distance 150 m, calories 4.5, steps 1000.
        let bytes = vec![
            0x02, 0x51, 0x01, 30, 5, 0x78, 0x00, // Time (index 5, 6)
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
            (1, MachineStatus::Running),
            (3, MachineStatus::Running),
            (2, MachineStatus::Paused),
            (10, MachineStatus::Paused),
            (4, MachineStatus::Starting),
            (6, MachineStatus::Stopped),
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
        assert!(parse_status(&[0x02, 0x51, 0x01, 0x00]).is_none());
        assert!(parse_status(&[0x01, 0x51, 0x01, 0x00, 0x03]).is_none());
        assert!(parse_status(&[0x02, 0x53, 0x01, 0x00, 0x03]).is_none());
        // Status only: the counters read as zero.
        let t = parse_status(&[0x02, 0x51, 0x01, 30, 5, 0x78, 0x00, 0x03]).unwrap();
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

    #[test]
    fn moving_means_running_or_starting() {
        assert!(MachineStatus::Running.is_moving());
        assert!(MachineStatus::Starting.is_moving());
        assert!(!MachineStatus::Paused.is_moving());
        assert!(!MachineStatus::Stopped.is_moving());
    }
}
