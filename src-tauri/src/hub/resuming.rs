//! Resuming and ending a session: a parked or closed one brought back (in a terminal tab here, in tmux on its
//! machine, or hosted by Cue), and Close. The window's commands are in ipc/sessions.rs and ipc/hosted.rs.

use super::Hub;
use crate::db;
use crate::logs::live;
use crate::machines;
use crate::model::{self, Origin};
use crate::terminal::focus;
use crate::terminal::launch::{Place, Started};
use std::sync::Arc;

/// What Resume says when the session was running again already: it only left Parked, and `message` wasn't sent.
pub(crate) const ALREADY_RUNNING: &str = "It's already running";

pub(crate) fn resume_parked_session(h: &Arc<Hub>, session_id: &str, message: &str) -> Result<String, String> {
    let p = h.parked(session_id).ok_or("it isn't parked any more")?;
    // Running again already (you resumed it in a terminal yourself): a second agent would share its
    // conversation. It's live, so it just leaves Parked. The registry read fresh: the list Cue keeps is
    // a tick old, so a session parked a moment ago (Restart, after an MCP change) would still be on it.
    live::refresh();
    if p.session.origin.machine.is_empty() && live::claude().iter().any(|q| q.session_id == session_id) {
        h.take_parked(session_id);
        return Ok(ALREADY_RUNNING.into());
    }
    resume_record(h, p, message)
}

pub(crate) const CLOSED_DAYS: u64 = 30;

pub(crate) const CLOSED_MAX: usize = 30;

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
    if o.hosted {
        return resume_hosted(h, p, message);
    }
    // On this Mac, with its MCP selection when it has one (logs/mcp.rs), so the choice sticks.
    // And on its model, here or on its machine (focus::model_args).
    let mut args = if o.harness == "claude" { focus::model_args(&o.transcript_path, &o.machine) } else { vec![] };
    if o.machine.is_empty() {
        args.extend(crate::logs::mcp::selection_flags(session_id));
    }
    let mut line = focus::resume_line_for_with(&o.harness, &o.cwd, session_id, &args)?;
    if !message.trim().is_empty() {
        line += &format!(" {}", focus::shq(message.trim()));
    }
    let label = if p.session.name.is_empty() { model::folder_of(&o.cwd) } else { p.session.name.clone() };
    let base = model::Origin { session_id: session_id.to_string(), harness: o.harness.clone(), transcript_path: o.transcript_path.clone(), ..Default::default() };
    let (fresh, at) = if o.machine.is_empty() {
        let tab = h.launcher().here(&line, &o.cwd, &label, Place::chosen(None))?;
        let at = tab.what.clone();
        (model::Origin { cwd: o.cwd.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..base }, at)
    } else {
        let m = machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", o.machine))?;
        let Started { pane, tty, dir } = h.launcher().there(&m, &line, &o.cwd, &label)?;
        (model::Origin { cwd: dir, term_program: "tmux".into(), tty, tmux_pane: pane, machine: m.name.clone(), ..base }, format!("tmux on {}", m.name))
    };
    h.unparked(p, fresh, message.trim());
    Ok(format!("Resumed in {at}"))
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
        end_and_close(h, &session_id)
    }
}

/// End a session's agent and close its tab.
fn end_and_close(h: &Hub, session_id: &str) -> Result<String, String> {
    let origin = h.live_origin(session_id).ok_or("that session is gone")?;
    if origin.hosted {
        return close_hosted(h, origin);
    }
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
        // Many tabs close by themselves when the agent they run exits.
        Err(e) if e.contains("is gone") || e.contains("couldn't find") => "Ended it; its tab closed with it".into(),
        Err(e) => format!("Ended it ({e})"),
    })
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
