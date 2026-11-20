//! The operating-system boundary. Everything here is OS-specific; the rest of
//! the crate calls these functions and never the OS directly.
//!
//! - Input hooks: system-wide keyboard and mouse hooks that record the time of
//!   the last input and a count of typing key presses (`crate::hooks`), fire the
//!   Alt shortcuts, and switch the HUD's click-through as the pointer crosses
//!   its content (`crate::hud`).
//! - Session lock: whether the user's session is locked (walk-away).
//! - Display power: whether the PC's display is off (lights out).
//! - Microphone: which other apps are using the microphone (slow down on calls).
//! - Pointer: the cursor position and primary button, for dragging the HUD.
//! - Idle time: how long since the last keyboard or mouse input anywhere in
//!   the session (auto-install of updates).
//! - Bluetooth peripheral: GATT services the app serves to other devices,
//!   such as a watch (GHOST_POD, `crate::ghost_pod`), one provider each. The treadmill link is a
//!   central and goes through btleplug instead.
//!
//! `windows.rs` implements it. `other.rs` is the stand-in for every other OS:
//! each feature reports "unsupported" (no input seen, never locked, microphone
//! free, idle time unknown, no Bluetooth peripheral), so the app runs without them.

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(not(windows))]
#[path = "other.rs"]
mod imp;

/// Install the keyboard and mouse hooks on a thread of their own, for the life
/// of the app. Call once, before the event loop starts.
pub fn install_input_hooks() {
    imp::install_input_hooks();
}

/// Remember the HUD window, so the mouse hook can read its real click-through
/// state on every move and correct it.
pub fn remember_hud_window(window: &tauri::WebviewWindow) {
    imp::remember_hud_window(window);
}

/// Start watching for the session locking and unlocking, and for the display
/// turning off and on (`display_off`). Call once.
pub fn spawn_session_lock_watcher() {
    imp::spawn_session_lock_watcher();
}

/// The user's session is locked right now.
pub fn session_locked() -> bool {
    imp::session_locked()
}

/// Short names (exe file or package name, never a folder) of the apps using
/// the microphone right now, except the one at `own_exe`. Blocking.
pub fn apps_using_microphone(own_exe: Option<&str>) -> Vec<String> {
    imp::apps_using_microphone(own_exe)
}

/// The PC's display is off right now (the power plan turned it off, say).
/// Dimmed is not off. False until the watcher has heard the first state.
pub fn display_off() -> bool {
    imp::display_off()
}

/// The primary mouse button is held down.
pub fn primary_button_down() -> bool {
    imp::primary_button_down()
}

/// The cursor in physical desktop pixels (the coordinate space of
/// `placement` and the mouse hook).
pub fn cursor_position() -> Option<(i32, i32)> {
    imp::cursor_position()
}

/// How long since the last keyboard or mouse input in the user's session;
/// `None` when the OS can't say.
pub fn input_idle_for() -> Option<std::time::Duration> {
    imp::input_idle_for()
}

/// A 16-bit Bluetooth SIG number as a full UUID (on the Bluetooth base UUID).
pub const fn sig_uuid(short: u16) -> u128 {
    0x0000_0000_0000_1000_8000_0080_5f9b_34fb | ((short as u128) << 96)
}

/// A UUID as the log writes it: the four hex digits of a SIG number
/// (`1814`), or the full 8-4-4-4-12 form for a custom one.
pub fn uuid_text(u: u128) -> String {
    let base = sig_uuid(0);
    if u & !(0xffffu128 << 96) == base {
        return format!("{:04x}", (u >> 96) as u16);
    }
    let h = format!("{u:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// One characteristic of a peripheral service.
#[derive(Debug, Clone, PartialEq)]
pub struct PeripheralCharacteristic {
    pub uuid: u128,
    /// The value to answer reads with at first (`peripheral_notify` keeps it
    /// current); `None` when it can't be read.
    pub read: Option<Vec<u8>>,
    pub notify: bool,
    pub indicate: bool,
    /// Writable (with response).
    pub write: bool,
}

/// A service the app serves, with its characteristics. Each one is its own
/// provider: adding or removing one leaves the others (and the devices
/// connected to them) alone.
#[derive(Debug, Clone, PartialEq)]
pub struct PeripheralService {
    /// The name the log uses (`RSC`, `POWER`, `WRIST_LINK`).
    pub name: &'static str,
    pub uuid: u128,
    /// Put the service's UUID in the advertisement. Off, the service is still
    /// served and connectable, and a device finds it after connecting to the
    /// PC through another service's advertisement.
    pub advertise: bool,
    /// A second, legacy (31-byte) advertisement that runs while the service
    /// is served, carrying this manufacturer data. For a scanner that hears
    /// only legacy advertisements or can't read the service's own one.
    pub beacon: Option<PeripheralBeacon>,
    pub characteristics: Vec<PeripheralCharacteristic>,
}

/// Manufacturer data (AD type 0xFF) for a service's legacy beacon: the
/// company ID and the bytes after it.
#[derive(Debug, Clone, PartialEq)]
pub struct PeripheralBeacon {
    pub company: u16,
    pub data: Vec<u8>,
}

impl PeripheralBeacon {
    /// Bytes on air: the flags Windows adds (3) plus this section's length,
    /// type and company ID (4) and the data. A legacy advertisement holds 31.
    pub fn air_len(&self) -> usize {
        3 + 4 + self.data.len()
    }
}

/// What the peripheral reports, from a Bluetooth thread. `service` is the
/// service UUID the event belongs to.
#[derive(Debug, Clone, PartialEq)]
pub enum PeripheralEvent {
    /// The service's advertisement started: devices can find it and connect.
    Advertising { service: u128 },
    /// The service's advertisement stopped by itself; the reason in plain
    /// English. Windows can report this and then start it again a moment later.
    Failed { service: u128, why: String },
    /// How many devices are subscribed to this characteristic now.
    Subscribers { uuid: u128, count: u32 },
    /// A device wrote this value (already acknowledged).
    Write { uuid: u128, value: Vec<u8> },
}

pub type PeripheralEvents = std::sync::Arc<dyn Fn(PeripheralEvent) + Send + Sync>;

/// Serve `service` and start advertising it, plus its legacy beacon when it
/// has one (a beacon that fails is logged and leaves the service up).
/// Blocking (a second or so).
/// Adding a service that is already served replaces only that one. The error
/// is one plain sentence (no adapter, no peripheral role, Bluetooth off).
pub fn peripheral_add(service: &PeripheralService, events: PeripheralEvents) -> Result<(), String> {
    imp::peripheral_add(service, events)
}

/// Stop advertising one service and drop it; the others keep running. Fine
/// to call when it isn't served.
pub fn peripheral_remove(service: u128) {
    imp::peripheral_remove(service)
}

/// Send `value` to every device subscribed to `uuid` (a notification or an
/// indication, as the characteristic was declared). Does not wait.
pub fn peripheral_notify(uuid: u128, value: &[u8]) -> Result<(), String> {
    imp::peripheral_notify(uuid, value)
}

/// Stop advertising and drop every service. Fine to call when not started.
pub fn peripheral_stop() {
    imp::peripheral_stop()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sig_uuids_log_short_and_custom_ones_in_full() {
        assert_eq!(uuid_text(sig_uuid(0x1814)), "1814");
        assert_eq!(uuid_text(sig_uuid(0x2a53)), "2a53");
        assert_eq!(
            uuid_text(0x00b165d6_40c2_4f94_8df9_0f69431f21af),
            "00b165d6-40c2-4f94-8df9-0f69431f21af"
        );
    }
}
