//! The mini window's macOS side: a click in it never brings Cue forward (the app you're in stays active), it
//! floats on every Space and beside full-screen apps, out of ⌘` and Mission Control, and it takes the keyboard
//! only while you type in it.
//!
//! "Never brings Cue forward" is WindowServer's prevents-activation flag, the one a non-activating NSPanel
//! sets; Tao's window can't become an NSPanel (its class keeps state of its own), so the flag is set on the
//! window itself, where AppKit offers it (`_setPreventsActivation:`). Without it, a click activates Cue as any
//! window's would, and handing the keyboard back (`key(false)`) brings back the app you were in.

use objc2::runtime::AnyObject;
use objc2::{msg_send, sel};
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

/// Settle a new mini window: its level, its Spaces, and clicks that don't bring Cue forward.
pub(super) fn settle(w: &tauri::WebviewWindow, notch: bool) {
    let Ok(ns) = w.ns_window() else { return };
    let win = Win(ns as *mut AnyObject);
    let _ = w.run_on_main_thread(move || unsafe {
        let win = &*win.ptr();
        let _: () = msg_send![win, setLevel: if notch { OVER_MENU_BAR } else { FLOATING }];
        let _: () = msg_send![win, setCollectionBehavior: EVERYWHERE];
        let _: () = msg_send![win, setHidesOnDeactivate: false];
        let has: bool = msg_send![win, respondsToSelector: sel!(_setPreventsActivation:)];
        if has {
            let _: () = msg_send![win, _setPreventsActivation: true];
        }
    });
}

/// The mini window takes the keyboard (`on`) or gives it back to the app you were in.
pub(super) fn key(w: &tauri::WebviewWindow, on: bool) {
    let Ok(ns) = w.ns_window() else { return };
    let win = Win(ns as *mut AnyObject);
    let _ = w.run_on_main_thread(move || unsafe {
        let win = &*win.ptr();
        if on {
            BEFORE.store(frontmost_pid(), Ordering::SeqCst);
            let _: () = msg_send![win, makeKeyWindow];
        } else {
            // Out and back in: the window stops being key, and the app you're in has the keyboard again.
            let _: () = msg_send![win, orderOut: std::ptr::null::<AnyObject>()];
            let _: () = msg_send![win, orderFrontRegardless];
            give_back(BEFORE.swap(0, Ordering::SeqCst));
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

/// The app `pid` active again, when Cue took its place (a Mac without the prevents-activation flag).
unsafe fn give_back(pid: i32) {
    if pid <= 0 || pid == std::process::id() as i32 {
        return;
    }
    let Some(cls) = objc2::runtime::AnyClass::get(c"NSRunningApplication") else { return };
    let app: *mut AnyObject = msg_send![cls, runningApplicationWithProcessIdentifier: pid];
    if !app.is_null() {
        // NSApplicationActivateAllWindows is not wanted: only its front window, as a click would.
        let _: bool = msg_send![&*app, activateWithOptions: 0usize];
    }
}
