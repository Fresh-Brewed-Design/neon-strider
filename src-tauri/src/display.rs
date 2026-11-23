//! The displays as Tauri reports them, turned into `placement`'s plain
//! geometry, and the conversions between Tauri's window coordinates and
//! desktop units.
//!
//! Desktop units are the one space `placement`, the HUD's hit rectangle and
//! `platform::cursor_position` share: physical px on Windows, points on macOS
//! (`platform::DESKTOP_IN_POINTS`). On macOS Tauri's "physical" px are points
//! times a scale that differs per window and per display, so they don't line
//! up across displays with different scales; points do.

use crate::placement::{MonitorGeom, Rect};
use crate::platform::DESKTOP_IN_POINTS;

/// Desktop units in one CSS px of this window: its scale on Windows, 1 on
/// macOS (a CSS px is a point).
pub(crate) fn desktop_scale(w: &tauri::WebviewWindow) -> f64 {
    if DESKTOP_IN_POINTS {
        1.0
    } else {
        w.scale_factor().unwrap_or(1.0)
    }
}

/// A position in desktop units, for `set_position`.
pub(crate) fn position(x: i32, y: i32) -> tauri::Position {
    if DESKTOP_IN_POINTS {
        tauri::LogicalPosition::new(f64::from(x), f64::from(y)).into()
    } else {
        tauri::PhysicalPosition::new(x, y).into()
    }
}

/// A size in desktop units, for `set_size`.
pub(crate) fn size(w: i32, h: i32) -> tauri::Size {
    let (w, h) = (w.max(1) as u32, h.max(1) as u32);
    if DESKTOP_IN_POINTS {
        tauri::LogicalSize::new(f64::from(w), f64::from(h)).into()
    } else {
        tauri::PhysicalSize::new(w, h).into()
    }
}

/// A window's physical px (as Tauri reports positions, and as the HUD page
/// measures itself: CSS px times `devicePixelRatio`) in desktop units.
pub(crate) fn from_window_px(scale: f64, v: i32) -> i32 {
    if DESKTOP_IN_POINTS {
        (f64::from(v) / scale.max(0.1)).round() as i32
    } else {
        v
    }
}

/// The top-left of a window's client area, desktop units.
pub(crate) fn window_origin(w: &tauri::WebviewWindow) -> Option<(i32, i32)> {
    let origin = w.inner_position().ok()?;
    let scale = w.scale_factor().unwrap_or(1.0);
    Some((
        from_window_px(scale, origin.x),
        from_window_px(scale, origin.y),
    ))
}

/// The displays in desktop units. `is_primary` is matched by position and
/// size, since Tauri's `Monitor` has no flag for it.
pub(crate) fn monitor_geoms(
    all: Vec<tauri::window::Monitor>,
    primary: Option<tauri::window::Monitor>,
) -> Vec<MonitorGeom> {
    all.iter()
        .map(|m| {
            let scale = m.scale_factor();
            let rect = |p: &tauri::PhysicalPosition<i32>, s: &tauri::PhysicalSize<u32>| {
                let d = |v: i32| from_window_px(scale, v);
                Rect::new(d(p.x), d(p.y), d(s.width as i32), d(s.height as i32))
            };
            let work = m.work_area();
            MonitorGeom {
                name: m.name().cloned(),
                is_primary: primary
                    .as_ref()
                    .is_some_and(|p| p.position() == m.position() && p.size() == m.size()),
                bounds: rect(m.position(), m.size()),
                work: rect(&work.position, &work.size),
                scale: if DESKTOP_IN_POINTS { 1.0 } else { scale },
            }
        })
        .collect()
}

pub(crate) fn current_monitors(app: &tauri::AppHandle) -> Vec<MonitorGeom> {
    monitor_geoms(
        app.available_monitors().unwrap_or_default(),
        app.primary_monitor().ok().flatten(),
    )
}

/// The menu bar icon's horizontal centre, desktop units. Tauri reports the
/// icon in its display's own physical px, and the display isn't named, so the
/// scale is the one that puts the icon under `cursor_x` (the click that
/// opened the dashboard), or failing that the first display it falls on.
pub(crate) fn tray_icon_center_x(app: &tauri::AppHandle, cursor_x: Option<i32>) -> Option<i32> {
    let tray = app.tray_by_id(crate::snapshot::tray::TRAY_ID)?;
    let rect = tray.rect().ok().flatten()?;
    let (x, w) = match (rect.position, rect.size) {
        (tauri::Position::Physical(p), tauri::Size::Physical(s)) => {
            (f64::from(p.x), f64::from(s.width))
        }
        (tauri::Position::Logical(p), tauri::Size::Logical(s)) if DESKTOP_IN_POINTS => {
            return Some((p.x + s.width / 2.0).round() as i32)
        }
        _ => return None,
    };
    let monitors = app.available_monitors().unwrap_or_default();
    let scales = monitors.iter().map(|m| m.scale_factor());
    let centre = |s: f64| (x + w / 2.0) / s.max(0.1);
    let under_cursor = |s: &f64| {
        cursor_x.is_some_and(|c| {
            let c = f64::from(c);
            x / s.max(0.1) - 2.0 <= c && c <= (x + w) / s.max(0.1) + 2.0
        })
    };
    let on_a_display = |s: &f64| {
        monitors.iter().any(|m| {
            let left = f64::from(m.position().x) / m.scale_factor().max(0.1);
            let right = left + f64::from(m.size().width) / m.scale_factor().max(0.1);
            (left..right).contains(&centre(*s))
        })
    };
    let scale = scales
        .clone()
        .find(under_cursor)
        .or_else(|| scales.clone().find(on_a_display))?;
    Some(centre(scale).round() as i32)
}
