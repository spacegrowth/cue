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
    let line = focus::resume_line(&cwd, session_id)?;
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
    // Its end (SessionEnd, or its agent's exit) drops Cue's record of it: let that land first, so
    // nothing late from it lands on the parked one.
    for _ in 0..20 {
        if h.session_origin(session_id).is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    h.add_parked(s);
    Ok("Parked: its agent stopped and its terminal closed".into())
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
    // conversation. It's live, so it just leaves Parked.
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
    let mut line = focus::resume_line_for(&o.harness, &o.cwd, session_id)?;
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
/// close its tab); anything else by ending its agent (SIGTERM: the conversation is saved, `--resume`
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
        // An executor: relay / pi-lead close it, where it runs (on its machine, over SSH).
        // relay ends the agent it knows of; one resumed since runs in a pane relay's record doesn't
        // name, so a session still running here afterwards is ended below like any other.
        if let Some(plan) = leads::close_plan(&session_id) {
            plan?.run()?;
            let still = h.live_origin(&session_id).filter(|o| o.machine.is_empty()).and_then(|_| live::claude().into_iter().find(|q| q.session_id == session_id && unsafe { libc::kill(q.pid, 0) } == 0));
            if still.is_none() {
                return Ok("Closed".into());
            }
        }
        // A relay lead with no executors left: relay steps it down first (else it still thinks it
        // leads); if that fails, its error is the answer and the pane stays.
        if let Some(plan) = leads::lead_stop(&session_id) {
            plan?.run().map_err(|e| format!("relay couldn't step this lead down: {e}"))?;
        }
        let origin = h.live_origin(&session_id).ok_or("that session is gone")?;
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
}

/// "+ New session": open a terminal tab in `cwd`, in the background, and start `agent` there with an
/// optional first message and name. Cue picks the session id, so it knows the session at once (it
/// opens in Active, ready to type into). Returns the id.
#[tauri::command]
pub(crate) async fn new_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, agent: String, cwd: String, message: String, name: String, tmux: Option<bool>, perm: Option<String>, extra: Option<String>, machine: Option<String>) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    let opts = StartOpts { tmux, perm: perm.unwrap_or_default(), extra: extra.unwrap_or_default(), machine: machine.unwrap_or_default() };
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
}

pub(crate) fn start_session_with(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str, opts: &StartOpts) -> Result<serde_json::Value, String> {
    if !opts.machine.is_empty() {
        return start_on_machine(h, agent, cwd, message, name, opts);
    }
    let dir = cwd.trim();
    let dir = if let Some(rest) = dir.strip_prefix("~") { format!("{}{rest}", std::env::var("HOME").unwrap_or_default()) } else { dir.to_string() };
    if !std::path::Path::new(&dir).is_dir() {
        return Err(format!("{dir} isn't a folder"));
    }
    let sid = if agent == "codex" { String::new() } else { focus::new_session_id() };
    let asks_trust = agent == "claude" && !focus::claude_trusts(&dir);
    let line = focus::start_line_with(agent, &dir, &sid, message, name, &opts.perm, &opts.extra)?;
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
        h.started(origin, message, asks_trust);
        h.name_new(&sid, name);
    }
    Ok(serde_json::json!({ "session_id": sid, "detail": format!("Started {} in {}", agent, tab.what) }))
}

/// + New session on a machine: in tmux there (Cue's terminal shows it, over SSH), in `cwd` there.
pub(crate) fn start_on_machine(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str, opts: &StartOpts) -> Result<serde_json::Value, String> {
    let m = machines::get(&opts.machine).ok_or(format!("{} isn't one of your machines (+ New → Machine)", opts.machine))?;
    let sid = if agent == "codex" { String::new() } else { focus::new_session_id() };
    // It starts where tmux opens the window (`cwd`): the line itself stays in that folder.
    let line = focus::start_line_with(agent, ".", &sid, message, name, &opts.perm, &opts.extra)?;
    let label = Some(name.trim()).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| model::folder_of(cwd.trim()));
    let (pane, tty, dir) = machines::open_in_tmux(&m, &line, cwd, &label)?;
    if !sid.is_empty() {
        let origin = model::Origin { session_id: sid.clone(), harness: agent.to_string(), cwd: dir, term_program: "tmux".into(), tty, tmux_pane: pane, machine: m.name.clone(), ..Default::default() };
        h.started(origin, message, false);
        h.name_new(&sid, name);
    }
    Ok(serde_json::json!({ "session_id": sid, "detail": format!("Started {} on {}", agent, m.name) }))
}

/// Trust in Cue for a session Claude Code is asking "do you trust this folder?": presses Enter in its
/// terminal, which picks the question's first choice, "Yes, I trust this folder". Only from your click.
#[tauri::command]
pub(crate) async fn trust_folder(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    let origin = hub.session_origin(&session_id).ok_or("that session is gone")?;
    tauri::async_runtime::spawn_blocking(move || {
        // When Cue can see the screen, Enter goes only to the trust question: a button clicked after
        // the session moved on to a permission prompt would otherwise accept that.
        if let Some(d) = focus::dialog_on(&origin) {
            if !d.title.to_lowercase().contains("trust") {
                return Err("it isn't asking about the folder any more".into());
            }
        }
        focus::press_enter(&origin)
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
    let line = focus::resume_line(&cwd, session_id)?;
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
