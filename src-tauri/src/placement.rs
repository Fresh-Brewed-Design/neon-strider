//! Pure window-placement maths: where the dashboard was last, whether that spot
//! is still on a screen, which monitor the HUD should use, and the HUD strip's
//! geometry. No Tauri types, so every rule here is unit-tested.
//!
//! All coordinates are PHYSICAL pixels in the Windows virtual desktop, where the
//! primary monitor's top-left is (0, 0) and monitors left of or above it have
//! negative coordinates.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    fn right(&self) -> i32 {
        self.x + self.w
    }

    fn bottom(&self) -> i32 {
        self.y + self.h
    }

    fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// `self` lies entirely within `outer`.
    fn is_inside(&self, outer: &Rect) -> bool {
        self.x >= outer.x
            && self.y >= outer.y
            && self.right() <= outer.right()
            && self.bottom() <= outer.bottom()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorGeom {
    /// As the OS reports it (Windows: `\\.\DISPLAY2`). Can be missing.
    pub name: Option<String>,
    pub is_primary: bool,
    pub bounds: Rect,
    /// Bounds minus the taskbar and other docked bars.
    pub work: Rect,
    pub scale: f64,
}

/// What is stored under the `dashboard_position` setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedPosition {
    /// Outer top-left of the window, physical px.
    pub x: i32,
    pub y: i32,
    /// The monitor the window was on, so the spot can follow that monitor if the
    /// arrangement moves it, and be dropped if the monitor is gone.
    pub monitor_name: Option<String>,
    pub monitor_x: i32,
    pub monitor_y: i32,
    /// That monitor's scale factor when saved.
    pub scale: f64,
}

pub fn parse_saved_position(raw: &str) -> Option<SavedPosition> {
    let p: SavedPosition = serde_json::from_str(raw).ok()?;
    (p.scale.is_finite() && p.scale > 0.0).then_some(p)
}

/// Physical size of a window of `logical` size on a monitor with `scale`.
fn physical_size(logical: (f64, f64), scale: f64) -> (i32, i32) {
    (
        (logical.0 * scale).round() as i32,
        (logical.1 * scale).round() as i32,
    )
}

/// Move `rect` the least distance that puts it inside `area`. `None` when it is
/// bigger than the area (it can never fit).
fn clamp_into(rect: Rect, area: &Rect) -> Option<Rect> {
    if rect.w > area.w || rect.h > area.h {
        return None;
    }
    Some(Rect {
        x: rect.x.clamp(area.x, area.right() - rect.w),
        y: rect.y.clamp(area.y, area.bottom() - rect.h),
        ..rect
    })
}

/// The monitor a window with this top-left corner is on: the one whose bounds
/// hold the corner. `None` when it is on no screen (minimised windows report
/// (-32000, -32000)).
fn monitor_at(x: i32, y: i32, monitors: &[MonitorGeom]) -> Option<&MonitorGeom> {
    monitors.iter().find(|m| m.bounds.contains_point(x, y))
}

/// Snapshot of where the dashboard is, ready to store. `None` if it is on no
/// monitor.
pub fn save_position(x: i32, y: i32, monitors: &[MonitorGeom]) -> Option<SavedPosition> {
    let m = monitor_at(x, y, monitors)?;
    Some(SavedPosition {
        x,
        y,
        monitor_name: m.name.clone(),
        monitor_x: m.bounds.x,
        monitor_y: m.bounds.y,
        scale: m.scale,
    })
}

/// Where to open the dashboard (outer top-left, physical px), or `None` to use
/// the default spot.
///
/// * The saved monitor still exists (matched by name): keep the window at the same
///   offset from that monitor's corner, so a rearranged desktop or a changed
///   resolution does not lose it, then clamp it into that monitor's work area.
///   The offset is scaled by the scale-factor change.
/// * No name match (name missing, or the monitor is gone): the saved coordinates
///   are used as they are, and only if the whole window then lies inside some
///   monitor's work area. Anything else (an unplugged screen) returns `None`.
///
/// The window size is `logical_size` at the target monitor's scale, because the
/// window is created at the primary scale and resized when it crosses to a monitor
/// with another one.
pub fn resolve_dashboard_position(
    saved: &SavedPosition,
    logical_size: (f64, f64),
    monitors: &[MonitorGeom],
) -> Option<(i32, i32)> {
    let named = saved
        .monitor_name
        .as_ref()
        .and_then(|n| monitors.iter().find(|m| m.name.as_ref() == Some(n)));
    if let Some(m) = named {
        let ratio = m.scale / saved.scale;
        let off_x = ((saved.x - saved.monitor_x) as f64 * ratio).round() as i32;
        let off_y = ((saved.y - saved.monitor_y) as f64 * ratio).round() as i32;
        let (w, h) = physical_size(logical_size, m.scale);
        let rect = Rect::new(m.bounds.x + off_x, m.bounds.y + off_y, w, h);
        return clamp_into(rect, &m.work).map(|r| (r.x, r.y));
    }

    let m = monitors
        .iter()
        .find(|m| m.work.contains_point(saved.x, saved.y))?;
    let (w, h) = physical_size(logical_size, m.scale);
    let rect = Rect::new(saved.x, saved.y, w, h);
    rect.is_inside(&m.work).then_some((rect.x, rect.y))
}

/// The monitor the HUD was put on: its name and, since names can change after a
/// driver update, its bounds when it was chosen. What the `hud_monitor` setting
/// stores (as JSON); older builds stored only the name, as a JSON string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedMonitor {
    pub name: Option<String>,
    /// Physical desktop bounds when chosen; `None` for a setting saved by an
    /// older build (filled in once that monitor is seen again).
    #[serde(default)]
    pub rect: Option<Rect>,
}

/// Parse the `hud_monitor` setting: the current object form, or an older
/// build's plain name (a JSON string). `null`, blanks and junk mean the primary.
pub fn parse_saved_monitor(raw: &str) -> Option<SavedMonitor> {
    if let Ok(saved) = serde_json::from_str::<SavedMonitor>(raw) {
        let name = saved.name.filter(|n| !n.is_empty());
        return (name.is_some() || saved.rect.is_some()).then_some(SavedMonitor {
            name,
            rect: saved.rect,
        });
    }
    serde_json::from_str::<Option<String>>(raw)
        .ok()
        .flatten()
        .filter(|n| !n.is_empty())
        .map(|name| SavedMonitor {
            name: Some(name),
            rect: None,
        })
}

/// How the saved monitor was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorMatch {
    /// Nothing saved: the primary, as asked.
    Primary,
    NameAndPosition,
    /// The name changed (a driver update), but a monitor sits at the same spot
    /// with the same size.
    PositionAndSize,
    /// The same name, moved or resized.
    Name,
    /// Nothing matched: the primary instead.
    FellBack,
}

/// Which monitor the HUD goes on.
#[derive(Debug, PartialEq)]
pub struct MonitorChoice<'a> {
    pub monitor: &'a MonitorGeom,
    pub how: MonitorMatch,
}

/// Find the saved monitor, in this order: name and position, position and
/// size, name, then the primary. `None` only when there are no monitors.
pub fn choose_monitor<'a>(
    wanted: Option<&SavedMonitor>,
    monitors: &'a [MonitorGeom],
) -> Option<MonitorChoice<'a>> {
    let primary = monitors
        .iter()
        .find(|m| m.is_primary)
        .or_else(|| monitors.first())?;
    let Some(saved) = wanted else {
        return Some(MonitorChoice {
            monitor: primary,
            how: MonitorMatch::Primary,
        });
    };
    let same_name =
        |m: &MonitorGeom| saved.name.is_some() && m.name.as_deref() == saved.name.as_deref();
    let same_pos = |m: &MonitorGeom| {
        saved
            .rect
            .is_some_and(|r| m.bounds.x == r.x && m.bounds.y == r.y)
    };
    let same_rect = |m: &MonitorGeom| saved.rect == Some(m.bounds);

    let found = monitors
        .iter()
        .find(|m| same_name(m) && same_pos(m))
        .map(|m| (m, MonitorMatch::NameAndPosition))
        .or_else(|| {
            monitors
                .iter()
                .find(|m| same_rect(m))
                .map(|m| (m, MonitorMatch::PositionAndSize))
        })
        .or_else(|| {
            monitors
                .iter()
                .find(|m| same_name(m))
                .map(|m| (m, MonitorMatch::Name))
        });
    Some(match found {
        Some((monitor, how)) => MonitorChoice { monitor, how },
        None => MonitorChoice {
            monitor: primary,
            how: MonitorMatch::FellBack,
        },
    })
}

/// What to store when the user picks the monitor called `name`: its name and
/// its current bounds (if it is connected).
pub fn saved_monitor_for(name: &str, monitors: &[MonitorGeom]) -> SavedMonitor {
    SavedMonitor {
        name: Some(name.to_string()),
        rect: monitors
            .iter()
            .find(|m| m.name.as_deref() == Some(name))
            .map(|m| m.bounds),
    }
}

/// An older build's setting (a name only) gains the bounds of the monitor
/// with that name, once it is connected. `None` when there is nothing to
/// migrate (already has bounds, or no such monitor now).
pub fn migrate_saved_monitor(
    saved: &SavedMonitor,
    monitors: &[MonitorGeom],
) -> Option<SavedMonitor> {
    if saved.rect.is_some() {
        return None;
    }
    let name = saved.name.as_deref()?;
    let m = monitors.iter().find(|m| m.name.as_deref() == Some(name))?;
    Some(SavedMonitor {
        name: saved.name.clone(),
        rect: Some(m.bounds),
    })
}

/// The HUD strip: full work-area height, `panel_logical_width` wide at the
/// monitor's scale, on the left or right edge. Physical px.
pub fn hud_strip(work: &Rect, scale: f64, panel_logical_width: f64, on_left: bool) -> Rect {
    let w = ((panel_logical_width * scale).round() as i32).min(work.w);
    let x = if on_left { work.x } else { work.x + work.w - w };
    Rect::new(x, work.y, w, work.h)
}

// ---------------------------------------------------------------------------
// Custom HUD position (dragged by its logo)
// ---------------------------------------------------------------------------
//
// The HUD window stays a full-height strip of the monitor's work area, so it can
// never end up half off-screen vertically. A drag moves the strip sideways (its
// x) and the content up and down inside it (its top offset). In custom mode the
// content sits `HUD_PAD_X` in from the strip's left edge, with the panel first.

/// Content inset from the strip's left edge in custom mode (logical px; the
/// CSS padding of `.hud-root.anchor-custom`).
const HUD_PAD_X: f64 = 16.0;
/// Space kept above and below the content (logical px).
const HUD_PAD_Y: f64 = 8.0;
/// Within this distance (logical px) of a work-area edge, a drag snaps to it.
const HUD_SNAP: f64 = 24.0;
/// A restored top offset leaves at least this much of the strip below it.
const HUD_MIN_VISIBLE: f64 = 48.0;

/// A dragged HUD position, relative to its monitor's work area, in logical px
/// so it survives a scale change. The monitor is `HudConfig`'s.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HudCustom {
    /// Strip left minus work-area left.
    pub dx: f64,
    /// Content top minus work-area top (= the strip's top).
    pub top: f64,
}

pub fn parse_hud_custom(raw: &str) -> Option<HudCustom> {
    let c: HudCustom = serde_json::from_str(raw).ok()?;
    (c.dx.is_finite() && c.top.is_finite()).then_some(c)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Strip width in physical px on a monitor (never wider than its work area).
fn strip_width(work: &Rect, scale: f64, panel_logical_width: f64) -> i32 {
    ((panel_logical_width * scale).round() as i32)
        .min(work.w)
        .max(1)
}

/// Where a saved custom position goes on `work` today: the strip (physical px)
/// and the content's top offset inside it (logical px). Both are clamped to the
/// nearest spot that is fully on this work area, so a position saved on a
/// larger or since-rearranged monitor still lands on-screen.
pub fn custom_strip(
    work: &Rect,
    scale: f64,
    panel_logical_width: f64,
    c: &HudCustom,
) -> (Rect, f64) {
    let w = strip_width(work, scale, panel_logical_width);
    let x = (work.x as f64 + c.dx * scale).round() as i32;
    let x = x.clamp(work.x, work.x + work.w - w);
    let work_h = work.h as f64 / scale;
    let max_top = (work_h - HUD_MIN_VISIBLE).max(HUD_PAD_Y);
    let top = c.top.clamp(HUD_PAD_Y.min(max_top), max_top);
    (Rect::new(x, work.y, w, work.h), top)
}

/// One step of a drag.
#[derive(Debug, Clone, PartialEq)]
pub struct DragFrame {
    /// Index into the monitors the frame was computed for.
    pub monitor: usize,
    /// The strip window, physical px.
    pub strip: Rect,
    /// The content, physical desktop px (for the click-through hit rect).
    pub content: Rect,
    /// What to save: offsets from the work area, logical px.
    pub custom: HudCustom,
    /// Snapped into a corner: becomes that corner's anchor on drop.
    pub corner: Option<Corner>,
}

/// The monitor under a point, else the one whose bounds are nearest to it.
fn monitor_near(x: i32, y: i32, monitors: &[MonitorGeom]) -> Option<usize> {
    if let Some(i) = monitors.iter().position(|m| m.bounds.contains_point(x, y)) {
        return Some(i);
    }
    let dist = |m: &MonitorGeom| {
        let b = &m.bounds;
        let dx = (b.x - x).max(0).max(x - b.right() + 1) as i64;
        let dy = (b.y - y).max(0).max(y - b.bottom() + 1) as i64;
        dx * dx + dy * dy
    };
    monitors
        .iter()
        .enumerate()
        .min_by_key(|(_, m)| dist(m))
        .map(|(i, _)| i)
}

/// Where the HUD goes while its logo is dragged with the cursor at `cursor`
/// (physical desktop px). `grab` is where the logo was grabbed, relative to the
/// content's top-left, and `content` the content's size, both logical px.
///
/// The monitor under the cursor takes the HUD. The content's top-left follows
/// the cursor minus the grab offset, then is clamped so the content stays fully
/// inside the work area, then snaps to any edge within `HUD_SNAP`. Snapping to
/// both a horizontal and a vertical edge is a corner.
pub fn drag_frame(
    cursor: (i32, i32),
    grab: (f64, f64),
    content: (f64, f64),
    monitors: &[MonitorGeom],
    panel_logical_width: f64,
) -> Option<DragFrame> {
    let index = monitor_near(cursor.0, cursor.1, monitors)?;
    let m = &monitors[index];
    let (work, s) = (m.work, m.scale);
    let strip_w = strip_width(&work, s, panel_logical_width);
    let (cw, ch) = (
        ((content.0 * s).round() as i32).max(1),
        ((content.1 * s).round() as i32).max(1),
    );
    let pad_x = (HUD_PAD_X * s).round() as i32;
    let pad_y = (HUD_PAD_Y * s).round() as i32;
    let snap = (HUD_SNAP * s).round() as i32;

    // Horizontal: the strip.
    let want_x = cursor.0 - (grab.0 * s).round() as i32 - pad_x;
    let (min_x, max_x) = (work.x, work.x + work.w - strip_w);
    let mut x = want_x.clamp(min_x, max_x.max(min_x));
    let mut h_edge = None;
    if x - min_x <= snap {
        x = min_x;
        h_edge = Some(false); // left
    } else if max_x - x <= snap {
        x = max_x;
        h_edge = Some(true); // right
    }

    // Vertical: the content inside the strip.
    let want_y = cursor.1 - (grab.1 * s).round() as i32;
    let (min_y, max_y) = (work.y + pad_y, work.y + work.h - pad_y - ch);
    let mut y = want_y.clamp(min_y, max_y.max(min_y));
    let mut v_edge = None;
    if y - min_y <= snap {
        y = min_y;
        v_edge = Some(false); // top
    } else if max_y - y <= snap {
        y = max_y.max(min_y);
        v_edge = Some(true); // bottom
    }

    let corner = match (h_edge, v_edge) {
        (Some(false), Some(false)) => Some(Corner::TopLeft),
        (Some(true), Some(false)) => Some(Corner::TopRight),
        (Some(false), Some(true)) => Some(Corner::BottomLeft),
        (Some(true), Some(true)) => Some(Corner::BottomRight),
        _ => None,
    };
    Some(DragFrame {
        monitor: index,
        strip: Rect::new(x, work.y, strip_w, work.h),
        content: Rect::new(x + pad_x, y, cw, ch),
        custom: HudCustom {
            dx: (x - work.x) as f64 / s,
            top: (y - work.y) as f64 / s,
        },
        corner,
    })
}

/// Human name for the selector: "Display 2 · 2560×1440". The number comes from
/// the Windows device name (`\\.\DISPLAY2`) when it has one, else the list order.
pub fn monitor_label(name: Option<&str>, index: usize, width: i32, height: i32) -> String {
    let number = name
        .and_then(|n| {
            n.to_ascii_uppercase()
                .rsplit("DISPLAY")
                .next()
                .map(str::to_owned)
        })
        .and_then(|tail| {
            let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<usize>().ok()
        })
        .unwrap_or(index + 1);
    format!("Display {number} \u{b7} {width}\u{d7}{height}")
}

/// The HUD hit rectangle in desktop coordinates `(left, top, right, bottom)`:
/// the content rect the page measured, in physical px relative to the window's
/// client area, shifted by the client area's desktop origin. Measuring relative
/// to the window avoids `window.screenX`, whose units differ per monitor when
/// monitors have different scales.
pub fn hit_rect_abs(client_origin: (i32, i32), rel: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let (l, t, w, h) = rel;
    let (ox, oy) = client_origin;
    (ox + l, oy + t, ox + l + w, oy + t + h)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DASH: (f64, f64) = (760.0, 600.0);

    fn mon(name: &str, primary: bool, b: Rect, taskbar: i32, scale: f64) -> MonitorGeom {
        MonitorGeom {
            name: Some(name.to_string()),
            is_primary: primary,
            bounds: b,
            work: Rect::new(b.x, b.y, b.w, b.h - taskbar),
            scale,
        }
    }

    fn primary() -> MonitorGeom {
        mon(
            "\\\\.\\DISPLAY1",
            true,
            Rect::new(0, 0, 1920, 1080),
            48,
            1.0,
        )
    }

    /// Second screen to the LEFT of the primary, higher DPI: negative x.
    fn left_hidpi() -> MonitorGeom {
        mon(
            "\\\\.\\DISPLAY2",
            false,
            Rect::new(-2560, -200, 2560, 1440),
            60,
            1.5,
        )
    }

    fn saved(x: i32, y: i32, m: &MonitorGeom) -> SavedPosition {
        save_position(x, y, std::slice::from_ref(m)).unwrap()
    }

    #[test]
    fn rect_inside_and_clamp() {
        let area = Rect::new(0, 0, 1000, 800);
        assert!(Rect::new(0, 0, 1000, 800).is_inside(&area));
        assert!(!Rect::new(1, 0, 1000, 800).is_inside(&area));
        assert_eq!(
            clamp_into(Rect::new(900, 700, 200, 200), &area),
            Some(Rect::new(800, 600, 200, 200))
        );
        assert_eq!(
            clamp_into(Rect::new(-50, -50, 200, 200), &area),
            Some(Rect::new(0, 0, 200, 200))
        );
        assert_eq!(clamp_into(Rect::new(0, 0, 1200, 100), &area), None);
        // Already inside: untouched.
        assert_eq!(
            clamp_into(Rect::new(10, 20, 30, 40), &area),
            Some(Rect::new(10, 20, 30, 40))
        );
    }

    #[test]
    fn restores_a_position_that_is_still_on_screen() {
        let ms = [primary()];
        let s = saved(1000, 300, &ms[0]);
        assert_eq!(resolve_dashboard_position(&s, DASH, &ms), Some((1000, 300)));
    }

    #[test]
    fn restores_on_a_monitor_with_negative_coordinates() {
        let ms = [primary(), left_hidpi()];
        // 760x600 logical at 1.5 = 1140x900 physical.
        let s = saved(-1500, -100, &ms[1]);
        assert_eq!(s.monitor_x, -2560);
        assert_eq!(
            resolve_dashboard_position(&s, DASH, &ms),
            Some((-1500, -100))
        );
    }

    #[test]
    fn monitor_removed_falls_back() {
        let both = [primary(), left_hidpi()];
        let s = saved(-1500, -100, &both[1]);
        // The second screen was unplugged: the saved spot is off every screen.
        assert_eq!(resolve_dashboard_position(&s, DASH, &[primary()]), None);
    }

    #[test]
    fn named_monitor_that_moved_keeps_its_offset() {
        let before = [primary(), left_hidpi()];
        let s = saved(-1500, -100, &before[1]); // offset (1060, 100) in the monitor
                                                // The screen is now to the right of the primary instead.
        let mut moved = left_hidpi();
        moved.bounds = Rect::new(1920, 0, 2560, 1440);
        moved.work = Rect::new(1920, 0, 2560, 1380);
        let after = [primary(), moved];
        assert_eq!(
            resolve_dashboard_position(&s, DASH, &after),
            Some((1920 + 1060, 100))
        );
    }

    #[test]
    fn named_monitor_with_a_smaller_resolution_is_clamped() {
        let ms = [primary()];
        // Saved near the bottom-right of a larger 1920x1080 primary, now 1366x768.
        let s = saved(1100, 450, &ms[0]);
        let mut small = primary();
        small.bounds = Rect::new(0, 0, 1366, 768);
        small.work = Rect::new(0, 0, 1366, 720);
        assert_eq!(
            resolve_dashboard_position(&s, DASH, &[small]),
            Some((1366 - 760, 720 - 600))
        );
    }

    #[test]
    fn scale_change_rescales_the_offset_and_size() {
        let ms = [primary()];
        let s = saved(100, 100, &ms[0]); // scale 1.0
        let mut hi = primary();
        hi.scale = 2.0;
        hi.bounds = Rect::new(0, 0, 3840, 2160);
        hi.work = Rect::new(0, 0, 3840, 2100);
        assert_eq!(
            resolve_dashboard_position(&s, DASH, &[hi]),
            Some((200, 200))
        );
    }

    #[test]
    fn unnamed_position_must_be_fully_inside_a_work_area() {
        let ms = [primary()];
        let mut s = saved(1000, 300, &ms[0]);
        s.monitor_name = None;
        assert_eq!(resolve_dashboard_position(&s, DASH, &ms), Some((1000, 300)));
        // Hangs 100 px off the right edge: partially off-screen is rejected.
        s.x = 1920 - 760 + 100;
        assert_eq!(resolve_dashboard_position(&s, DASH, &ms), None);
        // Overlaps the taskbar strip: outside the work area.
        s.x = 1000;
        s.y = 1080 - 48 - 600 + 10;
        assert_eq!(resolve_dashboard_position(&s, DASH, &ms), None);
        // Top-left off every screen.
        s.x = -5000;
        s.y = 0;
        assert_eq!(resolve_dashboard_position(&s, DASH, &ms), None);
    }

    #[test]
    fn minimised_window_position_is_not_saved() {
        let ms = [primary(), left_hidpi()];
        assert_eq!(save_position(-32000, -32000, &ms), None);
    }

    #[test]
    fn save_picks_the_monitor_by_corner() {
        let ms = [primary(), left_hidpi()];
        let s = save_position(-10, 50, &ms).unwrap();
        assert_eq!(s.monitor_name.as_deref(), Some("\\\\.\\DISPLAY2"));
        assert_eq!(s.scale, 1.5);
        let s = save_position(0, 50, &ms).unwrap();
        assert_eq!(s.monitor_name.as_deref(), Some("\\\\.\\DISPLAY1"));
    }

    #[test]
    fn saved_position_json_round_trips_and_rejects_junk() {
        let ms = [primary()];
        let s = saved(5, 6, &ms[0]);
        let raw = serde_json::to_string(&s).unwrap();
        assert_eq!(parse_saved_position(&raw), Some(s));
        assert_eq!(parse_saved_position("not json"), None);
        assert_eq!(parse_saved_position("{}"), None);
        let zero_scale =
            r#"{"x":1,"y":2,"monitor_name":null,"monitor_x":0,"monitor_y":0,"scale":0}"#;
        assert_eq!(parse_saved_position(zero_scale), None);
    }

    fn saved_mon(name: &str, rect: Option<Rect>) -> SavedMonitor {
        SavedMonitor {
            name: Some(name.to_string()),
            rect,
        }
    }

    /// A third screen right of the primary, same size as the left one.
    fn right_screen() -> MonitorGeom {
        mon(
            "\\\\.\\DISPLAY3",
            false,
            Rect::new(1920, 0, 2560, 1440),
            60,
            1.0,
        )
    }

    #[test]
    fn hud_monitor_nothing_saved_is_the_primary() {
        let ms = [left_hidpi(), primary()];
        // Primary wherever it sits in the list.
        let c = choose_monitor(None, &ms).unwrap();
        assert!(c.monitor.is_primary);
        assert_eq!(c.how, MonitorMatch::Primary);
        assert_ne!(c.how, MonitorMatch::FellBack);
        // No monitor flagged primary: first one.
        let none_primary = [left_hidpi()];
        let c = choose_monitor(None, &none_primary).unwrap();
        assert_eq!(c.monitor.scale, 1.5);
        assert!(choose_monitor(None, &[]).is_none());
    }

    #[test]
    fn hud_monitor_matches_name_and_position_first() {
        // Two monitors with the same name (it happens with some drivers): the
        // one at the saved position wins.
        let mut twin = right_screen();
        twin.name = Some("\\\\.\\DISPLAY2".to_string());
        let ms = [primary(), twin, left_hidpi()];
        let saved = saved_mon("\\\\.\\DISPLAY2", Some(left_hidpi().bounds));
        let c = choose_monitor(Some(&saved), &ms).unwrap();
        assert_eq!(c.how, MonitorMatch::NameAndPosition);
        assert_eq!(c.monitor.bounds, left_hidpi().bounds);
    }

    #[test]
    fn hud_monitor_renamed_matches_by_position_and_size() {
        // After a driver update DISPLAY2 is called DISPLAY5, same spot and size.
        let mut renamed = left_hidpi();
        renamed.name = Some("\\\\.\\DISPLAY5".to_string());
        let ms = [primary(), renamed];
        let saved = saved_mon("\\\\.\\DISPLAY2", Some(left_hidpi().bounds));
        let c = choose_monitor(Some(&saved), &ms).unwrap();
        assert_eq!(c.how, MonitorMatch::PositionAndSize);
        assert_eq!(c.monitor.name.as_deref(), Some("\\\\.\\DISPLAY5"));
        // Same spot but another size is not a match on position.
        let mut resized = left_hidpi();
        resized.name = Some("\\\\.\\DISPLAY5".to_string());
        resized.bounds.w = 1920;
        let ms = [primary(), resized];
        let c = choose_monitor(Some(&saved), &ms).unwrap();
        assert_eq!(c.how, MonitorMatch::FellBack);
    }

    #[test]
    fn hud_monitor_moved_matches_by_name() {
        // The screen was dragged to the other side in Display settings.
        let mut moved = left_hidpi();
        moved.bounds = Rect::new(1920, 0, 2560, 1440);
        let ms = [primary(), moved];
        let saved = saved_mon("\\\\.\\DISPLAY2", Some(left_hidpi().bounds));
        let c = choose_monitor(Some(&saved), &ms).unwrap();
        assert_eq!(c.how, MonitorMatch::Name);
        assert_eq!(c.monitor.bounds.x, 1920);
        // An older build's setting (no bounds) matches by name too.
        let legacy = saved_mon("\\\\.\\DISPLAY2", None);
        let c = choose_monitor(Some(&legacy), &ms).unwrap();
        assert_eq!(c.how, MonitorMatch::Name);
    }

    #[test]
    fn hud_monitor_gone_falls_back_to_the_primary() {
        let ms = [left_hidpi(), primary()];
        let saved = saved_mon("\\\\.\\DISPLAY9", Some(right_screen().bounds));
        let c = choose_monitor(Some(&saved), &ms).unwrap();
        assert!(c.monitor.is_primary);
        assert_eq!(c.how, MonitorMatch::FellBack);
        let legacy = saved_mon("\\\\.\\DISPLAY9", None);
        assert_eq!(
            choose_monitor(Some(&legacy), &ms).unwrap().how,
            MonitorMatch::FellBack
        );
    }

    // --- custom HUD position ---

    /// The content of a panel: 300 x 200 logical, grabbed at its logo (20, 10).
    const GRAB: (f64, f64) = (20.0, 10.0);
    const CONTENT: (f64, f64) = (300.0, 200.0);

    #[test]
    fn drag_in_the_middle_follows_the_cursor() {
        let ms = [primary()];
        let f = drag_frame((800, 500), GRAB, CONTENT, &ms, 340.0).unwrap();
        assert_eq!(f.monitor, 0);
        assert_eq!(f.corner, None);
        // Content top-left = cursor - grab; the strip starts 16 px left of it.
        assert_eq!(f.content, Rect::new(780, 490, 300, 200));
        assert_eq!(f.strip, Rect::new(764, 0, 340, 1032));
        assert_eq!(
            f.custom,
            HudCustom {
                dx: 764.0,
                top: 490.0
            }
        );
    }

    #[test]
    fn drag_is_clamped_to_the_work_area() {
        let ms = [primary()];
        // Far below the taskbar and past the right edge: the nearest valid spot
        // (which is also a corner snap).
        let f = drag_frame((5000, 5000), GRAB, CONTENT, &ms, 340.0).unwrap();
        assert_eq!(f.strip.x, 1920 - 340);
        assert_eq!(f.content.y, 1032 - 8 - 200);
        assert!(f.content.is_inside(&ms[0].work));
        assert_eq!(f.corner, Some(Corner::BottomRight));
    }

    #[test]
    fn drag_snaps_to_edges_and_corners() {
        let ms = [primary()];
        // 20 px from the left edge (within 24): snaps left, no corner.
        let f = drag_frame((20 + 16 + 20, 500), GRAB, CONTENT, &ms, 340.0).unwrap();
        assert_eq!(f.strip.x, 0);
        assert_eq!(f.corner, None);
        // 30 px off the left edge: no snap.
        let f = drag_frame((30 + 16 + 20, 500), GRAB, CONTENT, &ms, 340.0).unwrap();
        assert_eq!(f.strip.x, 30);
        // Near the top too: the top-left corner.
        let f = drag_frame((40, 8 + 15 + 10), GRAB, CONTENT, &ms, 340.0).unwrap();
        assert_eq!((f.strip.x, f.content.y), (0, 8));
        assert_eq!(f.corner, Some(Corner::TopLeft));
        // Near the right edge and the bottom: bottom-right.
        let f = drag_frame((1900, 1000), GRAB, CONTENT, &ms, 340.0).unwrap();
        assert_eq!(f.corner, Some(Corner::BottomRight));
        // Top-right and bottom-left.
        assert_eq!(
            drag_frame((1900, 20), GRAB, CONTENT, &ms, 340.0)
                .unwrap()
                .corner,
            Some(Corner::TopRight)
        );
        assert_eq!(
            drag_frame((40, 1000), GRAB, CONTENT, &ms, 340.0)
                .unwrap()
                .corner,
            Some(Corner::BottomLeft)
        );
    }

    #[test]
    fn drag_onto_another_monitor_takes_its_scale() {
        let ms = [primary(), left_hidpi()];
        // Cursor on the hidpi screen left of the primary.
        let f = drag_frame((-1500, 300), GRAB, CONTENT, &ms, 340.0).unwrap();
        assert_eq!(f.monitor, 1);
        // Everything 1.5x: strip 510 wide, content 450 x 300, grab (30, 15).
        assert_eq!(f.strip.w, 510);
        assert_eq!(f.content, Rect::new(-1530, 285, 450, 300));
        assert_eq!(f.strip.x, -1530 - 24);
        assert!(f.content.is_inside(&ms[1].work));
        // Saved offsets are logical: (strip.x - work.x) / 1.5.
        assert_eq!(f.custom.dx, (-1554.0 + 2560.0) / 1.5);
        assert_eq!(f.custom.top, (285.0 + 200.0) / 1.5);
    }

    #[test]
    fn cursor_between_monitors_picks_the_nearest() {
        let ms = [primary(), right_screen()];
        assert_eq!(monitor_near(100, 100, &ms), Some(0));
        assert_eq!(monitor_near(2000, 100, &ms), Some(1));
        // Below the primary (no screen there): the primary is nearer.
        assert_eq!(monitor_near(500, 1500, &ms), Some(0));
        assert_eq!(monitor_near(0, 0, &[]), None);
    }

    #[test]
    fn custom_position_restores_and_clamps() {
        let m = primary();
        let c = HudCustom {
            dx: 700.0,
            top: 300.0,
        };
        assert_eq!(
            custom_strip(&m.work, 1.0, 340.0, &c),
            (Rect::new(700, 0, 340, 1032), 300.0)
        );
        // Same logical spot on a 2x screen.
        let mut hi = primary();
        hi.bounds = Rect::new(0, 0, 3840, 2160);
        hi.work = Rect::new(0, 0, 3840, 2064);
        assert_eq!(
            custom_strip(&hi.work, 2.0, 340.0, &c),
            (Rect::new(1400, 0, 680, 2064), 300.0)
        );
        // Saved on a bigger screen: pulled back to the nearest valid spot.
        let far = HudCustom {
            dx: 3000.0,
            top: 2000.0,
        };
        let (strip, top) = custom_strip(&m.work, 1.0, 340.0, &far);
        assert_eq!(strip.x, 1920 - 340);
        assert_eq!(top, 1032.0 - 48.0);
        let neg = HudCustom {
            dx: -50.0,
            top: -10.0,
        };
        let (strip, top) = custom_strip(&m.work, 1.0, 340.0, &neg);
        assert_eq!((strip.x, top), (0, 8.0));
        // On a monitor with negative coordinates.
        let l = left_hidpi();
        let (strip, _) = custom_strip(
            &l.work,
            1.5,
            340.0,
            &HudCustom {
                dx: 100.0,
                top: 50.0,
            },
        );
        assert_eq!(strip.x, -2560 + 150);
        assert!(strip.is_inside(&l.work));
    }

    #[test]
    fn custom_position_json() {
        let c = HudCustom {
            dx: 12.5,
            top: 300.0,
        };
        assert_eq!(
            parse_hud_custom(&serde_json::to_string(&c).unwrap()),
            Some(c)
        );
        assert_eq!(parse_hud_custom("null"), None);
        assert_eq!(parse_hud_custom("{\"dx\":1}"), None);
    }

    #[test]
    fn saved_monitor_parses_both_formats() {
        // Older builds: a JSON string, or null.
        assert_eq!(
            parse_saved_monitor("\"\\\\\\\\.\\\\DISPLAY2\""),
            Some(saved_mon("\\\\.\\DISPLAY2", None))
        );
        assert_eq!(parse_saved_monitor("null"), None);
        assert_eq!(parse_saved_monitor("\"\""), None);
        assert_eq!(parse_saved_monitor("garbage"), None);
        assert_eq!(parse_saved_monitor("{}"), None);
        // This build: an object that round-trips.
        let s = saved_mon("\\\\.\\DISPLAY2", Some(left_hidpi().bounds));
        let raw = serde_json::to_string(&s).unwrap();
        assert_eq!(parse_saved_monitor(&raw), Some(s));
    }

    #[test]
    fn legacy_saved_monitor_migrates_once_seen() {
        let legacy = saved_mon("\\\\.\\DISPLAY2", None);
        // Not connected now: nothing to do yet.
        assert_eq!(migrate_saved_monitor(&legacy, &[primary()]), None);
        let ms = [primary(), left_hidpi()];
        let migrated = migrate_saved_monitor(&legacy, &ms).unwrap();
        assert_eq!(
            migrated,
            saved_mon("\\\\.\\DISPLAY2", Some(left_hidpi().bounds))
        );
        // Already has bounds: left alone.
        assert_eq!(migrate_saved_monitor(&migrated, &ms), None);
        // Picking a monitor stores its bounds.
        assert_eq!(saved_monitor_for("\\\\.\\DISPLAY2", &ms), migrated);
        assert_eq!(saved_monitor_for("\\\\.\\DISPLAY9", &ms).rect, None);
    }

    #[test]
    fn hud_strip_on_a_negative_hidpi_monitor() {
        let m = left_hidpi();
        let right = hud_strip(&m.work, m.scale, 340.0, false);
        assert_eq!(right, Rect::new(-510, -200, 510, 1380));
        assert!(right.is_inside(&m.work));
        let left = hud_strip(&m.work, m.scale, 340.0, true);
        assert_eq!(left, Rect::new(-2560, -200, 510, 1380));
    }

    #[test]
    fn hud_strip_never_wider_than_a_tiny_work_area() {
        let work = Rect::new(0, 0, 200, 600);
        assert_eq!(
            hud_strip(&work, 1.0, 340.0, false),
            Rect::new(0, 0, 200, 600)
        );
    }

    #[test]
    fn labels() {
        assert_eq!(
            monitor_label(Some("\\\\.\\DISPLAY2"), 0, 2560, 1440),
            "Display 2 \u{b7} 2560\u{d7}1440"
        );
        assert_eq!(
            monitor_label(Some("Dell U2720Q"), 1, 3840, 2160),
            "Display 2 \u{b7} 3840\u{d7}2160"
        );
        assert_eq!(
            monitor_label(None, 0, 1920, 1080),
            "Display 1 \u{b7} 1920\u{d7}1080"
        );
    }

    #[test]
    fn hit_rect_follows_the_window_origin_including_negative() {
        // Content 40 px in from the client's left, at the strip's bottom.
        let rel = (40, 900, 300, 400);
        assert_eq!(hit_rect_abs((0, 0), rel), (40, 900, 340, 1300));
        // HUD on a monitor left of and above the primary.
        assert_eq!(hit_rect_abs((-510, -200), rel), (-470, 700, -170, 1100));
        // The mouse hook's test (x in [left, right], y in [top, bottom]) sees the
        // cursor at a negative desktop coordinate inside it.
        let (l, t, r, b) = hit_rect_abs((-510, -200), rel);
        assert!(l <= -300 && -300 <= r && t <= 800 && 800 <= b);
    }
}
