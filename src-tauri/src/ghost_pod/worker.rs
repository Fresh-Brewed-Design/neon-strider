//! GHOST_POD's runtime: the Labs setting (`ghost_pod_enabled`, off by
//! default), the pod's status, and the 1 s tick that sends the belt to the
//! watch. The GATT server is `platform::peripheral_*`.

use super::{
    control_point, encode_measurement, feature_value, frame_for, status_message, ControlRequest,
    GhostPodSnapshot, PodMeter, PodStatus, RscFrame, RSC_FEATURE, RSC_MEASUREMENT, RSC_SERVICE,
    SC_CONTROL_POINT,
};
use crate::bluetooth::ORCHESTRATOR;
use crate::persistence;
use crate::platform::{
    self, sig_uuid, PeripheralCharacteristic, PeripheralEvent, PeripheralEvents, PeripheralService,
};
use crate::wrist_link;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::AppHandle;

const KEY_ENABLED: &str = "ghost_pod_enabled";
const KEY_WRIST_LINK: &str = "wrist_link_enabled";
/// How often the pod sends a measurement.
const TICK: Duration = Duration::from_secs(1);
/// After a failure, try again this long later (while the switch is on).
const RETRY_AFTER: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Runtime {
    enabled: bool,
    /// The WRIST_LINK Labs switch: serve its service next to the pod.
    wrist_enabled: bool,
    /// Watches subscribed to WRIST_LINK's State.
    wrist_watches: u32,
    /// The services up now include WRIST_LINK.
    wrist_serving: bool,
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
}

static RT: LazyLock<Mutex<Runtime>> = LazyLock::new(|| Mutex::new(Runtime::default()));

fn with<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    f(&mut RT.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Read the switch. Call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let on = persistence::parse_setting(persistence::read_setting(app, KEY_ENABLED).as_deref())
        .unwrap_or(false);
    let wrist =
        persistence::parse_setting(persistence::read_setting(app, KEY_WRIST_LINK).as_deref())
            .unwrap_or(false);
    with(|rt| {
        rt.enabled = on;
        rt.wrist_enabled = wrist;
    });
    if on {
        log::info!("GHOST_POD: on; starting the pod");
    }
    if wrist {
        log::info!("WRIST_LINK: on");
    }
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
            rt.wrist_watches = 0;
            rt.wrist_serving = false;
            rt.speed_kmh = 0.0;
        });
    }
    Ok(())
}

/// The WRIST_LINK Labs switch. It rides on the pod's server, which restarts
/// within a tick to add or drop the service.
pub fn set_wrist_link(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_WRIST_LINK, if on { "true" } else { "false" })?;
    log::info!("WRIST_LINK {}", if on { "on" } else { "off" });
    let restart = with(|rt| {
        rt.wrist_enabled = on;
        rt.wrist_watches = 0;
        rt.running && rt.wrist_serving != on
    });
    if restart {
        platform::peripheral_stop();
        with(|rt| {
            rt.running = false;
            rt.wrist_serving = false;
            rt.watches = 0;
            rt.status = PodStatus::Starting;
            rt.failed_at = None;
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
            wrist_link: wrist_link::WristLinkSnapshot {
                enabled: rt.wrist_enabled,
                serving: rt.wrist_serving,
                watches: rt.wrist_watches,
                message: wrist_link::status_message(
                    rt.wrist_enabled,
                    rt.enabled,
                    rt.wrist_serving,
                    rt.wrist_watches,
                    if rt.status == PodStatus::Error {
                        rt.error.as_deref()
                    } else {
                        None
                    },
                ),
            },
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

/// What the server serves: the foot pod, and WRIST_LINK when it is on.
fn services(wrist: bool) -> Vec<PeripheralService> {
    let mut services = vec![PeripheralService {
        uuid: sig_uuid(RSC_SERVICE),
        characteristics: pod_characteristics(),
    }];
    if wrist {
        services.push(PeripheralService {
            uuid: wrist_link::SERVICE,
            characteristics: vec![PeripheralCharacteristic {
                uuid: wrist_link::STATE,
                read: Some(wrist_link::WristState::default().encode()),
                notify: true,
                indicate: false,
                write: false,
            }],
        });
    }
    services
}

fn pod_characteristics() -> Vec<PeripheralCharacteristic> {
    vec![
        PeripheralCharacteristic {
            uuid: sig_uuid(RSC_MEASUREMENT),
            read: None,
            notify: true,
            indicate: false,
            write: false,
        },
        PeripheralCharacteristic {
            uuid: sig_uuid(RSC_FEATURE),
            read: Some(feature_value()),
            notify: false,
            indicate: false,
            write: false,
        },
        PeripheralCharacteristic {
            uuid: sig_uuid(SC_CONTROL_POINT),
            read: None,
            notify: false,
            indicate: true,
            write: true,
        },
    ]
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
            rt.status = PodStatus::Error;
            rt.error = Some(why);
            rt.failed_at = Some(Instant::now());
        }),
        PeripheralEvent::Subscribers { uuid, count } => {
            log::info!("GHOST_POD: {} subscribed to {:x}", count, uuid);
            if uuid == wrist_link::STATE {
                with(|rt| rt.wrist_watches = count);
            }
            if uuid == sig_uuid(RSC_MEASUREMENT) {
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
                "GHOST_POD: write to {:x}: {}",
                uuid,
                crate::traffic::to_hex(&value)
            );
            if uuid == sig_uuid(SC_CONTROL_POINT) {
                let (request, response) = control_point(&value);
                if let ControlRequest::SetTotal(m) = request {
                    with(|rt| rt.meter.total_m = m);
                }
                if let Err(e) = platform::peripheral_notify(sig_uuid(SC_CONTROL_POINT), &response) {
                    log::warn!("GHOST_POD: control point reply failed: {}", e);
                }
            }
        }
    }
}

async fn tick() {
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

    let (frame, wrist) = {
        let state = ORCHESTRATOR.lock().await;
        (frame_for(&state), wrist_link::state_for(&state))
    };
    let now = Instant::now();
    let bytes = with(|rt| {
        let dt = rt.last_tick.map_or(0.0, |t| (now - t).as_secs_f64());
        rt.last_tick = Some(now);
        rt.meter.advance(frame.speed_mps, dt);
        rt.speed_kmh = frame.speed_mps * 3.6;
        encode_measurement(&RscFrame {
            total_m: rt.meter.total_m,
            ..frame
        })
    });
    if let Err(e) = platform::peripheral_notify(sig_uuid(RSC_MEASUREMENT), &bytes) {
        log::warn!("GHOST_POD: measurement not sent: {}", e);
    }
    if with(|rt| rt.wrist_serving) {
        if let Err(e) = platform::peripheral_notify(wrist_link::STATE, &wrist.encode()) {
            log::warn!("WRIST_LINK: state not sent: {}", e);
        }
    }
}

async fn start_pod() {
    let events: PeripheralEvents = Arc::new(on_event);
    let wrist = with(|rt| rt.wrist_enabled);
    let result =
        crate::util::blocking(move || platform::peripheral_start(&services(wrist), events)).await;
    let switched_off = with(|rt| {
        rt.starting = false;
        match &result {
            Ok(()) => {
                rt.running = true;
                rt.wrist_serving = wrist;
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
            rt.wrist_serving = false;
            rt.status = PodStatus::Off;
        });
    }
}
