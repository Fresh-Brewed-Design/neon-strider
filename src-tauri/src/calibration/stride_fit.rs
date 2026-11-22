//! STRIDE_FIT: the fastest speed (calibrated km/h) at which the user's
//! walking stride still fits the treadmill's deck. Set by feel in System,
//! Calibration, kept per treadmill under the same Bluetooth identity as the
//! belt loop (`stride_fit_key_for`), and unset until the user sets it.
//!
//! No sourced rule ties deck length to stride (#223: only industry opinion
//! such as "50 to 55 inch deck for walking"), so nothing here computes the
//! speed from GROUND_TRUTH's belt loop or strides. The program editor flags a
//! step above it; nothing changes a speed by itself, and nothing here touches
//! the belt.

use std::sync::Mutex;

/// The settings key the STRIDE_FIT speed of the treadmill with this device id
/// (as DECK_PROFILE normalizes it) is stored under, in calibrated km/h.
pub fn stride_fit_key_for(device_id: &str) -> String {
    format!("strideFit:{device_id}")
}

/// The STRIDE_FIT speed of the treadmill in use, and its settings key.
struct Fit {
    key: Option<String>,
    kmh: Option<f32>,
}

static FIT: Mutex<Fit> = Mutex::new(Fit {
    key: None,
    kmh: None,
});

fn with_fit<T>(f: impl FnOnce(&mut Fit) -> T) -> T {
    f(&mut FIT.lock().unwrap_or_else(|e| e.into_inner()))
}

/// A stored speed, if it is a positive finite number. The range is applied
/// when it is read (`in_deck_range`), since the deck's range can change after
/// it was saved.
fn decode(raw: Option<&str>) -> Option<f32> {
    raw.and_then(|r| r.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
}

/// Rounded to the tenth: the deck runs and the editor shows speeds in tenths
/// of a km/h, so a finer value would only make the flag's text disagree with
/// the speed it compares.
fn tenths(kmh: f32) -> f32 {
    (kmh * 10.0).round() / 10.0
}

/// Held inside the deck's calibrated speed range. A program step can only run
/// between the floor and the top, so a fit above the top would never flag a
/// step and one below the floor would flag every step; both are typos or a
/// range that changed since, and the nearest end says the same thing.
pub fn in_deck_range(kmh: f32, min_kmh: f32, max_kmh: f32) -> f32 {
    if min_kmh > max_kmh {
        // A deck snapshot before the range is known: nothing to hold it to.
        return tenths(kmh);
    }
    tenths(kmh.clamp(min_kmh, max_kmh))
}

/// The speed to save for a set (`Some`) or a clear (`None`). Refuses a value
/// that is not a positive number; holds the rest to the deck's range.
pub fn checked(kmh: Option<f32>, min_kmh: f32, max_kmh: f32) -> Result<Option<f32>, String> {
    match kmh {
        None => Ok(None),
        Some(v) if !v.is_finite() || v <= 0.0 => {
            Err("STRIDE_FIT must be a speed above 0 km/h.".to_string())
        }
        Some(v) => Ok(Some(in_deck_range(v, min_kmh, max_kmh))),
    }
}

/// Make the STRIDE_FIT speed the one stored for this treadmill. Called with
/// the belt loop's switch (`calibration::switch_belt`).
pub fn switch(store: &impl crate::persistence::SettingsStore, device_id: &str) {
    let key = stride_fit_key_for(device_id);
    let kmh = decode(store.get(&key).as_deref());
    with_fit(|f| {
        f.key = Some(key);
        f.kmh = kmh;
    });
}

/// Set (`Some`) or clear (`None`) the STRIDE_FIT speed of the treadmill in
/// use, held to its range. Gives the speed saved.
pub fn set(
    store: &impl crate::persistence::SettingsStore,
    kmh: Option<f32>,
    min_kmh: f32,
    max_kmh: f32,
) -> Result<Option<f32>, String> {
    let kmh = checked(kmh, min_kmh, max_kmh)?;
    let key = with_fit(|f| f.key.clone())
        .ok_or("Connect the treadmill first, so STRIDE_FIT is kept for that treadmill.")?;
    match kmh {
        Some(v) => store.put(&key, &format!("{v}"))?,
        None => store.remove(&key)?,
    }
    with_fit(|f| f.kmh = kmh);
    match kmh {
        Some(v) => log::info!("STRIDE_FIT set to {v:.1} km/h"),
        None => log::info!("STRIDE_FIT cleared"),
    }
    Ok(kmh)
}

/// The STRIDE_FIT speed of the treadmill in use, for the snapshot, held to
/// the deck's range as it is now. None while unset.
pub fn kmh(min_kmh: f32, max_kmh: f32) -> Option<f32> {
    with_fit(|f| f.kmh).map(|v| in_deck_range(v, min_kmh, max_kmh))
}

/// Whether the HUD shows its STRIDE_FIT line: the belt is running and both
/// its reported speed and its target are above STRIDE_FIT. The lower of the
/// two is compared so the line neither shows while the belt only ramps up
/// towards a faster target nor stays once the user has chosen a speed at or
/// under the fit (the reading lags a change by a second or two). Compared in
/// tenths, the speed the deck runs and the HUD shows, so a belt at the fit
/// speed whose reading wobbles by a few hundredths never shows it. False
/// while STRIDE_FIT is unset. Only a hint: nothing here changes a speed.
pub fn above(running: bool, speed_kmh: f32, target_kmh: f32, fit_kmh: Option<f32>) -> bool {
    running && exceeds(speed_kmh.min(target_kmh), fit_kmh)
}

/// Whether a speed is faster than STRIDE_FIT, compared in tenths (the speed
/// the deck runs), so a speed at the fit never counts. False while unset.
/// Also what the heart rate zones check before they raise a speed (#227).
pub fn exceeds(kmh: f32, fit_kmh: Option<f32>) -> bool {
    let tenth = |v: f32| (v * 10.0).round() as i32;
    fit_kmh.is_some_and(|fit| tenth(kmh) > tenth(fit))
}

/// Tests: the treadmill in use, with this speed, without a database.
#[cfg(test)]
pub(crate) fn set_for_test(device_id: &str, kmh: Option<f32>) {
    with_fit(|f| {
        f.key = Some(stride_fit_key_for(device_id));
        f.kmh = kmh;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::SettingsStore;

    fn guard() -> std::sync::MutexGuard<'static, ()> {
        super::super::TEST_GLOBALS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn above_shows_only_while_running_faster_than_the_fit() {
        assert!(above(true, 5.0, 5.0, Some(4.5)));
        assert!(!above(true, 5.0, 5.0, None), "unset: never");
        assert!(!above(false, 5.0, 5.0, Some(4.5)), "paused or stopped");
        assert!(!above(true, 4.5, 4.5, Some(4.5)), "at the fit");
        assert!(
            !above(true, 4.54, 4.5, Some(4.5)),
            "a reading a few hundredths over"
        );
        assert!(!above(true, 4.0, 5.0, Some(4.5)), "still ramping up");
        assert!(
            !above(true, 5.0, 4.0, Some(4.5)),
            "slowing to a fitting speed"
        );
        assert!(above(true, 4.6, 4.6, Some(4.5)), "one tenth over");
    }

    #[test]
    fn unset_by_default() {
        let _g = guard();
        let store = crate::persistence::memory_db();
        switch(&store, "AAAA");
        assert_eq!(kmh(0.6, 6.0), None);
    }

    #[test]
    fn stored_per_treadmill() {
        let _g = guard();
        let store = crate::persistence::memory_db();
        switch(&store, "AAAA");
        assert_eq!(set(&store, Some(5.4), 0.6, 6.0), Ok(Some(5.4)));
        assert_eq!(store.get("strideFit:AAAA").as_deref(), Some("5.4"));
        switch(&store, "BBBB");
        assert_eq!(kmh(0.6, 6.0), None, "another treadmill has its own");
        set(&store, Some(4.8), 0.6, 6.0).unwrap();
        switch(&store, "AAAA");
        assert_eq!(kmh(0.6, 6.0), Some(5.4));
        switch(&store, "BBBB");
        assert_eq!(kmh(0.6, 6.0), Some(4.8));
    }

    #[test]
    fn held_to_the_deck_range_and_tenths() {
        assert_eq!(checked(Some(9.0), 0.6, 6.0), Ok(Some(6.0)));
        assert_eq!(checked(Some(0.3), 0.6, 6.0), Ok(Some(0.6)));
        assert_eq!(checked(Some(5.43), 0.6, 6.0), Ok(Some(5.4)));
        assert!(checked(Some(0.0), 0.6, 6.0).is_err());
        assert!(checked(Some(f32::NAN), 0.6, 6.0).is_err());
        assert_eq!(checked(None, 0.6, 6.0), Ok(None));
        // A stored speed above a top lowered since reads at the top.
        let _g = guard();
        set_for_test("TEST", Some(5.4));
        assert_eq!(kmh(0.6, 5.0), Some(5.0));
        set_for_test("TEST", None);
    }

    #[test]
    fn cleared_removes_the_setting() {
        let _g = guard();
        let store = crate::persistence::memory_db();
        switch(&store, "AAAA");
        set(&store, Some(5.0), 0.6, 6.0).unwrap();
        assert_eq!(set(&store, None, 0.6, 6.0), Ok(None));
        assert_eq!(store.get("strideFit:AAAA"), None);
        assert_eq!(kmh(0.6, 6.0), None);
        switch(&store, "AAAA");
        assert_eq!(kmh(0.6, 6.0), None, "still cleared after a reconnect");
    }

    #[test]
    fn a_damaged_stored_value_reads_as_unset() {
        let _g = guard();
        let store = crate::persistence::memory_db();
        store.put("strideFit:AAAA", "fast").unwrap();
        switch(&store, "AAAA");
        assert_eq!(kmh(0.6, 6.0), None);
    }

    #[test]
    fn needs_a_treadmill() {
        let _g = guard();
        with_fit(|f| {
            f.key = None;
            f.kmh = None;
        });
        let store = crate::persistence::memory_db();
        assert!(set(&store, Some(5.0), 0.6, 6.0)
            .unwrap_err()
            .contains("Connect the treadmill"));
    }
}
