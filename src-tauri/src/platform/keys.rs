//! The keys a global shortcut can use, in portable terms: the
//! `KeyboardEvent.code` the dashboard's recorder sees, with each OS's key
//! code and how each OS writes it. Also the rules a key combination must
//! pass on each OS, each OS's default shortcuts, and the pure parts of
//! matching a key press (Windows) or registering hot keys (macOS).
//!
//! Pure: compiled into every build and test, so the Windows and Linux CI runs
//! test the macOS rules too, and the other way round.

use super::Os;
use crate::hooks::Shortcut;
use crate::shortcuts::{Bindings, KeyCombo};

/// One key: its `KeyboardEvent.code`, its key code on each OS (`None` where
/// that OS can't use it in a shortcut), and how each OS writes it.
pub struct Key {
    pub code: &'static str,
    /// Windows virtual-key code.
    pub windows: Option<u16>,
    /// macOS key code (`kVK_*`, a hardware position).
    pub macos: Option<u16>,
    /// Windows' name: "Page Up".
    pub name: &'static str,
    /// The Mac's glyph: "⇞".
    pub glyph: &'static str,
}

const fn key(
    code: &'static str,
    windows: Option<u16>,
    macos: Option<u16>,
    name: &'static str,
    glyph: &'static str,
) -> Key {
    Key {
        code,
        windows,
        macos,
        name,
        glyph,
    }
}

const fn both(code: &'static str, vk: u16, kvk: u16, name: &'static str) -> Key {
    key(code, Some(vk), Some(kvk), name, name)
}

const fn windows_only(code: &'static str, vk: u16, name: &'static str) -> Key {
    key(code, Some(vk), None, name, name)
}

/// Every key a shortcut can use. Letters, digits, F1 to F12, punctuation,
/// Space, Enter, the arrows and the navigation block; Insert, Pause, Scroll
/// Lock and the volume keys on Windows only (a Mac has none of the first
/// three, and its volume keys aren't key codes).
pub const KEYS: &[Key] = &[
    both("KeyA", 0x41, 0x00, "A"),
    both("KeyB", 0x42, 0x0B, "B"),
    both("KeyC", 0x43, 0x08, "C"),
    both("KeyD", 0x44, 0x02, "D"),
    both("KeyE", 0x45, 0x0E, "E"),
    both("KeyF", 0x46, 0x03, "F"),
    both("KeyG", 0x47, 0x05, "G"),
    both("KeyH", 0x48, 0x04, "H"),
    both("KeyI", 0x49, 0x22, "I"),
    both("KeyJ", 0x4A, 0x26, "J"),
    both("KeyK", 0x4B, 0x28, "K"),
    both("KeyL", 0x4C, 0x25, "L"),
    both("KeyM", 0x4D, 0x2E, "M"),
    both("KeyN", 0x4E, 0x2D, "N"),
    both("KeyO", 0x4F, 0x1F, "O"),
    both("KeyP", 0x50, 0x23, "P"),
    both("KeyQ", 0x51, 0x0C, "Q"),
    both("KeyR", 0x52, 0x0F, "R"),
    both("KeyS", 0x53, 0x01, "S"),
    both("KeyT", 0x54, 0x11, "T"),
    both("KeyU", 0x55, 0x20, "U"),
    both("KeyV", 0x56, 0x09, "V"),
    both("KeyW", 0x57, 0x0D, "W"),
    both("KeyX", 0x58, 0x07, "X"),
    both("KeyY", 0x59, 0x10, "Y"),
    both("KeyZ", 0x5A, 0x06, "Z"),
    both("Digit0", 0x30, 0x1D, "0"),
    both("Digit1", 0x31, 0x12, "1"),
    both("Digit2", 0x32, 0x13, "2"),
    both("Digit3", 0x33, 0x14, "3"),
    both("Digit4", 0x34, 0x15, "4"),
    both("Digit5", 0x35, 0x17, "5"),
    both("Digit6", 0x36, 0x16, "6"),
    both("Digit7", 0x37, 0x1A, "7"),
    both("Digit8", 0x38, 0x1C, "8"),
    both("Digit9", 0x39, 0x19, "9"),
    both("F1", 0x70, 0x7A, "F1"),
    both("F2", 0x71, 0x78, "F2"),
    both("F3", 0x72, 0x63, "F3"),
    both("F4", 0x73, 0x76, "F4"),
    both("F5", 0x74, 0x60, "F5"),
    both("F6", 0x75, 0x61, "F6"),
    both("F7", 0x76, 0x62, "F7"),
    both("F8", 0x77, 0x64, "F8"),
    both("F9", 0x78, 0x65, "F9"),
    both("F10", 0x79, 0x6D, "F10"),
    both("F11", 0x7A, 0x67, "F11"),
    both("F12", 0x7B, 0x6F, "F12"),
    both("Equal", 0xBB, 0x18, "="),
    both("Minus", 0xBD, 0x1B, "-"),
    both("BracketLeft", 0xDB, 0x21, "["),
    both("BracketRight", 0xDD, 0x1E, "]"),
    both("Semicolon", 0xBA, 0x29, ";"),
    both("Quote", 0xDE, 0x27, "'"),
    both("Comma", 0xBC, 0x2B, ","),
    both("Period", 0xBE, 0x2F, "."),
    both("Slash", 0xBF, 0x2C, "/"),
    both("Backslash", 0xDC, 0x2A, "\\"),
    both("Backquote", 0xC0, 0x32, "`"),
    key("Space", Some(0x20), Some(0x31), "Space", "Space"),
    key("Enter", Some(0x0D), Some(0x24), "Enter", "↩"),
    key("ArrowUp", Some(0x26), Some(0x7E), "Up", "↑"),
    key("ArrowDown", Some(0x28), Some(0x7D), "Down", "↓"),
    key("ArrowLeft", Some(0x25), Some(0x7B), "Left", "←"),
    key("ArrowRight", Some(0x27), Some(0x7C), "Right", "→"),
    key("PageUp", Some(0x21), Some(0x74), "Page Up", "⇞"),
    key("PageDown", Some(0x22), Some(0x79), "Page Down", "⇟"),
    key("Home", Some(0x24), Some(0x73), "Home", "↖"),
    key("End", Some(0x23), Some(0x77), "End", "↘"),
    key("Delete", Some(0x2E), Some(0x75), "Delete", "⌦"),
    windows_only("Insert", 0x2D, "Insert"),
    windows_only("Pause", 0x13, "Pause"),
    windows_only("ScrollLock", 0x91, "Scroll Lock"),
    windows_only("AudioVolumeUp", 0xAF, "Volume Up"),
    windows_only("AudioVolumeDown", 0xAE, "Volume Down"),
    windows_only("AudioVolumeMute", 0xAD, "Mute"),
];

pub fn find(code: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.code == code)
}

/// A letter or digit: with only an app's own command modifier (Ctrl on
/// Windows, ⌘ on a Mac) it is some app's shortcut.
fn letter_or_digit(code: &str) -> bool {
    code.starts_with("Key") || code.starts_with("Digit")
}

/// A combination as `os` writes it: `Ctrl+Alt+Page Up`, `⌃⌥⇞`.
pub fn label(os: Os, combo: &KeyCombo) -> String {
    let key = find(&combo.code);
    match os {
        Os::Macos => {
            let mut text = String::new();
            for (on, glyph) in [
                (combo.ctrl, "⌃"),
                (combo.alt, "⌥"),
                (combo.shift, "⇧"),
                (combo.meta, "⌘"),
            ] {
                if on {
                    text.push_str(glyph);
                }
            }
            text.push_str(key.map_or(combo.code.as_str(), |k| k.glyph));
            text
        }
        Os::Windows | Os::Other => {
            let mut parts: Vec<&str> = [
                (combo.ctrl, "Ctrl"),
                (combo.alt, "Alt"),
                (combo.shift, "Shift"),
                (combo.meta, "Win"),
            ]
            .into_iter()
            .filter_map(|(on, name)| on.then_some(name))
            .collect();
            parts.push(key.map_or(combo.code.as_str(), |k| k.name));
            parts.join("+")
        }
    }
}

/// Why `combo` can't be a shortcut on `os`, in one sentence; `None` when it can.
pub fn refusal(os: Os, combo: &KeyCombo) -> Option<String> {
    let Some(key) = find(&combo.code) else {
        return Some("That key can't be part of a shortcut.".to_string());
    };
    let simple = letter_or_digit(&combo.code);
    match os {
        Os::Macos => {
            if key.macos.is_none() {
                return Some(format!("A Mac keyboard has no {} key.", key.name));
            }
            if !(combo.ctrl || combo.meta) {
                return Some(
                    "Add ⌃ or ⌘: macOS doesn't let ⌥ or ⇧ alone start a shortcut.".to_string(),
                );
            }
            if combo.meta && !combo.ctrl && !combo.alt && simple {
                return Some(
                    "⌘ with a letter or digit is every app's own shortcut: add ⌃ or ⌥.".to_string(),
                );
            }
        }
        Os::Windows | Os::Other => {
            if key.windows.is_none() {
                return Some(format!("{} can't be part of a shortcut.", key.name));
            }
            if !(combo.ctrl || combo.alt || combo.meta) {
                return Some("Add Ctrl, Alt or the Windows key.".to_string());
            }
            if combo.ctrl && !combo.alt && !combo.meta && simple {
                return Some(
                    "Ctrl with a letter or digit is every app's own shortcut: add Alt.".to_string(),
                );
            }
            if combo.meta && !combo.ctrl && !combo.alt && simple {
                return Some(
                    "Windows keeps the Windows key with a letter or digit: add Ctrl or Alt."
                        .to_string(),
                );
            }
        }
    }
    None
}

fn combo(code: &str, ctrl: bool, alt: bool) -> KeyCombo {
    KeyCombo {
        code: code.to_string(),
        ctrl,
        alt,
        shift: false,
        meta: false,
    }
}

/// The shortcuts each OS starts with. Windows: Alt plus a key, as every
/// build had (the volume keys too, for speed). macOS: ⌃⌥ plus a key, since
/// ⌥ alone types characters (⌥= is ≠) and Macs have no Home or Page keys.
/// `minify`: the Windows minify key an older build's setting chose (`None`
/// when it was switched off).
pub fn defaults(os: Os, minify: Option<&str>) -> Bindings {
    let alt = |code: &str| combo(code, false, true);
    let ctrl_alt = |code: &str| combo(code, true, true);
    let list: Vec<(Shortcut, Vec<KeyCombo>)> = match os {
        Os::Macos => vec![
            (Shortcut::SpeedUp, vec![ctrl_alt("Equal")]),
            (Shortcut::SpeedDown, vec![ctrl_alt("Minus")]),
            (Shortcut::InclineUp, vec![ctrl_alt("ArrowUp")]),
            (Shortcut::InclineDown, vec![ctrl_alt("ArrowDown")]),
            (Shortcut::ToggleStart, vec![ctrl_alt("Enter")]),
            (Shortcut::ToggleProgram, vec![ctrl_alt("KeyP")]),
            (Shortcut::ToggleMinified, vec![ctrl_alt("KeyM")]),
            (Shortcut::CalibrationTap, vec![ctrl_alt("Space")]),
        ],
        Os::Windows | Os::Other => vec![
            (Shortcut::SpeedUp, vec![alt("Equal"), alt("AudioVolumeUp")]),
            (
                Shortcut::SpeedDown,
                vec![alt("Minus"), alt("AudioVolumeDown")],
            ),
            (Shortcut::InclineUp, vec![alt("PageUp")]),
            (Shortcut::InclineDown, vec![alt("PageDown")]),
            (Shortcut::ToggleStart, vec![alt("Home")]),
            (Shortcut::ToggleProgram, vec![alt("End")]),
            (
                Shortcut::ToggleMinified,
                minify.map(alt).into_iter().collect(),
            ),
            (Shortcut::CalibrationTap, vec![alt("Space")]),
        ],
    };
    list.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Windows: the hook's table
// ---------------------------------------------------------------------------

/// Slots in the hook's table: two keys for each shortcut.
#[cfg(any(windows, test))]
pub const SLOTS: usize = Shortcut::ALL.len() * 2;

#[cfg(any(windows, test))]
const PRESENT: u32 = 1 << 31;
#[cfg(any(windows, test))]
const CTRL: u32 = 1 << 16;
#[cfg(any(windows, test))]
const ALT: u32 = 1 << 17;
#[cfg(any(windows, test))]
const SHIFT: u32 = 1 << 18;
#[cfg(any(windows, test))]
const WIN: u32 = 1 << 19;

/// One key press or binding as the hook compares them: the virtual key and
/// the modifiers held, in a `u32` an atomic can hold.
#[cfg(any(windows, test))]
pub fn windows_code(vk: u16, ctrl: bool, alt: bool, shift: bool, win: bool) -> u32 {
    let mut code = PRESENT | u32::from(vk);
    for (on, bit) in [(ctrl, CTRL), (alt, ALT), (shift, SHIFT), (win, WIN)] {
        if on {
            code |= bit;
        }
    }
    code
}

/// The hook's table: slot `2 * i + n` holds shortcut `i`'s `n`th key, 0 for none.
#[cfg(any(windows, test))]
pub fn windows_table(bindings: &Bindings) -> [u32; SLOTS] {
    let mut table = [0; SLOTS];
    for (i, shortcut) in Shortcut::ALL.iter().enumerate() {
        let keys = bindings.get(shortcut).map(Vec::as_slice).unwrap_or(&[]);
        for (n, combo) in keys.iter().take(2).enumerate() {
            if let Some(vk) = find(&combo.code).and_then(|k| k.windows) {
                table[2 * i + n] = windows_code(vk, combo.ctrl, combo.alt, combo.shift, combo.meta);
            }
        }
    }
    table
}

/// The shortcut a key press (`windows_code`) is in `table`, if any.
#[cfg(any(windows, test))]
pub fn windows_match(table: &[u32], pressed: u32) -> Option<Shortcut> {
    table
        .iter()
        .position(|&slot| slot != 0 && slot == pressed)
        .and_then(|i| Shortcut::ALL.get(i / 2).copied())
}

// ---------------------------------------------------------------------------
// macOS: the hot keys
// ---------------------------------------------------------------------------

/// Carbon hot key modifiers (`Events.h`).
#[cfg(any(target_os = "macos", test))]
const CARBON_CMD: u32 = 1 << 8;
#[cfg(any(target_os = "macos", test))]
const CARBON_SHIFT: u32 = 1 << 9;
#[cfg(any(target_os = "macos", test))]
const CARBON_OPTION: u32 = 1 << 11;
#[cfg(any(target_os = "macos", test))]
const CARBON_CONTROL: u32 = 1 << 12;

/// One Carbon hot key to register.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq)]
pub struct HotKey {
    pub shortcut: Shortcut,
    pub combo: KeyCombo,
    pub key_code: u16,
    pub modifiers: u32,
}

/// The hot keys for `bindings` (keys a Mac has, at most two a shortcut).
#[cfg(any(target_os = "macos", test))]
pub fn macos_hot_keys(bindings: &Bindings) -> Vec<HotKey> {
    Shortcut::ALL
        .iter()
        .flat_map(|shortcut| {
            let keys = bindings.get(shortcut).cloned().unwrap_or_default();
            keys.into_iter().take(2).filter_map(move |combo| {
                let key_code = find(&combo.code)?.macos?;
                let modifiers = [
                    (combo.meta, CARBON_CMD),
                    (combo.shift, CARBON_SHIFT),
                    (combo.alt, CARBON_OPTION),
                    (combo.ctrl, CARBON_CONTROL),
                ]
                .into_iter()
                .filter_map(|(on, bit)| on.then_some(bit))
                .fold(0, |acc, bit| acc | bit);
                Some(HotKey {
                    shortcut: *shortcut,
                    combo,
                    key_code,
                    modifiers,
                })
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(code: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> KeyCombo {
        KeyCombo {
            code: code.to_string(),
            ctrl,
            alt,
            shift,
            meta,
        }
    }

    #[test]
    fn every_key_is_listed_once_with_a_code_for_some_os() {
        for (i, a) in KEYS.iter().enumerate() {
            assert!(a.windows.is_some() || a.macos.is_some(), "{}", a.code);
            for b in &KEYS[i + 1..] {
                assert_ne!(a.code, b.code);
                if a.windows.is_some() && b.windows.is_some() {
                    assert_ne!(a.windows, b.windows, "{} and {}", a.code, b.code);
                }
                if a.macos.is_some() && b.macos.is_some() {
                    assert_ne!(a.macos, b.macos, "{} and {}", a.code, b.code);
                }
            }
        }
    }

    #[test]
    fn labels_follow_each_os() {
        let page_up = c("PageUp", true, true, false, false);
        assert_eq!(label(Os::Windows, &page_up), "Ctrl+Alt+Page Up");
        assert_eq!(label(Os::Macos, &page_up), "⌃⌥⇞");
        let all = c("KeyP", true, true, true, true);
        assert_eq!(label(Os::Windows, &all), "Ctrl+Alt+Shift+Win+P");
        assert_eq!(label(Os::Macos, &all), "⌃⌥⇧⌘P");
        assert_eq!(
            label(Os::Macos, &c("Enter", true, true, false, false)),
            "⌃⌥↩"
        );
    }

    #[test]
    fn a_shortcut_needs_a_modifier_and_a_key_the_os_has() {
        let alt_p = c("KeyP", false, true, false, false);
        assert_eq!(refusal(Os::Windows, &alt_p), None);
        // ⌥ alone: macOS refuses such hot keys.
        assert!(refusal(Os::Macos, &alt_p).unwrap().contains("⌃ or ⌘"));
        assert!(refusal(Os::Windows, &c("KeyP", false, false, true, false)).is_some());
        assert!(refusal(Os::Windows, &c("F5", false, false, false, false)).is_some());
        assert_eq!(
            refusal(Os::Macos, &c("KeyP", true, true, false, false)),
            None
        );
        // A Mac has no Insert key; Windows does.
        let insert = c("Insert", true, true, false, false);
        assert!(refusal(Os::Macos, &insert).unwrap().contains("no Insert"));
        assert_eq!(refusal(Os::Windows, &insert), None);
        assert!(refusal(Os::Windows, &c("Escape", false, true, false, false)).is_some());
    }

    #[test]
    fn an_apps_own_command_with_a_letter_is_refused() {
        assert!(refusal(Os::Windows, &c("KeyC", true, false, false, false)).is_some());
        assert!(refusal(Os::Windows, &c("KeyC", true, false, true, false)).is_some());
        assert!(refusal(Os::Windows, &c("KeyE", false, false, false, true)).is_some());
        assert_eq!(
            refusal(Os::Windows, &c("KeyC", true, true, false, false)),
            None
        );
        // Ctrl with a non-letter is fine.
        assert_eq!(
            refusal(Os::Windows, &c("F5", true, false, false, false)),
            None
        );
        assert!(refusal(Os::Macos, &c("KeyC", false, false, false, true)).is_some());
        assert_eq!(
            refusal(Os::Macos, &c("KeyC", false, true, false, true)),
            None
        );
        assert_eq!(
            refusal(Os::Macos, &c("ArrowUp", false, false, false, true)),
            None
        );
    }

    #[test]
    fn every_default_passes_the_rules_of_its_os() {
        for os in [Os::Windows, Os::Macos] {
            let defaults = defaults(os, Some("Insert"));
            assert_eq!(defaults.len(), Shortcut::ALL.len());
            for (shortcut, keys) in &defaults {
                assert!(!keys.is_empty() && keys.len() <= 2, "{os:?} {shortcut:?}");
                for k in keys {
                    assert_eq!(refusal(os, k), None, "{os:?} {}", label(os, k));
                }
            }
        }
        // An older build's minify setting, carried over or switched off.
        let pause = defaults(Os::Windows, Some("Pause"));
        assert_eq!(
            label(Os::Windows, &pause[&Shortcut::ToggleMinified][0]),
            "Alt+Pause"
        );
        assert!(defaults(Os::Windows, None)[&Shortcut::ToggleMinified].is_empty());
    }

    #[test]
    fn the_windows_table_matches_exact_modifiers_only() {
        let table = windows_table(&defaults(Os::Windows, Some("Insert")));
        let alt = |vk| windows_code(vk, false, true, false, false);
        assert_eq!(
            windows_match(&table, alt(0x24)),
            Some(Shortcut::ToggleStart)
        );
        assert_eq!(windows_match(&table, alt(0xAF)), Some(Shortcut::SpeedUp));
        assert_eq!(windows_match(&table, alt(0xBB)), Some(Shortcut::SpeedUp));
        assert_eq!(
            windows_match(&table, alt(0x2D)),
            Some(Shortcut::ToggleMinified)
        );
        // Home without Alt, or with Ctrl too, is not the shortcut.
        assert_eq!(
            windows_match(&table, windows_code(0x24, false, false, false, false)),
            None
        );
        assert_eq!(
            windows_match(&table, windows_code(0x24, true, true, false, false)),
            None
        );
        assert_eq!(windows_match(&[0; SLOTS], alt(0x24)), None);
    }

    #[test]
    fn mac_hot_keys_carry_carbon_modifiers() {
        let keys = macos_hot_keys(&defaults(Os::Macos, None));
        assert_eq!(keys.len(), Shortcut::ALL.len());
        let start = keys
            .iter()
            .find(|k| k.shortcut == Shortcut::ToggleStart)
            .unwrap();
        assert_eq!(start.key_code, 0x24);
        assert_eq!(start.modifiers, CARBON_CONTROL | CARBON_OPTION);
        // A Windows-only key is left out rather than registered wrong.
        let mut bindings = defaults(Os::Macos, None);
        bindings.insert(
            Shortcut::ToggleMinified,
            vec![c("Insert", true, true, false, false)],
        );
        assert_eq!(macos_hot_keys(&bindings).len(), Shortcut::ALL.len() - 1);
    }
}
