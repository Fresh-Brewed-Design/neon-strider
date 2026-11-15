//! Live tray readout: the tooltip says what the belt is doing, and the icon is
//! lit while the belt runs, normal when connected and idle, dimmed offline.
//!
//! The text and the throttle are pure and tested here; the snapshot emitter
//! calls `Updater::apply` with every snapshot it builds.
//!
//! This file is declared from `snapshot.rs` (`#[path]`), not `lib.rs`.

use super::AppSnapshot;
use crate::protocol::MachineStatus;
use tauri::AppHandle;

/// The tray's id (`TrayIconBuilder::with_id` in lib.rs).
pub const TRAY_ID: &str = "main";

/// Windows' tooltip limit: 128 UTF-16 units including the terminating null.
const MAX_TOOLTIP_UNITS: usize = 127;
/// The tooltip changes at most this often.
const TOOLTIP_EVERY_MS: u64 = 2000;

const APP: &str = "Neon Strider";
const SEP: &str = " \u{b7} ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    /// The belt is moving: lit icon.
    Running,
    /// Connected, belt still: the normal icon.
    Idle,
    /// No treadmill: dimmed icon.
    Offline,
}

/// What the tooltip needs, taken from a snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct TrayView {
    pub connected: bool,
    pub connecting: bool,
    pub status: MachineStatus,
    /// Calibrated belt speed.
    pub speed_kmh: f32,
    /// The open session's distance, if a session is open.
    pub session_m: Option<f64>,
    pub today_m: f64,
    /// The running program's name.
    pub program: Option<String>,
}

impl TrayView {
    pub fn from_snapshot(s: &AppSnapshot) -> Self {
        Self {
            connected: s.connection.connected,
            connecting: s.connection.connecting,
            status: s.deck.status,
            speed_kmh: s.deck.speed_kmh,
            session_m: s.session.active_id.map(|_| s.session.distance_m),
            today_m: s.session.daily_distance_m,
            program: s.active_program.as_ref().map(|a| a.name.clone()),
        }
    }

    pub fn state(&self) -> TrayState {
        if !self.connected {
            TrayState::Offline
        } else if matches!(
            self.status,
            MachineStatus::Running | MachineStatus::Starting
        ) {
            TrayState::Running
        } else {
            TrayState::Idle
        }
    }
}

fn units(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Cut `s` to at most `max` UTF-16 units, ending in an ellipsis when cut.
fn shorten(s: &str, max: usize) -> String {
    if units(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    for c in s.chars() {
        if units(&out) + c.len_utf16() + 1 > max {
            break;
        }
        out.push(c);
    }
    let trimmed = out.trim_end().to_string();
    trimmed + "\u{2026}"
}

/// The tooltip text, under Windows' 127-unit limit. Examples:
/// `Neon Strider · 3.2 km/h · 1.84 km · today 5.6 km`,
/// `Neon Strider · idle · today 5.6 km`, `Neon Strider · treadmill offline`.
/// A running program's name comes first; it is the part shortened if needed.
pub fn tooltip(v: &TrayView) -> String {
    if !v.connected {
        let what = if v.connecting {
            "connecting"
        } else {
            "treadmill offline"
        };
        return format!("{APP}{SEP}{what}");
    }
    let today = format!("today {:.1} km", v.today_m / 1000.0);
    let mut parts: Vec<String> = Vec::new();
    match v.status {
        MachineStatus::Running | MachineStatus::Starting => {
            parts.push(format!("{:.1} km/h", v.speed_kmh));
        }
        MachineStatus::Paused => parts.push("paused".to_string()),
        MachineStatus::Stopped => parts.push("idle".to_string()),
    }
    if v.status != MachineStatus::Stopped {
        if let Some(m) = v.session_m {
            parts.push(format!("{:.2} km", m / 1000.0));
        }
    }
    parts.push(today);
    let rest = parts.join(SEP);

    let text = match &v.program {
        Some(name) => {
            let fixed = units(APP) + 2 * units(SEP) + units(&rest);
            let room = MAX_TOOLTIP_UNITS.saturating_sub(fixed);
            let name = shorten(name.trim(), room);
            if name.is_empty() {
                format!("{APP}{SEP}{rest}")
            } else {
                format!("{APP}{SEP}{name}{SEP}{rest}")
            }
        }
        None => format!("{APP}{SEP}{rest}"),
    };
    // Belt and braces: never over the limit, whatever the numbers.
    shorten(&text, MAX_TOOLTIP_UNITS)
}

/// Remembers what the tray shows, so the tooltip changes at most once per
/// `TOOLTIP_EVERY_MS` and the icon only when the state does.
#[derive(Debug, Default)]
pub struct Throttle {
    text: Option<String>,
    text_at_ms: Option<u64>,
    state: Option<TrayState>,
}

impl Throttle {
    /// The tooltip to set now, if any: it changed and the last change is at
    /// least `TOOLTIP_EVERY_MS` old. A change held back is picked up by a later
    /// call (the emitter calls this every 250 ms).
    pub fn tooltip(&mut self, text: &str, now_ms: u64) -> Option<String> {
        if self.text.as_deref() == Some(text) {
            return None;
        }
        if self
            .text_at_ms
            .is_some_and(|t| now_ms.saturating_sub(t) < TOOLTIP_EVERY_MS)
        {
            return None;
        }
        self.text = Some(text.to_string());
        self.text_at_ms = Some(now_ms);
        Some(text.to_string())
    }

    /// The icon to set now, if the state changed.
    pub fn icon(&mut self, state: TrayState) -> Option<TrayState> {
        if self.state == Some(state) {
            return None;
        }
        self.state = Some(state);
        Some(state)
    }
}

// Variants written by build.rs from icons/tray.png (see tray_art.rs).
const ICON_IDLE: &[u8] = include_bytes!("../icons/tray.png");
const ICON_RUNNING: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tray-running.png"));
const ICON_OFFLINE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tray-offline.png"));

/// The tray image for a state.
pub fn icon(state: TrayState) -> Option<tauri::image::Image<'static>> {
    let bytes = match state {
        TrayState::Running => ICON_RUNNING,
        TrayState::Idle => ICON_IDLE,
        TrayState::Offline => ICON_OFFLINE,
    };
    tauri::image::Image::from_bytes(bytes).ok()
}

/// Keeps the tray in step with the snapshots.
#[derive(Debug, Default)]
pub struct Updater {
    throttle: Throttle,
}

impl Updater {
    pub fn apply(&mut self, app: &AppHandle, s: &AppSnapshot, now_ms: u64) {
        let view = TrayView::from_snapshot(s);
        let text = self.throttle.tooltip(&tooltip(&view), now_ms);
        let state = self.throttle.icon(view.state());
        if text.is_none() && state.is_none() {
            return;
        }
        let Some(tray) = app.tray_by_id(TRAY_ID) else {
            return;
        };
        if let Some(text) = text {
            if let Err(e) = tray.set_tooltip(Some(text)) {
                log::debug!("Tray tooltip not updated: {}", e);
            }
        }
        if let Some(state) = state {
            if let Err(e) = tray.set_icon(icon(state)) {
                log::debug!("Tray icon not updated: {}", e);
            }
        }
    }
}

#[cfg(test)]
#[path = "tray_art.rs"]
mod tray_art;

#[cfg(test)]
mod tests {
    use super::*;

    fn walking() -> TrayView {
        TrayView {
            connected: true,
            connecting: false,
            status: MachineStatus::Running,
            speed_kmh: 3.2,
            session_m: Some(1840.0),
            today_m: 5600.0,
            program: None,
        }
    }

    #[test]
    fn tooltip_while_walking() {
        assert_eq!(
            tooltip(&walking()),
            "Neon Strider \u{b7} 3.2 km/h \u{b7} 1.84 km \u{b7} today 5.6 km"
        );
    }

    #[test]
    fn tooltip_idle_paused_and_offline() {
        let idle = TrayView {
            status: MachineStatus::Stopped,
            speed_kmh: 0.0,
            session_m: None,
            ..walking()
        };
        assert_eq!(
            tooltip(&idle),
            "Neon Strider \u{b7} idle \u{b7} today 5.6 km"
        );
        let paused = TrayView {
            status: MachineStatus::Paused,
            ..walking()
        };
        assert_eq!(
            tooltip(&paused),
            "Neon Strider \u{b7} paused \u{b7} 1.84 km \u{b7} today 5.6 km"
        );
        let offline = TrayView {
            connected: false,
            ..walking()
        };
        assert_eq!(tooltip(&offline), "Neon Strider \u{b7} treadmill offline");
        let connecting = TrayView {
            connecting: true,
            ..offline
        };
        assert_eq!(tooltip(&connecting), "Neon Strider \u{b7} connecting");
    }

    #[test]
    fn tooltip_names_the_program() {
        let v = TrayView {
            program: Some("Workday".to_string()),
            ..walking()
        };
        assert_eq!(
            tooltip(&v),
            "Neon Strider \u{b7} Workday \u{b7} 3.2 km/h \u{b7} 1.84 km \u{b7} today 5.6 km"
        );
    }

    #[test]
    fn tooltip_stays_under_the_windows_limit() {
        // A very long program name (with characters outside the BMP, which take
        // two UTF-16 units each) is shortened; the numbers survive.
        let v = TrayView {
            program: Some("\u{1F3C3} Very long program name ".repeat(20)),
            today_m: 123_456.0,
            ..walking()
        };
        let t = tooltip(&v);
        assert!(
            t.encode_utf16().count() <= MAX_TOOLTIP_UNITS,
            "{} units",
            t.encode_utf16().count()
        );
        assert!(t.ends_with("3.2 km/h \u{b7} 1.84 km \u{b7} today 123.5 km"));
        assert!(t.contains('\u{2026}'));
        // Short names are not touched.
        assert!(!tooltip(&TrayView {
            program: Some("Focus".into()),
            ..walking()
        })
        .contains('\u{2026}'));
        // Absurd numbers still fit.
        let huge = TrayView {
            speed_kmh: f32::MAX,
            session_m: Some(f64::MAX),
            today_m: f64::MAX,
            ..walking()
        };
        assert!(tooltip(&huge).encode_utf16().count() <= MAX_TOOLTIP_UNITS);
    }

    #[test]
    fn shorten_counts_utf16_units() {
        assert_eq!(shorten("abc", 3), "abc");
        assert_eq!(shorten("abcd", 3), "ab\u{2026}");
        assert_eq!(shorten("abcd", 0), "");
        let emoji = "\u{1F3C3}\u{1F3C3}\u{1F3C3}";
        let s = shorten(emoji, 4);
        assert!(s.encode_utf16().count() <= 4);
        assert_eq!(s, "\u{1F3C3}\u{2026}");
    }

    #[test]
    fn tray_state_follows_the_belt_and_link() {
        assert_eq!(walking().state(), TrayState::Running);
        let starting = TrayView {
            status: MachineStatus::Starting,
            ..walking()
        };
        assert_eq!(starting.state(), TrayState::Running);
        let paused = TrayView {
            status: MachineStatus::Paused,
            ..walking()
        };
        assert_eq!(paused.state(), TrayState::Idle);
        let stopped = TrayView {
            status: MachineStatus::Stopped,
            ..walking()
        };
        assert_eq!(stopped.state(), TrayState::Idle);
        let offline = TrayView {
            connected: false,
            ..walking()
        };
        assert_eq!(offline.state(), TrayState::Offline);
    }

    #[test]
    fn tooltip_is_throttled_to_one_change_per_two_seconds() {
        let mut t = Throttle::default();
        assert_eq!(t.tooltip("a", 0), Some("a".to_string()));
        // Same text: nothing to do.
        assert_eq!(t.tooltip("a", 5000), None);
        // A change goes out at once when the last one is old enough...
        assert_eq!(t.tooltip("b", 5000), Some("b".to_string()));
        // ...but not within 2 s of it; the latest text goes out after.
        assert_eq!(t.tooltip("c", 5250), None);
        assert_eq!(t.tooltip("d", 6999), None);
        assert_eq!(t.tooltip("d", 7000), Some("d".to_string()));
    }

    #[test]
    fn icon_changes_only_with_the_state() {
        let mut t = Throttle::default();
        assert_eq!(t.icon(TrayState::Offline), Some(TrayState::Offline));
        assert_eq!(t.icon(TrayState::Offline), None);
        assert_eq!(t.icon(TrayState::Idle), Some(TrayState::Idle));
        assert_eq!(t.icon(TrayState::Running), Some(TrayState::Running));
        assert_eq!(t.icon(TrayState::Running), None);
    }

    #[test]
    fn icon_variants_decode_at_tray_size() {
        for state in [TrayState::Running, TrayState::Idle, TrayState::Offline] {
            let img = icon(state).expect("decodes");
            assert_eq!((img.width(), img.height()), (32, 32));
        }
        let idle = icon(TrayState::Idle).unwrap();
        let running = icon(TrayState::Running).unwrap();
        let offline = icon(TrayState::Offline).unwrap();
        assert_ne!(idle.rgba(), running.rgba());
        assert_ne!(idle.rgba(), offline.rgba());
    }

    #[test]
    fn running_art_rims_the_tile_and_keeps_the_middle() {
        // A 8x8 opaque tile inside a 12x12 transparent square.
        let (w, h) = (12u32, 12u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for y in 2..10 {
            for x in 2..10 {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&[6, 10, 16, 255]);
            }
        }
        let mut lit = px.clone();
        tray_art::running(&mut lit, w, h);
        let at = |p: &[u8], x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [p[i], p[i + 1], p[i + 2], p[i + 3]]
        };
        // Edge pixels are rim-coloured and keep their alpha.
        assert_eq!(at(&lit, 2, 5), [0x00, 0xf3, 0xff, 255]);
        assert_eq!(at(&lit, 3, 5), [0x00, 0xf3, 0xff, 255]);
        // The middle is untouched, and so is the transparent outside.
        assert_eq!(at(&lit, 5, 5), [6, 10, 16, 255]);
        assert_eq!(at(&lit, 0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn offline_art_is_grey_and_keeps_alpha() {
        let mut px = vec![0x00, 0xf3, 0xff, 200, 0xff, 0x2b, 0xd6, 255];
        tray_art::offline(&mut px);
        assert_eq!(px[0], px[1]);
        assert_eq!(px[1], px[2]);
        assert_eq!(px[3], 200);
        assert_eq!(px[4], px[5]);
        assert_eq!(px[7], 255);
        // Darker than the cyan's brightness.
        assert!(px[0] < 0xf3);
    }
}
