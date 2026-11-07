//! The window's side of starting and running: its first setup, the menu bar, what closing it does, and
//! what Cue does when it's reopened from the Dock or quits.

use crate::{actions, hub, locking};
use std::sync::Arc;
use tauri::{Manager, RunEvent, WindowEvent};

/// The window's first setup: the main thread watched, no WebRTC, the text size you chose.
pub(super) fn prepare(app: &tauri::App) {
    // A main thread that stops answering (the beachball) is logged with the locks held at the time.
    let main = app.handle().clone();
    locking::probe::watch_main(Box::new(move |f| main.run_on_main_thread(f).is_ok()));
    // No WebRTC in Cue's web view: Cue doesn't use it, and it's the one way out an extension's page
    // would otherwise have (a STUN or TURN request to a server of its choosing, which no page policy
    // stops). Extensions can read your code (git.read); nothing they read may leave this Mac.
    #[cfg(target_os = "macos")]
    if let Some(w) = app.get_webview_window("main") {
        no_webrtc(&w);
    }
    crate::apply_zoom(app.handle());
}

/// Hide the window (a quiet test instance).
pub(super) fn hide(app: &tauri::App) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

/// Switch WebRTC off for this web view (WebKit's peer-connection preference), where WebKit has it.
#[cfg(target_os = "macos")]
fn no_webrtc(w: &tauri::WebviewWindow) {
    let _ = w.with_webview(|wv| unsafe {
        use objc2::runtime::AnyObject;
        use objc2::{msg_send, sel};
        let wk = wv.inner() as *mut AnyObject;
        if wk.is_null() {
            return;
        }
        let config: *mut AnyObject = msg_send![&*wk, configuration];
        let prefs: *mut AnyObject = if config.is_null() { std::ptr::null_mut() } else { msg_send![&*config, preferences] };
        if prefs.is_null() {
            return;
        }
        let has: bool = msg_send![&*prefs, respondsToSelector: sel!(_setPeerConnectionEnabled:)];
        if has {
            let _: () = msg_send![&*prefs, _setPeerConnectionEnabled: false];
        }
    });
}

/// The menu bar's menus: Tauri's default ones, but Close (⌘W) is Cue's own: it goes to the window
/// ("close-key"), where inside Cue's terminal it closes the tab you're in, elsewhere it hides the window
/// as before. (Cue's terminal's shells run in tmux: quitting never asks, they carry on.)
pub(super) fn app_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem as P, Submenu};
    let pkg = app.package_info();
    let about = AboutMetadata { name: Some(pkg.name.clone()), version: Some(pkg.version.to_string()), ..Default::default() };
    Menu::with_items(app, &[
        &Submenu::with_items(app, pkg.name.clone(), true, &[
            &P::about(app, None, Some(about))?,
            &P::separator(app)?,
            &P::services(app, None)?,
            &P::separator(app)?,
            &P::hide(app, None)?,
            &P::hide_others(app, None)?,
            &P::separator(app)?,
            &MenuItem::with_id(app, "app-quit", "Quit Cue", true, Some("CmdOrCtrl+Q"))?,
        ])?,
        &Submenu::with_items(app, "File", true, &[&MenuItem::with_id(app, "app-close", "Close", true, Some("CmdOrCtrl+W"))?])?,
        &Submenu::with_items(app, "Edit", true, &[&P::undo(app, None)?, &P::redo(app, None)?, &P::separator(app)?, &P::cut(app, None)?, &P::copy(app, None)?, &P::paste(app, None)?, &P::select_all(app, None)?])?,
        &Submenu::with_items(app, "View", true, &[&P::fullscreen(app, None)?])?,
        &Submenu::with_items(app, "Window", true, &[&P::minimize(app, None)?, &P::maximize(app, None)?])?,
    ])
}

/// Quit Cue (⌘Q), or pass Close (⌘W) to the window.
pub(super) fn on_menu_event(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "app-quit" => app.exit(0),
        "app-close" => {
            use tauri::Emitter;
            let _ = app.emit_to("main", "close-key", ());
        }
        _ => {}
    }
}

/// Closing the window keeps Cue running (it must stay up to receive requests). The mini window steps aside
/// while Cue's own window is in front.
pub(super) fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::Focused(focused) = event {
        if window.label() == "main" {
            crate::mini::main_focused(window.app_handle(), *focused);
        }
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        if window.label() != "main" {
            return;
        }
        api.prevent_close();
        let _ = window.hide();
    }
}

/// Cue quitting, or opened again from the Dock.
pub(crate) fn on_run_event(handle: &tauri::AppHandle, event: RunEvent) {
    match event {
        // Actions run without tmux are Cue's own children: they end with it.
        RunEvent::Exit => {
            actions::end_plain();
            // What changed since the last draw is saved before Cue goes.
            if let Some(hub) = handle.try_state::<Arc<hub::Hub>>() {
                hub.save_live_and_wait();
            }
        }
        RunEvent::Reopen { .. } => {
            if let Some(w) = handle.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        _ => {}
    }
}
