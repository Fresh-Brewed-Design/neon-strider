//! The operating-system boundary. Everything here is OS-specific; the rest of
//! the crate calls these functions and never the OS directly.
//!
//! - Input hooks: system-wide keyboard and mouse hooks that record the time of
//!   the last input and a count of typing key presses (`crate::hooks`), fire the
//!   Alt shortcuts, and switch the HUD's click-through as the pointer crosses
//!   its content (`crate::hud`).
//! - Other instances: whether another build of the app is running (a dev build
//!   leaves the Alt shortcuts to the release app).
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
//! - Desktop units: the one coordinate space the HUD's placement, its hit
//!   rectangle and the cursor share (`DESKTOP_IN_POINTS`; `crate::display`
//!   converts at Tauri's edge).
//!
//! `windows.rs` implements it for Windows and `macos.rs` for macOS (with the
//! pure key tables in `macos_keys.rs`). `other.rs` is the stand-in for every
//! other OS: each feature reports "unsupported" (no input seen, never locked,
//! microphone free, idle time unknown, no Bluetooth peripheral), so the app
//! runs without them. A feature one OS lacks is hidden in the UI through
//! `snapshot.platform` ([`snapshot`]).

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod imp;

#[cfg(not(any(windows, target_os = "macos")))]
#[path = "other.rs"]
mod imp;

#[cfg(any(target_os = "macos", test))]
mod macos_keys;

use crate::hooks::Shortcut;
use serde::{Deserialize, Serialize};

/// Desktop units are points (macOS), not physical pixels (Windows). AppKit
/// places windows in points, and Tauri's physical pixels there are points
/// times a scale that differs per window and per display.
pub const DESKTOP_IN_POINTS: bool = imp::DESKTOP_IN_POINTS;

/// The app lives in the menu bar (macOS): the dashboard drops down from its
/// icon, rather than opening where it was last left like a tray popup.
pub const MENU_BAR_APP: bool = imp::MENU_BAR_APP;

/// The tray icon is a template image the OS tints for the menu bar
/// (`tray_art::template`), not the colour art.
pub const TRAY_ICON_TEMPLATE: bool = imp::TRAY_ICON_TEMPLATE;

/// How the OS names its own parts, for messages the user reads.
pub struct Words {
    /// The computer: "PC", "Mac".
    pub computer: &'static str,
    /// Where Bluetooth is switched on: "Windows settings", "System Settings".
    pub settings: &'static str,
    /// The sentence for an OS that refused the app Bluetooth, saying where to allow it.
    pub bluetooth_blocked: &'static str,
    /// Where secrets are kept: "Windows Credential Manager", "the Keychain".
    pub credential_store: &'static str,
    /// What a release carries for this OS: "Windows installer", "Mac download".
    pub release_download: &'static str,
}

/// This OS's words.
pub const WORDS: Words = imp::WORDS;

/// What the app is running on, for the UI's wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum Os {
    Windows,
    Macos,
    Other,
}

/// The OS and which OS-only features it has, so the UI words things the
/// way the OS does and hides what it lacks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PlatformSnapshot {
    pub os: Os,
    /// The computer, as the OS calls it: "PC", "Mac" (`WORDS`).
    pub computer: String,
    /// Where settings such as Bluetooth are changed: "Windows settings",
    /// "System Settings" (`WORDS`).
    pub settings: String,
    /// Slow down on calls: the apps using the microphone can be listed.
    pub microphone: bool,
    /// The HUD's minify hotkey can be put on another key (Windows: Insert,
    /// Pause or Scroll Lock); otherwise only switched on or off.
    pub minify_key_choice: bool,
    /// The global shortcuts as this OS writes them (`Alt+Home`, `⌃⌥↩`).
    pub shortcuts: ShortcutLabels,
}

/// Each global shortcut's keys as this OS writes them. On Windows the minify
/// key is the HUD setting's (`toggle_minified` is its default, Insert).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ShortcutLabels {
    pub speed_up: String,
    pub speed_down: String,
    pub incline_up: String,
    pub incline_down: String,
    pub toggle_start: String,
    pub toggle_program: String,
    pub toggle_minified: String,
    pub calibration_tap: String,
}

impl ShortcutLabels {
    fn from(label: impl Fn(Shortcut) -> String) -> Self {
        Self {
            speed_up: label(Shortcut::SpeedUp),
            speed_down: label(Shortcut::SpeedDown),
            incline_up: label(Shortcut::InclineUp),
            incline_down: label(Shortcut::InclineDown),
            toggle_start: label(Shortcut::ToggleStart),
            toggle_program: label(Shortcut::ToggleProgram),
            toggle_minified: label(Shortcut::ToggleMinified),
            calibration_tap: label(Shortcut::CalibrationTap),
        }
    }
}

/// The OS this build runs on, and its features.
pub fn snapshot() -> PlatformSnapshot {
    let os = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Other
    };
    PlatformSnapshot {
        os,
        computer: WORDS.computer.to_string(),
        settings: WORDS.settings.to_string(),
        microphone: imp::MICROPHONE,
        minify_key_choice: imp::MINIFY_KEY_CHOICE,
        shortcuts: ShortcutLabels::from(shortcut_label),
    }
}

/// A shortcut as this OS writes it. Pure, for every OS's build.
fn shortcut_label(shortcut: Shortcut) -> String {
    #[cfg(target_os = "macos")]
    return macos_keys::label(shortcut);
    #[cfg(not(target_os = "macos"))]
    return windows_shortcut_label(shortcut);
}

/// The Windows shortcuts, as `windows.rs` maps them (Alt plus a key).
#[cfg(not(target_os = "macos"))]
fn windows_shortcut_label(shortcut: Shortcut) -> String {
    let key = match shortcut {
        Shortcut::SpeedUp => "=",
        Shortcut::SpeedDown => "-",
        Shortcut::InclineUp => "Page Up",
        Shortcut::InclineDown => "Page Down",
        Shortcut::ToggleStart => "Home",
        Shortcut::ToggleProgram => "End",
        Shortcut::ToggleMinified => "Insert",
        Shortcut::CalibrationTap => "Space",
    };
    format!("Alt+{key}")
}

/// Once the app is up, on the main thread: OS setup that needs the app
/// (on macOS: no Dock icon, and the shortcuts).
pub fn app_started(app: &tauri::AppHandle) {
    imp::app_started(app);
}

/// Make the dashboard window behave like the OS's own dropdowns (on macOS:
/// it opens on the Space in use, over a full-screen app too).
pub fn prepare_popup_window(window: &tauri::WebviewWindow) {
    imp::prepare_popup_window(window);
}

/// Install the keyboard and mouse hooks on a thread of their own, for the life
/// of the app. Call once, before the event loop starts.
pub fn install_input_hooks() {
    imp::install_input_hooks();
}

/// Another instance of a Tauri app with this bundle identifier is running in
/// this user session: its single-instance lock exists. `tauri-plugin-single-instance`
/// names that lock after the identifier (on Windows a mutex,
/// `{identifier}-sim`). Fast enough for the hook thread, but not free: call it
/// per shortcut, not per key press.
pub fn app_instance_running(identifier: &str) -> bool {
    imp::app_instance_running(identifier)
}

/// Remember the HUD window, so the mouse hook can read its real click-through
/// state on every move and correct it (and, on macOS, keep it on every Space).
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

/// The cursor in desktop units (the coordinate space of `placement` and the
/// mouse hook; see `DESKTOP_IN_POINTS`).
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
    #[cfg_attr(
        not(any(windows, test)),
        expect(dead_code, reason = "only Windows serves beacons so far (#33)")
    )]
    /// Bytes on air: the flags Windows adds (3) plus this section's length,
    /// type and company ID (4) and the data. A legacy advertisement holds 31.
    pub fn air_len(&self) -> usize {
        3 + 4 + self.data.len()
    }
}

/// What the peripheral reports, from a Bluetooth thread. `service` is the
/// service UUID the event belongs to.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "only Windows serves a peripheral so far (#33)")
)]
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
