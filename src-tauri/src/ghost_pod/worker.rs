//! GHOST_POD's runtime: the Labs settings (`ghost_pod_enabled` and
//! `ghost_pod_power`, both off by default), the pod's status, and the 1 s
//! tick that sends the belt to the watch. The GATT server is
//! `platform::peripheral_*`.

use super::{
    control_point, encode_measurement, encode_power, feature_value, frame_for, power_feature_value,
    power_for, sensor_location_value, status_message, ControlRequest, GhostPodSnapshot, PodMeter,
    PodStatus, RscFrame, CP_FEATURE, CP_MEASUREMENT, CP_SERVICE, RSC_FEATURE, RSC_MEASUREMENT,
    RSC_SERVICE, SC_CONTROL_POINT, SENSOR_LOCATION,
};
use crate::bluetooth::ORCHESTRATOR;
use crate::persistence;
use crate::platform::{
    self, PeripheralCharacteristic, PeripheralEvent, PeripheralEvents, PeripheralService,
};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::AppHandle;

const KEY_ENABLED: &str = "ghost_pod_enabled";
const KEY_POWER: &str = "ghost_pod_power";
/// How often the pod sends a measurement.
const TICK: Duration = Duration::from_secs(1);
/// After a failure, try again this long later (while the switch is on).
const RETRY_AFTER: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Runtime {
    enabled: bool,
    /// The GATT server is up (advertising or serving a watch).
    running: bool,
    /// A start is in flight.
    starting: bool,
    status: PodStatus,
    error: Option<String>,
    watches: u32,
    meter: PodMeter,
    last_tick: Option<Instant>,
    failed_at: Option<Instant>,
    speed_kmh: f32,
    /// The power switch.
    power_enabled: bool,
    /// The running pod serves the Cycling Power service (it was started with
    /// the power switch on). The tick restarts the pod when the switch moves.
    serving_power: bool,
    power_watches: u32,
    power_w: u16,
}

static RT: LazyLock<Mutex<Runtime>> = LazyLock::new(|| Mutex::new(Runtime::default()));

fn with<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    f(&mut RT.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Read the switch. Call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let read = |key| {
        persistence::parse_setting(persistence::read_setting(app, key).as_deref()).unwrap_or(false)
    };
    let (on, power) = (read(KEY_ENABLED), read(KEY_POWER));
    with(|rt| {
        rt.enabled = on;
        rt.power_enabled = power;
    });
    if on {
        log::info!(
            "GHOST_POD: on{}; starting the pod",
            if power { " with power" } else { "" }
        );
    }
}

/// The power switch. While the pod runs, the next tick restarts it with or
/// without the Cycling Power service (the watch reconnects by itself).
pub fn set_power(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_POWER, if on { "true" } else { "false" })?;
    log::info!("GHOST_POD power {}", if on { "on" } else { "off" });
    with(|rt| rt.power_enabled = on);
    Ok(())
}

/// The Labs switch. Off stops the pod at once; on starts it within a tick.
pub fn set_enabled(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_ENABLED, if on { "true" } else { "false" })?;
    log::info!("GHOST_POD {}", if on { "on" } else { "off" });
    with(|rt| {
        rt.enabled = on;
        rt.failed_at = None;
        rt.error = None;
    });
    if !on {
        platform::peripheral_stop();
        with(|rt| {
            rt.running = false;
            rt.status = PodStatus::Off;
            rt.watches = 0;
            rt.speed_kmh = 0.0;
            rt.power_watches = 0;
            rt.power_w = 0;
        });
    }
    Ok(())
}

/// GHOST_POD's part of the snapshot.
pub fn snapshot() -> GhostPodSnapshot {
    with(|rt| {
        let status = if rt.enabled {
            rt.status
        } else {
            PodStatus::Off
        };
        let serving = status != PodStatus::Off && rt.running && rt.serving_power;
        GhostPodSnapshot {
            enabled: rt.enabled,
            status,
            label: super::status_label(status).to_string(),
            message: status_message(status, rt.error.as_deref()),
            watches: rt.watches,
            speed_kmh: if status == PodStatus::Off {
                0.0
            } else {
                rt.speed_kmh
            },
            power_enabled: rt.power_enabled,
            power_watches: if serving { rt.power_watches } else { 0 },
            power_w: if serving { rt.power_w } else { 0 },
        }
    })
}

/// The tick, for the life of the app.
pub fn spawn(_app: AppHandle) {
    tauri::async_runtime::spawn(async {
        let mut every = tokio::time::interval(TICK);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            tick().await;
        }
    });
}

fn characteristic(uuid: u16) -> PeripheralCharacteristic {
    PeripheralCharacteristic {
        uuid,
        read: None,
        notify: false,
        indicate: false,
        write: false,
    }
}

/// The foot pod, plus the power service when `power` is on.
fn services(power: bool) -> Vec<PeripheralService> {
    let mut out = vec![PeripheralService {
        uuid: RSC_SERVICE,
        characteristics: vec![
            PeripheralCharacteristic {
                notify: true,
                ..characteristic(RSC_MEASUREMENT)
            },
            PeripheralCharacteristic {
                read: Some(feature_value()),
                ..characteristic(RSC_FEATURE)
            },
            PeripheralCharacteristic {
                indicate: true,
                write: true,
                ..characteristic(SC_CONTROL_POINT)
            },
        ],
    }];
    if power {
        out.push(PeripheralService {
            uuid: CP_SERVICE,
            characteristics: vec![
                PeripheralCharacteristic {
                    notify: true,
                    ..characteristic(CP_MEASUREMENT)
                },
                PeripheralCharacteristic {
                    read: Some(power_feature_value()),
                    ..characteristic(CP_FEATURE)
                },
                PeripheralCharacteristic {
                    read: Some(sensor_location_value()),
                    ..characteristic(SENSOR_LOCATION)
                },
            ],
        });
    }
    out
}

/// From a Bluetooth thread. Every write and subscription is logged: a test on
/// the real watch shows whether it sends anything when its activity pauses.
fn on_event(event: PeripheralEvent) {
    match event {
        PeripheralEvent::Advertising => with(|rt| {
            log::info!("GHOST_POD: advertising");
            if rt.status != PodStatus::Connected {
                rt.status = PodStatus::Advertising;
            }
        }),
        PeripheralEvent::Failed(why) => with(|rt| {
            log::warn!("GHOST_POD: advertising stopped: {}", why);
            rt.running = false;
            rt.watches = 0;
            rt.power_watches = 0;
            rt.status = PodStatus::Error;
            rt.error = Some(why);
            rt.failed_at = Some(Instant::now());
        }),
        PeripheralEvent::Subscribers { uuid, count } => {
            log::info!("GHOST_POD: {} subscribed to {:04X}", count, uuid);
            if uuid == CP_MEASUREMENT {
                with(|rt| rt.power_watches = count);
            }
            if uuid == RSC_MEASUREMENT {
                with(|rt| {
                    rt.watches = count;
                    if rt.running || rt.starting {
                        rt.status = if count > 0 {
                            PodStatus::Connected
                        } else {
                            PodStatus::Advertising
                        };
                    }
                });
            }
        }
        PeripheralEvent::Write { uuid, value } => {
            log::info!(
                "GHOST_POD: write to {:04X}: {}",
                uuid,
                crate::traffic::to_hex(&value)
            );
            if uuid == SC_CONTROL_POINT {
                let (request, response) = control_point(&value);
                if let ControlRequest::SetTotal(m) = request {
                    with(|rt| rt.meter.total_m = m);
                }
                if let Err(e) = platform::peripheral_notify(SC_CONTROL_POINT, &response) {
                    log::warn!("GHOST_POD: control point reply failed: {}", e);
                }
            }
        }
    }
}

async fn tick() {
    // The power switch moved while the pod runs: stop it, and start it again
    // below with the new set of services.
    let restart = with(|rt| rt.running && rt.serving_power != rt.power_enabled);
    if restart {
        log::info!("GHOST_POD: restarting for the power switch");
        let _ = crate::util::blocking(|| {
            platform::peripheral_stop();
            Ok(())
        })
        .await;
        with(|rt| {
            rt.running = false;
            rt.watches = 0;
            rt.power_watches = 0;
            rt.power_w = 0;
        });
    }

    let (start, running) = with(|rt| {
        let start = rt.enabled
            && !rt.running
            && !rt.starting
            && rt.failed_at.is_none_or(|t| t.elapsed() >= RETRY_AFTER);
        if start {
            rt.starting = true;
            rt.status = PodStatus::Starting;
            rt.error = None;
        }
        (start, rt.running)
    });
    if start {
        start_pod().await;
        return;
    }
    if !running {
        return;
    }

    let (frame, watts) = {
        let state = ORCHESTRATOR.lock().await;
        (frame_for(&state), power_for(&state))
    };
    let now = Instant::now();
    let (bytes, power) = with(|rt| {
        let dt = rt.last_tick.map_or(0.0, |t| (now - t).as_secs_f64());
        rt.last_tick = Some(now);
        rt.meter.advance(frame.speed_mps, dt);
        rt.speed_kmh = frame.speed_mps * 3.6;
        rt.power_w = if rt.serving_power { watts } else { 0 };
        let bytes = encode_measurement(&RscFrame {
            total_m: rt.meter.total_m,
            ..frame
        });
        (bytes, rt.serving_power.then(|| encode_power(watts)))
    });
    if let Err(e) = platform::peripheral_notify(RSC_MEASUREMENT, &bytes) {
        log::warn!("GHOST_POD: measurement not sent: {}", e);
    }
    if let Some(power) = power {
        if let Err(e) = platform::peripheral_notify(CP_MEASUREMENT, &power) {
            log::warn!("GHOST_POD: power not sent: {}", e);
        }
    }
}

async fn start_pod() {
    let events: PeripheralEvents = Arc::new(on_event);
    let power = with(|rt| rt.power_enabled);
    let result =
        crate::util::blocking(move || platform::peripheral_start(&services(power), events)).await;
    let switched_off = with(|rt| {
        rt.starting = false;
        match &result {
            Ok(()) => {
                rt.running = true;
                rt.serving_power = power;
                rt.last_tick = None;
                rt.failed_at = None;
            }
            Err(why) => {
                log::warn!("GHOST_POD: could not start: {}", why);
                rt.status = PodStatus::Error;
                rt.error = Some(why.clone());
                rt.failed_at = Some(Instant::now());
            }
        }
        !rt.enabled
    });
    // Switched off while it was starting.
    if switched_off && result.is_ok() {
        platform::peripheral_stop();
        with(|rt| {
            rt.running = false;
            rt.status = PodStatus::Off;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuids(power: bool) -> Vec<(u16, Vec<u16>)> {
        services(power)
            .iter()
            .map(|s| (s.uuid, s.characteristics.iter().map(|c| c.uuid).collect()))
            .collect()
    }

    #[test]
    fn the_foot_pod_alone_until_power_is_on() {
        assert_eq!(uuids(false), vec![(0x1814, vec![0x2A53, 0x2A54, 0x2A55])]);
        assert_eq!(
            uuids(true),
            vec![
                (0x1814, vec![0x2A53, 0x2A54, 0x2A55]),
                (0x1818, vec![0x2A63, 0x2A65, 0x2A5D]),
            ]
        );
    }

    #[test]
    fn power_measurement_notifies_and_its_feature_and_location_read() {
        let power = &services(true)[1];
        let [measurement, feature, location] = &power.characteristics[..] else {
            panic!("three characteristics");
        };
        assert!(measurement.notify && measurement.read.is_none() && !measurement.write);
        assert_eq!(feature.read.as_deref(), Some(&[0u8, 0, 0, 0][..]));
        assert_eq!(location.read.as_deref(), Some(&[0x01u8][..]));
        assert!(!feature.notify && !location.notify);
    }
}
