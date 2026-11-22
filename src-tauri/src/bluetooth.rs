//! Bluetooth: finding FitShow treadmills, connecting, and the two tasks that
//! run for each connection.
//!
//! - The status listener decodes every 0x51 frame into the orchestrator, runs
//!   the session lifecycle and sends the HUD its change toasts.
//! - The protocol worker writes whatever the engine decides, every 50 ms
//!   (200 ms while the deck is stopped), and runs the once-a-second ticks for
//!   Neural flow and programs.
//!
//! Both share `ORCHESTRATOR`, a tokio mutex; neither holds it across BLE I/O.
//!
//! Every BLE call has a time limit (the `*_TIMEOUT`s): after the PC sleeps,
//! Windows can leave a call pending for good, and a call that never returns
//! would hold up the connect, the worker or the windows with it. A call past
//! its limit fails like any other error.
//!
//! Both talk to the deck through [`DeckLink`]: the real treadmill over BLE,
//! or, in dev builds only, the simulated one (`sim_link.rs`).

use crate::engine::{DeckAction, OrchestratorState, Packet, PacketKind, ProtocolEngine};
use crate::protocol::{self, MachineStatus, Telemetry};
use btleplug::api::{
    Central, Characteristic, Manager as _, Peripheral as _, ScanFilter, ValueNotification,
    WriteType,
};
use btleplug::platform::{Adapter, Manager as BtleManager, Peripheral};
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::future::Future;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// The Bluetooth adapter, once found.
static ADAPTER: Mutex<Option<Adapter>> = Mutex::const_new(None);
/// One scan at a time on the shared adapter: a second `start_scan` replaces
/// the first one's filter and its `stop_scan` ends both, so the treadmill
/// link and HEART_LINK take this around every scan.
pub(crate) static SCAN_LOCK: Mutex<()> = Mutex::const_new(());
/// The treadmill of the current (or last) connection. Lock order: this before
/// `ORCHESTRATOR`. Never held across BLE I/O: the snapshot emitter reads it
/// every second, so a BLE call under it would freeze every window.
pub(crate) static CONNECTED_DECK: Mutex<Option<DeckLink>> = Mutex::const_new(None);
/// The one shared engine state.
pub(crate) static ORCHESTRATOR: LazyLock<Mutex<OrchestratorState>> =
    LazyLock::new(|| Mutex::new(OrchestratorState::default()));

/// The FitShow GATT service (`0000fff0-...`): scans filter on it.
const FITSHOW_SERVICE: uuid::Uuid = uuid::Uuid::from_u128(0x0000fff0_0000_1000_8000_00805f9b34fb);
/// UUID fragments that identify the FitShow service, its notify (status) and
/// its write (control) characteristics.
const SERVICE_TAG: &str = "fff0";
const STATUS_TAG: &str = "fff1";
const CONTROL_TAG: &str = "fff2";

/// How long a scan listens.
/// BLE devices advertise every 0.1 to 1 s or so: 5 s hears a slow advertiser
/// several times without making Connect feel stuck.
const SCAN_TIME: Duration = Duration::from_secs(5);
/// Settle times around a connect: after dropping any old link, after the
/// connect, and after service discovery.
/// Windows' BLE stack needs a moment to release a dropped link before the same
/// device connects again; 1.5 s is a few connection intervals of headroom.
const DISCONNECT_SETTLE: Duration = Duration::from_millis(1500);
/// Half a second lets the link finish its parameter exchange before service
/// discovery, and after discovery before subscribing; both are one-off costs.
const CONNECT_SETTLE: Duration = Duration::from_millis(500);
const DISCOVER_SETTLE: Duration = Duration::from_millis(500);
/// Tries per characteristic to subscribe to status frames, and the wait between.
/// A subscribe that fails right after discovery usually works on the next try;
/// three tries 200 ms apart give up within a second, then the link reconnects.
const SUBSCRIBE_TRIES: u32 = 3;
const SUBSCRIBE_RETRY: Duration = Duration::from_millis(200);
/// Time limits for each BLE call; a call past its limit counts as failed.
/// Connecting to an awake deck in range takes a second or two on Windows, a
/// few more when the stack opens a fresh GATT session; 15 s is well past
/// that, and short enough that three hung connects end within a minute.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Service discovery reads the services and their characteristics, a few
/// round trips over the air (uncached on a first connect): 5 s, since it runs
/// once per connect and a slow first read must not fail the try.
const DISCOVER_TIMEOUT: Duration = Duration::from_secs(5);
/// Closing a link is local plus one exchange with the deck. One that hangs
/// leaves a link that is gone anyway, so after 3 s the app carries on.
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Subscribing writes the status characteristic's descriptor and waits for
/// the deck's answer: one round trip, so 3 s, like a disconnect.
const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(3);
/// A write with response is one round trip of a few connection intervals
/// (tens of ms). 2 s is far past that on a live link and keeps the worker
/// from stalling long on a dead one.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);
/// Asking whether the link is up is a local query with no radio traffic, so
/// it only hangs when the Bluetooth stack is stuck; after 2 s the link counts
/// as down and the watchdog reconnects.
const IS_CONNECTED_TIMEOUT: Duration = Duration::from_secs(2);
/// A whole connect attempt (the scan and every try). Longer than the three
/// tries can take with each call at its limit (`worst_connect_time`, about
/// two minutes), so it only ends a call with no limit of its own (adapter
/// queries, starting a scan) and `is_connecting` always clears.
const CONNECT_LIMIT: Duration = Duration::from_secs(150);
/// The protocol worker's tick, and its slower tick while the deck is stopped.
/// A third of the engine's 150 ms command gap, so a queued command goes out
/// within 50 ms of being allowed. Stopped, nothing is time critical, so 200 ms.
const WORKER_TICK: Duration = Duration::from_millis(50);
const WORKER_TICK_STOPPED: Duration = Duration::from_millis(200);
/// Neural flow and programs tick this often.
/// Programs count whole seconds and Neural flow's windows are 8 to 30 s long, so
/// once a second is fine enough for both.
const SECOND_TICK: Duration = Duration::from_secs(1);
/// How long after Neural flow changes the target the deck's speed change is
/// treated as its own (the deck ramps in steps over a few seconds).
const NEURAL_RAMP_QUIET: Duration = Duration::from_secs(10);
/// A speed change smaller than this (km/h, raw) gets no toast.
/// One and a half tenths: a steady deck wobbles by its 0.1 km/h step
/// (`calibration::MAX_SPREAD_KMH`), so one step is never a change worth a toast
/// and two always are. At exactly 0.1, float rounding decided it: 3.0 to 3.1
/// stayed quiet while 3.1 to 3.2 toasted.
const SPEED_TOAST_MIN_KMH: f32 = 0.15;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BleDevice {
    pub id: String,
    pub name: String,
}

/// The deck a connection's tasks write to and watch: the real treadmill, or
/// (dev builds only) the simulated one.
#[derive(Clone)]
pub(crate) enum DeckLink {
    Ble(Peripheral),
    #[cfg(debug_assertions)]
    Sim(crate::sim_link::SimLink),
}

impl DeckLink {
    pub(crate) fn id(&self) -> String {
        match self {
            DeckLink::Ble(p) => p.id().to_string(),
            #[cfg(debug_assertions)]
            DeckLink::Sim(_) => crate::sim_link::SIM_DECK_ID.to_string(),
        }
    }

    /// Its walks stay out of the database (the dev builds' simulated deck).
    fn simulated(&self) -> bool {
        match self {
            DeckLink::Ble(_) => false,
            #[cfg(debug_assertions)]
            DeckLink::Sim(_) => true,
        }
    }

    /// Whether the link is up. A check past `IS_CONNECTED_TIMEOUT` reads as
    /// down.
    pub(crate) async fn is_connected(&self) -> bool {
        let call = async {
            Ok(match self {
                DeckLink::Ble(p) => p.is_connected().await.unwrap_or(false),
                #[cfg(debug_assertions)]
                DeckLink::Sim(s) => s.is_connected(),
            })
        };
        match limited("Link check", IS_CONNECTED_TIMEOUT, call).await {
            Ok(up) => up,
            Err(e) => {
                log::warn!("{e}; the link counts as down");
                false
            }
        }
    }

    /// Close the link, waiting at most `DISCONNECT_TIMEOUT`.
    pub(crate) async fn disconnect(&self) {
        let call = async {
            match self {
                DeckLink::Ble(p) => ConnectSteps::disconnect(p).await,
                #[cfg(debug_assertions)]
                DeckLink::Sim(s) => {
                    #[cfg(test)]
                    if s.hangs_on_disconnect() {
                        std::future::pending::<()>().await;
                    }
                    s.disconnect();
                    Ok(())
                }
            }
        };
        if let Err(e) = limited("Disconnect", DISCONNECT_TIMEOUT, call).await {
            log::warn!("{e}; carrying on without it");
        }
    }

    /// Write a packet, waiting at most `WRITE_TIMEOUT` for the deck's answer.
    pub(crate) async fn write(&self, bytes: &[u8]) -> Result<(), String> {
        let call = async {
            match self {
                DeckLink::Ble(p) => {
                    let characteristic = find_control_characteristic(p)?;
                    p.write(&characteristic, bytes, WriteType::WithResponse)
                        .await
                        .map_err(|e| e.to_string())
                }
                #[cfg(debug_assertions)]
                DeckLink::Sim(s) => s.write(bytes),
            }
        };
        limited("Write", WRITE_TIMEOUT, call).await
    }
}

/// Run one BLE call within `limit`. Past it, the call is dropped and fails
/// with `link::timed_out`'s words, which `link::describe` reads as a timeout.
async fn limited<T>(
    what: &str,
    limit: Duration,
    call: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::time::timeout(limit, call)
        .await
        .unwrap_or_else(|_| Err(crate::link::timed_out(what, limit)))
}

/// The snapshot's `connection.simulated`: the linked deck is the dev builds'
/// simulated one. Always false in release builds.
pub(crate) fn is_simulated_id(device_id: Option<&str>) -> bool {
    #[cfg(debug_assertions)]
    {
        device_id == Some(crate::sim_link::SIM_DECK_ID)
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = device_id;
        false
    }
}

/// The `uuid` belongs to the FitShow part tagged `tag` (one of the `*_TAG`s).
fn has_tag(uuid: &uuid::Uuid, tag: &str) -> bool {
    uuid.to_string().to_lowercase().contains(tag)
}

/// The first Bluetooth adapter, found once and kept.
pub async fn find_central() -> Result<Adapter, Box<dyn Error>> {
    let mut adapter_guard = ADAPTER.lock().await;
    if let Some(adapter) = adapter_guard.as_ref() {
        return Ok(adapter.clone());
    }
    let manager = BtleManager::new().await?;
    let adapter = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| Box::<dyn Error>::from("No Bluetooth adapters found"))?;
    *adapter_guard = Some(adapter.clone());
    Ok(adapter)
}

fn find_control_characteristic(peripheral: &Peripheral) -> Result<Characteristic, String> {
    peripheral
        .services()
        .into_iter()
        .filter(|s| has_tag(&s.uuid, SERVICE_TAG))
        .flat_map(|s| s.characteristics)
        .find(|c| has_tag(&c.uuid, CONTROL_TAG))
        .ok_or_else(|| "Control Char not found".to_string())
}

// ---------------------------------------------------------------------------
// Protocol worker
// ---------------------------------------------------------------------------

/// A new connection: a new worker id, and the stored settings applied the
/// same way a settings change applies them (`OrchestratorState::sync_config`).
fn begin_worker(state: &mut OrchestratorState, config: &crate::persistence::AppConfig) -> u64 {
    state.worker_id += 1;
    state.sync_config(config);
    state.worker_id
}

/// Neural flow and the running program, once a second. Returns the program's
/// HUD toast, if it has one.
fn tick_each_second(
    state: &mut OrchestratorState,
    now: Instant,
) -> Option<crate::programs::ProgramToast> {
    crate::hooks::set_key_counting(state.neural_monitor_wanted());
    let keys = crate::hooks::take_key_count();
    let now_ms = crate::util::now_ms_u64();
    let quiet = crate::snapshot::calls::quiet_active();
    state.update_neural_flow(keys, quiet, now, now_ms);
    state.heart_bpm = crate::heart_rate::snapshot().bpm;
    state.max_heart_rate = u32::from(crate::heart_rate::max_heart_rate());
    state.zone_incline_trim = crate::heart_rate::zone_incline_trim();
    state.stride_fit_kmh = crate::calibration::stride_fit::kmh(
        state.min_physical_speed(),
        state.to_physical_speed(state.top_hw_speed()),
    );
    state.tick_profile_sweep(now);
    state.tick_program(now)
}

/// Write one packet; a successful write restarts the engine's command gap.
async fn write_packet(deck: &DeckLink, packet: &Packet) {
    if let Err(e) = deck.write(&packet.bytes).await {
        log::error!("Write failed: {}", e);
        return;
    }
    ORCHESTRATOR.lock().await.last_cmd_time = Instant::now();
    crate::traffic::record_tx(packet.kind.label(), &packet.bytes);
    if packet.kind != PacketKind::PollStatus {
        log::debug!("SENT_{}: {:?}", packet.kind.label(), packet.bytes);
    }
}

/// Drive one connection until the link drops or a newer worker takes over.
pub(crate) async fn spawn_protocol_worker(app: tauri::AppHandle, deck: DeckLink) {
    crate::hud::toast(
        &app,
        serde_json::json!({ "type": "STATUS", "message": "DEVICE_CONNECTED", "isActionable": true }),
    );

    let worker_id = {
        let mut state = ORCHESTRATOR.lock().await;
        let id = begin_worker(&mut state, &crate::persistence::load_config(&app));
        log::info!(
            "Spawning worker #{} for device {}. Config Loaded: Tilt={:.1}, Offset={:.1}",
            id,
            deck.id(),
            state.tilt_multiplier,
            state.incline_offset
        );
        id
    };

    let mut engine = ProtocolEngine::new();
    let mut last_second = Instant::now();

    loop {
        if ORCHESTRATOR.lock().await.worker_id != worker_id {
            log::info!("Worker #{} superseded. Terminating.", worker_id);
            return;
        }
        if !deck.is_connected().await {
            log::info!("Peripheral disconnected. Terminating worker.");
            crate::hooks::set_key_counting(false);
            return;
        }

        let now = Instant::now();
        let second = now.duration_since(last_second) >= SECOND_TICK;
        if second {
            if let Some(quiet) = crate::link::log_frame_flow() {
                drop_silent_link(&deck, worker_id, quiet).await;
                crate::hooks::set_key_counting(false);
                return;
            }
        }
        let (packet, program_toasts) = {
            let mut state = ORCHESTRATOR.lock().await;
            let warning = if second {
                last_second = now;
                tick_each_second(&mut state, now)
            } else {
                None
            };
            let packet = engine.determine_next_packet(&mut state, now);
            // A start at the treadmill taken as GO in this engine tick.
            let toasts: Vec<_> = warning
                .into_iter()
                .chain(state.take_program_toast())
                .collect();
            (packet, toasts)
        };

        for toast in program_toasts {
            // The 30 s warning offers SKIP REST; a program waiting for GO has
            // its GO on the HUD panel, so its toast carries no button.
            let actionable = matches!(toast, crate::programs::ProgramToast::RestEndingSoon(_));
            crate::hud::toast(
                &app,
                serde_json::json!({ "type": "AUTOMATION", "message": toast.message(), "isActionable": actionable }),
            );
        }
        if let Some(packet) = packet {
            write_packet(&deck, &packet).await;
        }

        let stopped = {
            let mut state = ORCHESTRATOR.lock().await;
            if state.should_start_session() {
                open_session(&app, &mut state, deck.simulated());
            }
            state.last_status == MachineStatus::Stopped
        };

        tokio::time::sleep(if stopped {
            WORKER_TICK_STOPPED
        } else {
            WORKER_TICK
        })
        .await;
    }
}

/// A link that reads as up but has sent no status frame for `quiet` (a stale
/// subscription after a deck power cycle or the PC sleeping): take it out of
/// the slot, so the watchdog sees no link even if the disconnect hangs, and
/// close it. The watchdog's reconnect subscribes again; it sends the deck
/// nothing, and the new connection holds the automatic start
/// (`reset_connection`), so a still belt stays still and a moving one is
/// adopted.
async fn drop_silent_link(deck: &DeckLink, worker_id: u64, quiet: Duration) {
    log::warn!(
        "No status frames for {} s on a link that reads as up; dropping it to reconnect.",
        quiet.as_secs()
    );
    {
        let mut slot = CONNECTED_DECK.lock().await;
        // Only while this worker is the current one: a newer connection's
        // deck stays.
        let current = ORCHESTRATOR.lock().await.worker_id == worker_id;
        if current && slot.as_ref().is_some_and(|d| d.id() == deck.id()) {
            slot.take();
        }
    }
    deck.disconnect().await;
    crate::link::retry_soon();
}

/// Open the session a moving belt calls for. A simulated deck's walks stay in
/// memory (negative ids), so a dev run never reaches History, the odometer or
/// Strava.
fn open_session(app: &tauri::AppHandle, state: &mut OrchestratorState, simulated: bool) {
    #[cfg(debug_assertions)]
    if simulated {
        state.start_session(crate::sim_link::next_session_id());
        return;
    }
    #[cfg(not(debug_assertions))]
    let _ = simulated;
    match crate::persistence::db_create_session(app) {
        Ok(id) => state.start_session(id),
        Err(e) => log::error!("Failed to create session: {}", e),
    }
}

// ---------------------------------------------------------------------------
// Status listener
// ---------------------------------------------------------------------------

/// Speed and incline before the first frame of a connection.
const NO_INCLINE: u8 = 255;

/// Which HUD toasts a status frame calls for.
#[derive(Debug, Default, PartialEq)]
struct FrameToasts {
    status: Option<MachineStatus>,
    speed: bool,
    incline: Option<u8>,
}

/// Remembers the last status, speed and incline seen, so a toast goes out
/// only for a change, and never for the first frame's values.
struct ToastTracker {
    status: MachineStatus,
    speed: Option<f32>,
    incline: u8,
}

impl Default for ToastTracker {
    fn default() -> Self {
        Self {
            status: MachineStatus::Stopped,
            speed: None,
            incline: NO_INCLINE,
        }
    }
}

impl ToastTracker {
    /// `neural_easing`: Neural flow just changed the target, so the speed
    /// change is its own (shown quietly on the HUD panel, not as a toast).
    fn on_frame(&mut self, t: &Telemetry, neural_easing: bool) -> FrameToasts {
        let mut out = FrameToasts::default();
        // An unknown byte reads as Stopped in `t.status` but changes nothing
        // (`OrchestratorState::take_status`), so it gets no status toast.
        if t.status_known {
            if t.status != self.status {
                out.status = Some(t.status);
            }
            self.status = t.status;
        }

        match self.speed {
            Some(last) if (t.speed_kmh - last).abs() > SPEED_TOAST_MIN_KMH => {
                out.speed = !neural_easing;
                self.speed = Some(t.speed_kmh);
            }
            Some(_) => {}
            None if t.speed_kmh >= 0.0 => self.speed = Some(t.speed_kmh),
            None => {}
        }

        if self.incline != NO_INCLINE && t.incline != self.incline {
            out.incline = Some(t.incline);
            self.incline = t.incline;
        } else if self.incline == NO_INCLINE && t.incline != NO_INCLINE {
            self.incline = t.incline;
        }
        out
    }
}

/// Take a decoded frame into the orchestrator.
pub(crate) fn apply_telemetry(state: &mut OrchestratorState, t: &Telemetry) {
    state.take_status(t);
    state.hardware_speed = t.speed_kmh;
    state.hardware_incline = t.incline;
    state.hardware_time = t.time_sec;
    state.awaiting_first_packet = false;
    state.last_frame_at = Some(std::time::Instant::now());
}

/// Handle status frames until the notification stream ends.
async fn listen_for_status(
    app: tauri::AppHandle,
    mut stream: impl Stream<Item = ValueNotification> + Unpin,
    simulated: bool,
) {
    let mut toasts = ToastTracker::default();
    while let Some(data) = stream.next().await {
        crate::traffic::record_rx(&data.uuid, &data.value);
        if !has_tag(&data.uuid, STATUS_TAG) {
            continue;
        }
        let Some(t) = protocol::parse_status(&data.value) else {
            if let Some(reply) = protocol::parse_reply(&data.value) {
                let mut state = ORCHESTRATOR.lock().await;
                // The simulated deck's limits are not the treadmill's.
                state.profile_learning = !simulated;
                state.on_deck_reply(reply);
            }
            continue;
        };
        crate::link::mark_frame();
        crate::link::note_status_byte(t.status_code, t.status_known.then_some(t.status));

        let mut state = ORCHESTRATOR.lock().await;
        apply_telemetry(&mut state, &t);
        if simulated {
            // In memory only, the way `sim::Rig` runs them.
            state.update_physics();
            if state.should_start_session() {
                open_session(&app, &mut state, true);
            } else if state.should_stop_session() {
                state.stop_session();
            }
        } else {
            // Resume or new session, once per connection, from the deck's live
            // totals, so a walk already under way is picked up.
            state.evaluate_session_on_connect(
                &app,
                t.time_sec,
                (t.distance_km * 1000.0) as f64,
                t.steps as f64,
                t.calories,
            );
            state.update_physics();
            state.manage_session_lifecycle(&app);
        }

        let neural_easing = state
            .neural_changed_at
            .is_some_and(|at| at.elapsed() < NEURAL_RAMP_QUIET);
        let changes = toasts.on_frame(&t, neural_easing);
        if let Some(status) = changes.status {
            crate::hud::toast(
                &app,
                serde_json::json!({ "type": "STATUS", "message": format!("STATUS: {}", status.toast_label()), "isActionable": true }),
            );
        }
        if changes.speed {
            let phys = state.to_physical_speed(t.speed_kmh);
            crate::hud::toast(
                &app,
                serde_json::json!({ "type": "VELOCITY", "message": format!("SPEED: {:.1}", phys), "value": phys }),
            );
        }
        if let Some(incline) = changes.incline {
            crate::hud::toast(
                &app,
                serde_json::json!({ "type": "ELEVATION", "message": format!("INCLINE: {}", incline), "value": incline }),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Scan and connect
// ---------------------------------------------------------------------------

/// Scan for FitShow treadmills. The scan shows in the link state while it runs,
/// and a failure comes back as a typed, human-readable error.
#[tauri::command]
pub async fn scan_bluetooth_devices() -> Result<Vec<BleDevice>, crate::link::LinkError> {
    let _scanning = crate::link::ScanGuard::new();
    crate::link::reset_backoff();
    let result = match scan_inner().await {
        Ok(devices) => {
            crate::link::adapter_ok();
            Ok(devices)
        }
        Err(raw) => {
            let err = crate::link::describe(&raw);
            crate::link::note_error(&err, &raw);
            Err(err)
        }
    };
    // Dev builds: the simulated treadmill is listed even with no Bluetooth
    // (the failure still shows in the link state).
    #[cfg(debug_assertions)]
    let result = Ok(with_sim_deck(result.unwrap_or_default()));
    result
}

/// Dev builds list the simulated treadmill first.
#[cfg(debug_assertions)]
fn with_sim_deck(found: Vec<BleDevice>) -> Vec<BleDevice> {
    let sim = BleDevice {
        id: crate::sim_link::SIM_DECK_ID.to_string(),
        name: crate::sim_link::SIM_DECK_NAME.to_string(),
    };
    std::iter::once(sim).chain(found).collect()
}

/// The name to list a device under, or `None` when it is not a FitShow deck:
/// it must advertise the FitShow service or carry a FitShow-like name.
fn listed_name(
    local_name: Option<String>,
    has_manufacturer_data: bool,
    has_fitshow_service: bool,
) -> Option<String> {
    let name = match local_name.filter(|n| !n.is_empty()) {
        Some(n) => n,
        None if has_manufacturer_data => "FitShow Device (Data)".to_string(),
        None => "Unknown Device".to_string(),
    };
    let lower = name.to_lowercase();
    (has_fitshow_service || lower.contains("fitshow") || lower.contains("fs-")).then_some(name)
}

async fn scan_inner() -> Result<Vec<BleDevice>, String> {
    let central = find_central().await.map_err(|e| e.to_string())?;
    let _scan = SCAN_LOCK.lock().await;
    let filter = ScanFilter {
        services: vec![FITSHOW_SERVICE],
    };
    central
        .start_scan(filter)
        .await
        .map_err(|e| e.to_string())?;
    tokio::time::sleep(SCAN_TIME).await;
    let _ = central.stop_scan().await;

    let mut seen_ids = std::collections::HashSet::new();
    let mut found = Vec::new();
    for peripheral in central.peripherals().await.map_err(|e| e.to_string())? {
        let Some(props) = peripheral.properties().await.map_err(|e| e.to_string())? else {
            continue;
        };
        let id = peripheral.id().to_string();
        if !seen_ids.insert(id.clone()) {
            continue;
        }
        let has_service = props
            .services
            .iter()
            .any(|uuid| uuid.to_string().contains(SERVICE_TAG));
        if let Some(name) = listed_name(
            props.local_name,
            !props.manufacturer_data.is_empty(),
            has_service,
        ) {
            found.push((props.rssi.unwrap_or(-100), BleDevice { id, name }));
        }
    }
    // Strongest signal first.
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    Ok(found.into_iter().map(|(_, d)| d).collect())
}

#[tauri::command]
pub async fn connect_bluetooth_device(
    app_handle: tauri::AppHandle,
    id: String,
) -> Result<(), crate::link::LinkError> {
    let attempt = async {
        crate::link::begin_connect();
        connect_to(&app_handle, &id).await
    };
    let Some(result) = while_connecting(attempt).await else {
        log::info!(
            "Connection attempt already in progress. Ignoring request for {}",
            id
        );
        return Ok(());
    };

    match result {
        Ok(()) => {
            crate::link::mark_linked();
            Ok(())
        }
        Err(raw) => {
            let err = crate::link::describe(&raw);
            crate::link::connect_failed(&err, &raw);
            Err(err)
        }
    }
}

/// Run one connect attempt with `is_connecting` set, within `CONNECT_LIMIT`,
/// and clear it after, however the attempt ended (a timeout included). None
/// when an attempt is already running.
async fn while_connecting(
    attempt: impl Future<Output = Result<(), String>>,
) -> Option<Result<(), String>> {
    {
        let mut state = ORCHESTRATOR.lock().await;
        if state.is_connecting {
            return None;
        }
        state.is_connecting = true;
    }
    let result = limited("Connecting", CONNECT_LIMIT, attempt).await;
    ORCHESTRATOR.lock().await.is_connecting = false;
    Some(result)
}

/// Whether the treadmill link is up, asked of the peripheral itself (the slot
/// is only cleared when the next connect begins, so a dropped link still fills
/// it).
pub async fn is_bluetooth_connected() -> bool {
    // Cloned out so the slot isn't locked across the BLE query, which would
    // stall a connect attempt waiting on it.
    let deck = CONNECTED_DECK.lock().await.clone();
    match deck {
        Some(d) => d.is_connected().await,
        None => false,
    }
}

/// A device id without separators, upper case, for matching.
pub(crate) fn normalize_id(id: &str) -> String {
    id.replace([':', '-'], "").to_uppercase()
}

/// How many hex digits a Bluetooth address has (6 bytes).
/// Six bytes per the Bluetooth spec.
const ADDRESS_DIGITS: usize = 12;

/// The part of a device id that names the treadmill itself, for its
/// DECK_PROFILE key: the last `ADDRESS_DIGITS` of the normalized id, its
/// Bluetooth address. The saved id and the one a scan reports may differ by a
/// prefix (`ids_match`), and both forms must find the same profile.
fn profile_device_id(raw_id: &str) -> String {
    let id = normalize_id(raw_id);
    let skip = id.chars().count().saturating_sub(ADDRESS_DIGITS);
    id.chars().skip(skip).collect()
}

/// Two normalized ids name the same device. Either may be the shorter form
/// (Windows ids carry a prefix the saved id may lack, or the other way round).
pub(crate) fn ids_match(a: &str, b: &str) -> bool {
    a.contains(b) || b.contains(a)
}

/// Drop any old link and reset the engine for a new connection (which sets
/// `hold_auto_start`: connecting never starts the belt).
async fn begin_new_link() {
    drop_old_link(&CONNECTED_DECK).await;
    ORCHESTRATOR.lock().await.reset_connection();
    log::debug!("Readiness reset for new node entry.");
}

/// Take the old link out of `slot` and close it. The guard is dropped before
/// the disconnect, so the snapshot emitter's read of the slot never waits on
/// a disconnect (one that hangs would freeze every window).
async fn drop_old_link(slot: &Mutex<Option<DeckLink>>) {
    let existing = slot.lock().await.take();
    if let Some(existing) = existing {
        log::debug!("Disconnecting existing active link before new connection...");
        existing.disconnect().await;
    }
}

async fn connect_to(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if id == crate::sim_link::SIM_DECK_ID {
        connect_sim(app).await;
        return Ok(());
    }

    let central = find_central().await.map_err(|e| e.to_string())?;
    begin_new_link().await;

    let target = normalize_id(id);
    let mut peripherals = central.peripherals().await.map_err(|e| e.to_string())?;
    log::debug!(
        "Discovered {} peripherals. Target: {}",
        peripherals.len(),
        id
    );

    let known = peripherals
        .iter()
        .any(|p| ids_match(&normalize_id(&p.id().to_string()), &target));
    if !known {
        log::debug!("Target {} not in cache. Starting 5s scan...", id);
        let scan = SCAN_LOCK.lock().await;
        let _ = central.start_scan(ScanFilter::default()).await;
        tokio::time::sleep(SCAN_TIME).await;
        let _ = central.stop_scan().await;
        drop(scan);
        peripherals = central.peripherals().await.map_err(|e| e.to_string())?;
        log::debug!("Post-scan peripheral count: {}", peripherals.len());
        // A working radio always hears something in five seconds.
        if peripherals.is_empty() {
            return Err(crate::link::NO_DEVICES_SEEN.to_string());
        }
    }

    for peripheral in peripherals {
        let raw_id = peripheral.id().to_string();
        let p_id = normalize_id(&raw_id);
        if target.len() > 4 {
            log::debug!(
                "Checking peripheral: {} (Raw: {}) against Target: {}",
                p_id,
                raw_id,
                target
            );
        }
        if ids_match(&p_id, &target) {
            log::info!(
                "Target peripheral found: {}. Starting connection sequence...",
                p_id
            );
            return connect_with_retries(&peripheral, id, || {
                subscribe_and_start(app, &peripheral, &raw_id)
            })
            .await;
        }
    }
    Err("Device not found".to_string())
}

/// Link the dev builds' simulated treadmill and start the connection's tasks,
/// as `subscribe_and_start` does for a real one, and save it as the treadmill
/// to reconnect to. Dev builds have their own database (`IDENTIFIER`), so
/// this never touches the release app's saved treadmill.
#[cfg(debug_assertions)]
async fn connect_sim(app: &tauri::AppHandle) {
    begin_new_link().await;
    log::info!("Linking the simulated treadmill (dev build).");
    // Its own belt length and points, under an id no real Bluetooth address can have.
    switch_calibration(app, "SIMDECK").await;
    let link = crate::sim_link::SimLink::connect();
    tokio::spawn(listen_for_status(app.clone(), link.notifications(), true));
    let deck = DeckLink::Sim(link);
    tokio::spawn(spawn_protocol_worker(app.clone(), deck.clone()));
    *CONNECTED_DECK.lock().await = Some(deck);

    let app_b = app.clone();
    let saved = crate::util::blocking(move || {
        let mut config = crate::persistence::load_config(&app_b);
        config.last_device_id = Some(crate::sim_link::SIM_DECK_ID.to_string());
        crate::persistence::save_config(&app_b, &config)
    })
    .await;
    if let Err(e) = saved {
        log::warn!("Could not remember the simulated treadmill: {}", e);
    }
}

/// The calls of one connect try, apart from the peripheral so a test can
/// stand in a link that hangs. `connect_with_retries` gives each its limit.
trait ConnectSteps {
    fn connect(&self) -> impl Future<Output = Result<(), String>> + Send;
    fn discover(&self) -> impl Future<Output = Result<(), String>> + Send;
    fn disconnect(&self) -> impl Future<Output = Result<(), String>> + Send;
}

impl ConnectSteps for Peripheral {
    async fn connect(&self) -> Result<(), String> {
        btleplug::api::Peripheral::connect(self)
            .await
            .map_err(|e| e.to_string())
    }
    async fn discover(&self) -> Result<(), String> {
        self.discover_services().await.map_err(|e| e.to_string())
    }
    async fn disconnect(&self) -> Result<(), String> {
        btleplug::api::Peripheral::disconnect(self)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Close `link` within `DISCONNECT_TIMEOUT`; a failure only goes to the log.
async fn close<S: ConnectSteps>(link: &S) {
    if let Err(e) = limited("Disconnect", DISCONNECT_TIMEOUT, link.disconnect()).await {
        log::debug!("{e}");
    }
}

/// The longest `connect_with_retries` can take with every call at its limit
/// (one status characteristic). `CONNECT_LIMIT` must outlast it.
#[cfg(test)]
fn worst_connect_time() -> Duration {
    let subscribe = (SUBSCRIBE_TIMEOUT + SUBSCRIBE_RETRY) * SUBSCRIBE_TRIES + SUBSCRIBE_TIMEOUT;
    let one_try = DISCONNECT_TIMEOUT
        + DISCONNECT_SETTLE
        + CONNECT_TIMEOUT
        + CONNECT_SETTLE
        + DISCOVER_TIMEOUT
        + DISCOVER_SETTLE
        + subscribe
        + DISCONNECT_TIMEOUT;
    SCAN_TIME + one_try * crate::link::CONNECT_TRIES
}

/// Up to `link::CONNECT_TRIES` full cycles of disconnect, connect, discover
/// and `subscribe`. A step past its time limit fails that try.
async fn connect_with_retries<S, F, Fut>(link: &S, id: &str, mut subscribe: F) -> Result<(), String>
where
    S: ConnectSteps,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<bool, String>>,
{
    for attempt in 1..=crate::link::CONNECT_TRIES {
        crate::link::set_attempt(attempt);
        log::info!(
            "Connection attempt {}/{}...",
            attempt,
            crate::link::CONNECT_TRIES
        );
        close(link).await;
        tokio::time::sleep(DISCONNECT_SETTLE).await;

        if let Err(e) = limited("Connect", CONNECT_TIMEOUT, link.connect()).await {
            log::warn!("Connection failed: {}. Retrying...", e);
            continue;
        }
        tokio::time::sleep(CONNECT_SETTLE).await;

        if let Err(e) = limited("Service discovery", DISCOVER_TIMEOUT, link.discover()).await {
            log::warn!("Service discovery failed: {}. Retrying...", e);
            close(link).await;
            continue;
        }
        tokio::time::sleep(DISCOVER_SETTLE).await;

        if subscribe().await? {
            return Ok(());
        }
        close(link).await;
    }
    Err(format!(
        "Failed to connect to {} after {} full cycles.",
        id,
        crate::link::CONNECT_TRIES
    ))
}

/// Subscribe to status frames and start the connection's tasks. `Ok(false)`
/// when no status characteristic would take the subscription.
async fn subscribe_and_start(
    app: &tauri::AppHandle,
    peripheral: &Peripheral,
    raw_id: &str,
) -> Result<bool, String> {
    let status_characteristics: Vec<Characteristic> = peripheral
        .services()
        .into_iter()
        .filter(|s| has_tag(&s.uuid, SERVICE_TAG))
        .flat_map(|s| s.characteristics)
        .filter(|c| has_tag(&c.uuid, STATUS_TAG))
        .collect();

    for characteristic in &status_characteristics {
        for _ in 0..SUBSCRIBE_TRIES {
            let subscribe = async {
                peripheral
                    .subscribe(characteristic)
                    .await
                    .map_err(|e| e.to_string())
            };
            if let Err(e) = limited("Subscribe", SUBSCRIBE_TIMEOUT, subscribe).await {
                log::debug!("Subscribe failed: {e}");
                tokio::time::sleep(SUBSCRIBE_RETRY).await;
                continue;
            }
            let notifications =
                async { peripheral.notifications().await.map_err(|e| e.to_string()) };
            let stream = limited("Notifications", SUBSCRIBE_TIMEOUT, notifications).await?;
            switch_deck_profile(app, raw_id).await;
            tokio::spawn(listen_for_status(app.clone(), stream, false));
            let deck = DeckLink::Ble(peripheral.clone());
            tokio::spawn(spawn_protocol_worker(app.clone(), deck.clone()));
            *CONNECTED_DECK.lock().await = Some(deck);

            // The raw id matches fastest next time.
            let mut config = crate::persistence::load_config(app);
            config.last_device_id = Some(raw_id.to_string());
            if let Err(e) = crate::persistence::save_config(app, &config) {
                log::warn!("Could not remember the treadmill: {}", e);
            }
            return Ok(true);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// Commands and shared mutations
// ---------------------------------------------------------------------------

/// Run a control verb by name (`DeckAction::as_str`). An unknown name is
/// logged and ignored.
pub async fn trigger_action(action: String) -> Result<(), String> {
    match DeckAction::parse(&action) {
        Some(action) => ORCHESTRATOR.lock().await.trigger_action(action),
        None => log::warn!("Unknown action: {}", action),
    }
    Ok(())
}

/// Load the stored settings into the orchestrator at startup. Whether to
/// resume a session is decided later, on the first frame after connecting
/// (`OrchestratorState::evaluate_session_on_connect`).
pub async fn init_from_config(config: &crate::persistence::AppConfig) {
    ORCHESTRATOR.lock().await.sync_config(config);
}

/// The DECK_PROFILE saved for a treadmill. The first treadmill asked adopts
/// the app-wide profile from before there was one per treadmill.
fn load_profile_for(
    store: &impl crate::persistence::SettingsStore,
    key: &str,
) -> crate::deck_profile::DeckProfile {
    use crate::deck_profile::{DeckProfile, LEGACY_KEY};
    if let Some(raw) = store.get(key) {
        return DeckProfile::from_stored(Some(&raw));
    }
    let Some(raw) = store.get(LEGACY_KEY) else {
        return DeckProfile::default();
    };
    let profile = DeckProfile::from_stored(Some(&raw));
    if let Err(e) = store.put(key, &raw).and_then(|_| store.remove(LEGACY_KEY)) {
        log::warn!("could not move the deck profile to its treadmill: {}", e);
    }
    profile
}

/// Save the profile the previous treadmill taught (`pending`: its key and the
/// JSON, None to delete), then load the one stored under `key`.
fn swap_profile(
    store: &impl crate::persistence::SettingsStore,
    pending: Option<(String, Option<String>)>,
    key: &str,
) -> crate::deck_profile::DeckProfile {
    if let Some((old_key, json)) = pending {
        let saved = match json {
            Some(json) => store.put(&old_key, &json),
            None => store.remove(&old_key),
        };
        if let Err(e) = saved {
            log::warn!("could not save the deck profile: {}", e);
        }
    }
    load_profile_for(store, key)
}

/// Make GROUND_TRUTH's belt length and calibration points the ones of the
/// treadmill with this (normalized) id. When the treadmill changed, the engine
/// takes its points; a running belt keeps its byte (`CurveHold`).
async fn switch_calibration(app: &tauri::AppHandle, device_id: &str) {
    let (app_b, id_b) = (app.clone(), device_id.to_string());
    let points = crate::util::blocking(move || {
        Ok::<_, String>(crate::calibration::switch_belt(&app_b, &id_b))
    })
    .await
    .ok()
    .flatten();
    if let Some(points) = points {
        log::info!(
            "Calibration points loaded: {} for this treadmill",
            points.len()
        );
        ORCHESTRATOR.lock().await.set_calibration_points(points);
    }
}

/// Make DECK_PROFILE the one of the treadmill with this device id: what the
/// previous treadmill taught it is saved first. Does nothing when it already
/// is. Called at startup with the saved treadmill and on every connection.
pub async fn switch_deck_profile(app: &tauri::AppHandle, raw_id: &str) {
    let device_id = profile_device_id(raw_id);
    // GROUND_TRUTH's belt length and points are kept per treadmill by the same identity.
    switch_calibration(app, &device_id).await;
    let key = crate::deck_profile::key_for(&device_id);
    let pending = {
        let mut state = ORCHESTRATOR.lock().await;
        if state.deck_profile_key.as_deref() == Some(&key) {
            return;
        }
        let json = state.deck_profile.take_unsaved();
        state.deck_profile_key.clone().zip(json)
    };
    let app_c = app.clone();
    let key_c = key.clone();
    let profile =
        crate::util::blocking(move || Ok::<_, String>(swap_profile(&app_c, pending, &key_c)))
            .await
            .unwrap_or_default();
    log::info!("Deck profile loaded: {} speeds learned", profile.len());
    let mut state = ORCHESTRATOR.lock().await;
    state.deck_profile = profile;
    state.deck_profile_key = Some(key);
}

#[tauri::command]
pub async fn sync_orchestrator_config(
    app: tauri::AppHandle,
    config: crate::persistence::AppConfig,
) -> Result<(), String> {
    let saved_id = config.last_device_id.clone();
    let stored = crate::persistence::load_config(&app);
    let mut config = config;
    // Points loaded for another treadmill (a window that loaded them before
    // the connection switched) never land on this one: the treadmill in use
    // keeps its own, and the window reloads them.
    if config.calibration_treadmill != stored.calibration_treadmill {
        log::info!("Settings sync carried another treadmill's calibration points; kept this one's");
        config.calibration_points = stored.calibration_points.clone();
        config.calibration_treadmill = stored.calibration_treadmill.clone();
        crate::calibration::points_changed();
    }
    crate::calibration::check_point_scales(&config.calibration_points, &stored.calibration_points)?;
    {
        let mut state = ORCHESTRATOR.lock().await;
        state.sync_config(&config);
        crate::hooks::set_key_counting(state.neural_monitor_wanted());
    }
    crate::persistence::save_config(&app, &config)?;

    // A saved treadmill but no link (first run after settings were restored):
    // connect now rather than wait for the watchdog.
    if !is_bluetooth_connected().await {
        if let Some(id) = saved_id {
            if !ORCHESTRATOR.lock().await.is_connecting {
                tauri::async_runtime::spawn(async move {
                    log::info!("Triggering auto-connect from sync for {}", id);
                    let _ = connect_bluetooth_device(app, id).await;
                });
            }
        }
    }
    Ok(())
}

/// Frame a raw body from the diagnostics command cycler and write it to the deck.
#[tauri::command]
pub async fn send_bluetooth_command(body: Vec<u8>) -> Result<(), String> {
    let data = crate::protocol::raw_packet(&body)?;
    log::debug!("Raw command from diagnostics: {:02X?}", data);
    let deck = CONNECTED_DECK.lock().await.clone();
    let Some(deck) = deck else {
        return Err("No device connected".to_string());
    };
    deck.write(&data).await?;
    crate::traffic::record_tx("RAW", &data);
    Ok(())
}

/// Set the manual intent (speed and/or incline) as the user's own change.
pub(crate) fn apply_manual_intent(
    state: &mut OrchestratorState,
    speed: Option<f32>,
    incline: Option<u8>,
) {
    state.set_intent(speed, incline, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(status: MachineStatus, speed_kmh: f32, incline: u8) -> Telemetry {
        Telemetry {
            status,
            status_code: 0,
            status_known: true,
            speed_kmh,
            incline,
            time_sec: 0,
            distance_km: 0.0,
            calories: 0.0,
            steps: 0,
        }
    }

    fn calibrated() -> crate::persistence::AppConfig {
        crate::persistence::AppConfig {
            calibration_points: vec![
                protocol::CalibrationPoint {
                    speed: 2.0,
                    stride_length_cm: 60.0,
                    distance_scale: 0.9,
                    runs: 1,
                    stride_runs: 0,
                },
                protocol::CalibrationPoint {
                    speed: 1.0,
                    stride_length_cm: 55.0,
                    distance_scale: 0.8,
                    runs: 1,
                    stride_runs: 0,
                },
            ],
            min_speed: 1.0,
            max_speed: 8.0,
            max_incline: 10,
            tilt_multiplier: 0.75,
            incline_offset: -0.5,
            // Out of range: both paths clamp it.
            body_mass_kg: 500.0,
            focus_stability_enabled: true,
            neural_floor_pct: 70.0,
            neural_sensitivity: crate::neural::NeuralSensitivity::High,
            ..Default::default()
        }
    }

    /// Settings changed while linked, and the same settings loaded by a
    /// reconnect, leave the engine in the same state.
    #[test]
    fn a_settings_change_and_a_reconnect_apply_the_same_config() {
        let startup = crate::persistence::AppConfig::default();

        let mut changed_while_linked = OrchestratorState::default();
        changed_while_linked.sync_config(&startup);
        begin_worker(&mut changed_while_linked, &startup);
        changed_while_linked.sync_config(&calibrated());

        let mut reconnected = OrchestratorState::default();
        reconnected.sync_config(&startup);
        begin_worker(&mut reconnected, &calibrated());

        assert_eq!(
            changed_while_linked.config_fingerprint(),
            reconnected.config_fingerprint()
        );
        assert_eq!(reconnected.tilt_multiplier, 0.75);
        assert_eq!(reconnected.incline_offset, -0.5);
        assert_eq!(
            reconnected.body_mass_kg,
            crate::persistence::MAX_BODY_MASS_KG
        );
        assert_eq!(reconnected.sorted_points[0].speed, 1.0);
    }

    #[test]
    fn each_connection_gets_a_new_worker_id() {
        let mut state = OrchestratorState::default();
        let config = crate::persistence::AppConfig::default();
        let first = begin_worker(&mut state, &config);
        assert_eq!(begin_worker(&mut state, &config), first + 1);
    }

    #[test]
    fn the_first_frame_only_toasts_a_status_change() {
        let mut t = ToastTracker::default();
        let first = t.on_frame(&frame(MachineStatus::Running, 3.0, 5), false);
        assert_eq!(
            first,
            FrameToasts {
                status: Some(MachineStatus::Running),
                speed: false,
                incline: None
            }
        );
        // An idle deck's first frame says nothing at all.
        let mut t = ToastTracker::default();
        assert_eq!(
            t.on_frame(&frame(MachineStatus::Stopped, 0.0, 0), false),
            FrameToasts::default()
        );
    }

    #[test]
    fn changes_are_toasted_once() {
        let mut t = ToastTracker::default();
        t.on_frame(&frame(MachineStatus::Running, 3.0, 5), false);
        // 0.1 km/h is not a change worth a toast.
        assert_eq!(
            t.on_frame(&frame(MachineStatus::Running, 3.1, 5), false),
            FrameToasts::default()
        );
        let up = t.on_frame(&frame(MachineStatus::Running, 3.5, 6), false);
        assert_eq!(
            up,
            FrameToasts {
                status: None,
                speed: true,
                incline: Some(6)
            }
        );
        assert_eq!(
            t.on_frame(&frame(MachineStatus::Running, 3.5, 6), false),
            FrameToasts::default()
        );
        let paused = t.on_frame(&frame(MachineStatus::Paused, 3.5, 6), false);
        assert_eq!(paused.status, Some(MachineStatus::Paused));
    }

    #[test]
    fn a_one_step_wobble_never_toasts_at_any_speed() {
        // Speeds arrive as tenths (byte / 10), so test them the same way.
        for byte in 1u8..=120 {
            let kmh = |b: u8| f32::from(b) / 10.0;
            let mut t = ToastTracker::default();
            t.on_frame(&frame(MachineStatus::Running, kmh(byte), 0), false);
            for wobble in [byte + 1, byte, byte - 1, byte] {
                let out = t.on_frame(&frame(MachineStatus::Running, kmh(wobble), 0), false);
                assert!(!out.speed, "{} to {} km/h toasted", kmh(byte), kmh(wobble));
            }
            let out = t.on_frame(&frame(MachineStatus::Running, kmh(byte + 2), 0), false);
            assert!(
                out.speed,
                "two steps up from {} km/h did not toast",
                kmh(byte)
            );
        }
    }

    #[test]
    fn neural_flow_easing_is_not_toasted_but_is_tracked() {
        let mut t = ToastTracker::default();
        t.on_frame(&frame(MachineStatus::Running, 4.0, 0), false);
        assert!(
            !t.on_frame(&frame(MachineStatus::Running, 3.6, 0), true)
                .speed
        );
        // The eased speed is the new reference: going back is a change again.
        assert!(
            !t.on_frame(&frame(MachineStatus::Running, 3.6, 0), false)
                .speed
        );
        assert!(
            t.on_frame(&frame(MachineStatus::Running, 4.0, 0), false)
                .speed
        );
    }

    #[test]
    fn listed_names_keep_only_fitshow_decks() {
        assert_eq!(
            listed_name(Some("FS-1234".into()), false, false),
            Some("FS-1234".into())
        );
        assert_eq!(
            listed_name(Some("My FitShow".into()), false, false),
            Some("My FitShow".into())
        );
        assert_eq!(listed_name(Some("Headphones".into()), true, false), None);
        assert_eq!(
            listed_name(Some("Deck".into()), false, true),
            Some("Deck".into())
        );
        assert_eq!(
            listed_name(None, true, true),
            Some("FitShow Device (Data)".into())
        );
        assert_eq!(
            listed_name(Some(String::new()), false, true),
            Some("Unknown Device".into())
        );
        assert_eq!(listed_name(None, false, false), None);
    }

    #[test]
    fn device_ids_match_with_or_without_separators() {
        assert_eq!(normalize_id("aa:bb-cc"), "AABBCC");
        assert!(ids_match(
            &normalize_id("BluetoothLE#aa:bb:cc:dd:ee:ff"),
            &normalize_id("AA:BB:CC:DD:EE:FF")
        ));
        assert!(ids_match(
            &normalize_id("AA:BB:CC:DD:EE:FF"),
            &normalize_id("BluetoothLE#aa:bb:cc:dd:ee:ff")
        ));
        assert!(!ids_match("AABBCC", "DDEEFF"));
    }

    #[test]
    fn both_forms_of_an_id_share_one_deck_profile() {
        assert_eq!(
            profile_device_id("BluetoothLE#BluetoothLE11:22:33:44:55:66-aa:bb:cc:dd:ee:ff"),
            "AABBCCDDEEFF"
        );
        assert_eq!(profile_device_id("aa:bb:cc:dd:ee:ff"), "AABBCCDDEEFF");
        assert_eq!(profile_device_id("dev-1"), "DEV1");
    }

    /// Run `f` on a test clock that jumps ahead whenever every task waits.
    fn paused<F: Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap()
            .block_on(f)
    }

    /// A stand-in for the peripheral: the first `hung_connects` connects
    /// never return, and disconnect or discovery can hang too.
    #[derive(Default)]
    struct FakeLink {
        hung_connects: u32,
        hang_disconnect: bool,
        hang_discover: bool,
        connects: std::sync::atomic::AtomicU32,
    }

    impl FakeLink {
        fn connects(&self) -> u32 {
            self.connects.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    async fn hang_if(hang: bool) {
        if hang {
            std::future::pending::<()>().await;
        }
    }

    impl ConnectSteps for FakeLink {
        async fn connect(&self) -> Result<(), String> {
            let n = self
                .connects
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            hang_if(n < self.hung_connects).await;
            Ok(())
        }
        async fn discover(&self) -> Result<(), String> {
            hang_if(self.hang_discover).await;
            Ok(())
        }
        async fn disconnect(&self) -> Result<(), String> {
            hang_if(self.hang_disconnect).await;
            Ok(())
        }
    }

    /// Connect `link` as `connect_to` does; how it ended, how long it took
    /// on the test clock, and how many times it subscribed.
    async fn try_link(link: &FakeLink) -> (Result<(), String>, Duration, u32) {
        let start = tokio::time::Instant::now();
        let mut subscribed = 0;
        let result = connect_with_retries(link, "deck", || {
            subscribed += 1;
            async { Ok(true) }
        })
        .await;
        (result, start.elapsed(), subscribed)
    }

    #[test]
    fn a_connect_that_never_returns_fails_each_try_then_gives_up() {
        paused(async {
            let link = FakeLink {
                hung_connects: u32::MAX,
                ..Default::default()
            };
            let (result, took, subscribed) = try_link(&link).await;
            let err = result.unwrap_err();
            assert_eq!(
                crate::link::describe(&err).kind,
                crate::link::LinkErrorKind::Timeout,
                "{err}"
            );
            assert_eq!(link.connects(), crate::link::CONNECT_TRIES);
            assert_eq!(subscribed, 0);
            assert_eq!(
                took,
                (DISCONNECT_SETTLE + CONNECT_TIMEOUT) * crate::link::CONNECT_TRIES
            );
        });
    }

    #[test]
    fn a_connect_that_hangs_once_links_on_the_next_try() {
        paused(async {
            let link = FakeLink {
                hung_connects: 1,
                ..Default::default()
            };
            let (result, took, subscribed) = try_link(&link).await;
            assert_eq!(result, Ok(()));
            assert_eq!((link.connects(), subscribed), (2, 1));
            assert!(took < CONNECT_TIMEOUT * 2, "{took:?}");
        });
    }

    #[test]
    fn a_hanging_disconnect_and_discovery_only_cost_their_limits() {
        paused(async {
            let link = FakeLink {
                hang_disconnect: true,
                hang_discover: true,
                ..Default::default()
            };
            let (result, took, subscribed) = try_link(&link).await;
            assert!(result.is_err());
            assert_eq!((link.connects(), subscribed), (3, 0));
            let one_try = DISCONNECT_TIMEOUT
                + DISCONNECT_SETTLE
                + CONNECT_SETTLE
                + DISCOVER_TIMEOUT
                + DISCONNECT_TIMEOUT;
            assert_eq!(took, one_try * crate::link::CONNECT_TRIES);
        });
    }

    #[test]
    fn a_hung_connect_attempt_still_clears_is_connecting() {
        paused(async {
            let start = tokio::time::Instant::now();
            let hung = while_connecting(std::future::pending());
            // A second request while the first hangs is turned away.
            let second = async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let refused = while_connecting(async { Ok(()) }).await.is_none();
                (refused, ORCHESTRATOR.lock().await.is_connecting)
            };
            let (first, (refused, connecting_meanwhile)) = tokio::join!(hung, second);
            assert!(refused && connecting_meanwhile);
            let err = first.expect("it ran").unwrap_err();
            assert_eq!(
                crate::link::describe(&err).kind,
                crate::link::LinkErrorKind::Timeout,
                "{err}"
            );
            assert_eq!(start.elapsed(), CONNECT_LIMIT);
            assert!(!ORCHESTRATOR.lock().await.is_connecting);
            // The next attempt runs.
            assert_eq!(while_connecting(async { Ok(()) }).await, Some(Ok(())));
        });
    }

    #[test]
    fn the_connect_limit_outlasts_every_try_at_its_limits() {
        assert!(
            worst_connect_time() < CONNECT_LIMIT,
            "{:?}",
            worst_connect_time()
        );
    }

    /// The snapshot emitter reads the slot every second; a disconnect that
    /// never returns must not hold it, and ends at its limit.
    #[cfg(debug_assertions)]
    #[test]
    fn a_hanging_disconnect_never_holds_the_slot_the_emitter_reads() {
        paused(async {
            let sim = std::sync::Arc::new(std::sync::Mutex::new(crate::sim::SimDeck::new(
                Instant::now(),
            )));
            let link = crate::sim_link::SimLink::connect_to(sim).hanging_disconnect();
            let slot = Mutex::new(Some(DeckLink::Sim(link)));
            let start = tokio::time::Instant::now();
            let emitter = async {
                for _ in 0..2 {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let read = tokio::time::timeout(Duration::from_millis(10), async {
                        slot.lock().await.is_some()
                    })
                    .await;
                    assert_eq!(read.ok(), Some(false), "the emitter waited on the slot");
                }
            };
            tokio::join!(drop_old_link(&slot), emitter);
            assert_eq!(start.elapsed(), DISCONNECT_TIMEOUT);
        });
    }

    #[test]
    fn the_fitshow_service_uuid() {
        assert_eq!(
            FITSHOW_SERVICE.to_string(),
            "0000fff0-0000-1000-8000-00805f9b34fb"
        );
        assert!(has_tag(&FITSHOW_SERVICE, SERVICE_TAG));
    }

    /// Connect to the treadmill with `id`: what `switch_deck_profile` does to
    /// the settings, without the orchestrator.
    fn connect(
        store: &rusqlite::Connection,
        current: &mut (Option<String>, crate::deck_profile::DeckProfile),
        id: &str,
    ) {
        let key = crate::deck_profile::key_for(id);
        if current.0.as_deref() == Some(&key) {
            return;
        }
        let json = current.1.take_unsaved();
        let pending = current.0.clone().zip(json);
        current.1 = swap_profile(store, pending, &key);
        current.0 = Some(key);
    }

    #[test]
    fn profile_follows_the_treadmill_that_connects() {
        use crate::deck_profile::{DeckProfile, Outcome, LEGACY_KEY};
        use crate::persistence::SettingsStore;
        let store = crate::persistence::memory_db();
        let mut old = DeckProfile::default();
        old.learn(40, Outcome::Took(40));
        store
            .put(LEGACY_KEY, &serde_json::to_string(&old).unwrap())
            .unwrap();

        let mut current = (None, DeckProfile::default());
        // The first treadmill adopts the old app-wide profile, which moves.
        connect(&store, &mut current, "AAAA");
        assert!(current.1.knows(40));
        assert!(store.get(LEGACY_KEY).is_none());
        assert!(store.get(&crate::deck_profile::key_for("AAAA")).is_some());

        // It learns one more speed, then a second treadmill starts empty.
        current.1.learn(50, Outcome::Took(50));
        connect(&store, &mut current, "BBBB");
        assert!(current.1.is_empty());

        // Back to the first: what it learned is there, and the second's
        // empty profile left nothing behind.
        connect(&store, &mut current, "AAAA");
        assert!(current.1.knows(40) && current.1.knows(50));
        assert!(store.get(&crate::deck_profile::key_for("BBBB")).is_none());
    }

    #[test]
    fn profile_learned_on_the_second_treadmill_stays_with_it() {
        use crate::deck_profile::{DeckProfile, Outcome};
        let store = crate::persistence::memory_db();
        let mut current = (None, DeckProfile::default());
        connect(&store, &mut current, "AAAA");
        connect(&store, &mut current, "BBBB");
        current.1.learn(60, Outcome::Took(60));
        connect(&store, &mut current, "AAAA");
        assert!(current.1.is_empty());
        connect(&store, &mut current, "BBBB");
        assert!(current.1.knows(60));
    }
}
