//! The chat of a session: its box (sending, interrupting, commands), steps and log, search, crew actions.

use crate::db;
use crate::commands;
use crate::leads;
use crate::model;
use crate::server;
use crate::logs::transcript;
use crate::logs::steps;
use crate::logs::live;
use crate::logs::archive;
use crate::terminal::focus;
use crate::hub::Hub;
use std::sync::Arc;
use tauri::State;

/// The chat's ⋯ menu: Compact, or a relay lead's Hand off (typed into the session; opens no window).
#[tauri::command]
pub(crate) async fn session_command(hub: State<'_, Arc<Hub>>, session_id: String, action: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.session_command(&session_id, &action)).await.map_err(|e| e.to_string())?
}

/// The "/" menu for a session's box. Claude Code: the commands it has enabled for the folder, read
/// from it once and kept a week. Pi: its own commands plus what its extension reported when it
/// connected. Codex: its own commands. None while a list is being read (ask again shortly). `fresh`:
/// Refresh was pressed: every kept Claude Code list is forgotten and this one read again; a Pi
/// session is asked for its list again.
#[tauri::command]
pub(crate) fn session_commands(hub: State<Arc<Hub>>, session_id: String, fresh: Option<bool>) -> Option<Vec<commands::Cmd>> {
    let fresh = fresh == Some(true);
    if fresh {
        commands::forget_all();
    }
    // A session that reported its commands (Pi): those, or a fresh set once it answers.
    if hub.agent_commands(&session_id).is_some() && fresh && hub.ask_agent_commands(&session_id) {
        return None;
    }
    if let Some(list) = hub.agent_commands(&session_id) {
        return Some(list);
    }
    let (cwd, machine) = match hub.session_origin(&session_id) {
        Some(o) if o.harness == "claude" => (o.cwd, o.machine),
        Some(o) if o.harness == "codex" => return Some(commands::for_codex()),
        Some(o) if o.harness == "pi" => return Some(commands::for_pi(&[])),
        Some(_) => return Some(vec![]),
        None => match live::claude().into_iter().find(|q| q.session_id == session_id) {
            Some(q) => (q.cwd, String::new()),
            None => return Some(vec![]),
        },
    };
    commands::for_folder(&cwd, &machine)
}

/// The models to pick from in + New: what your Claude Code offers, read in your home folder (the list
/// is Claude Code's, not a folder's). The window keeps it until you press Reload (`fresh`), which asks
/// again. None while it's being read (ask again shortly).
#[tauri::command]
pub(crate) fn models_list(fresh: bool) -> Option<Vec<commands::Arg>> {
    commands::models_for(&std::env::var("HOME").unwrap_or_default(), fresh)
}

/// Off the window's thread: finding the tab runs AppleScript over every terminal window, and waits on
/// macOS's Automation consent dialog the first time.
#[tauri::command]
pub(crate) async fn focus_session(hub: State<'_, Arc<Hub>>, id: String) -> Result<String, String> {
    let it = hub.get(&id).ok_or("no such item")?;
    tauri::async_runtime::spawn_blocking(move || focus::focus(&it.origin)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub(crate) async fn send_to_session(hub: State<'_, Arc<Hub>>, session_id: String, text: String, images: Option<Vec<model::Upload>>, now: Option<bool>) -> Result<String, String> {
    let h = hub.inner().clone();
    let images = images.unwrap_or_default();
    let now = now.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || h.send_to_session(&session_id, &text, &images, now)).await.map_err(|e| e.to_string())?
}

/// "Send now" on a queued message: stop the current turn so the agent reads it straight away.
#[tauri::command]
pub(crate) async fn send_queued_now(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.send_queued_now(&session_id)).await.map_err(|e| e.to_string())?
}

/// Stop a working session mid-turn (the Stop button, Esc twice).
#[tauri::command]
pub(crate) async fn interrupt_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.interrupt(&session_id)).await.map_err(|e| e.to_string())?
}

/// Search everything Cue has logged: session names, your messages, the agents' replies (ended sessions too).
#[tauri::command]
pub(crate) fn search(q: String) -> Vec<serde_json::Value> {
    search_all(&q)
}

/// Search, as the window's: Cue's own sessions and messages, then Claude Code's older sessions by name.
pub(crate) fn search_all(q: &str) -> Vec<serde_json::Value> {
    let mut out = db::search(q, 60);
    out.extend(archive::find(q, &db::known_session_ids(), 20));
    out
}

/// What a live session did, step by step (the chat's step lines), from its transcript. `version`: what
/// the caller has; unchanged, only the version comes back. Claude Code sessions only for now.
#[tauri::command]
pub(crate) async fn session_steps(hub: State<'_, Arc<Hub>>, session_id: String, version: Option<u64>) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || steps_for(&h, &session_id, version.unwrap_or(0))).await.map_err(|e| e.to_string())
}

/// Opened steps: each one's full command or path and its output or diff.
#[tauri::command]
pub(crate) async fn step_detail(hub: State<'_, Arc<Hub>>, session_id: String, ids: Vec<String>) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || step_detail_for(&h, &session_id, &ids)).await.map_err(|e| e.to_string())
}

/// The log a session's steps come from: a Claude Code session's transcript (a quiet one's found through
/// Claude Code's own list of running sessions), a Codex session's rollout log, or a Pi session's log.
pub(crate) fn steps_path(h: &Hub, session_id: &str) -> Option<String> {
    let origin = h.session_origin(session_id);
    if origin.as_ref().is_some_and(|o| o.harness == "pi") {
        return pi_log(session_id);
    }
    origin
        .filter(|o| (o.harness == "claude" || o.harness == "codex") && !o.transcript_path.is_empty())
        .map(|o| o.transcript_path)
        .or_else(|| live::claude().into_iter().find(|q| q.session_id == session_id).and_then(|q| live::claude_transcript(&q.cwd, session_id)))
        .or_else(|| pi_log(session_id))
        .or_else(|| codex_log(session_id))
}

/// A Codex session's rollout log, `~/.codex/sessions/<year>/<month>/<day>/rollout-<time>-<session id>.jsonl`
/// (CODEX_HOME moves ~/.codex), newest days first. Looked up once per session.
pub(crate) fn codex_log(session_id: &str) -> Option<String> {
    static FOUND: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);
    if session_id.len() < 8 {
        return None;
    }
    if let Some(p) = crate::locking::lock(&FOUND, "FOUND").get_or_insert_with(Default::default).get(session_id).filter(|p| std::path::Path::new(p).is_file()) {
        return Some(p.clone());
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let root = std::env::var("CODEX_HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".codex")).join("sessions");
    let tail = format!("-{session_id}.jsonl");
    let sorted = |dir: &std::path::Path| -> Vec<std::path::PathBuf> {
        let mut v: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().filter_map(Result::ok).map(|e| e.path()).collect();
        v.sort();
        v.reverse();
        v
    };
    for year in sorted(&root) {
        for month in sorted(&year) {
            for day in sorted(&month) {
                if let Some(f) = sorted(&day).into_iter().find(|f| f.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&tail))) {
                    let found = f.to_string_lossy().into_owned();
                    crate::locking::lock(&FOUND, "FOUND").get_or_insert_with(Default::default).insert(session_id.to_string(), found.clone());
                    return Some(found);
                }
            }
        }
    }
    None
}

/// A Pi session's log: `<Pi's folder>/sessions/<its folder>/<time>_<session id>.jsonl` (Pi's folder is
/// ~/.pi/agent, or PI_CODING_AGENT_DIR). Looked up once per session.
pub(crate) fn pi_log(session_id: &str) -> Option<String> {
    static FOUND: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);
    if session_id.is_empty() {
        return None;
    }
    if let Some(p) = crate::locking::lock(&FOUND, "FOUND").get_or_insert_with(Default::default).get(session_id).filter(|p| std::path::Path::new(p).is_file()) {
        return Some(p.clone());
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let root = std::env::var("PI_CODING_AGENT_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".pi/agent")).join("sessions");
    let tail = format!("_{session_id}.jsonl");
    let found = std::fs::read_dir(root).ok()?.filter_map(Result::ok).filter_map(|d| std::fs::read_dir(d.path()).ok()).flatten().filter_map(Result::ok)
        .map(|f| f.path()).find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&tail)))?
        .to_string_lossy().into_owned();
    crate::locking::lock(&FOUND, "FOUND").get_or_insert_with(Default::default).insert(session_id.to_string(), found.clone());
    Some(found)
}

pub(crate) fn steps_for(h: &Hub, session_id: &str, version: u64) -> serde_json::Value {
    match steps_path(h, session_id) {
        Some(path) => {
            let mut v = steps::steps(&path, version);
            // Claude Code's cost isn't in its transcript: it's what its status line last said.
            if let (Some(meta), Some(cost)) = (v.get_mut("meta").filter(|m| m.is_object() && m["cost"].is_null()), h.session_cost(session_id)) {
                meta["cost"] = serde_json::json!(cost);
            }
            v
        }
        None => serde_json::json!({ "version": 0, "turns": [] }),
    }
}

pub(crate) fn step_detail_for(h: &Hub, session_id: &str, ids: &[String]) -> serde_json::Value {
    let ids: Vec<String> = ids.iter().take(60).cloned().collect();
    steps_path(h, session_id).map(|path| steps::detail(&path, &ids)).unwrap_or_else(|| serde_json::json!({}))
}

/// A session's whole conversation as Cue logged it (search opens it, scrolled to the message you found).
#[tauri::command]
pub(crate) fn session_log(session_id: String) -> Vec<serde_json::Value> {
    db::session_log(&session_id)
}

/// Earlier than Cue's log goes (a session from before Cue, or resumed): the conversation from the
/// session's Claude Code transcript, the `limit` messages before `before_ms`. Only when you ask for it.
#[tauri::command]
pub(crate) async fn transcript_page(session_id: String, before_ms: u64, limit: Option<usize>) -> Vec<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || transcript::find_claude(&session_id).map(|p| transcript::conversation_before(&p, before_ms, limit.unwrap_or(30).min(200))).unwrap_or_default())
        .await
        .unwrap_or_default()
}

/// Earlier messages for the chat, a page at a time as you scroll up: the `limit` before `before_ms`.
#[tauri::command]
pub(crate) fn session_log_page(session_id: String, before_ms: u64, limit: Option<usize>) -> Vec<serde_json::Value> {
    db::session_log_page(&session_id, before_ms, limit.unwrap_or(30).min(200))
}

/// A relay / pi-lead button (Review, Diff, Auto): type the command into the lead, open the page, or run relay.
#[tauri::command]
pub(crate) async fn crew_action(hub: State<'_, Arc<Hub>>, session_id: String, action: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || run_crew_action(&h, &session_id, &action)).await.map_err(|e| e.to_string())?
}

/// What a relay / pi-lead button does.
pub(crate) fn run_crew_action(h: &Hub, session_id: &str, action: &str) -> Result<String, String> {
    // Executors whose lead is gone, all of them at once: closed by relay, or handed to a lead that's running.
    if action == "close-left" {
        let left = leads::close_left_behind(session_id)?;
        let failed: Vec<String> = left.iter().filter_map(|(sid, name, cmd)| cmd.run().and_then(|_| super::sessions::end_if_still_here(h, sid)).err().map(|e| format!("{name}: {e}"))).collect();
        return if failed.is_empty() { Ok(format!("Closed {} executor{}", left.len(), if left.len() == 1 { "" } else { "s" })) } else { Err(failed.join("; ")) };
    }
    if let Some(to) = action.strip_prefix("hand:") {
        let cmds = leads::hand_left_behind(session_id, to)?;
        for cmd in &cmds {
            cmd.run()?;
        }
        return Ok(format!("Handed {} executor{} to {}", cmds.len(), if cmds.len() == 1 { "" } else { "s" }, leads::name_of(to)));
    }
    match leads::act(session_id, action)? {
        // One review at a time per lead: refused while it's busy (see Hub::review).
        leads::Act::Send { to, text } if action == "review" => h.review(&to, session_id, &text),
        leads::Act::Send { to, text } => h.send_to_session(&to, &text, &[], false).map(|_| format!("Sent {text}")),
        leads::Act::Run(cmd, done) => cmd.run().map(|_| done),
        // relay writes the page there and prints its path: bring it here and open it.
        leads::Act::RemoteDiff(cmd) => {
            let said = cmd.run()?;
            let path = said.lines().rev().map(str::trim).find(|l| l.ends_with(".html")).ok_or("relay didn't say where it wrote the page")?.to_string();
            let page = leads::Cmd { bin: "cat".into(), args: vec![path.clone()], machine: cmd.machine.clone() }.run()?;
            let dir = server::cue_dir().join("diffs");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            // The file's name here is made of the crew id and the page's name, both from relay's output
            // on the machine: only plain characters, so they can't name a path outside diffs/.
            let plain = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect::<String>();
            let name = std::path::Path::new(&path).file_name().map(|n| plain(&n.to_string_lossy())).filter(|n| n.ends_with(".html") && n.len() > 5).unwrap_or_else(|| "diff.html".into());
            let here = dir.join(format!("{}-{name}", plain(&cmd.args.get(1).cloned().unwrap_or_default())));
            std::fs::write(&here, page).map_err(|e| e.to_string())?;
            std::process::Command::new("/usr/bin/open").arg(&here).status().map(|_| "Opened in your browser".into()).map_err(|e| e.to_string())
        }
        leads::Act::Open(page) => std::process::Command::new("open").arg(&page).status().map(|_| "Opened in your browser".into()).map_err(|e| e.to_string()),
    }
}

/// "Go to tab" in the Sessions view: also for a session Cue hasn't heard from (quiet, or Pi that only connected).
#[tauri::command]
pub(crate) async fn focus_live(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || focus::focus(&h.live_origin(&session_id).ok_or("that session is gone")?)).await.map_err(|e| e.to_string())?
}

/// "By the way": a side question about a Claude Code session, answered in a panel in Cue. The session
/// never sees it (Cue asks a copy of its conversation).
#[tauri::command]
pub(crate) async fn btw(hub: State<'_, Arc<Hub>>, session_id: String, question: String) -> Result<String, String> {
    let origin = hub.session_origin(&session_id).ok_or("that session is gone")?;
    tauri::async_runtime::spawn_blocking(move || crate::btw::ask_about(&origin, &question)).await.map_err(|e| e.to_string())?
}

/// "Go to tab" for a session that has no card (a working one).
#[tauri::command]
pub(crate) async fn focus_session_id(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let origin = hub.session_origin(&session_id).ok_or("that session is gone")?;
    tauri::async_runtime::spawn_blocking(move || focus::focus(&origin)).await.map_err(|e| e.to_string())?
}
