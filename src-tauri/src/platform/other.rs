//! Stand-ins for operating systems other than Windows and macOS: each feature reports
//! "unsupported". No input is ever seen (so the sit-too-long nudge and the
//! walk-away idle timer never fire), the session is never locked, the display
//! is never off (so lights out never fires), and the microphone is always
//! free. PC idle time is unknown, so updates never install by themselves. The
//! HUD stays click-through. There is no Bluetooth peripheral, so GHOST_POD
//! says it isn't supported.

pub const DESKTOP_IN_POINTS: bool = false;
pub const MENU_BAR_APP: bool = false;
pub const TRAY_ICON_TEMPLATE: bool = false;
pub const MICROPHONE: bool = false;
pub const MINIFY_KEY_CHOICE: bool = false;

pub const WORDS: super::Words = super::Words {
    computer: "computer",
    settings: "the system settings",
    bluetooth_blocked: "The system blocked Bluetooth access. Allow it in the system settings.",
    credential_store: "the system keyring",
    release_download: "download for this system",
};

pub fn app_started(_app: &tauri::AppHandle) {}

pub fn prepare_popup_window(_window: &tauri::WebviewWindow) {}

pub fn install_input_hooks() {
    log::info!("Global input hooks are not supported on this OS");
}

/// No single-instance lock to look for here, so no other instance is seen.
pub fn app_instance_running(_identifier: &str) -> bool {
    false
}

pub fn remember_hud_window(_window: &tauri::WebviewWindow) {}

pub fn spawn_session_lock_watcher() {
    log::info!("Session lock events are not supported on this OS");
}

pub fn session_locked() -> bool {
    false
}

pub fn display_off() -> bool {
    false
}

pub fn apps_using_microphone(_own_exe: Option<&str>) -> Vec<String> {
    Vec::new()
}

pub fn primary_button_down() -> bool {
    false
}

pub fn cursor_position() -> Option<(i32, i32)> {
    None
}

pub fn input_idle_for() -> Option<std::time::Duration> {
    None
}

pub fn peripheral_add(
    _service: &super::PeripheralService,
    _events: super::PeripheralEvents,
) -> Result<(), String> {
    Err("The watch pod only works on Windows for now.".to_string())
}

pub fn peripheral_notify(_uuid: u128, _value: &[u8]) -> Result<(), String> {
    Ok(())
}

pub fn peripheral_remove(_service: u128) {}

pub fn peripheral_stop() {}
