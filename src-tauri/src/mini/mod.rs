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
/// Cue's own window is in front (its last focus event): the mini window waits, hidden. Kept from the events
/// themselves: asked of the window just as it goes to the back, it can still say it's in front.
static MAIN_FRONT: AtomicBool = AtomicBool::new(false);
/// Its page has sized it at least once (a window never sized isn't shown: it would flash at 4 × 200).
static SIZED: AtomicBool = AtomicBool::new(false);

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
    SIZED.store(false, Ordering::SeqCst);
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

/// The screen the mini window goes on (your main display), in points: its left, top, width and height.
fn screen(w: &tauri::WebviewWindow) -> Option<(f64, f64, f64, f64)> {
    let m = w.primary_monitor().ok().flatten()?;
    let s = m.scale_factor();
    let (p, z) = (m.position(), m.size());
    Some((p.x as f64 / s, p.y as f64 / s, z.width as f64 / s, z.height as f64 / s))
}

/// Room kept free above and below the rail's panels: the menu bar, then a margin.
const RAIL_TOP: f64 = 37.0 + 24.0;
const RAIL_BOTTOM: f64 = 24.0;

/// Where a mini window of this size goes: the rail against the right edge, centred on `at` (a height on
/// the screen, the dot you opened) or the screen's middle, kept clear of the menu bar and the bottom; the
/// notch's island centred at the top.
fn place(mode: Mode, (x, y, sw, sh): (f64, f64, f64, f64), w: f64, h: f64, at: Option<f64>) -> (f64, f64) {
    match mode {
        Mode::Notch => (x + (sw - w) / 2.0, y),
        _ => {
            let mid = at.unwrap_or(sh / 2.0);
            let top = (mid - h / 2.0).clamp(RAIL_TOP, (sh - RAIL_BOTTOM - h).max(RAIL_TOP));
            (x + sw - w, y + top)
        }
    }
}

/// The page's size for what it shows now (a hairline, the dots, the list, the drawer), in points, and where
/// its middle should sit (`at`, the rail only). `show` false hides it (nothing to show).
#[tauri::command]
pub(crate) async fn mini_shape(app: AppHandle, w: f64, h: f64, at: Option<f64>, show: bool) -> Result<(), String> {
    let win = app.get_webview_window(LABEL).ok_or("no mini window")?;
    if !show {
        return win.hide().map_err(|e| e.to_string());
    }
    let mode = if OPEN.load(Ordering::SeqCst) == Mode::Notch as u8 { Mode::Notch } else { Mode::Rail };
    let scr = screen(&win).ok_or("no screen")?;
    let (w, h) = (w.clamp(1.0, scr.2), h.clamp(1.0, scr.3));
    let (x, y) = place(mode, scr, w, h, at);
    #[cfg(debug_assertions)]
    eprintln!("[cue] mini window {w}×{h} at {x},{y}");
    win.set_size(tauri::LogicalSize::new(w, h)).map_err(|e| e.to_string())?;
    win.set_position(tauri::LogicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    SIZED.store(true, Ordering::SeqCst);
    if !MAIN_FRONT.load(Ordering::SeqCst) {
        win.show().map_err(|e| e.to_string())?;
    }
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

/// Cue's window came forward (hide the mini window) or went back (show it again, as its page last sized it).
pub(crate) fn main_focused(app: &AppHandle, focused: bool) {
    MAIN_FRONT.store(focused, Ordering::SeqCst);
    let Some(win) = app.get_webview_window(LABEL) else { return };
    if focused {
        let _ = win.hide();
    } else if SIZED.load(Ordering::SeqCst) {
        let _ = win.show();
    }
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
    fn the_rail_sits_on_the_right_edge_centred_on_its_dot() {
        assert_eq!(place(Mode::Rail, SCREEN, 420.0, 442.0, Some(500.0)), (1092.0, 279.0));
    }

    #[test]
    fn the_rail_stays_clear_of_the_menu_bar_and_the_bottom() {
        assert_eq!(place(Mode::Rail, SCREEN, 420.0, 442.0, Some(40.0)).1, RAIL_TOP);
        assert_eq!(place(Mode::Rail, SCREEN, 420.0, 442.0, Some(980.0)).1, 982.0 - RAIL_BOTTOM - 442.0);
    }

    #[test]
    fn the_notch_island_is_centred_at_the_top() {
        assert_eq!(place(Mode::Notch, SCREEN, 560.0, 324.0, Some(500.0)), (476.0, 0.0));
    }
}
