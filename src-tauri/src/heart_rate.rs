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
//! The PULSE_RATE spark chart reads `history` (`heart_rate/history.rs`): a
//! rolling 10 minutes of readings, and `zone`, the running step's heart rate
//! zone in bpm.
//!
//! It only reads. Nothing here sends to the treadmill, so a heart rate can
//! never start or move the belt.

#[path = "heart_rate/history.rs"]
mod history;
#[path = "heart_rate/worker.rs"]
mod worker;

pub use worker::{
    current_bpm, forget_sensor, init, max_heart_rate, pick_sensor, scan, set_max_heart_rate,
    set_zone_incline_trim, snapshot, spawn, zone_incline_trim, DEFAULT_MAX_BPM, MAX_MAX_BPM,
    MIN_MAX_BPM,
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
/// Above any measured human maximum; 251 to 255 is a sensor glitch.
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
    /// The PULSE_RATE chart: the last 10 minutes, one point per 5 s, oldest
    /// first, always a full window; `None` is a step with no reading. Empty
    /// while no sensor is picked, so there is no chart.
    pub history: Vec<Option<u32>>,
    /// The running program step's heart rate zone, for the chart's band;
    /// none without a walking step that has a zone.
    pub zone: Option<ZoneBand>,
    /// The running cool-down step's pulse target (#233); none without a
    /// walking step that ends on heart rate.
    pub cool_down: Option<PulseTarget>,
    /// A scan is running.
    pub scanning: bool,
    /// What the last scan found.
    pub found: Vec<HeartRateSensor>,
    /// The maximum heart rate (bpm) that program zones are a percentage of.
    pub max_bpm: u32,
    /// False while `max_bpm` is still the default estimate.
    pub max_bpm_set: bool,
    /// Labs, ZONE_TILT (off by default): program zones may trim incline as
    /// each step asks, not just speed.
    pub incline_trim: bool,
}

/// A heart rate zone in beats per minute, and where the pulse sits in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ZoneBand {
    pub low_bpm: u32,
    pub high_bpm: u32,
    /// Under, in or over the zone; none without a fresh reading.
    pub position: Option<ZonePosition>,
    /// The deck-voice word for the position (`ZONE // LOCKED`).
    pub label: String,
    /// One plain sentence for the position.
    pub message: String,
    /// Seconds the step still waits for the pulse to settle before the zone
    /// trims the belt (`programs::ZONE_SETTLE`); none once it trims.
    pub settle_s: Option<u32>,
}

impl ZoneBand {
    /// The band while the step waits `left_s` more seconds for the pulse to
    /// settle: a reading shows `ZONE // SETTLING` (its position still
    /// colours it), and no reading stays `ZONE // WAIT`.
    pub fn settling(mut self, left_s: u32) -> Self {
        self.settle_s = Some(left_s);
        if self.position.is_some() {
            self.label = "ZONE // SETTLING".to_string();
            self.message = "Heart rate is settling before the zone trims.".to_string();
        }
        self
    }
}

/// Where the pulse is against a heart rate zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum ZonePosition {
    Under,
    In,
    Over,
}

/// The zone `low`..=`high` (bpm, as the program's trim computes them) with
/// the reading `bpm` placed against it. The same rule as the trim: above
/// `high` is over, below `low` is under, so the HUD and the belt agree. The
/// whole bpm shown are the first and last readings that count as in (low
/// rounded up, high rounded down), so the range on screen matches. Pure.
pub fn zone_band(low: f32, high: f32, bpm: Option<u32>) -> ZoneBand {
    let position = bpm.map(|b| {
        let b = b as f32;
        if b > high {
            ZonePosition::Over
        } else if b < low {
            ZonePosition::Under
        } else {
            ZonePosition::In
        }
    });
    let (label, message) = match position {
        Some(ZonePosition::Under) => ("ZONE // UNDER", "Heart rate is below the zone."),
        Some(ZonePosition::In) => ("ZONE // LOCKED", "Heart rate is in the zone."),
        Some(ZonePosition::Over) => ("ZONE // OVER", "Heart rate is above the zone."),
        None => ("ZONE // WAIT", "Waiting for a heart rate reading."),
    };
    ZoneBand {
        low_bpm: low.ceil() as u32,
        high_bpm: high.floor() as u32,
        position,
        label: label.to_string(),
        message: message.to_string(),
        settle_s: None,
    }
}

/// A cool-down step's pulse target in beats per minute, and where the pulse
/// sits against it (#233).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PulseTarget {
    /// The step ends once the pulse stays at or under this.
    pub target_bpm: u32,
    /// The reading is at or under the target; none without a fresh reading.
    pub down: Option<bool>,
    /// The deck-voice word for where the pulse is (`PULSE // HIGH`).
    pub label: String,
    /// One plain sentence for it.
    pub message: String,
    /// Seconds the pulse must still stay down (`programs::PULSE_END_HOLD`);
    /// none once it has, or while it isn't down.
    pub hold_s: Option<u32>,
}

/// The cool-down target `target` (whole bpm) with the reading `bpm` placed
/// against it: at or under is down, the same rule that ends the step.
/// `hold_s` is how long the pulse must still stay down, as the program
/// counts it. Pure.
pub fn pulse_target(target: u32, bpm: Option<u32>, hold_s: Option<u32>) -> PulseTarget {
    let down = bpm.map(|b| b <= target);
    let (label, message, hold_s) = match (down, hold_s) {
        (None, _) => (
            "PULSE // WAIT",
            "Without a heart rate reading the cool-down ends on its minutes.",
            None,
        ),
        (Some(false), _) => (
            "PULSE // HIGH",
            "The cool-down walks on until heart rate drops to the target.",
            None,
        ),
        (Some(true), Some(left)) => (
            "PULSE // HOLDING",
            "Heart rate is at the target and needs to stay there for 30 s.",
            Some(left),
        ),
        (Some(true), None) => (
            "PULSE // CLEAR",
            "Heart rate is down, so the cool-down ends on its minutes.",
            None,
        ),
    };
    PulseTarget {
        target_bpm: target,
        down,
        label: label.to_string(),
        message: message.to_string(),
        hold_s,
    }
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
            "Looking for the heart rate sensor. On a watch, switch on Broadcast Heart Rate."
                .to_string()
        }
        HeartStatus::Waiting => WAITING_MESSAGE.to_string(),
        HeartStatus::Connected if reading => "Reading heart rate from the sensor.".to_string(),
        HeartStatus::Connected => {
            "Linked to the heart rate sensor, waiting for a reading.".to_string()
        }
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
    fn a_reading_sits_under_in_or_over_the_zone() {
        let band = |bpm| zone_band(133.0, 152.0, bpm);
        let under = band(Some(132));
        assert_eq!(under.position, Some(ZonePosition::Under));
        assert_eq!(under.label, "ZONE // UNDER");
        // The bounds themselves are in the zone.
        assert_eq!(band(Some(133)).position, Some(ZonePosition::In));
        assert_eq!(band(Some(152)).position, Some(ZonePosition::In));
        assert_eq!(band(Some(142)).label, "ZONE // LOCKED");
        let over = band(Some(153));
        assert_eq!(over.position, Some(ZonePosition::Over));
        assert_eq!(over.label, "ZONE // OVER");
        // No reading: the zone still shows, with no position.
        let none = band(None);
        assert_eq!(none.position, None);
        assert_eq!((none.low_bpm, none.high_bpm), (133, 152));
        assert_eq!(none.label, "ZONE // WAIT");
    }

    #[test]
    fn the_range_shown_is_the_readings_that_count_as_in() {
        // 85 to 95 % of 190 is 161.5 to 180.5: the screen says 162 to 180,
        // and 161 and 181 are outside it.
        let b = zone_band(161.5, 180.5, Some(161));
        assert_eq!(b.position, Some(ZonePosition::Under));
        assert_eq!((b.low_bpm, b.high_bpm), (162, 180));
        assert_eq!(
            zone_band(161.5, 180.5, Some(162)).position,
            Some(ZonePosition::In)
        );
        assert_eq!(
            zone_band(161.5, 180.5, Some(180)).position,
            Some(ZonePosition::In)
        );
        assert_eq!(
            zone_band(161.5, 180.5, Some(181)).position,
            Some(ZonePosition::Over)
        );
    }

    #[test]
    fn messages_are_short_and_have_no_dashes() {
        let msgs = [
            WAITING_MESSAGE.to_string(),
            describe_failure(Step::Find, "Other error: x").message,
            describe_failure(Step::DiscoverServices, "x").message,
            describe_failure(Step::Subscribe, "x").message,
            describe_failure(Step::Subscribe, NO_MEASUREMENT).message,
            zone_band(130.0, 150.0, Some(120)).message,
            zone_band(130.0, 150.0, Some(140)).message,
            zone_band(130.0, 150.0, Some(160)).message,
            zone_band(130.0, 150.0, None).message,
            zone_band(130.0, 150.0, Some(140)).settling(90).message,
        ];
        for m in msgs {
            assert!(!m.contains('\u{2014}') && !m.contains('\u{2013}'), "{m}");
            assert!(m.len() < 120, "{m}");
        }
    }

    #[test]
    fn a_settling_zone_says_so_and_keeps_its_position() {
        let b = zone_band(130.0, 150.0, Some(120)).settling(75);
        assert_eq!(b.settle_s, Some(75));
        assert_eq!(b.position, Some(ZonePosition::Under));
        assert_eq!(b.label, "ZONE // SETTLING");
        // Without a reading it still waits for one.
        let b = zone_band(130.0, 150.0, None).settling(75);
        assert_eq!((b.label.as_str(), b.settle_s), ("ZONE // WAIT", Some(75)));
        assert_eq!(zone_band(130.0, 150.0, Some(140)).settle_s, None);
    }

    #[test]
    fn a_cool_down_target_places_the_pulse_at_or_under_it() {
        let wait = pulse_target(114, None, Some(30));
        assert_eq!(
            (wait.down, wait.label.as_str(), wait.hold_s),
            (None, "PULSE // WAIT", None)
        );
        let high = pulse_target(114, Some(115), Some(30));
        assert_eq!(
            (high.down, high.label.as_str()),
            (Some(false), "PULSE // HIGH")
        );
        // At the target counts as down, so the number shown is the one to reach.
        let holding = pulse_target(114, Some(114), Some(12));
        assert_eq!(
            (holding.down, holding.label.as_str(), holding.hold_s),
            (Some(true), "PULSE // HOLDING", Some(12))
        );
        let clear = pulse_target(114, Some(100), None);
        assert_eq!(
            (clear.down, clear.label.as_str()),
            (Some(true), "PULSE // CLEAR")
        );
    }
}
