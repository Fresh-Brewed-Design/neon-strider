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
}
