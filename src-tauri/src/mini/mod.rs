//! The mini window: Cue at the edge of your screen while you work in another app, so a session that needs you
//! can be read and answered without bringing the whole window forward. Settings → Mini window picks its shape:
//! Off, the right edge (the rail: a small tab tucked into the screen's right edge, as high as you dragged it,
//! that slides out into a panel with the chat, as picture in picture does) or the notch (a dark island that
//! drops the same chat). The page is `ui/mini.html`; it gets the same `state` the
//! window gets and calls the same commands (reply, respond), so nothing here knows about sessions.
//!
//! The window never takes the app forward: a click in it leaves the app you're in active (macos.rs), and it
//! takes the keyboard only while you're typing in its box. It hides while Cue's own window is in front.

#[cfg(target_os = "macos")]
mod macos;

use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
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
/// The display the rail is on, where your pointer settled last (its left, top, width, height as f64 bits;
/// width 0: none yet, so Cue's window's display). Kept by `follow`.
static ON: [AtomicU64; 4] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
/// Where the mini window is now (as mini_shape placed it; width 0: hidden), for the pointer watch: kept here so
/// it never asks the window (each ask is a round trip through the main thread).
static FRAME: [AtomicU64; 4] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
/// The window's frame when a drag of its bar began (mini_drag; width 0: no drag).
static DRAG_FROM: [AtomicU64; 4] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
/// The pointer-watching thread is running (started once, with the first mini window).
static FOLLOWING: AtomicBool = AtomicBool::new(false);

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
    store(&FRAME, (0.0, 0.0, 0.0, 0.0));
    if want == Mode::Off || std::env::var_os("CUE_QUIET").is_some() {
        return;
    }
    match open_window(app, want) {
        Ok(()) => OPEN.store(want as u8, Ordering::SeqCst),
        Err(e) => eprintln!("[cue] mini window: {e}"),
    }
    #[cfg(target_os = "macos")]
    if !FOLLOWING.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        let _ = std::thread::Builder::new().name("cue-mini-follow".into()).spawn(move || follow(app));
    }
}

/// How often the pointer's display is looked at, and how many looks in a row it must stay on a new one before
/// the rail moves there (a pointer crossing a display on its way doesn't move it).
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(700);
const SETTLED_LOOKS: u32 = 2;

/// How often the pointer is looked at while it's over the mini window, so its page hears it leave (WebKit tells a
/// window of an app that isn't in front nothing about the pointer).
const TRACK_EVERY: std::time::Duration = std::time::Duration::from_millis(120);

/// The pointer, watched from here (any thread, no permission needed):
/// - over the mini window, its place in the window goes to the page ("mini-pointer" {x, y}), every 120 ms, and
///   once it leaves, {out: true}: hover works whichever app is in front;
/// - elsewhere, one look every 700 ms: the rail moves to the display the pointer has settled on (mini-place).
#[cfg(target_os = "macos")]
fn follow(app: AppHandle) {
    use tauri::Emitter;
    let (mut seen, mut looks, mut over) = (None, 0u32, false);
    // The displays, asked again every few seconds (each ask is a round trip through the main thread).
    let (mut all, mut asked) = (Vec::new(), 0u32);
    loop {
        std::thread::sleep(if over { TRACK_EVERY } else { LOOK_EVERY });
        let open = OPEN.load(Ordering::SeqCst);
        let Some(win) = app.get_webview_window(LABEL).filter(|_| open != Mode::Off as u8) else { continue };
        let p = macos::pointer();
        let inside = load(&FRAME).and_then(|f| inside(f, p));
        match inside {
            Some((x, y)) => drop(app.emit_to(LABEL, "mini-pointer", serde_json::json!({ "x": x, "y": y }))),
            None if over => drop(app.emit_to(LABEL, "mini-pointer", serde_json::json!({ "out": true }))),
            None => {}
        }
        over = inside.is_some();
        if over || open != Mode::Rail as u8 {
            continue;
        }
        if asked == 0 {
            all = win.available_monitors().ok().unwrap_or_default().iter().map(points).collect::<Vec<_>>();
        }
        asked = (asked + 1) % 7;
        let here = display_at(&all, p);
        if here != seen {
            (seen, looks) = (here, 0);
            continue;
        }
        looks += 1;
        if let Some(d) = here.filter(|d| looks == SETTLED_LOOKS && Some(*d) != on()) {
            set_on(d);
            let _ = app.emit_to(LABEL, "mini-place", ());
        }
    }
}

/// The point's place inside the frame, if it's inside.
fn inside((x, y, w, h): (f64, f64, f64, f64), (px, py): (f64, f64)) -> Option<(f64, f64)> {
    (px >= x && px < x + w && py >= y && py < y + h).then_some((px - x, py - y))
}

/// The display a point is on (all in points, from the main display's top left).
fn display_at(all: &[(f64, f64, f64, f64)], (px, py): (f64, f64)) -> Option<(f64, f64, f64, f64)> {
    all.iter().copied().find(|&(x, y, w, h)| px >= x && px < x + w && py >= y && py < y + h)
}

/// A frame kept in four atomics (width 0: none).
fn load(cell: &[AtomicU64; 4]) -> Option<(f64, f64, f64, f64)> {
    let v: Vec<f64> = cell.iter().map(|a| f64::from_bits(a.load(Ordering::SeqCst))).collect();
    (v[2] > 0.0).then_some((v[0], v[1], v[2], v[3]))
}
fn store(cell: &[AtomicU64; 4], (x, y, w, h): (f64, f64, f64, f64)) {
    for (a, v) in cell.iter().zip([x, y, w, h]) {
        a.store(v.to_bits(), Ordering::SeqCst);
    }
}
fn on() -> Option<(f64, f64, f64, f64)> {
    load(&ON)
}
fn set_on(d: (f64, f64, f64, f64)) {
    store(&ON, d);
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

/// A display's room for windows (without the menu bar and the Dock): left, top, width, height, in points.
fn room(m: &tauri::Monitor) -> (f64, f64, f64, f64) {
    let s = m.scale_factor();
    let a = m.work_area();
    (a.position.x as f64 / s, a.position.y as f64 / s, a.size.width as f64 / s, a.size.height as f64 / s)
}

/// The room the mini window goes in: the notch's island on the main display, at its very top (the built-in
/// one, with the notch); the rail in the room (no menu bar, no Dock) of the display your pointer settled on last
/// (`follow`), until then the one Cue's own window is on, else the main display.
fn screen(app: &AppHandle, w: &tauri::WebviewWindow, mode: Mode) -> Option<(f64, f64, f64, f64)> {
    let main = w.primary_monitor().ok().flatten();
    if mode == Mode::Notch {
        return main.map(|m| points(&m));
    }
    let settled = on().and_then(|d| w.available_monitors().ok()?.into_iter().find(|m| points(m) == d));
    let cue = app.get_webview_window("main").and_then(|m| m.current_monitor().ok().flatten());
    settled.or(cue).or(main).map(|m| room(&m))
}

/// The rail's distance at least from the room's top and bottom edges.
const RAIL_MARGIN: f64 = 12.0;

/// Where a mini window of this size goes. The rail: flat against the right edge, its top `at` of the way down
/// the room (where you dragged it), tucked or out, so the tab and the panel share one top and never move; a
/// panel that wouldn't fit below it goes up just enough. The notch's island: centred at the top.
fn place(mode: Mode, (x, y, sw, sh): (f64, f64, f64, f64), w: f64, h: f64, at: f64) -> (f64, f64) {
    if mode == Mode::Notch {
        return (x + (sw - w) / 2.0, y);
    }
    let top = (at.clamp(0.0, 1.0) * sh).min(sh - RAIL_MARGIN - h).max(RAIL_MARGIN.min(sh - h).max(0.0));
    (x + sw - w, y + top)
}

/// The page's size for what it shows now (a hairline, the dots, the list, the drawer), in points. `show`
/// false hides it (nothing to show).
#[tauri::command]
pub(crate) async fn mini_shape(app: AppHandle, w: f64, h: f64, show: bool, at: Option<f64>) -> Result<(), String> {
    let win = app.get_webview_window(LABEL).ok_or("no mini window")?;
    if !show {
        store(&FRAME, (0.0, 0.0, 0.0, 0.0));
        return win.hide().map_err(|e| e.to_string());
    }
    let mode = if OPEN.load(Ordering::SeqCst) == Mode::Notch as u8 { Mode::Notch } else { Mode::Rail };
    let scr = screen(&app, &win, mode).ok_or("no screen")?;
    let (w, h) = (w.clamp(1.0, scr.2), h.clamp(1.0, scr.3));
    let (x, y) = place(mode, scr, w, h, at.unwrap_or(1.0));
    store(&DRAG_FROM, (0.0, 0.0, 0.0, 0.0));
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
        store(&FRAME, (0.0, 0.0, 0.0, 0.0));
        return Ok(());
    }
    store(&FRAME, (x, y, w, h));
    #[cfg(target_os = "macos")]
    macos::show_quietly(&win);
    #[cfg(not(target_os = "macos"))]
    win.show().map_err(|e| e.to_string())?;
    Ok(())
}

/// The rail's panel dragged by its bar: moved by (dx, dy) points from where it was when the drag began
/// (`begin`). Let go, its page snaps it to a corner or tucks it away (mini_shape).
#[tauri::command]
pub(crate) async fn mini_drag(app: AppHandle, dx: f64, dy: f64, begin: bool) -> Result<(), String> {
    let win = app.get_webview_window(LABEL).ok_or("no mini window")?;
    if begin {
        store(&DRAG_FROM, load(&FRAME).ok_or("not shown")?);
    }
    let (x, y, w, h) = load(&DRAG_FROM).ok_or("no drag")?;
    let to = (x + dx, y + dy, w, h);
    store(&FRAME, to);
    #[cfg(target_os = "macos")]
    macos::frame(&win, to, primary_height(&win));
    #[cfg(not(target_os = "macos"))]
    win.set_position(tauri::LogicalPosition::new(to.0, to.1)).map_err(|e| e.to_string())?;
    Ok(())
}

/// A drag let go: how far down the room the window's top is now (0 at its top, 1 at its bottom), for its page
/// to keep (the rail stays at that height from then on).
#[tauri::command]
pub(crate) async fn mini_drop(app: AppHandle) -> Result<f64, String> {
    let win = app.get_webview_window(LABEL).ok_or("no mini window")?;
    let (_, y, _, _) = load(&FRAME).ok_or("not shown")?;
    let (_, sy, _, sh) = screen(&app, &win, Mode::Rail).ok_or("no screen")?;
    Ok(((y - sy) / sh.max(1.0)).clamp(0.0, 1.0))
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
        store(&FRAME, (0.0, 0.0, 0.0, 0.0));
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
    fn the_rail_sits_flat_against_the_right_edge_where_it_was_dragged_tucked_or_out() {
        let tab = place(Mode::Rail, SCREEN, 36.0, 76.0, 0.5);
        let panel = place(Mode::Rail, SCREEN, 420.0, 440.0, 0.7);
        assert_eq!(tab, (1512.0 - 36.0, 491.0));
        assert_eq!(panel, (1512.0 - 420.0, 982.0 - RAIL_MARGIN - 440.0), "a panel that wouldn't fit below goes up");
        assert_eq!(place(Mode::Rail, SCREEN, 420.0, 440.0, 0.2).1, 0.2 * 982.0, "one that fits keeps the tab's top");
        assert_eq!(place(Mode::Rail, SCREEN, 36.0, 76.0, 0.0).1, RAIL_MARGIN);
        assert_eq!(place(Mode::Rail, SCREEN, 36.0, 76.0, 1.0).1, 982.0 - RAIL_MARGIN - 76.0);
    }

    #[test]
    fn the_rail_never_leaves_its_room_and_takes_another_displays_own_edge() {
        assert_eq!(place(Mode::Rail, SCREEN, 420.0, 982.0, 1.0).1, 0.0);
        let external = (1800.0, -567.0, 3008.0, 1692.0);
        assert_eq!(place(Mode::Rail, external, 36.0, 76.0, 0.0), (1800.0 + 3008.0 - 36.0, -567.0 + RAIL_MARGIN));
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
    fn the_pointer_is_found_on_its_display() {
        let built_in = (0.0, 0.0, 1800.0, 1125.0);
        let external = (1800.0, -567.0, 3008.0, 1692.0);
        let all = [built_in, external];
        assert_eq!(display_at(&all, (900.0, 500.0)), Some(built_in));
        assert_eq!(display_at(&all, (3000.0, 100.0)), Some(external));
        assert_eq!(display_at(&all, (3000.0, -400.0)), Some(external), "above the built-in one");
        assert_eq!(display_at(&all, (-10.0, 10.0)), None);
    }

    #[test]
    fn the_pointer_inside_the_window_is_given_from_its_corner() {
        let rail = (4780.0, 564.0, 28.0, 161.0);
        assert_eq!(inside(rail, (4790.0, 600.0)), Some((10.0, 36.0)));
        assert_eq!(inside(rail, (4779.0, 600.0)), None);
        assert_eq!(inside(rail, (4790.0, 725.0)), None, "just below it");
    }

    #[test]
    fn the_notch_island_is_centred_at_the_top() {
        assert_eq!(place(Mode::Notch, SCREEN, 560.0, 324.0, 0.0), (476.0, 0.0));
    }
}
