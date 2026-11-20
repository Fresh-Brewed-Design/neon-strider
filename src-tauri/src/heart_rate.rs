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
    /// Linked and subscribed.
    Connected,
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
}

/// The status word, in the deck's voice.
pub fn status_label(status: HeartStatus) -> &'static str {
    match status {
        HeartStatus::Off => "OFFLINE",
        HeartStatus::Searching => "SEEKING",
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
}
