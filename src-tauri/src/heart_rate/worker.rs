//! HEART_LINK's runtime: the picked sensor (settings `heart_rate_sensor_id`
//! and `heart_rate_sensor_name`; none by default), the scan for the picker,
//! and the link, which is separate from the treadmill's. A 1 s tick
//! (re)connects while a sensor is picked.

use super::history::History;
use super::{
    describe_failure, parse_measurement, status_label, status_message, HeartRateSensor,
    HeartRateSnapshot, HeartStatus, Step, HR_MEASUREMENT, HR_SERVICE, NOT_FOUND, NO_MEASUREMENT,
};
use crate::bluetooth::{find_central, ids_match, normalize_id};
use crate::persistence;
use btleplug::api::{Central, Peripheral as _, ScanFilter};
use btleplug::platform::Peripheral;
use futures::StreamExt;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::AppHandle;

const KEY_ID: &str = "heart_rate_sensor_id";
const KEY_NAME: &str = "heart_rate_sensor_name";
const KEY_MAX: &str = "max_heart_rate";
/// Used for zones until the user sets their own: 220 minus age for a
/// 30-year-old, the common estimate, so zones are near right before it is set.
pub const DEFAULT_MAX_BPM: u16 = 190;
static MAX_BPM: AtomicU16 = AtomicU16::new(DEFAULT_MAX_BPM);
/// The user saved a maximum heart rate (else `MAX_BPM` is the default).
static MAX_SET: AtomicBool = AtomicBool::new(false);
/// Labs, ZONE_TILT: program zones may trim incline. Off by default, since it
/// moves the deck in a way only a walk with a heart rate sensor can test.
const KEY_INCLINE_TRIM: &str = "zone_incline_trim";
static INCLINE_TRIM: AtomicBool = AtomicBool::new(false);
/// How long the picker's scan listens: a sensor advertises about once a
/// second, and the treadmill scan uses the same 5 s.
const SCAN_TIME: Duration = Duration::from_secs(5);
/// How long a connect attempt looks for the picked sensor. Short, since it
/// holds the shared scan and a treadmill reconnect waits behind it.
const FIND_TIME: Duration = Duration::from_secs(5);
/// The first retry after a failure, doubling on each failure after that.
const RETRY_FIRST: Duration = Duration::from_secs(10);
/// The longest wait between attempts: a watch that is not broadcasting
/// (most of the day) costs one 5 s scan a minute, and one switched on
/// mid-walk links within about a minute.
const RETRY_MAX: Duration = Duration::from_secs(60);
/// A reading older than this no longer shows: sensors send about once a
/// second, so six missed readings mean the watch stopped broadcasting.
const STALE_AFTER: Duration = Duration::from_secs(6);
/// A sensor name is cut to this long, so a device can't put a huge string
/// in the settings store; real names are under 30 characters.
const NAME_MAX: usize = 100;
/// `find` hands this back when another scan holds the radio; the tick tries
/// again without counting a failure.
const SCAN_BUSY: &str = "Another Bluetooth scan is running.";

/// The wait before the next attempt after `failures` failures in a row.
fn retry_after(failures: u32) -> Duration {
    // Six doublings of 10 s pass `RETRY_MAX`; the cap only stops the overflow.
    let doublings = failures.saturating_sub(1).min(6);
    (RETRY_FIRST * 2u32.pow(doublings)).min(RETRY_MAX)
}

#[derive(Default)]
struct Runtime {
    sensor: Option<HeartRateSensor>,
    status: HeartStatus,
    error: Option<String>,
    /// The last failure's log line, so one reason logs once.
    logged: Option<String>,
    /// A connect attempt is in flight.
    connecting: bool,
    /// Bumped on every pick or forget, so a stale link's tasks stand down.
    generation: u64,
    link: Option<Peripheral>,
    bpm: Option<u16>,
    bpm_at: Option<Instant>,
    failed_at: Option<Instant>,
    /// Failed attempts in a row, for the backoff; reset by a link or a pick.
    failures: u32,
    scanning: bool,
    found: Vec<HeartRateSensor>,
    /// The PULSE_RATE chart's rolling window.
    history: History,
}

/// The chart's monotonic clock, so a change to the PC's time can't shift it.
static CLOCK: LazyLock<Instant> = LazyLock::new(Instant::now);

static RT: LazyLock<Mutex<Runtime>> = LazyLock::new(|| Mutex::new(Runtime::default()));

fn with<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    f(&mut RT.lock().unwrap_or_else(PoisonError::into_inner))
}

/// A 16-bit Bluetooth UUID on the standard base.
fn short_uuid(short: u16) -> uuid::Uuid {
    uuid::Uuid::from_u128((u128::from(short) << 96) | 0x0000_1000_8000_0080_5f9b_34fb)
}

/// Read the picked sensor. Call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    if let Some(bpm) =
        persistence::parse_setting::<u16>(persistence::read_setting(app, KEY_MAX).as_deref())
            .filter(|b| valid_max(*b))
    {
        MAX_BPM.store(bpm, Ordering::Relaxed);
        MAX_SET.store(true, Ordering::Relaxed);
    }
    let incline = persistence::parse_setting::<bool>(
        persistence::read_setting(app, KEY_INCLINE_TRIM).as_deref(),
    );
    INCLINE_TRIM.store(incline.unwrap_or(false), Ordering::Relaxed);
    let id = persistence::read_setting(app, KEY_ID).filter(|s| !s.is_empty());
    let name = persistence::read_setting(app, KEY_NAME).unwrap_or_default();
    if let Some(id) = id {
        log::info!("HEART_LINK: sensor picked; linking");
        with(|rt| {
            rt.sensor = Some(HeartRateSensor { id, name });
            rt.status = HeartStatus::Searching;
        });
    }
}

/// A maximum heart rate outside 120 to 230 bpm is a typing slip: 220 minus
/// age gives about 120 at 100 years old, and measured maxima above 230 are
/// almost unheard of. HeartRateSection reads the range from `SettingLimits`.
pub const MIN_MAX_BPM: u16 = 120;
pub const MAX_MAX_BPM: u16 = 230;

fn valid_max(bpm: u16) -> bool {
    (MIN_MAX_BPM..=MAX_MAX_BPM).contains(&bpm)
}

/// The maximum heart rate that zones are a percentage of.
pub fn max_heart_rate() -> u16 {
    MAX_BPM.load(Ordering::Relaxed)
}

/// Save the maximum heart rate (120 to 230 bpm).
pub fn set_max_heart_rate(app: &AppHandle, bpm: u16) -> Result<(), String> {
    if !valid_max(bpm) {
        return Err("Maximum heart rate must be between 120 and 230 bpm.".to_string());
    }
    persistence::write_setting(app, KEY_MAX, &bpm.to_string())?;
    MAX_BPM.store(bpm, Ordering::Relaxed);
    MAX_SET.store(true, Ordering::Relaxed);
    Ok(())
}

/// ZONE_TILT (Labs) is on: program zones may trim incline.
pub fn zone_incline_trim() -> bool {
    INCLINE_TRIM.load(Ordering::Relaxed)
}

/// Save the ZONE_TILT switch. The engine reads it on its next second.
pub fn set_zone_incline_trim(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_INCLINE_TRIM, if on { "true" } else { "false" })?;
    INCLINE_TRIM.store(on, Ordering::Relaxed);
    log::info!("ZONE_TILT {}", if on { "on" } else { "off" });
    Ok(())
}

/// Pick a sensor (from the last scan). It links within a tick.
pub fn pick_sensor(app: &AppHandle, id: String, name: String) -> Result<(), String> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err("No sensor was given.".to_string());
    }
    let name: String = name.chars().take(NAME_MAX).collect();
    persistence::write_setting(app, KEY_ID, &id)?;
    persistence::write_setting(app, KEY_NAME, &name)?;
    log::info!("HEART_LINK: picked a sensor");
    drop_link(Some(HeartRateSensor { id, name }));
    Ok(())
}

/// Stop reading and forget the sensor: the feature is off again.
pub fn forget_sensor(app: &AppHandle) -> Result<(), String> {
    persistence::write_setting(app, KEY_ID, "")?;
    persistence::write_setting(app, KEY_NAME, "")?;
    log::info!("HEART_LINK: sensor forgotten");
    drop_link(None);
    Ok(())
}

/// Replace the sensor: end any link and reset the reading.
fn drop_link(sensor: Option<HeartRateSensor>) {
    let old = with(|rt| {
        rt.generation += 1;
        rt.status = if sensor.is_some() {
            HeartStatus::Searching
        } else {
            HeartStatus::Off
        };
        rt.sensor = sensor;
        rt.error = None;
        rt.logged = None;
        rt.bpm = None;
        rt.bpm_at = None;
        rt.failed_at = None;
        rt.failures = 0;
        rt.connecting = false;
        rt.history.clear();
        rt.link.take()
    });
    if let Some(p) = old {
        tauri::async_runtime::spawn(async move {
            let _ = p.disconnect().await;
        });
    }
}

/// The latest reading while it is fresh, in beats per minute.
fn fresh_bpm(rt: &Runtime) -> Option<u16> {
    rt.bpm
        .zip(rt.bpm_at)
        .filter(|(_, at)| at.elapsed() < STALE_AFTER)
        .map(|(b, _)| b)
}

/// The heart rate to save with this second's sample; `None` without a
/// sensor or a fresh reading.
pub fn current_bpm() -> Option<u16> {
    with(|rt| fresh_bpm(rt))
}

/// Heart rate's part of the snapshot.
pub fn snapshot() -> HeartRateSnapshot {
    with(|rt| {
        let bpm = fresh_bpm(rt).map(u32::from);
        HeartRateSnapshot {
            sensor: rt.sensor.clone(),
            status: rt.status,
            label: status_label(rt.status).to_string(),
            message: status_message(rt.status, rt.error.as_deref(), bpm.is_some()),
            bpm,
            // No sensor, no chart.
            history: if rt.sensor.is_some() {
                rt.history.points()
            } else {
                Vec::new()
            },
            zone: None,
            scanning: rt.scanning,
            found: rt.found.clone(),
            max_bpm: u32::from(max_heart_rate()),
            max_bpm_set: MAX_SET.load(Ordering::Relaxed),
            incline_trim: zone_incline_trim(),
        }
    })
}

/// Scan for heart rate sensors; the result lands in `snapshot().found`.
pub async fn scan() -> Result<(), String> {
    if with(|rt| std::mem::replace(&mut rt.scanning, true)) {
        return Err("A scan is already running.".to_string());
    }
    let result = scan_inner().await;
    with(|rt| {
        rt.scanning = false;
        if let Ok(found) = &result {
            rt.found = found.clone();
        }
    });
    result
        .map(|_| ())
        .map_err(|raw| crate::link::describe(&raw).message)
}

async fn scan_inner() -> Result<Vec<HeartRateSensor>, String> {
    let service = short_uuid(HR_SERVICE);
    // The radio's scan is shared with the treadmill link: one at a time.
    let _scan = crate::bluetooth::SCAN_LOCK.lock().await;
    let central = find_central().await.map_err(|e| e.to_string())?;
    central
        .start_scan(ScanFilter {
            services: vec![service],
        })
        .await
        .map_err(|e| e.to_string())?;
    tokio::time::sleep(SCAN_TIME).await;
    let _ = central.stop_scan().await;

    let mut found: Vec<(i16, HeartRateSensor)> = Vec::new();
    for p in central.peripherals().await.map_err(|e| e.to_string())? {
        let Some(props) = p.properties().await.map_err(|e| e.to_string())? else {
            continue;
        };
        if !props.services.contains(&service) {
            continue;
        }
        let id = p.id().to_string();
        if found.iter().any(|(_, s)| s.id == id) {
            continue;
        }
        let name = props
            .local_name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Heart rate sensor".to_string());
        found.push((props.rssi.unwrap_or(-100), HeartRateSensor { id, name }));
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    Ok(found.into_iter().map(|(_, s)| s).collect())
}

/// The tick, for the life of the app.
pub fn spawn(_app: AppHandle) {
    tauri::async_runtime::spawn(async {
        // Sensors send about one reading a second (`STALE_AFTER`).
        let mut every = tokio::time::interval(Duration::from_secs(1));
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            record_history();
            tick().await;
        }
    });
}

/// Add this second's reading (or a gap) to the chart while a sensor is picked.
fn record_history() {
    let at_ms = u64::try_from(CLOCK.elapsed().as_millis()).unwrap_or(u64::MAX);
    with(|rt| {
        if rt.sensor.is_some() {
            let bpm = fresh_bpm(rt);
            rt.history.record(at_ms, bpm);
        }
    });
}

async fn tick() {
    let start = with(|rt| {
        let go = rt.sensor.is_some()
            && rt.link.is_none()
            && !rt.connecting
            && rt
                .failed_at
                .is_none_or(|t| t.elapsed() >= retry_after(rt.failures));
        if go {
            rt.connecting = true;
            // A sensor that is not broadcasting keeps showing as waiting
            // while the retry looks, so the text does not flicker.
            if rt.status != HeartStatus::Waiting {
                rt.status = HeartStatus::Searching;
            }
        }
        go.then(|| (rt.sensor.clone(), rt.generation))
    });
    let Some((Some(sensor), generation)) = start else {
        return;
    };
    if let Err((step, raw)) = connect(&sensor, generation).await {
        with(|rt| {
            if rt.generation != generation {
                return;
            }
            rt.connecting = false;
            rt.link = None;
            if raw == SCAN_BUSY {
                return;
            }
            let failure = describe_failure(step, &raw);
            // Logged once per new reason, so an absent watch doesn't fill the log.
            if rt.logged.as_deref() != Some(failure.log.as_str()) {
                log::warn!("HEART_LINK: {}", failure.log);
                rt.logged = Some(failure.log);
            }
            rt.status = if failure.waiting {
                HeartStatus::Waiting
            } else {
                HeartStatus::Error
            };
            rt.error = Some(failure.message);
            rt.failed_at = Some(Instant::now());
            rt.failures = rt.failures.saturating_add(1);
        });
    }
}

/// The step that failed and btleplug's raw reason.
type StepError = (Step, String);

async fn find(sensor: &HeartRateSensor) -> Result<Peripheral, StepError> {
    // The scan is shared with the treadmill link, which goes first: skip
    // this tick rather than wait, or stop its scan halfway.
    let Ok(_scan) = crate::bluetooth::SCAN_LOCK.try_lock() else {
        return Err((Step::Find, SCAN_BUSY.to_string()));
    };
    let fail = |e: &dyn std::fmt::Display| (Step::Find, e.to_string());
    let central = find_central().await.map_err(|e| fail(&e))?;
    central
        .start_scan(ScanFilter {
            services: vec![short_uuid(HR_SERVICE)],
        })
        .await
        .map_err(|e| fail(&e))?;
    let want = normalize_id(&sensor.id);
    let started = Instant::now();
    let found = loop {
        let list = central.peripherals().await.map_err(|e| fail(&e))?;
        if let Some(p) = list
            .into_iter()
            .find(|p| ids_match(&normalize_id(&p.id().to_string()), &want))
        {
            break Some(p);
        }
        if started.elapsed() >= FIND_TIME {
            break None;
        }
        // Half the sensor's one-second advertising, so it is seen on its first advert.
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    let _ = central.stop_scan().await;
    found.ok_or_else(|| (Step::Find, NOT_FOUND.to_string()))
}

async fn connect(sensor: &HeartRateSensor, generation: u64) -> Result<(), StepError> {
    let p = find(sensor).await?;
    p.connect()
        .await
        .map_err(|e| (Step::Connect, e.to_string()))?;
    p.discover_services()
        .await
        .map_err(|e| (Step::DiscoverServices, e.to_string()))?;
    let measurement = short_uuid(HR_MEASUREMENT);
    let characteristic = p
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == measurement)
        .ok_or_else(|| (Step::DiscoverServices, NO_MEASUREMENT.to_string()))?;
    p.subscribe(&characteristic)
        .await
        .map_err(|e| (Step::Subscribe, e.to_string()))?;
    let mut stream = p
        .notifications()
        .await
        .map_err(|e| (Step::Subscribe, e.to_string()))?;

    let current = with(|rt| {
        let current = rt.generation == generation;
        if current {
            rt.connecting = false;
            rt.link = Some(p.clone());
            rt.status = HeartStatus::Connected;
            rt.error = None;
            rt.failures = 0;
            rt.logged = None;
        }
        current
    });
    if !current {
        let _ = p.disconnect().await;
        return Ok(());
    }
    log::info!("HEART_LINK: linked");

    tauri::async_runtime::spawn(async move {
        while let Some(data) = stream.next().await {
            if data.uuid != measurement {
                continue;
            }
            let bpm = parse_measurement(&data.value);
            let stale = with(|rt| {
                if rt.generation != generation {
                    return true;
                }
                rt.bpm = bpm;
                rt.bpm_at = bpm.map(|_| Instant::now());
                false
            });
            if stale {
                return;
            }
        }
        // The stream ended: the sensor went away. Retry after a pause.
        with(|rt| {
            if rt.generation != generation {
                return;
            }
            log::info!("HEART_LINK: sensor went away");
            rt.link = None;
            rt.bpm = None;
            // The watch stopped broadcasting: waiting, not a fault.
            rt.status = HeartStatus::Waiting;
            rt.error = None;
            rt.failed_at = Some(Instant::now());
            rt.failures = 1;
        });
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_maximum_heart_rate_is_120_to_230_bpm() {
        assert!(valid_max(DEFAULT_MAX_BPM));
        for bpm in [120, 185, 230] {
            assert!(valid_max(bpm), "{}", bpm);
        }
        for bpm in [0, 119, 231, 300] {
            assert!(!valid_max(bpm), "{}", bpm);
        }
    }

    #[test]
    fn retries_back_off_to_a_minute() {
        assert_eq!(retry_after(0), Duration::from_secs(10));
        assert_eq!(retry_after(1), Duration::from_secs(10));
        assert_eq!(retry_after(2), Duration::from_secs(20));
        assert_eq!(retry_after(3), Duration::from_secs(40));
        assert_eq!(retry_after(4), RETRY_MAX);
        assert_eq!(retry_after(u32::MAX), RETRY_MAX);
    }

    #[test]
    fn the_uuids_are_the_standard_ones() {
        assert_eq!(
            short_uuid(HR_SERVICE).to_string(),
            "0000180d-0000-1000-8000-00805f9b34fb"
        );
        assert_eq!(
            short_uuid(HR_MEASUREMENT).to_string(),
            "00002a37-0000-1000-8000-00805f9b34fb"
        );
    }
}
