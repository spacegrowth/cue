//! What's waiting on you and a session's small flags: the commands that read or change the queue.

use super::{blocking, with_hub};
use crate::model;
use crate::hub::Hub;
use crate::model::Decision;
use std::sync::Arc;
use tauri::State;

/// The window's state as last built (hub/shown.rs): never waits for the store, and not for the blocking
/// pool either (busy hub calls can fill it). Only the first ask, before anything is built, builds it there.
#[tauri::command]
pub(crate) async fn get_state(hub: State<'_, Arc<Hub>>) -> Result<serde_json::Value, String> {
    match hub.kept_state() {
        Some(v) => Ok(v),
        None => with_hub(&hub, |h| h.state()).await,
    }
}

/// The window that asks drew the last state it was sent: the next can go to it (hub/redraw.rs, DRAWN_WAIT). Not on
/// the main thread (a command that isn't async runs there), and never waits.
#[tauri::command]
pub(crate) async fn state_drawn(window: tauri::WebviewWindow, hub: State<'_, Arc<Hub>>) -> Result<(), String> {
    hub.state_drawn(window.label());
    Ok(())
}

/// A card in full, waiting or in History: the state's History rows are brief (hub/kept_history.rs), and a
/// row opened (in History, or in Active from Recently answered) asks for the rest. None once it's gone.
#[tauri::command]
pub(crate) async fn history_item(hub: State<'_, Arc<Hub>>, id: String) -> Result<Option<model::Item>, String> {
    with_hub(&hub, move |h| h.get(&id)).await
}

#[tauri::command]
pub(crate) async fn respond(hub: State<'_, Arc<Hub>>, id: String, decision: Decision) -> Result<bool, String> {
    with_hub(&hub, move |h| h.respond(&id, decision)).await
}

#[tauri::command]
pub(crate) async fn dismiss(hub: State<'_, Arc<Hub>>, id: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.dismiss(&id)).await
}

/// × on a session whose machine can't be reached: take it out of Cue.
#[tauri::command]
pub(crate) async fn forget_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.forget(&session_id)).await
}

/// Skip on a queued review (its member's row in the crew tray): take it back, unless it went meanwhile.
#[tauri::command]
pub(crate) async fn unqueue_review(hub: State<'_, Arc<Hub>>, exec: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.unqueue_review(&exec)).await?
}

#[tauri::command]
pub(crate) async fn send_to_back(hub: State<'_, Arc<Hub>>, id: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.send_to_back(&id)).await?
}

/// × on the chat's "didn't take the new name" notice.
#[tauri::command]
pub(crate) async fn rename_dismiss(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.dismiss_rename_error(&session_id)).await
}

/// The message Cue kept for a session while it worked, back into your box (Esc, Edit): its text and images.
#[tauri::command]
pub(crate) async fn unhold_message(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<Option<model::Exchange>, String> {
    with_hub(&hub, move |h| h.unhold(&session_id)).await
}

/// Send the kept message now: stops the turn first (as Send now on a queued one).
#[tauri::command]
pub(crate) async fn send_held_now(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| h.send_held(&session_id, true)).await?
}

/// "Send at…": `text` kept for the session, sent by Cue at `at_ms` (ms since the epoch).
#[tauri::command]
pub(crate) async fn send_at(hub: State<'_, Arc<Hub>>, session_id: String, text: String, at_ms: u64) -> Result<String, String> {
    with_hub(&hub, move |h| h.send_at(&session_id, &text, at_ms)).await?
}

/// The message for later dropped (Cancel), or taken back to change (Edit): its text, if there was one.
#[tauri::command]
pub(crate) async fn send_at_drop(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<Option<String>, String> {
    with_hub(&hub, move |h| h.drop_send_at(&session_id).map(|l| l.text)).await
}

/// The message for later, sent now.
#[tauri::command]
pub(crate) async fn send_at_now(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| h.send_at_now(&session_id)).await?
}

/// "Later" on a session: put it off (its finished turns wait under Need to decide), or back.
#[tauri::command]
pub(crate) async fn set_later(hub: State<'_, Arc<Hub>>, session_id: String, on: bool) -> Result<(), String> {
    with_hub(&hub, move |h| h.set_later(&session_id, on)).await
}

/// "Tell me" on a working session's turn: told when it's done, even with Cue in front. Or off.
#[tauri::command]
pub(crate) async fn set_watch(hub: State<'_, Arc<Hub>>, session_id: String, on: bool) -> Result<(), String> {
    with_hub(&hub, move |h| h.set_watch(&session_id, on)).await
}

/// Star a session (you're following it), or unstar it.
#[tauri::command]
pub(crate) async fn set_starred(hub: State<'_, Arc<Hub>>, session_id: String, on: bool) -> Result<(), String> {
    with_hub(&hub, move |h| h.set_starred(&session_id, on)).await
}

#[tauri::command]
pub(crate) async fn answer_terminal(hub: State<'_, Arc<Hub>>, id: String, n: usize) -> Result<String, String> {
    let hub = hub.inner().clone();
    blocking(move || hub.answer_terminal(&id, n)).await?
}

#[tauri::command]
pub(crate) async fn reply(hub: State<'_, Arc<Hub>>, id: String, text: String, images: Option<Vec<model::Upload>>) -> Result<String, String> {
    let images = images.unwrap_or_default();
    with_hub(&hub, move |h| h.reply(&id, &text, &images)).await?
}
