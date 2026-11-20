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
//!
//! `windows.rs` implements it. `other.rs` is the stand-in for every other OS:
//! each feature reports "unsupported" (no input seen, never locked, microphone
//! free, idle time unknown), so the app runs without them.

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
