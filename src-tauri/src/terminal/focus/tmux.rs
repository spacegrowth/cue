//! Sessions in tmux: the tmux sessions Cue starts them in, a pane's screen and what runs in its foreground,
//! keys pressed in it, and tmux on a session's machine (over SSH) as on this Mac.

use super::exec::{bin, capped, run, tmux_cap, CAP};
use super::screen::{at_shell_prompt, is_shell};
use crate::model::Origin;
use std::process::Command;

/// The tmux session Cue starts sessions in (Settings → New sessions in tmux): one tmux window each, so
/// one terminal window shows them all (in iTerm, attached with `tmux -CC`, as ordinary tabs). They keep
/// running when the terminal, or Cue, quits.
pub const TMUX_SESSION: &str = "cue";
/// The tmux session for sessions you chose to see in a terminal tab ("Runs in iTerm"): tmux underneath
/// all the same (they outlive the terminal and Cue, and Cue's terminal can show them), with iTerm attached
/// to this session alone (`tmux -CC`, each window an ordinary tab), so "Runs in Cue" sessions never show
/// up there.
pub(super) const TMUX_TABS_SESSION: &str = "cue-tabs";

pub fn tmux_bin() -> String {
    bin("tmux")
}

pub fn tmux_installed() -> bool {
    std::path::Path::new(&bin("tmux")).is_absolute()
}

/// tmux's answer, trimmed; None if it failed or didn't answer in time.
pub(crate) fn tmux_out(args: &[&str]) -> Option<String> {
    let out = capped(Command::new(bin("tmux")).args(args), tmux_cap()).ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether a pane lives in a tmux session Cue opened for a chat session named `label` (Cue's own terminal, or
/// the tabs session "Runs in iTerm" used), not one of your own tmux sessions, and is still the session's: on
/// its terminal `tty` (after a reboot a pane id can name another pane). A pane that's gone says no.
pub(crate) fn pane_is_cues(pane: &str, label: &str, tty: &str) -> bool {
    let Some(said) = tmux_out(&["display-message", "-p", "-t", pane, "#{session_name}|#{pane_tty}"]) else { return false };
    let (s, pane_tty) = said.rsplit_once('|').unwrap_or((said.as_str(), ""));
    if s.is_empty() || tty.is_empty() || pane_tty != tty {
        return false;
    }
    // Named after the session as Cue's own terminal named it ("bug-fixes", "bug-fixes-2"): the tag below is
    // missing on sessions opened before it was set right.
    let base = tmux_name(label);
    let named = s == base || s.strip_prefix(&format!("{base}-")).is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    s == TMUX_TABS_SESSION || named || tmux_is_cues(&s)
}

/// A tmux session name from a session's name, as Cue's own terminal named them: no '.', ':' or whitespace, never empty.
fn tmux_name(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c == '.' || c == ':' || c.is_whitespace() { '-' } else { c }).collect();
    if s.is_empty() { "cue".into() } else { s }
}

/// What a tmux pane shows right now, as plain text (wrapped lines joined), blank lines at the end dropped.
pub fn tmux_screen(pane: &str) -> Result<String, String> {
    let out = capped(Command::new(bin("tmux")).args(["capture-pane", "-p", "-J", "-t", pane]), tmux_cap()).map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("tmux pane {pane} is gone"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// The machine a session runs on, as Cue knows it now.
pub(super) fn machine_of(o: &Origin) -> Result<crate::machines::Machine, String> {
    crate::machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", o.machine))
}

/// tmux for a session: this Mac's, or the one on the machine it runs on (over SSH).
pub(super) fn tmux_for(o: &Origin, args: &[&str]) -> bool {
    if o.machine.is_empty() {
        return run("tmux", args);
    }
    crate::machines::get(&o.machine).is_some_and(|m| crate::machines::tmux(&m.host, args).is_ok())
}

/// `tmux_for` saying why it failed: on a machine, a trip that never reached it isn't a pane that's gone.
pub(super) fn tmux_try_for(o: &Origin, pane: &str, args: &[&str]) -> Result<String, String> {
    if o.machine.is_empty() {
        return tmux_out(args).ok_or(format!("tmux pane {pane} is gone"));
    }
    let m = machine_of(o)?;
    let unreached = crate::machines::cant_connect(&m.host);
    crate::machines::tmux(&m.host, args).map(|s| s.trim_end().to_string()).map_err(|e| if e == unreached { e } else { format!("tmux pane {pane} is gone") })
}

/// What a session's tmux pane shows right now (this Mac's or a machine's).
pub fn session_screen(o: &Origin) -> Result<String, String> {
    if o.machine.is_empty() {
        return tmux_screen(&o.tmux_pane);
    }
    let m = machine_of(o)?;
    crate::machines::tmux(&m.host, &["capture-pane", "-p", "-J", "-t", &o.tmux_pane]).map(|s| s.trim_end().to_string())
}

/// Whether a tmux session is one Cue made for a session (its terminal is Cue's own, not a window to open).
pub(super) fn tmux_is_cues(session: &str) -> bool {
    session == TMUX_SESSION || tmux_out(&["show-option", "-v", "-t", &format!("={session}:"), "@cue"]).map(|v| v.trim() == "1").unwrap_or(false)
}

/// A tmux pane as one trip reads it: what tmux names running there, its screen, and whether a shell has it.
pub(super) struct PaneNow {
    pub(super) running: String,
    pub(super) screen: String,
    pub(super) shell_in_front: bool,
}

/// One trip to the pane (this Mac's or a machine's): its command, its terminal and its screen; then, when
/// tmux names a shell, the processes in the foreground of that terminal.
pub(super) fn pane_now(o: &Origin, pane: &str) -> Result<PaneNow, String> {
    let out = tmux_try_for(o, pane, &["display-message", "-p", "-t", pane, "#{pane_current_command}|#{pane_tty}", ";", "capture-pane", "-p", "-J", "-t", pane])?;
    let (first, screen) = out.split_once('\n').unwrap_or((out.as_str(), ""));
    let (running, tty) = first.rsplit_once('|').unwrap_or((first, ""));
    let (running, tty) = (running.trim(), tty.trim());
    let shell_in_front = is_shell(running) && match foreground(o, tty) {
        Some(names) => names.iter().all(|n| is_shell(n)),
        // Its foreground can't be listed (no ps answer, the machine's trip failed): the screen decides.
        None => at_shell_prompt(screen),
    };
    Ok(PaneNow { running: running.to_string(), screen: screen.to_string(), shell_in_front })
}

/// The names of the processes in the foreground of terminal `tty` ("/dev/ttys004"), on this Mac or the
/// session's machine: those `ps` marks `+`. A shell tmux names may only be what started the agent
/// (`sh -c "claude …"` runs it in the shell's own process group), so the agent shows here too; with only
/// shells here, the agent has exited back to one, whatever its prompt looks like. None when it can't be read.
fn foreground(o: &Origin, tty: &str) -> Option<Vec<String>> {
    let tty = tty.strip_prefix("/dev/")?;
    if tty.is_empty() || !tty.chars().all(|c| c.is_ascii_alphanumeric() || c == '/') {
        return None;
    }
    let listing = if o.machine.is_empty() {
        let out = capped(Command::new(bin("ps")).args(["-o", "stat=,comm=", "-t", tty]), CAP).ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())?
    } else {
        crate::machines::run(&crate::machines::get(&o.machine)?.host, &format!("ps -o stat=,comm= -t {tty}")).ok()?
    };
    let names: Vec<String> = listing
        .lines()
        .filter_map(|l| l.trim().split_once(char::is_whitespace))
        .filter(|(stat, _)| stat.contains('+'))
        .map(|(_, comm)| comm.trim().split_whitespace().next().unwrap_or("").rsplit('/').next().unwrap_or("").to_string())
        .collect();
    (!names.is_empty()).then_some(names)
}

/// `text` as one tmux command-line word: a trailing ";" escaped, since tmux reads a word that ends
/// in ";" as the end of the command (and strips a "\;" back to ";").
pub(super) fn tmux_word(text: &str) -> String {
    match text.strip_suffix(';') {
        Some(head) if !head.ends_with('\\') => format!("{head}\\;"),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::focus::type_into;

    #[test]
    fn a_message_typed_into_a_tmux_pane_lands_whole_and_is_submitted() {
        if !tmux_installed() {
            return;
        }
        let name = format!("cue-type-{}", std::process::id());
        let pane = tmux_out(&["new-session", "-d", "-s", &name, "-x", "80", "-y", "10", "-P", "-F", "#{pane_id}", "cat"]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let o = Origin { tmux_pane: pane.clone(), ..Default::default() };
        let t0 = std::time::Instant::now();
        type_into(&o, "it's a two\nline message").unwrap();
        let took = t0.elapsed();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let screen = session_screen(&o);
        run("tmux", &["kill-session", "-t", &name]);
        // cat echoes what Enter submitted: both lines, the quote intact.
        let screen = screen.unwrap();
        assert!(screen.contains("it's a two") && screen.contains("line message"), "{screen:?}");
        assert!(took < std::time::Duration::from_secs(1), "Enter went as soon as the paste showed, not after the whole gap: {took:?}");
        assert!(type_into(&o, "x").is_err(), "a pane that's gone says so");
    }

    #[test]
    fn a_pane_in_a_session_cue_opened_is_cues_and_one_of_yours_isnt() {
        if !tmux_installed() {
            return;
        }
        let pid = std::process::id();
        let mine = format!("cues-test-{pid}");
        let pane = tmux_out(&["new-session", "-d", "-s", &mine, "-x", "80", "-y", "10", "-P", "-F", "#{pane_id}", "cat"]).unwrap();
        let _ = run("tmux", &["set-option", "-t", &format!("={mine}:"), "@cue", "1"]);
        let yours = format!("yours-test-{pid}");
        let your_pane = tmux_out(&["new-session", "-d", "-s", &yours, "-x", "80", "-y", "10", "-P", "-F", "#{pane_id}", "cat"]).unwrap();
        let named = format!("named test {pid}");
        let named_pane = tmux_out(&["new-session", "-d", "-s", &tmux_name(&named), "-x", "80", "-y", "10", "-P", "-F", "#{pane_id}", "cat"]).unwrap();
        let tty_of = |p: &str| tmux_out(&["display-message", "-p", "-t", p, "#{pane_tty}"]).unwrap_or_default();
        let (tty, your_tty, named_tty) = (tty_of(&pane), tty_of(&your_pane), tty_of(&named_pane));
        let (cues, theirs, by_name) = (pane_is_cues(&pane, "x", &tty), pane_is_cues(&your_pane, "x", &your_tty), pane_is_cues(&named_pane, &named, &named_tty));
        let stale = pane_is_cues(&pane, "x", &your_tty);
        let _ = run("tmux", &["kill-session", "-t", &format!("={}", tmux_name(&named))]);
        let _ = run("tmux", &["kill-session", "-t", &format!("={mine}")]);
        let _ = run("tmux", &["kill-session", "-t", &format!("={yours}")]);
        assert!(cues, "tagged as Cue's");
        assert!(by_name, "untagged, but named after the session as Cue names it");
        assert!(!theirs, "a tmux session of your own");
        assert!(!stale, "its id now on another terminal: not the session's pane");
        assert!(!pane_is_cues(&pane, "x", &tty), "a pane that's gone");
    }
}
