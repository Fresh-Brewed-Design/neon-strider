//! The HUD overlay window: creating it, placing it, its click-through hit
//! region, and toasts.
//!
//! The window is a full-height strip at a monitor edge whose geometry only
//! changes when it is placed (`position_hud`) or dragged (`snapshot::hud_drag`),
//! never because of what it shows: the page anchors its content inside the
//! strip in CSS. So content measurement can never clip or misplace it.
//!
//! The strip is click-through except over its visible content. The page
//! reports the content's rectangle (`set_hud_hit_region`), and the mouse hook
//! (`platform`) asks [`click_through_at`] on every move. A stale rectangle can
//! at worst put the clickable zone a few pixels off for a few milliseconds; it
//! never moves or resizes the window.

use crate::placement;
use crate::snapshot::{self, HudAnchor, HudConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use tauri::{Emitter, Manager};

pub(crate) const HUD_LABEL: &str = "hud";
/// The event that carries toasts to the HUD page.
const EVENT_HUD_UPDATE: &str = "hud-update";
/// Logical width of the HUD strip.
pub(crate) const HUD_STRIP_WIDTH: f64 = 340.0;
/// Logical height before the first placement.
const PROVISIONAL_HEIGHT: f64 = 800.0;
/// Slack (physical px) around the content's rectangle. Small, so the strip
/// doesn't block clicks on whatever sits just beside the HUD (a window's close
/// button, say).
const HIT_MARGIN: i32 = 6;

/// A rectangle as (left, top, right, bottom), physical desktop px.
type Edges = (i32, i32, i32, i32);
/// A rectangle as (left, top, width, height), physical px.
type Area = (i32, i32, i32, i32);

/// The desktop rectangle covered by visible HUD content, in the mouse hook's
/// coordinate space (physical px on the virtual desktop). The hook only ever
/// `try_lock`s it.
static HIT_RECT: Mutex<Option<Edges>> = Mutex::new(None);
/// The same rectangle as the page measured it, relative to the HUD's client
/// area. `HIT_RECT` is this plus the window's position, re-derived whenever
/// either changes.
static HIT_REL: Mutex<Option<Area>> = Mutex::new(None);

/// A drag of the HUD by its logo is in progress (`snapshot::hud_drag`). The
/// window stays interactive meanwhile; the drag clears this within a frame of
/// the button's release, so a lost drag can't leave it blocking clicks.
pub(crate) static DRAGGING: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Send a toast to the HUD page (and any other window listening).
pub(crate) fn toast(app: &tauri::AppHandle, payload: serde_json::Value) {
    let _ = app.emit(EVENT_HUD_UPDATE, payload);
}

/// (x, y) falls inside `rect`, widened by `HIT_MARGIN` on every side.
fn hit(rect: Edges, x: i32, y: i32) -> bool {
    let (left, top, right, bottom) = rect;
    x >= left - HIT_MARGIN
        && x <= right + HIT_MARGIN
        && y >= top - HIT_MARGIN
        && y <= bottom + HIT_MARGIN
}

/// Whether the HUD should be click-through for a pointer at (x, y): yes,
/// unless the pointer is over its content or a drag is on. `None` when the
/// rectangle is being updated right now; the next mouse move asks again.
pub(crate) fn click_through_at(x: i32, y: i32) -> Option<bool> {
    let rect = *HIT_RECT.try_lock().ok()?;
    let over_content = rect.is_some_and(|r| hit(r, x, y));
    Some(!over_content && !DRAGGING.load(Ordering::Relaxed))
}

/// Make the HUD window click-through or interactive. Never blocks.
pub(crate) fn set_click_through(click_through: bool) {
    let Some(app) = crate::app_handle() else {
        return;
    };
    if let Some(hud) = app.get_webview_window(HUD_LABEL) {
        let _ = hud.set_ignore_cursor_events(click_through);
    }
}

/// Report the rectangle covered by visible HUD content. The page sends it in
/// physical px relative to its own window (`getBoundingClientRect` times
/// `devicePixelRatio`); adding the window's client-area position gives the
/// mouse hook's coordinate space. (`window.screenX/Y` would not do: its units
/// differ per monitor when monitors have different scales.) An empty rect
/// (`width` or `height` <= 0) clears it.
#[tauri::command]
pub fn set_hud_hit_region(
    window: tauri::WebviewWindow,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
) {
    if width <= 0 || height <= 0 {
        *lock(&HIT_REL) = None;
        *lock(&HIT_RECT) = None;
        return;
    }
    *lock(&HIT_REL) = Some((left, top, width, height));
    refresh_hit_rect(&window);
}

/// Recompute the desktop hit rectangle from the page's window-relative one and
/// the window's current position. Call after the window moves.
fn refresh_hit_rect(w: &tauri::WebviewWindow) {
    let rel = *lock(&HIT_REL);
    let abs = match (rel, w.inner_position()) {
        (Some(rel), Ok(origin)) => {
            let rect = placement::hit_rect_abs((origin.x, origin.y), rel);
            log::debug!("hud hit rect={:?}", rect);
            Some(rect)
        }
        _ => None,
    };
    *lock(&HIT_RECT) = abs;
}

/// While dragging, the drag sets the hit rectangle itself each frame (the
/// content moves before the page re-measures). `rel` is relative to the
/// strip's client area, `abs` on the desktop.
pub(crate) fn set_hit_live(rel: Area, abs: Edges) {
    *lock(&HIT_REL) = Some(rel);
    *lock(&HIT_RECT) = Some(abs);
}

/// The HUD content's last measured size, physical px (`None` before the page
/// reported one).
pub(crate) fn hit_size() -> Option<(i32, i32)> {
    lock(&HIT_REL).map(|(_, _, w, h)| (w, h))
}

/// Place the HUD strip, full height, on the chosen monitor's work area: at its
/// left or right edge for a corner anchor, or at the dragged spot for a custom
/// position. The monitor is the saved one (see `placement::choose_monitor`);
/// none, or one no longer connected, means the primary.
///
/// Physical coordinates throughout: a logical position would be converted with
/// the window's current scale, which is wrong when the target monitor has
/// another one.
pub(crate) fn position_hud(w: &tauri::WebviewWindow, cfg: &HudConfig) {
    let saved = cfg.saved_monitor();
    let monitor = saved.as_ref();
    let monitors = crate::display::monitor_geoms(
        w.available_monitors().unwrap_or_default(),
        w.primary_monitor().ok().flatten(),
    );
    let Some(choice) = placement::choose_monitor(monitor, &monitors) else {
        log::warn!("No monitors reported; HUD not positioned");
        return;
    };
    let saved_name = || monitor.and_then(|m| m.name.as_deref()).unwrap_or_default();
    match choice.how {
        placement::MonitorMatch::FellBack => {
            log::warn!(
                "HUD monitor {:?} not found; using the primary display",
                saved_name()
            )
        }
        placement::MonitorMatch::PositionAndSize => log::info!(
            "HUD monitor {:?} found by position as {:?}",
            saved_name(),
            choice.monitor.name.as_deref().unwrap_or_default()
        ),
        _ => {}
    }
    let m = choice.monitor;

    let strip = match &cfg.custom {
        Some(c) => placement::custom_strip(&m.work, m.scale, HUD_STRIP_WIDTH, c).0,
        None => {
            let on_left = matches!(cfg.anchor, HudAnchor::TopLeft | HudAnchor::BottomLeft);
            placement::hud_strip(&m.work, m.scale, HUD_STRIP_WIDTH, on_left)
        }
    };
    place_hud_strip(w, strip, true);
    refresh_hit_rect(w);
}

/// Move (and with `resize`, size) the HUD strip window, physical px.
pub(crate) fn place_hud_strip(w: &tauri::WebviewWindow, strip: placement::Rect, resize: bool) {
    let pos = tauri::PhysicalPosition::new(strip.x, strip.y);
    let _ = w.set_position(pos);
    if resize {
        // Move, size, move: a move onto a monitor with another scale makes
        // Windows rescale and nudge the window, so it is placed again after.
        let size = tauri::PhysicalSize::new(strip.w.max(1) as u32, strip.h.max(1) as u32);
        let _ = w.set_size(size);
        let _ = w.set_position(pos);
    }
}

/// Create the HUD window if it isn't open: hidden, click-through, then placed
/// and shown.
pub(crate) fn ensure_hud_native(app: &tauri::AppHandle) {
    if app.get_webview_window(HUD_LABEL).is_some() {
        return;
    }
    log::info!("Spawning HUD (Fixed Panel)");

    // Its own page: the overlay loads only the HUD bundle.
    let built =
        tauri::WebviewWindowBuilder::new(app, HUD_LABEL, tauri::WebviewUrl::App("hud.html".into()))
            .title("HUD")
            .inner_size(HUD_STRIP_WIDTH, PROVISIONAL_HEIGHT)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            // Hidden until placed, so it never flashes at the provisional spot.
            .visible(false)
            .build();

    match built {
        Ok(w) => {
            // The mouse hook makes it interactive over its content.
            let _ = w.set_ignore_cursor_events(true);
            crate::platform::remember_hud_window(&w);
            position_hud(&w, &snapshot::hud_config());
            let _ = w.show();
        }
        Err(e) => log::warn!("HUD window not created: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hit_area_is_the_rect_plus_a_small_margin() {
        let r = (100, 200, 440, 600);
        assert!(hit(r, 100, 200));
        assert!(hit(r, 440, 600));
        assert!(hit(r, 300, 400));
        // The margin is inclusive on every side.
        assert!(hit(r, 94, 194));
        assert!(hit(r, 446, 606));
        assert!(!hit(r, 93, 400));
        assert!(!hit(r, 447, 400));
        assert!(!hit(r, 300, 193));
        assert!(!hit(r, 300, 607));
    }
}
