//! HEART_LINK: read a heart rate from a standard Bluetooth heart rate sensor,
//! such as a Garmin watch with Broadcast Heart Rate on (roadmap #78).
//!
//! The sensor serves the Heart Rate service (0x180D) and notifies Heart Rate
//! Measurement (0x2A37). This file is the pure part: the measurement decoder,
//! the status text and the snapshot types. `heart_rate/worker.rs` owns the
//! setting, the scan and the link, which is kept apart from the treadmill's.
//!
//! Each second of a walk saves the fresh reading with its sample
//! (`current_bpm`), and the session row keeps the average and maximum.
//!
//! It only reads. Nothing here sends to the treadmill, so a heart rate can
//! never start or move the belt.

#[path = "heart_rate/worker.rs"]
mod worker;

pub use worker::{
    current_bpm, forget_sensor, init, max_heart_rate, pick_sensor, scan, set_max_heart_rate,
    snapshot, spawn, DEFAULT_MAX_BPM,
};

use serde::{Deserialize, Serialize};

/// Heart Rate service.
pub const HR_SERVICE: u16 = 0x180D;
/// Heart Rate Measurement (notify).
pub const HR_MEASUREMENT: u16 = 0x2A37;

/// Flags bit 0: the value is a u16 rather than a u8.
const FLAG_U16: u8 = 0x01;
/// Flags bits 1 and 2: sensor contact. `0b10` is "supported, no contact".
const CONTACT_MASK: u8 = 0x06;
const CONTACT_LOST: u8 = 0x04;

/// Readings outside this are noise (a u8 can say 255).
const MAX_BPM: u16 = 250;

/// The beats per minute in one Heart Rate Measurement, or `None` when the
/// packet is short, the sensor says it has no skin contact, or the value is
/// 0 or implausible.
pub fn parse_measurement(bytes: &[u8]) -> Option<u16> {
    let flags = *bytes.first()?;
    if flags & CONTACT_MASK == CONTACT_LOST {
        return None;
    }
    let bpm = if flags & FLAG_U16 != 0 {
        u16::from_le_bytes([*bytes.get(1)?, *bytes.get(2)?])
    } else {
        u16::from(*bytes.get(1)?)
    };
    (1..=MAX_BPM).contains(&bpm).then_some(bpm)
}

/// A sensor from a scan, or the one picked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HeartRateSensor {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum HeartStatus {
    /// No sensor picked (the default).
    #[default]
    Off,
    /// A sensor is picked and the link is being made.
    Searching,
    /// A sensor is picked but not broadcasting yet (a watch outside an
    /// activity); the retry links it as soon as it starts.
    Waiting,
    /// Linked and subscribed.
    Connected,
    /// A real failure, such as Bluetooth off or the adapter missing.
    Error,
}

/// The heart rate link as the windows see it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HeartRateSnapshot {
    /// The picked sensor; none means the feature is off.
    pub sensor: Option<HeartRateSensor>,
    pub status: HeartStatus,
    /// The deck-voice word for the status (`HEART_LINK // LINKED`).
    pub label: String,
    /// One plain sentence for the status.
    pub message: String,
    /// Beats per minute now; none while there is no fresh reading.
    pub bpm: Option<u32>,
    /// A scan is running.
    pub scanning: bool,
    /// What the last scan found.
    pub found: Vec<HeartRateSensor>,
    /// The maximum heart rate (bpm) that program zones are a percentage of.
    pub max_bpm: u32,
    /// False while `max_bpm` is still the default estimate.
    pub max_bpm_set: bool,
}

/// Shown while a picked sensor does not answer. A Garmin watch only
/// broadcasts during an activity or with Broadcast Heart Rate on.
pub const WAITING_MESSAGE: &str =
    "Waiting for heart rate. On a Garmin watch, start an activity or turn on Broadcast Heart Rate.";

/// The raw reason when a linked device has no heart rate measurement.
pub const NO_MEASUREMENT: &str = "no heart rate measurement";
/// The raw reason when the picked sensor is not in range.
pub const NOT_FOUND: &str = "sensor not found";

/// The step of making the link that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Find,
    Connect,
    DiscoverServices,
    Subscribe,
}

impl Step {
    /// The step's name in the log.
    pub fn name(self) -> &'static str {
        match self {
            Step::Find => "find",
            Step::Connect => "connect",
            Step::DiscoverServices => "discover services",
            Step::Subscribe => "subscribe",
        }
    }
}

/// A failed step, in plain English.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The sensor is not broadcasting yet: not a fault, and the retry links it.
    pub waiting: bool,
    /// The sentence the status shows.
    pub message: String,
    /// The log line: the step and the plain reason.
    pub log: String,
}

/// Turn a btleplug error from `step` into a [`Failure`]. A picked sensor that
/// cannot be found or connected is a watch that is not broadcasting, so that
/// is `waiting`; Bluetooth being off or missing, or a sensor that links but
/// will not share heart rate, is a real fault. Pure.
pub fn describe_failure(step: Step, raw: &str) -> Failure {
    use crate::link::LinkErrorKind as K;
    let described = crate::link::describe(raw);
    let (waiting, message, reason) = match described.kind {
        K::AdapterOff | K::AdapterMissing | K::Permission => {
            (false, described.message, "Bluetooth is not available")
        }
        _ if raw.to_lowercase().contains(NO_MEASUREMENT) => (
            false,
            "The picked device has no heart rate measurement. Pick a different sensor.".to_string(),
            "the device has no heart rate measurement",
        ),
        kind => match step {
            Step::Find if kind == K::NotFound => (
                true,
                WAITING_MESSAGE.to_string(),
                "the sensor is not broadcasting",
            ),
            Step::Find => (
                false,
                "Could not scan for the heart rate sensor. Try again.".to_string(),
                "the scan failed",
            ),
            Step::Connect => (
                true,
                WAITING_MESSAGE.to_string(),
                "the sensor did not accept the connection",
            ),
            Step::DiscoverServices => (
                false,
                "Could not read the services of the heart rate sensor. Try again.".to_string(),
                "the sensor's services could not be read",
            ),
            Step::Subscribe => (
                false,
                "Could not start readings from the heart rate sensor. Try again.".to_string(),
                "the sensor would not start sending readings",
            ),
        },
    };
    Failure {
        waiting,
        message,
        log: format!("{} failed: {reason} ({raw})", step.name()),
    }
}

/// The status word, in the deck's voice.
pub fn status_label(status: HeartStatus) -> &'static str {
    match status {
        HeartStatus::Off => "OFFLINE",
        HeartStatus::Searching => "SEEKING",
        HeartStatus::Waiting => "WAITING",
        HeartStatus::Connected => "LINKED",
        HeartStatus::Error => "FAULT",
    }
}

/// The sentence for a status; `error` is the reason when it failed, and
/// `reading` says whether a fresh reading is arriving.
pub fn status_message(status: HeartStatus, error: Option<&str>, reading: bool) -> String {
    match status {
        HeartStatus::Off => {
            "Scan for a heart rate sensor and pick your watch to show heart rate.".to_string()
        }
        HeartStatus::Searching => {
            "Looking for the sensor; on a watch, switch on Broadcast Heart Rate.".to_string()
        }
        HeartStatus::Waiting => WAITING_MESSAGE.to_string(),
        HeartStatus::Connected if reading => "Reading heart rate from the sensor.".to_string(),
        HeartStatus::Connected => "Linked to the sensor and waiting for a reading.".to_string(),
        HeartStatus::Error => error.unwrap_or("The heart rate link stopped.").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_u8_value() {
        assert_eq!(parse_measurement(&[0x00, 72]), Some(72));
    }

    #[test]
    fn reads_a_u16_value_little_endian() {
        assert_eq!(parse_measurement(&[0x01, 0x2C, 0x00]), Some(44));
        assert_eq!(parse_measurement(&[0x01, 0xB4, 0x00]), Some(180));
    }

    #[test]
    fn ignores_the_fields_after_the_value() {
        // Energy expended (bit 3) and an RR interval (bit 4) follow.
        assert_eq!(
            parse_measurement(&[0x18, 95, 0x10, 0x00, 0x20, 0x03]),
            Some(95)
        );
    }

    #[test]
    fn short_packets_read_nothing() {
        assert_eq!(parse_measurement(&[]), None);
        assert_eq!(parse_measurement(&[0x00]), None);
        assert_eq!(parse_measurement(&[0x01, 0x50]), None);
    }

    #[test]
    fn no_skin_contact_reads_nothing() {
        // Contact supported but not detected: the value is stale.
        assert_eq!(parse_measurement(&[0x04, 70]), None);
        // Contact detected, or not supported, reads.
        assert_eq!(parse_measurement(&[0x06, 70]), Some(70));
        assert_eq!(parse_measurement(&[0x02, 70]), Some(70));
    }

    #[test]
    fn zero_and_implausible_values_read_nothing() {
        assert_eq!(parse_measurement(&[0x00, 0]), None);
        assert_eq!(parse_measurement(&[0x00, 255]), None);
        assert_eq!(parse_measurement(&[0x01, 0xE8, 0x03]), None);
    }

    #[test]
    fn off_is_the_default() {
        let s = HeartRateSnapshot::default();
        assert_eq!(s.status, HeartStatus::Off);
        assert!(s.sensor.is_none() && s.bpm.is_none());
    }

    #[test]
    fn every_status_has_a_label_and_a_sentence() {
        for s in [
            HeartStatus::Off,
            HeartStatus::Searching,
            HeartStatus::Waiting,
            HeartStatus::Connected,
            HeartStatus::Error,
        ] {
            assert!(!status_label(s).is_empty());
            assert!(!status_message(s, None, true).is_empty());
        }
        assert_eq!(
            status_message(HeartStatus::Error, Some("No adapter"), false),
            "No adapter"
        );
    }

    #[test]
    fn a_sensor_that_is_not_broadcasting_is_waiting_not_a_fault() {
        let missing = describe_failure(Step::Find, NOT_FOUND);
        assert!(missing.waiting);
        assert_eq!(missing.message, WAITING_MESSAGE);
        assert!(missing.log.starts_with("find failed: "));
        for raw in [
            "Not connected",
            "Device not found",
            "timed out",
            "Other error: x",
        ] {
            let f = describe_failure(Step::Connect, raw);
            assert!(f.waiting, "{raw}");
            assert_eq!(f.message, WAITING_MESSAGE);
            assert!(f.log.starts_with("connect failed: "));
            assert!(f.log.contains(raw));
        }
        assert!(!status_label(HeartStatus::Waiting).contains("FAULT"));
    }

    #[test]
    fn bluetooth_problems_are_real_faults_at_every_step() {
        for step in [
            Step::Find,
            Step::Connect,
            Step::DiscoverServices,
            Step::Subscribe,
        ] {
            let off = describe_failure(step, "The Bluetooth radio is powered off");
            assert!(!off.waiting);
            assert_eq!(
                off.message,
                "Bluetooth is off. Turn it on in Windows settings."
            );
            let none = describe_failure(step, "No Bluetooth adapters found");
            assert!(!none.waiting);
            assert_eq!(none.message, "No Bluetooth adapter found on this PC.");
            assert!(none.log.starts_with(&format!("{} failed: ", step.name())));
        }
    }

    #[test]
    fn a_linked_sensor_that_will_not_share_heart_rate_is_a_fault() {
        let services = describe_failure(Step::DiscoverServices, "Other error: gatt");
        assert!(!services.waiting);
        assert!(services.message.contains("services"));
        assert!(services.log.starts_with("discover services failed: "));
        let sub = describe_failure(Step::Subscribe, "Other error: gatt");
        assert!(!sub.waiting);
        assert!(sub.log.starts_with("subscribe failed: "));
        let none = describe_failure(Step::DiscoverServices, NO_MEASUREMENT);
        assert!(!none.waiting);
        assert!(none.message.contains("no heart rate measurement"));
        let scan = describe_failure(Step::Find, "Other error: scan");
        assert!(!scan.waiting);
    }

    #[test]
    fn messages_are_short_and_have_no_dashes() {
        let msgs = [
            WAITING_MESSAGE.to_string(),
            describe_failure(Step::Find, "Other error: x").message,
            describe_failure(Step::DiscoverServices, "x").message,
            describe_failure(Step::Subscribe, "x").message,
            describe_failure(Step::Subscribe, NO_MEASUREMENT).message,
        ];
        for m in msgs {
            assert!(!m.contains('\u{2014}') && !m.contains('\u{2013}'), "{m}");
            assert!(m.len() < 120, "{m}");
        }
    }
}
