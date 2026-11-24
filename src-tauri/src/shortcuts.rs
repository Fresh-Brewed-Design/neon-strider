//! Global shortcuts: which keys run each one, saved per user and changed in
//! System -> Shortcuts.
//!
//! A key is its `KeyboardEvent.code` plus modifiers (`KeyCombo`), so the
//! dashboard records it as the browser sees it and each OS maps it to its own
//! key code (`platform::keys`). Each shortcut has up to two keys; none
//! switches it off. The `shortcuts` setting holds them all; until it is
//! first saved, each OS's defaults apply (on Windows the minify key comes from
//! the older `hud_minify_hotkey` setting, which a save then deletes).
//!
//! While the dashboard records a key, the shortcuts pause (`set_recording`),
//! so pressing one that is already in use reaches the recorder rather than
//! running; the pause ends by itself after `RECORDING_LIMIT`.

use crate::hooks::Shortcut;
use crate::persistence;
use crate::platform::{self, keys, Os};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tauri::AppHandle;

const KEY_SHORTCUTS: &str = "shortcuts";
/// The minify hotkey of builds before shortcuts could be changed
/// (`{"enabled":true,"key":"insert"}`); read once for the default.
const KEY_OLD_MINIFY_HOTKEY: &str = "hud_minify_hotkey";
/// Most keys a shortcut can have.
pub const MAX_KEYS: usize = 2;
/// A recording the dashboard never ended (closed mid-way) ends by itself, so
/// the shortcuts never stay paused. Recording a key takes a few seconds.
const RECORDING_LIMIT: Duration = Duration::from_secs(30);

/// One key combination: the key as `KeyboardEvent.code` names it ("KeyP",
/// "Equal", "PageUp") and the modifiers held with it. `alt` is ⌥ and `meta`
/// ⌘ on a Mac; `meta` is the Windows key on Windows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct KeyCombo {
    pub code: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub meta: bool,
}

/// Each shortcut's keys. A shortcut left out has its default; an empty list
/// is switched off.
pub type Bindings = BTreeMap<Shortcut, Vec<KeyCombo>>;

/// System -> Shortcuts as the windows see it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ShortcutsSnapshot {
    /// Every shortcut, in `Shortcut::ALL` order.
    pub list: Vec<ShortcutView>,
    /// Paused while the dashboard records a key.
    pub recording: bool,
    /// Some shortcut differs from this OS's defaults (RESET offered).
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ShortcutView {
    pub shortcut: Shortcut,
    /// Its keys; none while it is switched off.
    pub keys: Vec<ShortcutKey>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ShortcutKey {
    pub combo: KeyCombo,
    /// As the OS writes it: `Alt+Page Up`, `⌃⌥⇞`.
    pub label: String,
    /// Another app already has it, so it does nothing here (macOS).
    pub taken: bool,
}

static BINDINGS: Mutex<Option<Bindings>> = Mutex::new(None);
/// Counts recordings, so a recording's own time limit only ends that one.
static RECORDING: AtomicU64 = AtomicU64::new(0);
static RECORDING_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn lock() -> MutexGuard<'static, Option<Bindings>> {
    BINDINGS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The older build's minify key: its `KeyboardEvent.code`, or `None` when it
/// was switched off. Unreadable or missing is the old default, Insert.
fn old_minify_code(raw: Option<&str>) -> Option<&'static str> {
    let Some(raw) = raw else {
        return Some("Insert");
    };
    let value: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
    if value.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
        return None;
    }
    match value.get("key").and_then(serde_json::Value::as_str) {
        Some("pause") => Some("Pause"),
        Some("scroll_lock") => Some("ScrollLock"),
        _ => Some("Insert"),
    }
}

/// This OS's defaults, with `saved` laid over them (a shortcut the saved
/// copy names takes its keys; unknown or malformed entries are skipped).
pub fn resolve(os: Os, saved: Option<&str>, old_minify: Option<&str>) -> Bindings {
    let mut bindings = keys::defaults(os, old_minify_code(old_minify));
    if let Some(raw) = saved {
        match serde_json::from_str::<Bindings>(raw) {
            Ok(stored) => {
                for (shortcut, keys) in stored {
                    bindings.insert(shortcut, keys.into_iter().take(MAX_KEYS).collect());
                }
            }
            Err(e) => log::warn!("Saved shortcuts unreadable ({}); using the defaults", e),
        }
    }
    bindings
}

/// Read the saved shortcuts and hand them to the OS. Call once at startup.
pub fn init(app: &AppHandle) {
    let saved = persistence::read_setting(app, KEY_SHORTCUTS);
    let old = persistence::read_setting(app, KEY_OLD_MINIFY_HOTKEY);
    let bindings = resolve(platform::OS, saved.as_deref(), old.as_deref());
    platform::set_shortcuts(&bindings);
    *lock() = Some(bindings);
}

/// The shortcuts in use now (the defaults before `init`).
pub fn current() -> Bindings {
    lock()
        .clone()
        .unwrap_or_else(|| keys::defaults(platform::OS, Some("Insert")))
}

/// Why `bindings` can't be saved on `os`, in one sentence: a key the OS
/// refuses, too many keys for one shortcut, or a key used twice. A key used
/// twice names the shortcut that already has it, not `changed`.
pub fn check(os: Os, bindings: &Bindings, changed: Option<Shortcut>) -> Result<(), String> {
    for (shortcut, keys) in bindings {
        if keys.len() > MAX_KEYS {
            return Err(format!(
                "{} can have at most {MAX_KEYS} keys.",
                shortcut.name()
            ));
        }
        for combo in keys {
            if let Some(why) = keys::refusal(os, combo) {
                return Err(format!("{}: {why}", keys::label(os, combo)));
            }
        }
    }
    let all: Vec<(Shortcut, &KeyCombo)> = bindings
        .iter()
        .flat_map(|(s, keys)| keys.iter().map(move |k| (*s, k)))
        .collect();
    for (i, (a, combo)) in all.iter().enumerate() {
        if let Some((b, _)) = all[i + 1..].iter().find(|(_, other)| other == combo) {
            let label = keys::label(os, combo);
            if a == b {
                return Err(format!("{label} is already one of its keys."));
            }
            let other = if Some(*a) == changed { b } else { a };
            return Err(format!("{label} already {}.", other.what_it_does()));
        }
    }
    Ok(())
}

/// Give `shortcut` these keys (none switches it off), after `check`.
pub fn set(app: &AppHandle, shortcut: Shortcut, keys: Vec<KeyCombo>) -> Result<(), String> {
    let mut bindings = current();
    bindings.insert(shortcut, keys);
    check(platform::OS, &bindings, Some(shortcut))?;
    apply(app, bindings)
}

/// Every shortcut back to this OS's defaults (Windows: Alt+Insert to minify).
pub fn reset(app: &AppHandle) -> Result<(), String> {
    apply(app, keys::defaults(platform::OS, Some("Insert")))
}

fn apply(app: &AppHandle, bindings: Bindings) -> Result<(), String> {
    let json = serde_json::to_string(&bindings).map_err(|e| e.to_string())?;
    persistence::write_setting(app, KEY_SHORTCUTS, &json)?;
    // The older setting has been carried over; nothing reads it after a save.
    let _ = persistence::delete_setting(app, KEY_OLD_MINIFY_HOTKEY);
    // Published before the recording check: a recording that ends meanwhile
    // (another thread) then applies these keys too, never the ones before.
    *lock() = Some(bindings.clone());
    if !RECORDING_ON.load(Ordering::SeqCst) {
        platform::set_shortcuts(&bindings);
    }
    Ok(())
}

/// Pause the shortcuts while the dashboard records a key (`on`), or bring
/// them back. A pause the dashboard never ends lifts after `RECORDING_LIMIT`.
pub fn set_recording(on: bool) {
    let id = RECORDING.fetch_add(1, Ordering::SeqCst) + 1;
    if RECORDING_ON.swap(on, Ordering::SeqCst) == on {
        return;
    }
    if on {
        platform::set_shortcuts(&Bindings::new());
        std::thread::spawn(move || {
            std::thread::sleep(RECORDING_LIMIT);
            if RECORDING.load(Ordering::SeqCst) == id {
                log::info!("Shortcut recording timed out; shortcuts back on");
                set_recording(false);
            }
        });
    } else {
        platform::set_shortcuts(&current());
    }
}

/// System -> Shortcuts: each shortcut's keys as the OS writes them, and
/// which another app has taken.
pub fn snapshot() -> ShortcutsSnapshot {
    let os = platform::OS;
    let bindings = current();
    let taken = platform::taken_shortcuts();
    let defaults = keys::defaults(os, Some("Insert"));
    ShortcutsSnapshot {
        list: Shortcut::ALL
            .iter()
            .map(|shortcut| ShortcutView {
                shortcut: *shortcut,
                keys: bindings
                    .get(shortcut)
                    .into_iter()
                    .flatten()
                    .map(|combo| ShortcutKey {
                        label: keys::label(os, combo),
                        taken: taken.contains(combo),
                        combo: combo.clone(),
                    })
                    .collect(),
            })
            .collect(),
        recording: RECORDING_ON.load(Ordering::SeqCst),
        changed: bindings != defaults,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(code: &str, ctrl: bool, alt: bool) -> KeyCombo {
        KeyCombo {
            code: code.to_string(),
            ctrl,
            alt,
            shift: false,
            meta: false,
        }
    }

    #[test]
    fn the_older_minify_setting_seeds_the_windows_default() {
        let minify =
            |old: Option<&str>| resolve(Os::Windows, None, old)[&Shortcut::ToggleMinified].clone();
        assert_eq!(minify(None), vec![c("Insert", false, true)]);
        assert_eq!(
            minify(Some(r#"{"enabled":true,"key":"scroll_lock"}"#)),
            vec![c("ScrollLock", false, true)]
        );
        assert_eq!(minify(Some(r#"{"enabled":false,"key":"pause"}"#)), vec![]);
        assert_eq!(minify(Some("junk")), vec![c("Insert", false, true)]);
    }

    #[test]
    fn saved_shortcuts_lie_over_the_defaults() {
        let saved =
            r#"{"toggle_start":[{"code":"KeyS","ctrl":true,"alt":true}],"toggle_minified":[]}"#;
        let b = resolve(Os::Windows, Some(saved), None);
        assert_eq!(b[&Shortcut::ToggleStart], vec![c("KeyS", true, true)]);
        assert!(b[&Shortcut::ToggleMinified].is_empty());
        // Ones not saved keep their default.
        assert_eq!(b[&Shortcut::InclineUp], vec![c("PageUp", false, true)]);
        // Unreadable: the defaults.
        assert_eq!(
            resolve(Os::Macos, Some("{"), None),
            keys::defaults(Os::Macos, Some("Insert"))
        );
    }

    #[test]
    fn a_key_can_run_only_one_shortcut() {
        let mut b = keys::defaults(Os::Windows, Some("Insert"));
        assert_eq!(check(Os::Windows, &b, None), Ok(()));
        b.insert(Shortcut::ToggleProgram, vec![c("Home", false, true)]);
        let taken = Err("Alt+Home already starts or pauses the belt.".to_string());
        assert_eq!(check(Os::Windows, &b, Some(Shortcut::ToggleProgram)), taken);
        // Named the same way when the shortcut changed sorts first.
        let mut b = keys::defaults(Os::Windows, Some("Insert"));
        b.insert(Shortcut::SpeedUp, vec![c("End", false, true)]);
        assert_eq!(
            check(Os::Windows, &b, Some(Shortcut::SpeedUp)),
            Err("Alt+End already runs or ends a program.".to_string())
        );
        b.insert(
            Shortcut::SpeedUp,
            vec![c("KeyQ", true, true), c("KeyQ", true, true)],
        );
        assert!(check(Os::Windows, &b, Some(Shortcut::SpeedUp))
            .unwrap_err()
            .contains("already one of its keys"));
    }

    #[test]
    fn check_names_the_key_and_the_rule() {
        let mut b = keys::defaults(Os::Macos, None);
        b.insert(Shortcut::ToggleMinified, vec![c("KeyM", false, true)]);
        let err = check(Os::Macos, &b, None).unwrap_err();
        assert!(err.starts_with("⌥M: Add ⌃ or ⌘"), "{err}");
        b.insert(
            Shortcut::ToggleMinified,
            vec![
                c("KeyM", true, true),
                c("KeyN", true, true),
                c("KeyB", true, true),
            ],
        );
        assert!(check(Os::Macos, &b, None)
            .unwrap_err()
            .contains("at most 2"));
    }
}
