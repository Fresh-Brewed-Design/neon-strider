//! Stand-ins for operating systems other than Windows: each feature reports
//! "unsupported". No input is ever seen (so the sit-too-long nudge and the
//! walk-away idle timer never fire), the session is never locked, the display
//! is never off (so lights out never fires), and the microphone is always
//! free. PC idle time is unknown, so updates never install by themselves. The
//! HUD stays click-through.

pub fn install_input_hooks() {
    log::info!("Global input hooks are not supported on this OS");
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
