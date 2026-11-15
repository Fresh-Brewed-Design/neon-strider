//! Dragging the HUD by its NEON STRIDER logo.
//!
//! The HUD window is a full-height, click-through strip with the content inside
//! it, so the window itself never moves vertically. A drag moves the strip
//! sideways and the content up and down inside it (`placement::drag_frame`),
//! clamped so the content stays fully on the monitor under the cursor, and
//! snapped to edges within 24 px. Dropped in a corner, it becomes that corner's
//! anchor; anywhere else it is saved as a custom position (`HudConfig::custom`),
//! with the monitor saved by name, position and size.
//!
//! The page only says when a drag starts (after the pointer moved about 4 px,
//! so a click stays a click). From then on Rust follows the real cursor
//! (physical px, right across monitors with different scales) every frame until
//! the button is released, sets the click-through hit rect itself each frame,
//! and tells the page the content's new top (`hud-drag`). `hud::DRAGGING` keeps
//! the window interactive meanwhile; it is cleared within a frame of the
//! button's release (or after a minute), so the HUD can never stay stuck
//! blocking clicks.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use super::{mutate_hud, HudAnchor};
use crate::placement::{self, Corner, DragFrame, HudCustom};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Event to the HUD page while dragging and once at the end.
const EVENT_HUD_DRAG: &str = "hud-drag";

const FRAME: Duration = Duration::from_millis(16);
/// A drag held longer than this is given up (the button state got lost).
const MAX_DRAG: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
struct DragEvent {
    /// The placement to show: a custom position, or `None` for `anchor`.
    custom: Option<HudCustom>,
    anchor: HudAnchor,
    /// The drag is over (the placement is what was saved).
    done: bool,
}

fn corner_anchor(c: Corner) -> HudAnchor {
    match c {
        Corner::TopLeft => HudAnchor::TopLeft,
        Corner::TopRight => HudAnchor::TopRight,
        Corner::BottomLeft => HudAnchor::BottomLeft,
        Corner::BottomRight => HudAnchor::BottomRight,
    }
}

/// Start following the cursor. `grab_*`: where the logo was grabbed, relative
/// to the content's top-left, CSS (logical) px.
pub fn start(app: &AppHandle, grab_x: f64, grab_y: f64) -> Result<(), String> {
    if !(grab_x.is_finite() && grab_y.is_finite()) {
        return Err("Bad grab point".to_string());
    }
    let Some(w) = app.get_webview_window(crate::hud::HUD_LABEL) else {
        return Err("The HUD is not open".to_string());
    };
    let Some((cw, ch)) = crate::hud::hit_size() else {
        return Err("The HUD has not measured itself yet".to_string());
    };
    if crate::hud::DRAGGING.swap(true, Ordering::SeqCst) {
        // Already dragging.
        return Ok(());
    }
    let scale = w.scale_factor().unwrap_or(1.0).max(0.1);
    let content = (cw as f64 / scale, ch as f64 / scale);
    let grab = (grab_x.clamp(0.0, content.0), grab_y.clamp(0.0, content.1));
    let app = app.clone();
    std::thread::spawn(move || {
        let last = follow(&app, &w, grab, content);
        crate::hud::DRAGGING.store(false, Ordering::SeqCst);
        finish(&app, last);
    });
    Ok(())
}

/// Follow the cursor until the button goes up. Returns the last frame.
fn follow(
    app: &AppHandle,
    w: &tauri::WebviewWindow,
    grab: (f64, f64),
    content: (f64, f64),
) -> Option<DragFrame> {
    let monitors = crate::display::current_monitors(app);
    let anchor = super::hud_config().anchor;
    let began = Instant::now();
    let mut last: Option<DragFrame> = None;
    while crate::platform::primary_button_down() && began.elapsed() < MAX_DRAG {
        let frame = crate::platform::cursor_position().and_then(|c| {
            placement::drag_frame(c, grab, content, &monitors, crate::hud::HUD_STRIP_WIDTH)
        });
        if let Some(f) = frame {
            if last.as_ref() != Some(&f) {
                let strip_moved = last.as_ref().is_none_or(|l| l.strip != f.strip);
                let resize = last
                    .as_ref()
                    .is_none_or(|l| l.monitor != f.monitor || l.strip.w != f.strip.w);
                if strip_moved {
                    crate::hud::place_hud_strip(w, f.strip, resize);
                }
                let c = f.content;
                crate::hud::set_hit_live(
                    (c.x - f.strip.x, c.y - f.strip.y, c.w, c.h),
                    (c.x, c.y, c.x + c.w, c.y + c.h),
                );
                let _ = app.emit_to(
                    crate::hud::HUD_LABEL,
                    EVENT_HUD_DRAG,
                    DragEvent {
                        custom: Some(f.custom),
                        anchor,
                        done: false,
                    },
                );
                last = Some(f);
            }
        }
        std::thread::sleep(FRAME);
    }
    last
}

/// Save where the drag ended: a corner anchor, or a custom position, on the
/// monitor it ended on.
fn finish(app: &AppHandle, last: Option<DragFrame>) {
    let Some(f) = last else {
        return;
    };
    let monitors = crate::display::current_monitors(app);
    let target = monitors.get(f.monitor).cloned();
    mutate_hud(app, |hud| {
        match &target {
            // The primary is stored as "no monitor", like the HUD page's picker.
            Some(m) if !m.is_primary && m.name.is_some() => {
                hud.monitor = m.name.clone();
                hud.monitor_rect = Some(m.bounds);
            }
            _ => {
                hud.monitor = None;
                hud.monitor_rect = None;
            }
        }
        match f.corner {
            Some(c) => {
                hud.anchor = corner_anchor(c);
                hud.custom = None;
            }
            None => hud.custom = Some(f.custom),
        }
    });
    let cfg = super::hud_config();
    match cfg.custom {
        Some(c) => log::info!(
            "HUD dragged to a custom position ({:.0}, {:.0})",
            c.dx,
            c.top
        ),
        None => log::info!("HUD dragged to the {:?} corner", cfg.anchor),
    }
    let _ = app.emit_to(
        crate::hud::HUD_LABEL,
        EVENT_HUD_DRAG,
        DragEvent {
            custom: cfg.custom,
            anchor: cfg.anchor,
            done: true,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_map_to_anchors() {
        assert_eq!(corner_anchor(Corner::TopLeft), HudAnchor::TopLeft);
        assert_eq!(corner_anchor(Corner::TopRight), HudAnchor::TopRight);
        assert_eq!(corner_anchor(Corner::BottomLeft), HudAnchor::BottomLeft);
        assert_eq!(corner_anchor(Corner::BottomRight), HudAnchor::BottomRight);
    }
}
