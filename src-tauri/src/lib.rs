//! Neon Strider: a desktop controller for FitShow Bluetooth treadmills.
//!
//! This file wires the app together: plugins, the tray icon, the startup
//! sequence, the treadmill link watchdog and the window lifecycle. The app
//! lives in the tray; closing or leaving its windows never quits it, only the
//! tray menu's Quit does.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

pub mod backup;
pub mod bluetooth;
pub mod calibration;
mod dashboard;
pub mod deck_profile;
pub mod defaults;
mod display;
pub mod easy_start;
pub mod easy_stop;
pub mod echo;
pub mod engine;
pub mod export;
pub mod ghost_pod;
pub mod guided_task;
pub mod heart_rate;
pub mod history;
mod hooks;
mod hud;
pub mod limits;
pub mod link;
pub mod logging;
pub mod neural;
pub mod persistence;
pub mod placement;
mod platform;
pub mod profile_sweep;
pub mod programs;
pub mod protocol;
pub mod secrets;
pub mod shortcuts;
pub mod speed_drift;
// The simulated deck: tests, and the dev builds' "simulated treadmill"
// choice (`sim_link`). Never in release builds.
#[cfg(any(test, debug_assertions))]
mod sim;
#[cfg(debug_assertions)]
mod sim_link;
pub mod snapshot;
pub mod strava;
pub mod strava_plan;
pub mod strava_save;
pub mod strava_status;
pub mod strava_template;
pub mod traffic;
pub mod typing_speed;
pub mod update;
mod util;

// Used by `snapshot` under these names.
pub(crate) use display::current_monitors;
pub(crate) use hud::{ensure_hud_native, position_hud};

/// The release app's bundle identifier, as in `tauri.conf.json`. Its data,
/// logs, WebView data, Credential Manager entries and single-instance lock
/// are all keyed by it.
pub const RELEASE_IDENTIFIER: &str = "com.freshbreweddesign.neonstrider";
/// Dev builds (`cfg(debug_assertions)`) run under their own identifier, so
/// `npm run tauri dev` gets its own folders, credentials and single-instance
/// lock and never opens the real database.
pub const DEV_IDENTIFIER: &str = "com.freshbreweddesign.neonstrider.dev";
/// The identifier this build runs under.
pub const IDENTIFIER: &str = if cfg!(debug_assertions) {
    DEV_IDENTIFIER
} else {
    RELEASE_IDENTIFIER
};

/// The app's Tauri context. Release builds use `tauri.conf.json` as it is;
/// dev builds swap in [`DEV_IDENTIFIER`] before anything resolves a path.
#[cfg(not(debug_assertions))]
fn app_context() -> tauri::Context {
    tauri::generate_context!()
}

#[cfg(debug_assertions)]
fn app_context() -> tauri::Context {
    let mut context = tauri::generate_context!();
    context.config_mut().identifier = DEV_IDENTIFIER.to_string();
    context
}

/// Argument the autostart entry passes, so a login launch can be told apart.
const AUTOSTART_ARG: &str = "--autostart";
/// A tray click this soon after the dashboard closed on blur is the click that
/// took its focus away: opening it again would make the click a no-op toggle.
/// The blur from the same click lands within a few frames; 300 ms is shorter
/// than a deliberate second click.
const TRAY_CLICK_DEBOUNCE: Duration = Duration::from_millis(300);

/// The app, once it has started. Read by the input hooks, which must not wait
/// for it.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
/// Windows are being closed by `close_to_tray`; their own events are ignored.
static CLOSING: AtomicBool = AtomicBool::new(false);
/// Quit was chosen in the tray menu: let the app exit.
static QUITTING: AtomicBool = AtomicBool::new(false);
/// When the dashboard was last closed by `close_to_tray` or a blur.
static DASHBOARD_CLOSED_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// Let the app exit (or restart) next time it asks: Quit, or an update
/// relaunching it (`update.rs`). Otherwise closing never quits it.
pub(crate) fn allow_exit() {
    QUITTING.store(true, Ordering::SeqCst);
}

pub(crate) fn app_handle() -> Option<&'static tauri::AppHandle> {
    APP.get()
}

fn note_dashboard_closed() {
    *DASHBOARD_CLOSED_AT
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
}

fn dashboard_just_closed() -> bool {
    DASHBOARD_CLOSED_AT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some_and(|t| t.elapsed() < TRAY_CLICK_DEBOUNCE)
}

/// Whether the app opens at login (`platform`: the autostart entry on Windows,
/// a Login Item on macOS).
#[cfg(not(debug_assertions))]
#[tauri::command]
fn get_launch_at_login(app: tauri::AppHandle) -> Result<bool, String> {
    platform::launch_at_login(&app)
}

#[cfg(not(debug_assertions))]
#[tauri::command]
fn set_launch_at_login(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    platform::set_launch_at_login(&app, enabled)
}

/// Dev builds never launch at login: the autostart entry is named after the
/// product, so a dev build would replace the release app's own entry.
#[cfg(debug_assertions)]
#[tauri::command]
fn get_launch_at_login() -> Result<bool, String> {
    Ok(false)
}

#[cfg(debug_assertions)]
#[tauri::command]
fn set_launch_at_login(enabled: bool) -> Result<bool, String> {
    if enabled {
        Err("Dev builds never launch at login.".to_string())
    } else {
        Ok(false)
    }
}

/// Close the windows down to the tray: the dashboard unless pinned, the HUD
/// unless it has pinned widgets. Anything else stays.
fn close_to_tray(app: &tauri::AppHandle) {
    if CLOSING.swap(true, Ordering::SeqCst) {
        return;
    }
    let windows = app.webview_windows();
    if !windows.is_empty() {
        log::info!("Closing windows to the tray ({} open)", windows.len());
        let hud_pinned = persistence::has_pinned_elements(app);
        let dashboard_pinned = persistence::is_window_pinned(app) || guided_task::pinned();
        for (label, window) in windows {
            match label.as_str() {
                dashboard::DASHBOARD_LABEL if dashboard_pinned => {
                    log::debug!("Dashboard pinned; keeping it open")
                }
                dashboard::DASHBOARD_LABEL => {
                    log::debug!("Closing the dashboard");
                    let _ = window.destroy();
                    note_dashboard_closed();
                }
                hud::HUD_LABEL if !hud_pinned => {
                    log::debug!("Closing the HUD (no pins)");
                    let _ = window.destroy();
                }
                _ => log::debug!("Keeping {} open", label),
            }
        }
    }
    CLOSING.store(false, Ordering::SeqCst);
}

fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if CLOSING.load(Ordering::SeqCst) {
        return;
    }
    let is_dashboard = window.label() == dashboard::DASHBOARD_LABEL;
    match event {
        tauri::WindowEvent::CloseRequested { .. } => {
            log::info!("Close requested on {}; closing to the tray", window.label());
            close_to_tray(window.app_handle());
        }
        tauri::WindowEvent::Focused(false) if is_dashboard => {
            if dashboard::should_close_on_blur(window.app_handle()) {
                let _ = window.destroy();
                note_dashboard_closed();
                close_to_tray(window.app_handle());
            }
        }
        tauri::WindowEvent::Moved(pos) if is_dashboard => {
            dashboard::dashboard_moved(window.app_handle(), pos.x, pos.y);
        }
        _ => {}
    }
}

/// The tray icon (the menu bar icon on macOS): left click opens the
/// dashboard; right click (or Control-click) opens the menu
/// (`snapshot::tray_menu`: status, controls, programs, windows, Quit).
/// The snapshot emitter keeps its tooltip, icon and menu live.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let menu = snapshot::tray_menu::initial_menu(app)?;

    let mut tray = TrayIconBuilder::with_id(snapshot::tray::TRAY_ID)
        .tooltip("Neon Strider")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id.as_ref() == snapshot::tray_menu::QUIT_ID {
                log::info!("quitting...");
                allow_exit();
                app.exit(0);
            } else {
                snapshot::tray_menu::on_menu_event(app, event.id.as_ref());
            }
        })
        .on_tray_icon_event(|tray, event| {
            let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            else {
                return;
            };
            if dashboard_just_closed() {
                log::debug!("Tray click ignored (Debounce vs Blur)");
                return;
            }
            dashboard::show_dashboard(tray.app_handle());
        });
    // The tray's own art (icons/tray.png): the app icon's thin lines vanish at
    // tray size. Dimmed to start with: nothing is connected yet. In the macOS
    // menu bar, a template image macOS tints to suit the menu bar.
    let icon = snapshot::tray::icon(snapshot::tray::TrayState::Offline)
        .or_else(|| app.default_window_icon().cloned());
    if let Some(icon) = icon {
        tray = tray
            .icon(icon)
            .icon_as_template(platform::TRAY_ICON_TEMPLATE);
    }
    tray.build(app)?;
    Ok(())
}

/// Everything after the window system is up, in order: the database, settings
/// and programs, the snapshot emitter and background workers, the HUD if it
/// has pins, then the link watchdog for the life of the app.
async fn start_backend(app: tauri::AppHandle) {
    persistence::init_db(&app);
    backup::spawn_daily(app.clone());
    let config = persistence::load_config(&app);
    bluetooth::init_from_config(&config).await;
    if let Some(id) = &config.last_device_id {
        bluetooth::switch_deck_profile(&app, id).await;
    }
    profile_sweep::init(&app).await;
    easy_start::init(&app).await;
    easy_stop::init(&app).await;

    // Once per database: earlier builds saved bad session averages.
    let (app_c, tilt, offset) = (app.clone(), config.tilt_multiplier, config.incline_offset);
    let _ = tauri::async_runtime::spawn_blocking(move || {
        persistence::backfill_session_averages(&app_c, tilt, offset);
    })
    .await;

    snapshot::init(&app);
    shortcuts::init(&app);
    strava::init(&app);
    programs::init(&app).await;
    snapshot::spawn_emitter(app.clone());
    snapshot::desk::spawn(app.clone());
    strava::spawn_worker(app.clone());
    update::init(&app);
    update::spawn(app.clone());
    ghost_pod::init(&app);
    ghost_pod::spawn(app.clone());
    heart_rate::init(&app);
    speed_drift::init(&app);
    typing_speed::init(&app);
    heart_rate::spawn(app.clone());

    if persistence::has_pinned_elements(&app) {
        log::info!("Pinned elements found. Auto-spawning HUD.");
        hud::ensure_hud_native(&app);
    }

    run_link_watchdog(&app).await;
}

/// Reconnect to the saved treadmill whenever the link is down, backing off
/// while the PC is idle and the treadmill stays away (`link::retry_delay`), or at once on "Retry now".
async fn run_link_watchdog(app: &tauri::AppHandle) -> ! {
    loop {
        let mut delay = link::RETRY_EVERY;
        if !bluetooth::is_bluetooth_connected().await {
            // Off the async threads: the watchdog shares them with the belt.
            let app_c = app.clone();
            let saved =
                util::blocking(move || Ok(persistence::load_config(&app_c).last_device_id)).await;
            let last_device_id = match saved {
                Ok(id) => {
                    link::set_saved_device(id.is_some());
                    id
                }
                // Tried again on the next round; the saved state shown stays.
                Err(e) => {
                    log::warn!("Saved treadmill not read: {}", e);
                    None
                }
            };
            if let Some(id) = last_device_id {
                link::count_retry(&id);
                let linked = bluetooth::connect_bluetooth_device(app.clone(), id)
                    .await
                    .is_ok();
                delay = link::retry_finished(linked);
            }
        }
        link::set_wake(delay);
        // Re-checks the PC in slices and keeps the windows' countdown right.
        link::wait_for_retry(delay).await;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::install_panic_hook();
    // Installed for the life of the app: the shortcuts, the last-input time,
    // the typing count and the HUD's click-through all need them.
    platform::install_input_hooks();

    let app = tauri::Builder::default()
        // Must be registered first. A second launch exits and opens the dashboard
        // here instead (except an autostart launch, which stays tray-only).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !args.iter().any(|a| a == AUTOSTART_ARG) {
                dashboard::show_dashboard(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .plugin(tauri_plugin_opener::init())
        // Driven from Rust (`update.rs`); no window has updater permissions.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(logging::plugin())
        .setup(|app| {
            logging::log_startup(app.handle());
            build_tray(app)?;
            // On macOS: no Dock icon, and the shortcuts (main thread, app up).
            platform::app_started(app.handle());
            let _ = APP.set(app.handle().clone());
            tauri::async_runtime::spawn(start_backend(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bluetooth::scan_bluetooth_devices,
            bluetooth::connect_bluetooth_device,
            bluetooth::send_bluetooth_command,
            strava::strava_tcx_preview,
            strava::strava_template_preview,
            strava::strava_save_for_strava,
            get_launch_at_login,
            set_launch_at_login,
            bluetooth::sync_orchestrator_config,
            persistence::get_rust_config,
            history::history_sessions,
            history::history_older,
            history::history_stats,
            persistence::get_samples,
            persistence::delete_session,
            persistence::get_db_setting,
            persistence::set_db_setting,
            snapshot::get_snapshot,
            snapshot::dispatch,
            hud::set_hud_hit_region,
            logging::open_log_folder,
            export::export_sessions_csv,
            echo::program_from_session
        ])
        .on_window_event(on_window_event)
        .build(app_context())
        // Nothing can run without it; the panic hook logs why.
        .expect("error building tauri application");
    app.run(|_app, event| {
        // The app lives in the tray: closing its windows never quits it.
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if QUITTING.load(Ordering::SeqCst) {
                log::info!("Exit allowed.");
            } else {
                api.prevent_exit();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Release builds run under `tauri.conf.json`'s identifier, unchanged;
    /// dev and test builds run under their own.
    #[test]
    fn identifiers() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], RELEASE_IDENTIFIER);
        assert_ne!(DEV_IDENTIFIER, RELEASE_IDENTIFIER);
        #[cfg(debug_assertions)]
        assert_eq!(IDENTIFIER, DEV_IDENTIFIER);
        #[cfg(not(debug_assertions))]
        assert_eq!(IDENTIFIER, RELEASE_IDENTIFIER);
    }
}
