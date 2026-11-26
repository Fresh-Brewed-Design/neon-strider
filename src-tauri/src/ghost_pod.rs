//! GHOST_POD: the PC poses as a Bluetooth foot pod so a watch can read the
//! belt (roadmap #55; research in docs/watch-pod.md).
//!
//! Garmin watches pair natively with a Running Speed and Cadence (RSC) foot
//! pod, not with FTMS, so the pod serves the RSC service (0x1814). This file is
//! the pure part: the RSC byte layouts, the SC control point, what to report
//! for the deck right now, and the status text. `ghost_pod/worker.rs` owns the
//! setting and the 1 s tick; the GATT server itself is in `platform/`.
//!
//! With its power switch on, the pod also serves the Cycling Power service
//! (0x1818) the way a Stryd pod does, so the watch gets running power: watts
//! from METABOLIC_BURN's estimate (speed, incline, weight; roadmap #77).
//!
//! It only reports. RSC has no message from the watch for pause or resume, so
//! nothing here ever moves the belt. Pausing the belt makes the pod report
//! 0 km/h, which lets the watch's Auto Pause stop its timer.

#[path = "ghost_pod/worker.rs"]
mod worker;

pub use worker::{init, set_enabled, set_power, snapshot, spawn};

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

/// Cycling Power service: what Stryd serves for running power.
pub const CP_SERVICE: u16 = 0x1818;
/// Cycling Power Measurement (notify).
pub const CP_MEASUREMENT: u16 = 0x2A63;
/// Cycling Power Feature (read).
pub const CP_FEATURE: u16 = 0x2A65;
/// Sensor Location (read), required by the Cycling Power service.
pub const SENSOR_LOCATION: u16 = 0x2A5D;
/// Sensor Location value: top of shoe, where a foot pod sits.
const LOCATION_TOP_OF_SHOE: u8 = 0x01;

/// Share of the metabolic rate a body turns into work (muscle efficiency of
/// about 25 %), so the watts land near what a running power pod reports.
const MUSCLE_EFFICIENCY: f64 = 0.25;
/// Watts in one kcal per minute (4184 J / 60 s).
const WATTS_PER_KCAL_MIN: f64 = 4184.0 / 60.0;

/// RSC Measurement flag bits.
const FLAG_STRIDE: u8 = 0x01;
const FLAG_DISTANCE: u8 = 0x02;
const FLAG_RUNNING: u8 = 0x04;

/// RSC Feature bits: stride length, total distance, walking or running status.
pub const FEATURES: u16 = 0x0007;

/// At or above this the pod says "running" rather than "walking".
/// Around 7 km/h is where people switch from walking to running.
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

/// The most a write may set the pod's total distance to: 100 000 km. The RSC
/// field itself would allow 429 496 km (u32 of 0.1 m), far beyond any real
/// odometer, and the control point has no pairing (the watch connects without
/// encryption), so any nearby central can write it. Larger values are capped
/// here, never passed on.
pub const MAX_TOTAL_M: f64 = 100_000_000.0;

/// How many bytes of a control point write the log shows (the length is
/// always logged), so a stranger's long write cannot flood the log file.
const LOG_BYTES: usize = 16;

/// Log text for a control point write: at most the first 16 bytes as hex,
/// then the total length.
pub fn write_log_text(value: &[u8]) -> String {
    let shown = &value[..value.len().min(LOG_BYTES)];
    let more = if value.len() > LOG_BYTES { "..." } else { "" };
    format!(
        "{}{} ({} bytes)",
        crate::traffic::to_hex(shown),
        more,
        value.len()
    )
}

/// Lets one write log line through per interval and counts the rest, so a
/// central writing in a loop cannot fill the log (an info line per write
/// would grow it without bound).
#[derive(Debug, Default)]
pub struct WriteLogLimiter {
    last: Option<std::time::Instant>,
    skipped: u32,
}

impl WriteLogLimiter {
    pub const INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

    /// `Some(skipped since the last logged write)` when this write may be
    /// logged, `None` when it is held back.
    pub fn check(&mut self, now: std::time::Instant) -> Option<u32> {
        if self
            .last
            .is_some_and(|t| now.saturating_duration_since(t) < Self::INTERVAL)
        {
            self.skipped = self.skipped.saturating_add(1);
            return None;
        }
        self.last = Some(now);
        Some(std::mem::take(&mut self.skipped))
    }
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
            ControlRequest::SetTotal((u32::from_le_bytes(b) as f64 / 10.0).min(MAX_TOTAL_M)),
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
        // The engine's stall limit too (`engine::MAX_PHYSICS_STEP_S`).
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

/// The Cycling Power Feature value (u32): no optional features, so the
/// measurement is the instantaneous power alone.
pub fn power_feature_value() -> Vec<u8> {
    0u32.to_le_bytes().to_vec()
}

/// The Sensor Location value (u8).
pub fn sensor_location_value() -> Vec<u8> {
    vec![LOCATION_TOP_OF_SHOE]
}

/// Encode a Cycling Power Measurement: flags (u16, none set), instantaneous
/// power (s16, watts), little-endian (Cycling Power Service 1.1).
pub fn encode_power(watts: u16) -> Vec<u8> {
    let w = watts.min(i16::MAX as u16) as i16;
    let mut out = vec![0, 0];
    out.extend_from_slice(&w.to_le_bytes());
    out
}

/// Running power for a calibrated speed and incline: METABOLIC_BURN's rate
/// (ACSM walking equation) in watts, times the muscle efficiency. 0 when the
/// speed or weight isn't positive.
pub fn watts_for(speed_kmh: f32, grade_pct: f32, body_mass_kg: f32) -> u16 {
    let kcal_min = crate::engine::walking_kcal_per_min(
        speed_kmh as f64,
        grade_pct as f64,
        body_mass_kg as f64,
    );
    clamp_round(
        kcal_min * WATTS_PER_KCAL_MIN * MUSCLE_EFFICIENCY,
        i16::MAX as f64,
    ) as u16
}

/// The deck right now as watts. A paused, stopped or still belt sends 0 W.
pub fn power_for(state: &OrchestratorState) -> u16 {
    let kmh = state.to_physical_speed(state.hardware_speed);
    if !(state.last_status.is_moving() && kmh > STILL_KMH) {
        return 0;
    }
    watts_for(
        kmh,
        state.incline_pct(state.hardware_incline),
        state.body_mass_kg,
    )
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
    /// The power switch (off by default): the pod also serves running power.
    pub power_enabled: bool,
    /// Watches subscribed to the power measurements.
    pub power_watches: u32,
    /// The watts being sent now (0 while the belt is still or power is off).
    pub power_w: u16,
}

/// The status word, in the deck's voice.
pub fn status_label(status: PodStatus) -> &'static str {
    match status {
        PodStatus::Off => "NO_CARRIER",
        PodStatus::Starting => "BOOTING",
        PodStatus::Advertising => "BROADCASTING",
        PodStatus::Connected => "LINKED",
        PodStatus::Error => "FAULT",
    }
}

/// The sentence for a status; `error` is the reason when it failed.
pub fn status_message(status: PodStatus, error: Option<&str>) -> String {
    match status {
        PodStatus::Off => "Switch GHOST_POD on to let a watch read the belt as a foot pod.".to_string(),
        PodStatus::Starting => "Starting the pod.".to_string(),
        PodStatus::Advertising => {
            "Waiting for a watch. Add a foot pod sensor on the watch; GHOST_POD shows under the PC's Bluetooth name."
                .to_string()
        }
        PodStatus::Connected => {
            "A watch is connected and gets the belt speed every second.".to_string()
        }
        PodStatus::Error => error.unwrap_or("GHOST_POD stopped.").to_string(),
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
    fn control_point_caps_the_total_and_refuses_long_writes() {
        let (req, resp) = control_point(&[0x01, 0xff, 0xff, 0xff, 0xff]);
        assert_eq!(req, ControlRequest::SetTotal(MAX_TOTAL_M));
        assert_eq!(resp, vec![0x10, 0x01, 0x01]);
        // Oversized writes change nothing.
        let long = [0x01u8; 600];
        let (req, resp) = control_point(&long);
        assert_eq!(req, ControlRequest::Other);
        assert_eq!(resp, vec![0x10, 0x01, 0x03]);
        assert_eq!(
            control_point(&[0x01, 0, 0, 0, 0, 0]).0,
            ControlRequest::Other
        );
    }

    #[test]
    fn write_log_text_is_short() {
        assert_eq!(write_log_text(&[0x01, 0xab]), "01ab (2 bytes)");
        let t = write_log_text(&[0xee; 600]);
        assert_eq!(t, format!("{}... (600 bytes)", "ee".repeat(16)));
    }

    #[test]
    fn write_log_limiter_lets_one_per_interval_through() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut l = WriteLogLimiter::default();
        assert_eq!(l.check(t0), Some(0));
        assert_eq!(l.check(t0 + Duration::from_secs(1)), None);
        assert_eq!(l.check(t0 + Duration::from_secs(9)), None);
        assert_eq!(l.check(t0 + Duration::from_secs(10)), Some(2));
        assert_eq!(l.check(t0 + Duration::from_secs(11)), None);
        assert_eq!(l.check(t0 + Duration::from_secs(20)), Some(1));
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
    fn power_measurement_matches_the_spec_layout() {
        // 135 W = 0x0087; flags 0.
        assert_eq!(encode_power(135), vec![0x00, 0x00, 0x87, 0x00]);
        assert_eq!(encode_power(0), vec![0, 0, 0, 0]);
        // Saturates at the largest s16 instead of going negative.
        assert_eq!(encode_power(u16::MAX), vec![0, 0, 0xff, 0x7f]);
        assert_eq!(power_feature_value(), vec![0, 0, 0, 0]);
        assert_eq!(sensor_location_value(), vec![0x01]);
    }

    #[test]
    fn watts_follow_metabolic_burn_and_rise_with_the_incline() {
        // 70 kg at 4 km/h: VO2 10.1667 mL/kg/min flat, 22.1667 at 10 %
        // (engine.rs's worked example). L/min * 5 kcal * 4184/60 * 0.25.
        let flat = watts_for(4.0, 0.0, 70.0);
        let hill = watts_for(4.0, 10.0, 70.0);
        assert_eq!(flat, 62); // 10.1667 * 0.07 * 5 * 69.733 * 0.25 = 62.0
        assert_eq!(hill, 135); // 22.1667 * 0.07 * 5 * 69.733 * 0.25 = 135.3
                               // Heavier means more watts; a downhill counts as level.
        assert!(watts_for(4.0, 0.0, 90.0) > flat);
        assert_eq!(watts_for(4.0, -5.0, 70.0), flat);
        // Nothing to estimate from.
        assert_eq!(watts_for(0.0, 10.0, 70.0), 0);
        assert_eq!(watts_for(4.0, 10.0, 0.0), 0);
        assert_eq!(watts_for(f32::NAN, 0.0, 70.0), 0);
    }

    #[test]
    fn a_paused_or_stopped_belt_sends_zero_watts() {
        let mut moving = deck(MachineStatus::Running, 4.0);
        moving.body_mass_kg = 70.0;
        assert!(power_for(&moving) > 0);
        for status in [MachineStatus::Paused, MachineStatus::Stopped] {
            let mut s = deck(status, 4.0);
            s.body_mass_kg = 70.0;
            assert_eq!(power_for(&s), 0);
        }
        assert_eq!(power_for(&deck(MachineStatus::Running, 0.0)), 0);
    }

    #[test]
    fn power_follows_the_deck_incline() {
        let mut s = deck(MachineStatus::Running, 4.0);
        s.body_mass_kg = 70.0;
        let flat = power_for(&s);
        s.hardware_incline = 10;
        let pct = s.incline_pct(10);
        assert!(pct > 0.0);
        assert_eq!(power_for(&s), watts_for(4.0, pct, 70.0));
        assert!(power_for(&s) > flat);
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
