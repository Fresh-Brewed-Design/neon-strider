//! WRIST_LINK: a second Bluetooth service the PC serves next to GHOST_POD's
//! foot pod, for a Connect IQ data field on the watch (roadmap #55 and #56;
//! the plan is in the roadmap issue #82).
//!
//! Phase 0 only: the State characteristic (read and notify). The watch reads
//! the belt from it; nothing the watch sends is acted on yet, so nothing here
//! can move the belt. This file is the pure part: the UUIDs, the State byte
//! layout, the status text. `ghost_pod/worker.rs` owns the switch and the tick
//! that sends State; the GATT server is in `platform/`.

use crate::engine::{OrchestratorState, STILL_KMH};
use crate::protocol::MachineStatus;
use serde::{Deserialize, Serialize};

/// The WRIST_LINK service. Little-endian bytes, like the foot pod's.
pub const SERVICE: u128 = 0x00b165d6_40c2_4f94_8df9_0f69431f21af;
/// State (read and notify, PC to watch). The other two UUIDs come with the
/// phases that use them.
pub const STATE: u128 = 0x00b165d6_40c2_4f94_8df9_0f69431f21b0;

/// The beacon's manufacturer data: company 0xFFFF, which the Bluetooth SIG
/// keeps for tests and devices with no company ID, so it belongs to nobody.
/// Windows' advertisement publisher only takes manufacturer data (service
/// data and UUID lists are reserved to the system), so this is the form left.
pub const BEACON_COMPANY: u16 = 0xffff;
/// After the company ID: "NSTR" plus a version byte. Other 0xFFFF beacons are
/// test gear, and four letters make a clash with one of them unlikely.
/// Keep in step with `watch/source/BeltLink.mc`.
pub const BEACON_MARKER: [u8; 5] = [b'N', b'S', b'T', b'R', 1];

/// `belt` byte of State.
pub const BELT_STILL: u8 = 0;
pub const BELT_MOVING: u8 = 1;
pub const BELT_PAUSED: u8 = 2;

/// `flags` bit 0 of State: the app holds its automatic start (no press yet on
/// this connection).
pub const FLAG_HOLD: u8 = 0b01;

/// State is seven bytes: `[belt u8][speed_kmh_x100 u16][incline u8][flags u8]
/// [last_seq u8][result u8]`; the last two stay zero until commands exist.
pub const STATE_LEN: usize = 7;

/// What the watch reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WristState {
    pub belt: u8,
    pub speed_kmh_x100: u16,
    pub incline: u8,
    pub flags: u8,
    pub last_seq: u8,
    pub result: u8,
}

impl WristState {
    pub fn encode(&self) -> Vec<u8> {
        let speed = self.speed_kmh_x100.to_le_bytes();
        vec![
            self.belt,
            speed[0],
            speed[1],
            self.incline,
            self.flags,
            self.last_seq,
            self.result,
        ]
    }
}

/// The deck right now as the watch should see it. A moving belt reads its
/// real speed (calibrated, like the pod); paused and still read 0 km/h.
pub fn state_for(state: &OrchestratorState) -> WristState {
    let kmh = state.to_physical_speed(state.hardware_speed);
    let belt = match state.last_status {
        MachineStatus::Paused => BELT_PAUSED,
        s if s.is_moving() && kmh > STILL_KMH => BELT_MOVING,
        _ => BELT_STILL,
    };
    let speed = if belt == BELT_MOVING {
        (kmh * 100.0).round().clamp(0.0, u16::MAX as f32) as u16
    } else {
        0
    };
    WristState {
        belt,
        speed_kmh_x100: speed,
        incline: state.hardware_incline,
        flags: if state.hold_auto_start { FLAG_HOLD } else { 0 },
        last_seq: 0,
        result: 0,
    }
}

/// WRIST_LINK as the windows see it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct WristLinkSnapshot {
    /// The Labs switch (off by default).
    pub enabled: bool,
    /// The service is up (served next to the foot pod).
    pub serving: bool,
    /// Watches subscribed to the state.
    pub watches: u32,
    /// One plain sentence for the link.
    pub message: String,
}

/// The sentence for the link.
pub fn status_message(
    enabled: bool,
    pod_on: bool,
    serving: bool,
    watches: u32,
    error: Option<&str>,
) -> String {
    if !enabled {
        return "Switch WRIST_LINK on to let the watch's data field read the belt.".to_string();
    }
    if !pod_on {
        return "Waiting for GHOST_POD: the watch keeps its foot pod link while the field reads the belt."
            .to_string();
    }
    if let Some(why) = error {
        return why.to_string();
    }
    if !serving {
        return "Starting the link.".to_string();
    }
    if watches > 0 {
        "The watch's data field is connected and reads the belt.".to_string()
    } else {
        "Waiting for the watch's data field to connect.".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(status: MachineStatus, kmh: f32) -> OrchestratorState {
        OrchestratorState {
            last_status: status,
            hardware_speed: kmh,
            ..OrchestratorState::default()
        }
    }

    #[test]
    fn the_uuids_are_the_ones_in_the_plan() {
        assert_eq!(uuid_text(SERVICE), "00b165d6-40c2-4f94-8df9-0f69431f21af");
    }

    fn uuid_text(u: u128) -> String {
        let h = format!("{u:032x}");
        format!(
            "{}-{}-{}-{}-{}",
            &h[0..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..32]
        )
    }

    #[test]
    fn state_bytes_are_little_endian() {
        let s = WristState {
            belt: BELT_MOVING,
            speed_kmh_x100: 0x0272, // 6.26 km/h
            incline: 3,
            flags: FLAG_HOLD,
            last_seq: 0,
            result: 0,
        };
        assert_eq!(s.encode(), vec![1, 0x72, 0x02, 3, 1, 0, 0]);
        assert_eq!(s.encode().len(), STATE_LEN);
    }

    #[test]
    fn a_moving_belt_reports_its_speed() {
        let s = state_for(&deck(MachineStatus::Running, 4.5));
        assert_eq!(s.belt, BELT_MOVING);
        assert_eq!(s.speed_kmh_x100, 450);
    }

    #[test]
    fn a_paused_or_still_belt_reads_zero() {
        let paused = state_for(&deck(MachineStatus::Paused, 4.5));
        assert_eq!((paused.belt, paused.speed_kmh_x100), (BELT_PAUSED, 0));
        let stopped = state_for(&deck(MachineStatus::Stopped, 4.5));
        assert_eq!((stopped.belt, stopped.speed_kmh_x100), (BELT_STILL, 0));
        let zero = state_for(&deck(MachineStatus::Running, 0.0));
        assert_eq!(zero.belt, BELT_STILL);
    }

    #[test]
    fn the_hold_shows_in_the_flags() {
        let mut d = deck(MachineStatus::Stopped, 0.0);
        d.hold_auto_start = true;
        assert_eq!(state_for(&d).flags, FLAG_HOLD);
        d.hold_auto_start = false;
        assert_eq!(state_for(&d).flags, 0);
    }

    #[test]
    fn the_message_follows_the_link() {
        assert!(status_message(false, true, false, 0, None).starts_with("Switch WRIST_LINK on"));
        assert!(status_message(true, false, false, 0, None).starts_with("Waiting for GHOST_POD"));
        assert_eq!(status_message(true, true, true, 0, Some("boom")), "boom");
        assert!(status_message(true, true, true, 1, None).contains("connected"));
        assert!(status_message(true, true, true, 0, None).starts_with("Waiting for the watch"));
    }
}
