//! A hosted session's life (docs/dev/cue-hosted-sessions.md): + New → Cue-hosted, closed and parked, resumed,
//! Move to Cue into one (Settings → Cue-hosted sessions on), and Move to terminal out of one. What it reports,
//! and what's sent to it: hub/hosted.rs.

use super::{with_hub, StartOpts};
use crate::hosted::{Agent, Start};
use crate::hub::{Hub, HostedStart};
use crate::logs::{live, transcript};
use crate::model::{self, Origin};
use crate::terminal::focus;
use crate::terminal::launch::Place;
use std::sync::Arc;
use tauri::State;

/// + New session, hosted: `agent` started by Cue in `dir` with no terminal, its first message (and images)
/// handed to it once it's up. Its id is known when it has started (Cue's pick, or Codex's), so the session is in
/// Active at once.
pub(crate) fn start_hosted(h: &Arc<Hub>, agent: Agent, dir: &str, message: &str, name: &str, opts: &StartOpts, saved: &[crate::uploads::Saved]) -> Result<serde_json::Value, String> {
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let (mut args, model) = model_apart(agent, flag_words(&opts.extra)?);
    // An MCP selection is Claude Code's (Pi has no MCP servers of its own).
    if let Some(names) = opts.mcp.as_ref().filter(|_| agent == Agent::Claude) {
        let path = crate::logs::mcp::selection_path(&sid);
        crate::logs::mcp::write_selection(&path, &crate::logs::mcp::definitions(dir), names)?;
        args.extend(crate::logs::mcp::flags_for(&path));
    }
    // Claude Code's conversation is a file Cue can name now; Pi's is named by Pi when it starts.
    let transcript_path = if agent == Agent::Claude { live::claude_transcript_path(dir, &sid).to_string_lossy().into_owned() } else { String::new() };
    let origin = Origin { session_id: sid.clone(), harness: agent.program().into(), cwd: dir.to_string(), transcript_path, hosted: true, ..Default::default() };
    let origin = h.run_hosted(&origin, start, &HostedStart { permissions: opts.perm.clone(), args, model })?;
    let sid = origin.session_id.clone();
    let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
    h.started(origin, "", &[], false);
    h.name_new(&sid, name);
    if !message.trim().is_empty() || !paths.is_empty() {
        h.send_to_hosted(&sid, message, saved, false)?;
    }
    Ok(serde_json::json!({ "session_id": sid, "detail": format!("Started {}, hosted in Cue", agent.name()) }))
}

/// Codex takes its model in what Cue sends it, not as an argument: `--model x` / `-m x` taken out of + New's flags
/// for it (the rest, its own settings as `-c key=value`, go on its command line). Other agents keep theirs.
fn model_apart(agent: Agent, words: Vec<String>) -> (Vec<String>, Option<String>) {
    if agent != Agent::Codex {
        return (words, None);
    }
    let (mut rest, mut model) = (vec![], None);
    let mut it = words.into_iter();
    while let Some(w) = it.next() {
        match w.as_str() {
            "--model" | "-m" => model = it.next(),
            _ => match w.strip_prefix("--model=") {
                Some(m) => model = Some(m.to_string()),
                None => rest.push(w),
            },
        }
    }
    (rest, model)
}

/// + New's extra flags, as the arguments they are. Taken word by word: a hosted session's Claude gets them
/// as arguments, with no shell to read quotes, so a quoted flag is refused rather than passed in pieces.
/// SHORTCUT: no quoting at all, fine for the flags + New is used with (--model, --effort); read them with a
/// shell-words parser if a quoted value (a system prompt) is ever wanted for a hosted session.
fn flag_words(extra: &str) -> Result<Vec<String>, String> {
    if extra.contains(['"', '\'', '\\']) {
        return Err("a hosted session takes flags without quotes for now: start it in a terminal for that".into());
    }
    Ok(extra.split_whitespace().map(str::to_string).collect())
}

/// Close, for a hosted session: its Claude closed (its conversation kept), then its end, as a terminal
/// session's SessionEnd hook reports it.
pub(crate) fn close_hosted(h: &Hub, origin: Origin) -> Result<String, String> {
    h.hosted().close(&origin.session_id);
    h.event(origin, "ended", String::new(), vec![], "");
    Ok("Closed it".into())
}

/// Resume a parked or closed hosted session: hosted again, on the model it last used and its MCP
/// selection, its record back; `message` (Resume & send) goes to it once it's up.
pub(crate) fn resume_hosted(h: &Arc<Hub>, p: crate::sessions::Parked, message: &str) -> Result<String, String> {
    let o = Origin { agent_pid: None, ..p.session.origin.clone() };
    let sid = o.session_id.clone();
    let woken = h.resume_hosted_as(&o)?;
    h.unparked(p, woken, message.trim());
    if !message.trim().is_empty() {
        h.hosted().send(&sid, message.trim())?;
    }
    Ok("Resumed, hosted in Cue".into())
}

/// Move to Cue with Settings → Cue-hosted sessions on: the terminal session ended (its tab closed with it),
/// then resumed as a hosted one, its record (name, star, recent thread) carried over. `end`: ending it as
/// Close does. Its history needs nothing done: both write the same transcript.
pub(crate) fn move_into_hosted(h: &Arc<Hub>, origin: Origin, end: impl FnOnce() -> Result<String, String>) -> Result<String, String> {
    let sid = origin.session_id.clone();
    let name = h.session_name(&sid);
    let star = h.session_star(&sid);
    let path = if origin.transcript_path.is_empty() { crate::logs::archive::lookup(&sid).map(|(p, _, _)| p).unwrap_or_default() } else { origin.transcript_path.clone() };
    end()?;
    // Its SessionEnd drops Cue's record of it as it exits: that lands before the hosted one is made.
    for _ in 0..20 {
        if h.session_origin(&sid).is_none() {
            break;
        }
        crate::clock::sleep_ms(100);
    }
    let fresh = Origin { session_id: sid.clone(), harness: "claude".into(), cwd: origin.cwd.clone(), transcript_path: path.clone(), hosted: true, ..Default::default() };
    let moved = h.resume_hosted_as(&fresh)?;
    let recent = if path.is_empty() { Vec::new() } else { transcript::recent_context(&path, 6) };
    let label = if name.is_empty() { model::folder_of(&origin.cwd) } else { name };
    h.resumed(moved, &label, &recent, false);
    if star > 0 {
        h.restore_star(&sid, star);
    }
    Ok("Moved to Cue: hosted here now".into())
}

/// "Move to terminal": a hosted session that isn't working or asking you something, closed and resumed with
/// `claude --resume` in a terminal tab, where it's an ordinary terminal session from then on.
#[tauri::command]
pub(crate) async fn move_to_terminal(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| move_hosted_to_terminal(h, &session_id)).await?
}

pub(crate) fn move_hosted_to_terminal(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    let origin = h.hosted_origin(session_id).ok_or("it isn't hosted in Cue")?;
    h.closable(session_id)?;
    let line = focus::resume_line_local(&origin.cwd, session_id, &origin.transcript_path)?;
    let name = h.session_name(session_id);
    let label = if name.is_empty() { model::folder_of(&origin.cwd) } else { name };
    h.hosted().close(session_id);
    let tab = h.launcher().here(&line, &origin.cwd, &label, Place::chosen(Some(false)))?;
    let moved = Origin { session_id: session_id.to_string(), harness: "claude".into(), cwd: origin.cwd.clone(), transcript_path: origin.transcript_path.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..Default::default() };
    h.resumed(moved, &label, &[], false);
    Ok(format!("Moved to {}", tab.what))
}

#[cfg(test)]
mod tests {
    use super::flag_words;

    #[test]
    fn flags_are_taken_word_by_word_and_quoted_ones_are_refused() {
        assert_eq!(flag_words(" --model opus  --verbose ").unwrap(), vec!["--model", "opus", "--verbose"]);
        assert!(flag_words("--append-system-prompt 'be brief'").is_err());
    }
}

/// What quitting Cue would end now (hub/quitting.rs): asked before ⌘Q goes through and before an update restarts Cue.
#[tauri::command]
pub(crate) async fn quit_check(hub: State<'_, Arc<Hub>>) -> Result<Vec<crate::hub::EndsWithCue>, String> {
    with_hub(&hub, |h| h.ends_with_cue()).await
}

/// Quit, having been told what it ends (the window's quit dialog: Quit anyway).
#[tauri::command]
pub(crate) async fn quit_now(app: tauri::AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

/// cue-host as Settings → Agents shows it: reachable, its build, the sessions it runs, older hosts still finishing.
#[tauri::command]
pub(crate) async fn host_status(hub: State<'_, Arc<Hub>>) -> Result<serde_json::Value, String> {
    with_hub(&hub, |h| h.hosted().host_status()).await
}

/// Restart cue-host (Settings → Agents): every hosted agent ended (each session comes back paused and resumes with
/// your next message), and every host, for one that misbehaves.
#[tauri::command]
pub(crate) async fn host_restart(hub: State<'_, Arc<Hub>>) -> Result<String, String> {
    with_hub(&hub, |h| h.hosted().restart_host()).await?
}
