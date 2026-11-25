//! A session's life: start, resume, park, close, rename, move to Cue.

use super::{blocking, with_hub};
use crate::db;
use crate::model;
use crate::machines;
use crate::logs::transcript;
use crate::logs::live;
use crate::logs::archive;
use crate::terminal::focus;
use crate::terminal::shell;
use crate::hub::Hub;
// Kept at this path for the add-on.
#[allow(unused_imports)]
pub(crate) use crate::hub::{closed_record, end_session, resume_parked_session, resume_record, start_session, start_session_with, StartOpts, ALREADY_RUNNING, CLOSED_DAYS, CLOSED_MAX};
use std::sync::Arc;
use tauri::State;
use crate::show_main;

/// Resume an older Claude Code session, hosted by Cue in its folder. Cue knows it at once (name and last few
/// messages from its transcript), so it opens in Active.
#[tauri::command]
pub(crate) async fn resume_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String) -> Result<serde_json::Value, String> {
    let res = with_hub(&hub, move |h| resume_older(&h, &session_id)).await?;
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// The work behind Resume (the window's search). Blocking: it waits for its agent to start.
pub(crate) fn resume_older(h: &Arc<Hub>, session_id: &str) -> Result<serde_json::Value, String> {
    let (path, cwd, title) = archive::lookup(session_id).ok_or("can't tell which folder that session ran in")?;
    if h.session_origin(session_id).is_some() || live::claude().iter().any(|q| q.session_id == session_id) {
        return Err("it's running already".into());
    }
    let origin = model::Origin { session_id: session_id.to_string(), harness: "claude".into(), cwd, transcript_path: path.clone(), hosted: true, ..Default::default() };
    let woken = h.resume_hosted_as(&origin)?;
    h.resumed(woken, &title, &transcript::recent_context(&path, 6), false);
    Ok(serde_json::json!({ "session_id": session_id, "detail": "Resumed, hosted in Cue" }))
}

/// Park: stop a session (its agent ended as Close ends it, its tab closed) and keep it under Parked,
/// to resume with its whole conversation. Claude Code, Codex and Pi; not while it works or asks you
/// something.
#[tauri::command]
pub(crate) async fn park_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| park(&h, &session_id)).await?
}

pub(crate) fn park(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    let s = h.session_record(session_id).ok_or("Cue has no record of that session")?;
    if s.origin.cwd.is_empty() {
        return Err("Cue doesn't know which folder it runs in, so it couldn't resume it".into());
    }
    // Only one Cue can bring back: checked before anything is ended.
    focus::resume_line_for(&s.origin.harness, &s.origin.cwd, session_id)?;
    h.closable(session_id)?;
    // Its finished turn leaves Waiting before its end, which would file it as "session ended", and comes back
    // as it was if its agent can't be stopped after all. (Only a finished turn can be here: closable refuses
    // one that asks you something.)
    let cards = h.take_cards(session_id);
    if let Err(e) = end_session(h, session_id) {
        h.put_back(cards);
        return Err(e);
    }
    // Nothing late from it lands on the parked one.
    wait_for_end(h, session_id);
    h.file_taken(cards, "gone", "parked");
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
        crate::clock::sleep_ms(100);
    }
}

/// Resume a parked session: its agent's resume command, in tmux on this Mac or on its machine; its
/// record back as it was (name, star, recent thread), at its prompt. With a message (Resume & send),
/// the message goes on that command line, so the agent starts on it once the conversation is loaded.
#[tauri::command]
pub(crate) async fn resume_parked(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, message: Option<String>) -> Result<String, String> {
    let message = message.unwrap_or_default();
    let res = with_hub(&hub, move |h| resume_parked_session(&h, &session_id, &message)).await?;
    // Cue stays in front: a terminal tab may have opened behind it.
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// Closed, in the Parked drawer: sessions that ended in the last 30 days (at most 30), newest first,
/// not parked and not running again. To find the ones that got lost (an agent that exited, a tab closed).
#[tauri::command]
pub(crate) async fn closed_sessions(hub: State<'_, Arc<Hub>>) -> Result<Vec<serde_json::Value>, String> {
    with_hub(&hub, |h| h.closed(CLOSED_DAYS * 24 * 3600 * 1000, CLOSED_MAX)).await
}

/// Resume a closed session, as a parked one resumes: its agent's resume command, its record back.
#[tauri::command]
pub(crate) async fn resume_closed(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, message: Option<String>) -> Result<String, String> {
    let message = message.unwrap_or_default();
    let res = with_hub(&hub, move |h| {
        let p = closed_record(&h, &session_id)?;
        resume_record(&h, p, &message)
    }).await?;
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// Park a closed session: kept under Parked (nothing to stop, it has ended already).
#[tauri::command]
pub(crate) async fn park_closed(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    with_hub(&hub, move |h| {
        let p = closed_record(h, &session_id)?;
        h.add_parked(p.session);
        Ok(())
    })
    .await?
}

/// Unpark: take it off Parked without resuming it. Its conversation stays in History and search.
#[tauri::command]
pub(crate) async fn unpark_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    with_hub(&hub, move |h| {
        h.take_parked(&session_id);
    })
    .await
}

/// Rename a session (Cue, and the agent where it can be told). Typing into a terminal blocks: off the UI thread.
#[tauri::command]
pub(crate) async fn rename_session(hub: State<'_, Arc<Hub>>, session_id: String, name: String) -> Result<String, String> {
    with_hub(&hub, move |h| h.rename(&session_id, &name)).await?
}

/// Open a session Cue hasn't heard from yet in Cue: its chat and message box, not only its tab.
#[tauri::command]
pub(crate) async fn adopt_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<(), String> {
    with_hub(&hub, move |h| h.adopt(&session_id)).await?
}

/// Close a session from the Sessions view: by ending its agent (SIGTERM: the conversation is saved, `--resume`
/// brings it back; on a machine, one SIGTERM to everything in its tmux pane there) and closing its tab.
/// Never while it works or asks you something.
#[tauri::command]
pub(crate) async fn close_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| end_session(&h, &session_id)).await?
}

/// Delete, in the chat header's ⋯ (asked once more first): a session gone for good. A running one is
/// closed first as Close does (never while it works or asks you), then
/// Cue forgets everything it kept of it, and its conversation file goes, so it can't be resumed.
#[tauri::command]
pub(crate) async fn delete_session(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| delete(&h, &session_id)).await?
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
    let m = crate::machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more", o.machine))?;
    let beside = beside.to_string_lossy().into_owned();
    let line = ["rm", "-rf", "--", path.as_str(), beside.as_str()].map(focus::shq).join(" ");
    // Its own words either way (Cue's SSH hides a script's stderr), and its exit code last.
    let out = crate::machines::run(&m.host, &format!("{line} 2>&1; printf '\\n__CUE_EXIT__:%d\\n' \"$?\""))?;
    match out.rsplit_once("\n__CUE_EXIT__:") {
        Some((_, code)) if code.trim() == "0" => Ok(()),
        Some((said, _)) => Err(said.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("it failed").trim().to_string()),
        None => Err("it didn't run there".into()),
    }
}

/// "+ New session": open a terminal tab in `cwd`, in the background, and start `agent` there with an
/// optional first message and name. Cue picks the session id, so it knows the session at once (it
/// opens in Active, ready to type into). Returns the id.
#[tauri::command]
pub(crate) async fn new_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, agent: String, cwd: String, message: String, name: String, tmux: Option<bool>, perm: Option<String>, extra: Option<String>, machine: Option<String>, mcp: Option<Vec<String>>, images: Option<Vec<model::Upload>>, hosted: Option<bool>, trust: Option<bool>) -> Result<serde_json::Value, String> {
    let opts = StartOpts { tmux, perm: perm.unwrap_or_default(), extra: extra.unwrap_or_default(), machine: machine.unwrap_or_default(), mcp, images: images.unwrap_or_default(), hosted: hosted.unwrap_or(false), trust: trust.unwrap_or(false), ..Default::default() };
    let res = with_hub(&hub, move |h| start_session_with(&h, &agent, &cwd, &message, &name, &opts))
        .await?;
    // Cue stays in front: the terminal tab opened behind it.
    if res.is_ok() {
        show_main(&app, None);
    }
    res
}

/// btw's Fork: a Claude Code session copied into a new one that keeps going, in the same folder (on the
/// same machine), with the model it last used, named "<name>-fork" (then -fork-2, -fork-3…). `message`,
/// when there is one, is its first. The session itself goes on untouched. Returns the fork's id.
#[tauri::command]
pub(crate) async fn fork_session(app: tauri::AppHandle, hub: State<'_, Arc<Hub>>, session_id: String, message: String) -> Result<serde_json::Value, String> {
    let res = with_hub(&hub, move |h| fork(&h, &session_id, &message)).await?;
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
    // Hosted, here or on its machine (cue-host there), on the model it runs on now (its transcript is read here
    // only on this Mac). Trusted: the session it forks from already works in that folder.
    let model = (from.machine.is_empty() && !from.transcript_path.is_empty()).then(|| crate::logs::steps::model_of(&from.transcript_path)).flatten();
    let opts = StartOpts { extra: model.map(|m| format!("--model {m}")).unwrap_or_default(), machine: from.machine.clone(), trust: true, fork_of: session_id.to_string(), ..Default::default() };
    let agent = crate::hosted::Agent::Claude;
    let started = crate::hub::start_hosted(h, agent, &from.cwd, "", &name, &opts, &[])?;
    let sid = started.get("session_id").and_then(|v| v.as_str()).ok_or("the fork started without a session id")?.to_string();
    let origin = h.session_origin(&sid).ok_or("the fork ended as it started")?;
    // Active shows where it forked from, then what you asked it (if anything).
    let recent = if from.transcript_path.is_empty() || !from.machine.is_empty() { vec![] } else { transcript::recent_context(&from.transcript_path, 6) };
    h.resumed(origin, &name, &recent, false);
    if !message.trim().is_empty() {
        h.send_to_hosted(&sid, message, &[], false)?;
    }
    crate::config::carry_move(session_id, &sid);
    let what = if from.machine.is_empty() { "Cue".to_string() } else { crate::machines::display_name(&from.machine) };
    Ok(serde_json::json!({ "session_id": sid, "name": name, "detail": format!("Forked as {name}, hosted in {what}") }))
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
    blocking(move || {
        if machine.is_empty() {
            shell::run(&dir, &line)
        } else {
            let m = machines::get(&machine).ok_or(format!("{machine} isn't one of your machines any more (+ New → Machine)"))?;
            shell::run_on_machine(&m, &dir, &line)
        }
    }).await?
}

/// "Move to Cue": a Claude Code session in a terminal tab (or Cue's own terminal), ended (its tab closed with it)
/// and resumed hosted by Cue, with no terminal. Idle sessions only: the conversation comes back whole, but anything
/// mid-turn would be cut off. Shell state in the old tab is gone.
#[tauri::command]
pub(crate) async fn move_to_cue(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| move_session_to_cue(&h, &session_id)).await?
}

pub(crate) fn move_session_to_cue(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    let origin = movable_origin(h, session_id)?;
    let old = origin.clone();
    // Ended as Close does (SIGTERM, its tab closed); one that won't take the signal is asked to /exit.
    super::move_into_hosted(h, origin, || end_session(h, session_id).or_else(|e| ask_to_exit(h, &old, session_id, e).map(|_| "Ended it".into())))
}

/// The session, when it can be moved to Cue: a live Claude Code session not hosted already, in a folder Cue knows.
fn movable_origin(h: &Arc<Hub>, session_id: &str) -> Result<model::Origin, String> {
    let origin = h.live_origin(session_id).ok_or("that session is gone")?;
    if origin.harness != "claude" {
        return Err("only a Claude Code session can be resumed in Cue".into());
    }
    if origin.hosted {
        return Err("it's already hosted in Cue".into());
    }
    if origin.cwd.is_empty() {
        return Err("Cue doesn't know which folder it runs in".into());
    }
    Ok(origin)
}

/// A session that didn't take the signal within 3 s (`ended`, why ending it failed) is asked to leave the way
/// you would, with /exit typed into it, and given a few seconds more; then its tab is closed.
fn ask_to_exit(h: &Arc<Hub>, origin: &model::Origin, session_id: &str, ended: String) -> Result<(), String> {
    if !ended.contains("didn't quit") {
        return Err(ended);
    }
    let pid = origin.agent_pid.or_else(|| live::claude().into_iter().find(|q| q.session_id == session_id).map(|q| q.pid)).unwrap_or(0);
    h.type_command(origin, "/exit")?;
    let alive = || pid > 0 && unsafe { libc::kill(pid, 0) == 0 };
    for _ in 0..60 {
        if !alive() {
            break;
        }
        crate::clock::sleep_ms(100);
    }
    if alive() {
        return Err("it didn't quit, even to /exit: close it in its tab, then Resume it here".into());
    }
    let _ = focus::close_tab(origin);
    Ok(())
}
