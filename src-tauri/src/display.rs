//! The displays as Tauri reports them, turned into `placement`'s plain geometry.

use crate::placement::{MonitorGeom, Rect};

/// The displays in physical px. `is_primary` is matched by position and size,
/// since Tauri's `Monitor` has no flag for it.
pub(crate) fn monitor_geoms(
    all: Vec<tauri::window::Monitor>,
    primary: Option<tauri::window::Monitor>,
) -> Vec<MonitorGeom> {
    let rect = |p: &tauri::PhysicalPosition<i32>, s: &tauri::PhysicalSize<u32>| {
        Rect::new(p.x, p.y, s.width as i32, s.height as i32)
    };
    all.iter()
        .map(|m| {
            let work = m.work_area();
            MonitorGeom {
                name: m.name().cloned(),
                is_primary: primary
                    .as_ref()
                    .is_some_and(|p| p.position() == m.position() && p.size() == m.size()),
                bounds: rect(m.position(), m.size()),
                work: rect(&work.position, &work.size),
                scale: m.scale_factor(),
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
