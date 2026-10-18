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
/// connected. Codex: its own commands. `list` is null while a list is read for the first time, and
/// `reading` is true while a newer one than `list` is on its way: ask again shortly either way. `fresh`:
/// Refresh was pressed: every kept Claude Code list is read again (shown until the new one's in); a Pi
/// session is asked for its list again.
#[tauri::command]
pub(crate) fn session_commands(hub: State<Arc<Hub>>, session_id: String, fresh: Option<bool>) -> serde_json::Value {
    let fresh = fresh == Some(true);
    if fresh {
        commands::reread_all();
    }
    let done = |list: Option<Vec<commands::Cmd>>| serde_json::json!({ "list": list, "reading": false });
    // A session that reported its commands (Pi): those, or a fresh set once it answers.
    if hub.agent_commands(&session_id).is_some() && fresh && hub.ask_agent_commands(&session_id) {
        return done(None);
    }
    if let Some(list) = hub.agent_commands(&session_id) {
        return done(Some(list));
    }
    let (cwd, machine) = match hub.session_origin(&session_id) {
        Some(o) if o.harness == "claude" => (o.cwd, o.machine),
        Some(o) if o.harness == "codex" => return done(Some(commands::for_codex())),
        Some(o) if o.harness == "pi" => return done(Some(commands::for_pi(&[]))),
        Some(_) => return done(Some(vec![])),
        None => match live::claude().into_iter().find(|q| q.session_id == session_id) {
            Some(q) => (q.cwd, String::new()),
            None => return done(Some(vec![])),
        },
    };
    let list = commands::for_folder(&cwd, &machine);
    serde_json::json!({ "list": list, "reading": commands::reading(&cwd, &machine) })
}

/// The models to pick from in + New: what Claude Code offers on the machine the session starts on
/// (`machine` empty or absent: this Mac), read in your home folder there (the list is Claude Code's, not
/// a folder's). The window keeps it until you press Reload (`fresh`), which asks again. `models` is null
/// while it's read for the first time, and `reading` true while a newer list is on its way: ask again
/// shortly either way.
#[tauri::command]
pub(crate) fn models_list(fresh: bool, machine: Option<String>) -> serde_json::Value {
    let machine = machine.unwrap_or_default();
    let home = commands::home_folder(&machine);
    let models = commands::models_for(&home, &machine, fresh);
    serde_json::json!({ "models": models, "reading": commands::reading(&home, &machine) })
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

/// Where Codex keeps its sessions' logs: ~/.codex/sessions (CODEX_HOME moves ~/.codex).
pub(crate) fn codex_sessions_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::env::var("CODEX_HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".codex")).join("sessions")
}

/// Where Pi keeps its sessions' logs: ~/.pi/agent/sessions (PI_CODING_AGENT_DIR moves ~/.pi/agent).
pub(crate) fn pi_sessions_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::env::var("PI_CODING_AGENT_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(home).join(".pi/agent")).join("sessions")
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
    let root = codex_sessions_dir();
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
    let root = pi_sessions_dir();
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

/// A session's MCP servers: every scope, with what it called through each (logs/mcp.rs).
#[tauri::command]
pub(crate) async fn mcp_for(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp_for_session(&h, &session_id)).await.map_err(|e| e.to_string())?
}

/// Every configured MCP server, for Settings.
#[tauri::command]
pub(crate) async fn mcp_all() -> Vec<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(crate::logs::mcp::all).await.unwrap_or_default()
}

/// Turn a server on or off for the session's folder; the fresh list comes back. The session has to
/// be restarted (mcp_restart) for Claude Code to notice.
#[tauri::command]
pub(crate) async fn mcp_set(hub: State<'_, Arc<Hub>>, session_id: String, name: String, on: bool) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp_set_for(&h, &session_id, &name, on)).await.map_err(|e| e.to_string())?
}

/// A folder's MCP servers before any session runs there (the + New session form): the folder's config
/// alone, no transcript. "~" stands for home.
#[tauri::command]
pub(crate) async fn mcp_for_folder(cwd: String) -> Result<serde_json::Value, String> {
    let cwd = expand_home(&cwd);
    tauri::async_runtime::spawn_blocking(move || crate::logs::mcp::for_session(&cwd, "")).await.map_err(|e| e.to_string())
}

/// Turn a server on or off for a folder (the + New session form): the session started there next loads it.
#[tauri::command]
pub(crate) async fn mcp_set_folder(cwd: String, name: String, on: bool) -> Result<serde_json::Value, String> {
    let cwd = expand_home(&cwd);
    tauri::async_runtime::spawn_blocking(move || { crate::logs::mcp::set_enabled(&cwd, &name, on)?; Ok(crate::logs::mcp::for_session(&cwd, "")) }).await.map_err(|e| e.to_string())?
}

fn expand_home(p: &str) -> String {
    let p = p.trim().trim_end_matches('/');
    match (p.strip_prefix("~"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}{rest}"),
        _ => p.to_string(),
    }
}

/// Restart a session so Claude Code loads its MCP servers again: parked, then resumed at once.
#[tauri::command]
pub(crate) async fn mcp_restart(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp_restart_session(&h, &session_id)).await.map_err(|e| e.to_string())?
}

/// The folder a session's MCP config is read from: on this Mac only.
fn mcp_cwd(h: &Hub, session_id: &str) -> Result<model::Origin, String> {
    let o = h.session_origin(session_id).ok_or("Cue has no record of that session")?;
    if !o.machine.is_empty() {
        return Err("on another machine: change it there".into());
    }
    if o.harness != "claude" {
        return Err("only Claude Code sessions have MCP servers Cue can read".into());
    }
    if o.cwd.is_empty() {
        return Err("Cue doesn't know which folder it runs in".into());
    }
    Ok(o)
}

pub(crate) fn mcp_for_session(h: &Hub, session_id: &str) -> Result<serde_json::Value, String> {
    let o = mcp_cwd(h, session_id)?;
    let path = if o.transcript_path.is_empty() { steps_path(h, session_id).unwrap_or_default() } else { o.transcript_path.clone() };
    // With what the running session really has from each (connected, waiting for sign-in, failed).
    let v = crate::logs::mcp::for_session_id(&o.cwd, &path, session_id);
    Ok(crate::logs::mcp::with_live(v, &path, crate::live::started_ms(session_id)))
}

pub(crate) fn mcp_set_for(h: &Hub, session_id: &str, name: &str, on: bool) -> Result<serde_json::Value, String> {
    let o = mcp_cwd(h, session_id)?;
    crate::logs::mcp::set_enabled(&o.cwd, name, on)?;
    mcp_for_session(h, session_id)
}

/// Park, then resume right away: the same session (`claude --resume`), its conversation kept, with
/// the MCP servers as the config now says. Not mid-turn.
pub(crate) fn mcp_restart_session(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    mcp_cwd(h, session_id)?;
    restart_with(h, session_id, true)
}

/// ⋯ → Restart session: stop its agent and resume the same conversation, so it picks up what it reads
/// only when it starts (new skills, MCP servers, settings). The same steps, and the same cover over its
/// pane, as a restart for its MCP servers. Any agent Cue can stop and resume, on this Mac or a machine.
#[tauri::command]
pub(crate) async fn restart_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || restart_session_now(&h, &session_id)).await.map_err(|e| e.to_string())?
}

/// What restart_session does (it waits on the agent: call it off the main thread).
pub(crate) fn restart_session_now(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    let o = h.session_origin(session_id).ok_or("Cue has no record of that session")?;
    if !["claude", "codex", "pi"].contains(&o.harness.as_str()) || o.cwd.is_empty() {
        return Err("Cue can't stop and resume this one".to_string());
    }
    restart_with(h, session_id, false)
}

/// Stop a session and resume it, keeping any MCP servers it's pinned to (Claude Code on this Mac).
fn restart_with(h: &Arc<Hub>, session_id: &str, mcp: bool) -> Result<String, String> {
    if h.session_record(session_id).is_some_and(|s| s.state == "working") {
        return Err("it's mid-turn; try when it's idle".into());
    }
    let servers = crate::logs::mcp::read_selection(&crate::logs::mcp::selection_path(session_id)).map(|m| m.keys().cloned().collect());
    // What it starts with may have changed (that's often why it's restarted): its "/" commands are read again.
    if let Some(o) = h.session_origin(session_id) {
        crate::commands::reread(&o.cwd, &o.machine);
    }
    h.restart_begin(session_id, servers, mcp);
    let res = super::sessions::park(h, session_id).and_then(|_| {
        h.restart_step(session_id, "start");
        super::sessions::resume_parked_session(h, session_id, "")
    });
    if let Err(e) = &res {
        h.restart_end(session_id, Some(e));
        return res;
    }
    h.restart_step(session_id, "load");
    restart_ends_when_running(h.clone(), session_id.to_string());
    res
}

/// Its pane stays covered until its agent is running again: in Claude Code's list of running sessions
/// on this Mac, checked a few times a second, for 20 s at most. One on a machine isn't in that list: its
/// restart ends as soon as it's started there.
fn restart_ends_when_running(h: Arc<Hub>, session_id: String) {
    let here = h.session_origin(&session_id).is_some_and(|o| o.machine.is_empty() && o.harness == "claude");
    std::thread::spawn(move || {
        let until = crate::model::now_ms() + 20_000;
        while here && crate::model::now_ms() < until {
            live::refresh();
            if live::claude().iter().any(|q| q.session_id == session_id) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        h.restart_end(&session_id, None);
    });
}

/// Run a session with just these MCP servers: its selection written (logs/mcp.rs), then restarted
/// with `--mcp-config <file> --strict-mcp-config`. The choice sticks: every later resume passes it too.
#[tauri::command]
pub(crate) async fn mcp_apply(hub: State<'_, Arc<Hub>>, session_id: String, names: Vec<String>) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp_apply_for(&h, &session_id, names)).await.map_err(|e| e.to_string())?
}

/// Back to the folder's servers: its selection dropped, then restarted without the flags.
#[tauri::command]
pub(crate) async fn mcp_apply_clear(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mcp_apply_clear_for(&h, &session_id)).await.map_err(|e| e.to_string())?
}

pub(crate) fn mcp_apply_for(h: &Arc<Hub>, session_id: &str, names: Vec<String>) -> Result<String, String> {
    let o = mcp_cwd(h, session_id)?;
    if h.session_record(session_id).is_some_and(|s| s.state == "working") {
        return Err("it's mid-turn; try when it's idle".into());
    }
    let path = crate::logs::mcp::selection_path(session_id);
    let before = std::fs::read(&path).ok();
    let n = crate::logs::mcp::write_selection(&path, &crate::logs::mcp::definitions(&o.cwd), &names)?;
    // Not restarted (it couldn't be parked): the selection it had stays.
    if let Err(e) = mcp_restart_session(h, session_id) {
        let _ = match before { Some(b) => std::fs::write(&path, b), None => std::fs::remove_file(&path) };
        return Err(e);
    }
    Ok(match n {
        0 => "Restarted with no MCP servers".into(),
        1 => "Restarted with 1 MCP server".into(),
        n => format!("Restarted with {n} MCP servers"),
    })
}

pub(crate) fn mcp_apply_clear_for(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    mcp_cwd(h, session_id)?;
    if h.session_record(session_id).is_some_and(|s| s.state == "working") {
        return Err("it's mid-turn; try when it's idle".into());
    }
    let path = crate::logs::mcp::selection_path(session_id);
    let before = std::fs::read(&path).ok();
    if before.is_some() {
        std::fs::remove_file(&path).map_err(|e| format!("Couldn't remove {}: {e}", path.display()))?;
    }
    if let Err(e) = mcp_restart_session(h, session_id) {
        if let Some(b) = before {
            let _ = std::fs::write(&path, b);
        }
        return Err(e);
    }
    Ok("Restarted with its folder's MCP servers".into())
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
