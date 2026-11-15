//! File logging, panic capture, and the "open log folder" command.
//!
//! The release build has no console, so everything worth knowing after the
//! fact goes through the `log` crate into the app's log folder.

use log::LevelFilter;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
use tauri_plugin_opener::OpenerExt;

/// Base name of the log file (`neon-strider.log`).
const LOG_FILE: &str = "neon-strider";
/// Rotate at about 1 MB.
const MAX_FILE_BYTES: u128 = 1_000_000;
/// Older log files kept after rotation.
const KEEP_ROTATED: usize = 3;

/// Debug in development builds, info in release builds.
pub fn level() -> LevelFilter {
    if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    }
}

/// The log plugin: rotating file in the app log dir, plus stdout in debug builds.
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let mut targets = vec![Target::new(TargetKind::LogDir {
        file_name: Some(LOG_FILE.into()),
    })];
    if cfg!(debug_assertions) {
        targets.push(Target::new(TargetKind::Stdout));
    }
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .targets(targets)
        .level(level())
        // Dependencies stay at warnings: their request and radio traces can
        // carry URLs with OAuth codes and are not useful here.
        .level_for("reqwest", LevelFilter::Warn)
        .level_for("hyper", LevelFilter::Warn)
        .level_for("tiny_http", LevelFilter::Warn)
        .level_for("btleplug", LevelFilter::Warn)
        .level_for("tao", LevelFilter::Warn)
        .level_for("wry", LevelFilter::Warn)
        .level_for("mio", LevelFilter::Warn)
        .max_file_size(MAX_FILE_BYTES)
        .rotation_strategy(RotationStrategy::KeepSome(KEEP_ROTATED))
        .build()
}

/// Log panics (message, location, thread) before the default hook runs.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-text panic payload".to_string());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        let thread = std::thread::current();
        log::error!(
            "PANIC on thread '{}' at {}: {}",
            thread.name().unwrap_or("unnamed"),
            location,
            message
        );
        log::logger().flush();
        default_hook(info);
    }));
}

/// The first line of every run: version and log level.
pub fn log_startup(app: &AppHandle) {
    log::info!(
        "Neon Strider {} starting (log level {})",
        app.package_info().version,
        level()
    );
}

/// Open the log folder in the file manager.
#[tauri::command]
pub fn open_log_folder(app: AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("Could not find the log folder: {}", e))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| format!("Could not open {}: {}", dir.display(), e))
}
