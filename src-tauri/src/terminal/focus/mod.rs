//! "Go to session" and "reply": find the agent's terminal and bring it forward, read it, or type into it.
//! tmux by pane (tmux.rs); iTerm by session id (the reliable path), else by tty, Terminal.app by tty, and
//! WezTerm and kitty through their own CLIs (apps.rs); what a screen shows (screen.rs); typing and its
//! keys (typing.rs); the command lines that start an agent (lines.rs); ending one (agents.rs); and how each
//! program is found and cut off when it doesn't answer (exec.rs).

mod agents;
mod apps;
mod lines;
mod exec;
mod screen;
mod tmux;
mod typing;

pub use agents::{agents_installed, end_agent, end_machine_agent};
pub use apps::{open_tab_with, terminal_name, NewTab};
pub use lines::{claude_trusts, fork_line, model_args, new_session_id, resume_line_for, resume_line_for_with, resume_line_local, resume_line_with, shq, start_line_args, start_line_with};
#[cfg(test)]
pub(crate) use exec::find_bin;
pub use screen::{agent_gone, box_empty, dialog_on, text_in_box};
pub use tmux::{open_in_tmux, open_in_tmux_tab, session_screen, tmux_bin, tmux_installed, tmux_keys, tmux_kill_pane, tmux_type_line, TMUX_SESSION};
#[cfg(test)]
pub(crate) use tmux::tmux_screen;
pub(crate) use tmux::{pane_in_its_session, tmux_out};
pub use typing::{press_enter, press_escape, type_into};

use crate::model::Origin;

/// Close a session's terminal tab (after its agent has ended): its tmux pane, its iTerm session (by
/// id, else tty), or a Terminal window whose only tab it is. Terminal can't close one tab of several
/// from a script: that's an error the caller reports.
pub fn close_tab(o: &Origin) -> Result<String, String> {
    if !o.tmux_pane.is_empty() {
        return if tmux::tmux_for(o, &["kill-pane", "-t", &o.tmux_pane]) { Ok(format!("tmux pane {}", o.tmux_pane)) } else { Err(format!("tmux pane {} is gone", o.tmux_pane)) };
    }
    apps::close_app_tab(o).unwrap_or_else(|| Err("couldn't find its tab".into()))
}

/// Bring the session's terminal forward: its tmux pane in whichever window shows it (or one opened on it),
/// its iTerm session or Terminal.app tab, its WezTerm pane or kitty window, else just its app.
pub fn focus(o: &Origin) -> Result<String, String> {
    if !o.machine.is_empty() {
        return Err(format!("It runs on {}: open its terminal in Cue (Runs in Cue, over the reply box)", o.machine));
    }
    if !o.tmux_pane.is_empty() {
        return focus_tmux(&o.tmux_pane);
    }
    if o.term_program == "iTerm.app" {
        return apps::focus_iterm(o);
    }
    if o.term_program == "Apple_Terminal" {
        return apps::by_tty_now(o);
    }
    if !o.wezterm_pane.is_empty() && exec::run("wezterm", &["cli", "activate-pane", "--pane-id", &o.wezterm_pane]) {
        let _ = apps::activate_app("WezTerm");
        return Ok(format!("WezTerm pane {}", o.wezterm_pane));
    }
    if !o.kitty_window_id.is_empty() && apps::kitty(o, &["focus-window", "--match", &format!("id:{}", o.kitty_window_id)]) {
        let _ = apps::activate_app("kitty");
        return Ok(format!("kitty window {}", o.kitty_window_id));
    }
    apps::by_tty(&o.tty).or_else(|_| apps::activate_app(&o.term_program))
}

/// A tmux pane, selected, in the terminal tab whose client shows it.
fn focus_tmux(pane: &str) -> Result<String, String> {
    exec::run("tmux", &["select-window", "-t", pane]);
    exec::run("tmux", &["select-pane", "-t", pane]);
    let ask = |fmt: &str| tmux::tmux_out(&["display-message", "-p", "-t", pane, fmt]).unwrap_or_default();
    let client = ask("#{client_tty}");
    if !client.is_empty() {
        return apps::by_tty(&client).map(|d| format!("tmux pane {pane} via {d}"));
    }
    // No window shows this tmux session (it's detached): open one attached to it. Not for a "Runs in Cue"
    // session: its terminal is the one in Cue's window, and attaching iTerm to that session would mirror
    // every such session into iTerm tabs.
    let name = ask("#{session_name}");
    if name.is_empty() {
        return Err(format!("tmux pane {pane} is gone"));
    }
    if tmux::tmux_is_cues(&name) {
        return Err("it runs in Cue: its terminal is here, the Terminal strip above the reply box".into());
    }
    apps::attach_in_new_window(&name).map(|app| format!("tmux session \"{name}\" (wasn't open in any window; opened it in {app})"))
}

#[cfg(test)]
mod typing_tests;
