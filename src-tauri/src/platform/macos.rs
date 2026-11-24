//! macOS implementation of the platform boundary (see `mod.rs`).
//!
//! Desktop units are points: AppKit places windows in points, and they are
//! the same size on every display (`display.rs` converts at Tauri's edge).
//!
//! Nothing here asks for a permission at startup:
//! - The HUD's click-through follows AppKit mouse monitors (event-driven, no
//!   permission needed for mouse events), and the idle time is the session's
//!   own (`CGEventSourceSecondsSinceLastEventType`).
//! - The shortcuts are Carbon hot keys (⌃⌥ plus a key by default, changed in
//!   System -> Shortcuts; `keys.rs`), which need no permission and take the
//!   key press from the app in front.
//! - Session lock and display sleep are read from CoreGraphics twice a second.
//! - Only the typing count for Neural flow needs a permission (Input
//!   Monitoring). It is asked for the first time Neural flow wants the count,
//!   with the system's own prompt; until it is granted nothing is counted.

use super::{keys, macos_keys};
use crate::hooks::Shortcut;
use crate::shortcuts::{Bindings, KeyCombo};
use objc2::rc::Weak;
use objc2_app_kit::{NSEvent, NSEventMask, NSWindow, NSWindowCollectionBehavior};
use smappservice_rs::{AppService, ServiceStatus, ServiceType};
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Once;
use std::time::Duration;

pub const DESKTOP_IN_POINTS: bool = true;
pub const MENU_BAR_APP: bool = true;
pub const TRAY_ICON_TEMPLATE: bool = true;
pub const MICROPHONE: bool = true;

pub const WORDS: super::Words = super::Words {
    computer: "Mac",
    settings: "System Settings",
    bluetooth_blocked: "macOS blocked Bluetooth access. Allow it in Privacy & Security, Bluetooth.",
    login_not_saved: "The Strava login could not be saved securely in the Keychain.",
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

    /// `kIOHIDRequestTypeListenEvent`.
    pub const HID_REQUEST_LISTEN_EVENT: u32 = 1;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        /// Returns `kIOHIDAccessTypeGranted` (0), `Denied` (1) or `Unknown` (2).
        pub fn IOHIDCheckAccess(request: u32) -> u32;
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
        pub fn UnregisterEventHotKey(hot_key: *mut c_void) -> i32;
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
    install_hot_key_handler();
}

// ---------------------------------------------------------------------------
// Shortcuts (Carbon hot keys)
// ---------------------------------------------------------------------------

/// The signature in each hot key's id: "NEON".
const HOT_KEY_SIGNATURE: u32 = u32::from_be_bytes(*b"NEON");

thread_local! {
    /// The hot keys registered now (main thread, where Carbon calls the
    /// handler): each one's Carbon ref (null when another app has it) and the
    /// shortcut it runs. A hot key's id is its place here.
    static HOT_KEYS: RefCell<Vec<(*mut c_void, Shortcut)>> = const { RefCell::new(Vec::new()) };
}

/// Keys another app registered first, so they do nothing here.
static TAKEN: std::sync::Mutex<Vec<KeyCombo>> = std::sync::Mutex::new(Vec::new());

fn install_hot_key_handler() {
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
    }
}

/// Register `bindings` as the hot keys, replacing the ones before, on the main
/// thread. None registers nothing (a recording pauses them this way).
pub fn set_shortcuts(bindings: &Bindings) {
    let hot_keys = keys::macos_hot_keys(bindings);
    let Some(app) = crate::app_handle() else {
        log::warn!("Shortcuts not registered: the app isn't up yet");
        return;
    };
    if let Err(e) = app.run_on_main_thread(move || register_hot_keys(hot_keys)) {
        log::warn!("Shortcuts not registered: {}", e);
    }
}

pub fn taken_shortcuts() -> Vec<KeyCombo> {
    TAKEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Main thread only.
fn register_hot_keys(wanted: Vec<keys::HotKey>) {
    HOT_KEYS.with(|registered| {
        for (hot_key_ref, _) in registered.borrow_mut().drain(..) {
            if !hot_key_ref.is_null() {
                // SAFETY: a ref RegisterEventHotKey returned and not yet released.
                unsafe { ffi::UnregisterEventHotKey(hot_key_ref) };
            }
        }
        let mut taken = Vec::new();
        // A dev build leaves the shortcuts to the release app: a hot key belongs
        // to whichever app registered it first, and can't be passed on.
        if !wanted.is_empty() && crate::hooks::shortcuts_left_to_release() {
            log::info!("The release app is running; this dev build leaves it the shortcuts");
        } else {
            let mut list = registered.borrow_mut();
            for (i, key) in wanted.iter().enumerate() {
                let id = ffi::EventHotKeyID {
                    signature: HOT_KEY_SIGNATURE,
                    id: i as u32,
                };
                let mut hot_key_ref = std::ptr::null_mut();
                // SAFETY: plain arguments and an out pointer that lives for the
                // call; the ref is kept until the next registration releases it.
                let status = unsafe {
                    ffi::RegisterEventHotKey(
                        u32::from(key.key_code),
                        key.modifiers,
                        id,
                        ffi::GetApplicationEventTarget(),
                        0,
                        &mut hot_key_ref,
                    )
                };
                if status != ffi::NO_ERR {
                    hot_key_ref = std::ptr::null_mut();
                    taken.push(key.combo.clone());
                }
                list.push((hot_key_ref, key.shortcut));
            }
            if !wanted.is_empty() {
                log::info!("Shortcuts registered ({} keys)", wanted.len() - taken.len());
            }
        }
        if !taken.is_empty() {
            let labels: Vec<String> = taken
                .iter()
                .map(|c| keys::label(super::Os::Macos, c))
                .collect();
            log::warn!("Shortcuts another app has: {}", labels.join(", "));
        }
        *TAKEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = taken;
    });
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
    let Some(shortcut) = HOT_KEYS.with(|r| r.borrow().get(id.id as usize).map(|(_, s)| *s)) else {
        return ffi::EVENT_NOT_HANDLED;
    };
    let wanted = shortcut != Shortcut::CalibrationTap || crate::hooks::tap_listening();
    if wanted && !crate::hooks::shortcuts_left_to_release() {
        crate::hooks::shortcut_pressed(shortcut);
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

// ---------------------------------------------------------------------------
// Open at login (a Login Item)
// ---------------------------------------------------------------------------

/// Opening at login is the app itself registered with SMAppService, so it
/// shows under System Settings, General, Login Items like any Mac app. An
/// earlier build used a LaunchAgent plist instead; that one moves over.
#[cfg_attr(
    debug_assertions,
    expect(dead_code, reason = "dev builds never launch at login")
)]
pub fn launch_at_login(app: &tauri::AppHandle) -> Result<bool, String> {
    move_launch_agent(app);
    Ok(login_item().status() == ServiceStatus::Enabled)
}

#[cfg_attr(
    debug_assertions,
    expect(dead_code, reason = "dev builds never launch at login")
)]
pub fn set_launch_at_login(app: &tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    move_launch_agent(app);
    let item = login_item();
    if enabled {
        if item.status() != ServiceStatus::Enabled {
            item.register()
                .map_err(|e| format!("macOS refused the Login Item: {e}."))?;
        }
        if item.status() == ServiceStatus::RequiresApproval {
            AppService::open_system_settings_login_items();
            return Err(
                "Switch Neon Strider on in System Settings, General, Login Items.".to_string(),
            );
        }
    } else if item.status() != ServiceStatus::NotRegistered {
        item.unregister()
            .map_err(|e| format!("macOS kept the Login Item: {e}."))?;
    }
    Ok(item.status() == ServiceStatus::Enabled)
}

fn login_item() -> AppService {
    AppService::new(ServiceType::MainApp)
}

/// An earlier build's LaunchAgent plist (`tauri-plugin-autostart`): removed,
/// and the Login Item registered in its place, so the choice carries over.
#[cfg_attr(
    debug_assertions,
    expect(dead_code, reason = "dev builds never launch at login")
)]
fn move_launch_agent(app: &tauri::AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    let agent = app.autolaunch();
    if !agent.is_enabled().unwrap_or(false) {
        return;
    }
    if let Err(e) = agent.disable() {
        log::warn!("Could not remove the old LaunchAgent: {}", e);
        return;
    }
    match login_item().register() {
        Ok(()) => log::info!("Open at login moved from a LaunchAgent to a Login Item"),
        Err(e) => log::warn!(
            "Login Item not registered in the LaunchAgent's place: {}",
            e
        ),
    }
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
// Permissions
// ---------------------------------------------------------------------------

/// How often the permission states are read again: changing one means a trip
/// to System Settings, so 2 s is quick enough to see it on return.
const PERMISSIONS_EVERY: Duration = Duration::from_secs(2);

static PERMISSIONS: std::sync::Mutex<Option<(std::time::Instant, Vec<super::Permission>)>> =
    std::sync::Mutex::new(None);

pub fn permissions() -> Vec<super::Permission> {
    let mut cached = PERMISSIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((at, list)) = cached.as_ref() {
        if at.elapsed() < PERMISSIONS_EVERY {
            return list.clone();
        }
    }
    let list = vec![
        super::Permission {
            kind: super::PermissionKind::Bluetooth,
            state: bluetooth_permission(),
        },
        super::Permission {
            kind: super::PermissionKind::InputMonitoring,
            state: input_monitoring_permission(),
        },
    ];
    *cached = Some((std::time::Instant::now(), list.clone()));
    list
}

/// `+[CBManager authorization]` (macOS 10.15+): not determined, restricted,
/// denied, allowed always.
fn bluetooth_permission() -> super::PermissionState {
    use super::PermissionState as S;
    let Some(class) = objc2::runtime::AnyClass::get(c"CBManager") else {
        return S::NotAsked;
    };
    // SAFETY: a class property returning CBManagerAuthorization (NSInteger);
    // CoreBluetooth is linked by btleplug.
    let raw: isize = unsafe { objc2::msg_send![class, authorization] };
    match raw {
        3 => S::Allowed,
        1 | 2 => S::Denied,
        _ => S::NotAsked,
    }
}

/// `IOHIDCheckAccess(kIOHIDRequestTypeListenEvent)`: granted, denied or
/// unknown (never asked), which CoreGraphics' preflight can't tell apart.
fn input_monitoring_permission() -> super::PermissionState {
    use super::PermissionState as S;
    // SAFETY: plain query with a constant argument.
    match unsafe { ffi::IOHIDCheckAccess(ffi::HID_REQUEST_LISTEN_EVENT) } {
        0 => S::Allowed,
        1 => S::Denied,
        _ => S::NotAsked,
    }
}

/// The Privacy & Security pane for `kind`, through its settings URL.
pub fn open_permission_settings(kind: super::PermissionKind) -> Result<(), String> {
    let pane = match kind {
        super::PermissionKind::Bluetooth => "Privacy_Bluetooth",
        super::PermissionKind::InputMonitoring => "Privacy_ListenEvent",
    };
    let url = format!("x-apple.systempreferences:com.apple.preference.security?{pane}");
    std::process::Command::new("/usr/bin/open")
        .arg(&url)
        .status()
        .map_err(|e| format!("Could not open System Settings: {e}."))
        .and_then(|s| {
            s.success()
                .then_some(())
                .ok_or_else(|| "Could not open System Settings.".to_string())
        })
}

// ---------------------------------------------------------------------------
// Microphone (Core Audio)
// ---------------------------------------------------------------------------

/// The apps recording from the microphone right now, by name. macOS 14.2 and
/// later list each process using audio input; macOS 13 only says whether the
/// default input device is running for anyone, which reads as one unnamed
/// app. Only metadata is read: no audio, and no permission is needed.
pub fn apps_using_microphone(_own_exe: Option<&str>) -> Vec<String> {
    let own = std::process::id() as i32;
    match core_audio::input_processes() {
        Some(processes) => processes
            .into_iter()
            .filter(|p| p.pid != own)
            .map(|p| {
                app_name(p.pid)
                    .or(p.bundle_id)
                    .unwrap_or_else(|| format!("process {}", p.pid))
            })
            .collect(),
        None if core_audio::default_input_running() => vec!["an app".to_string()],
        None => Vec::new(),
    }
}

/// The name the user knows an app by (Microsoft Teams, zoom.us).
fn app_name(pid: i32) -> Option<String> {
    let app = objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
    app.localizedName().map(|n| n.to_string())
}

mod core_audio {
    use std::ffi::c_void;

    #[repr(C)]
    struct PropertyAddress {
        selector: u32,
        scope: u32,
        element: u32,
    }

    const fn code(c: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*c)
    }

    /// `kAudioObjectSystemObject`, `kAudioObjectPropertyScopeGlobal`,
    /// `kAudioObjectPropertyElementMain`.
    const SYSTEM_OBJECT: u32 = 1;
    const SCOPE_GLOBAL: u32 = code(b"glob");
    const ELEMENT_MAIN: u32 = 0;
    /// `kAudioHardwarePropertyProcessObjectList` (macOS 14.2).
    const PROCESS_OBJECT_LIST: u32 = code(b"prs#");
    /// `kAudioProcessPropertyPID`, `kAudioProcessPropertyBundleID`,
    /// `kAudioProcessPropertyIsRunningInput`.
    const PROCESS_PID: u32 = code(b"ppid");
    const PROCESS_BUNDLE_ID: u32 = code(b"pbid");
    const PROCESS_IS_RUNNING_INPUT: u32 = code(b"piri");
    /// `kAudioHardwarePropertyDefaultInputDevice`,
    /// `kAudioDevicePropertyDeviceIsRunningSomewhere`.
    const DEFAULT_INPUT_DEVICE: u32 = code(b"dIn ");
    const DEVICE_IS_RUNNING_SOMEWHERE: u32 = code(b"gone");

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectHasProperty(object: u32, address: *const PropertyAddress) -> u8;
        fn AudioObjectGetPropertyDataSize(
            object: u32,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn AudioObjectGetPropertyData(
            object: u32,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    fn address(selector: u32) -> PropertyAddress {
        PropertyAddress {
            selector,
            scope: SCOPE_GLOBAL,
            element: ELEMENT_MAIN,
        }
    }

    /// A fixed-size property (a number or an object id).
    fn read<T: Copy + Default>(object: u32, selector: u32) -> Option<T> {
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>() as u32;
        // SAFETY: the buffer is a T of the size given; Core Audio writes at
        // most that many bytes.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &address(selector),
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut T).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    pub struct InputProcess {
        pub pid: i32,
        pub bundle_id: Option<String>,
    }

    /// The processes taking audio input now; `None` before macOS 14.2.
    pub fn input_processes() -> Option<Vec<InputProcess>> {
        let list = address(PROCESS_OBJECT_LIST);
        // SAFETY: reads whether the system object has the property.
        if unsafe { AudioObjectHasProperty(SYSTEM_OBJECT, &list) } == 0 {
            return None;
        }
        let mut size = 0u32;
        // SAFETY: plain size query into a u32.
        let status = unsafe {
            AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &list, 0, std::ptr::null(), &mut size)
        };
        if status != 0 {
            return Some(Vec::new());
        }
        let mut ids = vec![0u32; size as usize / std::mem::size_of::<u32>()];
        let mut bytes = (ids.len() * std::mem::size_of::<u32>()) as u32;
        // SAFETY: `ids` holds `bytes` bytes of object ids.
        let status = unsafe {
            AudioObjectGetPropertyData(
                SYSTEM_OBJECT,
                &list,
                0,
                std::ptr::null(),
                &mut bytes,
                ids.as_mut_ptr().cast(),
            )
        };
        if status != 0 {
            return Some(Vec::new());
        }
        ids.truncate(bytes as usize / std::mem::size_of::<u32>());
        Some(
            ids.into_iter()
                .filter(|&id| read::<u32>(id, PROCESS_IS_RUNNING_INPUT).unwrap_or(0) != 0)
                .filter_map(|id| {
                    Some(InputProcess {
                        pid: read::<i32>(id, PROCESS_PID)?,
                        bundle_id: bundle_id(id),
                    })
                })
                .collect(),
        )
    }

    /// The process's bundle identifier (a CFString the caller releases).
    fn bundle_id(process: u32) -> Option<String> {
        let cf = read::<usize>(process, PROCESS_BUNDLE_ID)? as *const c_void;
        if cf.is_null() {
            return None;
        }
        // SAFETY: a CFStringRef, toll-free bridged to NSString; read, then
        // released once (Core Audio returned it retained).
        unsafe {
            let text = (*cf.cast::<objc2_foundation::NSString>()).to_string();
            super::ffi::CFRelease(cf);
            Some(text).filter(|t| !t.is_empty())
        }
    }

    /// The default input device is running for some process (macOS 13).
    pub fn default_input_running() -> bool {
        read::<u32>(SYSTEM_OBJECT, DEFAULT_INPUT_DEVICE)
            .filter(|&device| device != 0)
            .and_then(|device| read::<u32>(device, DEVICE_IS_RUNNING_SOMEWHERE))
            .is_some_and(|running| running != 0)
    }
}

// ---------------------------------------------------------------------------
// Not yet on macOS
// ---------------------------------------------------------------------------

pub fn peripheral_add(
    _service: &super::PeripheralService,
    _events: super::PeripheralEvents,
) -> Result<(), String> {
    Err("The watch pod only works on Windows for now.".to_string())
}

pub fn peripheral_notify(_uuid: u128, _value: &[u8]) -> Result<(), String> {
    Ok(())
}

pub fn peripheral_remove(_service: u128) {}

pub fn peripheral_stop() {}
