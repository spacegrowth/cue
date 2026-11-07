//! The mini window: Cue at the edge of your screen while you work in another app, so a session that needs you
//! can be read and answered without bringing the whole window forward. Settings → Mini window picks its shape:
//! Off, the right edge (a rail: a hairline, dots for what waits on you, a drawer with the chat) or the notch (a
//! dark island that drops the same list and chat). The page is `ui/mini.html`; it gets the same `state` the
//! window gets and calls the same commands (reply, respond), so nothing here knows about sessions.
//!
//! The window never takes the app forward: a click in it leaves the app you're in active (macos.rs), and it
//! takes the keyboard only while you're typing in its box. It hides while Cue's own window is in front.

#[cfg(target_os = "macos")]
mod macos;

use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The mini window's label (its page asks for it by this name too).
pub(crate) const LABEL: &str = "mini";

/// Where the mini window lives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Off = 0,
    Rail = 1,
    Notch = 2,
}

/// The shape open now (Off: none).
static OPEN: AtomicU8 = AtomicU8::new(Mode::Off as u8);
/// Cue's own window is in front: the mini window steps aside. Kept from that window's focus events (asked of
/// the window just as it goes to the back, it can still say it's in front).
static MAIN_FRONT: AtomicBool = AtomicBool::new(false);

impl Mode {
    fn from_setting(v: Option<&str>) -> Mode {
        match v {
            Some("rail") => Mode::Rail,
            Some("notch") => Mode::Notch,
            _ => Mode::Off,
        }
    }
    fn page(self) -> &'static str {
        if self == Mode::Notch { "mini.html#notch" } else { "mini.html#rail" }
    }
}

/// Settings → Mini window (`mini.mode`): "off" (the default), "rail" or "notch".
pub(crate) fn mode() -> Mode {
    Mode::from_setting(crate::config::load().pointer("/mini/mode").and_then(Value::as_str))
}

/// The mini window as the setting has it now: opened, swapped for the other shape, or closed.
pub(crate) fn apply(app: &AppHandle) {
    let want = mode();
    let open = OPEN.load(Ordering::SeqCst);
    if open == want as u8 && (want == Mode::Off || app.get_webview_window(LABEL).is_some()) {
        return;
    }
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
    OPEN.store(Mode::Off as u8, Ordering::SeqCst);
    if want == Mode::Off || std::env::var_os("CUE_QUIET").is_some() {
        return;
    }
    match open_window(app, want) {
        Ok(()) => OPEN.store(want as u8, Ordering::SeqCst),
        Err(e) => eprintln!("[cue] mini window: {e}"),
    }
}

/// The window, made: no frame, see-through round its shapes, over other windows on every Space. Its page sizes
/// it (`mini_shape`) once it knows what it shows; until then it's a hairline, hidden.
fn open_window(app: &AppHandle, mode: Mode) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(mode.page().into()))
        .title("Cue")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .accept_first_mouse(true)
        .visible(false)
        .inner_size(4.0, 200.0)
        .build()?;
    #[cfg(target_os = "macos")]
    macos::settle(&w, mode == Mode::Notch);
    Ok(())
}

/// The main display's height in points: AppKit counts heights up from its bottom edge.
#[cfg(target_os = "macos")]
fn primary_height(w: &tauri::WebviewWindow) -> f64 {
    w.primary_monitor().ok().flatten().map(|m| points(&m).3).unwrap_or(0.0)
}

/// A display's left, top, width and height, in points.
fn points(m: &tauri::Monitor) -> (f64, f64, f64, f64) {
    let s = m.scale_factor();
    let (p, z) = (m.position(), m.size());
    (p.x as f64 / s, p.y as f64 / s, z.width as f64 / s, z.height as f64 / s)
}

/// The display the mini window goes on: the notch's island on the main display (the built-in one, with the
/// notch); the rail on the display Cue's own window is on (where you look for Cue), else the main display.
fn screen(app: &AppHandle, w: &tauri::WebviewWindow, mode: Mode) -> Option<(f64, f64, f64, f64)> {
    let main = w.primary_monitor().ok().flatten().map(|m| points(&m));
    if mode == Mode::Notch {
        return main;
    }
    let cue = app.get_webview_window("main").and_then(|m| m.current_monitor().ok().flatten()).map(|m| points(&m));
    cue.or(main)
}

/// Room kept free above and below the rail's panels: the menu bar, then a margin.
const RAIL_TOP: f64 = 37.0 + 24.0;
const RAIL_BOTTOM: f64 = 24.0;

/// Where a mini window of this size goes. The rail: against the right edge, its top always on the same line
/// (a third of the way down), whatever it shows, so it never moves: the dots, the list and the chat all grow
/// down from there (up only if the chat wouldn't fit below). The notch's island: centred at the top.
fn place(mode: Mode, (x, y, sw, sh): (f64, f64, f64, f64), w: f64, h: f64) -> (f64, f64) {
    match mode {
        Mode::Notch => (x + (sw - w) / 2.0, y),
        _ => {
            let top = (sh / 3.0).min(sh - RAIL_BOTTOM - h).max(RAIL_TOP);
            (x + sw - w, y + top)
        }
    }
}

/// The page's size for what it shows now (a hairline, the dots, the list, the drawer), in points. `show`
/// false hides it (nothing to show).
#[tauri::command]
pub(crate) async fn mini_shape(app: AppHandle, w: f64, h: f64, show: bool) -> Result<(), String> {
    let win = app.get_webview_window(LABEL).ok_or("no mini window")?;
    if !show {
        return win.hide().map_err(|e| e.to_string());
    }
    let mode = if OPEN.load(Ordering::SeqCst) == Mode::Notch as u8 { Mode::Notch } else { Mode::Rail };
    let scr = screen(&app, &win, mode).ok_or("no screen")?;
    let (w, h) = (w.clamp(1.0, scr.2), h.clamp(1.0, scr.3));
    let (x, y) = place(mode, scr, w, h);
    #[cfg(debug_assertions)]
    eprintln!("[cue] mini window {w}×{h} at {x},{y}");
    // Moved and sized at once (its page animates what it shows).
    #[cfg(target_os = "macos")]
    macos::frame(&win, (x, y, w, h), primary_height(&win));
    #[cfg(not(target_os = "macos"))]
    {
        win.set_size(tauri::LogicalSize::new(w, h)).map_err(|e| e.to_string())?;
        win.set_position(tauri::LogicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    }
    // Not while Cue's own window is in front (it would only repeat it). Shown without ever taking the keyboard:
    // Tauri's show() makes it the key window, and when that ran each time Cue's window came forward, the two
    // took focus from each other and you couldn't type in Cue.
    if MAIN_FRONT.load(Ordering::SeqCst) {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    macos::show_quietly(&win);
    #[cfg(not(target_os = "macos"))]
    win.show().map_err(|e| e.to_string())?;
    Ok(())
}

/// The mini window takes the keyboard (you clicked into its box) or hands it back to the app you were in.
#[tauri::command]
pub(crate) async fn mini_key(app: AppHandle, on: bool) -> Result<(), String> {
    let win = app.get_webview_window(LABEL).ok_or("no mini window")?;
    #[cfg(target_os = "macos")]
    macos::key(&win, on);
    #[cfg(not(target_os = "macos"))]
    let _ = (win, on);
    Ok(())
}

/// Cue's own window, on a session's chat (the mini window's ⤢) or as it was (Open Cue).
#[tauri::command]
pub(crate) async fn mini_open_in_cue(app: AppHandle, session_id: Option<String>) -> Result<(), String> {
    crate::show_main(&app, session_id.filter(|s| !s.is_empty()).map(|s| format!("s:{s}")));
    Ok(())
}

/// Cue's window came forward (the mini window steps aside: hidden, which never takes the keyboard) or went
/// back (its page places it again, on the display Cue's window is on now, and shows it: mini_shape).
pub(crate) fn main_focused(app: &AppHandle, focused: bool) {
    MAIN_FRONT.store(focused, Ordering::SeqCst);
    if focused {
        if let Some(win) = app.get_webview_window(LABEL) {
            let _ = win.hide();
        }
        return;
    }
    use tauri::Emitter;
    let _ = app.emit_to(LABEL, "mini-place", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (f64, f64, f64, f64) = (0.0, 0.0, 1512.0, 982.0);

    #[test]
    fn the_setting_names_the_shape() {
        assert_eq!(Mode::from_setting(Some("rail")), Mode::Rail);
        assert_eq!(Mode::from_setting(Some("notch")), Mode::Notch);
        assert_eq!(Mode::from_setting(Some("other")), Mode::Off);
        assert_eq!(Mode::from_setting(None), Mode::Off);
    }

    #[test]
    fn the_rail_keeps_one_top_whatever_it_shows() {
        let top = |w, h| place(Mode::Rail, SCREEN, w, h).1;
        assert_eq!(top(4.0, 200.0), 982.0 / 3.0);
        assert_eq!(top(28.0, 130.0), 982.0 / 3.0, "dots");
        assert_eq!(top(320.0, 300.0), 982.0 / 3.0, "the list");
        assert_eq!(top(448.0, 442.0), 982.0 / 3.0, "the chat, at its cap");
        assert_eq!(place(Mode::Rail, SCREEN, 28.0, 130.0).0, 1512.0 - 28.0, "against the right edge");
    }

    #[test]
    fn the_rail_goes_up_only_when_it_wouldnt_fit_below_and_never_under_the_menu_bar() {
        assert_eq!(place(Mode::Rail, SCREEN, 448.0, 800.0).1, 982.0 - RAIL_BOTTOM - 800.0);
        assert_eq!(place(Mode::Rail, SCREEN, 448.0, 982.0).1, RAIL_TOP);
    }

    #[test]
    fn on_a_display_left_of_or_above_another_it_still_takes_that_displays_own_right_edge() {
        let external = (1800.0, -567.0, 3008.0, 1692.0);
        assert_eq!(place(Mode::Rail, external, 28.0, 130.0), (1800.0 + 3008.0 - 28.0, -567.0 + 1692.0 / 3.0));
    }

    /// Placing or showing the mini window never takes the keyboard (Cue's own window lost it at once, every
    /// time it came forward): no show() (Tao's is makeKeyAndOrderFront) and no focus anywhere here, and the one
    /// makeKeyAndOrderFront is in `key`, which runs only on a click into its box.
    #[test]
    fn placing_the_mini_window_never_takes_the_keyboard() {
        let here = include_str!("mod.rs");
        let mac = include_str!("macos.rs");
        let code = |src: &'static str| src.split("#[cfg(test)]").next().unwrap();
        for (name, src) in [("mod.rs", code(here)), ("macos.rs", code(mac))] {
            for banned in ["set_focus(", "makeKeyWindow", "activateIgnoringOtherApps"] {
                assert!(!src.contains(banned), "{name} has {banned}");
            }
        }
        let shows: Vec<_> = code(here).lines().filter(|l| l.contains(".show()") && !l.trim_start().starts_with("//")).collect();
        assert!(shows.iter().all(|l| l.trim_start().starts_with("win.show().map_err")), "{shows:?}");
        assert_eq!(code(here).matches("win.show()").count(), 1, "show() only off macOS");
        let lines: Vec<&str> = code(here).lines().collect();
        let at = lines.iter().position(|l| l.contains("win.show()")).unwrap();
        assert!(lines[at - 1].contains(r#"#[cfg(not(target_os = "macos"))]"#), "the one show() is the non-macOS path: {}", lines[at - 1]);
        let key_fn = mac.split("pub(super) fn key(").nth(1).unwrap().split("\n}\n").next().unwrap();
        assert_eq!(code(mac).matches("makeKeyAndOrderFront").count(), 1);
        assert!(key_fn.contains("makeKeyAndOrderFront"), "only `key` makes it the key window");
    }

    #[test]
    fn the_notch_island_is_centred_at_the_top() {
        assert_eq!(place(Mode::Notch, SCREEN, 560.0, 324.0), (476.0, 0.0));
    }
}
