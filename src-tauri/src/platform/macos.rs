//! macOS implementation of the platform boundary (see `mod.rs`).
//!
//! Desktop units are points: AppKit places windows in points, and they are
//! the same size on every display (`display.rs` converts at Tauri's edge).
//!
//! Nothing here asks for a permission at startup:
//! - The HUD's click-through follows AppKit mouse monitors (event-driven, no
//!   permission needed for mouse events), and the idle time is the session's
//!   own (`CGEventSourceSecondsSinceLastEventType`).
//! - The shortcuts are Carbon hot keys (⌃⌥ plus a key, `macos_keys.rs`),
//!   which need no permission and take the key press from the app in front.
//! - Session lock and display sleep are read from CoreGraphics twice a second.
//! - Only the typing count for Neural flow needs a permission (Input
//!   Monitoring). It is asked for the first time Neural flow wants the count,
//!   with the system's own prompt; until it is granted nothing is counted.

use super::macos_keys::{self, HOT_KEYS};
use crate::hooks::Shortcut;
use objc2::rc::Weak;
use objc2_app_kit::{NSEvent, NSEventMask, NSWindow, NSWindowCollectionBehavior};
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Once;
use std::time::Duration;

pub const DESKTOP_IN_POINTS: bool = true;
pub const MENU_BAR_APP: bool = true;
pub const TRAY_ICON_TEMPLATE: bool = true;
pub const MICROPHONE: bool = false;
pub const MINIFY_KEY_CHOICE: bool = false;

pub const WORDS: super::Words = super::Words {
    computer: "Mac",
    settings: "System Settings",
    bluetooth_blocked: "macOS blocked Bluetooth access. Allow it in Privacy & Security, Bluetooth.",
    credential_store: "the Keychain",
    release_download: "Mac download",
};

/// The C APIs used here, declared as their headers declare them.
mod ffi {
    use std::ffi::{c_char, c_void};

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    pub type CFTypeRef = *const c_void;
    pub type CGEventRef = *mut c_void;
    pub type CFMachPortRef = *mut c_void;
    pub type TapCallback = extern "C" fn(*mut c_void, u32, CGEventRef, *mut c_void) -> CGEventRef;

    /// `kCGEventSourceStateCombinedSessionState`.
    pub const COMBINED_SESSION_STATE: i32 = 0;
    /// `kCGAnyInputEventType`.
    pub const ANY_INPUT_EVENT: u32 = !0;
    /// `kCGMouseButtonLeft`.
    pub const MOUSE_BUTTON_LEFT: u32 = 0;
    /// `kCGSessionEventTap`, `kCGHeadInsertEventTap`, `kCGEventTapOptionListenOnly`.
    pub const SESSION_EVENT_TAP: u32 = 1;
    pub const HEAD_INSERT_EVENT_TAP: u32 = 0;
    pub const TAP_LISTEN_ONLY: u32 = 1;
    /// `kCGEventKeyDown` and the tap-disabled pseudo-events.
    pub const EVENT_KEY_DOWN: u32 = 10;
    pub const EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
    pub const EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
    /// `kCGKeyboardEventKeycode`.
    pub const FIELD_KEYCODE: u32 = 9;
    /// `kCFStringEncodingUTF8`, `kCFNumberSInt32Type`.
    pub const UTF8: u32 = 0x0800_0100;
    pub const NUMBER_SINT32: isize = 3;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
        pub fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
        pub fn CGEventCreate(source: *const c_void) -> CGEventRef;
        pub fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
        pub fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        pub fn CGEventGetFlags(event: CGEventRef) -> u64;
        pub fn CGSessionCopyCurrentDictionary() -> CFTypeRef;
        pub fn CGMainDisplayID() -> u32;
        pub fn CGDisplayIsAsleep(display: u32) -> u32;
        pub fn CGPreflightListenEventAccess() -> bool;
        pub fn CGRequestListenEventAccess() -> bool;
        pub fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: TapCallback,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        pub static kCFRunLoopCommonModes: CFTypeRef;
        pub fn CFRelease(cf: CFTypeRef);
        pub fn CFGetTypeID(cf: CFTypeRef) -> usize;
        pub fn CFBooleanGetTypeID() -> usize;
        pub fn CFBooleanGetValue(boolean: CFTypeRef) -> u8;
        pub fn CFNumberGetTypeID() -> usize;
        pub fn CFNumberGetValue(number: CFTypeRef, the_type: isize, value: *mut c_void) -> u8;
        pub fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFTypeRef;
        pub fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
        pub fn CFMachPortCreateRunLoopSource(
            alloc: CFTypeRef,
            port: CFMachPortRef,
            order: isize,
        ) -> CFTypeRef;
        pub fn CFRunLoopGetCurrent() -> CFTypeRef;
        pub fn CFRunLoopAddSource(run_loop: CFTypeRef, source: CFTypeRef, mode: CFTypeRef);
        pub fn CFRunLoopRun();
    }

    #[repr(C)]
    pub struct EventTypeSpec {
        pub event_class: u32,
        pub event_kind: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct EventHotKeyID {
        pub signature: u32,
        pub id: u32,
    }

    pub type EventHandlerProc = extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32;

    /// `kEventClassKeyboard` ('keyb'), `kEventHotKeyPressed`,
    /// `kEventParamDirectObject` ('----'), `typeEventHotKeyID` ('hkid').
    pub const EVENT_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
    pub const EVENT_HOT_KEY_PRESSED: u32 = 5;
    pub const PARAM_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
    pub const TYPE_HOT_KEY_ID: u32 = u32::from_be_bytes(*b"hkid");
    /// `noErr`, `eventNotHandledErr`.
    pub const NO_ERR: i32 = 0;
    pub const EVENT_NOT_HANDLED: i32 = -9874;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        pub fn GetApplicationEventTarget() -> *mut c_void;
        pub fn InstallEventHandler(
            target: *mut c_void,
            handler: EventHandlerProc,
            num_types: usize,
            list: *const EventTypeSpec,
            user_data: *mut c_void,
            out_ref: *mut *mut c_void,
        ) -> i32;
        pub fn RegisterEventHotKey(
            key_code: u32,
            modifiers: u32,
            id: EventHotKeyID,
            target: *mut c_void,
            options: u32,
            out_ref: *mut *mut c_void,
        ) -> i32;
        pub fn GetEventParameter(
            event: *mut c_void,
            name: u32,
            desired_type: u32,
            actual_type: *mut u32,
            buffer_size: usize,
            actual_size: *mut usize,
            data: *mut c_void,
        ) -> i32;
    }
}

// ---------------------------------------------------------------------------
// App setup
// ---------------------------------------------------------------------------

/// A menu bar app: no Dock icon or ⌘-Tab entry (`LSUIElement` in Info.plist
/// does the same for the bundled app before this runs). Then the shortcuts.
/// On the main thread, once the app is up.
pub fn app_started(app: &tauri::AppHandle) {
    if let Err(e) = app.set_activation_policy(tauri::ActivationPolicy::Accessory) {
        log::warn!("Could not hide the Dock icon: {}", e);
    }
    register_hot_keys();
}

// ---------------------------------------------------------------------------
// Shortcuts (Carbon hot keys)
// ---------------------------------------------------------------------------

/// A Carbon hot key's modifiers (`Events.h`): Control and Option.
const HOTKEY_CONTROL: u32 = 1 << 12;
const HOTKEY_OPTION: u32 = 1 << 11;
/// The signature in each hot key's id: "NEON".
const HOT_KEY_SIGNATURE: u32 = u32::from_be_bytes(*b"NEON");

fn register_hot_keys() {
    // A dev build leaves the shortcuts to the release app: a hot key belongs
    // to whichever app registered it first, and can't be passed on.
    if crate::hooks::shortcuts_left_to_release() {
        log::info!("The release app is running; this dev build leaves it the shortcuts");
        return;
    }
    let spec = ffi::EventTypeSpec {
        event_class: ffi::EVENT_CLASS_KEYBOARD,
        event_kind: ffi::EVENT_HOT_KEY_PRESSED,
    };
    // SAFETY: called on the main thread once the app runs; `spec` outlives the
    // call (Carbon copies the list), and the handler is a plain function.
    let status = unsafe {
        ffi::InstallEventHandler(
            ffi::GetApplicationEventTarget(),
            on_hot_key,
            1,
            &spec,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if status != ffi::NO_ERR {
        log::warn!("Shortcuts unavailable: the hot key handler was refused ({status})");
        return;
    }
    let mut taken = Vec::new();
    for (i, key) in HOT_KEYS.iter().enumerate() {
        let id = ffi::EventHotKeyID {
            signature: HOT_KEY_SIGNATURE,
            id: i as u32,
        };
        let mut hot_key_ref = std::ptr::null_mut();
        // SAFETY: plain arguments and an out pointer that lives for the call.
        // The hot key stays registered for the life of the app.
        let status = unsafe {
            ffi::RegisterEventHotKey(
                u32::from(key.key_code),
                HOTKEY_CONTROL | HOTKEY_OPTION,
                id,
                ffi::GetApplicationEventTarget(),
                0,
                &mut hot_key_ref,
            )
        };
        if status != ffi::NO_ERR {
            taken.push(macos_keys::label(key.shortcut));
        }
    }
    if taken.is_empty() {
        log::info!("Shortcuts registered (⌃⌥)");
    } else {
        log::warn!("Shortcuts taken by another app: {}", taken.join(", "));
    }
}

extern "C" fn on_hot_key(_call: *mut c_void, event: *mut c_void, _data: *mut c_void) -> i32 {
    let mut id = ffi::EventHotKeyID::default();
    // SAFETY: `event` is the hot key event Carbon passed in; the buffer is an
    // EventHotKeyID of the size given.
    let status = unsafe {
        ffi::GetEventParameter(
            event,
            ffi::PARAM_DIRECT_OBJECT,
            ffi::TYPE_HOT_KEY_ID,
            std::ptr::null_mut(),
            std::mem::size_of::<ffi::EventHotKeyID>(),
            std::ptr::null_mut(),
            (&mut id as *mut ffi::EventHotKeyID).cast(),
        )
    };
    if status != ffi::NO_ERR || id.signature != HOT_KEY_SIGNATURE {
        return ffi::EVENT_NOT_HANDLED;
    }
    crate::hooks::note_input();
    let Some(key) = HOT_KEYS.get(id.id as usize) else {
        return ffi::EVENT_NOT_HANDLED;
    };
    let wanted = match key.shortcut {
        Shortcut::CalibrationTap => crate::hooks::tap_listening(),
        Shortcut::ToggleMinified => crate::hooks::minify_key().is_some(),
        _ => true,
    };
    if wanted && !crate::hooks::shortcuts_left_to_release() {
        crate::hooks::shortcut_pressed(key.shortcut);
    }
    ffi::NO_ERR
}

// ---------------------------------------------------------------------------
// Input: idle time and the typing count
// ---------------------------------------------------------------------------

/// How often the last-input time is refreshed from the session's idle time.
const IDLE_POLL: Duration = Duration::from_millis(500);
/// How often the typing thread checks whether Neural flow wants the count.
const TYPING_POLL: Duration = Duration::from_secs(1);

pub fn install_input_hooks() {
    let idle = std::thread::Builder::new()
        .name("input-idle".into())
        .spawn(|| loop {
            if let Some(idle) = input_idle_for() {
                crate::hooks::note_input_ago(idle);
            }
            std::thread::sleep(IDLE_POLL);
        });
    if let Err(e) = idle {
        log::warn!("Idle time not watched: {}", e);
    }
    let typing = std::thread::Builder::new()
        .name("typing-tap".into())
        .spawn(run_typing_tap);
    if let Err(e) = typing {
        log::warn!("Typing count unavailable: {}", e);
    }
}

/// The typing tap, once installed (re-enabled from its own callback).
static TYPING_TAP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Wait until Neural flow wants the typing count, ask for Input Monitoring
/// once if it isn't granted, and install a listen-only key tap as soon as it
/// is. The tap stays for the life of the app; it counts only while wanted.
fn run_typing_tap() {
    let mut asked = false;
    loop {
        if crate::hooks::key_counting() {
            // SAFETY: plain query, no arguments.
            if unsafe { ffi::CGPreflightListenEventAccess() } {
                if install_typing_tap() {
                    log::info!("Typing count on (Input Monitoring granted)");
                    // SAFETY: runs this thread's run loop, which holds the tap.
                    unsafe { ffi::CFRunLoopRun() };
                }
                log::warn!("Typing tap ended; Neural flow will not see typing");
                return;
            } else if !asked {
                asked = true;
                log::info!("Asking for Input Monitoring, for the typing count");
                // SAFETY: shows the system prompt at most once; no arguments.
                unsafe { ffi::CGRequestListenEventAccess() };
            }
        }
        std::thread::sleep(TYPING_POLL);
    }
}

fn install_typing_tap() -> bool {
    // SAFETY: a listen-only tap for key-downs; the callback is a plain
    // function, and the run loop source is added to this thread's run loop.
    unsafe {
        let tap = ffi::CGEventTapCreate(
            ffi::SESSION_EVENT_TAP,
            ffi::HEAD_INSERT_EVENT_TAP,
            ffi::TAP_LISTEN_ONLY,
            1 << ffi::EVENT_KEY_DOWN,
            on_key_event,
            std::ptr::null_mut(),
        );
        if tap.is_null() {
            return false;
        }
        TYPING_TAP.store(tap, Ordering::SeqCst);
        let source = ffi::CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        if source.is_null() {
            return false;
        }
        ffi::CFRunLoopAddSource(
            ffi::CFRunLoopGetCurrent(),
            source,
            ffi::kCFRunLoopCommonModes,
        );
        ffi::CGEventTapEnable(tap, true);
    }
    true
}

extern "C" fn on_key_event(
    _proxy: *mut c_void,
    event_type: u32,
    event: ffi::CGEventRef,
    _user_info: *mut c_void,
) -> ffi::CGEventRef {
    match event_type {
        // macOS switches a slow tap off; switch it back on.
        ffi::EVENT_TAP_DISABLED_BY_TIMEOUT | ffi::EVENT_TAP_DISABLED_BY_USER_INPUT => {
            let tap = TYPING_TAP.load(Ordering::SeqCst);
            if !tap.is_null() {
                // SAFETY: the tap this callback belongs to, never freed.
                unsafe { ffi::CGEventTapEnable(tap, true) };
            }
        }
        ffi::EVENT_KEY_DOWN if crate::hooks::key_counting() => {
            // SAFETY: a key-down event macOS passed in, valid for the call.
            let (code, flags) = unsafe {
                (
                    ffi::CGEventGetIntegerValueField(event, ffi::FIELD_KEYCODE),
                    ffi::CGEventGetFlags(event),
                )
            };
            if macos_keys::counts_as_typing(code as u16, flags) {
                crate::hooks::count_key();
            }
        }
        _ => {}
    }
    // Listen-only: the event goes on unchanged whatever is returned.
    event
}

/// How long since the last keyboard or mouse input in this login session.
pub fn input_idle_for() -> Option<Duration> {
    // SAFETY: plain query with constant arguments.
    let secs = unsafe {
        ffi::CGEventSourceSecondsSinceLastEventType(
            ffi::COMBINED_SESSION_STATE,
            ffi::ANY_INPUT_EVENT,
        )
    };
    (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs))
}

// ---------------------------------------------------------------------------
// Other instances
// ---------------------------------------------------------------------------

/// `tauri-plugin-single-instance` listens on a socket named after the
/// identifier while the app runs, and removes it when the app exits.
pub fn app_instance_running(identifier: &str) -> bool {
    let name = identifier.replace(['.', '-'], "_");
    std::path::Path::new(&format!("/tmp/{name}_si.sock")).exists()
}

// ---------------------------------------------------------------------------
// Windows: the HUD and the dashboard
// ---------------------------------------------------------------------------

thread_local! {
    /// The HUD's window, on the main thread (where AppKit and the mouse
    /// monitors run). Weak: a closed HUD reads as none.
    static HUD_WINDOW: RefCell<Option<Weak<NSWindow>>> = const { RefCell::new(None) };
}

static MONITORS: Once = Once::new();

/// The window behind a Tauri window, on the main thread.
fn ns_window(window: &tauri::WebviewWindow) -> Option<&NSWindow> {
    let ptr = window.ns_window().ok()?.cast::<NSWindow>();
    // SAFETY: Tauri's NSWindow, alive while `window` is; only called on the
    // main thread (from `run_on_main_thread`).
    unsafe { ptr.as_ref() }
}

/// The HUD: on every Space and over full-screen apps, left out of window
/// cycling, and click-through except over its content (the mouse monitors).
pub fn remember_hud_window(window: &tauri::WebviewWindow) {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Some(ns) = ns_window(&w) else {
            return;
        };
        ns.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        ns.setAcceptsMouseMovedEvents(true);
        HUD_WINDOW.with(|h| *h.borrow_mut() = Some(Weak::from(ns)));
        MONITORS.call_once(install_mouse_monitors);
    });
}

/// The dashboard opens on the Space in use, over a full-screen app too, like
/// a menu bar dropdown.
pub fn prepare_popup_window(window: &tauri::WebviewWindow) {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        if let Some(ns) = ns_window(&w) {
            ns.setCollectionBehavior(
                NSWindowCollectionBehavior::MoveToActiveSpace
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
        }
    });
}

/// Mouse events everywhere (global: other apps; local: this app's windows)
/// keep the HUD's click-through in step with the pointer.
fn install_mouse_monitors() {
    let mask = NSEventMask::MouseMoved
        | NSEventMask::LeftMouseDragged
        | NSEventMask::LeftMouseDown
        | NSEventMask::LeftMouseUp
        | NSEventMask::RightMouseDown;
    let global = block2::RcBlock::new(|_event: NonNull<NSEvent>| sync_hud_click_through());
    let local = block2::RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        sync_hud_click_through();
        event.as_ptr()
    });
    let global = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global);
    // SAFETY: the block returns the event it was given, unchanged.
    let local = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local) };
    if global.is_none() || local.is_none() {
        log::warn!("Mouse monitors not installed; the HUD stays click-through");
    }
    // Installed for the life of the app.
    std::mem::forget(global);
    std::mem::forget(local);
}

/// Make the HUD click-through or interactive for the pointer where it is now.
/// Reads the window's real state each time and corrects only a mismatch, as
/// on Windows. Main thread (the monitors' handlers).
fn sync_hud_click_through() {
    HUD_WINDOW.with(|h| {
        let Some(window) = h.borrow().as_ref().and_then(Weak::load) else {
            return;
        };
        let Some((x, y)) = cursor_position() else {
            return;
        };
        let Some(want) = crate::hud::click_through_at(x, y) else {
            return;
        };
        if window.ignoresMouseEvents() != want {
            // On the main thread, Tauri applies it at once.
            crate::hud::set_click_through(want);
        }
    });
}

// ---------------------------------------------------------------------------
// Pointer
// ---------------------------------------------------------------------------

/// The primary (left) mouse button is held down.
pub fn primary_button_down() -> bool {
    // SAFETY: plain query with constant arguments.
    unsafe { ffi::CGEventSourceButtonState(ffi::COMBINED_SESSION_STATE, ffi::MOUSE_BUTTON_LEFT) }
}

/// The cursor in points, from the top-left of the main display (the space
/// `display.rs` puts the monitors in).
pub fn cursor_position() -> Option<(i32, i32)> {
    // SAFETY: CGEventCreate(NULL) makes an event holding the current cursor;
    // it is released after reading.
    unsafe {
        let event = ffi::CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let p = ffi::CGEventGetLocation(event);
        ffi::CFRelease(event);
        Some((p.x.round() as i32, p.y.round() as i32))
    }
}

// ---------------------------------------------------------------------------
// Session lock and display sleep
// ---------------------------------------------------------------------------

static LOCKED: AtomicBool = AtomicBool::new(false);
static DISPLAY_OFF: AtomicBool = AtomicBool::new(false);
/// Twice a second: walk-away pauses "at once" on a lock.
const SESSION_POLL: Duration = Duration::from_millis(500);

pub fn spawn_session_lock_watcher() {
    let spawned = std::thread::Builder::new()
        .name("session-watch".into())
        .spawn(|| loop {
            let locked = screen_locked();
            if LOCKED.swap(locked, Ordering::SeqCst) != locked {
                log::info!("Session {}", if locked { "locked" } else { "unlocked" });
            }
            // SAFETY: plain queries.
            let off = unsafe { ffi::CGDisplayIsAsleep(ffi::CGMainDisplayID()) } != 0;
            if DISPLAY_OFF.swap(off, Ordering::SeqCst) != off {
                log::info!("Display {}", if off { "asleep" } else { "awake" });
            }
            std::thread::sleep(SESSION_POLL);
        });
    if let Err(e) = spawned {
        log::warn!("Session lock not watched: {}", e);
    }
}

/// The session dictionary carries `CGSSessionScreenIsLocked` while the screen
/// is locked.
fn screen_locked() -> bool {
    // SAFETY: the dictionary and key are created here and released here; the
    // value is borrowed from the dictionary and read before it is released.
    unsafe {
        let dict = ffi::CGSessionCopyCurrentDictionary();
        if dict.is_null() {
            return false;
        }
        let key = ffi::CFStringCreateWithCString(
            std::ptr::null(),
            c"CGSSessionScreenIsLocked".as_ptr(),
            ffi::UTF8,
        );
        let value = if key.is_null() {
            std::ptr::null()
        } else {
            ffi::CFDictionaryGetValue(dict, key)
        };
        let locked = if value.is_null() {
            false
        } else if ffi::CFGetTypeID(value) == ffi::CFBooleanGetTypeID() {
            ffi::CFBooleanGetValue(value) != 0
        } else if ffi::CFGetTypeID(value) == ffi::CFNumberGetTypeID() {
            let mut n: i32 = 0;
            ffi::CFNumberGetValue(value, ffi::NUMBER_SINT32, (&mut n as *mut i32).cast());
            n != 0
        } else {
            false
        };
        if !key.is_null() {
            ffi::CFRelease(key);
        }
        ffi::CFRelease(dict);
        locked
    }
}

pub fn session_locked() -> bool {
    LOCKED.load(Ordering::SeqCst)
}

pub fn display_off() -> bool {
    DISPLAY_OFF.load(Ordering::SeqCst)
}

// ---------------------------------------------------------------------------
// Not yet on macOS
// ---------------------------------------------------------------------------

/// Not yet: macOS lists which processes use audio input only from 14.2.
pub fn apps_using_microphone(_own_exe: Option<&str>) -> Vec<String> {
    Vec::new()
}

pub fn peripheral_add(
    _service: &super::PeripheralService,
    _events: super::PeripheralEvents,
) -> Result<(), String> {
    Err("This only works on Windows for now.".to_string())
}

pub fn peripheral_notify(_uuid: u128, _value: &[u8]) -> Result<(), String> {
    Ok(())
}

pub fn peripheral_remove(_service: u128) {}

pub fn peripheral_stop() {}
