//! GHOST_POD's runtime: the Labs settings (`ghost_pod_enabled` and
//! `ghost_pod_power`, both off by default), the pod's status, and the 1 s
//! tick that sends the belt to the watch. The GATT server is
//! `platform::peripheral_*`, one provider per service: the foot pod and
//! power each come and go on their own, so switching power never drops the
//! watch's foot pod link.

use super::{
    control_point, encode_measurement, encode_power, feature_value, frame_for, power_feature_value,
    power_for, sensor_location_value, status_message, ControlRequest, GhostPodSnapshot, PodMeter,
    PodStatus, RscFrame, CP_FEATURE, CP_MEASUREMENT, CP_SERVICE, RSC_FEATURE, RSC_MEASUREMENT,
    RSC_SERVICE, SC_CONTROL_POINT, SENSOR_LOCATION,
};
use crate::bluetooth::ORCHESTRATOR;
use crate::persistence;
use crate::platform::{
    self, sig_uuid, uuid_text, PeripheralCharacteristic, PeripheralEvent, PeripheralEvents,
    PeripheralService,
};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::AppHandle;

const KEY_ENABLED: &str = "ghost_pod_enabled";
const KEY_POWER: &str = "ghost_pod_power";
/// How often the pod sends a measurement.
/// Foot pods send RSC measurements about once a second; watches expect that rate.
const TICK: Duration = Duration::from_secs(1);
/// After a failure, try again this long later (while the switch is on).
/// A failed GATT server is a radio in use or switched off: twice a minute finds
/// it again soon without retrying in a tight loop.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// One of the services the pod serves, each on its own provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// The RSC foot pod, the watch's native sensor link.
    Pod,
    /// Cycling Power, with the power switch.
    Power,
}

impl Part {
    const ALL: [Part; 2] = [Part::Pod, Part::Power];

    fn uuid(self) -> u128 {
        match self {
            Part::Pod => sig_uuid(RSC_SERVICE),
            Part::Power => sig_uuid(CP_SERVICE),
        }
    }

    fn of(service: u128) -> Option<Part> {
        Part::ALL.into_iter().find(|p| p.uuid() == service)
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Which services are served (or wanted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Parts {
    pod: bool,
    power: bool,
}

impl Parts {
    fn has(self, part: Part) -> bool {
        match part {
            Part::Pod => self.pod,
            Part::Power => self.power,
        }
    }

    fn set(&mut self, part: Part, on: bool) {
        match part {
            Part::Pod => self.pod = on,
            Part::Power => self.power = on,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Add(Part),
    Remove(Part),
}

/// The steps from what is served to what is wanted. The foot pod goes up
/// first and down last, and a change to power alone never touches it, so
/// the watch's foot pod link survives the power switch.
fn plan(served: Parts, wanted: Parts) -> Vec<Step> {
    if !wanted.pod {
        return [Part::Power, Part::Pod]
            .into_iter()
            .filter(|p| served.has(*p))
            .map(Step::Remove)
            .collect();
    }
    let mut out = Vec::new();
    if !served.pod {
        out.push(Step::Add(Part::Pod));
    }
    match (served.power, wanted.power) {
        (false, true) => out.push(Step::Add(Part::Power)),
        (true, false) => out.push(Step::Remove(Part::Power)),
        _ => {}
    }
    out
}

#[derive(Default)]
struct Runtime {
    enabled: bool,
    /// The power switch.
    power_enabled: bool,
    /// The services up now.
    served: Parts,
    /// A step is in flight.
    busy: bool,
    status: PodStatus,
    error: Option<String>,
    watches: u32,
    meter: PodMeter,
    last_tick: Option<Instant>,
    /// When adding each service last failed (indexed by `Part`).
    failed_at: [Option<Instant>; 2],
    /// Windows aborted the foot pod's advertisement at this time and hasn't
    /// started it again. Windows often reports Aborted and then Started a
    /// moment later, so only an abort that lasts gets the pod re-added.
    pod_aborted_at: Option<Instant>,
    speed_kmh: f32,
    power_watches: u32,
    power_w: u16,
}

impl Runtime {
    /// What should be served now.
    fn wanted(&self) -> Parts {
        Parts {
            pod: self.enabled,
            power: self.enabled && self.power_enabled,
        }
    }

    /// Adding `part` failed less than `RETRY_AFTER` ago.
    fn backing_off(&self, part: Part, now: Instant) -> bool {
        self.failed_at[part.index()].is_some_and(|t| now.duration_since(t) < RETRY_AFTER)
    }

    /// Forget the watches and values of a service that went away.
    fn dropped(&mut self, part: Part) {
        self.served.set(part, false);
        match part {
            Part::Pod => {
                self.watches = 0;
                self.pod_aborted_at = None;
            }
            Part::Power => {
                self.power_watches = 0;
                self.power_w = 0;
            }
        }
    }
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

/// The power switch. While the pod runs, the next tick adds or removes the
/// Cycling Power service alone; the foot pod stays up.
pub fn set_power(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_POWER, if on { "true" } else { "false" })?;
    log::info!("GHOST_POD power {}", if on { "on" } else { "off" });
    with(|rt| {
        rt.power_enabled = on;
        rt.failed_at[Part::Power.index()] = None;
    });
    Ok(())
}

/// The Labs switch. Off stops the pod at once; on starts it within a tick.
pub fn set_enabled(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_ENABLED, if on { "true" } else { "false" })?;
    log::info!("GHOST_POD {}", if on { "on" } else { "off" });
    with(|rt| {
        rt.enabled = on;
        rt.failed_at = [None; 2];
        rt.error = None;
    });
    if !on {
        stop_all();
    }
    Ok(())
}

/// Stop every service and forget what they had.
fn stop_all() {
    platform::peripheral_stop();
    with(|rt| {
        for part in Part::ALL {
            rt.dropped(part);
        }
        rt.status = PodStatus::Off;
        rt.speed_kmh = 0.0;
    });
}

/// GHOST_POD's part of the snapshot.
pub fn snapshot() -> GhostPodSnapshot {
    with(|rt| {
        let status = if rt.enabled {
            rt.status
        } else {
            PodStatus::Off
        };
        let serving = status != PodStatus::Off && rt.served.pod && rt.served.power;
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
        uuid: sig_uuid(uuid),
        read: None,
        notify: false,
        indicate: false,
        write: false,
    }
}

/// The service a part serves.
fn service(part: Part) -> PeripheralService {
    match part {
        Part::Pod => PeripheralService {
            name: "RSC",
            uuid: part.uuid(),
            advertise: true,
            beacon: None,
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
        },
        Part::Power => PeripheralService {
            name: "POWER",
            uuid: part.uuid(),
            advertise: true,
            beacon: None,
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
        },
    }
}

/// From a Bluetooth thread. Every write and subscription is logged (the
/// platform logs each one with its service): a test on the real watch shows
/// whether it sends anything when its activity pauses.
fn on_event(event: PeripheralEvent) {
    match event {
        PeripheralEvent::Advertising { service } => with(|rt| {
            if Part::of(service) == Some(Part::Pod) {
                rt.pod_aborted_at = None;
                rt.error = None;
                if rt.status != PodStatus::Connected {
                    rt.status = PodStatus::Advertising;
                }
            }
        }),
        PeripheralEvent::Failed { service, why } => with(|rt| {
            log::warn!(
                "GHOST_POD: {} advertisement aborted: {}",
                uuid_text(service),
                why
            );
            if Part::of(service) == Some(Part::Pod) {
                rt.pod_aborted_at = Some(Instant::now());
                rt.status = PodStatus::Error;
                rt.error = Some(why);
            }
        }),
        PeripheralEvent::Subscribers { uuid, count } => {
            if uuid == sig_uuid(CP_MEASUREMENT) {
                with(|rt| rt.power_watches = count);
            }
            if uuid == sig_uuid(RSC_MEASUREMENT) {
                with(|rt| {
                    rt.watches = count;
                    if rt.served.pod || rt.busy {
                        rt.status = if count > 0 {
                            PodStatus::Connected
                        } else if rt.pod_aborted_at.is_some() {
                            PodStatus::Error
                        } else {
                            PodStatus::Advertising
                        };
                    }
                });
            }
        }
        PeripheralEvent::Write { uuid, value } => {
            log::info!(
                "GHOST_POD: write to {}: {}",
                uuid_text(uuid),
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

/// The steps to take now: `plan`, less the adds still backing off after a
/// failure, plus a fresh foot pod if Windows aborted its advertisement and
/// never started it again.
fn steps_now(rt: &Runtime, now: Instant) -> Vec<Step> {
    let mut steps: Vec<Step> = plan(rt.served, rt.wanted())
        .into_iter()
        .filter(|s| !matches!(s, Step::Add(p) if rt.backing_off(*p, now)))
        .collect();
    let stuck = rt
        .pod_aborted_at
        .is_some_and(|t| now.duration_since(t) >= RETRY_AFTER);
    if rt.served.pod && rt.wanted().pod && stuck {
        steps.insert(0, Step::Add(Part::Pod));
    }
    steps
}

async fn tick() {
    let steps = with(|rt| {
        if rt.busy {
            Vec::new()
        } else {
            steps_now(rt, Instant::now())
        }
    });
    if !steps.is_empty() {
        apply(steps).await;
    }
    if !with(|rt| rt.served.pod) {
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
        rt.power_w = if rt.served.power { watts } else { 0 };
        let bytes = encode_measurement(&RscFrame {
            total_m: rt.meter.total_m,
            ..frame
        });
        (bytes, rt.served.power.then(|| encode_power(watts)))
    });
    if let Err(e) = platform::peripheral_notify(sig_uuid(RSC_MEASUREMENT), &bytes) {
        log::warn!("GHOST_POD: measurement not sent: {}", e);
    }
    if let Some(power) = power {
        if let Err(e) = platform::peripheral_notify(sig_uuid(CP_MEASUREMENT), &power) {
            log::warn!("GHOST_POD: power not sent: {}", e);
        }
    }
}

/// Add and remove services, one provider at a time. A foot pod that fails
/// to start stops the rest: power rides on it.
async fn apply(steps: Vec<Step>) {
    with(|rt| rt.busy = true);
    for step in steps {
        match step {
            Step::Add(part) => {
                with(|rt| {
                    if part == Part::Pod {
                        rt.pod_aborted_at = None;
                        rt.error = None;
                        if !rt.served.pod {
                            rt.status = PodStatus::Starting;
                        }
                    }
                });
                let events: PeripheralEvents = Arc::new(on_event);
                let svc = service(part);
                let result =
                    crate::util::blocking(move || platform::peripheral_add(&svc, events)).await;
                let ok = with(|rt| match result {
                    Ok(()) => {
                        rt.served.set(part, true);
                        rt.failed_at[part.index()] = None;
                        if part == Part::Pod {
                            rt.last_tick = None;
                        }
                        true
                    }
                    Err(why) => {
                        log::warn!(
                            "GHOST_POD: could not start {}: {}",
                            uuid_text(part.uuid()),
                            why
                        );
                        rt.dropped(part);
                        rt.failed_at[part.index()] = Some(Instant::now());
                        if part == Part::Pod {
                            rt.status = PodStatus::Error;
                            rt.error = Some(why);
                        }
                        false
                    }
                });
                if !ok && part == Part::Pod {
                    break;
                }
            }
            Step::Remove(part) => {
                let uuid = part.uuid();
                let _ = crate::util::blocking(move || {
                    platform::peripheral_remove(uuid);
                    Ok(())
                })
                .await;
                with(|rt| {
                    rt.dropped(part);
                    if part == Part::Pod {
                        rt.status = PodStatus::Off;
                    }
                });
            }
        }
    }
    // Switched off while a service was starting.
    let switched_off = with(|rt| {
        rt.busy = false;
        !rt.enabled && rt.served != Parts::default()
    });
    if switched_off {
        stop_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 16-bit SIG numbers of a service and its characteristics.
    fn sig(part: Part) -> (u16, Vec<u16>) {
        let short = |u: u128| (u >> 96) as u16;
        let s = service(part);
        (
            short(s.uuid),
            s.characteristics.iter().map(|c| short(c.uuid)).collect(),
        )
    }

    fn parts(pod: bool, power: bool) -> Parts {
        Parts { pod, power }
    }

    #[test]
    fn the_foot_pod_and_power_services_carry_their_characteristics() {
        assert_eq!(sig(Part::Pod), (0x1814, vec![0x2A53, 0x2A54, 0x2A55]));
        assert_eq!(sig(Part::Power), (0x1818, vec![0x2A63, 0x2A65, 0x2A5D]));
    }

    #[test]
    fn both_services_are_advertised_and_neither_has_a_beacon() {
        for part in Part::ALL {
            let s = service(part);
            assert!(s.advertise, "{part:?}");
            assert!(s.beacon.is_none(), "{part:?}");
        }
    }

    #[test]
    fn power_measurement_notifies_and_its_feature_and_location_read() {
        let power = service(Part::Power);
        let [measurement, feature, location] = &power.characteristics[..] else {
            panic!("three characteristics");
        };
        assert!(measurement.notify && measurement.read.is_none() && !measurement.write);
        assert_eq!(feature.read.as_deref(), Some(&[0u8, 0, 0, 0][..]));
        assert_eq!(location.read.as_deref(), Some(&[0x01u8][..]));
        assert!(!feature.notify && !location.notify);
    }

    #[test]
    fn switching_power_never_touches_the_foot_pod() {
        assert_eq!(
            plan(parts(true, false), parts(true, true)),
            vec![Step::Add(Part::Power)]
        );
        assert_eq!(
            plan(parts(true, true), parts(true, false)),
            vec![Step::Remove(Part::Power)]
        );
        assert!(plan(parts(true, true), parts(true, true)).is_empty());
        assert!(plan(parts(true, false), parts(true, false)).is_empty());
    }

    #[test]
    fn the_foot_pod_goes_up_first_and_down_last() {
        assert_eq!(
            plan(Parts::default(), parts(true, true)),
            vec![Step::Add(Part::Pod), Step::Add(Part::Power)]
        );
        assert_eq!(
            plan(parts(true, true), Parts::default()),
            vec![Step::Remove(Part::Power), Step::Remove(Part::Pod)]
        );
    }

    #[test]
    fn a_failed_service_waits_before_it_is_tried_again() {
        let now = Instant::now();
        let mut rt = Runtime {
            enabled: true,
            power_enabled: true,
            served: parts(true, false),
            ..Runtime::default()
        };
        rt.failed_at[Part::Power.index()] = Some(now);
        assert!(steps_now(&rt, now + Duration::from_secs(5)).is_empty());
        assert_eq!(
            steps_now(&rt, now + RETRY_AFTER),
            vec![Step::Add(Part::Power)]
        );
    }

    #[test]
    fn only_an_abort_that_lasts_re_adds_the_foot_pod() {
        let now = Instant::now();
        let rt = Runtime {
            enabled: true,
            power_enabled: true,
            served: parts(true, true),
            pod_aborted_at: Some(now),
            ..Runtime::default()
        };
        assert!(steps_now(&rt, now + Duration::from_secs(1)).is_empty());
        assert_eq!(
            steps_now(&rt, now + RETRY_AFTER),
            vec![Step::Add(Part::Pod)]
        );
    }

    #[test]
    fn events_find_their_service() {
        assert_eq!(Part::of(sig_uuid(0x1814)), Some(Part::Pod));
        assert_eq!(Part::of(sig_uuid(0x1818)), Some(Part::Power));
        assert_eq!(Part::of(sig_uuid(0x180d)), None);
    }
}
