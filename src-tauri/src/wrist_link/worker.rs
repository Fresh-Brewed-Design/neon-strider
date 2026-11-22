//! WRIST_LINK's runtime: the Labs setting (`wrist_link_enabled`, off by
//! default), the link's status, and the 1 s tick that sends State to the
//! watch. The GATT server is `platform::peripheral_*`; the switch adds or
//! removes WRIST_LINK's provider (and its beacon) alone.

use super::{
    state_for, status_message, WristLinkSnapshot, WristState, BEACON_COMPANY, BEACON_MARKER,
    SERVICE, STATE,
};
use crate::bluetooth::ORCHESTRATOR;
use crate::persistence;
use crate::platform::{
    self, uuid_text, PeripheralBeacon, PeripheralCharacteristic, PeripheralEvent, PeripheralEvents,
    PeripheralService,
};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::AppHandle;

const KEY_ENABLED: &str = "wrist_link_enabled";
/// How often State is sent: the data field redraws once a second.
const TICK: Duration = Duration::from_secs(1);
/// After a failure, try again this long later (while the switch is on).
/// A failed GATT server is a radio in use or switched off: twice a minute finds
/// it again soon without retrying in a tight loop.
const RETRY_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Add,
    Remove,
}

#[derive(Default)]
struct Runtime {
    /// The Labs switch.
    enabled: bool,
    /// The service is up.
    served: bool,
    /// A step is in flight.
    busy: bool,
    /// Watches subscribed to State.
    watches: u32,
    /// Why the service failed, for its status line.
    error: Option<String>,
    /// When adding the service last failed.
    failed_at: Option<Instant>,
}

/// The step to take now: add the service when it is wanted and not backing
/// off after a failure, remove it when it is not wanted.
fn step_now(rt: &Runtime, now: Instant) -> Option<Step> {
    match (rt.served, rt.enabled) {
        (false, true) => {
            let backing_off = rt
                .failed_at
                .is_some_and(|t| now.duration_since(t) < RETRY_AFTER);
            (!backing_off).then_some(Step::Add)
        }
        (true, false) => Some(Step::Remove),
        _ => None,
    }
}

static RT: LazyLock<Mutex<Runtime>> = LazyLock::new(|| Mutex::new(Runtime::default()));

fn with<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    f(&mut RT.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Read the switch. Call once after the DB is initialised.
pub fn init(app: &AppHandle) {
    let on = persistence::parse_setting(persistence::read_setting(app, KEY_ENABLED).as_deref())
        .unwrap_or(false);
    with(|rt| rt.enabled = on);
    if on {
        log::info!("WRIST_LINK: on; starting the link");
    }
}

/// The Labs switch. Off stops the link at once; on starts it within a tick.
pub fn set_enabled(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_ENABLED, if on { "true" } else { "false" })?;
    log::info!("WRIST_LINK {}", if on { "on" } else { "off" });
    with(|rt| {
        rt.enabled = on;
        rt.error = None;
        rt.failed_at = None;
    });
    if !on {
        stop_all();
    }
    Ok(())
}

/// Stop the service and forget what it had.
fn stop_all() {
    platform::peripheral_stop();
    with(|rt| {
        rt.served = false;
        rt.watches = 0;
    });
}

/// WRIST_LINK's part of the snapshot.
pub fn snapshot() -> WristLinkSnapshot {
    with(|rt| WristLinkSnapshot {
        enabled: rt.enabled,
        serving: rt.served,
        watches: rt.watches,
        failed: rt.enabled && rt.error.is_some(),
        message: status_message(rt.enabled, rt.served, rt.watches, rt.error.as_deref()),
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

/// The service: served and discoverable, but left out of the GATT
/// advertisement. A 128-bit UUID in a Windows GATT advertisement was aborted
/// on the first hardware test (docs/wrist-link.md).
///
/// Its beacon is a legacy advertisement with the Neon Strider marker, which
/// is what the watch's Connect IQ scan is known to hear (hardware test,
/// 2026-11-20).
fn service() -> PeripheralService {
    PeripheralService {
        name: "WRIST_LINK",
        uuid: SERVICE,
        advertise: false,
        beacon: Some(PeripheralBeacon {
            company: BEACON_COMPANY,
            data: BEACON_MARKER.to_vec(),
        }),
        characteristics: vec![PeripheralCharacteristic {
            uuid: STATE,
            read: Some(WristState::default().encode()),
            notify: true,
            indicate: false,
            write: false,
        }],
    }
}

/// From a Bluetooth thread. The platform logs every subscription and read
/// with its service.
fn on_event(event: PeripheralEvent) {
    match event {
        PeripheralEvent::Advertising { service } if service == SERVICE => {
            with(|rt| rt.error = None)
        }
        PeripheralEvent::Failed { service, why } if service == SERVICE => {
            log::warn!(
                "WRIST_LINK: {} advertisement aborted: {}",
                uuid_text(service),
                why
            );
            with(|rt| rt.error = Some(why));
        }
        PeripheralEvent::Subscribers { uuid, count } if uuid == STATE => {
            with(|rt| rt.watches = count)
        }
        PeripheralEvent::Write { uuid, value } => log::info!(
            "WRIST_LINK: write to {}: {}",
            uuid_text(uuid),
            crate::traffic::to_hex(&value)
        ),
        _ => {}
    }
}

async fn tick() {
    let step = with(|rt| {
        if rt.busy {
            None
        } else {
            step_now(rt, Instant::now())
        }
    });
    if let Some(step) = step {
        apply(step).await;
    }
    if !with(|rt| rt.served) {
        return;
    }
    let state = state_for(&*ORCHESTRATOR.lock().await);
    if let Err(e) = platform::peripheral_notify(STATE, &state.encode()) {
        log::warn!("WRIST_LINK: state not sent: {}", e);
    }
}

async fn apply(step: Step) {
    with(|rt| {
        rt.busy = true;
        rt.error = None;
    });
    match step {
        Step::Add => {
            let events: PeripheralEvents = Arc::new(on_event);
            let svc = service();
            let result =
                crate::util::blocking(move || platform::peripheral_add(&svc, events)).await;
            with(|rt| match result {
                Ok(()) => {
                    rt.served = true;
                    rt.failed_at = None;
                }
                Err(why) => {
                    log::warn!(
                        "WRIST_LINK: could not start {}: {}",
                        uuid_text(SERVICE),
                        why
                    );
                    rt.served = false;
                    rt.watches = 0;
                    rt.failed_at = Some(Instant::now());
                    rt.error = Some(why);
                }
            });
        }
        Step::Remove => {
            let _ = crate::util::blocking(|| {
                platform::peripheral_remove(SERVICE);
                Ok(())
            })
            .await;
            with(|rt| {
                rt.served = false;
                rt.watches = 0;
            });
        }
    }
    // Switched off while the service was starting.
    let switched_off = with(|rt| {
        rt.busy = false;
        !rt.enabled && rt.served
    });
    if switched_off {
        stop_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_beacon_fits_a_legacy_advertisement() {
        let beacon = service().beacon.expect("WRIST_LINK beacon");
        assert_eq!(beacon.company, 0xffff);
        assert_eq!(beacon.data, b"NSTR\x01".to_vec());
        // Flags (3) + length, type, company (4) + "NSTR" and the version.
        assert_eq!(beacon.air_len(), 12);
        assert!(beacon.air_len() <= 31);
    }

    #[test]
    fn wrist_link_is_served_but_left_out_of_the_advertisement() {
        let wrist = service();
        assert_eq!(wrist.uuid, SERVICE);
        assert_eq!(wrist.characteristics[0].uuid, STATE);
        assert!(wrist.characteristics[0].notify);
        assert!(!wrist.advertise);
    }

    #[test]
    fn the_switch_alone_starts_and_stops_the_service() {
        let now = Instant::now();
        let on = Runtime {
            enabled: true,
            ..Runtime::default()
        };
        assert_eq!(step_now(&on, now), Some(Step::Add));
        let up = Runtime { served: true, ..on };
        assert_eq!(step_now(&up, now), None);
        let off = Runtime {
            enabled: false,
            served: true,
            ..Runtime::default()
        };
        assert_eq!(step_now(&off, now), Some(Step::Remove));
        assert_eq!(step_now(&Runtime::default(), now), None);
    }

    #[test]
    fn a_failed_service_waits_before_it_is_tried_again() {
        let now = Instant::now();
        let rt = Runtime {
            enabled: true,
            failed_at: Some(now),
            ..Runtime::default()
        };
        assert_eq!(step_now(&rt, now + Duration::from_secs(5)), None);
        assert_eq!(step_now(&rt, now + RETRY_AFTER), Some(Step::Add));
    }
}
