//! Windows implementation of the platform boundary (see `mod.rs`).
//!
//! The low-level hooks run on every key press and mouse move system-wide, and
//! Windows silently removes a hook that is slow to return. So the hook
//! procedures only read atomics, `try_lock` (never `lock`) and hand anything
//! slower to another thread.

#![warn(unsafe_op_in_unsafe_fn)]

use crate::hooks::Shortcut;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LBUTTON, VK_LMENU, VK_LWIN, VK_RBUTTON, VK_RMENU,
    VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, GetWindowLongPtrW, SetWindowsHookExW,
    TranslateMessage, GWL_EXSTYLE, KBDLLHOOKSTRUCT, LLKHF_ALTDOWN, MSG, MSLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_SYSKEYDOWN, WS_EX_TRANSPARENT,
};

// ---------------------------------------------------------------------------
// Input hooks
// ---------------------------------------------------------------------------

/// Raw HWND of the HUD window, 0 before it exists.
static HUD_HWND: AtomicIsize = AtomicIsize::new(0);

pub fn install_input_hooks() {
    let spawned = std::thread::Builder::new()
        .name("input-hooks".to_string())
        .spawn(|| {
            if let Err(e) = run_input_hooks() {
                log::error!("Input hooks not installed: {}", e);
            }
        });
    if let Err(e) = spawned {
        log::error!("Could not start the input hook thread: {}", e);
    }
}

/// Install both hooks and pump this thread's messages, which low-level hooks
/// need, until the app exits. Returning removes the hooks.
fn run_input_hooks() -> windows::core::Result<()> {
    // SAFETY: `None` asks for this executable's module handle, which lives as
    // long as the process.
    let module = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    // SAFETY: the procedures are `extern "system"` with the signature
    // SetWindowsHookExW expects, and live for the whole program.
    unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module.into()), 0)?;
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(module.into()), 0)?;
    }
    log::info!("Interaction hooks installed persistently.");
    pump_messages();
    Ok(())
}

/// Pump this thread's message queue until the app exits.
fn pump_messages() {
    let mut msg = MSG::default();
    // SAFETY: `msg` is a valid MSG for each call, on the thread that owns the queue.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn remember_hud_window(window: &tauri::WebviewWindow) {
    if let Ok(hwnd) = window.hwnd() {
        HUD_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    }
}

const VK_VOLUME_DOWN: u16 = 0xAE;
const VK_VOLUME_UP: u16 = 0xAF;
const VK_PRIOR: u16 = 0x21;
const VK_NEXT: u16 = 0x22;
const VK_END: u16 = 0x23;
const VK_HOME: u16 = 0x24;
const VK_INSERT: u16 = 0x2D;
const VK_OEM_PLUS: u16 = 0xBB;
const VK_OEM_MINUS: u16 = 0xBD;

/// The Alt shortcut a key press is, if any: Alt with volume up or down (or
/// `=` and `-`) for speed, Page Up and Page Down for incline, Home to start or
/// pause, End to run or end the last program, Insert to minify or expand the HUD.
fn shortcut_for_key(vk: u16, alt_down: bool) -> Option<Shortcut> {
    if !alt_down {
        return None;
    }
    match vk {
        VK_VOLUME_UP | VK_OEM_PLUS => Some(Shortcut::SpeedUp),
        VK_VOLUME_DOWN | VK_OEM_MINUS => Some(Shortcut::SpeedDown),
        VK_PRIOR => Some(Shortcut::InclineUp),
        VK_NEXT => Some(Shortcut::InclineDown),
        VK_HOME => Some(Shortcut::ToggleStart),
        VK_END => Some(Shortcut::ToggleProgram),
        VK_INSERT => Some(Shortcut::ToggleMinified),
        _ => None,
    }
}

/// A key that can count as typing, before the held modifiers are checked:
/// not a modifier, lock, navigation, function or media key, and not pressed
/// with Alt (`sys_key`, a WM_SYSKEYDOWN).
fn is_typing_key(vk: u32, sys_key: bool) -> bool {
    let modifier = (0x10..=0x12).contains(&vk) // Shift, Ctrl, Alt
        || (0xA0..=0xA5).contains(&vk) // left and right Shift, Ctrl, Alt
        || vk == 0x5B // left Win
        || vk == 0x5C // right Win
        || vk == 0x14 // Caps Lock
        || vk == 0x90 // Num Lock
        || vk == 0x91; // Scroll Lock
    let navigation = (0x21..=0x28).contains(&vk) // PgUp, PgDn, End, Home, arrows
        || vk == 0x1B // Esc
        || (0x70..=0x87).contains(&vk) // F1 to F24
        || (0xA6..=0xB7).contains(&vk); // browser, volume and media keys
    !(modifier || navigation || sys_key)
}

fn key_held(key: VIRTUAL_KEY) -> bool {
    // SAFETY: GetAsyncKeyState only reads the key state; any key code is valid.
    unsafe { GetAsyncKeyState(key.0 as i32) < 0 }
}

/// A key press that counts as typing for Neural flow: a typing key with no
/// Ctrl or Win held (those make it a shortcut). Reads the modifier state,
/// never stores the key.
fn counts_as_typing(vk: u32, sys_key: bool) -> bool {
    is_typing_key(vk, sys_key) && !(key_held(VK_CONTROL) || key_held(VK_LWIN) || key_held(VK_RWIN))
}

unsafe extern "system" fn keyboard_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let key_down = wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize;
    if ncode >= 0 && key_down {
        crate::hooks::note_input();
        // SAFETY: for a keyboard event (ncode >= 0) Windows passes a pointer to a
        // KBDLLHOOKSTRUCT that is valid for the duration of this call.
        let key = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
        // The hook flag alone misses Alt in some apps; the key state catches it.
        let alt_down =
            (key.flags.0 & LLKHF_ALTDOWN.0) != 0 || key_held(VK_LMENU) || key_held(VK_RMENU);
        let shortcut = shortcut_for_key(key.vkCode as u16, alt_down);

        if let Some(s) = shortcut {
            crate::hooks::shortcut_pressed(s);
        }
        if crate::hooks::key_counting()
            && shortcut.is_none()
            && counts_as_typing(key.vkCode, wparam.0 == WM_SYSKEYDOWN as usize)
        {
            crate::hooks::count_key();
        }
        // Swallow the shortcut, so Alt+Volume doesn't also change the volume.
        if shortcut.is_some() {
            return LRESULT(1);
        }
    }
    // SAFETY: passes the event on unchanged, as every hook must.
    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ncode >= 0 {
        crate::hooks::note_input();
        // SAFETY: for a mouse event (ncode >= 0) Windows passes a pointer to an
        // MSLLHOOKSTRUCT that is valid for the duration of this call.
        let event = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
        sync_hud_click_through(event.pt.x, event.pt.y);
    }
    // SAFETY: passes the event on unchanged, as every hook must.
    unsafe { CallNextHookEx(None, ncode, wparam, lparam) }
}

/// Make the HUD window click-through or interactive for a pointer at (x, y).
///
/// Click-through is window-wide, so while interactive the whole full-height
/// strip takes clicks, and turning it back must be reliable. Rather than
/// trust the asynchronous toggle (which can lag or be dropped and leave the
/// strip blocking everything behind it), the window's real style bit is read
/// on every move and the toggle sent again only on a mismatch, so a dropped
/// one heals on the next move.
fn sync_hud_click_through(x: i32, y: i32) {
    let Some(want_click_through) = crate::hud::click_through_at(x, y) else {
        return;
    };
    let raw = HUD_HWND.load(Ordering::SeqCst);
    if raw == 0 {
        return;
    }
    // SAFETY: GetWindowLongPtrW only reads. A stale handle (the HUD was
    // destroyed) reads as 0, and the toggle below then finds no HUD window.
    let ex_style = unsafe { GetWindowLongPtrW(HWND(raw as *mut core::ffi::c_void), GWL_EXSTYLE) };
    let is_click_through = (ex_style & WS_EX_TRANSPARENT.0 as isize) != 0;
    if is_click_through != want_click_through {
        crate::hud::set_click_through(want_click_through);
    }
}

// ---------------------------------------------------------------------------
// Pointer
// ---------------------------------------------------------------------------

/// The primary button: the physical right one when the buttons are swapped in
/// Windows settings.
pub fn primary_button_down() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
    // SAFETY: both calls only read system state.
    unsafe {
        let key = if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
            VK_RBUTTON
        } else {
            VK_LBUTTON
        };
        (GetAsyncKeyState(key.0 as i32) as u16 & 0x8000) != 0
    }
}

/// Physical, not logical: right across monitors with different scales.
pub fn cursor_position() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetPhysicalCursorPos;
    let mut p = POINT::default();
    // SAFETY: `p` is a valid out pointer for the call.
    unsafe { GetPhysicalCursorPos(&mut p) }.ok()?;
    Some((p.x, p.y))
}

/// From `GetLastInputInfo`: the whole session's last keyboard or mouse input,
/// not only what the hooks saw. Both tick counts are 32-bit and wrap after
/// 49.7 days, so the difference is taken wrapping.
pub fn input_idle_for() -> Option<std::time::Duration> {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid, sized out struct; GetTickCount only reads.
    let now = unsafe {
        if !GetLastInputInfo(&mut info).as_bool() {
            return None;
        }
        GetTickCount()
    };
    Some(std::time::Duration::from_millis(
        now.wrapping_sub(info.dwTime) as u64,
    ))
}

// ---------------------------------------------------------------------------
// Session lock
// ---------------------------------------------------------------------------
//
// Why WTSRegisterSessionNotification: Windows sends WM_WTSSESSION_CHANGE with
// WTS_SESSION_LOCK / WTS_SESSION_UNLOCK to a registered window the moment the
// session locks or unlocks, whatever locked it (Win+L, Ctrl+Alt+Del, the
// screen saver, sleep, a remote disconnect). It is an event, so nothing polls,
// and it is the documented way for an app to learn about locks. The other
// options are worse: polling OpenInputDesktop or the foreground window guesses
// from side effects, and the low-level hooks see no input either way.
//
// The notification needs a window and a message loop, so the watcher owns a
// hidden window (never shown) on its own thread. A top-level window rather than
// a message-only one, because only that is documented to receive it.

static LOCKED: AtomicBool = AtomicBool::new(false);

pub fn session_locked() -> bool {
    LOCKED.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Display power
// ---------------------------------------------------------------------------
//
// Why GUID_CONSOLE_DISPLAY_STATE: Windows sends WM_POWERBROADCAST /
// PBT_POWERSETTINGCHANGE to a registered window whenever the console display
// turns off, on or dims, whatever turned it off (the power plan's "turn off
// the display" timeout, the lock screen's own timeout, a monitor-off tool), and
// once right after registering with the current state. It is an event, so
// nothing polls, and it is the real display state rather than a guess from the
// idle time. The session watcher's hidden window already has a message loop,
// so it registers for this too.

/// GUID_CONSOLE_DISPLAY_STATE (winnt.h), defined here so the crate needs no
/// `Win32_System_SystemServices` feature for one constant.
const GUID_CONSOLE_DISPLAY_STATE: windows::core::GUID =
    windows::core::GUID::from_u128(0x6fe69556_704a_47a0_8f24_c28d936fda47);

static DISPLAY_OFF: AtomicBool = AtomicBool::new(false);

pub fn display_off() -> bool {
    DISPLAY_OFF.load(Ordering::Relaxed)
}

/// The setting's data: 0 off, 1 on, 2 dimmed. Dimmed still lights the room,
/// and is on its way back on at the next input, so only 0 is off.
fn display_state_is_off(data: u8) -> bool {
    data == 0
}

/// One PBT_POWERSETTINGCHANGE: record the display state if it is ours.
///
/// # Safety
/// `lparam` must be the `POWERBROADCAST_SETTING` pointer of that message.
unsafe fn on_power_setting(lparam: LPARAM) {
    use windows::Win32::System::Power::POWERBROADCAST_SETTING;
    let ptr = lparam.0 as *const POWERBROADCAST_SETTING;
    if ptr.is_null() {
        return;
    }
    // SAFETY: Windows passes a valid POWERBROADCAST_SETTING for the length of
    // the message; `Data` holds at least `DataLength` bytes (4 for this GUID).
    let setting = unsafe { &*ptr };
    if setting.PowerSetting != GUID_CONSOLE_DISPLAY_STATE || setting.DataLength < 1 {
        return;
    }
    let off = display_state_is_off(setting.Data[0]);
    if DISPLAY_OFF.swap(off, Ordering::Relaxed) != off {
        log::info!("Display {}", if off { "off" } else { "on" });
    }
}

pub fn spawn_session_lock_watcher() {
    let spawned = std::thread::Builder::new()
        .name("session-lock".to_string())
        .spawn(|| {
            if let Err(e) = run_session_watch() {
                log::warn!("Walk-away: could not watch for the PC locking: {}", e);
            }
        });
    if let Err(e) = spawned {
        log::warn!("Walk-away: could not start the lock watcher: {}", e);
    }
}

unsafe extern "system" fn session_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, PBT_POWERSETTINGCHANGE, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE,
        WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
    };
    if msg == WM_POWERBROADCAST && wparam.0 as u32 == PBT_POWERSETTINGCHANGE {
        // SAFETY: this is that message, so `lparam` is its setting.
        unsafe { on_power_setting(lparam) };
        // TRUE: handled.
        return LRESULT(1);
    }
    if msg == WM_WTSSESSION_CHANGE {
        match wparam.0 as u32 {
            WTS_SESSION_LOCK => {
                LOCKED.store(true, Ordering::Relaxed);
                log::info!("Walk-away: PC locked");
            }
            WTS_SESSION_UNLOCK => {
                LOCKED.store(false, Ordering::Relaxed);
                log::info!("Walk-away: PC unlocked");
            }
            _ => {}
        }
        return LRESULT(0);
    }
    // SAFETY: the default handling for every other message of our own window.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn run_session_watch() -> windows::core::Result<()> {
    use windows::core::w;
    use windows::Win32::System::RemoteDesktop::{
        WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, WINDOW_STYLE, WNDCLASSW, WS_EX_TOOLWINDOW,
    };

    // SAFETY: as in `run_input_hooks`.
    let instance = unsafe { GetModuleHandleW(None) }?;
    let class = w!("NeonStriderSessionWatch");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(session_wnd_proc),
        hInstance: instance.into(),
        lpszClassName: class,
        ..Default::default()
    };
    // SAFETY: `wc` is fully initialised and its strings are static.
    if unsafe { RegisterClassW(&wc) } == 0 {
        return Err(windows::core::Error::from_win32());
    }
    // SAFETY: the class was just registered; the window is never shown, and
    // lives (with its thread) as long as the app.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            w!("Neon Strider session watch"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }?;
    // SAFETY: `hwnd` is the window just created on this thread.
    unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) }?;
    log::info!("Walk-away: watching for the PC locking");
    // The display state next. A failure here leaves the lock watch running.
    // SAFETY: `hwnd` is our window on this thread; the GUID is a static. The
    // registration lives as long as the window (the app), so it is never
    // unregistered.
    let display = unsafe {
        windows::Win32::System::Power::RegisterPowerSettingNotification(
            windows::Win32::Foundation::HANDLE(hwnd.0),
            &GUID_CONSOLE_DISPLAY_STATE,
            windows::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_WINDOW_HANDLE,
        )
    };
    match display {
        Ok(_) => log::info!("Lights out: watching the display power state"),
        Err(e) => log::warn!("Lights out: could not watch the display power state: {}", e),
    }
    pump_messages();
    Ok(())
}

// ---------------------------------------------------------------------------
// Microphone
// ---------------------------------------------------------------------------
//
// Windows keeps, per app, when it last started and stopped using the
// microphone (the data behind the microphone icon in the taskbar). Reading it
// needs no permission and no audio access.

pub fn apps_using_microphone(own_exe: Option<&str>) -> Vec<String> {
    let entries = mic::read_entries();
    mic::apps_in_use(&entries, own_exe, mic::now_filetime())
        .iter()
        .map(|e| mic::short_name(&e.app).to_string())
        .collect()
}

mod mic {
    use windows::core::{w, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_READ, REG_QWORD, REG_VALUE_TYPE,
    };

    const CONSENT_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
    /// Desktop (unpackaged) apps are listed under this subkey, named by their
    /// exe path with `\` written as `#`. Store apps are listed directly.
    const DESKTOP_APPS: &str = "NonPackaged";
    /// FILETIME ticks (100 ns) in 6 hours.
    const STALE_AFTER_TICKS: u64 = 6 * 3600 * 10_000_000;
    /// Milliseconds from the FILETIME epoch (1601) to the Unix epoch.
    const FILETIME_UNIX_OFFSET_MS: u64 = 11_644_473_600_000;

    /// One app's entry in the consent store.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Entry {
        /// The key name: a package name, or an exe path with `#` for `\`.
        pub app: String,
        /// `LastUsedTimeStart`, a FILETIME; 0 if never.
        pub start: u64,
        /// `LastUsedTimeStop`, a FILETIME; 0 while still in use.
        pub stop: u64,
    }

    /// An app is using the microphone when it has started and not yet stopped.
    pub fn entry_in_use(start: u64, stop: u64) -> bool {
        start != 0 && stop == 0
    }

    /// A "still in use" entry that started more than 6 hours ago is almost
    /// certainly an app that crashed mid-call and never recorded a stop.
    pub fn entry_fresh(start: u64, now_filetime: u64) -> bool {
        now_filetime.saturating_sub(start) < STALE_AFTER_TICKS
    }

    /// Now as a FILETIME, the unit the consent store uses.
    pub fn now_filetime() -> u64 {
        let unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        (unix_ms + FILETIME_UNIX_OFFSET_MS) * 10_000
    }

    /// The consent-store key name for an exe path.
    pub fn exe_key(path: &str) -> String {
        path.replace(['\\', '/'], "#")
    }

    /// The part of a key worth logging: the exe file name or package name, never
    /// the folders (which hold the user name).
    pub fn short_name(app: &str) -> &str {
        app.rsplit('#').next().unwrap_or(app)
    }

    /// Entries using the microphone, ignoring our own process (`own_exe` is our
    /// exe path).
    pub fn apps_in_use<'a>(
        entries: &'a [Entry],
        own_exe: Option<&str>,
        now_filetime: u64,
    ) -> Vec<&'a Entry> {
        let own = own_exe.map(exe_key);
        entries
            .iter()
            .filter(|e| entry_in_use(e.start, e.stop) && entry_fresh(e.start, now_filetime))
            .filter(|e| {
                own.as_deref()
                    .is_none_or(|o| !e.app.eq_ignore_ascii_case(o))
            })
            .collect()
    }

    /// An open registry key, closed on drop.
    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful RegOpenKeyExW and is closed once.
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn open(parent: HKEY, sub: &str) -> Option<Key> {
        let name = wide(sub);
        let mut handle = HKEY::default();
        // SAFETY: `name` is NUL-terminated and outlives the call; `handle` is a valid out pointer.
        let rc =
            unsafe { RegOpenKeyExW(parent, PCWSTR(name.as_ptr()), None, KEY_READ, &mut handle) };
        (rc == ERROR_SUCCESS).then_some(Key(handle))
    }

    fn subkeys(key: &Key) -> Vec<String> {
        let mut out = Vec::new();
        let mut index = 0u32;
        loop {
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            // SAFETY: `buf` holds `len` UTF-16 units; the optional out parameters are unused.
            let rc = unsafe {
                RegEnumKeyExW(
                    key.0,
                    index,
                    Some(PWSTR(buf.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            index += 1;
            if rc == ERROR_SUCCESS {
                out.push(String::from_utf16_lossy(&buf[..len as usize]));
            } else if rc != ERROR_MORE_DATA {
                // ERROR_NO_MORE_ITEMS, or something wrong with the key: stop.
                // (A name over 511 units is not an app key; skipped.)
                break;
            }
        }
        out
    }

    fn qword(key: &Key, name: PCWSTR) -> u64 {
        let mut kind = REG_VALUE_TYPE(0);
        let mut data = [0u8; 8];
        let mut size = data.len() as u32;
        // SAFETY: `data` holds `size` bytes; `kind` and `size` are valid out pointers.
        let rc = unsafe {
            RegQueryValueExW(
                key.0,
                name,
                None,
                Some(&mut kind),
                Some(data.as_mut_ptr()),
                Some(&mut size),
            )
        };
        if rc == ERROR_SUCCESS && kind == REG_QWORD && size == 8 {
            u64::from_le_bytes(data)
        } else {
            0
        }
    }

    fn read_app(parent: &Key, name: &str) -> Option<Entry> {
        let app = open(parent.0, name)?;
        Some(Entry {
            app: name.to_string(),
            start: qword(&app, w!("LastUsedTimeStart")),
            stop: qword(&app, w!("LastUsedTimeStop")),
        })
    }

    /// Every app in the consent store. Empty when the key is missing (the
    /// microphone was never used) or unreadable.
    pub fn read_entries() -> Vec<Entry> {
        let Some(root) = open(HKEY_CURRENT_USER, CONSENT_KEY) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for name in subkeys(&root) {
            if name == DESKTOP_APPS {
                if let Some(desktop) = open(root.0, DESKTOP_APPS) {
                    out.extend(
                        subkeys(&desktop)
                            .iter()
                            .filter_map(|n| read_app(&desktop, n)),
                    );
                }
            } else if let Some(entry) = read_app(&root, &name) {
                out.push(entry);
            }
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn entry(app: &str, start: u64, stop: u64) -> Entry {
            Entry {
                app: app.to_string(),
                start,
                stop,
            }
        }

        #[test]
        fn in_use_means_started_and_not_stopped() {
            assert!(entry_in_use(133_000_000_000_000_000, 0));
            assert!(!entry_in_use(
                133_000_000_000_000_000,
                133_000_000_100_000_000
            ));
            // Never used: both zero.
            assert!(!entry_in_use(0, 0));
            // No start but a stop is nonsense, and not "in use".
            assert!(!entry_in_use(0, 5));
        }

        #[test]
        fn exe_paths_map_to_key_names() {
            assert_eq!(
                exe_key(r"C:\Program Files\Zoom\bin\Zoom.exe"),
                "C:#Program Files#Zoom#bin#Zoom.exe"
            );
            assert_eq!(exe_key("C:/a/b.exe"), "C:#a#b.exe");
        }

        #[test]
        fn log_names_drop_the_folders() {
            assert_eq!(
                short_name("C:#Users#someone#AppData#Teams#Teams.exe"),
                "Teams.exe"
            );
            assert_eq!(
                short_name("Microsoft.WindowsCamera_8wekyb3d8bbwe"),
                "Microsoft.WindowsCamera_8wekyb3d8bbwe"
            );
        }

        #[test]
        fn any_app_in_use_counts_but_not_our_own() {
            let own = r"C:\Users\me\AppData\Local\Neon Strider\neon-strider.exe";
            let entries = vec![
                entry("C:#Program Files#Zoom#Zoom.exe", 100, 200),
                entry(
                    "C:#Users#me#AppData#Local#Neon Strider#neon-strider.exe",
                    100,
                    0,
                ),
            ];
            // Only we are "using" it: not a call.
            assert!(apps_in_use(&entries, Some(own), 400).is_empty());
            // Case differences in the path do not matter.
            let shouting = vec![entry(
                "C:#USERS#ME#APPDATA#LOCAL#NEON STRIDER#NEON-STRIDER.EXE",
                100,
                0,
            )];
            assert!(apps_in_use(&shouting, Some(own), 400).is_empty());
            // Another app starts: a call.
            let mut with_call = entries.clone();
            with_call.push(entry("Microsoft.Teams_8wekyb3d8bbwe", 300, 0));
            let using = apps_in_use(&with_call, Some(own), 400);
            assert_eq!(using.len(), 1);
            assert_eq!(using[0].app, "Microsoft.Teams_8wekyb3d8bbwe");
            // Without knowing our path nothing is filtered.
            assert_eq!(apps_in_use(&with_call, None, 400).len(), 2);
            assert!(apps_in_use(&[], Some(own), 400).is_empty());
        }

        #[test]
        fn a_stop_never_recorded_hours_ago_is_not_a_call() {
            let hour = 3600 * 10_000_000u64;
            let now = 100 * hour;
            let crashed = vec![entry("C:#Apps#Crashy.exe", now - 7 * hour, 0)];
            assert!(apps_in_use(&crashed, None, now).is_empty());
            let live = vec![entry("C:#Apps#Zoom.exe", now - hour, 0)];
            assert_eq!(apps_in_use(&live, None, now).len(), 1);
        }

        /// Reads the real consent store (read-only) to prove the registry calls
        /// work. What it finds depends on the machine, so nothing is asserted
        /// about the count.
        #[test]
        fn reading_the_real_consent_store_does_not_fail() {
            let entries = read_entries();
            assert!(entries.iter().all(|e| !e.app.is_empty()));
            eprintln!(
                "consent store: {} apps, {} in use",
                entries.len(),
                entries
                    .iter()
                    .filter(|e| entry_in_use(e.start, e.stop))
                    .count()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Bluetooth peripheral (GHOST_POD)
// ---------------------------------------------------------------------------
//
// A WinRT GATT server (`GattServiceProvider`). It needs an adapter with the
// peripheral role; most can be a central (the treadmill, through btleplug)
// and a peripheral at once. Windows advertises with the PC's own Bluetooth
// name. Each service has a provider of its own, so switching one on or off
// never drops a watch connected to another. Event handlers run on WinRT
// threads and only call `events` and log.

pub use peripheral::{peripheral_add, peripheral_notify, peripheral_remove, peripheral_stop};

mod peripheral {
    use super::super::{
        uuid_text, PeripheralBeacon, PeripheralCharacteristic, PeripheralEvent, PeripheralEvents,
        PeripheralService,
    };
    use std::collections::HashMap;
    use std::sync::{Mutex, MutexGuard, PoisonError};
    use windows::core::{IInspectable, Ref, GUID};
    use windows::Devices::Bluetooth::Advertisement::{
        BluetoothLEAdvertisement, BluetoothLEAdvertisementPublisher,
        BluetoothLEAdvertisementPublisherStatus,
        BluetoothLEAdvertisementPublisherStatusChangedEventArgs, BluetoothLEManufacturerData,
    };
    use windows::Devices::Bluetooth::GenericAttributeProfile::{
        GattCharacteristicProperties, GattLocalCharacteristic, GattLocalCharacteristicParameters,
        GattLocalService, GattProtectionLevel, GattReadRequestedEventArgs, GattServiceProvider,
        GattServiceProviderAdvertisementStatus,
        GattServiceProviderAdvertisementStatusChangedEventArgs,
        GattServiceProviderAdvertisingParameters, GattSession, GattSessionStatus,
        GattSessionStatusChangedEventArgs, GattWriteOption, GattWriteRequestedEventArgs,
    };
    use windows::Devices::Bluetooth::{BluetoothAdapter, BluetoothError};
    use windows::Foundation::TypedEventHandler;
    use windows::Storage::Streams::{DataReader, DataWriter, IBuffer};

    /// One service, on a `GattServiceProvider` of its own, so it can come and
    /// go without touching the others.
    struct Provided {
        service: u128,
        name: &'static str,
        provider: GattServiceProvider,
        chars: Vec<(u128, GattLocalCharacteristic)>,
        /// The service's legacy beacon, while it runs.
        beacon: Option<BluetoothLEAdvertisementPublisher>,
    }

    static SERVER: Mutex<Vec<Provided>> = Mutex::new(Vec::new());
    /// The latest value of each readable characteristic, which reads answer
    /// with (`peripheral_notify` updates it).
    static VALUES: Mutex<Option<HashMap<u128, Vec<u8>>>> = Mutex::new(None);
    /// Each central seen (by its address), with its session. Windows has no
    /// "a device connected" event for a GATT server, so a central shows up at
    /// its first read, write or subscription; its session then says when it
    /// disconnects.
    static CENTRALS: Mutex<Option<HashMap<String, GattSession>>> = Mutex::new(None);

    fn values() -> MutexGuard<'static, Option<HashMap<u128, Vec<u8>>>> {
        VALUES.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn server() -> MutexGuard<'static, Vec<Provided>> {
        SERVER.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn centrals() -> MutexGuard<'static, Option<HashMap<String, GattSession>>> {
        CENTRALS.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn win(e: windows::core::Error) -> String {
        format!("Windows Bluetooth error: {}", e.message())
    }

    /// Plain English for a WinRT Bluetooth error.
    fn bt_error(e: BluetoothError) -> String {
        match e {
            BluetoothError::Success => "Windows stopped the advertisement.".to_string(),
            BluetoothError::RadioNotAvailable => {
                "Bluetooth is off or the adapter is missing.".to_string()
            }
            BluetoothError::ResourceInUse => {
                "Another app is already serving a foot pod on this PC.".to_string()
            }
            BluetoothError::DisabledByPolicy => {
                "A Windows policy turns Bluetooth sensors off on this PC.".to_string()
            }
            BluetoothError::DisabledByUser => "Bluetooth is turned off in Windows.".to_string(),
            BluetoothError::NotSupported | BluetoothError::TransportNotSupported => {
                "This PC's Bluetooth adapter can't act as a sensor.".to_string()
            }
            BluetoothError::ConsentRequired => {
                "Windows needs permission to use Bluetooth.".to_string()
            }
            other => format!("Bluetooth refused the pod (error {}).", other.0),
        }
    }

    /// The name of a `BluetoothError`, for the log.
    fn bt_error_name(e: BluetoothError) -> String {
        let name = match e {
            BluetoothError::Success => "Success",
            BluetoothError::RadioNotAvailable => "RadioNotAvailable",
            BluetoothError::ResourceInUse => "ResourceInUse",
            BluetoothError::DeviceNotConnected => "DeviceNotConnected",
            BluetoothError::OtherError => "OtherError",
            BluetoothError::DisabledByPolicy => "DisabledByPolicy",
            BluetoothError::NotSupported => "NotSupported",
            BluetoothError::DisabledByUser => "DisabledByUser",
            BluetoothError::ConsentRequired => "ConsentRequired",
            BluetoothError::TransportNotSupported => "TransportNotSupported",
            other => return format!("error {}", other.0),
        };
        name.to_string()
    }

    /// `GattServiceProviderAdvertisementStatus` in words.
    fn status_name(s: GattServiceProviderAdvertisementStatus) -> String {
        let name = match s {
            GattServiceProviderAdvertisementStatus::Created => "Created",
            GattServiceProviderAdvertisementStatus::Stopped => "Stopped",
            GattServiceProviderAdvertisementStatus::Started => "Started",
            GattServiceProviderAdvertisementStatus::Aborted => "Aborted",
            GattServiceProviderAdvertisementStatus::StartedWithoutAllAdvertisementData => {
                "Started without all advertisement data"
            }
            other => return format!("status {}", other.0),
        };
        name.to_string()
    }

    fn buffer(bytes: &[u8]) -> windows::core::Result<IBuffer> {
        let writer = DataWriter::new()?;
        writer.WriteBytes(bytes)?;
        writer.DetachBuffer()
    }

    fn bytes_of(buf: &IBuffer) -> windows::core::Result<Vec<u8>> {
        let reader = DataReader::FromBuffer(buf)?;
        let mut out = vec![0u8; reader.UnconsumedBufferLength()? as usize];
        reader.ReadBytes(&mut out)?;
        Ok(out)
    }

    /// The central's address from its device ID
    /// (`BluetoothLE#BluetoothLE<adapter>-<central>`), or the whole ID.
    fn address_of(session: &GattSession) -> String {
        let id = session
            .DeviceId()
            .and_then(|d| d.Id())
            .map(|h| h.to_string())
            .unwrap_or_default();
        id.rsplit('-').next().unwrap_or(&id).to_string()
    }

    /// Log a central the first time it shows up, and watch its session so
    /// its disconnect is logged too. `what` is what it did ("read of 2a54 on
    /// RSC").
    fn note_central(session: &GattSession, what: &str) {
        let address = address_of(session);
        let mut guard = centrals();
        let map = guard.get_or_insert_with(HashMap::new);
        if map.contains_key(&address) {
            return;
        }
        log::info!(
            "GHOST_POD: central {} connected (first seen: {}; max PDU {} bytes)",
            address,
            what,
            session.MaxPduSize().unwrap_or(0)
        );
        let addr = address.clone();
        let watched = session.SessionStatusChanged(&TypedEventHandler::new(
            move |_: Ref<GattSession>, args: Ref<GattSessionStatusChangedEventArgs>| {
                let args = args.ok()?;
                if args.Status()? == GattSessionStatus::Closed {
                    log::info!(
                        "GHOST_POD: central {} disconnected ({})",
                        addr,
                        bt_error_name(args.Error()?)
                    );
                    if let Some(map) = centrals().as_mut() {
                        map.remove(&addr);
                    }
                }
                Ok(())
            },
        ));
        if let Err(e) = watched {
            log::warn!(
                "GHOST_POD: can't watch central {}: {}",
                address,
                e.message()
            );
        }
        map.insert(address, session.clone());
    }

    /// What the adapter can advertise, for the log: a second advertisement
    /// next to the foot pod's depends on it.
    pub(super) fn adapter_caps(adapter: &BluetoothAdapter) -> String {
        let yes = |r: windows::core::Result<bool>| match r {
            Ok(true) => "yes",
            Ok(false) => "no",
            Err(_) => "unknown",
        };
        format!(
            "extended advertising {}, advertisement offload {}, max advertisement data {} bytes",
            yes(adapter.IsExtendedAdvertisingSupported()),
            yes(adapter.IsAdvertisementOffloadSupported()),
            adapter
                .MaxAdvertisementDataLength()
                .map_or("unknown".to_string(), |n| n.to_string())
        )
    }

    pub fn peripheral_add(
        service: &PeripheralService,
        events: PeripheralEvents,
    ) -> Result<(), String> {
        peripheral_remove(service.uuid);
        let adapter = BluetoothAdapter::GetDefaultAsync()
            .and_then(|op| op.get())
            .map_err(|_| "No Bluetooth adapter found.".to_string())?;
        if !adapter.IsPeripheralRoleSupported().map_err(win)? {
            return Err(
                "This PC's Bluetooth adapter can't act as a sensor (no peripheral role)."
                    .to_string(),
            );
        }
        if server().is_empty() {
            log::info!("GHOST_POD: adapter: {}", adapter_caps(&adapter));
        }
        values().get_or_insert_with(HashMap::new);
        let mut chars = Vec::new();
        match serve(service, &events, &mut chars) {
            Ok(provider) => {
                // The beacon is extra: if Windows refuses it, the service
                // still serves the devices that find it the usual way.
                let beacon = service.beacon.as_ref().and_then(|b| {
                    start_beacon(service.name, b)
                        .map_err(|why| {
                            log::warn!("GHOST_POD: {} beacon not started: {}", service.name, why)
                        })
                        .ok()
                });
                server().push(Provided {
                    service: service.uuid,
                    name: service.name,
                    provider,
                    chars,
                    beacon,
                });
                Ok(())
            }
            Err(why) => {
                forget_values(&chars);
                Err(why)
            }
        }
    }

    fn forget_values(chars: &[(u128, GattLocalCharacteristic)]) {
        if let Some(map) = values().as_mut() {
            for (uuid, _) in chars {
                map.remove(uuid);
            }
        }
    }

    /// Create, fill and advertise one service. `chars` gets each
    /// characteristic as it is made, so the caller can undo a half start.
    fn serve(
        service: &PeripheralService,
        events: &PeripheralEvents,
        chars: &mut Vec<(u128, GattLocalCharacteristic)>,
    ) -> Result<GattServiceProvider, String> {
        let (name, uuid) = (service.name, service.uuid);
        let created = GattServiceProvider::CreateAsync(GUID::from_u128(uuid))
            .and_then(|op| op.get())
            .map_err(win)?;
        let err = created.Error().map_err(win)?;
        if err != BluetoothError::Success {
            log::warn!(
                "GHOST_POD: {} ({}) not created: {}",
                name,
                uuid_text(uuid),
                bt_error_name(err)
            );
            return Err(bt_error(err));
        }
        let provider = created.ServiceProvider().map_err(win)?;
        let local = provider.Service().map_err(win)?;
        for c in &service.characteristics {
            let ch = add_characteristic(name, &local, c, events)?;
            chars.push((c.uuid, ch));
        }

        let on_status = events.clone();
        provider
            .AdvertisementStatusChanged(&TypedEventHandler::new(
                move |_: Ref<GattServiceProvider>,
                      args: Ref<GattServiceProviderAdvertisementStatusChangedEventArgs>| {
                    let args = args.ok()?;
                    let status = args.Status()?;
                    let error = args.Error()?;
                    log::info!(
                        "GHOST_POD: {} ({}) advertisement {} ({})",
                        name,
                        uuid_text(uuid),
                        status_name(status),
                        bt_error_name(error)
                    );
                    if status == GattServiceProviderAdvertisementStatus::Started
                        || status
                            == GattServiceProviderAdvertisementStatus::StartedWithoutAllAdvertisementData
                    {
                        on_status(PeripheralEvent::Advertising { service: uuid });
                    } else if status == GattServiceProviderAdvertisementStatus::Aborted {
                        on_status(PeripheralEvent::Failed {
                            service: uuid,
                            why: bt_error(error),
                        });
                    }
                    Ok(())
                },
            ))
            .map_err(win)?;

        // Discoverable always: Microsoft's BluetoothLE sample says it decides
        // whether a remote device can query the PC for the service. Windows
        // adds the service's UUID to the advertisement only when the service
        // is connectable too (Microsoft's GATT server guide), so a service
        // that stays out of the advertisement is discoverable but not
        // connectable; a device reaches it over the connection it made through
        // another service's advertisement.
        let params = GattServiceProviderAdvertisingParameters::new().map_err(win)?;
        params.SetIsDiscoverable(true).map_err(win)?;
        params.SetIsConnectable(service.advertise).map_err(win)?;
        log::info!(
            "GHOST_POD: {} ({}) starting: {}",
            name,
            uuid_text(uuid),
            if service.advertise {
                "connectable, UUID in the advertisement"
            } else {
                "discoverable only, UUID left out of the advertisement"
            }
        );
        provider
            .StartAdvertisingWithParameters(&params)
            .map_err(win)?;
        // A service left out of the advertisement may never fire the status
        // event, so its state right after the start goes in the log too.
        if let Ok(now) = provider.AdvertisementStatus() {
            log::info!(
                "GHOST_POD: {} ({}) status after start: {}",
                name,
                uuid_text(uuid),
                status_name(now)
            );
        }
        Ok(provider)
    }

    /// `BluetoothLEAdvertisementPublisherStatus` in words.
    fn publisher_status_name(s: BluetoothLEAdvertisementPublisherStatus) -> String {
        let name = match s {
            BluetoothLEAdvertisementPublisherStatus::Created => "Created",
            BluetoothLEAdvertisementPublisherStatus::Waiting => "Waiting",
            BluetoothLEAdvertisementPublisherStatus::Started => "Started",
            BluetoothLEAdvertisementPublisherStatus::Stopping => "Stopping",
            BluetoothLEAdvertisementPublisherStatus::Stopped => "Stopped",
            BluetoothLEAdvertisementPublisherStatus::Aborted => "Aborted",
            other => return format!("status {}", other.0),
        };
        name.to_string()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Start a legacy advertisement with `beacon`'s manufacturer data, next
    /// to the service's own. A GATT server's advertisement can't be made
    /// legacy (its parameters have no such setting), and on an adapter with
    /// extended advertising a scanner that hears only legacy ones may miss it;
    /// this one is legacy for certain. Windows chooses the address and whether
    /// it is connectable (the publisher has no setting for either); it is not
    /// anonymous, so the PC's address is in it.
    fn start_beacon(
        name: &'static str,
        beacon: &PeripheralBeacon,
    ) -> Result<BluetoothLEAdvertisementPublisher, String> {
        let advertisement = BluetoothLEAdvertisement::new().map_err(win)?;
        let section = BluetoothLEManufacturerData::Create(
            beacon.company,
            &buffer(&beacon.data).map_err(win)?,
        )
        .map_err(win)?;
        advertisement
            .ManufacturerData()
            .map_err(win)?
            .Append(&section)
            .map_err(win)?;
        let publisher = BluetoothLEAdvertisementPublisher::Create(&advertisement).map_err(win)?;
        publisher.SetUseExtendedAdvertisement(false).map_err(win)?;
        publisher
            .StatusChanged(&TypedEventHandler::new(
                move |_: Ref<BluetoothLEAdvertisementPublisher>,
                      args: Ref<BluetoothLEAdvertisementPublisherStatusChangedEventArgs>| {
                    let args = args.ok()?;
                    log::info!(
                        "GHOST_POD: {} beacon {} ({})",
                        name,
                        publisher_status_name(args.Status()?),
                        bt_error_name(args.Error()?)
                    );
                    Ok(())
                },
            ))
            .map_err(win)?;
        log::info!(
            "GHOST_POD: {} beacon starting: legacy advertisement, PC's address, manufacturer data {:04x} {} ({} of 31 bytes with the flags)",
            name,
            beacon.company,
            hex(&beacon.data),
            beacon.air_len()
        );
        publisher.Start().map_err(win)?;
        Ok(publisher)
    }

    fn add_characteristic(
        service: &'static str,
        local: &GattLocalService,
        c: &PeripheralCharacteristic,
        events: &PeripheralEvents,
    ) -> Result<GattLocalCharacteristic, String> {
        let params = GattLocalCharacteristicParameters::new().map_err(win)?;
        let mut props = GattCharacteristicProperties::None;
        if c.read.is_some() {
            props |= GattCharacteristicProperties::Read;
        }
        if c.notify {
            props |= GattCharacteristicProperties::Notify;
        }
        if c.indicate {
            props |= GattCharacteristicProperties::Indicate;
        }
        if c.write {
            props |= GattCharacteristicProperties::Write;
        }
        params.SetCharacteristicProperties(props).map_err(win)?;
        params
            .SetReadProtectionLevel(GattProtectionLevel::Plain)
            .map_err(win)?;
        if c.write {
            params
                .SetWriteProtectionLevel(GattProtectionLevel::Plain)
                .map_err(win)?;
        }
        let made = local
            .CreateCharacteristicAsync(GUID::from_u128(c.uuid), &params)
            .and_then(|op| op.get())
            .map_err(win)?;
        let err = made.Error().map_err(win)?;
        if err != BluetoothError::Success {
            return Err(bt_error(err));
        }
        let ch = made.Characteristic().map_err(win)?;
        let uuid = c.uuid;

        if let Some(v) = &c.read {
            if let Some(map) = values().as_mut() {
                map.insert(uuid, v.clone());
            }
            ch.ReadRequested(&TypedEventHandler::new(
                move |_: Ref<GattLocalCharacteristic>, args: Ref<GattReadRequestedEventArgs>| {
                    let args = args.ok()?;
                    let what = format!("read of {} on {}", uuid_text(uuid), service);
                    if let Ok(session) = args.Session() {
                        note_central(&session, &what);
                    }
                    let deferral = args.GetDeferral()?;
                    let request = args.GetRequestAsync()?.get()?;
                    let value = values()
                        .as_ref()
                        .and_then(|map| map.get(&uuid).cloned())
                        .unwrap_or_default();
                    log::info!("GHOST_POD: {}", what);
                    request.RespondWithValue(&buffer(&value)?)?;
                    deferral.Complete()?;
                    Ok(())
                },
            ))
            .map_err(win)?;
        }
        if c.notify || c.indicate {
            let ev = events.clone();
            ch.SubscribedClientsChanged(&TypedEventHandler::new(
                move |sender: Ref<GattLocalCharacteristic>, _: Ref<IInspectable>| {
                    let clients = sender.ok()?.SubscribedClients()?;
                    let count = clients.Size()?;
                    let what = format!("subscribed to {} on {}", uuid_text(uuid), service);
                    let mut who = Vec::new();
                    for client in clients {
                        if let Ok(session) = client.Session() {
                            note_central(&session, &what);
                            who.push(address_of(&session));
                        }
                    }
                    log::info!(
                        "GHOST_POD: {} {}{}",
                        count,
                        what,
                        if who.is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", who.join(", "))
                        }
                    );
                    ev(PeripheralEvent::Subscribers { uuid, count });
                    Ok(())
                },
            ))
            .map_err(win)?;
        }
        if c.write {
            let ev = events.clone();
            ch.WriteRequested(&TypedEventHandler::new(
                move |_: Ref<GattLocalCharacteristic>, args: Ref<GattWriteRequestedEventArgs>| {
                    let args = args.ok()?;
                    if let Ok(session) = args.Session() {
                        let what = format!("write to {} on {}", uuid_text(uuid), service);
                        note_central(&session, &what);
                    }
                    let deferral = args.GetDeferral()?;
                    let request = args.GetRequestAsync()?.get()?;
                    let value = bytes_of(&request.Value()?)?;
                    if request.Option()? == GattWriteOption::WriteWithResponse {
                        request.Respond()?;
                    }
                    deferral.Complete()?;
                    ev(PeripheralEvent::Write { uuid, value });
                    Ok(())
                },
            ))
            .map_err(win)?;
        }
        Ok(ch)
    }

    pub fn peripheral_notify(uuid: u128, value: &[u8]) -> Result<(), String> {
        if let Some(map) = values().as_mut() {
            if let Some(v) = map.get_mut(&uuid) {
                *v = value.to_vec();
            }
        }
        let guard = server();
        if guard.is_empty() {
            return Ok(());
        }
        let Some((_, ch)) = guard
            .iter()
            .flat_map(|p| p.chars.iter())
            .find(|(u, _)| *u == uuid)
        else {
            return Err(format!("No characteristic {} on the pod.", uuid_text(uuid)));
        };
        // Not awaited: Windows delivers it to each subscriber by itself.
        let _op = ch
            .NotifyValueAsync(&buffer(value).map_err(win)?)
            .map_err(win)?;
        Ok(())
    }

    /// The status of service's beacon in words, or None without one.
    #[cfg(test)]
    pub(super) fn beacon_status(service: u128) -> Option<String> {
        let guard = server();
        let beacon = guard
            .iter()
            .find(|p| p.service == service)?
            .beacon
            .as_ref()?;
        beacon.Status().ok().map(publisher_status_name)
    }

    fn stop_one(p: &Provided) {
        if let Some(beacon) = &p.beacon {
            match beacon.Stop() {
                Ok(()) => log::info!("GHOST_POD: {} beacon stopped", p.name),
                Err(e) => log::warn!("GHOST_POD: {} beacon stop failed: {}", p.name, e.message()),
            }
        }
        if let Err(e) = p.provider.StopAdvertising() {
            log::warn!(
                "GHOST_POD: {} ({}) stop advertising failed: {}",
                p.name,
                uuid_text(p.service),
                e.message()
            );
        }
        forget_values(&p.chars);
        log::info!("GHOST_POD: {} ({}) stopped", p.name, uuid_text(p.service));
    }

    pub fn peripheral_remove(service: u128) {
        let removed: Vec<Provided> = {
            let mut guard = server();
            let (gone, kept): (Vec<Provided>, Vec<Provided>) =
                guard.drain(..).partition(|p| p.service == service);
            *guard = kept;
            gone
        };
        for p in &removed {
            stop_one(p);
        }
    }

    pub fn peripheral_stop() {
        let all: Vec<Provided> = server().drain(..).collect();
        for p in &all {
            stop_one(p);
        }
        if !all.is_empty() {
            *values() = None;
            *centrals() = None;
            log::info!("GHOST_POD: all services stopped");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_display_that_is_off_counts_as_off() {
        assert!(display_state_is_off(0));
        assert!(!display_state_is_off(1), "on");
        assert!(!display_state_is_off(2), "dimmed");
    }

    #[test]
    fn alt_shortcuts_map_to_their_actions() {
        let alt = |vk| shortcut_for_key(vk, true);
        assert_eq!(alt(0xAF), Some(Shortcut::SpeedUp));
        assert_eq!(alt(0xBB), Some(Shortcut::SpeedUp));
        assert_eq!(alt(0xAE), Some(Shortcut::SpeedDown));
        assert_eq!(alt(0xBD), Some(Shortcut::SpeedDown));
        assert_eq!(alt(0x21), Some(Shortcut::InclineUp));
        assert_eq!(alt(0x22), Some(Shortcut::InclineDown));
        assert_eq!(alt(0x24), Some(Shortcut::ToggleStart));
        assert_eq!(alt(0x23), Some(Shortcut::ToggleProgram));
        assert_eq!(alt(0x2D), Some(Shortcut::ToggleMinified));
        // Other keys with Alt, and the same keys without it, are not shortcuts.
        for vk in [0x41u16, 0x25, 0x09, 0x0D, 0x70] {
            assert_eq!(alt(vk), None, "{vk:#x}");
        }
        for vk in [0xAFu16, 0xAE, 0x21, 0x22, 0x23, 0x24, 0x2D, 0xBB, 0xBD] {
            assert_eq!(shortcut_for_key(vk, false), None, "{vk:#x}");
        }
    }

    #[test]
    fn only_typing_keys_count() {
        // Letters, digits, space, Enter, Backspace, punctuation.
        for vk in [
            0x41u32, 0x5A, 0x30, 0x39, 0x20, 0x0D, 0x08, 0xBA, 0xBC, 0xBE, 0xDE,
        ] {
            assert!(is_typing_key(vk, false), "{vk:#x}");
        }
        // Any key pressed with Alt.
        assert!(!is_typing_key(0x41, true));
        // Modifiers and locks.
        for vk in [
            0x10u32, 0x11, 0x12, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x5B, 0x5C, 0x14, 0x90, 0x91,
        ] {
            assert!(!is_typing_key(vk, false), "{vk:#x}");
        }
        // Navigation, Esc, function and media keys.
        for vk in [
            0x21u32, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x1B, 0x70, 0x7B, 0x87, 0xA6, 0xAD,
            0xAF, 0xB7,
        ] {
            assert!(!is_typing_key(vk, false), "{vk:#x}");
        }
    }

    /// On this PC's real adapter: two services on providers of their own, one
    /// advertised and one only served; the served one never aborts, and
    /// removing it leaves the first advertising. It uses UUIDs the app never
    /// serves, so it doesn't clash with a running app's foot pod. Run with
    /// `cargo test peripheral_providers_are_independent -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs a Bluetooth adapter with the peripheral role"]
    fn peripheral_providers_are_independent() {
        use crate::platform::{
            uuid_text, PeripheralCharacteristic, PeripheralEvent, PeripheralService,
        };
        use std::sync::mpsc;
        use std::time::Duration;

        // Cycling Speed and Cadence: a 16-bit UUID (it fits the advertisement
        // next to the PC's name, as the foot pod's does) that the app never serves.
        const ADVERTISED: u128 = crate::platform::sig_uuid(0x1816);
        const SERVED: u128 = 0x5e1f7e57_0000_4000_8000_00000000b001;
        let svc = |name, uuid: u128, advertise| PeripheralService {
            name,
            uuid,
            advertise,
            beacon: None,
            characteristics: vec![PeripheralCharacteristic {
                uuid: uuid + 1,
                read: Some(vec![1]),
                notify: true,
                indicate: false,
                write: false,
            }],
        };
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let events: crate::platform::PeripheralEvents = std::sync::Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        });
        // Long enough for Windows to report each provider's status (it
        // answers within a second on a working adapter).
        let settle = Duration::from_secs(3);
        let drain = |rx: &mpsc::Receiver<PeripheralEvent>| {
            let mut got = Vec::new();
            while let Ok(e) = rx.recv_timeout(settle) {
                got.push(e);
            }
            got
        };

        peripheral_add(&svc("TEST_A", ADVERTISED, true), events.clone()).expect("add A");
        peripheral_add(&svc("TEST_B", SERVED, false), events.clone()).expect("add B");
        let first = drain(&rx);
        for e in &first {
            println!("both up: {e:?}");
        }
        assert!(
            first.contains(&PeripheralEvent::Advertising {
                service: ADVERTISED
            }),
            "{} never started: {first:?}",
            uuid_text(ADVERTISED)
        );
        assert!(
            !first.iter().any(
                |e| matches!(e, PeripheralEvent::Failed { service, .. } if *service == SERVED)
            ),
            "{} aborted: {first:?}",
            uuid_text(SERVED)
        );

        peripheral_remove(SERVED);
        let after = drain(&rx);
        for e in &after {
            println!("after removing B: {e:?}");
        }
        assert!(
            !after.iter().any(
                |e| matches!(e, PeripheralEvent::Failed { service, .. } if *service == ADVERTISED)
            ),
            "removing B aborted A: {after:?}"
        );
        assert!(
            peripheral_notify(ADVERTISED + 1, &[2]).is_ok(),
            "A is still served"
        );
        assert!(peripheral_notify(SERVED + 1, &[2]).is_err(), "B is gone");
        peripheral_stop();
    }

    /// On this PC's real adapter: a served-only service with a legacy beacon
    /// (a marker the app never sends) reports the beacon's status, and a
    /// watcher prints every advertisement it hears at -60 dBm or stronger:
    /// address, type, legacy or extended, connectable, and the raw sections.
    /// A PC usually can't hear its own advertisements, so an empty list says
    /// nothing; any line from this PC's address answers what it sends. Run with
    /// `cargo test legacy_beacon_probe -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs a Bluetooth adapter with the peripheral role"]
    fn legacy_beacon_probe() {
        use crate::platform::{PeripheralBeacon, PeripheralCharacteristic, PeripheralService};
        use std::time::Duration;
        use windows::core::Ref;
        use windows::Devices::Bluetooth::Advertisement::{
            BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
            BluetoothLEScanningMode,
        };
        use windows::Foundation::TypedEventHandler;
        use windows::Storage::Streams::DataReader;

        let watcher = BluetoothLEAdvertisementWatcher::new().unwrap();
        watcher
            .SetScanningMode(BluetoothLEScanningMode::Active)
            .unwrap();
        watcher.SetAllowExtendedAdvertisements(true).unwrap();
        watcher
            .Received(&TypedEventHandler::new(
                |_: Ref<BluetoothLEAdvertisementWatcher>,
                 args: Ref<BluetoothLEAdvertisementReceivedEventArgs>| {
                    let a = args.ok()?;
                    let rssi = a.RawSignalStrengthInDBm()?;
                    if rssi < -60 {
                        return Ok(());
                    }
                    let mut sections = String::new();
                    for s in a.Advertisement()?.DataSections()? {
                        let r = DataReader::FromBuffer(&s.Data()?)?;
                        let mut b = vec![0u8; r.UnconsumedBufferLength()? as usize];
                        r.ReadBytes(&mut b)?;
                        let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
                        sections += &format!(" {:02x}:{}", s.DataType()?, hex);
                    }
                    println!(
                        "heard {:012x} type {:?} rssi {} kind {:?} connectable {} scannable {} anonymous {} scan-response {} sections{}",
                        a.BluetoothAddress()?,
                        a.BluetoothAddressType()?,
                        rssi,
                        a.AdvertisementType()?,
                        a.IsConnectable()?,
                        a.IsScannable()?,
                        a.IsAnonymous()?,
                        a.IsScanResponse()?,
                        sections
                    );
                    Ok(())
                },
            ))
            .unwrap();
        watcher.Start().unwrap();

        let events: crate::platform::PeripheralEvents =
            std::sync::Arc::new(|e| println!("event: {e:?}"));
        let svc = PeripheralService {
            name: "TEST_BEACON",
            uuid: 0x5e1f7e57_0000_4000_8000_00000000b002,
            advertise: false,
            beacon: Some(PeripheralBeacon {
                company: 0xffff,
                data: b"TEST".to_vec(),
            }),
            characteristics: vec![PeripheralCharacteristic {
                uuid: 0x5e1f7e57_0000_4000_8000_00000000b003,
                read: Some(vec![1]),
                notify: false,
                indicate: false,
                write: false,
            }],
        };
        peripheral_add(&svc, events).expect("add");

        // Long enough for several advertising intervals and the status event.
        std::thread::sleep(Duration::from_secs(6));
        println!(
            "beacon status {}",
            peripheral::beacon_status(svc.uuid).expect("beacon started")
        );
        watcher.Stop().unwrap();
        peripheral_stop();
    }
}
