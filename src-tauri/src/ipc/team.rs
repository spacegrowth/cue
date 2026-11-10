//! A hosted lead's teammate's buttons in its chat (docs/dev/cue-teams.md §4–§5): hub/team.rs does them.

use super::with_hub;
use crate::hub::Hub;
use std::sync::Arc;
use tauri::State;

/// Review, Diff (answers with the diff), Commit, Send fixes, Stop or Set aside on `agent` (its agent id), a teammate
/// of hosted lead `session_id`.
#[tauri::command]
pub(crate) async fn team_action(hub: State<'_, Arc<Hub>>, session_id: String, agent: String, action: String) -> Result<String, String> {
    with_hub(&hub, move |h| h.team_action(&session_id, &agent, &action)).await?
}

/// The Team switch on a hosted lead's box: "auto" (the lead decides), "on" (split what you send among teammates),
/// "off" (no teammates).
#[tauri::command]
pub(crate) async fn team_mode(hub: State<'_, Arc<Hub>>, session_id: String, mode: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.set_team_mode(&session_id, &mode)).await?
}

/// The Crew setting for a lead's crew members whose review is clean: "clean" (commit on a branch of its own) or
/// "wait" (Commit is yours).
#[tauri::command]
pub(crate) async fn crew_commit(hub: State<'_, Arc<Hub>>, session_id: String, mode: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.set_crew_commit(&session_id, &mode)).await?
}

/// A task dropped on a hosted lead, before it goes: the files it names and who touches each now.
#[tauri::command]
pub(crate) async fn team_hand_out_preview(hub: State<'_, Arc<Hub>>, session_id: String, text: String) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.hand_out_preview(&session_id, &text)).await?
}

/// The dropped task, sent to the lead to hand out as a new teammate.
#[tauri::command]
pub(crate) async fn team_hand_out(hub: State<'_, Arc<Hub>>, session_id: String, text: String) -> Result<String, String> {
    with_hub(&hub, move |h| h.hand_out(&session_id, &text)).await?
}
