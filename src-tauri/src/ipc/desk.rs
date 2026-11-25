//! The Desk's commands (desk/, desk/hub/lead/, desk/hub/core/desk.rs, desk/hub/lead/run.rs): picking a project's lead, writing to it, adding an
//! item, giving one to a session, what you do to an item, and undoing a decision.

use super::with_hub;
use crate::hub::Hub;
pub(crate) use crate::desk::hub::lead::{desk_send_images, desk_send_now, desk_send_with, desk_set_lead_with, desk_start_with_agent};
// Kept at this path for the tests.
#[allow(unused_imports)]
pub(crate) use crate::desk::hub::lead::{desk_start_named, DeskStart};
use std::sync::Arc;
use tauri::State;

/// Write to `project`'s lead: each message sent after the Desk's summary; resumed with it when parked. An error
/// ("no lead", "lead ended") when there's none to write to. `now` (⌘↵): sent to a lead at work at once, not kept in
/// Cue until its turn ends. Returns { sid }.
#[tauri::command]
pub(crate) async fn desk_send(hub: State<'_, Arc<Hub>>, project: String, text: String, images: Option<Vec<crate::model::Upload>>, now: Option<bool>) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| desk_send_now(h, &project, &text, &images.unwrap_or_default(), now.unwrap_or(false))).await?
}

/// Pick `project`'s lead: session `sid` (of the project: live, parked or ended), or, without one, a new hosted
/// Claude session in the project's folder named "<project>-lead". The lead is sent the Desk (with how a lead works,
/// for a session that didn't start as one); a parked or ended one is resumed with it. The lead it replaces, still
/// open, is told. `trust`: you said you trust the folder, when Cue asked (as + New does). Returns { sid }.
#[tauri::command]
pub(crate) async fn desk_set_lead(hub: State<'_, Arc<Hub>>, project: String, sid: Option<String>, trust: Option<bool>) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| desk_set_lead_with(h, &project, sid.as_deref().filter(|s| !s.is_empty()), trust.unwrap_or(false))).await?
}

/// The lead Desk `project` had before session `sid` ran it, for "Show earlier" over the lead chat's hand-off band
/// (#792): its name, when it handed off, how many messages it has; null when `sid` was the first.
#[tauri::command]
pub(crate) async fn desk_lead_before(hub: State<'_, Arc<Hub>>, project: String, sid: String) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_lead_before(&project, &sid)).await
}

/// A new Desk in `folder` (Desk home's + New Desk, + New's Start a Desk): the folder's project, its lead a new
/// session Cue starts there. Every call creates an independent Desk. Returns { id, project: id, sid }.
#[tauri::command]
pub(crate) async fn desk_start(hub: State<'_, Arc<Hub>>, folder: String, trust: Option<bool>, name: Option<String>, agent: Option<String>) -> Result<serde_json::Value, String> {
    let agent = match agent.as_deref() {
        None => crate::hosted::Agent::Claude,
        Some(a) => crate::hosted::Agent::named(a).ok_or_else(|| format!("unknown agent: {a}"))?,
    };
    with_hub(&hub, move |h| desk_start_with_agent(h, &folder, trust.unwrap_or(false), name.as_deref(), agent)).await?
}

/// Your answer to Ask me before for `project` (a workspace project): `kinds` ticked, or None for "Not now".
#[tauri::command]
pub(crate) async fn ask_before_set(hub: State<'_, Arc<Hub>>, project: String, kinds: Option<Vec<String>>) -> Result<Vec<&'static str>, String> {
    with_hub(&hub, move |h| h.ask_before_set(&project, kinds.as_deref())).await?
}

/// How work lands on Desk `desk`'s project, from its setting: each part's choice, and `at_once`.
#[tauri::command]
pub(crate) async fn desk_lands_set(hub: State<'_, Arc<Hub>>, desk: String, lands: serde_json::Value) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_lands_set(&desk, &lands)).await?
}

/// New Desk's look at `folder` (#927), one part at a time ("files", "github", "history", "sessions"): { part, lines,
/// found }. Only reads; Cue's names for the folder's past sessions are taken from the store, the files read after.
#[tauri::command]
pub(crate) async fn desk_look(hub: State<'_, Arc<Hub>>, folder: String, part: String) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| {
        let names: std::collections::HashMap<String, String> = if part == "sessions" {
            h.read(|st| st.sessions.iter().filter_map(|s| {
                let n = if !s.name.is_empty() { s.name.clone() } else { s.claude_name.clone().unwrap_or_default() };
                (!n.is_empty()).then(|| (s.origin.session_id.clone(), n))
            }).collect())
        } else {
            Default::default()
        };
        crate::desk::look::look(&folder, &part, &|id| names.get(id).cloned())
    })
    .await?
}

/// New Desk's rows from the four parts' answers (`found`: { files, github, history, sessions }, any missing taken as
/// nothing found): { place, summary, rows, lands }, `lands` ready for desk_lands_set.
#[tauri::command]
pub(crate) async fn desk_look_rows(folder: String, found: serde_json::Value) -> Result<serde_json::Value, String> {
    super::blocking(move || crate::desk::look::rows(&folder, &found)).await?
}

/// Desk `desk`'s default model for new sessions (#620): "" or "default" for Claude Code's own pick.
#[tauri::command]
pub(crate) async fn desk_model_set(hub: State<'_, Arc<Hub>>, desk: String, model: String) -> Result<String, String> {
    with_hub(&hub, move |h| h.desk_model_set(&desk, &model)).await?
}

/// How work lands' Remove old working folders: the Desk's repository looked at now, its old copies and their size.
#[tauri::command]
pub(crate) async fn desk_old_copies_list(hub: State<'_, Arc<Hub>>, desk: String) -> Result<crate::hub::OldCopies, String> {
    with_hub(&hub, move |h| h.desk_old_copies_list(&desk)).await?
}

/// Your answer to Desk `desk`'s list of copies whose work is already in the app (#376): `yes` removes them (each
/// looked at again first), no removes nothing. Returns what was removed and what stayed.
#[tauri::command]
pub(crate) async fn desk_old_copies(hub: State<'_, Arc<Hub>>, desk: String, yes: bool) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_old_copies_answer(&desk, yes)).await?
}

/// Your Allow (`allow` true) or No to a step held while you were away (desk/hub/away/away_holds.rs).
#[tauri::command]
pub(crate) async fn desk_held_answer(hub: State<'_, Arc<Hub>>, id: String, allow: bool) -> Result<(), String> {
    with_hub(&hub, move |h| h.desk_held_answer(&id, allow)).await?
}

/// Away mode for Desk `desk` (`away`: { on, until, budget, rules }): saved, its lead told. Returns its view.
#[tauri::command]
pub(crate) async fn desk_away_set(hub: State<'_, Arc<Hub>>, desk: String, away: serde_json::Value) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_away_set(&desk, &away)).await?
}

/// Pause (`on`) or resume Desk `desk` (#366): its sessions stop at a safe point, or carry on. Returns its view.
#[tauri::command]
pub(crate) async fn desk_pause(hub: State<'_, Arc<Hub>>, desk: String, on: bool) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| {
        h.desk_pause_set(&desk, on, ("You", ""), "")?;
        Ok(crate::hub::desk_pause_view(&desk))
    })
    .await?
}

/// The owner turned Away on: Desk `desk`'s lead (woken if parked) is asked to settle what it can under `rules` and
/// leave at most 5 questions; the Desk shows it's sorting things out until its next turn ends. Only when something
/// changed on the Desk since its last pass (#612): true when it was asked, false when there was nothing to go through.
#[tauri::command]
pub(crate) async fn desk_away_sweep(hub: State<'_, Arc<Hub>>, desk: String, rules: serde_json::Value) -> Result<bool, String> {
    with_hub(&hub, move |h| desk_away_sweep_with(h, &desk, &rules)).await?
}

pub(crate) fn desk_away_sweep_with(h: &Arc<Hub>, desk: &str, rules: &serde_json::Value) -> Result<bool, String> {
    // Nothing new since the lead last went through the items, or it's going through them now: no second pass.
    let Some(pass) = h.desk_away_pass_due(desk) else {
        return Ok(false);
    };
    // Its lead stopped by a usage limit: nothing sent now (Cue carries it on after the reset).
    if !h.desk_away_sweeping(desk) {
        return Ok(false);
    }
    let words = crate::owner::in_prompt(&crate::hub::desk_away_sweep_words(rules, &pass));
    if let Err(e) = desk_send_with(h, desk, &words) {
        h.desk_away_sweep_failed(desk);
        return Err(e);
    }
    Ok(true)
}

#[tauri::command]
pub(crate) async fn desk_delete(hub: State<'_, Arc<Hub>>, id: String, close_sessions: Option<Vec<String>>) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| {
        let requested = close_sessions.unwrap_or_default();
        let eligible = h.desk_delete_selected(&id, &requested)?;
        let mut kept = Vec::new();
        for sid in &requested {
            if !eligible.contains(sid) || h.desk_session_shared(sid) { kept.push(serde_json::json!({ "sid": sid, "reason": "shared with another Desk or no longer on this Desk" })); continue; }
            if let Err(e) = super::sessions::end_session(h, sid) { kept.push(serde_json::json!({ "sid": sid, "reason": e })); }
        }
        Ok(serde_json::json!({ "kept": kept }))
    }).await?
}

#[tauri::command]
pub(crate) async fn desk_rename(hub: State<'_, Arc<Hub>>, id: String, name: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.desk_rename(&id, &name)).await?
}

/// The "New item" button, or "Add to Desk" on a session (`holder`: that session, which holds it). Returns the item.
#[tauri::command]
pub(crate) async fn desk_add(hub: State<'_, Arc<Hub>>, project: String, kind: String, title: String, see: Option<String>, done_when: Option<String>, holder: Option<String>, images: Option<Vec<crate::model::Upload>>) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| h.desk_add_with(&project, &kind, &title, see.as_deref().unwrap_or(""), done_when.as_deref().unwrap_or(""), holder.as_deref().filter(|s| !s.is_empty()), &images.unwrap_or_default())).await?
}

/// "Give to…": item `id` to session `sid`, which is sent it. Returns the item as it is now.
#[tauri::command]
pub(crate) async fn desk_give(hub: State<'_, Arc<Hub>>, id: String, sid: String) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| h.desk_give(&id, &sid)).await?
}

/// Session `sid` on `project`'s Desk (`on`) or off it: the sessions its lead routes to without asking you.
#[tauri::command]
pub(crate) async fn desk_member(hub: State<'_, Arc<Hub>>, project: String, sid: String, on: bool) -> Result<(), String> {
    with_hub(&hub, move |h| h.desk_set_member(&project, &sid, on)).await?
}

/// What you did to an item: "looks_right" | "reopen" | "send_back" | "start_now" | "put_before" | "install_now" | "skip_install"
/// | "rerun" | "cut" | "drop" | "remove_kept", with `text` for send_back (your words), cut (the item it no longer
/// waits on). Returns the item as it is now (state "dropped" once dropped).
#[tauri::command]
pub(crate) async fn desk_item_act(hub: State<'_, Arc<Hub>>, id: String, act: String, text: Option<String>, images: Option<Vec<crate::model::Upload>>) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| {
        // Pictures sent back with it: saved, and their paths go with your words, as an item's own pictures do.
        let saved = crate::uploads::save(&images.unwrap_or_default())?;
        let mut text = text.unwrap_or_default();
        if !saved.is_empty() {
            let paths: Vec<String> = saved.iter().map(|s| format!("`{}`", s.path)).collect();
            text = format!("{}{}{}", text.trim(), crate::desk::YOUR_PICTURES, paths.join(" "));
        }
        h.desk_item_act(&id, &act, &text)
    })
    .await?
}

/// Install now on Desk `project`'s group waiting for its update after landing (#911): its after-landing steps run now,
/// as the lead's INSTALL: main runs them. Ok with what started; Err with why nothing did.
#[tauri::command]
pub(crate) async fn desk_install_now(hub: State<'_, Arc<Hub>>, project: String) -> Result<String, String> {
    with_hub(&hub, move |h| {
        let said = h.desk_lead_install(&project);
        h.desk_install_watch();
        said
    })
    .await?
}

/// Looks right on item `id`: its changed files committed at once (word check first, no push) and it's done; none, just
/// done; with files another session is editing, done and the commit waits. { commit, files, waiting } (desk/hub/landing/commit.rs).
#[tauri::command]
pub(crate) async fn desk_looks_right(hub: State<'_, Arc<Hub>>, id: String) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_looks_right(&id)).await?
}

/// Item `id`'s Files changed as one diff: to check, what Looks right would commit; done, its commit (or what waits
/// to be). { diff, machine } (desk/hub/landing/diff.rs).
#[tauri::command]
pub(crate) async fn desk_item_diff(hub: State<'_, Arc<Hub>>, id: String) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| h.desk_item_diff(&id)).await?
}

/// Undo on Looks right's commit: taken back while it's the last one (its changes stay), the item ready to check again.
#[tauri::command]
pub(crate) async fn desk_commit_undo(hub: State<'_, Arc<Hub>>, id: String) -> Result<crate::desk::DeskItem, String> {
    with_hub(&hub, move |h| h.desk_commit_undo(&id)).await?
}

/// `project`'s done items, newest first, `take` of them after the one the list ends with (`before_ms`, `before_id`):
/// the Desk's list loads them as you scroll.
#[tauri::command]
pub(crate) async fn desk_done(hub: State<'_, Arc<Hub>>, project: String, before_ms: Option<u64>, before_id: Option<String>, take: u32) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| shown(h.desk_done_page(&project, before_ms.zip(before_id.as_deref()), take))).await
}

/// `list` as the screen shows it: "the owner" as your name (owner.rs).
fn shown<T: serde::Serialize>(list: T) -> serde_json::Value {
    let mut v = serde_json::to_value(list).unwrap_or_default();
    crate::owner::on_screen_json(&mut v);
    v
}

/// `project`'s lead's own moves (#374), newest first, `take` of them after the one the list ends with (`before_ms`,
/// `before_id`): "What the lead moved" loads them as you scroll.
#[tauri::command]
pub(crate) async fn desk_moves(hub: State<'_, Arc<Hub>>, project: String, before_ms: Option<u64>, before_id: Option<String>, take: u32) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| shown(h.desk_moves_page(&project, before_ms.zip(before_id.as_deref()), take))).await
}

/// Clear on "What the lead moved": the list starts again from now.
#[tauri::command]
pub(crate) async fn desk_moves_clear(hub: State<'_, Arc<Hub>>, project: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.desk_moves_clear(&project)).await?
}

/// Keep (Got it, Keep all) on calls made while you were away (#863): closed, nothing sent.
#[tauri::command]
pub(crate) async fn desk_calls_keep(hub: State<'_, Arc<Hub>>, ids: Vec<String>) -> Result<(), String> {
    with_hub(&hub, move |h| h.desk_calls_keep(&ids)).await?
}

/// Undo this: whoever made the decision is asked to undo it. What Cue did.
#[tauri::command]
pub(crate) async fn desk_undo(hub: State<'_, Arc<Hub>>, decision_id: String, answer: Option<String>) -> Result<String, String> {
    with_hub(&hub, move |h| h.desk_undo_with(&decision_id, answer.as_deref().unwrap_or(""))).await?
}
