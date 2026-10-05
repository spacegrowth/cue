//! The side panel: a small curved pill (how many wait) on the edge of the screen. While something
//! needs you it sits there closed; click it and it slides out into the panel: what's waiting, with
//! Allow / Deny, answers and a reply box. Click outside (or ✕) and it
//! tucks back in. It hides when nothing waits.
//!
//! Drag the pill: it stays at the height you let go, against the nearer edge (left or right) of the
//! monitor you dropped it on, and remembers all three (`panel.side`, `panel.top`, `panel.screen_*`),
//! so it comes back there after Cue restarts (on the main screen if that monitor isn't connected). Open, it grows down from the pill when there's room
//! below, else up.
//!
//! The window is see-through (the curve is real), and AppKit animates every change of size, so it
//! grows out from the edge and back instead of jumping.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// Closed: the tab's width (its height is what the page reports). Open: the panel's width.
const TAB_W: f64 = 46.0;
const OPEN_W: f64 = 380.0;
/// Before you've dragged it: this far below the top of the usable screen, on the right.
const DEFAULT_TOP: f64 = 24.0;
/// How long it takes to slide open or shut.
const SLIDE_SECS: f64 = 0.16;
/// NSFloatingWindowLevel, and on every Space, over full-screen apps too.
const LEVEL_FLOATING: isize = 3;
const ON_EVERY_SPACE: usize = 1 | 16 | 64 | 256;

/// What it shows now (so a drop keeps it open or closed) and the closed tab's height.
static OPEN: AtomicBool = AtomicBool::new(false);
static TAB_H: Mutex<f64> = Mutex::new(96.0);
/// A drag in progress (the page doesn't resize the window until the drop).
static DRAGGING: AtomicBool = AtomicBool::new(false);

fn ns_window(w: &tauri::WebviewWindow) -> Option<&'static AnyObject> {
    // SAFETY: ns_window() is this window's live NSWindow, used only on the main thread below.
    w.ns_window().ok().map(|p| unsafe { &*(p as *mut AnyObject) })
}

/// The usable part of its screen (below the menu bar, beside the Dock), Cocoa points: the monitor you
/// dragged it to while that's connected, else the one it's on, else the main screen.
fn usable(win: &AnyObject) -> NSRect {
    unsafe {
        let screen = match remembered_screen() {
            Some(s) => s,
            None => {
                let on: Option<Retained<AnyObject>> = msg_send![win, screen];
                match on {
                    Some(s) => s,
                    None => msg_send![class!(NSScreen), mainScreen],
                }
            }
        };
        msg_send![&*screen, visibleFrame]
    }
}

/// A monitor's display number (stays the same while it's connected the same way) and its name (for
/// when the number changed, e.g. after a reboot).
fn screen_id(screen: &AnyObject) -> (u64, String) {
    unsafe {
        let desc: Retained<AnyObject> = msg_send![screen, deviceDescription];
        let key = NSString::from_str("NSScreenNumber");
        let num: Option<Retained<AnyObject>> = msg_send![&*desc, objectForKey: &*key];
        let id: u64 = match num {
            Some(n) => msg_send![&*n, unsignedLongLongValue],
            None => 0,
        };
        let name: Option<Retained<NSString>> = msg_send![screen, localizedName];
        (id, name.map(|n| n.to_string()).unwrap_or_default())
    }
}

/// The monitor you dragged it to, if it's connected: same display number, else same name.
fn remembered_screen() -> Option<Retained<AnyObject>> {
    let (id, name) = crate::config::panel_screen()?;
    let screens: Vec<Retained<AnyObject>> = unsafe {
        let all: Retained<AnyObject> = msg_send![class!(NSScreen), screens];
        let n: usize = msg_send![&*all, count];
        (0..n).map(|i| msg_send![&*all, objectAtIndex: i]).collect()
    };
    let ids: Vec<(u64, String)> = screens.iter().map(|s| screen_id(s)).collect();
    let at = ids.iter().position(|(i, _)| id != 0 && *i == id).or_else(|| ids.iter().position(|(_, n)| !name.is_empty() && *n == name))?;
    screens.into_iter().nth(at)
}

/// Where the window goes, as (x, y, w, h) in Cocoa points (origin bottom-left): on its side of
/// `screen`, the pill's top `top` below the screen's top. Open, it grows down from the pill when
/// there's room, else up from the pill's bottom; always on screen.
fn frame(screen: (f64, f64, f64, f64), left: bool, top: f64, open: bool, height: f64, tab_h: f64) -> (f64, f64, f64, f64) {
    let (sx, sy, sw, sh) = screen;
    let w = if open { OPEN_W } else { TAB_W };
    let h = height.clamp(if open { 120.0 } else { 30.0 }, (sh - 16.0).max(120.0));
    let top = top.clamp(0.0, (sh - tab_h).max(0.0));
    let from_top = if !open || top + h <= sh { top } else { (top + tab_h - h).max(0.0) };
    let from_top = from_top.min((sh - h).max(0.0));
    let x = if left { sx } else { sx + sw - w };
    (x, sy + sh - from_top - h, w, h)
}

fn apply(win: &AnyObject, open: bool, height: f64, animate: bool) {
    let vf = usable(win);
    let (left, top) = crate::config::panel_edge();
    let tab_h = *TAB_H.lock().unwrap();
    let (x, y, w, h) = frame((vf.origin.x, vf.origin.y, vf.size.width, vf.size.height), left, top.unwrap_or(DEFAULT_TOP), open, height, tab_h);
    unsafe {
        let _: () = msg_send![win, setLevel: LEVEL_FLOATING];
        let _: () = msg_send![win, setCollectionBehavior: ON_EVERY_SPACE];
        let _: () = msg_send![win, setHasShadow: true];
        let rect = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
        if animate {
            // A short, fixed slide: AppKit's own resize animation gets slower the farther it goes.
            let ctx = class!(NSAnimationContext);
            let _: () = msg_send![ctx, beginGrouping];
            let current: Retained<AnyObject> = msg_send![ctx, currentContext];
            let _: () = msg_send![&*current, setDuration: SLIDE_SECS];
            let animator: Retained<AnyObject> = msg_send![win, animator];
            let _: () = msg_send![&*animator, setFrame: rect, display: true];
            let _: () = msg_send![ctx, endGrouping];
        } else {
            let _: () = msg_send![win, setFrame: rect, display: true];
        }
    }
}

/// Show it closed while something needs you; hide it otherwise.
pub fn sync(app: &AppHandle, want_visible: bool) {
    let Some(w) = app.get_webview_window("mini") else { return };
    let visible = w.is_visible().unwrap_or(false);
    if want_visible && !visible {
        let w2 = w.clone();
        let _ = w.run_on_main_thread(move || {
            let Some(win) = ns_window(&w2) else { return };
            OPEN.store(false, Ordering::Relaxed);
            // Read it first: apply() takes TAB_H itself (a guard held across the call deadlocked Cue).
            let tab_h = *TAB_H.lock().unwrap();
            apply(win, false, tab_h, false);
            // Never take the keyboard from what you're typing in.
            crate::tray::show_passive(&w2);
        });
    } else if !want_visible && visible {
        let _ = w.hide();
    }
}

/// The page asks: open or closed, `height` tall, animated. Returns which side it's on (the curve
/// faces the screen). Called from a sync command: main thread.
pub fn size(app: &AppHandle, open: bool, height: f64) -> bool {
    OPEN.store(open, Ordering::Relaxed);
    if !open {
        *TAB_H.lock().unwrap() = height;
    }
    if let Some(win) = app.get_webview_window("mini").as_ref().and_then(ns_window) {
        if !DRAGGING.load(Ordering::Relaxed) {
            apply(win, open, height, true);
        }
    }
    crate::config::panel_edge().0
}

/// You pressed on the pill (or the open panel's header) and moved: macOS drags the window. The drop is
/// when the mouse button comes up, checked ~30 times a second (macOS's window drag doesn't tell the
/// page, and "stopped moving" fired mid-drag).
pub fn drag(app: &AppHandle) {
    let Some(w) = app.get_webview_window("mini") else { return };
    if !DRAGGING.swap(true, Ordering::Relaxed) {
        let app = app.clone();
        std::thread::spawn(move || {
            let started = crate::model::now_ms();
            while mouse_down() || crate::model::now_ms().saturating_sub(started) < 150 {
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            DRAGGING.store(false, Ordering::Relaxed);
            snap(&app);
        });
    }
    let _ = w.start_dragging();
}

/// Is the left mouse button held right now? (NSEvent.pressedMouseButtons: safe from any thread.)
fn mouse_down() -> bool {
    let buttons: usize = unsafe { msg_send![class!(NSEvent), pressedMouseButtons] };
    buttons & 1 != 0
}

/// After a drop: flush to the nearer edge at the height you let go; remember it, and tell the page
/// which side (its curve flips).
fn snap(app: &AppHandle) {
    let Some(w) = app.get_webview_window("mini") else { return };
    let w2 = w.clone();
    let _ = w.run_on_main_thread(move || {
        let Some(win) = ns_window(&w2) else { return };
        // The monitor it was dropped on: remembered first, so the edge below is that monitor's.
        let on: Option<Retained<AnyObject>> = unsafe { msg_send![win, screen] };
        if let Some(screen) = on {
            let (id, name) = screen_id(&screen);
            let _ = crate::config::remember("panel.screen_id", serde_json::json!(id));
            let _ = crate::config::remember("panel.screen_name", serde_json::json!(name));
        }
        let vf = usable(win);
        let f: NSRect = unsafe { msg_send![win, frame] };
        let left = f.origin.x + f.size.width / 2.0 < vf.origin.x + vf.size.width / 2.0;
        // Open, the pill is the window's top unless it opened upward: keep the pill where it was.
        let mut top = (vf.origin.y + vf.size.height) - (f.origin.y + f.size.height);
        if OPEN.load(Ordering::Relaxed) && f.origin.y <= vf.origin.y + 1.0 {
            top += f.size.height - *TAB_H.lock().unwrap();
        }
        let _ = crate::config::remember("panel.side", serde_json::json!(if left { "left" } else { "right" }));
        let _ = crate::config::remember("panel.top", serde_json::json!(top.max(0.0).round()));
        apply(win, OPEN.load(Ordering::Relaxed), f.size.height, true);
        let _ = w2.emit_to("mini", "mini-edge", left);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (f64, f64, f64, f64) = (0.0, 0.0, 1512.0, 945.0);

    #[test]
    fn it_stays_at_its_height_and_opens_down_when_there_is_room() {
        assert_eq!(frame(SCREEN, false, 300.0, false, 34.0, 34.0), (1466.0, 611.0, 46.0, 34.0));
        assert_eq!(frame(SCREEN, false, 300.0, true, 500.0, 34.0), (1132.0, 145.0, 380.0, 500.0));
        assert_eq!(frame(SCREEN, true, 300.0, false, 34.0, 34.0).0, 0.0, "left edge: flush");
    }

    #[test]
    fn near_the_bottom_it_opens_upward_and_never_leaves_the_screen() {
        // Pill at 850 (bottom at 884): no room for 500 below, so the panel's bottom is the pill's.
        let (_, y, _, h) = frame(SCREEN, false, 850.0, true, 500.0, 34.0);
        assert_eq!((945.0 - y - h, h), (384.0, 500.0));
        assert_eq!(frame(SCREEN, false, 5000.0, false, 34.0, 34.0).1, 0.0, "dropped below the screen: kept on it");
        assert_eq!(frame(SCREEN, false, 0.0, true, 2000.0, 34.0).1, 16.0, "taller than the screen: fits");
    }
}
