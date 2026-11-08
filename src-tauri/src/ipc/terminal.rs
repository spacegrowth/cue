//! The built-in terminal, a session's screen and keys, and dictation.

use super::{blocking, with_hub};
use crate::leads;
use crate::model;
use crate::terminal::shells;
use crate::terminal::term;
use crate::terminal::dictation;
use crate::hub::Hub;
use std::sync::Arc;
use tauri::{Manager, State};

/// The mic button: start or stop dictating (words arrive as "dictation" events). Starting starts a helper.
#[tauri::command]
pub(crate) async fn dictate_start(app: tauri::AppHandle) -> Result<(), String> {
    blocking(move || dictation::start(app.clone(), &app.state::<dictation::Dictation>())).await?
}

#[tauri::command]
pub(crate) async fn dictate_stop(app: tauri::AppHandle) -> Result<(), String> {
    blocking(move || dictation::stop(&app.state::<dictation::Dictation>())).await
}

/// Listen's natural voices: a line to cue-speak (its lines come back as "speak" events), written to its
/// stdin (a pipe, which waits while the helper is busy).
#[tauri::command]
pub(crate) async fn speak_send(app: tauri::AppHandle, line: serde_json::Value) -> Result<(), String> {
    blocking(move || crate::terminal::speak::send(app.clone(), &app.state::<crate::terminal::speak::Speak>(), &line)).await?
}

/// Listen started: whatever is playing (a video, music) paused while it reads (terminal/music.rs).
#[tauri::command]
pub(crate) async fn music_hold() -> Result<(), String> {
    blocking(crate::terminal::music::hold).await
}

/// Listen is done: what it paused plays again.
#[tauri::command]
pub(crate) async fn music_release() -> Result<(), String> {
    blocking(crate::terminal::music::release).await
}

/// What a session's terminal shows, for a session in tmux: Cue's view of a prompt it has no buttons for.
#[tauri::command]
pub(crate) async fn session_screen(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| h.keys().screen(&tmux_origin_of(h, &session_id)?)).await?
}

/// Keys pressed in that view (↑ ↓ Enter Esc 1 2 3…). Only from your click.
#[tauri::command]
pub(crate) async fn session_keys(hub: State<'_, Arc<Hub>>, session_id: String, keys: Vec<String>) -> Result<(), String> {
    with_hub(&hub, move |h| h.keys().press_keys(&tmux_origin_of(h, &session_id)?, &keys)).await?
}

/// Where a session is, for the commands that only work in tmux (on this Mac or a machine).
pub(crate) fn tmux_origin_of(hub: &Hub, session_id: &str) -> Result<model::Origin, String> {
    let o = hub.session_origin(session_id).ok_or("that session is gone")?;
    if o.tmux_pane.is_empty() {
        return Err("that session isn't in tmux".into());
    }
    Ok(o)
}

/// The built-in terminal (sessions in tmux): open it at its size; its output comes as "term" events.
#[tauri::command]
pub(crate) async fn term_open(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, cols: u16, rows: u16) -> Result<u64, String> {
    // A relay / pi-lead executor on a machine, in a tmux session of its own: that session, whatever
    // pane Cue has on record for it (and whether or not Cue holds it at all). Never its lead's.
    with_hub(&hub, move |h| match leads::exec_tmux(&session_id) {
        Some((machine, name)) => term::open_attached(app, &session_id, &machine, &name, cols, rows),
        None => term::open(app, &session_id, &tmux_origin_of(h, &session_id)?, cols, rows),
    })
    .await?
}

/// A shell of Cue's own (the floating terminal's tabs), `id` from the window, in `cwd` (home when that
/// isn't a folder). Written to, resized and closed with term_write / term_resize / term_close by its id.
/// Its output's generation, this Mac's short host name, for its tab ("mac: ~"), and whether it's in tmux
/// (`kept`: quitting Cue doesn't end it). One still running in tmux is attached again, as it was.
#[tauri::command]
pub(crate) async fn shell_open(app: tauri::AppHandle, id: String, cwd: String, cols: u16, rows: u16) -> Result<serde_json::Value, String> {
    let (gen, kept) = blocking(move || shells::open(app, &id, &cwd, cols, rows)).await??;
    let host = crate::machines::hooks::hostname();
    Ok(serde_json::json!({ "gen": gen, "kept": kept, "host": host.split('.').next().unwrap_or(&host) }))
}

/// The shells still running in tmux, oldest first ({id, name, cwd}), and this Mac's short host name:
/// the window gives each its tab back.
#[tauri::command]
pub(crate) async fn shell_list(app: tauri::AppHandle) -> serde_json::Value {
    let list = blocking(move || shells::list(app)).await.unwrap_or_default();
    let host = crate::machines::hooks::hostname();
    serde_json::json!({ "shells": list, "host": host.split('.').next().unwrap_or(&host) })
}

/// A shell's tab renamed ("" for none): kept with the shell, for the next Cue.
#[tauri::command]
pub(crate) async fn shell_name(id: String, name: String) {
    let _ = blocking(move || shells::rename(&id, &name)).await;
}

/// ⌘K in a shell in tmux.
#[tauri::command]
pub(crate) async fn shell_clear(id: String) -> Result<(), String> {
    blocking(move || shells::clear(&id)).await?
}

// The three window commands below stay sync, on the window's thread: they only ask the window itself
// (AppKit's, main-thread work Tauri would send there anyway) and take no lock of Cue's.

/// ⌘W outside Cue's terminal: the window hides, as closing it does (Cue keeps running).
#[tauri::command]
pub(crate) fn hide_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

/// Esc in full screen: the window leaves it (the board follows, as it does for ⌃⌘F).
#[tauri::command]
pub(crate) fn set_fullscreen(app: tauri::AppHandle, on: bool) -> Result<(), String> {
    app.get_webview_window("main").ok_or("no window")?.set_fullscreen(on).map_err(|e| e.to_string())
}

/// Whether the window is full screen, which the board follows (⌃⌘F, the green button, View → Enter Full Screen).
#[tauri::command]
pub(crate) fn is_fullscreen(app: tauri::AppHandle) -> Result<bool, String> {
    app.get_webview_window("main").ok_or("no window")?.is_fullscreen().map_err(|e| e.to_string())
}

/// What you type in the built-in terminal: an error within 2 s when its client isn't reading, and at once
/// after that until term_resume (term::write). Waited for without a thread (not the blocking pool, which a
/// busy hub can fill): a keystroke never queues behind other commands' work. Two writes in flight at once
/// could land in either order: the window sends a terminal's next only once the last is back (ui/invoke.js).
#[tauri::command]
pub(crate) async fn term_write(session_id: String, data: String) -> Result<(), String> {
    term::write(&session_id, &data).await
}

/// Typing into it again after its input stopped (term_write failed): what was waiting to go in is dropped.
#[tauri::command]
pub(crate) async fn term_resume(session_id: String) -> Result<(), String> {
    term::resume(&session_id)
}

/// Its new size: two short locks (the open terminals', its pty's) and an ioctl, never waited on.
#[tauri::command]
pub(crate) async fn term_resize(session_id: String, cols: u16, rows: u16) -> Result<(), String> {
    term::resize(&session_id, cols, rows)
}

/// Close it (the session keeps running). A shell's tab closed: the shell ends. `gen`: the opening the window
/// means (term_open's answer), when it has it: then a newer one, opened since, stays (term::close_if);
/// without it, whatever is open or opening goes.
#[tauri::command]
pub(crate) async fn term_close(session_id: String, gen: Option<u64>) -> Result<(), String> {
    blocking(move || match gen {
        _ if session_id.starts_with(shells::PREFIX) => shells::close(&session_id),
        Some(g) if g > 0 => term::close_if(&session_id, g),
        _ => term::close(&session_id),
    })
    .await
}
