//! ⋯ → Move to terminal: a session Cue runs (hosted, or in Cue's own terminal) resumed where you run agents yourself,
//! for when you're moving away from Cue. Not something to do often, so it's in ⋯, not a button.
//! - On this Mac: its agent's own resume line in a plain tab of your terminal app (iTerm, Terminal), not one kept running
//!   by Cue's tmux, where it's an ordinary terminal session from then on, Cue or no Cue.
//! - Hosted on another machine: resumed in tmux there, on its own (that machine's terminal), as before.
//! Each comes back on the model it last used, with its MCP selection (Claude Code); its permission mode isn't kept, so
//! it may ask more after the move, never less (hub/hosted.rs `resume_hosted_as` says the same).
//! Never while it works or asks you something (`closable`).

use super::with_hub;
use crate::hub::Hub;
use crate::model::{self, Origin};
use crate::terminal::focus;
use crate::terminal::launch::Place;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub(crate) async fn move_to_terminal(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    with_hub(&hub, move |h| move_to_terminal_now(h, &session_id)).await?
}

/// What `move_to_terminal` does (it waits on the agent: off the main thread).
pub(crate) fn move_to_terminal_now(h: &Arc<Hub>, session_id: &str) -> Result<String, String> {
    if let Some(origin) = h.hosted_origin(session_id) {
        h.closable(session_id)?;
        return if origin.machine.is_empty() { hosted_here(h, &origin) } else { hosted_there(h, &origin) };
    }
    let origin = h.live_origin(session_id).ok_or("that session is gone")?;
    if origin.tmux_pane.is_empty() {
        return Err("it already runs in a terminal of yours".into());
    }
    if !origin.machine.is_empty() {
        return Err(format!("it already runs in tmux on {}, its own terminal", crate::machines::display_name(&origin.machine)));
    }
    h.closable(session_id)?;
    in_cues_terminal(h, &origin)
}

/// Its agent's resume line on this Mac: Claude Code on the model it last used and with its MCP selection; Pi and Codex
/// by their own (they pick up the model themselves).
fn resume_line_here(o: &Origin) -> Result<String, String> {
    if o.harness == "claude" {
        focus::resume_line_local(&o.cwd, &o.session_id, &o.transcript_path)
    } else {
        focus::resume_line_for(&o.harness, &o.cwd, &o.session_id)
    }
}

/// Its name as Cue shows it, or its folder's.
fn label(h: &Hub, o: &Origin) -> String {
    let name = h.session_name(&o.session_id);
    if name.is_empty() { model::folder_of(&o.cwd) } else { name }
}

/// `o` running from now on in `tab`, a terminal app's: an ordinary terminal session, under its name and star.
fn now_in(h: &Hub, o: &Origin, label: &str, star: u64, tab: focus::NewTab) -> String {
    let moved = Origin { session_id: o.session_id.clone(), harness: o.harness.clone(), cwd: o.cwd.clone(), transcript_path: o.transcript_path.clone(), term_program: tab.term_program, iterm_session_id: tab.iterm_session_id, tty: tab.tty, tmux_pane: tab.tmux_pane, ..Default::default() };
    h.resumed(moved, label, &[], false);
    if star > 0 {
        h.restore_star(&o.session_id, star);
    }
    format!("Moved to {}", tab.what)
}

/// A hosted session here: closed, then resumed in a tab of your terminal app.
fn hosted_here(h: &Arc<Hub>, o: &Origin) -> Result<String, String> {
    let line = resume_line_here(o)?;
    let (label, star) = (label(h, o), h.session_star(&o.session_id));
    h.hosted().close(&o.session_id);
    let tab = h.launcher().here(&line, &o.cwd, &label, Place::Tab)?;
    Ok(now_in(h, o, &label, star, tab))
}

/// A hosted session on another machine: closed there and resumed in tmux on the same machine (that machine's own
/// terminal), on the model it last used.
fn hosted_there(h: &Arc<Hub>, o: &Origin) -> Result<String, String> {
    let sid = o.session_id.as_str();
    let m = crate::machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", o.machine))?;
    let args = if o.harness == "claude" { focus::model_args(&o.transcript_path, &o.machine) } else { vec![] };
    let line = focus::resume_line_for_with(&o.harness, ".", sid, &args)?;
    let label = label(h, o);
    h.hosted().close(sid);
    let crate::terminal::launch::Started { pane, tty, dir } = h.launcher().there(&m, &line, &o.cwd, &label)?;
    let moved = Origin { session_id: sid.to_string(), harness: o.harness.clone(), cwd: dir, transcript_path: o.transcript_path.clone(), term_program: "tmux".into(), tty, tmux_pane: pane, machine: m.name.clone(), ..Default::default() };
    h.resumed(moved, &label, &[], false);
    Ok(format!("Moved to tmux on {}", crate::machines::display_name(&m.name)))
}

/// A session in Cue's own terminal: its agent ended and its window there closed (as Close does), then resumed in a tab
/// of your terminal app. The resume line is made first: one Cue couldn't resume is never ended.
fn in_cues_terminal(h: &Arc<Hub>, o: &Origin) -> Result<String, String> {
    let line = resume_line_here(o)?;
    let (label, star) = (label(h, o), h.session_star(&o.session_id));
    super::end_session(h, &o.session_id)?;
    // Its end drops Cue's record of it: let that land before the new one is made.
    for _ in 0..20 {
        if h.session_origin(&o.session_id).is_none() {
            break;
        }
        crate::clock::sleep_ms(100);
    }
    let tab = h.launcher().here(&line, &o.cwd, &label, Place::Tab)?;
    Ok(now_in(h, o, &label, star, tab))
}
