//! What's waiting on you and a session's small flags: the commands that read or change the queue.

use crate::model;
use crate::hub::Hub;
use crate::model::Decision;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub(crate) fn get_state(hub: State<Arc<Hub>>) -> serde_json::Value {
    hub.snapshot()
}

#[tauri::command]
pub(crate) fn respond(hub: State<Arc<Hub>>, id: String, decision: Decision) -> bool {
    hub.respond(&id, decision)
}

#[tauri::command]
pub(crate) fn dismiss(hub: State<Arc<Hub>>, id: String) {
    hub.dismiss(&id)
}

/// × on a session whose machine can't be reached: take it out of Cue.
#[tauri::command]
pub(crate) fn forget_session(hub: State<Arc<Hub>>, session_id: String) {
    hub.forget(&session_id)
}

/// × on a queued review in its lead's chat: take it back.
#[tauri::command]
pub(crate) fn unqueue_review(hub: State<Arc<Hub>>, exec: String) {
    hub.unqueue_review(&exec)
}

#[tauri::command]
pub(crate) fn send_to_back(hub: State<Arc<Hub>>, id: String) -> Result<(), String> {
    hub.send_to_back(&id)
}

/// × on the chat's "didn't take the new name" notice.
#[tauri::command]
pub(crate) fn rename_dismiss(hub: State<'_, Arc<Hub>>, session_id: String) {
    hub.dismiss_rename_error(&session_id);
}

/// The message Cue kept for a session while it worked, back into your box (Esc, Edit): its text and images.
#[tauri::command]
pub(crate) fn unhold_message(hub: State<Arc<Hub>>, session_id: String) -> Option<model::Exchange> {
    hub.unhold(&session_id)
}

/// Send the kept message now: stops the turn first (as Send now on a queued one).
#[tauri::command]
pub(crate) async fn send_held_now(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.send_held(&session_id, true)).await.map_err(|e| e.to_string())?
}

/// "Later" on a session: put it off (its finished turns wait under Need to decide), or back.
#[tauri::command]
pub(crate) fn set_later(hub: State<Arc<Hub>>, session_id: String, on: bool) {
    hub.set_later(&session_id, on);
}

/// Star a session (you're following it), or unstar it.
#[tauri::command]
pub(crate) fn set_starred(hub: State<Arc<Hub>>, session_id: String, on: bool) {
    hub.set_starred(&session_id, on);
}

#[tauri::command]
pub(crate) async fn answer_terminal(hub: State<'_, Arc<Hub>>, id: String, n: usize) -> Result<String, String> {
    let hub = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || hub.answer_terminal(&id, n)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn reply(hub: State<'_, Arc<Hub>>, id: String, text: String, images: Option<Vec<model::Upload>>) -> Result<String, String> {
    let h = hub.inner().clone();
    let images = images.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || h.reply(&id, &text, &images)).await.map_err(|e| e.to_string())?
}
