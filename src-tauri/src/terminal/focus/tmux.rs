//! Sessions in tmux: the tmux sessions Cue starts them in, a pane's screen and what runs in its foreground,
//! keys pressed in it, and tmux on a session's machine (over SSH) as on this Mac.

use super::apps::{attach_in_new_window, terminal_name, NewTab};
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

/// Type `line` into a tmux pane and press Enter.
pub fn tmux_type_line(pane: &str, line: &str) -> Result<(), String> {
    if run("tmux", &["send-keys", "-t", pane, "-l", line]) && run("tmux", &["send-keys", "-t", pane, "Enter"]) {
        Ok(())
    } else {
        Err(format!("tmux pane {pane} is gone"))
    }
}

/// Close a tmux pane Cue opened (its window goes with it, and its session if that was the last one).
pub fn tmux_kill_pane(pane: &str) {
    let _ = run("tmux", &["kill-pane", "-t", pane]);
}

/// tmux's answer, trimmed; None if it failed or didn't answer in time.
pub(crate) fn tmux_out(args: &[&str]) -> Option<String> {
    let out = capped(Command::new(bin("tmux")).args(args), tmux_cap()).ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether a pane still lives in the tmux session a Cue session of this name opened: its
/// session's name, asked of tmux, must be the one `name` makes (a reboot starts pane ids
/// over, so an id alone isn't proof it's the same pane).
pub(crate) fn pane_in_its_session(pane: &str, name: &str) -> bool {
    let base = tmux_name(name);
    tmux_out(&["display-message", "-p", "-t", pane, "#{session_name}"])
        .is_some_and(|s| s == base || s.starts_with(&format!("{base}-")))
}

/// What a tmux pane shows right now, as plain text (wrapped lines joined), blank lines at the end dropped.
pub fn tmux_screen(pane: &str) -> Result<String, String> {
    let out = capped(Command::new(bin("tmux")).args(["capture-pane", "-p", "-J", "-t", pane]), tmux_cap()).map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("tmux pane {pane} is gone"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// The keys Cue's screen view offers: enough to answer any menu or prompt (move, pick, confirm, back
/// out), nothing that types text.
const SCREEN_KEYS: [&str; 20] = ["Up", "Down", "Left", "Right", "Enter", "Escape", "Tab", "BTab", "Space", "1", "2", "3", "4", "5", "6", "7", "8", "9", "y", "n"];

/// Press `keys` in a session's tmux pane, in order. Only ones from SCREEN_KEYS.
pub fn tmux_keys(o: &Origin, keys: &[String]) -> Result<(), String> {
    if let Some(k) = keys.iter().find(|k| !SCREEN_KEYS.contains(&k.as_str())) {
        return Err(format!("{k} isn't a key Cue presses"));
    }
    for k in keys {
        if !tmux_for(o, &["send-keys", "-t", &o.tmux_pane, k]) {
            return Err(format!("tmux pane {} is gone", o.tmux_pane));
        }
    }
    Ok(())
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

/// Start `line` in a tmux session of its own, named after the session (`name`, made safe for tmux and
/// unique: "cue-bug-fixes", then "cue-bug-fixes-2"), in `dir`; typed into its shell, so the window stays
/// when the agent exits, as a tab would. One tmux session each, so `tmux ls` reads like the session list
/// and each keeps its own size. Tagged (@cue) as Cue's own: "Go to tab" knows its terminal is in Cue.
pub fn open_in_tmux(line: &str, dir: &str, name: &str) -> Result<NewTab, String> {
    let base = tmux_name(name);
    let taken: Vec<String> = tmux_out(&["list-sessions", "-F", "#{session_name}"]).map(|o| o.lines().map(String::from).collect()).unwrap_or_default();
    let mut session = base.clone();
    let mut n = 2;
    while taken.iter().any(|t| *t == session) {
        session = format!("{base}-{n}");
        n += 1;
    }
    let tab = open_in_tmux_session(&session, line, dir, name)?;
    let _ = run("tmux", &["set-option", "-t", &format!("={session}"), "@cue", "1"]);
    Ok(tab)
}

/// A tmux session name from a session's name: no '.', ':' or whitespace (tmux's rules), never empty.
pub(super) fn tmux_name(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c == '.' || c == ':' || c.is_whitespace() { '-' } else { c }).collect();
    if s.is_empty() { "cue".into() } else { s }
}

/// Whether a tmux session is one Cue made for a session (its terminal is Cue's own, not a window to open).
pub(super) fn tmux_is_cues(session: &str) -> bool {
    session == TMUX_SESSION || tmux_out(&["show-option", "-v", "-t", &format!("={session}"), "@cue"]).map(|v| v.trim() == "1").unwrap_or(false)
}

/// "Runs in iTerm" with tmux installed: the same, in the tabs session, shown in a terminal tab. iTerm is
/// attached to that session once (then each new window is a new tab of its own accord); without iTerm,
/// a Terminal window attached to it.
pub fn open_in_tmux_tab(line: &str, dir: &str, name: &str) -> Result<NewTab, String> {
    let attached = tmux_out(&["list-clients", "-t", TMUX_TABS_SESSION]).map(|c| !c.trim().is_empty()).unwrap_or(false);
    let mut tab = open_in_tmux_session(TMUX_TABS_SESSION, line, dir, name)?;
    let app = if attached { terminal_name().to_string() } else { attach_in_new_window(TMUX_TABS_SESSION)? };
    tab.what = format!("{app} (kept running by tmux)");
    Ok(tab)
}

pub(super) fn open_in_tmux_session(session: &str, line: &str, dir: &str, name: &str) -> Result<NewTab, String> {
    // "|", not a tab: some tmux versions print a tab in -F output as "_" ("%1_/dev/ttys003", one pane id
    // that isn't there). Neither a pane id nor a tty path has a "|". (The start script on a machine does the same.)
    const FMT: &str = "#{pane_id}|#{pane_tty}";
    // "=": that session exactly, never another whose name starts with it.
    let exists = run("tmux", &["has-session", "-t", &format!("={session}")]);
    let out = if exists {
        tmux_out(&["new-window", "-t", &format!("={session}:"), "-n", name, "-c", dir, "-P", "-F", FMT])
    } else {
        // Detached, so it needs a size until a terminal attaches.
        tmux_out(&["new-session", "-d", "-s", session, "-n", name, "-c", dir, "-x", "200", "-y", "50", "-P", "-F", FMT])
    }
    .ok_or("tmux couldn't open a window")?;
    let (pane, tty) = out.split_once('|').unwrap_or((out.as_str(), ""));
    // The pane it named is really there (an answer Cue misread would name one that isn't).
    if pane.is_empty() || tmux_out(&["display-message", "-p", "-t", pane, "#{pane_id}"]).as_deref() != Some(pane) {
        return Err(format!("tmux opened a window, but Cue can't find its pane (tmux said {out:?})"));
    }
    // Nothing to type: the window waits, ready, for `tmux_type_line`.
    if !line.is_empty() {
        tmux_type_line(pane, line)?;
    }
    // It shows in Cue's own terminal (the drawer over the reply box): no terminal window opens for it.
    Ok(NewTab { what: "Cue".into(), term_program: "tmux".into(), tty: tty.to_string(), tmux_pane: pane.to_string(), ..Default::default() })
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
    fn cues_screen_view_reads_a_tmux_pane_and_presses_only_its_keys() {
        if !tmux_installed() {
            return;
        }
        let name = format!("cue-test-{}", std::process::id());
        let pane = tmux_out(&["new-session", "-d", "-s", &name, "-x", "80", "-y", "10", "-P", "-F", "#{pane_id}", "cat"]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let o = Origin { tmux_pane: pane.clone(), ..Default::default() };
        tmux_keys(&o, &["y".into(), "Enter".into(), "2".into()]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let screen = session_screen(&o);
        assert!(tmux_keys(&o, &["rm -rf /".into()]).is_err(), "only the screen's keys");
        run("tmux", &["kill-session", "-t", &name]);
        let screen = screen.unwrap();
        assert!(screen.lines().any(|l| l.trim() == "y"), "{screen:?}");
        assert!(screen.ends_with('2'), "{screen:?}");
        assert!(tmux_screen(&pane).is_err(), "a pane that's gone says so");
    }

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
    fn a_new_tmux_window_is_read_back_as_its_pane_and_its_tty() {
        if !tmux_installed() {
            return;
        }
        let session = format!("cue-fmt-test-{}", std::process::id());
        let tab = open_in_tmux_session(&session, "true", "/tmp", "t").unwrap();
        let _ = run("tmux", &["kill-session", "-t", &session]);
        assert!(tab.tmux_pane.starts_with('%') && tab.tmux_pane[1..].chars().all(|c| c.is_ascii_digit()), "a pane id alone: {:?}", tab.tmux_pane);
        assert!(tab.tty.starts_with("/dev/"), "its tty: {:?}", tab.tty);
    }

    #[test]
    fn a_window_can_be_opened_empty_typed_into_later_and_closed() {
        if !tmux_installed() {
            return;
        }
        let session = format!("cue-empty-test-{}", std::process::id());
        let tab = open_in_tmux_session(&session, "", "/tmp", "t").unwrap();
        tmux_type_line(&tab.tmux_pane, "echo typed-$((40+2))").unwrap();
        let mut screen = String::new();
        for _ in 0..100 {
            screen = tmux_screen(&tab.tmux_pane).unwrap_or_default();
            if screen.contains("typed-42") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        tmux_kill_pane(&tab.tmux_pane);
        let gone = !run("tmux", &["has-session", "-t", &format!("={session}")]);
        let _ = run("tmux", &["kill-session", "-t", &format!("={session}")]);
        assert!(screen.contains("typed-42"), "what's typed later runs: {screen:?}");
        assert!(gone, "closing its only pane closes it");
        assert!(tmux_type_line(&tab.tmux_pane, "x").is_err(), "a closed pane says so");
    }

    #[test]
    fn a_pane_in_its_session_says_so_and_one_elsewhere_doesnt() {
        if !tmux_installed() {
            return;
        }
        let pid = std::process::id();
        let label = format!("alive test {pid}");
        let session = tmux_name(&label);
        let pane = tmux_out(&["new-session", "-d", "-s", &session, "-x", "80", "-y", "10", "-P", "-F", "#{pane_id}", "cat"]).unwrap();
        assert!(pane_in_its_session(&pane, &label), "its session carries the name from the label");
        let other = tmux_name(&format!("other test {pid}"));
        let other_pane = tmux_out(&["new-session", "-d", "-s", &other, "-x", "80", "-y", "10", "-P", "-F", "#{pane_id}", "cat"]).unwrap();
        assert!(!pane_in_its_session(&other_pane, &label), "a live pane of another session isn't this one's");
        let _ = run("tmux", &["kill-session", "-t", &session]);
        let _ = run("tmux", &["kill-session", "-t", &other]);
        assert!(!pane_in_its_session(&pane, &label), "a pane that's gone says so");
    }
}
