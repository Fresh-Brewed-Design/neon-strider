//! The dashboard window: a tray popup that opens where the user last left it
//! (or bottom-right of the primary display) and closes when it loses focus,
//! unless pinned: by the user (`Pin open`) or by a guided task
//! (`guided_task.rs`), which also keeps it on top.

use crate::{persistence, placement};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tauri::Manager;

pub(crate) const DASHBOARD_LABEL: &str = "main";
/// Settings key (JSON, see `placement::SavedPosition`) for where the dashboard was.
const KEY_DASHBOARD_POSITION: &str = "dashboard_position";
/// The size the dashboard's layout is designed at (`/dashboard-preview.html`).
const DASHBOARD_SIZE: (f64, f64) = (760.0, 600.0);
/// Gap from the primary display's right and bottom edges when there is no
/// saved spot, logical px (the bottom one clears the taskbar).
const DEFAULT_MARGIN: (f64, f64) = (20.0, 60.0);
/// Quiet time after the last move event before the position is saved.
/// Windows sends a move event per frame while dragging; half a second after the
/// last one the drag is over, so a drag is one write.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
/// Move events for this long after we placed the window ourselves are ours, not
/// the user's drag, and are not saved.
/// The moves Windows reports after a `set_position` arrive within a few frames;
/// 800 ms covers a slow repaint and is shorter than a deliberate drag.
const PLACE_GRACE: Duration = Duration::from_millis(800);
/// A focus loss this soon after opening is part of opening (the tray click
/// that opened it), not the user leaving.
/// The tray click's focus change lands within a few hundred ms; a second covers
/// a busy PC and is shorter than any real look away.
const OPEN_GRACE: Duration = Duration::from_millis(1000);

static PLACED_AT: Mutex<Option<Instant>> = Mutex::new(None);
static OPENED_AT: Mutex<Option<Instant>> = Mutex::new(None);
/// Latest move not yet saved: (x, y, when). Taken together with the scheduled
/// flag's clearing, so a move arriving mid-save starts a new save.
static PENDING: Mutex<Option<(i32, i32, Instant)>> = Mutex::new(None);
static SAVE_SCHEDULED: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Open (or focus) the dashboard window.
pub(crate) fn show_dashboard(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(DASHBOARD_LABEL) {
        // Reopening a hidden window is "opening" too: the click that asked for it
        // (a HUD button, the tray) can still steal focus back, and that blur must
        // not hide the window again before it was seen.
        *lock(&OPENED_AT) = Some(Instant::now());
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    log::info!("Spawning UI Flow...");
    let (width, height) = DASHBOARD_SIZE;
    let built = tauri::WebviewWindowBuilder::new(
        app,
        DASHBOARD_LABEL,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("NEON STRIDER")
    .inner_size(width, height)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build();
    let w = match built {
        Ok(w) => w,
        Err(e) => {
            log::warn!("Dashboard window not created: {}", e);
            return;
        }
    };

    crate::snapshot::refresh_monitors(app);
    *lock(&PLACED_AT) = Some(Instant::now());
    place(app, &w);
    *lock(&OPENED_AT) = Some(Instant::now());
    let _ = w.show();
    let _ = w.set_focus();
}

/// Where the user last left it, if that spot is still on a screen; otherwise
/// bottom-right of the primary display, like a tray popup.
fn place(app: &tauri::AppHandle, w: &tauri::WebviewWindow) {
    let monitors = crate::display::current_monitors(app);
    let restored = persistence::read_setting(app, KEY_DASHBOARD_POSITION)
        .and_then(|raw| placement::parse_saved_position(&raw))
        .and_then(|saved| placement::resolve_dashboard_position(&saved, DASHBOARD_SIZE, &monitors));

    if let Some((x, y)) = restored {
        log::debug!("Restoring dashboard position ({}, {})", x, y);
        let pos = tauri::PhysicalPosition::new(x, y);
        let _ = w.set_position(pos);
        // Crossing to a monitor with another scale makes Windows resize and
        // nudge the window; the second call lands it exactly.
        let _ = w.set_position(pos);
        return;
    }
    // The primary first: the current monitor can be unreliable this early.
    let monitor = w
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| w.current_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let size = monitor.size().to_logical::<f64>(scale);
        let pos = monitor.position().to_logical::<f64>(scale);
        let x = pos.x + size.width - DASHBOARD_SIZE.0 - DEFAULT_MARGIN.0;
        let y = pos.y + size.height - DASHBOARD_SIZE.1 - DEFAULT_MARGIN.1;
        let _ = w.set_position(tauri::LogicalPosition::new(x, y));
    }
}

/// The dashboard lost focus. Returns whether it should close now: not while
/// pinned, and not within `OPEN_GRACE` of opening.
pub(crate) fn should_close_on_blur(app: &tauri::AppHandle) -> bool {
    let fresh = lock(&OPENED_AT).is_some_and(|t| t.elapsed() < OPEN_GRACE);
    if persistence::is_window_pinned(app) {
        log::debug!("Main window blurred but pinned. Keeping.");
        false
    } else if crate::guided_task::pinned() {
        log::debug!("Main window blurred during a guided task. Keeping.");
        false
    } else if fresh {
        log::debug!("Main window blurred while opening. Keeping.");
        false
    } else {
        log::debug!("Main window blurred. Hiding tray menu.");
        true
    }
}

/// A guided task pinned or unpinned the dashboard. Pinned: it shows and stays
/// on top. Unpinned: a dashboard the user has left (no focus, no `Pin open`)
/// closes the way a blur would have closed it, since no blur is coming to do
/// it; one still in use stays until its next blur.
pub(crate) fn on_guided_pin_changed(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window(DASHBOARD_LABEL) else {
        return;
    };
    if crate::guided_task::pinned() {
        let _ = window.set_always_on_top(true);
        let _ = window.show();
        return;
    }
    let focused = window.is_focused().unwrap_or(true);
    if !focused && !persistence::is_window_pinned(app) {
        log::debug!("Guided task over and the dashboard has no focus. Hiding.");
        let _ = window.destroy();
    }
}

/// Called for every move event of the dashboard. A drag fires them continuously,
/// so only the latest position is kept and written once it has been still for
/// `SAVE_DEBOUNCE`. The write happens even if the window is destroyed
/// meanwhile (it closes on blur), because the position comes from the event.
pub(crate) fn dashboard_moved(app: &tauri::AppHandle, x: i32, y: i32) {
    let ours = lock(&PLACED_AT).is_some_and(|t| t.elapsed() < PLACE_GRACE);
    if ours {
        return;
    }
    *lock(&PENDING) = Some((x, y, Instant::now()));
    if SAVE_SCHEDULED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(SAVE_DEBOUNCE);
        let due = {
            let mut pending = lock(&PENDING);
            match *pending {
                Some((x, y, at)) if at.elapsed() >= SAVE_DEBOUNCE => {
                    *pending = None;
                    SAVE_SCHEDULED.store(false, Ordering::SeqCst);
                    Some((x, y))
                }
                // Still moving: wait another round.
                Some(_) => None,
                None => {
                    SAVE_SCHEDULED.store(false, Ordering::SeqCst);
                    return;
                }
            }
        };
        if let Some((x, y)) = due {
            save_position(&app, x, y);
            return;
        }
    });
}

fn save_position(app: &tauri::AppHandle, x: i32, y: i32) {
    let monitors = crate::display::current_monitors(app);
    let Some(saved) = placement::save_position(x, y, &monitors) else {
        // Minimised windows report (-32000, -32000); nothing worth remembering.
        log::debug!(
            "Dashboard at ({}, {}) is on no monitor; position not saved",
            x,
            y
        );
        return;
    };
    let Ok(json) = serde_json::to_string(&saved) else {
        return;
    };
    match persistence::write_setting(app, KEY_DASHBOARD_POSITION, &json) {
        Ok(()) => log::debug!("Saved dashboard position {}", json),
        Err(e) => log::warn!("Failed to save dashboard position: {}", e),
    }
}
