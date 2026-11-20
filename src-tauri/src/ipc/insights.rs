//! Insights: what the window reads to show them, and what it records about them. The rules themselves
//! live in the window (ui/insights.js): they work from each session's own log (its turns' tokens, its
//! context, when its cache runs out). What's sent to a session is what you click (Cue's hand-off ask, and
//! the first message of the session a hand-off continues in), and the hand-off ask before a cache runs
//! out where you turned that on (a session's "Hand off if I don't come back", or Settings: hub/activity.rs).

use super::{blocking, with_hub};
use crate::db;
use crate::hub::Hub;
use crate::logs::steps;
use std::sync::Arc;
// Kept at this path for the tests and the add-on.
#[allow(unused_imports)]
pub(crate) use crate::hub::{handoff_continue_now, handoff_cue_ahead, handoff_cue_now, handoff_fresh_now, handoff_keep_now, notice_handoff, record_insight, AHEAD_ALL, AHEAD_DESK, AHEAD_FULL, AHEAD_SESSION, AHEAD_YOU, CUE_HANDOFF};
use crate::hub::{local_log, notice_handoff_in};
use tauri::State;

/// For each session asked about: when its cached context runs out (0: its log doesn't say), its context
/// now (what the next reply re-writes once the cache is gone), the context it started with, and when its
/// latest reply came. Waiting
/// cards count down from it and say how far a session has grown.
#[tauri::command]
pub(crate) async fn insight_state(hub: State<'_, Arc<Hub>>, session_ids: Vec<String>) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| insight_states(&h, &session_ids)).await
}

/// The numbers insight_state answers with (it reads the sessions' logs: call it off the main thread).
/// A hand-off note one of them wrote is taken here too (notice_handoff_in): a machine's log copy can
/// trail its turn's end, when Cue looks first.
pub(crate) fn insight_states(h: &Hub, session_ids: &[String]) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    let mut noticed = false;
    for sid in session_ids.iter().take(50) {
        let Some(log) = local_log(h, sid) else { continue };
        noticed |= notice_handoff_in(sid, &log);
        if let Some(st) = steps::insight_state(&log) {
            out.insert(sid.clone(), serde_json::json!(st));
        }
    }
    if noticed {
        h.redraw();
    }
    serde_json::Value::Object(out)
}

/// Each host's hand-off skills for Claude Code (`hosts`: "" is this Mac, then your machines by name): a
/// list, or null while it's read for the first time; `reading` is true while a newer list is on its way
/// for any of them; `commands`: every command it has (name, kind, desc), for choosing one in Settings. `fresh`: every folder's command list is read again (after you add a skill somewhere),
/// so sessions' "/" menus and insights see it too, not only this list.
#[tauri::command]
pub(crate) async fn insight_host_skills(fresh: Option<bool>) -> Result<serde_json::Value, String> {
    let fresh = fresh.unwrap_or(false);
    blocking(move || {
        if fresh {
            crate::commands::reread_all();
        }
        let mut out = serde_json::Map::new();
        let mut all = serde_json::Map::new();
        let mut reading = false;
        for host in std::iter::once(String::new()).chain(crate::machines::list().into_iter().map(|m| m.name)) {
            out.insert(host.clone(), serde_json::json!(crate::commands::handoff_skills(&host)));
            // Everything it has, for choosing one in Settings (null while it's read).
            let cmds = crate::commands::on_host(&host)
                .map(|l| l.into_iter().map(|c| serde_json::json!({ "name": c.name, "kind": c.kind, "desc": c.desc })).collect::<Vec<_>>());
            all.insert(host.clone(), serde_json::json!(cmds));
            reading |= crate::commands::reading(&crate::commands::home_folder(&host), &host);
        }
        serde_json::json!({ "hosts": out, "commands": all, "reading": reading })
    })
    .await
}

/// You sent a session its hand-off skill (`skill`, without the slash): remembered until it continues in a
/// new session, so "Continue in a new session" is still offered after a restart. `resume`: the skill takes
/// "resume" (its description says so); otherwise Cue's own message starts the next session.
#[tauri::command]
pub(crate) async fn handoff_sent(hub: State<'_, Arc<Hub>>, session_id: String, skill: String, resume: Option<bool>) -> Result<(), String> {
    let skill: String = skill.trim_start_matches('/').chars().take(120).collect();
    if skill.is_empty() || skill.contains(char::is_whitespace) {
        return Ok(());
    }
    with_hub(&hub, move |h| {
        db::handoff_sent(&session_id, &skill, resume.unwrap_or(true) && skill != CUE_HANDOFF, 0);
        h.redraw();
    })
    .await
}

/// Cue's own hand-off: sends the session HANDOFF_ASK (your click is the message), queued if it's working.
/// `ahead`: Write hand-off now, before its cache runs out (its session then stays open when it continues).
#[tauri::command]
pub(crate) async fn handoff_cue(hub: State<'_, Arc<Hub>>, session_id: String, ahead: Option<bool>) -> Result<String, String> {
    let ahead = if ahead.unwrap_or(false) { AHEAD_YOU } else { 0 };
    with_hub(&hub, move |h| handoff_cue_ahead(&h, &session_id, ahead)).await?
}

/// "Continue here" on a note written ahead: it's dropped (the session goes on with all of its context).
#[tauri::command]
pub(crate) async fn handoff_keep(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    with_hub(&hub, move |h| handoff_keep_now(h, &session_id)).await
}

/// A session's "Hand off if I don't come back", on or off.
#[tauri::command]
pub(crate) async fn ahead_watch(hub: State<'_, Arc<Hub>>, session_id: String, on: bool) -> Result<(), String> {
    with_hub(&hub, move |h| {
        db::ahead_watch(&session_id, on);
        h.redraw();
    })
    .await
}

/// "Continue in a new session": a new session of the same agent, in the same folder and on the same
/// machine (Claude Code: the same model), started with `/<skill> resume`, or Cue's own resume message when
/// the skill doesn't take one (your click is the message); then the old one closes (Closed keeps it),
/// unless background agents are still at work there. Linked both ways.
#[tauri::command]
pub(crate) async fn handoff_continue(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| handoff_continue_now(&h, &session_id)).await?
}

/// A session whose cache is gone: a new session of the same agent, folder and machine (Claude Code: the same
/// model), started from its last reply instead of resuming the whole context. An open one closes (Closed keeps
/// it), as every way to a fresh session does; a parked or closed one stays where it is.
#[tauri::command]
pub(crate) async fn handoff_fresh(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<serde_json::Value, String> {
    with_hub(&hub, move |h| handoff_fresh_now(&h, &session_id)).await?
}

/// One thing an insight did, or you did with it: shown, acted (with what you picked), later, dismissed.
/// `tokens`: what acting on it saved, where that's known when it happens.
#[tauri::command]
pub(crate) async fn insight_log(session_id: String, rule: String, action: String, detail: Option<String>, tokens: Option<u64>) -> Result<(), String> {
    blocking(move || record_insight(&session_id, &rule, &action, &detail.unwrap_or_default(), tokens.unwrap_or(0))).await
}

/// The last 30 days, per insight: shown, acted on, and the tokens acting on them saved.
#[tauri::command]
pub(crate) async fn insight_counts() -> Result<serde_json::Value, String> {
    let since = crate::model::now_ms().saturating_sub(30 * 24 * 3600 * 1000);
    let counts = blocking(move || db::insight_counts(since)).await?;
    Ok(serde_json::Value::Array(counts.into_iter().map(|(rule, shown, acted, tokens)| serde_json::json!({ "rule": rule, "shown": shown, "acted": acted, "tokens": tokens })).collect()))
}
