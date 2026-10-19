//! The built-in terminal, a session's screen and keys, and dictation.

use crate::leads;
use crate::model;
use crate::terminal::focus;
use crate::terminal::shells;
use crate::terminal::term;
use crate::terminal::dictation;
use crate::hub::Hub;
use std::sync::Arc;
use tauri::State;

/// The mic button: start or stop dictating (words arrive as "dictation" events).
#[tauri::command]
pub(crate) fn dictate_start(app: tauri::AppHandle, d: State<dictation::Dictation>) -> Result<(), String> {
    dictation::start(app, &d)
}

#[tauri::command]
pub(crate) fn dictate_stop(d: State<dictation::Dictation>) {
    dictation::stop(&d)
}

/// Listen's natural voices: a line to cue-speak (its lines come back as "speak" events).
#[tauri::command]
pub(crate) fn speak_send(app: tauri::AppHandle, s: State<crate::terminal::speak::Speak>, line: serde_json::Value) -> Result<(), String> {
    crate::terminal::speak::send(app, &s, &line)
}

/// What a session's terminal shows, for a session in tmux: Cue's view of a prompt it has no buttons for.
#[tauri::command]
pub(crate) async fn session_screen(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let o = tmux_origin_of(&hub, &session_id)?;
    tauri::async_runtime::spawn_blocking(move || focus::session_screen(&o)).await.map_err(|e| e.to_string())?
}

/// Keys pressed in that view (↑ ↓ Enter Esc 1 2 3…). Only from your click.
#[tauri::command]
pub(crate) async fn session_keys(hub: State<'_, Arc<Hub>>, session_id: String, keys: Vec<String>) -> Result<(), String> {
    let o = tmux_origin_of(&hub, &session_id)?;
    tauri::async_runtime::spawn_blocking(move || focus::tmux_keys(&o, &keys)).await.map_err(|e| e.to_string())?
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
    if let Some((machine, name)) = leads::exec_tmux(&session_id) {
        return tauri::async_runtime::spawn_blocking(move || term::open_attached(app, &session_id, &machine, &name, cols, rows)).await.map_err(|e| e.to_string())?;
    }
    let o = tmux_origin_of(&hub, &session_id)?;
    tauri::async_runtime::spawn_blocking(move || term::open(app, &session_id, &o, cols, rows)).await.map_err(|e| e.to_string())?
}

/// A shell of Cue's own (the floating terminal's tabs), `id` from the window, in `cwd` (home when that
/// isn't a folder). Written to, resized and closed with term_write / term_resize / term_close by its id.
/// Its output's generation, this Mac's short host name, for its tab ("mac: ~"), and whether it's in tmux
/// (`kept`: quitting Cue doesn't end it). One still running in tmux is attached again, as it was.
#[tauri::command]
pub(crate) async fn shell_open(app: tauri::AppHandle, id: String, cwd: String, cols: u16, rows: u16) -> Result<serde_json::Value, String> {
    let (gen, kept) = tauri::async_runtime::spawn_blocking(move || shells::open(app, &id, &cwd, cols, rows)).await.map_err(|e| e.to_string())??;
    let host = crate::machines::hooks::hostname();
    Ok(serde_json::json!({ "gen": gen, "kept": kept, "host": host.split('.').next().unwrap_or(&host) }))
}

/// The shells still running in tmux, oldest first ({id, name, cwd}), and this Mac's short host name:
/// the window gives each its tab back.
#[tauri::command]
pub(crate) async fn shell_list(app: tauri::AppHandle) -> serde_json::Value {
    let list = tauri::async_runtime::spawn_blocking(move || shells::list(app)).await.unwrap_or_default();
    let host = crate::machines::hooks::hostname();
    serde_json::json!({ "shells": list, "host": host.split('.').next().unwrap_or(&host) })
}

/// A shell's tab renamed ("" for none): kept with the shell, for the next Cue.
#[tauri::command]
pub(crate) async fn shell_name(id: String, name: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || shells::rename(&id, &name)).await;
}

/// ⌘K in a shell in tmux.
#[tauri::command]
pub(crate) async fn shell_clear(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || shells::clear(&id)).await.map_err(|e| e.to_string())?
}

/// ⌘W outside Cue's terminal: the window hides, as closing it does (Cue keeps running).
#[tauri::command]
pub(crate) fn hide_window(app: tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

/// What you type in the built-in terminal.
#[tauri::command]
pub(crate) fn term_write(session_id: String, data: String) -> Result<(), String> {
    term::write(&session_id, &data)
}

#[tauri::command]
pub(crate) fn term_resize(session_id: String, cols: u16, rows: u16) -> Result<(), String> {
    term::resize(&session_id, cols, rows)
}

/// Close it (the session keeps running). A shell's tab closed: the shell ends.
#[tauri::command]
pub(crate) fn term_close(session_id: String) {
    if session_id.starts_with(shells::PREFIX) {
        return shells::close(&session_id);
    }
    term::close(&session_id);
}
