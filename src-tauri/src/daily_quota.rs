//! DAILY_QUOTA (#439): an optional daily distance target, set in System,
//! Desk, off by default. While set, DAILY_TOTAL (the Live tile, the HUD's
//! TODAY on the panel and the strip, STEP_MAP's readout) shows today's km
//! against it with a thin bar. A quiet glance at how far is left today: no
//! toast, no card, no streak and no record. Walking past it only fills the
//! bar. Rust works out the share from `session.daily_distance_m`; the
//! windows only draw it. It only reads the day's distance; nothing here
//! touches the belt.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

const KEY: &str = "daily_quota_km";

/// The smallest target: half a kilometre, a few minutes of walking at a
/// desk. Below it the bar would fill before the readout's hundredths of a
/// km had shown much, and a smaller number is a typing slip.
pub const MIN_KM: f32 = 0.5;
/// The largest target: 60 km, past a full working day at the desk (8 hours
/// at 6 km/h is 48 km). A larger number is a typing slip.
pub const MAX_KM: f32 = 60.0;
/// The target the field offers when the switch goes on: 5 km, about an
/// hour and a half at a desk walking pace of 3.5 km/h.
pub const DEFAULT_KM: f32 = 5.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DailyQuotaSnapshot {
    /// The target in km; none while DAILY_QUOTA is off.
    pub target_km: Option<f32>,
    /// Today's distance over the target, 0 to 1 for the bar; 1 once walked
    /// (it never runs past full), 0 while off.
    pub fraction: f32,
    /// What is left today in km, 0 once the target is walked or while off.
    pub left_km: f32,
    /// Rust's sentence for the tooltip; empty while off.
    pub message: String,
}

/// The quota for today's distance. Pure.
pub fn quota(target_km: Option<f32>, daily_m: f64) -> DailyQuotaSnapshot {
    let Some(target) = target_km.filter(|t| *t > 0.0) else {
        return DailyQuotaSnapshot::default();
    };
    let walked_km = (daily_m.max(0.0) / 1000.0) as f32;
    let fraction = (walked_km / target).clamp(0.0, 1.0);
    let left_km = (target - walked_km).max(0.0);
    // Hundredths, as the readout shows today's km: a target counts as walked
    // only once the readout reaches it.
    let left_shown = (left_km * 100.0).round() / 100.0;
    let message = if left_shown <= 0.0 {
        format!("Today's {target:.1} km target is walked.")
    } else {
        format!("{left_shown:.2} km left of today's {target:.1} km target.")
    };
    DailyQuotaSnapshot {
        target_km: Some(target),
        fraction: if left_shown <= 0.0 { 1.0 } else { fraction },
        left_km: left_shown,
        message,
    }
}

/// A target is within `MIN_KM` to `MAX_KM`.
pub fn valid(km: f32) -> bool {
    km.is_finite() && (MIN_KM..=MAX_KM).contains(&km)
}

/// A target to a tenth of a km, as the readout shows it.
fn tenths(km: f32) -> f32 {
    (km * 10.0).round() / 10.0
}

/// The stored text, read back; none when unset, unreadable or out of range.
fn parse(stored: Option<&str>) -> Option<f32> {
    crate::persistence::parse_setting::<f32>(stored).filter(|km| valid(*km))
}

static TARGET: Mutex<Option<f32>> = Mutex::new(None);

fn target() -> Option<f32> {
    *TARGET.lock().unwrap_or_else(|e| e.into_inner())
}

/// Load the saved target (none when never set). Blocking; call once after
/// the DB is initialised.
pub fn init(app: &AppHandle) {
    let km = parse(crate::persistence::read_setting(app, KEY).as_deref());
    *TARGET.lock().unwrap_or_else(|e| e.into_inner()) = km;
}

/// Set the target in km, or none to switch DAILY_QUOTA off. Blocking (SQLite).
pub fn set(app: &AppHandle, km: Option<f32>) -> Result<(), String> {
    let km = km.map(tenths);
    match km {
        Some(km) if !valid(km) => {
            return Err(format!(
                "Pick a daily target from {MIN_KM} to {MAX_KM:.0} km."
            ))
        }
        Some(km) => crate::persistence::write_setting(app, KEY, &km.to_string())?,
        None => crate::persistence::delete_setting(app, KEY)?,
    }
    *TARGET.lock().unwrap_or_else(|e| e.into_inner()) = km;
    match km {
        Some(km) => log::info!("DAILY_QUOTA set to {km} km"),
        None => log::info!("DAILY_QUOTA off"),
    }
    Ok(())
}

/// For the snapshot. No I/O.
pub fn snapshot(daily_m: f64) -> DailyQuotaSnapshot {
    quota(target(), daily_m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_shows_nothing() {
        assert_eq!(quota(None, 4310.0), DailyQuotaSnapshot::default());
        assert_eq!(quota(None, 4310.0).message, "");
    }

    #[test]
    fn part_way_gives_the_share_and_what_is_left() {
        let q = quota(Some(5.0), 4310.0);
        assert_eq!(q.target_km, Some(5.0));
        assert!((q.fraction - 0.862).abs() < 1e-4, "{}", q.fraction);
        assert!((q.left_km - 0.69).abs() < 1e-4, "{}", q.left_km);
        assert_eq!(q.message, "0.69 km left of today's 5.0 km target.");
    }

    #[test]
    fn nothing_walked_yet_is_an_empty_bar() {
        let q = quota(Some(5.0), 0.0);
        assert_eq!(q.fraction, 0.0);
        assert_eq!(q.left_km, 5.0);
        assert_eq!(q.message, "5.00 km left of today's 5.0 km target.");
    }

    #[test]
    fn over_target_stays_full_with_nothing_left() {
        let q = quota(Some(5.0), 7250.0);
        assert_eq!(q.fraction, 1.0);
        assert_eq!(q.left_km, 0.0);
        assert_eq!(q.message, "Today's 5.0 km target is walked.");
    }

    #[test]
    fn walked_once_the_readout_shows_the_target() {
        // 4.996 km reads 5.00 on the tile, so nothing is left to show.
        let q = quota(Some(5.0), 4996.0);
        assert_eq!(q.fraction, 1.0);
        assert_eq!(q.left_km, 0.0);
    }

    #[test]
    fn limits_hold_the_target() {
        assert!(valid(MIN_KM) && valid(MAX_KM) && valid(DEFAULT_KM));
        assert!(!valid(0.4) && !valid(60.1) && !valid(f32::NAN) && !valid(0.0));
    }

    #[test]
    fn targets_keep_a_tenth() {
        assert_eq!(tenths(7.25), 7.3);
        assert_eq!(tenths(5.0), 5.0);
        // 0.46 rounds up to the smallest target, 0.44 stays under it.
        assert!(valid(tenths(0.46)) && !valid(tenths(0.44)));
    }

    #[test]
    fn stored_text_reads_back() {
        assert_eq!(parse(Some("7.5")), Some(7.5));
        assert_eq!(parse(Some("\"7.5\"")), Some(7.5));
        assert_eq!(parse(None), None);
        assert_eq!(parse(Some("")), None);
        assert_eq!(parse(Some("abc")), None);
        // A value saved before a limit changed is dropped, not shown.
        assert_eq!(parse(Some("500")), None);
    }
}
