//! GHOST_POD's runtime: the Labs settings (`ghost_pod_enabled` and
//! `ghost_pod_power`, both off by default), the pod's status, and the 1 s
//! tick that sends the belt to the watch. The GATT server is
//! `platform::peripheral_*`, one provider per service: the foot pod, power
//! and WRIST_LINK each come and go on their own, so switching power or
//! WRIST_LINK never drops the watch's foot pod link.

use super::{
    control_point, encode_measurement, encode_power, feature_value, frame_for, power_feature_value,
    power_for, sensor_location_value, status_message, ControlRequest, GhostPodSnapshot, PodMeter,
    PodStatus, RscFrame, CP_FEATURE, CP_MEASUREMENT, CP_SERVICE, RSC_FEATURE, RSC_MEASUREMENT,
    RSC_SERVICE, SC_CONTROL_POINT, SENSOR_LOCATION,
};
use crate::bluetooth::ORCHESTRATOR;
use crate::persistence;
use crate::platform::{
    self, sig_uuid, uuid_text, PeripheralBeacon, PeripheralCharacteristic, PeripheralEvent,
    PeripheralEvents, PeripheralService,
};
use crate::wrist_link;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::AppHandle;

const KEY_ENABLED: &str = "ghost_pod_enabled";
const KEY_POWER: &str = "ghost_pod_power";
const KEY_WRIST_LINK: &str = "wrist_link_enabled";
/// How often the pod sends a measurement.
const TICK: Duration = Duration::from_secs(1);
/// After a failure, try again this long later (while the switch is on).
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// One of the services the pod serves, each on its own provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// The RSC foot pod, the watch's native sensor link.
    Pod,
    /// Cycling Power, with the power switch.
    Power,
    /// WRIST_LINK, the data field's service.
    Wrist,
}

impl Part {
    const ALL: [Part; 3] = [Part::Pod, Part::Power, Part::Wrist];

    fn uuid(self) -> u128 {
        match self {
            Part::Pod => sig_uuid(RSC_SERVICE),
            Part::Power => sig_uuid(CP_SERVICE),
            Part::Wrist => wrist_link::SERVICE,
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
    wrist: bool,
}

impl Parts {
    fn has(self, part: Part) -> bool {
        match part {
            Part::Pod => self.pod,
            Part::Power => self.power,
            Part::Wrist => self.wrist,
        }
    }

    fn set(&mut self, part: Part, on: bool) {
        match part {
            Part::Pod => self.pod = on,
            Part::Power => self.power = on,
            Part::Wrist => self.wrist = on,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Add(Part),
    Remove(Part),
}

/// The steps from what is served to what is wanted. The foot pod goes up
/// first and down last, and a change to power or WRIST_LINK alone never
/// touches it, so the watch's foot pod link survives either switch.
fn plan(served: Parts, wanted: Parts) -> Vec<Step> {
    if !wanted.pod {
        return [Part::Wrist, Part::Power, Part::Pod]
            .into_iter()
            .filter(|p| served.has(*p))
            .map(Step::Remove)
            .collect();
    }
    let mut out = Vec::new();
    if !served.pod {
        out.push(Step::Add(Part::Pod));
    }
    for part in [Part::Power, Part::Wrist] {
        match (served.has(part), wanted.has(part)) {
            (false, true) => out.push(Step::Add(part)),
            (true, false) => out.push(Step::Remove(part)),
            _ => {}
        }
    }
    out
}

#[derive(Default)]
struct Runtime {
    enabled: bool,
    /// The WRIST_LINK Labs switch: serve its service next to the pod.
    wrist_enabled: bool,
    /// Watches subscribed to WRIST_LINK's State.
    wrist_watches: u32,
    /// Why WRIST_LINK's service failed, for its status line.
    wrist_error: Option<String>,
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
    failed_at: [Option<Instant>; 3],
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
            wrist: self.enabled && self.wrist_enabled,
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
            Part::Wrist => self.wrist_watches = 0,
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
    let (on, power, wrist) = (read(KEY_ENABLED), read(KEY_POWER), read(KEY_WRIST_LINK));
    with(|rt| {
        rt.enabled = on;
        rt.power_enabled = power;
        rt.wrist_enabled = wrist;
    });
    if on {
        log::info!(
            "GHOST_POD: on{}; starting the pod",
            if power { " with power" } else { "" }
        );
    }
    if wrist {
        log::info!("WRIST_LINK: on");
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
        rt.failed_at = [None; 3];
        rt.error = None;
        rt.wrist_error = None;
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

/// The WRIST_LINK Labs switch. The next tick adds or removes its service
/// alone; the foot pod stays up.
pub fn set_wrist_link(app: &AppHandle, on: bool) -> Result<(), String> {
    persistence::write_setting(app, KEY_WRIST_LINK, if on { "true" } else { "false" })?;
    log::info!("WRIST_LINK {}", if on { "on" } else { "off" });
    with(|rt| {
        rt.wrist_enabled = on;
        rt.wrist_error = None;
        rt.failed_at[Part::Wrist.index()] = None;
    });
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
        let serving = status != PodStatus::Off && rt.served.pod && rt.served.power;
        let wrist_serving = rt.served.pod && rt.served.wrist;
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
            wrist_link: wrist_link::WristLinkSnapshot {
                enabled: rt.wrist_enabled,
                serving: wrist_serving,
                watches: rt.wrist_watches,
                message: wrist_link::status_message(
                    rt.wrist_enabled,
                    rt.enabled,
                    wrist_serving,
                    rt.wrist_watches,
                    rt.wrist_error
                        .as_deref()
                        .or(if rt.status == PodStatus::Error {
                            rt.error.as_deref()
                        } else {
                            None
                        }),
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
        // Served and connectable, but left out of the advertisement: a second
        // advertisement next to the foot pod's was aborted on the first
        // hardware test, and the field finds the PC through the foot pod's
        // RSC advertisement anyway (docs/wrist-link.md).
        //
        // Its beacon is a legacy advertisement with the Neon Strider marker:
        // the field's scan never reported the foot pod's advertisement
        // (hardware test, 2026-11-20), and a legacy one is what the watch's
        // Connect IQ scan is known to hear.
        Part::Wrist => PeripheralService {
            name: "WRIST_LINK",
            uuid: part.uuid(),
            advertise: false,
            beacon: Some(PeripheralBeacon {
                company: wrist_link::BEACON_COMPANY,
                data: wrist_link::BEACON_MARKER.to_vec(),
            }),
            characteristics: vec![PeripheralCharacteristic {
                uuid: wrist_link::STATE,
                read: Some(wrist_link::WristState::default().encode()),
                notify: true,
                indicate: false,
                write: false,
            }],
        },
    }
}

/// From a Bluetooth thread. Every write and subscription is logged (the
/// platform logs each one with its service): a test on the real watch shows
/// whether it sends anything when its activity pauses.
fn on_event(event: PeripheralEvent) {
    match event {
        PeripheralEvent::Advertising { service } => with(|rt| match Part::of(service) {
            Some(Part::Pod) => {
                rt.pod_aborted_at = None;
                rt.error = None;
                if rt.status != PodStatus::Connected {
                    rt.status = PodStatus::Advertising;
                }
            }
            Some(Part::Wrist) => rt.wrist_error = None,
            _ => {}
        }),
        PeripheralEvent::Failed { service, why } => with(|rt| {
            log::warn!(
                "GHOST_POD: {} advertisement aborted: {}",
                uuid_text(service),
                why
            );
            match Part::of(service) {
                Some(Part::Pod) => {
                    rt.pod_aborted_at = Some(Instant::now());
                    rt.status = PodStatus::Error;
                    rt.error = Some(why);
                }
                Some(Part::Wrist) => rt.wrist_error = Some(why),
                _ => {}
            }
        }),
        PeripheralEvent::Subscribers { uuid, count } => {
            if uuid == wrist_link::STATE {
                with(|rt| rt.wrist_watches = count);
            }
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

    let (frame, watts, wrist) = {
        let state = ORCHESTRATOR.lock().await;
        (
            frame_for(&state),
            power_for(&state),
            wrist_link::state_for(&state),
        )
    };
    let now = Instant::now();
    let (bytes, power, wrist_on) = with(|rt| {
        let dt = rt.last_tick.map_or(0.0, |t| (now - t).as_secs_f64());
        rt.last_tick = Some(now);
        rt.meter.advance(frame.speed_mps, dt);
        rt.speed_kmh = frame.speed_mps * 3.6;
        rt.power_w = if rt.served.power { watts } else { 0 };
        let bytes = encode_measurement(&RscFrame {
            total_m: rt.meter.total_m,
            ..frame
        });
        (
            bytes,
            rt.served.power.then(|| encode_power(watts)),
            rt.served.wrist,
        )
    });
    if let Err(e) = platform::peripheral_notify(sig_uuid(RSC_MEASUREMENT), &bytes) {
        log::warn!("GHOST_POD: measurement not sent: {}", e);
    }
    if let Some(power) = power {
        if let Err(e) = platform::peripheral_notify(sig_uuid(CP_MEASUREMENT), &power) {
            log::warn!("GHOST_POD: power not sent: {}", e);
        }
    }
    if wrist_on {
        if let Err(e) = platform::peripheral_notify(wrist_link::STATE, &wrist.encode()) {
            log::warn!("WRIST_LINK: state not sent: {}", e);
        }
    }
}

/// Add and remove services, one provider at a time. A foot pod that fails
/// to start stops the rest: power and WRIST_LINK ride on it.
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
                    if part == Part::Wrist {
                        rt.wrist_error = None;
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
                        match part {
                            Part::Pod => {
                                rt.status = PodStatus::Error;
                                rt.error = Some(why);
                            }
                            Part::Wrist => rt.wrist_error = Some(why),
                            Part::Power => {}
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

    fn parts(pod: bool, power: bool, wrist: bool) -> Parts {
        Parts { pod, power, wrist }
    }

    #[test]
    fn the_foot_pod_and_power_services_carry_their_characteristics() {
        assert_eq!(sig(Part::Pod), (0x1814, vec![0x2A53, 0x2A54, 0x2A55]));
        assert_eq!(sig(Part::Power), (0x1818, vec![0x2A63, 0x2A65, 0x2A5D]));
    }

    #[test]
    fn only_wrist_link_has_a_beacon_and_it_fits_a_legacy_advertisement() {
        let beacon = service(Part::Wrist).beacon.expect("WRIST_LINK beacon");
        assert_eq!(beacon.company, 0xffff);
        assert_eq!(beacon.data, b"NSTR\x01".to_vec());
        // Flags (3) + length, type, company (4) + "NSTR" and the version.
        assert_eq!(beacon.air_len(), 12);
        assert!(beacon.air_len() <= 31);
        assert!(service(Part::Pod).beacon.is_none());
        assert!(service(Part::Power).beacon.is_none());
    }

    #[test]
    fn wrist_link_is_served_but_left_out_of_the_advertisement() {
        let wrist = service(Part::Wrist);
        assert_eq!(wrist.uuid, wrist_link::SERVICE);
        assert_eq!(wrist.characteristics[0].uuid, wrist_link::STATE);
        assert!(!wrist.advertise);
        assert!(service(Part::Pod).advertise);
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
    fn switching_wrist_link_or_power_never_touches_the_foot_pod() {
        for power in [false, true] {
            for wrist in [false, true] {
                let served = parts(true, power, wrist);
                for (p2, w2) in [(power, !wrist), (!power, wrist), (!power, !wrist)] {
                    let steps = plan(served, parts(true, p2, w2));
                    assert!(
                        !steps.contains(&Step::Add(Part::Pod))
                            && !steps.contains(&Step::Remove(Part::Pod)),
                        "{served:?} to power {p2} wrist {w2}: {steps:?}"
                    );
                    assert!(!steps.is_empty());
                }
            }
        }
        assert_eq!(
            plan(parts(true, false, false), parts(true, false, true)),
            vec![Step::Add(Part::Wrist)]
        );
        assert_eq!(
            plan(parts(true, true, true), parts(true, true, false)),
            vec![Step::Remove(Part::Wrist)]
        );
        assert!(plan(parts(true, true, true), parts(true, true, true)).is_empty());
    }

    #[test]
    fn the_foot_pod_goes_up_first_and_down_last() {
        assert_eq!(
            plan(Parts::default(), parts(true, true, true)),
            vec![
                Step::Add(Part::Pod),
                Step::Add(Part::Power),
                Step::Add(Part::Wrist)
            ]
        );
        assert_eq!(
            plan(parts(true, true, true), Parts::default()),
            vec![
                Step::Remove(Part::Wrist),
                Step::Remove(Part::Power),
                Step::Remove(Part::Pod)
            ]
        );
    }

    #[test]
    fn a_failed_service_waits_before_it_is_tried_again() {
        let now = Instant::now();
        let mut rt = Runtime {
            enabled: true,
            wrist_enabled: true,
            served: parts(true, false, false),
            ..Runtime::default()
        };
        rt.failed_at[Part::Wrist.index()] = Some(now);
        assert!(steps_now(&rt, now + Duration::from_secs(5)).is_empty());
        assert_eq!(
            steps_now(&rt, now + RETRY_AFTER),
            vec![Step::Add(Part::Wrist)]
        );
    }

    #[test]
    fn only_an_abort_that_lasts_re_adds_the_foot_pod() {
        let now = Instant::now();
        let rt = Runtime {
            enabled: true,
            wrist_enabled: true,
            served: parts(true, false, true),
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
        assert_eq!(Part::of(wrist_link::SERVICE), Some(Part::Wrist));
        assert_eq!(Part::of(sig_uuid(0x180d)), None);
    }
}
