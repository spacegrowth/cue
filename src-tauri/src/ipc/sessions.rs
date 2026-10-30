//! A session's life: start, resume, park, close, rename, move to Cue.

use crate::config;
use crate::db;
use crate::leads;
use crate::model;
use crate::machines;
use crate::logs::transcript;
use crate::logs::live;
use crate::logs::archive;
use crate::terminal::focus;
use crate::terminal::shell;
use crate::hub::Hub;
use std::sync::Arc;
use tauri::State;
use crate::show_main;

/// Resume an older Claude Code session: `claude --resume` in a terminal tab opened behind Cue, in its
/// folder. Cue knows it at once (name and last few messages from its transcript), so it opens in Active.
#[tauri::command]
pub(crate) async fn resume_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    let res = tauri::async_runtime::spawn_blocking(move || resume_older(&h, &session_id)).await.map_err(|e| e.to_string())?;
    // Cue stays in front: the terminal tab opened behind it.
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// The work behind Resume (the window's search). Blocking: it opens a terminal tab.
pub(crate) fn resume_older(h: &Arc<Hub>, session_id: &str) -> Result<serde_json::Value, String> {
    let (path, cwd, title) = archive::lookup(session_id).ok_or("can't tell which folder that session ran in")?;
    let line = focus::resume_line_local(&cwd, session_id)?;
    let name = model::folder_of(&cwd);
    let tab = open_for_resume(&line, &cwd, &name)?;
    let origin = model::Origin { session_id: session_id.to_string(), harness: "claude".into(), cwd, transcript_path: path.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..Default::default() };
    h.resumed(origin, &title, &transcript::recent_context(&path, 6), false);
    Ok(serde_json::json!({ "session_id": session_id, "detail": format!("Resumed in {}", tab.what) }))
}

/// Where a resumed session runs on this Mac: its own tmux session (Settings), else a window of Cue's
/// tmux tabs, else a plain terminal tab.
pub(crate) fn open_for_resume(line: &str, cwd: &str, name: &str) -> Result<focus::NewTab, String> {
    if config::tmux_sessions() {
        focus::open_in_tmux(line, cwd, name)
    } else if focus::tmux_installed() {
        focus::open_in_tmux_tab(line, cwd, name)
    } else {
        focus::open_tab_with(line)
    }
}

/// Park: stop a session (its agent ended as Close ends it, its tab closed) and keep it under Parked,
/// to resume with its whole conversation. Claude Code, Codex and Pi; not while it works or asks you
/// something, and not one relay runs.
#[tauri::command]
pub(crate) async fn park_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || park(&h, &session_id)).await.map_err(|e| e.to_string())?
}

pub(crate) fn park(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    let s = h.session_record(session_id).ok_or("Cue has no record of that session")?;
    if s.origin.cwd.is_empty() {
        return Err("Cue doesn't know which folder it runs in, so it couldn't resume it".into());
    }
    // Only one Cue can bring back: checked before anything is ended.
    focus::resume_line_for(&s.origin.harness, &s.origin.cwd, session_id)?;
    // An executor is relay's (or pilead's) to close; a lead with executors still open can't go yet.
    // A lead with none left is parked like any session: `claude --resume` brings the lead back.
    match leads::close_plan(session_id) {
        Some(Err(e)) => return Err(e),
        Some(Ok(_)) => return Err("relay runs it: close it through relay instead".into()),
        None => {}
    }
    h.closable(session_id)?;
    // Its finished turn leaves Waiting as parked: before its end, which would call it "session ended".
    // (Only a finished turn can be here: closable refuses one that asks you something.)
    for id in h.pending().iter().filter(|i| i.origin.session_id == session_id).map(|i| i.id.clone()) {
        h.finish(&id, "gone", "parked");
    }
    end_session(h, session_id)?;
    // Nothing late from it lands on the parked one.
    wait_for_end(h, session_id);
    h.add_parked(s);
    Ok("Parked: its agent stopped and its terminal closed".into())
}

/// After ending a session: its end (SessionEnd, or its agent's exit) drops Cue's record of it. Up to
/// 2 s for that to land.
fn wait_for_end(h: &Hub, session_id: &str) {
    for _ in 0..20 {
        if h.session_origin(session_id).is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Resume a parked session: its agent's resume command, in tmux on this Mac or on its machine; its
/// record back as it was (name, star, recent thread), at its prompt. With a message (Resume & send),
/// the message goes on that command line, so the agent starts on it once the conversation is loaded.
#[tauri::command]
pub(crate) async fn resume_parked(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, message: Option<String>) -> Result<String, String> {
    let h = hub.inner().clone();
    let message = message.unwrap_or_default();
    let res = tauri::async_runtime::spawn_blocking(move || resume_parked_session(&h, &session_id, &message)).await.map_err(|e| e.to_string())?;
    // Cue stays in front: a terminal tab may have opened behind it.
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

pub(crate) fn resume_parked_session(h: &Arc<Hub>, session_id: &str, message: &str) -> Result<String, String> {
    let p = h.parked(session_id).ok_or("it isn't parked any more")?;
    // Running again already (you resumed it in a terminal yourself): a second agent would share its
    // conversation. It's live, so it just leaves Parked. The registry read fresh: the list Cue keeps is
    // a tick old, so a session parked a moment ago (Restart, after an MCP change) would still be on it.
    live::refresh();
    if p.session.origin.machine.is_empty() && live::claude().iter().any(|q| q.session_id == session_id) {
        h.take_parked(session_id);
        return Ok("It's already running".into());
    }
    resume_record(h, p, message)
}

/// Closed, in the Parked drawer: sessions that ended in the last 30 days (at most 30), newest first,
/// not parked and not running again. To find the ones that got lost (an agent that exited, a tab closed).
#[tauri::command]
pub(crate) fn closed_sessions(hub: State<Arc<Hub>>) -> Vec<serde_json::Value> {
    hub.closed(CLOSED_DAYS * 24 * 3600 * 1000, CLOSED_MAX)
}

pub(crate) const CLOSED_DAYS: u64 = 30;
pub(crate) const CLOSED_MAX: usize = 30;

/// Resume a closed session, as a parked one resumes: its agent's resume command, its record back.
#[tauri::command]
pub(crate) async fn resume_closed(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, message: Option<String>) -> Result<String, String> {
    let h = hub.inner().clone();
    let message = message.unwrap_or_default();
    let res = tauri::async_runtime::spawn_blocking(move || {
        let p = closed_record(&h, &session_id)?;
        resume_record(&h, p, &message)
    }).await.map_err(|e| e.to_string())?;
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// Park a closed session: kept under Parked (nothing to stop, it has ended already).
#[tauri::command]
pub(crate) fn park_closed(hub: State<Arc<Hub>>, session_id: String) -> Result<(), String> {
    let p = closed_record(hub.inner(), &session_id)?;
    hub.add_parked(p.session);
    Ok(())
}

/// A closed session's saved record, if it can be brought back: ended, not live again, an agent Cue resumes.
pub(crate) fn closed_record(h: &Arc<Hub>, session_id: &str) -> Result<crate::sessions::Parked, String> {
    if h.session_origin(session_id).is_some() || live::claude().iter().any(|q| q.session_id == session_id) {
        return Err("it's running again".into());
    }
    let (ended_ms, session) = db::ended_session(session_id).ok_or("Cue has no record of that session")?;
    focus::resume_line_for(&session.origin.harness, &session.origin.cwd, session_id)?;
    Ok(crate::sessions::Parked { parked_ms: ended_ms, session })
}

/// The work behind Resume, for a parked or a closed session: in tmux on this Mac or on its machine.
pub(crate) fn resume_record(h: &Arc<Hub>, p: crate::sessions::Parked, message: &str) -> Result<String, String> {
    let session_id = p.session.origin.session_id.clone();
    let session_id = session_id.as_str();
    let o = p.session.origin.clone();
    // On this Mac, with its MCP selection when it has one (logs/mcp.rs), so the choice sticks.
    let args = if o.machine.is_empty() { crate::logs::mcp::selection_flags(session_id) } else { vec![] };
    let mut line = focus::resume_line_for_with(&o.harness, &o.cwd, session_id, &args)?;
    if !message.trim().is_empty() {
        line += &format!(" {}", focus::shq(message.trim()));
    }
    let label = if p.session.name.is_empty() { model::folder_of(&o.cwd) } else { p.session.name.clone() };
    let base = model::Origin { session_id: session_id.to_string(), harness: o.harness.clone(), transcript_path: o.transcript_path.clone(), ..Default::default() };
    let (fresh, at) = if o.machine.is_empty() {
        let tab = open_for_resume(&line, &o.cwd, &label)?;
        let at = tab.what.clone();
        (model::Origin { cwd: o.cwd.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..base }, at)
    } else {
        let m = machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", o.machine))?;
        let (pane, tty, dir) = machines::open_in_tmux(&m, &line, &o.cwd, &label)?;
        (model::Origin { cwd: dir, term_program: "tmux".into(), tty, tmux_pane: pane, machine: m.name.clone(), ..base }, format!("tmux on {}", m.name))
    };
    h.unparked(p, fresh, message.trim());
    Ok(format!("Resumed in {at}"))
}

/// Unpark: take it off Parked without resuming it. Its conversation stays in History and search.
#[tauri::command]
pub(crate) fn unpark_session(hub: State<Arc<Hub>>, session_id: String) {
    hub.take_parked(&session_id);
}

/// Rename a session (Cue, and the agent where it can be told). Typing into a terminal blocks: off the UI thread.
#[tauri::command]
pub(crate) async fn rename_session(hub: State<'_, Arc<Hub>>, session_id: String, name: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.rename(&session_id, &name)).await.map_err(|e| e.to_string())?
}

/// Open a session Cue hasn't heard from yet in Cue: its chat and message box, not only its tab.
#[tauri::command]
pub(crate) async fn adopt_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || h.adopt(&session_id)).await.map_err(|e| e.to_string())?
}

/// Close a session from the Sessions view: an executor through relay / pilead (they mark it closed and
/// close its tab), a relay lead with its executors; anything else by ending its agent (SIGTERM: the conversation is saved, `--resume`
/// brings it back; on a machine, one SIGTERM to everything in its tmux pane there) and closing its tab.
/// Never while it works or asks you something.
#[tauri::command]
pub(crate) async fn close_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || end_session(&h, &session_id)).await.map_err(|e| e.to_string())?
}

/// The work behind Close. Blocking: it ends a process and closes a tab.
pub(crate) fn end_session(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    let session_id = session_id.to_string();
    {
        h.closable(&session_id)?;
        // Its machine can't be reached: nothing to end there now; it's taken out of Cue.
        if h.unreachable(&session_id) {
            h.forget(&session_id);
            return Ok("Removed from Cue (its machine can't be reached)".into());
        }
        // A relay lead: relay closes each executor it still has (it stops it and closes its tab), then
        // steps the lead down (else it still thinks it leads); then the lead is ended as any session. If any of
        // that fails, its error is the answer and the lead stays.
        if let Some(plan) = leads::lead_close(&session_id) {
            let plan = plan?;
            let failed: Vec<String> = plan.executors.iter().filter_map(|(sid, name, cmd)| cmd.run().and_then(|_| end_if_still_here(h, sid)).err().map(|e| format!("{name}: {e}"))).collect();
            if !failed.is_empty() {
                return Err(format!("relay couldn't close {} ({}); the lead stays", if failed.len() == 1 { "an executor" } else { "some executors" }, failed.join("; ")));
            }
            plan.stop.run().map_err(|e| format!("relay couldn't step this lead down: {e}"))?;
            let n = plan.executors.len();
            let said = end_and_close(h, &session_id)?;
            return Ok(match n { 0 => said, 1 => "Closed it and its executor".into(), _ => format!("Closed it and its {n} executors") });
        }
        // An executor: relay / pi-lead close it, where it runs (on its machine, over SSH).
        if let Some(plan) = leads::close_plan(&session_id) {
            plan?.run()?;
            return end_if_still_here(h, &session_id).map(|_| "Closed".into());
        }
        end_and_close(h, &session_id)
    }
}

/// Delete, in the chat header's ⋯ (asked once more first): a session gone for good. A running one is
/// closed first as Close does (a relay lead with its executors; never while it works or asks you), then
/// Cue forgets everything it kept of it, and its conversation file goes, so it can't be resumed.
#[tauri::command]
pub(crate) async fn delete_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || delete(&h, &session_id)).await.map_err(|e| e.to_string())?
}

pub(crate) fn delete(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    let origin = h
        .session_record(session_id)
        .map(|s| s.origin)
        .or_else(|| h.live_origin(session_id))
        .or_else(|| h.parked(session_id).map(|p| p.session.origin))
        .or_else(|| db::ended_session(session_id).map(|(_, s)| s.origin))
        .ok_or("Cue has no record of that session")?;
    if h.live_origin(session_id).is_some() {
        end_session(h, session_id)?;
        // Nothing late from it comes back after it's deleted.
        wait_for_end(h, session_id);
    }
    h.purge(session_id);
    match delete_transcript(&origin) {
        Ok(()) => Ok("Deleted".into()),
        Err(e) => Ok(format!("Deleted from Cue, but its conversation file stays ({e})")),
    }
}

/// A session's conversation file (and Claude Code's folder of the same name beside it, its subagents'
/// work), on this Mac or on its machine. Only a file named for the session: never anything else.
fn delete_transcript(o: &crate::model::Origin) -> Result<(), String> {
    let sid = o.session_id.as_str();
    let mut path = o.transcript_path.clone();
    if path.is_empty() && o.machine.is_empty() && o.harness == "claude" {
        let projects = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".claude/projects");
        path = std::fs::read_dir(&projects).ok().into_iter().flatten().flatten().map(|d| d.path().join(format!("{sid}.jsonl"))).find(|p| p.exists()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        // None: Claude Code writes it with the first message, and this one never had one.
        if path.is_empty() {
            return Ok(());
        }
    }
    let file = std::path::Path::new(&path);
    if sid.len() < 8 || !file.file_name().is_some_and(|n| n.to_string_lossy().contains(sid)) {
        return Err("Cue doesn't know where it is".into());
    }
    let beside = file.with_file_name(sid);
    if o.machine.is_empty() {
        if beside.is_dir() {
            std::fs::remove_dir_all(&beside).map_err(|e| e.to_string())?;
        }
        return match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let rm = leads::Cmd { bin: "rm".into(), args: vec!["-rf".into(), "--".into(), path.clone(), beside.to_string_lossy().into_owned()], machine: o.machine.clone() };
    rm.run().map(|_| ())
}

/// After relay / pi-lead closed an executor: relay ends the agent it knows of, but one resumed since runs
/// in a pane relay's record doesn't name, so one still running here is ended like any other session.
pub(crate) fn end_if_still_here(h: &Hub, session_id: &str) -> Result<(), String> {
    let still = h.live_origin(session_id).filter(|o| o.machine.is_empty()).and_then(|_| live::claude().into_iter().find(|q| q.session_id == session_id && unsafe { libc::kill(q.pid, 0) } == 0));
    if still.is_some() {
        end_and_close(h, session_id)?;
    }
    Ok(())
}

/// End a session's agent and close its tab.
fn end_and_close(h: &Hub, session_id: &str) -> Result<String, String> {
    let origin = h.live_origin(session_id).ok_or("that session is gone")?;
    // On this Mac, the agent's process. On a machine, there is no pid here (it runs there): its
    // pane is ended on the machine instead.
    if origin.machine.is_empty() {
        let pid = origin.agent_pid.or_else(|| live::claude().into_iter().find(|q| q.session_id == session_id).map(|q| q.pid)).unwrap_or(0);
        focus::end_agent(pid)?;
    } else {
        focus::end_machine_agent(&origin)?;
    }
    Ok(match focus::close_tab(&origin) {
        Ok(t) => format!("Closed its {t}"),
        // Many tabs close by themselves when the agent they run exits (relay launches its tabs so).
        Err(e) if e.contains("is gone") || e.contains("couldn't find") => "Ended it; its tab closed with it".into(),
        Err(e) => format!("Ended it ({e})"),
    })
}

/// "+ New session": open a terminal tab in `cwd`, in the background, and start `agent` there with an
/// optional first message and name. Cue picks the session id, so it knows the session at once (it
/// opens in Active, ready to type into). Returns the id.
#[tauri::command]
pub(crate) async fn new_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, agent: String, cwd: String, message: String, name: String, tmux: Option<bool>, perm: Option<String>, extra: Option<String>, machine: Option<String>, mcp: Option<Vec<String>>, images: Option<Vec<model::Upload>>) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    let opts = StartOpts { tmux, perm: perm.unwrap_or_default(), extra: extra.unwrap_or_default(), machine: machine.unwrap_or_default(), mcp, images: images.unwrap_or_default() };
    let res = tauri::async_runtime::spawn_blocking(move || start_session_with(&h, &agent, &cwd, &message, &name, &opts))
        .await
        .map_err(|e| e.to_string())?;
    // Cue stays in front: the terminal tab opened behind it.
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// The work behind "+ New session": open a terminal tab in `cwd`
/// and start `agent` there. Blocking: it types into a terminal.
pub(crate) fn start_session(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str) -> Result<serde_json::Value, String> {
    start_session_with(h, agent, cwd, message, name, &StartOpts::default())
}

/// What + New session can choose beyond the folder: where it runs (None: Settings' default), how much it
/// may do without asking, and any extra flags.
#[derive(Default)]
pub(crate) struct StartOpts {
    pub tmux: Option<bool>,
    pub perm: String,
    pub extra: String,
    /// A machine from + New → Machine to run it on (empty: this Mac). Always in tmux there.
    pub machine: String,
    /// The MCP servers it runs with (a Claude Code session on this Mac): written as its selection
    /// (logs/mcp.rs) under the id Cue picks, and passed as `--mcp-config <file> --strict-mcp-config`.
    /// None: whatever its folder has on.
    pub mcp: Option<Vec<String>>,
    /// Images with the first message (pasted or dropped into its box): saved, their paths typed after it.
    pub images: Vec<model::Upload>,
}

pub(crate) fn start_session_with(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str, opts: &StartOpts) -> Result<serde_json::Value, String> {
    let saved = crate::uploads::save(&opts.images)?;
    if !opts.machine.is_empty() {
        return start_on_machine(h, agent, cwd, message, name, opts, &saved);
    }
    let dir = cwd.trim();
    let dir = if let Some(rest) = dir.strip_prefix("~") { format!("{}{rest}", std::env::var("HOME").unwrap_or_default()) } else { dir.to_string() };
    if !std::path::Path::new(&dir).is_dir() {
        return Err(format!("{dir} isn't a folder"));
    }
    let sid = if agent == "codex" { String::new() } else { focus::new_session_id() };
    let asks_trust = agent == "claude" && !focus::claude_trusts(&dir);
    let mut args = vec![];
    if let (Some(names), "claude", false) = (&opts.mcp, agent, sid.is_empty()) {
        let path = crate::logs::mcp::selection_path(&sid);
        crate::logs::mcp::write_selection(&path, &crate::logs::mcp::definitions(&dir), names)?;
        args = crate::logs::mcp::flags_for(&path);
    }
    let line = focus::start_line_args(agent, &dir, &sid, &crate::uploads::with_paths(message, &saved), name, &opts.perm, &opts.extra, &args)?;
    let in_tmux = opts.tmux.map_or_else(config::tmux_sessions, |t| t && focus::tmux_installed());
    let label = Some(name.trim()).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| model::folder_of(&dir));
    // In tmux either way when it's installed: "Runs in iTerm" only adds a terminal tab showing it.
    let tab = if in_tmux {
        focus::open_in_tmux(&line, &dir, &label)?
    } else if focus::tmux_installed() {
        focus::open_in_tmux_tab(&line, &dir, &label)?
    } else {
        focus::open_tab_with(&line)?
    };
    if !sid.is_empty() {
        let origin = model::Origin { session_id: sid.clone(), harness: agent.to_string(), cwd: dir, term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..Default::default() };
        h.started(origin, message, &paths_of(&saved), asks_trust);
        h.name_new(&sid, name);
    }
    Ok(serde_json::json!({ "session_id": sid, "detail": format!("Started {} in {}", agent, tab.what) }))
}

/// + New session on a machine: in tmux there (Cue's terminal shows it, over SSH), in `cwd` there.
pub(crate) fn start_on_machine(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str, opts: &StartOpts, saved: &[crate::uploads::Saved]) -> Result<serde_json::Value, String> {
    let m = machines::get(&opts.machine).ok_or(format!("{} isn't one of your machines (+ New → Machine)", opts.machine))?;
    let sid = if agent == "codex" { String::new() } else { focus::new_session_id() };
    // Its images copied there first, so the paths in its first message open on that machine.
    let there = crate::uploads::on_machine(&m.name, saved, &|_, _| {})?;
    // It starts where tmux opens the window (`cwd`): the line itself stays in that folder.
    let line = focus::start_line_with(agent, ".", &sid, &crate::uploads::with_paths(message, &there), name, &opts.perm, &opts.extra)?;
    let label = Some(name.trim()).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| model::folder_of(cwd.trim()));
    let (pane, tty, dir) = machines::open_in_tmux(&m, &line, cwd, &label)?;
    if !sid.is_empty() {
        let origin = model::Origin { session_id: sid.clone(), harness: agent.to_string(), cwd: dir, term_program: "tmux".into(), tty, tmux_pane: pane, machine: m.name.clone(), ..Default::default() };
        h.started(origin, message, &paths_of(saved), false);
        h.name_new(&sid, name);
    }
    Ok(serde_json::json!({ "session_id": sid, "detail": format!("Started {} on {}", agent, m.name) }))
}

/// The saved images' paths on this Mac, for the chat's copy of the first message.
fn paths_of(saved: &[crate::uploads::Saved]) -> Vec<String> {
    saved.iter().map(|s| s.path.clone()).collect()
}

/// btw's Fork: a Claude Code session copied into a new one that keeps going, in the same folder (on the
/// same machine), with the model it last used, named "<name>-fork" (then -fork-2, -fork-3…). `message`,
/// when there is one, is its first. The session itself goes on untouched. Returns the fork's id.
#[tauri::command]
pub(crate) async fn fork_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, message: String) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    let res = tauri::async_runtime::spawn_blocking(move || fork(&h, &session_id, &message)).await.map_err(|e| e.to_string())?;
    // Cue stays in front: the terminal tab opened behind it.
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

pub(crate) fn fork(h: &Arc<Hub>, session_id: &str, message: &str) -> Result<serde_json::Value, String> {
    let from = h.session_origin(session_id).ok_or("that session is gone")?;
    if from.harness != "claude" {
        return Err("Fork works with Claude Code sessions".into());
    }
    let own = h.session_name(session_id);
    let base = if own.trim().is_empty() { model::folder_of(&from.cwd) } else { own };
    let name = fork_name(&base, &h.session_names());
    let sid = focus::new_session_id();
    let (origin, what) = if from.machine.is_empty() {
        // Its model and its MCP selection, as the session runs now; the fork keeps a copy of the selection.
        let mut args = vec![];
        if let Some(m) = (!from.transcript_path.is_empty()).then(|| crate::logs::steps::model_of(&from.transcript_path)).flatten() {
            args.extend(["--model".to_string(), m]);
        }
        let (was, now) = (crate::logs::mcp::selection_path(session_id), crate::logs::mcp::selection_path(&sid));
        if was.exists() && std::fs::copy(&was, &now).is_ok() {
            args.extend(crate::logs::mcp::flags_for(&now));
        }
        let line = focus::fork_line(&from.cwd, session_id, &sid, &name, &args, message)?;
        let tab = open_for_resume(&line, &from.cwd, &name)?;
        let o = model::Origin { session_id: sid.clone(), harness: "claude".into(), cwd: from.cwd.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..Default::default() };
        (o, tab.what)
    } else {
        // On a machine: there, in tmux, as + New starts one (its transcript is there, so no model from it).
        let m = machines::get(&from.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", from.machine))?;
        let line = focus::fork_line(".", session_id, &sid, &name, &[], message)?;
        let (pane, tty, dir) = machines::open_in_tmux(&m, &line, &from.cwd, &name)?;
        let o = model::Origin { session_id: sid.clone(), harness: "claude".into(), cwd: dir, term_program: "tmux".into(), tty, tmux_pane: pane, machine: m.name.clone(), ..Default::default() };
        (o, m.name)
    };
    // Active shows where it forked from, then what you asked it (if anything).
    let recent = if from.transcript_path.is_empty() || !from.machine.is_empty() { vec![] } else { transcript::recent_context(&from.transcript_path, 6) };
    h.resumed(origin.clone(), &name, &recent, false);
    if !message.trim().is_empty() {
        h.started(origin, message, &[], false);
    }
    h.name_new(&sid, &name);
    Ok(serde_json::json!({ "session_id": sid, "name": name, "detail": format!("Forked as {name} in {what}") }))
}

/// A fork's name: "<name>-fork", then "-fork-2", "-fork-3"…, the first no session has. A fork of a fork
/// counts on from the same name ("foo-fork" forks as "foo-fork-2"), so one session's forks stay one series.
fn fork_name(name: &str, taken: &[String]) -> String {
    let name = name.trim();
    let root = match name.rsplit_once("-fork") {
        Some((r, n)) if !r.is_empty() && (n.is_empty() || n.strip_prefix('-').is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))) => r,
        _ => name,
    };
    let taken = |n: &str| taken.iter().any(|t| t.trim().eq_ignore_ascii_case(n));
    let first = format!("{root}-fork");
    if !taken(&first) {
        return first;
    }
    (2..).map(|i| format!("{root}-fork-{i}")).find(|n| !taken(n)).unwrap()
}

#[cfg(test)]
mod fork_tests {
    use super::fork_name;

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn forks_count_up_from_one_name() {
        assert_eq!(fork_name("api", &names(&["api"])), "api-fork");
        assert_eq!(fork_name("api", &names(&["api", "api-fork"])), "api-fork-2");
        assert_eq!(fork_name("api", &names(&["api", "api-fork", "api-fork-2"])), "api-fork-3");
        // A gone fork's name can be had again.
        assert_eq!(fork_name("api", &names(&["api", "api-fork-2"])), "api-fork");
    }

    #[test]
    fn a_fork_of_a_fork_stays_in_the_series() {
        assert_eq!(fork_name("api-fork", &names(&["api", "api-fork"])), "api-fork-2");
        assert_eq!(fork_name("api-fork-2", &names(&["api", "api-fork", "api-fork-2"])), "api-fork-3");
    }

    #[test]
    fn only_a_real_fork_suffix_is_taken_off() {
        assert_eq!(fork_name("my-forklift", &[]), "my-forklift-fork");
        assert_eq!(fork_name("-fork", &[]), "-fork-fork");
        assert_eq!(fork_name("x-fork-a", &[]), "x-fork-a-fork");
    }
}

/// Trust in Cue for a session Claude Code is asking "do you trust this folder?": its cursor moved onto
/// "Yes, I trust this folder", then Enter. Only from your click, and only where Cue can see the screen
/// (its tmux pane, here or on a machine): Claude Code lists "No, exit" first, so a blind Enter would end it.
#[tauri::command]
pub(crate) async fn trust_folder(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    let origin = hub.session_origin(&session_id).ok_or("that session is gone")?;
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if origin.tmux_pane.is_empty() {
            return Err("Cue can't see its terminal".into());
        }
        let screen = || focus::session_screen(&origin).ok().filter(|s| crate::terminal::dialog::setup_on(s) == Some(crate::terminal::dialog::Setup::Trust));
        let Some((cursor, yes)) = screen().as_deref().and_then(crate::terminal::dialog::trust_choices) else {
            h.trust_answered(&session_id);
            return Err("it isn't asking about the folder any more".into());
        };
        if cursor != yes {
            let key = if yes > cursor { "Down" } else { "Up" };
            focus::tmux_keys(&origin, &vec![key.to_string(); cursor.abs_diff(yes)])?;
            std::thread::sleep(std::time::Duration::from_millis(250));
            if !screen().as_deref().and_then(crate::terminal::dialog::trust_choices).is_some_and(|(c, y)| c == y) {
                return Err("its cursor didn't reach “Yes”".into());
            }
        }
        focus::tmux_keys(&origin, &["Enter".to_string()])?;
        h.trust_answered(&session_id);
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

/// Continue in Cue for a session whose terminal waits for Enter before Claude Code starts ("Security
/// notes … Press Enter to continue…"): Enter, only while that screen is still there. Only from your click.
#[tauri::command]
pub(crate) async fn continue_start(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    let origin = hub.session_origin(&session_id).ok_or("that session is gone")?;
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if origin.tmux_pane.is_empty() {
            return Err("Cue can't see its terminal".into());
        }
        let on = focus::session_screen(&origin).ok().and_then(|s| crate::terminal::dialog::setup_on(&s)) == Some(crate::terminal::dialog::Setup::Enter);
        h.enter_pressed(&session_id);
        if !on {
            return Err("it isn't waiting for Enter any more".into());
        }
        focus::tmux_keys(&origin, &["Enter".to_string()])
    }).await.map_err(|e| e.to_string())?
}

/// "!" in a box: run the rest of the line in the session's folder (or `cwd`, for the + New session box),
/// in your shell — on this Mac, or on the machine the session runs on (the folder is there). What it
/// printed, its exit code and how long it took: shown as a card, never sent to the agent unless you
/// choose to.
#[tauri::command]
pub(crate) async fn run_line(hub: State<'_, Arc<Hub>>, session_id: Option<String>, cwd: Option<String>, machine: Option<String>, line: String) -> Result<shell::Ran, String> {
    let (dir, machine) = match session_id.filter(|s| !s.is_empty()) {
        Some(sid) => {
            let o = hub.live_origin(&sid).ok_or("Cue doesn't know that session's folder")?;
            let dir = o.cwd.trim().to_string();
            if dir.is_empty() {
                return Err("Cue doesn't know that session's folder".into());
            }
            (dir, o.machine)
        }
        None => {
            let c = cwd.unwrap_or_default().trim().to_string();
            if c.is_empty() {
                return Err("pick a folder first".into());
            }
            let m = machine.unwrap_or_default();
            // On this Mac a `~` becomes your home here; on a machine it stays, for the shell there.
            let c = if m.is_empty() {
                match c.strip_prefix('~') { Some(rest) => format!("{}{rest}", std::env::var("HOME").unwrap_or_default()), None => c }
            } else {
                c
            };
            (c, m)
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        if machine.is_empty() {
            shell::run(&dir, &line)
        } else {
            let m = machines::get(&machine).ok_or(format!("{machine} isn't one of your machines any more (+ New → Machine)"))?;
            shell::run_on_machine(&m, &dir, &line)
        }
    }).await.map_err(|e| e.to_string())?
}

/// "Move to Cue": a Claude Code session in a terminal tab, ended (its tab closed with it) and resumed in
/// Cue's tmux session, where its terminal shows in Cue. Idle sessions only: the conversation comes back
/// whole (claude --resume), but anything mid-turn would be cut off. Shell state in the old tab is gone.
#[tauri::command]
pub(crate) async fn move_to_cue(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || move_session_to_cue(&h, &session_id)).await.map_err(|e| e.to_string())?
}

pub(crate) fn move_session_to_cue(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    if !focus::tmux_installed() {
        return Err("tmux isn't installed".into());
    }
    let origin = h.live_origin(session_id).ok_or("that session is gone")?;
    if origin.harness != "claude" {
        return Err("only a Claude Code session can be resumed in Cue".into());
    }
    if !origin.tmux_pane.is_empty() {
        return Err("it already runs in Cue".into());
    }
    if leads::close_plan(session_id).is_some() {
        return Err("a relay lead: close it through relay, then resume it in Cue".into());
    }
    let cwd = origin.cwd.clone();
    if cwd.is_empty() {
        return Err("Cue doesn't know which folder it runs in".into());
    }
    let name = h.session_name(session_id);
    let star = h.session_star(session_id);   // its record goes with the old process: the star comes back on the new one
    let path = if origin.transcript_path.is_empty() { archive::lookup(session_id).map(|(p, _, _)| p).unwrap_or_default() } else { origin.transcript_path.clone() };
    let label = if name.is_empty() { model::folder_of(&cwd) } else { name };
    let line = focus::resume_line_local(&cwd, session_id)?;
    // Its window in Cue first, empty: only once that's there does the old one end, so a tmux that won't
    // cooperate never leaves the session ended and nowhere.
    let mut tab = focus::open_in_tmux("", &cwd, &label)?;
    // Ended as Close does (SIGTERM, its tab closed). One that doesn't take the signal within 3 s is asked
    // to leave the way you would, with /exit typed into it, and given a few seconds more.
    if let Err(e) = end_session(h, session_id) {
        if !e.contains("didn't quit") {
            focus::tmux_kill_pane(&tab.tmux_pane);
            return Err(e);
        }
        let pid = origin.agent_pid.or_else(|| live::claude().into_iter().find(|q| q.session_id == session_id).map(|q| q.pid)).unwrap_or(0);
        if let Err(e) = focus::type_into(&origin, "/exit") {
            focus::tmux_kill_pane(&tab.tmux_pane);
            return Err(e);
        }
        let alive = || pid > 0 && unsafe { libc::kill(pid, 0) == 0 };
        for _ in 0..60 {
            if !alive() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if alive() {
            focus::tmux_kill_pane(&tab.tmux_pane);
            return Err("it didn't quit, even to /exit: close it in its tab, then Resume it here".into());
        }
        let _ = focus::close_tab(&origin);
    }
    // Its SessionEnd hook drops Cue's record of it as it exits: let that land before the new one is made.
    for _ in 0..20 {
        if h.session_origin(session_id).is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    // It has ended: from here it always comes back somewhere. In Cue's window if it can be typed into,
    // else in a terminal tab, as Resume would.
    let mut fell_back = None;
    if let Err(e) = focus::tmux_type_line(&tab.tmux_pane, &line) {
        focus::tmux_kill_pane(&tab.tmux_pane);
        tab = focus::open_tab_with(&line)?;
        fell_back = Some(e);
    }
    let recent = if path.is_empty() { Vec::new() } else { transcript::recent_context(&path, 6) };
    let moved = model::Origin { session_id: session_id.to_string(), harness: "claude".into(), cwd, transcript_path: path, term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane.clone(), ..Default::default() };
    // In the window at once, as working (loading), so its screen can be watched while it comes up.
    h.resumed(moved.clone(), &label, &recent, true);
    if star > 0 {
        h.restore_star(session_id, star);
    }
    if let Some(e) = fell_back {
        return Ok(format!("Couldn't open it in Cue ({e}), so it was resumed in {} instead", tab.what));
    }
    // Until Claude Code is up in it with the conversation loaded: its prompt (or a question of its own)
    // on the screen. Up to 60 s (a long conversation takes a while); then it's left to finish on its own.
    for _ in 0..300 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if focus::tmux_screen(&tab.tmux_pane).map(|sc| sc.contains('❯')).unwrap_or(false) {
            break;
        }
    }
    h.mark_state(&moved, "waiting");
    Ok("Moved to Cue: resumed in its terminal here".into())
}
