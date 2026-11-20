//! The Edit menu's Cut, Copy, Paste and Select All (⌘X ⌘C ⌘V ⌘A), guarded (#413).
//!
//! WebKit's own menu items hand the command straight to the web view that has the keyboard, and WebKit stops the
//! whole app (a trap, not an error) when that web view has no page in it yet: ⌘V four seconds after Cue started,
//! its window up but its page still loading, crashed Cue. So these four are Cue's own items: the command goes on
//! (to whatever has the keyboard, as before) only when the web view in front has its page loaded and running, and
//! is skipped otherwise. Undo and Redo don't go that way in WebKit and stay its own.

use crate::locking;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::Manager;

/// The Edit items that are Cue's own: (menu id, title, key, the AppKit action it sends).
pub(super) const ITEMS: [(&str, &str, &str, &str); 4] = [
    ("edit-cut", "Cut", "CmdOrCtrl+X", "cut:"),
    ("edit-copy", "Copy", "CmdOrCtrl+C", "copy:"),
    ("edit-paste", "Paste", "CmdOrCtrl+V", "paste:"),
    ("edit-select-all", "Select All", "CmdOrCtrl+A", "selectAll:"),
];

/// The windows (by label) whose page has finished loading, and not started again since.
static LOADED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// A window's page started loading (`false`) or finished (`true`).
pub(super) fn page_load(label: &str, finished: bool) {
    let mut loaded = locking::lock(&LOADED, "edit.loaded");
    let set = loaded.get_or_insert_with(HashSet::new);
    if finished {
        set.insert(label.to_string());
    } else {
        set.remove(label);
    }
}

fn loaded(label: &str) -> bool {
    locking::lock(&LOADED, "edit.loaded").as_ref().is_some_and(|s| s.contains(label))
}

/// Whether an Edit command may go on. `front`: the Cue window that has the keyboard, if any, as (its page loaded,
/// its page's process running). No Cue window in front (the About box, say): it goes on, nothing of WebKit's is hit.
pub(super) fn may_send(front: Option<(bool, bool)>) -> bool {
    front.is_none_or(|(loaded, running)| loaded && running)
}

/// One of `ITEMS` chosen: sent on to what has the keyboard, or skipped (and logged) when the page isn't ready.
/// Returns false for a menu id that isn't one of them. The check and the send run together on the main thread,
/// where the web view is asked (the menu's own handler mustn't wait on it).
pub(super) fn on_menu(app: &tauri::AppHandle, id: &str) -> bool {
    let Some(&(_, _, _, action)) = ITEMS.iter().find(|i| i.0 == id) else { return false };
    let Some((label, w)) = app.webview_windows().into_iter().find(|(_, w)| w.is_focused().unwrap_or(false)) else {
        send(action);
        return true;
    };
    let is_loaded = loaded(&label);
    let asked = w.with_webview(move |wv| {
        let state = (is_loaded, running(wv.inner()));
        if may_send(Some(state)) {
            send(action);
        } else {
            eprintln!("cue: {action} skipped: {label} has no page loaded and running ({state:?})");
        }
    });
    if asked.is_err() {
        eprintln!("cue: {action} skipped: its window couldn't be reached");
    }
    true
}

/// The web view's page process is running (WebKit's process id for it isn't 0). Unknown (no such call): running.
#[cfg(target_os = "macos")]
fn running(wk: *mut std::ffi::c_void) -> bool {
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, sel};
    let wk = wk as *mut AnyObject;
    if wk.is_null() {
        return false;
    }
    unsafe {
        let has: bool = msg_send![&*wk, respondsToSelector: sel!(_webProcessIdentifier)];
        if !has {
            return true;
        }
        let pid: i32 = msg_send![&*wk, _webProcessIdentifier];
        pid != 0
    }
}
#[cfg(not(target_os = "macos"))]
fn running<T>(_: T) -> bool {
    true
}

/// The action to the first responder, as a menu item sends it.
#[cfg(target_os = "macos")]
fn send(action: &str) {
    use objc2::runtime::{AnyObject, Sel};
    use objc2::{class, msg_send};
    let Ok(name) = std::ffi::CString::new(action) else { return };
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let sel = Sel::register(&name);
        let nil: *mut AnyObject = std::ptr::null_mut();
        let _: bool = msg_send![&*app, sendAction: sel, to: nil, from: nil];
    }
}
#[cfg(not(target_os = "macos"))]
fn send(_: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edit_command_goes_on_only_to_a_page_that_is_loaded_and_running() {
        assert!(may_send(Some((true, true))));
        assert!(!may_send(Some((false, true))), "still loading: skipped, not crashed on");
        assert!(!may_send(Some((true, false))), "its page process gone: skipped");
        assert!(may_send(None), "no Cue window in front: goes on");
    }

    #[test]
    fn a_page_counts_as_loaded_from_finished_until_it_starts_again() {
        page_load("t-edit", false);
        assert!(!loaded("t-edit"));
        page_load("t-edit", true);
        assert!(loaded("t-edit"));
        page_load("t-edit", false);
        assert!(!loaded("t-edit"), "a reload: not loaded until it finishes");
    }

    /// WebKit's own Cut/Copy/Paste/Select All must not come back into the menu: they skip the guard.
    #[test]
    fn the_menu_uses_cues_own_cut_copy_paste_and_select_all() {
        let menu = include_str!("window.rs");
        for banned in ["P::cut(", "P::copy(", "P::paste(", "P::select_all("] {
            assert!(!menu.contains(banned), "{banned} is back in the menu: ⌘V on a page still loading crashes Cue");
        }
        assert!(menu.contains("edit::ITEMS"));
    }
}
