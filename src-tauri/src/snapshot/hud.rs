//! The HUD config (pinned widgets, corner, display, dashboard pin) and the
//! list of displays. Rust owns both; the config is persisted to the same
//! settings keys the dashboard reads and writes.

use super::{maintenance, HudAnchor, HudConfig, HudIdleMode, HudMonitor, HudWidget};
use crate::persistence::{self, KEY_IS_PINNED, KEY_PINNED_ELEMENTS};
use crate::placement::{self, MonitorGeom};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Manager};

const KEY_HUD_ANCHOR: &str = "hud_anchor";
/// The HUD's display: `placement::SavedMonitor` as JSON (name and bounds), or
/// `null` for the primary. Older builds stored a JSON string (the name only).
const KEY_HUD_MONITOR: &str = "hud_monitor";
/// `placement::HudCustom` as JSON while the HUD sits where it was dragged;
/// `null` at its corner anchor.
const KEY_HUD_POSITION: &str = "hud_position";
/// `HudIdleMode` as a JSON string (`"full"`, `"strip"`, `"hidden"`).
const KEY_HUD_IDLE_MODE: &str = "hud_idle_mode";
/// `true` while the user has the HUD minified to the running strip.
const KEY_HUD_MINIFIED: &str = "hud_minified";
/// Longest display name accepted from a window.
/// Display names are a few dozen characters; anything longer is not a name.
const MAX_MONITOR_NAME: usize = 200;

static HUD: Mutex<HudConfig> = Mutex::new(HudConfig {
    widgets: Vec::new(),
    anchor: HudAnchor::BottomRight,
    monitor: None,
    dashboard_pinned: false,
    monitor_rect: None,
    custom: None,
    idle_mode: HudIdleMode::Full,
    minified: false,
});

static MONITORS: Mutex<Vec<MonitorGeom>> = Mutex::new(Vec::new());

fn hud_lock() -> MutexGuard<'static, HudConfig> {
    HUD.lock().unwrap_or_else(|e| e.into_inner())
}

fn monitors_lock() -> MutexGuard<'static, Vec<MonitorGeom>> {
    MONITORS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The HUD config in effect.
pub fn hud_config() -> HudConfig {
    hud_lock().clone()
}

/// `widgets` in order, each once.
pub(super) fn dedup_widgets(widgets: impl IntoIterator<Item = HudWidget>) -> Vec<HudWidget> {
    let mut out = Vec::new();
    for w in widgets {
        if !out.contains(&w) {
            out.push(w);
        }
    }
    out
}

/// Parse the stored `pinned_elements` JSON array. Unknown or non-string entries
/// are skipped (an older/newer build may have written names we don't know), and
/// duplicates are dropped.
pub fn parse_widgets(raw: &str) -> Vec<HudWidget> {
    let values: Vec<serde_json::Value> = serde_json::from_str(raw).unwrap_or_default();
    dedup_widgets(
        values
            .into_iter()
            .filter_map(|v| serde_json::from_value::<HudWidget>(v).ok()),
    )
}

pub(super) fn widgets_to_json(widgets: &[HudWidget]) -> String {
    serde_json::to_string(widgets).unwrap_or_else(|_| "[]".to_string())
}

/// The stored anchor: JSON-quoted (as the dashboard writes it) or bare.
pub(super) fn parse_anchor(raw: &str) -> HudAnchor {
    serde_json::from_str(raw)
        .or_else(|_| serde_json::from_str(&format!("\"{}\"", raw.trim_matches('"'))))
        .unwrap_or_default()
}

/// The stored idle mode: JSON-quoted or bare; anything else is `Full`, so a
/// missing or garbled value leaves the HUD as it always was.
pub(super) fn parse_idle_mode(raw: &str) -> HudIdleMode {
    serde_json::from_str(raw)
        .or_else(|_| serde_json::from_str(&format!("\"{}\"", raw.trim().trim_matches('"'))))
        .unwrap_or_default()
}

/// A stored boolean: `true`, bare or JSON-quoted; anything else is `false`.
pub(super) fn parse_flag(raw: &str) -> bool {
    raw.trim().trim_matches('"') == "true"
}

/// The stored monitor setting as (name, bounds). Both formats load (see
/// `placement::parse_saved_monitor`); one without a name means the primary.
pub(super) fn parse_monitor(raw: &str) -> (Option<String>, Option<placement::Rect>) {
    match placement::parse_saved_monitor(raw) {
        Some(placement::SavedMonitor {
            name: Some(name),
            rect,
        }) => (Some(name), rect),
        _ => (None, None),
    }
}

fn monitor_to_json(cfg: &HudConfig) -> String {
    match cfg.saved_monitor() {
        Some(saved) => serde_json::to_string(&saved).unwrap_or_else(|_| "null".to_string()),
        None => "null".to_string(),
    }
}

/// The display a window chose, with its current bounds so a later rename
/// still finds it.
pub(super) fn monitor_choice(monitor: Option<String>) -> (Option<String>, Option<placement::Rect>) {
    let monitor = clean_monitor(monitor);
    let rect = monitor
        .as_deref()
        .and_then(|name| placement::saved_monitor_for(name, &monitors_lock()).rect);
    (monitor, rect)
}

/// A display name from a window; blank or absurdly long means the primary display.
fn clean_monitor(monitor: Option<String>) -> Option<String> {
    monitor.filter(|n| !n.is_empty() && n.len() <= MAX_MONITOR_NAME)
}

pub(super) fn load(app: &AppHandle) {
    let read = |key: &str| persistence::read_setting(app, key);
    let (monitor, monitor_rect) = read(KEY_HUD_MONITOR)
        .map(|s| parse_monitor(&s))
        .unwrap_or_default();
    let config = HudConfig {
        widgets: read(KEY_PINNED_ELEMENTS)
            .map(|s| parse_widgets(&s))
            .unwrap_or_default(),
        anchor: read(KEY_HUD_ANCHOR)
            .map(|s| parse_anchor(&s))
            .unwrap_or_default(),
        monitor,
        dashboard_pinned: read(KEY_IS_PINNED).is_some_and(|s| s.trim_matches('"') == "true"),
        monitor_rect,
        custom: read(KEY_HUD_POSITION).and_then(|s| placement::parse_hud_custom(&s)),
        idle_mode: read(KEY_HUD_IDLE_MODE)
            .map(|s| parse_idle_mode(&s))
            .unwrap_or_default(),
        minified: read(KEY_HUD_MINIFIED).is_some_and(|s| parse_flag(&s)),
    };
    *hud_lock() = config;
}

fn persist(app: &AppHandle, cfg: &HudConfig) {
    let writes = [
        (KEY_PINNED_ELEMENTS, widgets_to_json(&cfg.widgets)),
        (KEY_IS_PINNED, cfg.dashboard_pinned.to_string()),
        // Quoted like the frontend's JSON.stringify of a string setting.
        (
            KEY_HUD_ANCHOR,
            serde_json::to_string(&cfg.anchor).unwrap_or_default(),
        ),
        (KEY_HUD_MONITOR, monitor_to_json(cfg)),
        (
            KEY_HUD_POSITION,
            serde_json::to_string(&cfg.custom).unwrap_or_else(|_| "null".to_string()),
        ),
        (
            KEY_HUD_IDLE_MODE,
            serde_json::to_string(&cfg.idle_mode).unwrap_or_default(),
        ),
        (KEY_HUD_MINIFIED, cfg.minified.to_string()),
    ];
    for (key, value) in writes {
        if let Err(e) = persistence::write_setting(app, key, &value) {
            log::error!("Failed to persist {}: {}", key, e);
        }
    }
}

/// Minify the HUD, or expand it again (its shortcut, Alt+Insert or ⌃⌥M
/// unless changed).
pub fn toggle_minified(app: &AppHandle) {
    mutate(app, |cfg| cfg.minified = !cfg.minified);
}

/// Mutate the HUD config, persist it, and keep the HUD window in step.
pub(super) fn mutate(app: &AppHandle, f: impl FnOnce(&mut HudConfig)) {
    let (before, after) = {
        let mut guard = hud_lock();
        let before = guard.clone();
        f(&mut guard);
        (before, guard.clone())
    };
    if before == after {
        return;
    }
    persist(app, &after);
    sync_window(app, &before, &after);
}

/// Keep the HUD window in step with its config: create it when the first widget
/// is pinned, and move it when the anchor, monitor or custom position changes.
/// It is never destroyed here; an empty HUD is transparent and click-through.
fn sync_window(app: &AppHandle, before: &HudConfig, after: &HudConfig) {
    if !after.widgets.is_empty() {
        crate::ensure_hud_native(app);
    }
    if before.anchor != after.anchor
        || before.saved_monitor() != after.saved_monitor()
        || before.custom != after.custom
    {
        if let Some(w) = app.get_webview_window("hud") {
            crate::position_hud(&w, after);
        }
    }
}

/// The dashboard still writes HUD settings straight to the settings table
/// (through `set_db_setting`). Re-read them so the snapshot and the HUD window
/// follow those writes instead of going stale until restart.
pub fn on_setting_written(app: &AppHandle, key: &str) {
    if maintenance::is_maintenance_key(key) {
        maintenance::reload(app);
        return;
    }
    let hud_keys = [
        KEY_PINNED_ELEMENTS,
        KEY_IS_PINNED,
        KEY_HUD_ANCHOR,
        KEY_HUD_MONITOR,
        KEY_HUD_POSITION,
        KEY_HUD_IDLE_MODE,
        KEY_HUD_MINIFIED,
    ];
    if !hud_keys.contains(&key) {
        return;
    }
    let before = hud_config();
    load(app);
    sync_window(app, &before, &hud_config());
}

/// The displays as the snapshot lists them (last refresh), marking the one the
/// HUD with config `hud` is on.
pub(super) fn monitors(hud: &HudConfig) -> Vec<HudMonitor> {
    let monitors = monitors_lock();
    let saved = hud.saved_monitor();
    let active = placement::choose_monitor(saved.as_ref(), &monitors).map(|c| c.monitor.bounds);
    monitors
        .iter()
        .enumerate()
        .map(|(i, m)| HudMonitor {
            name: m.name.clone().unwrap_or_default(),
            label: placement::monitor_label(m.name.as_deref(), i, m.bounds.w, m.bounds.h),
            is_primary: m.is_primary,
            active: active == Some(m.bounds),
            width: m.bounds.w.max(0) as u32,
            height: m.bounds.h.max(0) as u32,
        })
        // A display without a name cannot be chosen by name.
        .filter(|m| !m.name.is_empty())
        .collect()
}

/// Re-read the displays. When the layout, scale or work area changed (a monitor
/// plugged in or out, resolution or taskbar change) and the HUD exists, put it
/// back where its config says. Blocking (asks the main thread); never per tick.
pub fn refresh_monitors(app: &AppHandle) {
    let now = crate::current_monitors(app);
    if now.is_empty() {
        // The query failed; keep what we had.
        return;
    }
    migrate_monitor(app, &now);
    {
        let mut cached = monitors_lock();
        if *cached == now {
            return;
        }
        log::info!("Displays changed: {} now", now.len());
        *cached = now;
    }
    if let Some(w) = app.get_webview_window("hud") {
        crate::position_hud(&w, &hud_config());
    }
}

/// A HUD display saved by an older build has only a name. Once a display with
/// that name is connected, store its bounds too (the window is already there).
fn migrate_monitor(app: &AppHandle, monitors: &[MonitorGeom]) {
    let migrated = {
        let mut hud = hud_lock();
        let Some(next) = hud
            .saved_monitor()
            .and_then(|saved| placement::migrate_saved_monitor(&saved, monitors))
        else {
            return;
        };
        hud.monitor_rect = next.rect;
        hud.clone()
    };
    log::info!("HUD display setting now remembers the display's position as well as its name");
    persist(app, &migrated);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_monitor_setting_round_trips_and_defaults_to_primary() {
        // An older build's value (a name only) still loads.
        let name = Some("DISPLAY2".to_string());
        let legacy = serde_json::to_string(&name).unwrap();
        assert_eq!(parse_monitor(&legacy), (name.clone(), None));
        assert_eq!(parse_monitor("null"), (None, None));
        assert_eq!(parse_monitor("\"\""), (None, None));
        assert_eq!(parse_monitor("garbage"), (None, None));
        assert_eq!(HudConfig::default().monitor, None);
        assert_eq!(monitor_to_json(&HudConfig::default()), "null");
        // This build stores the bounds too, and reads them back.
        let cfg = HudConfig {
            monitor: name.clone(),
            monitor_rect: Some(placement::Rect::new(-2560, 0, 2560, 1440)),
            ..HudConfig::default()
        };
        let stored = monitor_to_json(&cfg);
        assert_eq!(parse_monitor(&stored), (name, cfg.monitor_rect));
        // The bounds never reach the windows.
        let json = serde_json::to_value(&cfg).unwrap();
        assert!(json.get("monitor_rect").is_none());
    }

    #[test]
    fn monitor_names_from_windows_are_bounded() {
        assert_eq!(
            clean_monitor(Some("DISPLAY2".into())).as_deref(),
            Some("DISPLAY2")
        );
        assert_eq!(clean_monitor(Some(String::new())), None);
        assert_eq!(clean_monitor(Some("x".repeat(201))), None);
        assert_eq!(
            clean_monitor(Some("x".repeat(200))).map(|n| n.len()),
            Some(200)
        );
        assert_eq!(clean_monitor(None), None);
    }

    #[test]
    fn hud_widget_round_trips_legacy_strings() {
        let legacy = [
            "TIMER",
            "DISTANCE",
            "CALORIES",
            "STEPS",
            "INCLINE",
            "VELOCITY",
            "NEURAL",
            "PROGRAMS",
            "DAILY",
            "CONTROLS",
            "ODOMETER",
            "HEART_RATE",
            "PC_WALK_SHARE",
        ];
        for name in legacy {
            let w: HudWidget = serde_json::from_str(&format!("\"{}\"", name)).unwrap();
            assert_eq!(serde_json::to_string(&w).unwrap(), format!("\"{}\"", name));
        }

        let all = parse_widgets(&format!("[\"{}\"]", legacy.join("\",\"")));
        assert_eq!(all.len(), 13);
        assert_eq!(
            widgets_to_json(&all),
            format!("[\"{}\"]", legacy.join("\",\""))
        );
    }

    #[test]
    fn legacy_automation_widget_loads_as_programs() {
        let w: HudWidget = serde_json::from_str("\"AUTOMATION\"").unwrap();
        assert_eq!(w, HudWidget::Programs);
        assert_eq!(serde_json::to_string(&w).unwrap(), "\"PROGRAMS\"");
        assert_eq!(
            parse_widgets(r#"["TIMER","AUTOMATION","PROGRAMS"]"#),
            vec![HudWidget::Timer, HudWidget::Programs]
        );
    }

    #[test]
    fn parse_widgets_skips_unknown_and_garbage() {
        assert_eq!(
            parse_widgets(r#"["TIMER","HOLOGRAM",7,null,"CONTROLS","TIMER"]"#),
            vec![HudWidget::Timer, HudWidget::Controls]
        );
        assert!(parse_widgets("not json").is_empty());
        assert!(parse_widgets("{}").is_empty());
        assert!(parse_widgets("[]").is_empty());
    }

    #[test]
    fn dedup_keeps_first_occurrences_in_order() {
        use HudWidget::*;
        assert_eq!(
            dedup_widgets([Steps, Timer, Steps, Daily, Timer]),
            vec![Steps, Timer, Daily]
        );
        assert!(dedup_widgets([]).is_empty());
    }

    #[test]
    fn idle_mode_round_trips_and_falls_back_to_full() {
        assert_eq!(HudConfig::default().idle_mode, HudIdleMode::Full);
        for mode in [HudIdleMode::Full, HudIdleMode::Strip, HudIdleMode::Hidden] {
            let stored = serde_json::to_string(&mode).unwrap();
            assert_eq!(parse_idle_mode(&stored), mode);
        }
        assert_eq!(
            serde_json::to_string(&HudIdleMode::Hidden).unwrap(),
            "\"hidden\""
        );
        assert_eq!(parse_idle_mode("strip"), HudIdleMode::Strip);
        assert_eq!(parse_idle_mode(" \"hidden\" "), HudIdleMode::Hidden);
        assert_eq!(parse_idle_mode(""), HudIdleMode::Full);
        assert_eq!(parse_idle_mode("null"), HudIdleMode::Full);
        assert_eq!(parse_idle_mode("\"STRIP\""), HudIdleMode::Full);
        assert_eq!(parse_idle_mode("{\"mode\":1}"), HudIdleMode::Full);
        assert_eq!(parse_idle_mode("garbage"), HudIdleMode::Full);
    }

    #[test]
    fn minified_defaults_off_and_reads_true_only() {
        assert!(!HudConfig::default().minified);
        assert!(parse_flag(&true.to_string()));
        assert!(parse_flag(" \"true\" "));
        for raw in ["false", "", "null", "TRUE", "1", "garbage"] {
            assert!(!parse_flag(raw), "{raw}");
        }
    }

    #[test]
    fn anchor_serialises_snake_case_with_default() {
        assert_eq!(HudAnchor::default(), HudAnchor::BottomRight);
        assert_eq!(
            serde_json::to_string(&HudAnchor::TopLeft).unwrap(),
            "\"top_left\""
        );
        assert_eq!(parse_anchor("\"bottom_left\""), HudAnchor::BottomLeft);
        assert_eq!(parse_anchor("top_right"), HudAnchor::TopRight);
        assert_eq!(parse_anchor("???"), HudAnchor::BottomRight);
    }
}
