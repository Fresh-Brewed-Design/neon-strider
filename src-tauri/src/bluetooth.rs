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
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// The Bluetooth adapter, once found.
static ADAPTER: Mutex<Option<Adapter>> = Mutex::const_new(None);
/// The treadmill of the current (or last) connection. Lock order: this before
/// `ORCHESTRATOR`.
pub(crate) static CONNECTED_PERIPHERAL: Mutex<Option<Peripheral>> = Mutex::const_new(None);
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
const SCAN_TIME: Duration = Duration::from_secs(5);
/// Settle times around a connect: after dropping any old link, after the
/// connect, and after service discovery.
const DISCONNECT_SETTLE: Duration = Duration::from_millis(1500);
const CONNECT_SETTLE: Duration = Duration::from_millis(500);
const DISCOVER_SETTLE: Duration = Duration::from_millis(500);
/// Tries per characteristic to subscribe to status frames, and the wait between.
const SUBSCRIBE_TRIES: u32 = 3;
const SUBSCRIBE_RETRY: Duration = Duration::from_millis(200);
/// The protocol worker's tick, and its slower tick while the deck is stopped.
const WORKER_TICK: Duration = Duration::from_millis(50);
const WORKER_TICK_STOPPED: Duration = Duration::from_millis(200);
/// Neural flow and programs tick this often.
const SECOND_TICK: Duration = Duration::from_secs(1);
/// How long after Neural flow changes the target the deck's speed change is
/// treated as its own (the deck ramps in steps over a few seconds).
const NEURAL_RAMP_QUIET: Duration = Duration::from_secs(10);
/// A speed change smaller than this (km/h, raw) gets no toast.
const SPEED_TOAST_MIN_KMH: f32 = 0.1;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BleDevice {
    pub id: String,
    pub name: String,
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
    crate::link::log_frame_flow();
    state.tick_program(now)
}

/// Write one packet; a successful write restarts the engine's command gap.
async fn write_packet(peripheral: &Peripheral, packet: &Packet) {
    let Ok(characteristic) = find_control_characteristic(peripheral) else {
        return;
    };
    if let Err(e) = peripheral
        .write(&characteristic, &packet.bytes, WriteType::WithResponse)
        .await
    {
        log::error!("Write failed: {}", e);
        return;
    }
    ORCHESTRATOR.lock().await.last_cmd_time = Instant::now();
    if packet.kind != PacketKind::PollStatus {
        log::debug!("SENT_{}: {:?}", packet.kind.label(), packet.bytes);
    }
}

/// Drive one connection until the link drops or a newer worker takes over.
pub async fn spawn_protocol_worker(app: tauri::AppHandle, peripheral: Peripheral) {
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
            peripheral.id(),
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
        if !peripheral.is_connected().await.unwrap_or(false) {
            log::info!("Peripheral disconnected. Terminating worker.");
            crate::hooks::set_key_counting(false);
            return;
        }

        let now = Instant::now();
        let (packet, program_toasts) = {
            let mut state = ORCHESTRATOR.lock().await;
            let warning = if now.duration_since(last_second) >= SECOND_TICK {
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
            write_packet(&peripheral, &packet).await;
        }

        let stopped = {
            let mut state = ORCHESTRATOR.lock().await;
            if state.should_start_session() {
                match crate::persistence::db_create_session(&app) {
                    Ok(id) => state.start_session(id),
                    Err(e) => log::error!("Failed to create session: {}", e),
                }
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
        if t.status != self.status {
            out.status = Some(t.status);
        }
        self.status = t.status;

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
fn apply_telemetry(state: &mut OrchestratorState, t: &Telemetry) {
    state.last_status = t.status;
    state.unknown_status_code = (!t.status_known).then_some(t.status_code);
    state.hardware_speed = t.speed_kmh;
    state.hardware_incline = t.incline;
    state.hardware_time = t.time_sec;
    state.awaiting_first_packet = false;
}

/// Handle status frames until the notification stream ends.
async fn listen_for_status(
    app: tauri::AppHandle,
    mut stream: impl Stream<Item = ValueNotification> + Unpin,
) {
    let mut toasts = ToastTracker::default();
    while let Some(data) = stream.next().await {
        if !has_tag(&data.uuid, STATUS_TAG) {
            continue;
        }
        let Some(t) = protocol::parse_status(&data.value) else {
            continue;
        };
        crate::link::mark_frame();
        crate::link::note_status_byte(t.status_code, t.status_known.then_some(t.status));

        let mut state = ORCHESTRATOR.lock().await;
        apply_telemetry(&mut state, &t);
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

        let neural_easing = state
            .neural_changed_at
            .is_some_and(|at| at.elapsed() < NEURAL_RAMP_QUIET);
        let changes = toasts.on_frame(&t, neural_easing);
        if let Some(status) = changes.status {
            let name = format!("{:?}", status).to_uppercase();
            crate::hud::toast(
                &app,
                serde_json::json!({ "type": "STATUS", "message": format!("STATUS: {}", name), "isActionable": true }),
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
    match scan_inner().await {
        Ok(devices) => {
            crate::link::adapter_ok();
            Ok(devices)
        }
        Err(raw) => {
            let err = crate::link::describe(&raw);
            crate::link::note_error(&err, &raw);
            Err(err)
        }
    }
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
    found.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(found.into_iter().map(|(_, d)| d).collect())
}

#[tauri::command]
pub async fn connect_bluetooth_device(
    app_handle: tauri::AppHandle,
    id: String,
) -> Result<(), crate::link::LinkError> {
    {
        let mut state = ORCHESTRATOR.lock().await;
        if state.is_connecting {
            log::info!(
                "Connection attempt already in progress. Ignoring request for {}",
                id
            );
            return Ok(());
        }
        state.is_connecting = true;
    }

    crate::link::begin_connect();
    let result = connect_to(&app_handle, &id).await;
    ORCHESTRATOR.lock().await.is_connecting = false;

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

/// Whether the treadmill link is up, asked of the peripheral itself (the slot
/// is only cleared when the next connect begins, so a dropped link still fills
/// it).
pub async fn is_bluetooth_connected() -> bool {
    // Cloned out so the slot isn't locked across the BLE query, which would
    // stall a connect attempt waiting on it.
    let peripheral = CONNECTED_PERIPHERAL.lock().await.clone();
    match peripheral {
        Some(p) => p.is_connected().await.unwrap_or(false),
        None => false,
    }
}

/// A device id without separators, upper case, for matching.
fn normalize_id(id: &str) -> String {
    id.replace([':', '-'], "").to_uppercase()
}

/// Two normalized ids name the same device. Either may be the shorter form
/// (Windows ids carry a prefix the saved id may lack, or the other way round).
fn ids_match(a: &str, b: &str) -> bool {
    a.contains(b) || b.contains(a)
}

async fn connect_to(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let central = find_central().await.map_err(|e| e.to_string())?;

    {
        let mut connected = CONNECTED_PERIPHERAL.lock().await;
        if let Some(existing) = connected.take() {
            log::debug!("Disconnecting existing active link before new connection...");
            let _ = existing.disconnect().await;
        }
        ORCHESTRATOR.lock().await.reset_connection();
        log::debug!("Readiness reset for new node entry.");
    }

    let target = normalize_id(id);
    let mut peripherals = central.peripherals().await.map_err(|e| e.to_string())?;
    log::info!(
        "Discovered {} peripherals. Target: {}",
        peripherals.len(),
        id
    );

    let known = peripherals
        .iter()
        .any(|p| ids_match(&normalize_id(&p.id().to_string()), &target));
    if !known {
        log::info!("Target {} not in cache. Starting 5s scan...", id);
        let _ = central.start_scan(ScanFilter::default()).await;
        tokio::time::sleep(SCAN_TIME).await;
        let _ = central.stop_scan().await;
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
            return connect_with_retries(app, &peripheral, &raw_id, id).await;
        }
    }
    Err("Device not found".to_string())
}

/// Up to `link::CONNECT_TRIES` full cycles of disconnect, connect, discover
/// and subscribe.
async fn connect_with_retries(
    app: &tauri::AppHandle,
    peripheral: &Peripheral,
    raw_id: &str,
    id: &str,
) -> Result<(), String> {
    for attempt in 1..=crate::link::CONNECT_TRIES {
        crate::link::set_attempt(attempt);
        log::info!(
            "Connection attempt {}/{}...",
            attempt,
            crate::link::CONNECT_TRIES
        );
        let _ = peripheral.disconnect().await;
        tokio::time::sleep(DISCONNECT_SETTLE).await;

        if let Err(e) = peripheral.connect().await {
            log::warn!("Connection failed: {}. Retrying...", e);
            continue;
        }
        tokio::time::sleep(CONNECT_SETTLE).await;

        if peripheral.discover_services().await.is_err() {
            let _ = peripheral.disconnect().await;
            continue;
        }
        tokio::time::sleep(DISCOVER_SETTLE).await;

        if subscribe_and_start(app, peripheral, raw_id).await? {
            return Ok(());
        }
        let _ = peripheral.disconnect().await;
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
            if peripheral.subscribe(characteristic).await.is_err() {
                tokio::time::sleep(SUBSCRIBE_RETRY).await;
                continue;
            }
            let stream = peripheral
                .notifications()
                .await
                .map_err(|e| e.to_string())?;
            tokio::spawn(listen_for_status(app.clone(), stream));
            tokio::spawn(spawn_protocol_worker(app.clone(), peripheral.clone()));
            *CONNECTED_PERIPHERAL.lock().await = Some(peripheral.clone());

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

#[tauri::command]
pub async fn sync_orchestrator_config(
    app: tauri::AppHandle,
    config: crate::persistence::AppConfig,
) -> Result<(), String> {
    let saved_id = config.last_device_id.clone();
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

/// Write raw bytes to the deck (the diagnostics page).
#[tauri::command]
pub async fn send_bluetooth_command(data: Vec<u8>) -> Result<(), String> {
    log::debug!("Raw command from diagnostics: {:02X?}", data);
    let peripheral = CONNECTED_PERIPHERAL.lock().await.clone();
    if let Some(peripheral) = peripheral {
        if let Ok(characteristic) = find_control_characteristic(&peripheral) {
            peripheral
                .write(&characteristic, &data, WriteType::WithResponse)
                .await
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    Err("No device connected".to_string())
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
                },
                protocol::CalibrationPoint {
                    speed: 1.0,
                    stride_length_cm: 55.0,
                    distance_scale: 0.8,
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
    fn the_fitshow_service_uuid() {
        assert_eq!(
            FITSHOW_SERVICE.to_string(),
            "0000fff0-0000-1000-8000-00805f9b34fb"
        );
        assert!(has_tag(&FITSHOW_SERVICE, SERVICE_TAG));
    }
}
