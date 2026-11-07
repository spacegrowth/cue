//! The terminal apps Cue reaches by AppleScript or their own CLIs: iTerm (by session id, the reliable path,
//! else by tty), Terminal.app (by tty), WezTerm and kitty; and new tabs and windows opened in them.

use super::exec::{app_running, bin, osa, run, run_osascript};
use super::tmux::TMUX_TABS_SESSION;
use crate::model::Origin;
use std::process::Command;

/// An AppleScript that finds the iTerm session `predicate` names (`s`, in tab `t` of window `w`) and does
/// `then` there (which returns what it found); "`otherwise`" when no session is it.
fn each_iterm_session(predicate: &str, then: &str, otherwise: &str) -> String {
    format!(
        "tell application \"iTerm\"\n\
           repeat with w in windows\n\
             repeat with t in tabs of w\n\
               repeat with s in sessions of t\n\
                 if {predicate} then\n\
                   {then}\n\
                 end if\n\
               end repeat\n\
             end repeat\n\
           end repeat\n\
         end tell\n\
         return \"{otherwise}\""
    )
}

/// The same for the Terminal.app tab on `tty` (`t`, in window `w`).
fn each_terminal_tab(tty: &str, then: &str, otherwise: &str) -> String {
    format!(
        "tell application \"Terminal\"\n\
           repeat with w in windows\n\
             repeat with t in tabs of w\n\
               if (tty of t) is \"{}\" then\n\
                 {then}\n\
               end if\n\
             end repeat\n\
           end repeat\n\
         end tell\n\
         return \"{otherwise}\"",
        osa(tty)
    )
}

/// What a contents-reading script found: its text after "found", or None when it found nothing (an
/// empty screen still says "found").
fn found_text(out: &str) -> Option<String> {
    out.strip_prefix("found").map(|t| t.trim_start_matches(['\n', '\r']).trim_end().to_string())
}

/// Do `action` in the iTerm session `predicate` names: whether there was one.
pub(super) fn iterm_write(predicate: &str, action: &str) -> Result<bool, String> {
    run_osascript(&each_iterm_session(predicate, &format!("{action}\nreturn \"true\""), "false")).map(|r| r == "true")
}

/// Bring the iTerm session `predicate` names forward.
fn iterm_select(predicate: &str) -> Result<bool, String> {
    iterm_write(predicate, "activate\ntell w to select\nselect t\ntell s to select")
}

/// Do `then` in the Terminal.app tab on `tty`; its answer ("true", or what `then` returns).
pub(super) fn terminal_tab(tty: &str, then: &str) -> Result<String, String> {
    run_osascript(&each_terminal_tab(tty, then, "false"))
}

/// Where a session Cue started runs, so Cue can type into it and jump to it from the start.
#[derive(Default)]
pub struct NewTab {
    pub what: String,
    pub term_program: String,
    pub iterm_session_id: String,
    pub tty: String,
    pub tmux_pane: String,
}

/// Open a new terminal tab in the background (Cue stays in front) and type `line` into it: iTerm (a
/// new tab in its front window, or a new window), else Terminal. Typed into a login shell, so the
/// agent's PATH (nvm, Homebrew) is the one you have in your terminal.
pub fn open_tab_with(line: &str) -> Result<NewTab, String> {
    if std::path::Path::new("/Applications/iTerm.app").exists() {
        let script = format!(
            "tell application \"iTerm\"\n\
               if (count of windows) is 0 then\n\
                 set w to (create window with default profile)\n\
               else\n\
                 set w to current window\n\
                 tell w to create tab with default profile\n\
               end if\n\
               set s to current session of w\n\
               tell s to write text \"{}\"\n\
               return (id of s) & \"\t\" & (tty of s)\n\
             end tell",
            osa(line)
        );
        let out = run_osascript(&script)?;
        let (id, tty) = out.split_once('\t').unwrap_or((out.as_str(), ""));
        // The same shape as $ITERM_SESSION_ID ("w0t1p0:<uuid>"): focus and typing read the part after ':'.
        return Ok(NewTab { what: "a new iTerm tab".into(), term_program: "iTerm.app".into(), iterm_session_id: format!("cue:{id}"), tty: tty.to_string(), ..Default::default() });
    }
    let tty = run_osascript(&format!("tell application \"Terminal\"\nset t to do script \"{}\"\nreturn tty of t\nend tell", osa(line)))?;
    Ok(NewTab { what: "a new Terminal window".into(), term_program: "Apple_Terminal".into(), tty, ..Default::default() })
}

/// The terminal a new session's tab opens in when it isn't in tmux: iTerm if it's installed, else Terminal.
pub fn terminal_name() -> &'static str {
    if std::path::Path::new("/Applications/iTerm.app").exists() { "iTerm" } else { "Terminal" }
}

/// Bring the iTerm or Terminal.app tab on `tty` forward.
pub(super) fn by_tty(tty: &str) -> Result<String, String> {
    if tty.is_empty() {
        return Err("no tty recorded for this session".into());
    }
    if app_running("iTerm2") && iterm_select(&format!("(tty of s) is \"{}\"", osa(tty)))? {
        return Ok(format!("iTerm tab {tty}"));
    }
    if app_running("Terminal") && terminal_tab(tty, "activate\nset selected of t to true\nset index of w to 1\nreturn \"true\"")? == "true" {
        return Ok(format!("Terminal tab {tty}"));
    }
    Err(format!("no iTerm or Terminal tab has {tty}"))
}

/// The tab with the session's terminal; when that terminal's gone (the conversation moved to another
/// tab, or the terminal app restarted), the one its process is in now.
pub(super) fn by_tty_now(o: &Origin) -> Result<String, String> {
    by_tty(&o.tty).or_else(|e| match current_tty(o) {
        Some(now) if now != o.tty => by_tty(&now),
        _ => Err(e),
    })
}

/// Bring the iTerm session Cue remembers forward, else its tab by tty.
pub(super) fn focus_iterm(o: &Origin) -> Result<String, String> {
    if let Some(uuid) = o.iterm_session_id.split(':').nth(1) {
        if iterm_select(&format!("(id of s) is \"{}\"", osa(uuid)))? {
            return Ok("iTerm session".into());
        }
    }
    by_tty_now(o)
}

pub(super) fn activate_app(term_program: &str) -> Result<String, String> {
    let app = match term_program {
        "ghostty" => "Ghostty",
        "WezTerm" => "WezTerm",
        "vscode" => "Visual Studio Code",
        "WarpTerminal" => "Warp",
        "zed" => "Zed",
        "" => return Err("unknown terminal".into()),
        other => other,
    };
    let ok = Command::new("open").arg("-a").arg(app).status().map(|s| s.success()).unwrap_or(false);
    if ok { Ok(format!("opened {app} (can't pick the exact tab there)")) } else { Err(format!("couldn't open {app}")) }
}

/// A new terminal window attached to a detached tmux session.
pub(super) fn attach_in_new_window(session: &str) -> Result<String, String> {
    let tmux = bin("tmux");
    // Cue's tabs session in iTerm: attached with -CC (iTerm's tmux mode, its windows as ordinary iTerm tabs),
    // from a new tab of the window you have rather than a window of its own. With iTerm's "Open tmux windows
    // as tabs in the attaching window" and "Bury the tmux client session", that's where they all go.
    if session == TMUX_TABS_SESSION && std::path::Path::new("/Applications/iTerm.app").exists() {
        return open_tab_with(&format!("{tmux} -CC attach -t {TMUX_TABS_SESSION}")).map(|t| t.what);
    }
    // Single-quoted for the shell inside the window; tmux names can't contain quotes we'd need to escape further.
    run_in_new_window(&format!("{} attach -t '{}'", tmux, session.replace('\'', "")))
}

/// A new terminal window running `cmd`: iTerm if it's installed, else Terminal.
fn run_in_new_window(cmd: &str) -> Result<String, String> {
    let iterm_installed = std::path::Path::new("/Applications/iTerm.app").exists();
    if iterm_installed {
        let script = format!(
            "tell application \"iTerm\"\n\
               activate\n\
               create window with default profile command \"{}\"\n\
             end tell",
            osa(cmd)
        );
        return run_osascript(&script).map(|_| "a new iTerm window".into());
    }
    let script = format!("tell application \"Terminal\"\nactivate\ndo script \"exec {}\"\nend tell", osa(cmd));
    run_osascript(&script).map(|_| "a new Terminal window".into())
}

/// kitty's remote control (needs allow_remote_control in kitty.conf).
pub(super) fn kitty(o: &Origin, args: &[&str]) -> bool {
    let mut full: Vec<&str> = vec!["@"];
    if !o.kitty_listen_on.is_empty() {
        full.extend(["--to", o.kitty_listen_on.as_str()]);
    }
    full.extend(args);
    run("kitten", &full)
}

/// Type `text` (plus the newline `do script` adds) into the Terminal.app tab on `tty`: whether it's there.
pub(super) fn terminal_app_script(tty: &str, text: &str) -> Result<bool, String> {
    terminal_tab(tty, &format!("do script \"{}\" in t\nreturn \"true\"", osa(text))).map(|r| r == "true")
}

/// The visible text of the agent's iTerm session: the one Cue remembers, else the tab its process is in
/// now (as `iterm_session_do` finds it). The script answers "found" plus the text, so an empty screen
/// isn't mistaken for no session.
pub(super) fn iterm_contents(o: &Origin, predicate: &str) -> Option<String> {
    let read = |predicate: &str| found_text(&run_osascript(&each_iterm_session(predicate, "return \"found\" & linefeed & (contents of s)", "")).ok()?);
    if let Some(text) = read(predicate) {
        return Some(text);
    }
    match current_tty(o) {
        Some(tty) if tty != o.tty => read(&format!("(tty of s) is \"{}\"", osa(&tty))),
        _ => None,
    }
}

/// The visible text of the Terminal.app tab on `tty` (its `contents`: the screen, not the scrollback).
pub(super) fn terminal_app_contents(tty: &str) -> Option<String> {
    found_text(&run_osascript(&each_terminal_tab(tty, "return \"found\" & linefeed & (contents of t)", "")).ok()?)
}

pub(super) fn iterm_predicate(o: &Origin) -> Option<String> {
    if o.term_program == "iTerm.app" {
        if let Some(uuid) = o.iterm_session_id.split(':').nth(1) {
            return Some(format!("(id of s) is \"{}\"", osa(uuid)));
        }
    }
    // By tty only when the session may be in iTerm (TERM_PROGRAM unset, or tmux's own): a Terminal.app
    // session's tty is never in iTerm, and it must go on to Terminal, not stop at "iTerm session is gone".
    let maybe_iterm = matches!(o.term_program.as_str(), "" | "iTerm.app" | "tmux");
    if maybe_iterm && !o.tty.is_empty() && app_running("iTerm2") {
        return Some(format!("(tty of s) is \"{}\"", osa(&o.tty)));
    }
    None
}

/// Do `action` in the agent's iTerm session: the one Cue remembers, else (that one's gone because the
/// conversation moved to another tab with `claude --resume`, or iTerm restarted) the tab its process
/// is in now. Claude Code's list of running sessions has the process; its terminal comes from that.
pub(super) fn iterm_session_do(o: &Origin, predicate: &str, action: &str) -> Result<bool, String> {
    if iterm_write(predicate, action)? {
        return Ok(true);
    }
    match current_tty(o) {
        Some(tty) if tty != o.tty => iterm_write(&format!("(tty of s) is \"{}\"", osa(&tty)), action),
        _ => Ok(false),
    }
}

/// The terminal the session's process runs in now. Claude Code: its list of running sessions has the
/// session's current process (a `claude --resume` in another tab is a new one). Any agent: the process
/// its hook last reported (Codex, Pi too), while it's still running.
pub(super) fn current_tty(o: &Origin) -> Option<String> {
    let from_list = crate::logs::live::claude().into_iter().find(|q| q.session_id == o.session_id).map(|q| q.pid);
    from_list.into_iter().chain(o.agent_pid).filter(|p| *p > 1).map(crate::logs::live::tty_of).find(|t| !t.is_empty())
}

/// Close the agent's iTerm session (by id, else tty), or the Terminal window whose only tab it is.
/// None when neither app has it.
pub(super) fn close_app_tab(o: &Origin) -> Option<Result<String, String>> {
    let uuid = o.iterm_session_id.split(':').nth(1).unwrap_or("");
    if app_running("iTerm2") && (!uuid.is_empty() || !o.tty.is_empty()) {
        let pred = if uuid.is_empty() { format!("(tty of s) is \"{}\"", osa(&o.tty)) } else { format!("(id of s) is \"{}\"", osa(uuid)) };
        match iterm_write(&pred, "close s") {
            Ok(true) => return Some(Ok("iTerm tab".into())),
            Ok(false) => {}
            Err(e) => return Some(Err(e)),
        }
    }
    if app_running("Terminal") && !o.tty.is_empty() {
        match terminal_tab(&o.tty, "if (count of tabs of w) is 1 then\nclose w\nreturn \"true\"\nend if\nreturn \"shared\"").as_deref() {
            Ok("true") => return Some(Ok("Terminal window".into())),
            Ok("shared") => return Some(Err("Terminal can't close one tab of several from a script: close it yourself".into())),
            Ok(_) => {}
            Err(e) => return Some(Err(e.clone())),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_app_session_is_not_looked_for_in_iterm() {
        let o = Origin { term_program: "Apple_Terminal".into(), tty: "/dev/ttys004".into(), ..Default::default() };
        assert_eq!(iterm_predicate(&o), None);
        let o = Origin { term_program: "iTerm.app".into(), iterm_session_id: "w0t1p0:ABC".into(), tty: "/dev/ttys004".into(), ..Default::default() };
        assert_eq!(iterm_predicate(&o).as_deref(), Some("(id of s) is \"ABC\""));
    }
}
