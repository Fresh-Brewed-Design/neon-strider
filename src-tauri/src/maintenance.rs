//! Belt maintenance reminder.
//!
//! The belt needs lubricating or servicing every so often. We track distance
//! walked (or, optionally, belt running hours) and time since the last service
//! and call it due when either interval is reached, whichever comes first. The
//! manufacturer's figure is 3 months or 100 miles. Distances are stored and sent
//! in km; the UI converts. Which of distance and hours counts is a setting
//! (`MaintenanceBasis`); the other is still reported but never makes it due.
//!
//! Like the daily distance, the saved-sessions part of the total is cached and
//! refreshed off-thread (startup, session change, settings change, once a
//! minute); the snapshot only adds the live session on top. The due/progress
//! maths is pure so it can be unit-tested.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use crate::persistence;
use crate::util::now_ms;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

// Settings keys (raw values in the `settings` table).
const KEY_LAST_SERVICE_MS: &str = "maintenance_last_service_ms";
const KEY_INTERVAL_KM: &str = "maintenance_interval_km";
const KEY_INTERVAL_DAYS: &str = "maintenance_interval_days";
const KEY_BASIS: &str = "maintenance_basis";
const KEY_INTERVAL_HOURS: &str = "maintenance_interval_hours";

/// 100 miles.
const DEFAULT_INTERVAL_KM: f32 = 160.934;
const DEFAULT_INTERVAL_DAYS: u32 = 90;
const MIN_INTERVAL_KM: f32 = 50.0;
const MAX_INTERVAL_KM: f32 = 5000.0;
const MAX_INTERVAL_DAYS: u32 = 730;
const DEFAULT_INTERVAL_HOURS: f32 = 50.0;
const MIN_INTERVAL_HOURS: f32 = 5.0;
const MAX_INTERVAL_HOURS: f32 = 1000.0;
/// 2020-01-01T00:00:00Z. Earlier "service dates" are typos.
const MIN_LAST_SERVICE_MS: i64 = 1_577_836_800_000;

const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

pub fn is_maintenance_key(key: &str) -> bool {
    [
        KEY_LAST_SERVICE_MS,
        KEY_INTERVAL_KM,
        KEY_INTERVAL_DAYS,
        KEY_BASIS,
        KEY_INTERVAL_HOURS,
    ]
    .contains(&key)
}

/// What the usage interval counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum MaintenanceBasis {
    /// Distance walked (Jogwell's 100 miles) and days.
    #[default]
    Distance,
    /// Belt running hours and days.
    Hours,
}

impl MaintenanceBasis {
    fn parse(raw: Option<&str>) -> Self {
        match raw.map(|s| s.trim().trim_matches('"')) {
            Some("hours") => Self::Hours,
            _ => Self::Distance,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Distance => "distance",
            Self::Hours => "hours",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct MaintenanceSnapshot {
    /// Epoch ms of the last service.
    #[ts(type = "number")]
    pub last_service_ms: i64,
    /// Distance walked since then, including the live session.
    pub distance_since_km: f32,
    pub interval_km: f32,
    /// Which of distance and belt hours counts towards `due` and `progress`.
    pub basis: MaintenanceBasis,
    /// Belt running hours since then, including the live session.
    pub hours_since: f32,
    pub interval_hours: f32,
    /// Whole days since the last service.
    pub days_since: u32,
    /// 0 means no time limit.
    pub interval_days: u32,
    pub due: bool,
    /// The larger of the usage (per `basis`) and time fractions; 1.0 or more is due.
    pub progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub last_service_ms: i64,
    pub interval_km: f32,
    pub interval_days: u32,
    pub basis: MaintenanceBasis,
    pub interval_hours: f32,
}

// Pure logic.

/// Due/progress from the config, the saved usage since service (`base_m` metres,
/// `base_s` belt seconds), the live session's (`live_m`, `live_s`, already
/// filtered to count) and now.
pub fn compute(
    cfg: &Config,
    base_m: f64,
    base_s: i64,
    live_m: f64,
    live_s: i64,
    now_ms: i64,
) -> MaintenanceSnapshot {
    let dist_km = ((base_m + live_m) / 1000.0).max(0.0);
    let hours = ((base_s.max(0) + live_s.max(0)) as f64 / 3600.0).max(0.0);
    let elapsed_ms = (now_ms - cfg.last_service_ms).max(0);
    let days = elapsed_ms / MS_PER_DAY;

    let frac_km = if cfg.basis == MaintenanceBasis::Distance && cfg.interval_km > 0.0 {
        dist_km / cfg.interval_km as f64
    } else {
        0.0
    };
    let frac_hours = if cfg.basis == MaintenanceBasis::Hours && cfg.interval_hours > 0.0 {
        hours / cfg.interval_hours as f64
    } else {
        0.0
    };
    let frac_days = if cfg.interval_days == 0 {
        0.0
    } else {
        elapsed_ms as f64 / (cfg.interval_days as i64 * MS_PER_DAY) as f64
    };
    let progress = frac_km.max(frac_hours).max(frac_days);

    MaintenanceSnapshot {
        last_service_ms: cfg.last_service_ms,
        distance_since_km: dist_km as f32,
        interval_km: cfg.interval_km,
        basis: cfg.basis,
        hours_since: hours as f32,
        interval_hours: cfg.interval_hours,
        days_since: days.min(u32::MAX as i64) as u32,
        interval_days: cfg.interval_days,
        due: progress >= 1.0,
        progress: progress as f32,
    }
}

/// Build the config from raw stored values. A missing last-service time is a
/// first run: it becomes `now_ms` so a fresh install doesn't nag at once. The
/// bool says whether it was defaulted (and so needs writing back). Bad or
/// out-of-range intervals fall back to the defaults.
pub fn config_from_raw(
    last_service: Option<&str>,
    interval_km: Option<&str>,
    interval_days: Option<&str>,
    basis: Option<&str>,
    interval_hours: Option<&str>,
    now_ms: i64,
) -> (Config, bool) {
    let stored: Option<i64> =
        persistence::parse_setting(last_service).filter(|v| *v >= MIN_LAST_SERVICE_MS);
    let first_run = stored.is_none();
    // The finite ranges also rule out NaN and infinity.
    let km = MIN_INTERVAL_KM..=MAX_INTERVAL_KM;
    let hours = MIN_INTERVAL_HOURS..=MAX_INTERVAL_HOURS;
    (
        Config {
            last_service_ms: stored.unwrap_or(now_ms),
            interval_km: persistence::setting_in(interval_km, km, DEFAULT_INTERVAL_KM),
            interval_days: persistence::setting_in(
                interval_days,
                0..=MAX_INTERVAL_DAYS,
                DEFAULT_INTERVAL_DAYS,
            ),
            basis: MaintenanceBasis::parse(basis),
            interval_hours: persistence::setting_in(interval_hours, hours, DEFAULT_INTERVAL_HOURS),
        },
        first_run,
    )
}

/// km 50 to 5000, days 0 to 730 (0 = no time limit).
fn validate_intervals(km: f32, days: u32) -> Result<(f32, u32), String> {
    if !km.is_finite() || !(MIN_INTERVAL_KM..=MAX_INTERVAL_KM).contains(&km) {
        return Err(format!(
            "Distance interval must be between {} and {} km",
            MIN_INTERVAL_KM, MAX_INTERVAL_KM
        ));
    }
    if days > MAX_INTERVAL_DAYS {
        return Err(format!(
            "Time interval must be at most {} days (0 for none)",
            MAX_INTERVAL_DAYS
        ));
    }
    Ok((km, days))
}

/// Hours 5 to 1000.
fn validate_interval_hours(hours: f32) -> Result<f32, String> {
    if !hours.is_finite() || !(MIN_INTERVAL_HOURS..=MAX_INTERVAL_HOURS).contains(&hours) {
        return Err(format!(
            "Hours interval must be between {} and {}",
            MIN_INTERVAL_HOURS, MAX_INTERVAL_HOURS
        ));
    }
    Ok(hours)
}

/// Not in the future, not before 2020.
fn validate_last_service(at_ms: i64, now_ms: i64) -> Result<i64, String> {
    if at_ms > now_ms {
        return Err("Service date can't be in the future".to_string());
    }
    if at_ms < MIN_LAST_SERVICE_MS {
        return Err("Service date is too far in the past".to_string());
    }
    Ok(at_ms)
}

// Cached state.

struct State {
    cfg: Config,
    /// Saved distance since the last service, excluding the live session.
    base_m: f64,
    /// Saved belt seconds since then, excluding the live session.
    base_s: i64,
    /// The live session started after the last service, so its distance counts.
    live_counts: bool,
    /// Live session id at the last refresh.
    active_id: Option<i64>,
}

static STATE: Mutex<State> = Mutex::new(State {
    cfg: Config {
        last_service_ms: 0,
        interval_km: DEFAULT_INTERVAL_KM,
        interval_days: DEFAULT_INTERVAL_DAYS,
        basis: MaintenanceBasis::Distance,
        interval_hours: DEFAULT_INTERVAL_HOURS,
    },
    base_m: 0.0,
    base_s: 0,
    live_counts: false,
    active_id: None,
});

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn load_config(app: &AppHandle) -> Config {
    let now = now_ms();
    let read = |k: &str| persistence::read_setting(app, k);
    let (cfg, first_run) = config_from_raw(
        read(KEY_LAST_SERVICE_MS).as_deref(),
        read(KEY_INTERVAL_KM).as_deref(),
        read(KEY_INTERVAL_DAYS).as_deref(),
        read(KEY_BASIS).as_deref(),
        read(KEY_INTERVAL_HOURS).as_deref(),
        now,
    );
    if first_run {
        write(app, KEY_LAST_SERVICE_MS, &cfg.last_service_ms.to_string());
    }
    cfg
}

fn write(app: &AppHandle, key: &str, value: &str) {
    if let Err(e) = persistence::write_setting(app, key, value) {
        log::warn!("Failed to persist {}: {}", key, e);
    }
}

/// Load the settings and the saved distance. Blocking; call once after the DB
/// is initialised.
pub fn init(app: &AppHandle) {
    let cfg = load_config(app);
    lock().cfg = cfg;
    refresh(app, None);
}

/// Re-read the saved distance and belt time since the last service from SQLite,
/// skipping the live session `active_id` (the snapshot adds it). Blocking;
/// never called per snapshot tick.
pub fn refresh(app: &AppHandle, active_id: Option<i64>) {
    let since = lock().cfg.last_service_ms;
    let (base, base_s) = persistence::usage_since(app, since, active_id);
    let live_counts = match active_id {
        Some(id) => persistence::session_start_ms(app, id).is_none_or(|s| s >= since),
        None => false,
    };
    let mut st = lock();
    st.base_m = base;
    st.base_s = base_s;
    st.live_counts = live_counts;
    st.active_id = active_id;
}

/// The settings table changed behind our back: reload and refresh.
pub fn reload(app: &AppHandle) {
    let cfg = load_config(app);
    let active = {
        let mut st = lock();
        st.cfg = cfg;
        st.active_id
    };
    refresh(app, active);
}

/// Snapshot from the cache plus the live session's distance and belt seconds
/// (0 when no session is open). No I/O.
pub fn snapshot(live_m: f64, live_s: i64) -> MaintenanceSnapshot {
    let st = lock();
    let (live_m, live_s) = if st.live_counts {
        (live_m, live_s)
    } else {
        (0.0, 0)
    };
    compute(&st.cfg, st.base_m, st.base_s, live_m, live_s, now_ms())
}

fn set_last_service_unchecked(app: &AppHandle, at_ms: i64) {
    write(app, KEY_LAST_SERVICE_MS, &at_ms.to_string());
    lock().cfg.last_service_ms = at_ms;
    let active = lock().active_id;
    refresh(app, active);
}

/// Blocking (SQLite).
pub fn mark_serviced(app: &AppHandle) -> Result<(), String> {
    set_last_service_unchecked(app, now_ms());
    Ok(())
}

/// Blocking (SQLite).
pub fn set_last_service(app: &AppHandle, at_ms: i64) -> Result<(), String> {
    let at = validate_last_service(at_ms, now_ms())?;
    set_last_service_unchecked(app, at);
    Ok(())
}

/// Blocking (SQLite).
pub fn set_intervals(app: &AppHandle, km: f32, days: u32) -> Result<(), String> {
    let (km, days) = validate_intervals(km, days)?;
    write(app, KEY_INTERVAL_KM, &km.to_string());
    write(app, KEY_INTERVAL_DAYS, &days.to_string());
    let mut st = lock();
    st.cfg.interval_km = km;
    st.cfg.interval_days = days;
    Ok(())
}

/// Choose what the usage interval counts, and the hours interval. Blocking (SQLite).
pub fn set_basis(app: &AppHandle, basis: MaintenanceBasis, hours: f32) -> Result<(), String> {
    let hours = validate_interval_hours(hours)?;
    write(app, KEY_BASIS, basis.as_str());
    write(app, KEY_INTERVAL_HOURS, &hours.to_string());
    let mut st = lock();
    st.cfg.basis = basis;
    st.cfg.interval_hours = hours;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = MS_PER_DAY;
    const T0: i64 = 1_800_000_000_000; // a service time well after 2020

    fn cfg(km: f32, days: u32) -> Config {
        Config {
            last_service_ms: T0,
            interval_km: km,
            interval_days: days,
            basis: MaintenanceBasis::Distance,
            interval_hours: DEFAULT_INTERVAL_HOURS,
        }
    }

    fn hours_cfg(hours: f32, days: u32) -> Config {
        Config {
            basis: MaintenanceBasis::Hours,
            interval_hours: hours,
            ..cfg(500.0, days)
        }
    }

    /// Distance-basis compute with no belt time, as the old tests had it.
    fn dist(c: &Config, base_m: f64, live_m: f64, now: i64) -> MaintenanceSnapshot {
        compute(c, base_m, 0, live_m, 0, now)
    }

    #[test]
    fn due_by_distance_only() {
        let c = cfg(500.0, 90);
        assert!(!dist(&c, 499_000.0, 0.0, T0 + 10 * DAY).due);
        let s = dist(&c, 499_000.0, 1_000.0, T0 + 10 * DAY);
        assert!(s.due);
        assert_eq!(s.distance_since_km, 500.0);
        assert_eq!(s.days_since, 10);
    }

    #[test]
    fn due_by_days_only() {
        let c = cfg(500.0, 90);
        assert!(!dist(&c, 1_000.0, 0.0, T0 + 89 * DAY).due);
        let s = dist(&c, 1_000.0, 0.0, T0 + 90 * DAY);
        assert!(s.due);
        assert_eq!(s.days_since, 90);
    }

    #[test]
    fn due_by_both_and_progress_is_the_larger() {
        let c = cfg(500.0, 100);
        // 50% of distance, 80% of time: time leads.
        let s = dist(&c, 250_000.0, 0.0, T0 + 80 * DAY);
        assert!((s.progress - 0.8).abs() < 1e-6);
        assert!(!s.due);
        // Distance leads.
        let s = dist(&c, 400_000.0, 0.0, T0 + 10 * DAY);
        assert!((s.progress - 0.8).abs() < 1e-6);
        // Both over: progress above 1.
        let s = dist(&c, 600_000.0, 0.0, T0 + 150 * DAY);
        assert!(s.due);
        assert!((s.progress - 1.5).abs() < 1e-6);
    }

    #[test]
    fn zero_days_disables_the_time_limit() {
        let c = cfg(500.0, 0);
        let s = dist(&c, 0.0, 0.0, T0 + 5000 * DAY);
        assert!(!s.due);
        assert_eq!(s.progress, 0.0);
        assert_eq!(s.interval_days, 0);
        // Distance still works.
        assert!(dist(&c, 500_000.0, 0.0, T0).due);
    }

    #[test]
    fn live_distance_adds_and_clock_skew_is_safe() {
        let c = cfg(100.0, 0);
        let s = dist(&c, 60_000.0, 30_000.0, T0);
        assert!((s.distance_since_km - 90.0).abs() < 1e-4);
        assert!((s.progress - 0.9).abs() < 1e-6);
        // Service time in the future relative to now: no negative days.
        let s = dist(&c, 0.0, 0.0, T0 - DAY);
        assert_eq!(s.days_since, 0);
    }

    #[test]
    fn default_interval_is_one_hundred_miles() {
        assert!((DEFAULT_INTERVAL_KM - 100.0 * 1.609_344).abs() < 0.01);
        let c = cfg(DEFAULT_INTERVAL_KM, DEFAULT_INTERVAL_DAYS);
        assert!(!dist(&c, 160_000.0, 0.0, T0).due);
        assert!(dist(&c, 161_000.0, 0.0, T0).due);
    }

    #[test]
    fn first_run_defaults_last_service_to_now() {
        let now = T0 + 7;
        let (c, first) = config_from_raw(None, None, None, None, None, now);
        assert!(first);
        assert_eq!(c.last_service_ms, now);
        assert_eq!(c.interval_km, 160.934);
        assert_eq!(c.interval_days, 90);
        assert_eq!(c.basis, MaintenanceBasis::Distance);
        assert_eq!(c.interval_hours, 50.0);
        // And so a new install is not due.
        assert!(!dist(&c, 0.0, 0.0, now).due);

        // A garbage or pre-2020 value counts as missing.
        assert!(config_from_raw(Some("abc"), None, None, None, None, now).1);
        assert!(config_from_raw(Some("5"), None, None, None, None, now).1);
    }

    #[test]
    fn stored_values_are_read_and_bad_intervals_fall_back() {
        let now = T0 + 99 * DAY;
        let (c, first) = config_from_raw(
            Some(&T0.to_string()),
            Some("250"),
            Some("0"),
            None,
            None,
            now,
        );
        assert!(!first);
        assert_eq!(c.last_service_ms, T0);
        assert_eq!(c.interval_km, 250.0);
        assert_eq!(c.interval_days, 0);

        let (c, _) = config_from_raw(
            Some(&T0.to_string()),
            Some("10"),
            Some("9999"),
            None,
            Some("2"),
            now,
        );
        assert_eq!(c.interval_km, DEFAULT_INTERVAL_KM);
        assert_eq!(c.interval_days, DEFAULT_INTERVAL_DAYS);
        let (c, _) = config_from_raw(
            Some(&T0.to_string()),
            Some("nan"),
            Some("-3"),
            Some("sideways"),
            Some("nan"),
            now,
        );
        assert_eq!(c.interval_km, DEFAULT_INTERVAL_KM);
        assert_eq!(c.interval_days, DEFAULT_INTERVAL_DAYS);
    }

    #[test]
    fn interval_validation() {
        assert_eq!(validate_intervals(500.0, 90), Ok((500.0, 90)));
        assert_eq!(validate_intervals(50.0, 0), Ok((50.0, 0)));
        assert_eq!(validate_intervals(5000.0, 730), Ok((5000.0, 730)));
        assert!(validate_intervals(49.9, 90).is_err());
        assert!(validate_intervals(5000.1, 90).is_err());
        assert!(validate_intervals(f32::NAN, 90).is_err());
        assert!(validate_intervals(f32::INFINITY, 90).is_err());
        assert!(validate_intervals(500.0, 731).is_err());
    }

    #[test]
    fn last_service_validation() {
        let now = T0;
        assert_eq!(validate_last_service(now, now), Ok(now));
        assert_eq!(
            validate_last_service(now - 30 * DAY, now),
            Ok(now - 30 * DAY)
        );
        assert!(validate_last_service(now + 1, now).is_err());
        assert!(validate_last_service(MIN_LAST_SERVICE_MS - 1, now).is_err());
        assert_eq!(
            validate_last_service(MIN_LAST_SERVICE_MS, now),
            Ok(MIN_LAST_SERVICE_MS)
        );
        assert!(validate_last_service(0, now).is_err());
        assert!(validate_last_service(-5, now).is_err());
    }

    #[test]
    fn hours_basis_counts_belt_time_and_ignores_distance() {
        let c = hours_cfg(50.0, 90);
        let h = 3600;
        // 49 h saved and 30 min live: not yet. A huge distance does not matter.
        let s = compute(&c, 9_999_000.0, 49 * h, 0.0, h / 2, T0 + 10 * DAY);
        assert!(!s.due);
        assert!((s.hours_since - 49.5).abs() < 1e-4);
        assert!((s.progress - 0.99).abs() < 1e-6);
        assert_eq!(s.basis, MaintenanceBasis::Hours);
        // Another 30 minutes tips it over.
        assert!(compute(&c, 0.0, 49 * h, 0.0, h, T0).due);
        // Days still count on the hours basis.
        let s = compute(&c, 0.0, h, 0.0, 0, T0 + 90 * DAY);
        assert!(s.due);
        assert_eq!(s.days_since, 90);
    }

    #[test]
    fn distance_basis_ignores_belt_hours_but_still_reports_them() {
        let c = cfg(500.0, 0);
        let s = compute(&c, 1_000.0, 500 * 3600, 0.0, 0, T0);
        assert!(!s.due);
        assert_eq!(s.progress, s.distance_since_km / 500.0);
        assert!((s.hours_since - 500.0).abs() < 1e-3);
    }

    #[test]
    fn basis_and_hours_settings_parse_with_fallbacks() {
        let at = |b, h| config_from_raw(Some(&T0.to_string()), None, None, b, h, T0).0;
        assert_eq!(at(Some("hours"), None).basis, MaintenanceBasis::Hours);
        assert_eq!(at(Some("\"hours\""), None).basis, MaintenanceBasis::Hours);
        assert_eq!(at(Some("distance"), None).basis, MaintenanceBasis::Distance);
        assert_eq!(at(Some("???"), None).basis, MaintenanceBasis::Distance);
        assert_eq!(at(None, Some("80")).interval_hours, 80.0);
        assert_eq!(at(None, Some("4.9")).interval_hours, DEFAULT_INTERVAL_HOURS);
        assert_eq!(
            at(None, Some("1000.1")).interval_hours,
            DEFAULT_INTERVAL_HOURS
        );
        assert_eq!(MaintenanceBasis::Hours.as_str(), "hours");
        assert_eq!(
            MaintenanceBasis::parse(Some(MaintenanceBasis::Hours.as_str())),
            MaintenanceBasis::Hours
        );
    }

    #[test]
    fn hours_interval_validation() {
        assert_eq!(validate_interval_hours(50.0), Ok(50.0));
        assert_eq!(validate_interval_hours(5.0), Ok(5.0));
        assert_eq!(validate_interval_hours(1000.0), Ok(1000.0));
        assert!(validate_interval_hours(4.9).is_err());
        assert!(validate_interval_hours(1000.1).is_err());
        assert!(validate_interval_hours(f32::NAN).is_err());
    }
}
