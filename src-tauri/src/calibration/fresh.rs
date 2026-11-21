//! The calibration points kept per treadmill (#165), and START FRESH (#164).
//!
//! Each treadmill's points are one JSON array in the settings table under
//! `persistence::points_key` (`calibrationPoints:<id>`, the id as DECK_PROFILE
//! normalizes it), read when the connection switches to that treadmill. A
//! settings key per treadmill, not a table, for the same reason as
//! DECK_PROFILE and the belt loop: the array is small and written whole.
//! Migration 9 gave the points saved before this to the treadmill saved then;
//! points saved with no treadmill known stay under the old app-wide key and
//! the first treadmill to connect adopts them here.
//!
//! START FRESH sets the treadmill's points aside (`calibrationPointsAside:<id>`)
//! and the guided walk starts from none, since a run averages into a saved
//! point at the same speed. While a set is aside, RESTORE brings it back (the
//! new points are dropped, the walk is abandoned) and KEEP NEW deletes it. One
//! set is aside at most: START FRESH waits until RESTORE or KEEP NEW settles
//! the last one, so the set put aside is always the one from before the walk.
//! None of this moves the belt: new points on a running belt keep the deck at
//! its byte (`CurveHold`) until the next speed change.

use super::{close_all, points_changed, with_belt, with_guide, with_run, with_sweep};
use crate::engine::OrchestratorState;
use crate::persistence::{SettingsStore, KEY_CALIBRATION_POINTS};
use crate::protocol::CalibrationPoint;
use tauri::AppHandle;

/// The settings key of the points START FRESH set aside for the treadmill with
/// this id.
pub fn aside_key_for(device_id: &str) -> String {
    format!("calibrationPointsAside:{device_id}")
}

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

/// The set START FRESH put aside for the treadmill with this id, if any.
pub(super) fn aside_for(
    store: &impl SettingsStore,
    device_id: &str,
) -> Option<Vec<CalibrationPoint>> {
    decode(store.get(&aside_key_for(device_id)).as_deref())
}

/// What a START FRESH, RESTORE or KEEP NEW leaves to write for one treadmill.
#[derive(Debug, Clone, PartialEq)]
pub struct FreshSave {
    device_id: String,
    /// The treadmill's points to write, if they changed.
    points: Option<Vec<CalibrationPoint>>,
    /// The set to keep aside (written), or none (the aside key is deleted).
    aside: Option<Vec<CalibrationPoint>>,
}

impl FreshSave {
    /// Write it. The old set is in at least one place at every step: a set
    /// going aside is written before the points are cleared, and a set coming
    /// back is written as the points before its aside copy is deleted.
    pub fn write(&self, store: &impl SettingsStore) -> Result<(), String> {
        let json = |p: &[CalibrationPoint]| serde_json::to_string(p).map_err(|e| e.to_string());
        let aside_key = aside_key_for(&self.device_id);
        let points_key = crate::persistence::points_key(Some(&self.device_id));
        if let Some(aside) = &self.aside {
            store.put(&aside_key, &json(aside)?)?;
        }
        if let Some(points) = &self.points {
            store.put(&points_key, &json(points)?)?;
        }
        if self.aside.is_none() {
            store.remove(&aside_key)?;
        }
        Ok(())
    }
}

/// Write `save` to the database. Run off the async runtime.
pub fn persist(app: &AppHandle, save: &FreshSave) -> Result<(), String> {
    save.write(app).map_err(|e| {
        log::warn!("could not save the calibration points: {e}");
        format!("Could not save the calibration points: {e}")
    })
}

fn treadmill() -> Result<String, String> {
    with_belt(|b| b.device_id.clone()).ok_or_else(|| "Connect the treadmill first.".to_string())
}

/// START FRESH: the treadmill's points go aside and the walk starts from none.
/// Refused while a run, RUN ALL or the guided walk is open, since each of
/// those works from the points it started with.
pub(crate) fn start_fresh(state: &mut OrchestratorState) -> Result<FreshSave, String> {
    let device_id = treadmill()?;
    let open =
        with_run(|r| r.is_some()) || with_sweep(|s| s.is_some()) || with_guide(|g| g.is_some());
    if open {
        return Err("End the open run or walk first.".into());
    }
    if with_belt(|b| b.aside.is_some()) {
        return Err("Old points are already set aside.".into());
    }
    if state.calibration_points.is_empty() {
        return Err("There are no points to set aside.".into());
    }
    let old = state.calibration_points.clone();
    state.set_calibration_points(Vec::new());
    with_belt(|b| b.aside = Some(old.clone()));
    points_changed();
    log::info!("START FRESH: {} calibration points set aside", old.len());
    Ok(FreshSave {
        device_id,
        points: Some(Vec::new()),
        aside: Some(old),
    })
}

/// RESTORE: the set aside comes back and the points measured since are
/// dropped. It abandons the new walk, so an open run, sweep or walk closes;
/// the belt keeps going.
pub(crate) fn restore(state: &mut OrchestratorState) -> Result<FreshSave, String> {
    let device_id = treadmill()?;
    let old = with_belt(|b| b.aside.take()).ok_or("No old points are set aside.")?;
    close_all();
    state.set_calibration_points(old.clone());
    points_changed();
    log::info!("RESTORE: {} calibration points back", old.len());
    Ok(FreshSave {
        device_id,
        points: Some(old),
        aside: None,
    })
}

/// KEEP NEW: the set aside is deleted and the new points stay.
pub(crate) fn keep_new() -> Result<FreshSave, String> {
    let device_id = treadmill()?;
    let old = with_belt(|b| b.aside.take()).ok_or("No old points are set aside.")?;
    log::info!("KEEP NEW: {} old calibration points deleted", old.len());
    Ok(FreshSave {
        device_id,
        points: None,
        aside: None,
    })
}

/// How many points are aside for the treadmill in use, if a set is.
pub(super) fn aside_count() -> Option<u32> {
    with_belt(|b| b.aside.as_ref().map(|p| p.len() as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A treadmill known and nothing open, with these points.
    fn ready(points: Vec<CalibrationPoint>) -> OrchestratorState {
        close_all();
        super::super::set_belt_for_test(Some(300.0));
        let mut state = OrchestratorState::default();
        state.set_calibration_points(points);
        state
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
        store
            .put(&aside_key_for("AAAA"), &json(&[pt(2.0), pt(4.0)]))
            .unwrap();

        let mut state = OrchestratorState::default();
        let points = super::super::switch_belt(&store, "AAAA").expect("a new treadmill");
        state.set_calibration_points(points);
        assert_eq!(state.calibration_points, vec![pt(3.0)]);
        assert_eq!(aside_count(), Some(2));
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
        assert_eq!(
            aside_count(),
            None,
            "the aside set stays with its treadmill"
        );
        assert!(super::super::snapshot(&state).offer, "first-connect offer");

        let points = super::super::switch_belt(&store, "AAAA").unwrap();
        assert_eq!(points, vec![pt(3.0)]);
        assert_eq!(aside_count(), Some(2));
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

    #[test]
    fn start_fresh_sets_the_points_aside_and_restore_brings_them_back() {
        let _g = super::super::TEST_GLOBALS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let store = crate::persistence::memory_db();
        let old = vec![pt(2.0), pt(4.0)];
        let mut state = ready(old.clone());
        let rev = super::super::POINTS_REV.load(std::sync::atomic::Ordering::Relaxed);

        let save = start_fresh(&mut state).unwrap();
        save.write(&store).unwrap();
        assert!(
            state.calibration_points.is_empty(),
            "the walk starts from none"
        );
        assert_eq!(aside_count(), Some(2));
        assert_eq!(
            aside_for(&store, "TEST"),
            Some(old.clone()),
            "kept, not lost"
        );
        assert!(points_for(&store, "TEST").is_empty());
        assert!(super::super::POINTS_REV.load(std::sync::atomic::Ordering::Relaxed) > rev);
        assert_eq!(
            start_fresh(&mut state).unwrap_err(),
            "Old points are already set aside."
        );

        // A new point, then the walk is abandoned: RESTORE.
        state.set_calibration_points(vec![pt(3.0)]);
        super::super::guide_start(&state, 5.0).unwrap();
        let save = restore(&mut state).unwrap();
        save.write(&store).unwrap();
        assert_eq!(state.calibration_points, old);
        assert!(with_guide(|g| g.is_none()), "the walk is abandoned");
        assert_eq!(aside_count(), None);
        assert_eq!(aside_for(&store, "TEST"), None);
        assert_eq!(points_for(&store, "TEST"), old);
        assert_eq!(
            restore(&mut state).unwrap_err(),
            "No old points are set aside."
        );
    }

    #[test]
    fn keep_new_deletes_the_old_set_and_keeps_the_new_points() {
        let _g = super::super::TEST_GLOBALS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let store = crate::persistence::memory_db();
        let mut state = ready(vec![pt(2.0)]);
        start_fresh(&mut state).unwrap().write(&store).unwrap();
        state.set_calibration_points(vec![pt(3.0)]);
        store
            .put(
                &crate::persistence::points_key(Some("TEST")),
                &json(&[pt(3.0)]),
            )
            .unwrap();
        keep_new().unwrap().write(&store).unwrap();
        assert_eq!(aside_count(), None);
        assert_eq!(aside_for(&store, "TEST"), None);
        assert_eq!(points_for(&store, "TEST"), vec![pt(3.0)]);
        assert_eq!(state.calibration_points, vec![pt(3.0)]);
        assert_eq!(keep_new().unwrap_err(), "No old points are set aside.");
    }

    #[test]
    fn start_fresh_needs_points_and_nothing_open() {
        let _g = super::super::TEST_GLOBALS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut state = ready(Vec::new());
        assert_eq!(
            start_fresh(&mut state).unwrap_err(),
            "There are no points to set aside."
        );
        let mut state = ready(vec![pt(2.0)]);
        super::super::guide_start(&state, 5.0).unwrap();
        assert_eq!(
            start_fresh(&mut state).unwrap_err(),
            "End the open run or walk first."
        );
        assert_eq!(
            state.calibration_points,
            vec![pt(2.0)],
            "nothing moved aside"
        );
        close_all();
    }
}
