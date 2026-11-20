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
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

pub mod backup;
pub mod bluetooth;
mod dashboard;
mod display;
pub mod engine;
pub mod export;
mod hooks;
mod hud;
pub mod link;
pub mod logging;
pub mod neural;
pub mod persistence;
pub mod placement;
mod platform;
pub mod programs;
pub mod protocol;
pub mod secrets;
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
pub mod update;
mod util;

// Used by `snapshot` under these names.
pub(crate) use display::current_monitors;
pub(crate) use hud::{ensure_hud_native, position_hud};

/// Argument the autostart entry passes, so a login launch can be told apart.
const AUTOSTART_ARG: &str = "--autostart";
const TRAY_MENU_QUIT: &str = "quit";
/// A tray click this soon after the dashboard closed on blur is the click that
/// took its focus away: opening it again would make the click a no-op toggle.
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

#[tauri::command]
fn get_launch_at_login(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_launch_at_login(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable().map_err(|e| e.to_string())?;
    } else {
        launcher.disable().map_err(|e| e.to_string())?;
    }
    launcher.is_enabled().map_err(|e| e.to_string())
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
        let dashboard_pinned = persistence::is_window_pinned(app);
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

/// The tray icon: left click opens the dashboard; the menu has Quit. The
/// snapshot emitter keeps its tooltip and icon live (`snapshot::tray`).
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, TRAY_MENU_QUIT, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;

    let mut tray = TrayIconBuilder::with_id(snapshot::tray::TRAY_ID)
        .tooltip("Neon Strider")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id.as_ref() == TRAY_MENU_QUIT {
                log::info!("quitting...");
                QUITTING.store(true, Ordering::SeqCst);
                app.exit(0);
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
    // tray size. Dimmed to start with: nothing is connected yet.
    let icon = snapshot::tray::icon(snapshot::tray::TrayState::Offline)
        .or_else(|| app.default_window_icon().cloned());
    if let Some(icon) = icon {
        tray = tray.icon(icon);
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

    // Once per database: earlier builds saved bad session averages.
    let (app_c, tilt, offset) = (app.clone(), config.tilt_multiplier, config.incline_offset);
    let _ = tauri::async_runtime::spawn_blocking(move || {
        persistence::backfill_session_averages(&app_c, tilt, offset);
    })
    .await;

    snapshot::init(&app);
    strava::init(&app);
    programs::init(&app).await;
    snapshot::spawn_emitter(app.clone());
    snapshot::desk::spawn(app.clone());
    strava::spawn_worker(app.clone());
    update::init(&app);
    update::spawn(app.clone());

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
            let config = persistence::load_config(app);
            link::set_saved_device(config.last_device_id.is_some());
            if let Some(id) = config.last_device_id {
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
            persistence::get_sessions,
            persistence::get_samples,
            persistence::delete_session,
            persistence::get_db_setting,
            persistence::set_db_setting,
            snapshot::get_snapshot,
            snapshot::dispatch,
            hud::set_hud_hit_region,
            logging::open_log_folder,
            export::export_sessions_csv
        ])
        .on_window_event(on_window_event)
        .build(tauri::generate_context!())
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
