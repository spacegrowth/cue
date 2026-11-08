//! The mini window: Cue at the edge of your screen while you work in another app, so a session that needs you
//! can be read and answered without bringing the whole window forward. Settings → Mini window picks its shape:
//! Off, the edge (the rail: a small dark tab tucked into a side edge of every display, wherever you dragged it
//! on that display, that slides out into a panel with the chat, as picture in picture does) or the notch (a
//! dark island that drops the same chat). The page is `ui/mini.html`; it gets the same `state` the window gets
//! and calls the same commands (reply, respond), so nothing here knows about sessions.
//!
//! The rail is one window per display (`mini`, `mini-1`, …), each its own page. The one on the display Cue's
//! own window is on steps aside while that window is in front; the others stay. The notch is one window, on
//! the main display, and always shown, Cue's window in front or not.
//!
//! The window never takes the app forward: a click in it leaves the app you're in active (macos.rs), and it
//! takes the keyboard only while you're typing in its box.

#[cfg(target_os = "macos")]
mod macos;

use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The first mini window's label (the notch's, or the rail's on the first display); the rail's others are
/// `mini-1`, `mini-2`, … (capabilities/default.json lets `mini*` call Cue).
pub(crate) const LABEL: &str = "mini";

/// A display (or its room), in points from the main display's top left: left, top, width, height.
type Rect = (f64, f64, f64, f64);

/// Where the mini window lives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Off = 0,
    Rail = 1,
    Notch = 2,
}

/// The shape open now (Off: none).
static OPEN: AtomicU8 = AtomicU8::new(Mode::Off as u8);
/// Cue's own window is in front: the mini window on its display steps aside. Kept from that window's focus
/// events (asked of the window just as it goes to the back, it can still say it's in front).
static MAIN_FRONT: AtomicBool = AtomicBool::new(false);
/// The pointer-watching thread is running (started once, with the first mini window).
static FOLLOWING: AtomicBool = AtomicBool::new(false);

/// Each mini window open now, by label.
#[derive(Clone, Copy, Debug, Default)]
struct Slot {
    /// The display it's on.
    display: Rect,
    /// Where it is now (as mini_shape placed it; None: hidden). Kept here so the pointer watch never asks the
    /// window (each ask is a round trip through the main thread).
    frame: Option<Rect>,
}
static SLOTS: Mutex<Option<HashMap<String, Slot>>> = Mutex::new(None);

fn slots<R>(f: impl FnOnce(&mut HashMap<String, Slot>) -> R) -> R {
    let mut g = crate::locking::lock(&SLOTS, "mini windows");
    f(g.get_or_insert_with(HashMap::new))
}

/// Where you put the tab on each display (dragged it there): how far down its room (0 top, 1 bottom) and on
/// which side. Kept in Cue's folder, so each display keeps its own across restarts. Until you drag one: the
/// right edge's foot.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    at: f64,
    left: bool,
}
const UNPLACED: Place = Place { at: 1.0, left: false };
static PLACES: Mutex<Option<HashMap<String, Place>>> = Mutex::new(None);

fn key(d: Rect) -> String {
    format!("{},{},{},{}", d.0, d.1, d.2, d.3)
}
fn places_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("mini-places.json")
}
fn place_of(d: Rect) -> Place {
    let mut g = crate::locking::lock(&PLACES, "mini places");
    let all = g.get_or_insert_with(|| {
        let v: Value = std::fs::read(places_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
        v.as_object()
            .map(|o| o.iter().filter_map(|(k, p)| Some((k.clone(), Place { at: p.get("at")?.as_f64()?, left: p.get("left")?.as_bool()? }))).collect())
            .unwrap_or_default()
    });
    all.get(&key(d)).copied().unwrap_or(UNPLACED)
}
fn set_place(d: Rect, p: Place) {
    place_of(d);
    let json = {
        let mut g = crate::locking::lock(&PLACES, "mini places");
        let all = g.get_or_insert_with(HashMap::new);
        all.insert(key(d), p);
        let o: serde_json::Map<String, Value> = all.iter().map(|(k, p)| (k.clone(), serde_json::json!({ "at": p.at, "left": p.left }))).collect();
        Value::Object(o)
    };
    if let Err(e) = std::fs::write(places_file(), json.to_string()) {
        eprintln!("[cue] mini window: couldn't keep where it was put: {e}");
    }
}

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

fn label_for(i: usize) -> String {
    if i == 0 { LABEL.to_string() } else { format!("{LABEL}-{i}") }
}
fn is_mini(label: &str) -> bool {
    label == LABEL || label.strip_prefix("mini-").is_some_and(|n| n.parse::<usize>().is_ok())
}

/// The mini window as the setting has it now: opened, swapped for the other shape, or closed.
pub(crate) fn apply(app: &AppHandle) {
    let want = mode();
    let open = OPEN.load(Ordering::SeqCst);
    if open == want as u8 && (want == Mode::Off || app.get_webview_window(LABEL).is_some()) {
        return;
    }
    close_all(app);
    OPEN.store(Mode::Off as u8, Ordering::SeqCst);
    if want == Mode::Off || std::env::var_os("CUE_QUIET").is_some() {
        return;
    }
    OPEN.store(want as u8, Ordering::SeqCst);
    if want == Mode::Notch {
        let primary = app.primary_monitor().ok().flatten().map(|m| points(&m)).unwrap_or_default();
        if let Err(e) = open_window(app, LABEL, want, primary) {
            eprintln!("[cue] mini window: {e}");
        }
    } else {
        reconcile(app);
    }
    #[cfg(target_os = "macos")]
    if !FOLLOWING.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        let _ = std::thread::Builder::new().name("cue-mini-follow".into()).spawn(move || follow(app));
    }
}

fn close_all(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if is_mini(&label) {
            let _ = w.destroy();
        }
    }
    slots(|s| s.clear());
}

/// The displays, in order (left to right, then top to bottom), each with its room.
fn displays(app: &AppHandle) -> Vec<(Rect, Rect)> {
    let mut all: Vec<(Rect, Rect)> = app.available_monitors().ok().unwrap_or_default().iter().map(|m| (points(m), room(m))).collect();
    all.sort_by(|a, b| a.0 .0.total_cmp(&b.0 .0).then(a.0 .1.total_cmp(&b.0 .1)));
    all
}

/// The rail: a window on each display there is now (one opened for a display that came, closed for one that
/// went), each told to place itself again. True when anything changed.
fn reconcile(app: &AppHandle) -> bool {
    if OPEN.load(Ordering::SeqCst) != Mode::Rail as u8 {
        return false;
    }
    let all = displays(app);
    // None while the displays sleep: the windows stay as they are, and are looked at again a few seconds on.
    if all.is_empty() {
        return false;
    }
    let mut changed = false;
    for (i, (d, _)) in all.iter().enumerate() {
        let label = label_for(i);
        let had = slots(|s| s.get(&label).map(|x| x.display));
        if had == Some(*d) && app.get_webview_window(&label).is_some() {
            continue;
        }
        changed = true;
        eprintln!("[cue] mini window {label}: on the display at {},{} ({}×{})", d.0, d.1, d.2, d.3);
        if app.get_webview_window(&label).is_some() {
            slots(|s| s.insert(label.clone(), Slot { display: *d, frame: None }));
            use tauri::Emitter;
            let _ = app.emit_to(label.as_str(), "mini-place", ());
        } else if let Err(e) = open_window(app, &label, Mode::Rail, *d) {
            eprintln!("[cue] mini window: {e}");
        }
    }
    for (label, w) in app.webview_windows() {
        let n = label.strip_prefix("mini-").and_then(|n| n.parse::<usize>().ok());
        if n.is_some_and(|n| n >= all.len()) {
            let _ = w.destroy();
            slots(|s| s.remove(&label));
            changed = true;
        }
    }
    changed
}

/// How often the pointer is looked for while it isn't over a mini window.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(700);
/// How often while it's over one: its page is told where, for hovering.
const TRACK_EVERY: std::time::Duration = std::time::Duration::from_millis(120);
/// Displays and Cue's own window's display are looked at every so many looks.
const LOOKS_PER_CHECK: u32 = 7;

/// Watches the pointer for the pages (a window that doesn't take the keyboard gets no hover from WebKit until
/// it's clicked): over which mini window, and where. Every few looks: the displays (a window for each) and
/// which display Cue's own window is on while it's in front.
#[cfg(target_os = "macos")]
fn follow(app: AppHandle) {
    use tauri::Emitter;
    let (mut over, mut asked): (Option<String>, u32) = (None, 0);
    let mut main_on: Option<Rect> = None;
    loop {
        std::thread::sleep(if over.is_some() { TRACK_EVERY } else { LOOK_EVERY });
        if OPEN.load(Ordering::SeqCst) == Mode::Off as u8 {
            over = None;
            continue;
        }
        let p = macos::pointer();
        let frames: Vec<(String, Rect)> = slots(|s| s.iter().filter_map(|(l, x)| Some((l.clone(), x.frame?))).collect());
        let now = frames.iter().find_map(|(l, f)| inside(*f, p).map(|xy| (l.clone(), xy)));
        if let Some(was) = over.as_ref().filter(|w| now.as_ref().map(|n| &n.0) != Some(*w)) {
            let _ = app.emit_to(was.as_str(), "mini-pointer", serde_json::json!({ "out": true }));
        }
        if let Some((l, (x, y))) = &now {
            let _ = app.emit_to(l.as_str(), "mini-pointer", serde_json::json!({ "x": x, "y": y }));
        }
        over = now.map(|n| n.0);
        if over.is_some() {
            continue;
        }
        asked = (asked + 1) % LOOKS_PER_CHECK;
        if asked != 0 {
            continue;
        }
        if reconcile(&app) {
            continue;
        }
        // Cue's window moved to another display while in front: the mini windows there and here swap.
        let on = MAIN_FRONT.load(Ordering::SeqCst).then(|| main_display(&app)).flatten();
        if on != main_on {
            if main_on.is_some() || on.is_some() {
                let _ = app.emit("mini-place", ());
            }
            main_on = on;
        }
    }
}

fn inside((x, y, w, h): Rect, (px, py): (f64, f64)) -> Option<(f64, f64)> {
    (px >= x && px < x + w && py >= y && py < y + h).then_some((px - x, py - y))
}

fn open_window(app: &AppHandle, label: &str, mode: Mode, display: Rect) -> tauri::Result<()> {
    slots(|s| s.insert(label.to_string(), Slot { display, frame: None }));
    let w = WebviewWindowBuilder::new(app, label, WebviewUrl::App(mode.page().into()))
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

#[cfg(target_os = "macos")]
fn primary_height(w: &tauri::WebviewWindow) -> f64 {
    w.primary_monitor().ok().flatten().map(|m| points(&m).3).unwrap_or(0.0)
}

fn points(m: &tauri::Monitor) -> Rect {
    let s = m.scale_factor();
    let (p, z) = (m.position(), m.size());
    (p.x as f64 / s, p.y as f64 / s, z.width as f64 / s, z.height as f64 / s)
}

/// A display's room: without the menu bar and the Dock.
fn room(m: &tauri::Monitor) -> Rect {
    let s = m.scale_factor();
    let a = m.work_area();
    (a.position.x as f64 / s, a.position.y as f64 / s, a.size.width as f64 / s, a.size.height as f64 / s)
}

/// The display Cue's own window is on.
fn main_display(app: &AppHandle) -> Option<Rect> {
    app.get_webview_window("main").and_then(|m| m.current_monitor().ok().flatten()).map(|m| points(&m))
}

/// The tab off an edge, by this much (the edge of the room, top and bottom).
const RAIL_MARGIN: f64 = 12.0;

/// Where a window `w`×`h` goes in `room`: the notch's island centred at the top; the rail's tab or panel flat
/// against the side edge you put it on, its top `at` of the way down the room (moved up when it wouldn't fit).
fn place(mode: Mode, (x, y, sw, sh): Rect, w: f64, h: f64, at: f64, left: bool) -> (f64, f64) {
    if mode == Mode::Notch {
        return (x + (sw - w) / 2.0, y);
    }
    let top = (at.clamp(0.0, 1.0) * sh).min(sh - RAIL_MARGIN - h).max(RAIL_MARGIN.min(sh - h).max(0.0));
    (if left { x } else { x + sw - w }, y + top)
}

/// Where a drag let the rail go: the display it's on (the one its middle is over, else the nearest), the side
/// edge nearer its middle, and how far down that display's room its top is.
fn dropped(all: &[(Rect, Rect)], (x, y, w, h): Rect) -> Option<(Rect, Place)> {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let far = |(dx, dy, dw, dh): Rect| (cx - cx.clamp(dx, dx + dw)).abs() + (cy - cy.clamp(dy, dy + dh)).abs();
    let (display, (_, ry, _, rh)) = all.iter().copied().min_by(|a, b| far(a.0).total_cmp(&far(b.0)))?;
    let left = cx < display.0 + display.2 / 2.0;
    Some((display, Place { at: ((y - ry) / rh.max(1.0)).clamp(0.0, 1.0), left }))
}

/// Move and size a mini window, in points.
fn set_frame(win: &tauri::WebviewWindow, r: Rect) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    macos::frame(win, r, primary_height(win));
    #[cfg(not(target_os = "macos"))]
    {
        win.set_size(tauri::LogicalSize::new(r.2, r.3)).map_err(|e| e.to_string())?;
        win.set_position(tauri::LogicalPosition::new(r.0, r.1)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The page's size for what it shows now (the tab, the island, the chat), in points; `show` false hides it.
/// Placed on its own display where you put it there. Says which side that is (`left`), for the page.
#[tauri::command]
pub(crate) async fn mini_shape(app: AppHandle, window: tauri::WebviewWindow, w: f64, h: f64, show: bool) -> Result<Value, String> {
    let r = shape(&app, &window, w, h, show);
    if let Err(e) = &r {
        eprintln!("[cue] mini window {}: couldn't place it: {e}", window.label());
    }
    r
}

fn shape(app: &AppHandle, window: &tauri::WebviewWindow, w: f64, h: f64, show: bool) -> Result<Value, String> {
    let app = app.clone();
    let window = window.clone();
    let label = window.label().to_string();
    let hide = |win: &tauri::WebviewWindow| {
        slots(|s| s.get_mut(&label).map(|x| x.frame = None));
        win.hide().map_err(|e| e.to_string())
    };
    if !show {
        hide(&window)?;
        return Ok(serde_json::json!({ "left": false }));
    }
    let mode = if OPEN.load(Ordering::SeqCst) == Mode::Notch as u8 { Mode::Notch } else { Mode::Rail };
    let display = slots(|s| s.get(&label).map(|x| x.display)).ok_or("not a mini window")?;
    let all = displays(&app);
    let room = if mode == Mode::Notch { Some(display) } else { all.iter().find(|(d, _)| *d == display).map(|(_, r)| *r) };
    let room = room.ok_or("its display is gone")?;
    let p = if mode == Mode::Notch { UNPLACED } else { place_of(display) };
    let (w, h) = (w.clamp(1.0, room.2), h.clamp(1.0, room.3));
    let (x, y) = place(mode, room, w, h, p.at, p.left);
    #[cfg(debug_assertions)]
    eprintln!("[cue] mini window {label} {w}×{h} at {x},{y}");
    set_frame(&window, (x, y, w, h))?;
    // The rail isn't on the display where Cue's own window is in front (it would only repeat it); the notch is
    // always there, Cue in front or not. Shown without ever taking the keyboard: Tauri's show() makes it the
    // key window, and when that ran each time Cue's window came forward, the two took focus from each other and
    // you couldn't type in Cue.
    if mode == Mode::Rail && MAIN_FRONT.load(Ordering::SeqCst) && main_display(&app) == Some(display) {
        hide(&window)?;
        return Ok(serde_json::json!({ "left": p.left }));
    }
    slots(|s| s.get_mut(&label).map(|sl| sl.frame = Some((x, y, w, h))));
    #[cfg(target_os = "macos")]
    macos::show_quietly(&window);
    #[cfg(not(target_os = "macos"))]
    window.show().map_err(|e| e.to_string())?;
    if mode != Mode::Notch {
        return Ok(serde_json::json!({ "left": p.left }));
    }
    Ok(serde_json::json!({ "left": false, "notch": notch_of(&app, &window, display) }))
}

/// A display without a notch gets one of Cue's own, this wide, as tall as its menu bar.
const OWN_NOTCH_W: f64 = 185.0;

/// The notch the island grows out of on `display`: { w, h, real }. A display without one (an external display, a
/// Mac without one, a notched one set to keep the menu bar below the camera) gets Cue's own, drawn at its top as
/// tall as its menu bar.
fn notch_of(app: &AppHandle, window: &tauri::WebviewWindow, display: Rect) -> Value {
    #[cfg(target_os = "macos")]
    if let Some((w, h)) = macos::notch(window, (display.0, display.2, display.3)) {
        return serde_json::json!({ "w": w, "h": h, "real": true });
    }
    let _ = window;
    let menu_bar = displays(app).iter().find(|(d, _)| *d == display).map(|(d, r)| r.1 - d.1).filter(|h| (20.0..=44.0).contains(h)).unwrap_or(24.0);
    serde_json::json!({ "w": OWN_NOTCH_W, "h": menu_bar, "real": false })
}

/// How often a drag follows the pointer (about two frames).
#[cfg(target_os = "macos")]
const DRAG_EVERY: std::time::Duration = std::time::Duration::from_millis(8);

/// A tab dragged (or its panel by its bar): from the press until you let go, the window follows the pointer
/// here (not a round trip from the page per step, which trailed it and could miss the release). Let go on its
/// own display, it takes the nearer side edge there, at that height, at once, and keeps it. Let go on another
/// display, that display's tab goes there, and this one goes back to its place. Says the side it's on now.
#[tauri::command]
pub(crate) async fn mini_drag(app: AppHandle, window: tauri::WebviewWindow) -> Result<Value, String> {
    let label = window.label().to_string();
    let slot = slots(|s| s.get(&label).copied()).ok_or("not a mini window")?;
    let from = slot.frame.ok_or("not shown")?;
    #[cfg(target_os = "macos")]
    let frame = {
        let main_h = primary_height(&window);
        let p0 = macos::pointer();
        let win = window.clone();
        let l = label.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let (mut at, mut last) = (from, p0);
            while macos::pressed() {
                std::thread::sleep(DRAG_EVERY);
                let p = macos::pointer();
                if p == last || p.0 < -0.5 {
                    continue;
                }
                last = p;
                at = (from.0 + p.0 - p0.0, from.1 + p.1 - p0.1, from.2, from.3);
                slots(|s| s.get_mut(&l).map(|x| x.frame = Some(at)));
                macos::frame(&win, at, main_h);
            }
            at
        })
        .await
        .map_err(|e| e.to_string())?
    };
    #[cfg(not(target_os = "macos"))]
    let frame = from;
    let all = displays(&app);
    let (onto, p) = dropped(&all, frame).ok_or("no display")?;
    set_place(onto, p);
    if onto != slot.display {
        let other = slots(|s| s.iter().find(|(_, x)| x.display == onto).map(|(l, _)| l.clone()));
        if let Some(other) = other {
            use tauri::Emitter;
            let _ = app.emit_to(other.as_str(), "mini-place", ());
        }
    }
    // Onto its edge now (its own display's place), as the page's next mini_shape would put it.
    let mine = place_of(slot.display);
    if let Some((_, room)) = all.iter().find(|(d, _)| *d == slot.display) {
        let (x, y) = place(Mode::Rail, *room, frame.2, frame.3, mine.at, mine.left);
        slots(|s| s.get_mut(&label).map(|sl| sl.frame = Some((x, y, frame.2, frame.3))));
        set_frame(&window, (x, y, frame.2, frame.3))?;
    }
    Ok(serde_json::json!({ "left": mine.left }))
}

/// The mini window takes the keyboard (you clicked into its box) or hands it back to the app you were in.
#[tauri::command]
pub(crate) async fn mini_key(window: tauri::WebviewWindow, on: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    macos::key(&window, on);
    #[cfg(not(target_os = "macos"))]
    let _ = (window, on);
    Ok(())
}

/// The session whose chat this mini window shows now (None: folded): nothing about it is notified meanwhile.
#[tauri::command]
pub(crate) async fn mini_reading(window: tauri::WebviewWindow, session_id: Option<String>) -> Result<(), String> {
    crate::notify::reading(window.label(), session_id);
    Ok(())
}

/// Cue's own window, on a session's chat (the mini window's ⤢) or as it was (Open Cue); with `doc`, a markdown
/// file a reply named, open in its reader (a link clicked in the mini window).
#[tauri::command]
pub(crate) async fn mini_open_in_cue(app: AppHandle, session_id: Option<String>, doc: Option<String>) -> Result<(), String> {
    let sid = session_id.filter(|s| !s.is_empty());
    let id = match (&sid, doc) {
        (Some(s), Some(path)) => Some(format!("d:{}", serde_json::json!({ "sid": s, "path": path }))),
        _ => sid.map(|s| format!("s:{s}")),
    };
    crate::show_main(&app, id);
    Ok(())
}

/// Cue's own window came to the front (the rail on its display steps aside; the notch stays) or went back (every
/// mini window places itself again, so that one comes back).
pub(crate) fn main_focused(app: &AppHandle, focused: bool) {
    MAIN_FRONT.store(focused, Ordering::SeqCst);
    if focused && OPEN.load(Ordering::SeqCst) == Mode::Notch as u8 {
        return;
    }
    if focused {
        let Some(on) = main_display(app) else { return };
        let here: Vec<String> = slots(|s| s.iter().filter(|(_, x)| x.display == on).map(|(l, _)| l.clone()).collect());
        for l in here {
            slots(|s| s.get_mut(&l).map(|x| x.frame = None));
            if let Some(win) = app.get_webview_window(&l) {
                let _ = win.hide();
            }
        }
        return;
    }
    use tauri::Emitter;
    let _ = app.emit("mini-place", ());
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
        let tab = place(Mode::Rail, SCREEN, 36.0, 76.0, 0.5, false);
        let panel = place(Mode::Rail, SCREEN, 420.0, 440.0, 0.7, false);
        assert_eq!(tab, (1512.0 - 36.0, 491.0));
        assert_eq!(panel, (1512.0 - 420.0, 982.0 - RAIL_MARGIN - 440.0), "a panel that wouldn't fit below goes up");
        assert_eq!(place(Mode::Rail, SCREEN, 420.0, 440.0, 0.2, false).1, 0.2 * 982.0, "one that fits keeps the tab's top");
        assert_eq!(place(Mode::Rail, SCREEN, 36.0, 76.0, 0.0, false).1, RAIL_MARGIN);
        assert_eq!(place(Mode::Rail, SCREEN, 36.0, 76.0, 1.0, false).1, 982.0 - RAIL_MARGIN - 76.0);
    }

    #[test]
    fn the_rail_never_leaves_its_room_and_takes_another_displays_own_edge() {
        assert_eq!(place(Mode::Rail, SCREEN, 420.0, 982.0, 1.0, false).1, 0.0);
        let external = (1800.0, -567.0, 3008.0, 1692.0);
        assert_eq!(place(Mode::Rail, external, 36.0, 76.0, 0.0, false), (1800.0 + 3008.0 - 36.0, -567.0 + RAIL_MARGIN));
    }

    #[test]
    fn dragged_anywhere_the_rail_takes_the_nearer_side_edge_of_the_display_it_was_let_go_on() {
        assert_eq!(place(Mode::Rail, SCREEN, 36.0, 76.0, 0.5, true), (0.0, 491.0), "the left edge");
        let built_in = ((0.0, 0.0, 1512.0, 982.0), (0.0, 37.0, 1512.0, 945.0));
        let external = ((1512.0, -300.0, 2560.0, 1440.0), (1512.0, -270.0, 2560.0, 1410.0));
        let all = [built_in, external];
        let (d, p) = dropped(&all, (300.0, 509.0, 36.0, 76.0)).unwrap();
        assert_eq!((d, p), (built_in.0, Place { at: (509.0 - 37.0) / 945.0, left: true }));
        let (d, p) = dropped(&all, (3500.0, -100.0, 36.0, 76.0)).unwrap();
        assert_eq!((d, p.left), (external.0, false), "the external one's right edge");
        assert_eq!(dropped(&all, (5000.0, 100.0, 36.0, 76.0)).unwrap().0, external.0, "past every display: the nearest");
    }

    #[test]
    fn each_display_has_a_window_of_its_own() {
        assert_eq!((label_for(0), label_for(2)), ("mini".to_string(), "mini-2".to_string()));
        assert!(is_mini("mini") && is_mini("mini-3"));
        assert!(!is_mini("main") && !is_mini("mini-x") && !is_mini("minimal"));
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
        assert!(shows.iter().all(|l| l.trim_start().starts_with("window.show().map_err")), "{shows:?}");
        assert_eq!(code(here).matches("window.show()").count(), 1, "show() only off macOS");
        let lines: Vec<&str> = code(here).lines().collect();
        let at = lines.iter().position(|l| l.contains("window.show()")).unwrap();
        assert!(lines[at - 1].contains(r#"#[cfg(not(target_os = "macos"))]"#), "the one show() is the non-macOS path: {}", lines[at - 1]);
        let key_fn = mac.split("pub(super) fn key(").nth(1).unwrap().split("\n}\n").next().unwrap();
        assert_eq!(code(mac).matches("makeKeyAndOrderFront").count(), 1);
        assert!(key_fn.contains("makeKeyAndOrderFront"), "only `key` makes it the key window");
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
        assert_eq!(place(Mode::Notch, SCREEN, 560.0, 324.0, 0.0, false), (476.0, 0.0));
    }
}
