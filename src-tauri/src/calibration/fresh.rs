//! The calibration points kept per treadmill (#165).
//!
//! Each treadmill's points are one JSON array in the settings table under
//! `persistence::points_key` (`calibrationPoints:<id>`, the id as DECK_PROFILE
//! normalizes it), read when the connection switches to that treadmill. A
//! settings key per treadmill, not a table, for the same reason as
//! DECK_PROFILE and the belt loop: the array is small and written whole.
//! Migration 9 gave the points saved before this to the treadmill saved then;
//! points saved with no treadmill known stay under the old app-wide key and
//! the first treadmill to connect adopts them here.

use crate::persistence::{SettingsStore, KEY_CALIBRATION_POINTS};
use crate::protocol::CalibrationPoint;

/// Stored points, if the value is a readable JSON array.
fn decode(raw: Option<&str>) -> Option<Vec<CalibrationPoint>> {
    match serde_json::from_str(raw?) {
        Ok(points) => Some(points),
        Err(e) => {
            log::warn!("could not read stored calibration points: {e}");
            None
        }
    }
}

/// The points stored for the treadmill with this id. A treadmill with none
/// of its own adopts the old app-wide points (saved before any treadmill was
/// known), which then move to its key; an empty old value is left alone, so a
/// later treadmill isn't handed nothing as if it were a calibration.
pub(super) fn points_for(store: &impl SettingsStore, device_id: &str) -> Vec<CalibrationPoint> {
    let key = crate::persistence::points_key(Some(device_id));
    if let Some(raw) = store.get(&key) {
        return decode(Some(&raw)).unwrap_or_default();
    }
    let Some(raw) = store.get(KEY_CALIBRATION_POINTS) else {
        return Vec::new();
    };
    let points = decode(Some(&raw)).unwrap_or_default();
    if points.is_empty() {
        return points;
    }
    match store
        .put(&key, &raw)
        .and_then(|_| store.remove(KEY_CALIBRATION_POINTS))
    {
        Ok(()) => log::info!("Calibration points from before any treadmill moved to this one"),
        Err(e) => log::warn!("could not move the calibration points to this treadmill: {e}"),
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::OrchestratorState;

    fn pt(speed: f32) -> CalibrationPoint {
        CalibrationPoint {
            speed,
            stride_length_cm: 70.0,
            distance_scale: 0.9,
            runs: 1,
            stride_runs: 0,
        }
    }

    fn json(points: &[CalibrationPoint]) -> String {
        serde_json::to_string(points).unwrap()
    }

    #[test]
    fn each_treadmill_reads_its_own_points_and_a_new_one_has_none() {
        let store = crate::persistence::memory_db();
        store
            .put(
                &crate::persistence::points_key(Some("AAAA")),
                &json(&[pt(3.0)]),
            )
            .unwrap();
        assert_eq!(points_for(&store, "AAAA"), vec![pt(3.0)]);
        assert!(points_for(&store, "BBBB").is_empty());
    }

    #[test]
    fn a_second_treadmill_reads_as_never_calibrated_and_gets_the_offer() {
        let _g = super::super::TEST_GLOBALS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let store = crate::persistence::memory_db();
        store
            .put(
                &crate::persistence::points_key(Some("AAAA")),
                &json(&[pt(3.0)]),
            )
            .unwrap();
        store
            .put(&super::super::belt_key_for("AAAA"), "280")
            .unwrap();

        let mut state = OrchestratorState::default();
        let points = super::super::switch_belt(&store, "AAAA").expect("a new treadmill");
        state.set_calibration_points(points);
        assert_eq!(state.calibration_points, vec![pt(3.0)]);
        assert_eq!(super::super::treadmill_id().as_deref(), Some("AAAA"));
        assert!(!super::super::snapshot(&state).offer);
        assert_eq!(
            super::super::switch_belt(&store, "AAAA"),
            None,
            "same treadmill"
        );

        let points = super::super::switch_belt(&store, "BBBB").expect("a new treadmill");
        state.set_calibration_points(points);
        assert!(state.calibration_points.is_empty());
        assert!(super::super::snapshot(&state).offer, "first-connect offer");

        let points = super::super::switch_belt(&store, "AAAA").unwrap();
        assert_eq!(points, vec![pt(3.0)]);
        super::super::set_belt_for_test(None);
    }

    #[test]
    fn points_saved_before_any_treadmill_go_to_the_first_one() {
        let store = crate::persistence::memory_db();
        store
            .put(KEY_CALIBRATION_POINTS, &json(&[pt(3.0)]))
            .unwrap();
        assert_eq!(points_for(&store, "AAAA"), vec![pt(3.0)]);
        assert_eq!(store.get(KEY_CALIBRATION_POINTS), None);
        assert!(
            points_for(&store, "BBBB").is_empty(),
            "the second gets none"
        );
        assert_eq!(points_for(&store, "AAAA"), vec![pt(3.0)]);
    }

    #[test]
    fn an_empty_old_value_is_not_adopted() {
        let store = crate::persistence::memory_db();
        store.put(KEY_CALIBRATION_POINTS, "[]").unwrap();
        assert!(points_for(&store, "AAAA").is_empty());
        assert_eq!(
            store.get(&crate::persistence::points_key(Some("AAAA"))),
            None
        );
    }
}
