//! macOS key codes for the typing count (Neural flow, typing by speed): which
//! key-downs count as typing. Pure, so it is compiled into every test build
//! (the Windows and Linux CI runs test it too), and into `macos.rs` on macOS.
//! The shortcuts' keys are `keys.rs`.
//!
//! Key codes are the hardware positions AppKit uses (`kVK_*` in `Events.h`),
//! the same on every keyboard layout.

#[cfg(test)]
const KVK_SPACE: u16 = 0x31;
const KVK_ESCAPE: u16 = 0x35;
const KVK_HELP: u16 = 0x72;
const KVK_HOME: u16 = 0x73;
const KVK_PAGE_UP: u16 = 0x74;
const KVK_END: u16 = 0x77;
const KVK_PAGE_DOWN: u16 = 0x79;
const KVK_LEFT: u16 = 0x7B;
const KVK_RIGHT: u16 = 0x7C;
const KVK_DOWN: u16 = 0x7D;
const KVK_UP: u16 = 0x7E;
/// F1 to F20.
const KVK_FUNCTION_KEYS: [u16; 20] = [
    0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F, 0x69, 0x6B, 0x71, 0x6A,
    0x40, 0x4F, 0x50, 0x5A,
];

/// `CGEventFlags` bits for the modifiers that make a key press a shortcut.
pub const FLAG_CONTROL: u64 = 1 << 18;
pub const FLAG_COMMAND: u64 = 1 << 20;

/// A key-down that counts as typing for Neural flow: not Escape, an arrow,
/// Home, End, Page Up or Down, Help or a function key, and not with ⌘ or ⌃
/// held (those make it a shortcut). ⌥ is typing on a Mac: it types accented
/// letters and symbols. Modifier keys alone never arrive as key-downs.
pub fn counts_as_typing(key_code: u16, flags: u64) -> bool {
    let navigation = matches!(
        key_code,
        KVK_ESCAPE
            | KVK_HELP
            | KVK_HOME
            | KVK_END
            | KVK_PAGE_UP
            | KVK_PAGE_DOWN
            | KVK_LEFT
            | KVK_RIGHT
            | KVK_DOWN
            | KVK_UP
    ) || KVK_FUNCTION_KEYS.contains(&key_code);
    let shortcut = flags & (FLAG_CONTROL | FLAG_COMMAND) != 0;
    !(navigation || shortcut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_is_letters_and_symbols_without_command_or_control() {
        let a = 0x00;
        assert!(counts_as_typing(a, 0));
        // ⌥ types characters (⌥e, ⌥=), so it still counts; Shift too.
        assert!(counts_as_typing(a, 1 << 19));
        assert!(counts_as_typing(a, 1 << 17));
        assert!(counts_as_typing(KVK_SPACE, 0));
        assert!(!counts_as_typing(a, FLAG_COMMAND));
        assert!(!counts_as_typing(a, FLAG_CONTROL));
        assert!(!counts_as_typing(KVK_LEFT, 0));
        assert!(!counts_as_typing(KVK_PAGE_DOWN, 0));
        assert!(!counts_as_typing(KVK_ESCAPE, 0));
        assert!(!counts_as_typing(0x7A, 0)); // F1
    }
}
