//! GHOST_POD: the PC poses as a Bluetooth foot pod so a watch can read the
//! belt (roadmap #55; research in docs/watch-pod.md).
//!
//! Garmin watches pair natively with a Running Speed and Cadence (RSC) foot
//! pod, not with FTMS, so the pod serves the RSC service (0x1814). This file is
//! the pure part: the RSC byte layouts, the SC control point, what to report
//! for the deck right now, and the status text. `ghost_pod/worker.rs` owns the
//! setting and the 1 s tick; the GATT server itself is in `platform/`.
//!
//! It only reports. RSC has no message from the watch for pause or resume, so
//! nothing here ever moves the belt. Pausing the belt makes the pod report
//! 0 km/h, which lets the watch's Auto Pause stop its timer.

#[path = "ghost_pod/worker.rs"]
mod worker;

pub use worker::{init, set_enabled, set_wrist_link, snapshot, spawn};

use crate::engine::{OrchestratorState, STILL_KMH};
use serde::{Deserialize, Serialize};

/// Running Speed and Cadence service.
pub const RSC_SERVICE: u16 = 0x1814;
/// RSC Measurement (notify).
pub const RSC_MEASUREMENT: u16 = 0x2A53;
/// RSC Feature (read).
pub const RSC_FEATURE: u16 = 0x2A54;
/// SC Control Point (write, indicate).
pub const SC_CONTROL_POINT: u16 = 0x2A55;

/// RSC Measurement flag bits.
const FLAG_STRIDE: u8 = 0x01;
const FLAG_DISTANCE: u8 = 0x02;
const FLAG_RUNNING: u8 = 0x04;

/// RSC Feature bits: stride length, total distance, walking or running status.
pub const FEATURES: u16 = 0x0007;

/// At or above this the pod says "running" rather than "walking".
const RUNNING_KMH: f32 = 7.0;

/// SC control point op codes and result codes.
const OP_SET_CUMULATIVE: u8 = 0x01;
const OP_RESPONSE: u8 = 0x10;
const RESULT_SUCCESS: u8 = 0x01;
const RESULT_NOT_SUPPORTED: u8 = 0x02;
const RESULT_INVALID_PARAMETER: u8 = 0x03;

/// The RSC Feature characteristic's value (little-endian u16).
pub fn feature_value() -> Vec<u8> {
    FEATURES.to_le_bytes().to_vec()
}

/// One RSC Measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RscFrame {
    pub speed_mps: f32,
    /// Steps per minute.
    pub cadence_spm: f32,
    /// Stride length, cm.
    pub stride_cm: f32,
    /// Distance since the pod started (or since the watch set it), metres.
    pub total_m: f64,
    pub running: bool,
}

impl RscFrame {
    pub const STILL: RscFrame = RscFrame {
        speed_mps: 0.0,
        cadence_spm: 0.0,
        stride_cm: 0.0,
        total_m: 0.0,
        running: false,
    };
}

fn clamp_round(v: f64, max: f64) -> f64 {
    if v.is_finite() {
        v.round().clamp(0.0, max)
    } else {
        0.0
    }
}

/// Encode an RSC Measurement: flags (u8), instantaneous speed (u16, 1/256 m/s),
/// cadence (u8, steps per minute), stride length (u16, 1/100 m), total
/// distance (u32, 1/10 m), all little-endian (RSC Service 1.0.1).
pub fn encode_measurement(f: &RscFrame) -> Vec<u8> {
    let mut flags = FLAG_STRIDE | FLAG_DISTANCE;
    if f.running {
        flags |= FLAG_RUNNING;
    }
    let speed = clamp_round(f.speed_mps as f64 * 256.0, u16::MAX as f64) as u16;
    let cadence = clamp_round(f.cadence_spm as f64, u8::MAX as f64) as u8;
    let stride = clamp_round(f.stride_cm as f64, u16::MAX as f64) as u16;
    let total = clamp_round(f.total_m * 10.0, u32::MAX as f64) as u32;
    let mut out = Vec::with_capacity(10);
    out.push(flags);
    out.extend_from_slice(&speed.to_le_bytes());
    out.push(cadence);
    out.extend_from_slice(&stride.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out
}

/// What a write to the SC control point asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlRequest {
    /// Set the total distance, metres.
    SetTotal(f64),
    /// Anything else: answered with `response`, changes nothing.
    Other,
}

/// Read a control point write. Returns the request and the indication to send
/// back: `[0x10, op code, result]`. Only Set Cumulative Value is supported
/// (no calibration, one sensor location).
pub fn control_point(value: &[u8]) -> (ControlRequest, Vec<u8>) {
    let Some(&op) = value.first() else {
        return (
            ControlRequest::Other,
            vec![OP_RESPONSE, 0, RESULT_NOT_SUPPORTED],
        );
    };
    if op != OP_SET_CUMULATIVE {
        return (
            ControlRequest::Other,
            vec![OP_RESPONSE, op, RESULT_NOT_SUPPORTED],
        );
    }
    match <[u8; 4]>::try_from(&value[1..]) {
        Ok(b) => (
            ControlRequest::SetTotal(u32::from_le_bytes(b) as f64 / 10.0),
            vec![OP_RESPONSE, op, RESULT_SUCCESS],
        ),
        Err(_) => (
            ControlRequest::Other,
            vec![OP_RESPONSE, op, RESULT_INVALID_PARAMETER],
        ),
    }
}

/// The pod's own distance: what it has reported, so it never jumps back when
/// the app starts a new session. Set Cumulative Value from the watch resets it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PodMeter {
    pub total_m: f64,
}

impl PodMeter {
    /// Add `dt_s` seconds at `speed_mps`. Gaps over 5 s (sleep, a stalled
    /// tick) add nothing.
    pub fn advance(&mut self, speed_mps: f32, dt_s: f64) {
        if dt_s > 0.0 && dt_s <= 5.0 && speed_mps > 0.0 {
            self.total_m += speed_mps as f64 * dt_s;
        }
    }
}

/// The deck right now as a pod reading (total distance not filled in). A
/// paused, stopped or still belt reads 0 km/h and 0 steps per minute.
pub fn frame_for(state: &OrchestratorState) -> RscFrame {
    let kmh = state.to_physical_speed(state.hardware_speed);
    if !(state.last_status.is_moving() && kmh > STILL_KMH) {
        return RscFrame::STILL;
    }
    let speed_mps = kmh / 3.6;
    let stride_cm = state.calculate_stride_length(kmh).max(0.0);
    let cadence_spm = if stride_cm > 0.0 {
        speed_mps * 100.0 / stride_cm * 60.0
    } else {
        0.0
    };
    RscFrame {
        speed_mps,
        cadence_spm,
        stride_cm,
        total_m: 0.0,
        running: kmh >= RUNNING_KMH,
    }
}

/// Where the pod is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
#[serde(rename_all = "snake_case")]
pub enum PodStatus {
    #[default]
    Off,
    Starting,
    /// Advertising, no watch listening yet.
    Advertising,
    /// A watch is subscribed to the measurements.
    Connected,
    Error,
}

/// GHOST_POD as the windows see it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct GhostPodSnapshot {
    /// The Labs switch (off by default).
    pub enabled: bool,
    pub status: PodStatus,
    /// The deck-voice word for the status (`GHOST_POD // LINKED`).
    pub label: String,
    /// One plain sentence for the status.
    pub message: String,
    /// Watches subscribed to the measurements.
    pub watches: u32,
    /// What the pod is sending now, km/h (0 while the belt is still).
    pub speed_kmh: f32,
    /// WRIST_LINK, the watch data field's service served next to the pod.
    pub wrist_link: crate::wrist_link::WristLinkSnapshot,
}

/// The status word, in the deck's voice.
pub fn status_label(status: PodStatus) -> &'static str {
    match status {
        PodStatus::Off => "OFFLINE",
        PodStatus::Starting => "BOOTING",
        PodStatus::Advertising => "BROADCASTING",
        PodStatus::Connected => "LINKED",
        PodStatus::Error => "FAULT",
    }
}

/// The sentence for a status; `error` is the reason when it failed.
pub fn status_message(status: PodStatus, error: Option<&str>) -> String {
    match status {
        PodStatus::Off => "Switch it on to let a watch read the belt as a foot pod.".to_string(),
        PodStatus::Starting => "Starting the pod.".to_string(),
        PodStatus::Advertising => {
            "Waiting for a watch. Add a foot pod sensor on it; the pod shows under this PC's Bluetooth name."
                .to_string()
        }
        PodStatus::Connected => {
            "A watch is connected and gets the belt speed every second.".to_string()
        }
        PodStatus::Error => error.unwrap_or("The pod stopped.").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::MachineStatus;

    #[test]
    fn measurement_matches_the_spec_layout() {
        // 4.5 km/h = 1.25 m/s = 320/256; 110 spm; 68 cm; 1234.5 m.
        let f = RscFrame {
            speed_mps: 1.25,
            cadence_spm: 110.0,
            stride_cm: 68.0,
            total_m: 1234.5,
            running: false,
        };
        assert_eq!(
            encode_measurement(&f),
            vec![
                0x03, // stride + total distance, walking
                0x40, 0x01, // 320
                110,  // cadence
                0x44, 0x00, // 68 cm
                0x39, 0x30, 0x00, 0x00, // 12345 dm
            ]
        );
    }

    #[test]
    fn running_sets_the_status_bit_and_still_reads_zero() {
        let mut f = RscFrame::STILL;
        f.running = true;
        assert_eq!(encode_measurement(&f)[0], 0x07);
        let still = encode_measurement(&RscFrame::STILL);
        assert_eq!(still, vec![0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn measurement_fields_saturate_instead_of_wrapping() {
        let f = RscFrame {
            speed_mps: 1000.0,
            cadence_spm: 400.0,
            stride_cm: -5.0,
            total_m: f64::NAN,
            running: false,
        };
        let b = encode_measurement(&f);
        assert_eq!(&b[1..3], &[0xff, 0xff]);
        assert_eq!(b[3], 255);
        assert_eq!(&b[4..6], &[0, 0]);
        assert_eq!(&b[6..10], &[0, 0, 0, 0]);
    }

    #[test]
    fn feature_says_stride_distance_and_status() {
        assert_eq!(feature_value(), vec![0x07, 0x00]);
    }

    #[test]
    fn control_point_sets_the_total_and_refuses_the_rest() {
        let (req, resp) = control_point(&[0x01, 0x39, 0x30, 0x00, 0x00]);
        assert_eq!(req, ControlRequest::SetTotal(1234.5));
        assert_eq!(resp, vec![0x10, 0x01, 0x01]);

        // Calibration, sensor location: not supported.
        assert_eq!(control_point(&[0x02]).1, vec![0x10, 0x02, 0x02]);
        assert_eq!(control_point(&[0x03, 0x01]).1, vec![0x10, 0x03, 0x02]);
        assert_eq!(control_point(&[0x04]).1, vec![0x10, 0x04, 0x02]);
        // A short Set Cumulative Value is an invalid parameter.
        let (req, resp) = control_point(&[0x01, 0x39]);
        assert_eq!(req, ControlRequest::Other);
        assert_eq!(resp, vec![0x10, 0x01, 0x03]);
        assert_eq!(control_point(&[]).0, ControlRequest::Other);
    }

    #[test]
    fn meter_adds_distance_and_skips_gaps() {
        let mut m = PodMeter::default();
        m.advance(1.25, 1.0);
        m.advance(1.25, 1.0);
        assert!((m.total_m - 2.5).abs() < 1e-9);
        m.advance(1.25, 60.0); // the PC slept
        m.advance(0.0, 1.0);
        assert!((m.total_m - 2.5).abs() < 1e-9);
    }

    fn deck(status: MachineStatus, kmh: f32) -> OrchestratorState {
        OrchestratorState {
            last_status: status,
            hardware_speed: kmh,
            ..OrchestratorState::default()
        }
    }

    #[test]
    fn a_moving_belt_reports_speed_cadence_and_stride() {
        let s = deck(MachineStatus::Running, 4.5);
        let f = frame_for(&s);
        assert!((f.speed_mps - 1.25).abs() < 1e-4);
        assert!(f.stride_cm > 0.0);
        let expected = 1.25 * 100.0 / f.stride_cm * 60.0;
        assert!((f.cadence_spm - expected).abs() < 1e-3);
        assert!(!f.running);
        assert!(frame_for(&deck(MachineStatus::Running, 8.0)).running);
    }

    #[test]
    fn a_paused_or_stopped_belt_reports_zero_so_the_watch_can_auto_pause() {
        // The deck still reports its last speed while paused.
        assert_eq!(
            frame_for(&deck(MachineStatus::Paused, 4.5)),
            RscFrame::STILL
        );
        assert_eq!(
            frame_for(&deck(MachineStatus::Stopped, 4.5)),
            RscFrame::STILL
        );
        assert_eq!(
            frame_for(&deck(MachineStatus::Running, 0.0)),
            RscFrame::STILL
        );
    }

    #[test]
    fn every_status_has_a_sentence() {
        for s in [
            PodStatus::Off,
            PodStatus::Starting,
            PodStatus::Advertising,
            PodStatus::Connected,
        ] {
            assert!(status_message(s, None).ends_with('.'));
            assert!(!status_label(s).is_empty());
        }
        assert_eq!(
            status_message(PodStatus::Error, Some("No adapter.")),
            "No adapter."
        );
    }
}
