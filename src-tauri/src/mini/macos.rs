//! The mini window's macOS side: a click in it never brings Cue forward (the app you're in stays active), it
//! floats on every Space and beside full-screen apps, out of ⌘` and Mission Control, and it takes the keyboard
//! only while you type in it.
//!
//! A click in it brings forward that window alone (not Cue's main window), so its box can take the cursor;
//! Esc, Send-and-close or a click outside hands the keyboard back to the app you were in. (Not an NSPanel:
//! swapping Tao's window class for one crashed Cue when its main window was clicked.)

use objc2::runtime::AnyObject;
use objc2::msg_send;
use std::sync::atomic::{AtomicI32, Ordering};

/// Over other windows: the rail floats; the notch's island goes over the menu bar (a pop-up menu's level).
const FLOATING: isize = 3;
const OVER_MENU_BAR: isize = 101;
/// NSWindowCollectionBehavior: on every Space, stationary in Exposé, out of ⌘`, beside full-screen apps.
const EVERYWHERE: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);

/// The NSWindow, carried to the main thread (AppKit's windows are touched only there).
struct Win(*mut AnyObject);
unsafe impl Send for Win {}
impl Win {
    /// The window (a method, so a closure takes the whole `Win` across, not its bare pointer).
    fn ptr(&self) -> *mut AnyObject {
        self.0
    }
}

/// The app you were in when the mini window last took the keyboard (its pid), given it back after.
static BEFORE: AtomicI32 = AtomicI32::new(0);

/// Settle a new mini window: its level and its Spaces.
pub(super) fn settle(w: &tauri::WebviewWindow, notch: bool) {
    let Ok(ns) = w.ns_window() else { return };
    let win = Win(ns as *mut AnyObject);
    let _ = w.run_on_main_thread(move || unsafe {
        let win = &*win.ptr();
        let _: () = msg_send![win, setLevel: if notch { OVER_MENU_BAR } else { FLOATING }];
        let _: () = msg_send![win, setCollectionBehavior: EVERYWHERE];
        let _: () = msg_send![win, setHidesOnDeactivate: false];
    });
}

/// The mini window takes the keyboard (`on`) or gives it back to the app you were in.
pub(super) fn key(w: &tauri::WebviewWindow, on: bool) {
    let Ok(ns) = w.ns_window() else { return };
    let win = Win(ns as *mut AnyObject);
    let _ = w.run_on_main_thread(move || unsafe {
        let win = &*win.ptr();
        if on {
            let before = frontmost_pid();
            if before != std::process::id() as i32 {
                BEFORE.store(before, Ordering::SeqCst);
            }
            let _: () = msg_send![win, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
            // Cue active with this window alone in front (not every window: the main one stays where it is).
            if let Some(cls) = objc2::runtime::AnyClass::get(c"NSRunningApplication") {
                let me: *mut AnyObject = msg_send![cls, currentApplication];
                if !me.is_null() {
                    let _: bool = msg_send![&*me, activateWithOptions: 0usize];
                }
            }
        } else {
            // The app you were in active again, which takes the keyboard from this window. Never by ordering
            // this window out and back: Cue was still the active app then, so AppKit made its main window key
            // and brought it forward, over the app you were in. With no app to go back to, Cue steps back.
            if !give_back(BEFORE.swap(0, Ordering::SeqCst)) {
                let app: *mut AnyObject = msg_send![objc2::runtime::AnyClass::get(c"NSApplication").expect("AppKit"), sharedApplication];
                if !app.is_null() {
                    let _: () = msg_send![&*app, deactivate];
                }
            }
        }
    });
}

/// The pid of the app in front now (0: none known).
unsafe fn frontmost_pid() -> i32 {
    let Some(ws) = objc2::runtime::AnyClass::get(c"NSWorkspace") else { return 0 };
    let shared: *mut AnyObject = msg_send![ws, sharedWorkspace];
    if shared.is_null() {
        return 0;
    }
    let app: *mut AnyObject = msg_send![&*shared, frontmostApplication];
    if app.is_null() {
        return 0;
    }
    msg_send![&*app, processIdentifier]
}

/// The app `pid` active again: Cue took its place while you typed.
/// False when there was none to go back to.
unsafe fn give_back(pid: i32) -> bool {
    if pid <= 0 || pid == std::process::id() as i32 {
        return false;
    }
    let Some(cls) = objc2::runtime::AnyClass::get(c"NSRunningApplication") else { return false };
    let app: *mut AnyObject = msg_send![cls, runningApplicationWithProcessIdentifier: pid];
    if app.is_null() {
        return false;
    }
    // NSApplicationActivateAllWindows is not wanted: only its front window, as a click would.
    msg_send![&*app, activateWithOptions: 0usize]
}

/// Move and size the window to `(x, y, w, h)` (points, from the top left of the main display), at once: the
/// window is see-through, and its page slides the panel in (mini.css), which is smooth where animating the
/// window's own frame wasn't (the page redrew at its final size every frame). `main_h`: the main display's
/// height (AppKit's frames count up from its bottom).
pub(super) fn frame(w: &tauri::WebviewWindow, (x, y, wd, ht): (f64, f64, f64, f64), main_h: f64) {
    let Ok(ns) = w.ns_window() else { return };
    let win = Win(ns as *mut AnyObject);
    let rect = objc2_foundation::NSRect::new(objc2_foundation::NSPoint::new(x, main_h - y - ht), objc2_foundation::NSSize::new(wd, ht));
    let _ = w.run_on_main_thread(move || unsafe {
        let _: () = msg_send![&*win.ptr(), setFrame: rect, display: true];
    });
}

/// In front of other windows without becoming the key window or activating Cue (`orderFrontRegardless`), and
/// only when it isn't already: the app you're in, or Cue's own window, keeps the keyboard.
pub(super) fn show_quietly(w: &tauri::WebviewWindow) {
    let Ok(ns) = w.ns_window() else { return };
    let win = Win(ns as *mut AnyObject);
    let _ = w.run_on_main_thread(move || unsafe {
        let win = &*win.ptr();
        let visible: bool = msg_send![win, isVisible];
        if !visible {
            let _: () = msg_send![win, orderFrontRegardless];
        }
    });
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
}

/// The left mouse button is down now (any thread, no permission needed).
pub(super) fn pressed() -> bool {
    // kCGEventSourceStateCombinedSessionState, kCGMouseButtonLeft.
    unsafe { CGEventSourceButtonState(0, 0) }
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const std::ffi::c_void);
}

/// Where the pointer is now, in points from the main display's top left (CoreGraphics' own coordinates, as
/// Cue's displays are given; any thread, no permission needed). (-1, -1) when it can't be read.
pub(super) fn pointer() -> (f64, f64) {
    unsafe {
        let e = CGEventCreate(std::ptr::null());
        if e.is_null() {
            return (-1.0, -1.0);
        }
        let p = CGEventGetLocation(e);
        CFRelease(e);
        (p.x, p.y)
    }
}
