//! "Go to session" and "reply": find the agent's terminal and bring it forward or type into it.
//! tmux by pane; iTerm by session id (the reliable path), else by
//! tty; Terminal.app by tty; WezTerm and kitty through their own CLIs.

use crate::model::Origin;
use std::process::Command;

/// Find a CLI tool. An app opened from Finder gets a bare PATH (/usr/bin:/bin:…), so Homebrew's
/// tmux and friends must be looked up explicitly.
fn bin(name: &str) -> String {
    let candidates = [
        format!("/opt/homebrew/bin/{name}"),
        format!("/usr/local/bin/{name}"),
        format!("/Applications/WezTerm.app/Contents/MacOS/{name}"),
        format!("/Applications/kitty.app/Contents/MacOS/{name}"),
    ];
    candidates.into_iter().find(|p| std::path::Path::new(p).exists()).unwrap_or_else(|| name.to_string())
}

fn run(cmd: &str, args: &[&str]) -> bool {
    Command::new(bin(cmd)).args(args).status().map(|s| s.success()).unwrap_or(false)
}

fn osa(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "")
}

fn run_osascript(script: &str) -> Result<String, String> {
    let out = Command::new("osascript").arg("-e").arg(script).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn iterm(predicate: &str) -> Result<bool, String> {
    let script = format!(
        "tell application \"iTerm\"\n\
           repeat with w in windows\n\
             repeat with t in tabs of w\n\
               repeat with s in sessions of t\n\
                 if {predicate} then\n\
                   activate\n\
                   tell w to select\n\
                   select t\n\
                   tell s to select\n\
                   return \"true\"\n\
                 end if\n\
               end repeat\n\
             end repeat\n\
           end repeat\n\
         end tell\n\
         return \"false\""
    );
    run_osascript(&script).map(|r| r == "true")
}

/// Single-quoted for the shell: 'it'\''s' (a typed command line, so a message can't run as code).
pub fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A random session id (UUID v4), for an agent Cue starts: Cue knows the session before it says anything.
pub fn new_session_id() -> String {
    use std::io::Read;
    let mut b = [0u8; 16];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// The command line that starts an agent in a folder: `cd <folder> && claude --session-id <id> -n <name> <message>`.
/// Claude and Pi take the session id Cue picked; Pi has no name option; Codex takes the message only.
#[cfg(test)]
pub fn start_line(agent: &str, cwd: &str, session_id: &str, message: &str, name: &str) -> Result<String, String> {
    start_line_with(agent, cwd, session_id, message, name, "ask", "")
}

/// How much a new session may do without asking you (+ New session → Permissions), per agent, as its
/// flags. "ask": as it normally does. Pi asks through Cue's own gate (Settings → Pi), so "skip" turns
/// that off for this session alone.
fn permission_flags(agent: &str, perm: &str) -> Result<(&'static str, &'static str), String> {
    // (environment before the program, flags after it)
    Ok(match (agent, perm) {
        (_, "ask" | "") => ("", ""),
        ("claude", "auto") => ("", " --permission-mode auto"),
        ("claude", "edits") => ("", " --permission-mode acceptEdits"),
        ("claude", "plan") => ("", " --permission-mode plan"),
        ("claude", "skip") => ("", " --dangerously-skip-permissions"),
        ("codex", "edits") => ("", " --sandbox workspace-write"),
        ("codex", "skip") => ("", " --dangerously-bypass-approvals-and-sandbox"),
        ("pi", "skip") => ("CUE_PI_GATE=off ", ""),
        _ => return Err(format!("{agent} has no “{perm}” permission mode")),
    })
}

/// The same, with permissions (`perm`, see permission_flags) and any extra flags you typed (each word
/// quoted on its own, so it's flags, never more shell).
pub fn start_line_with(agent: &str, cwd: &str, session_id: &str, message: &str, name: &str, perm: &str, extra: &str) -> Result<String, String> {
    let program = match agent {
        "claude" => "claude",
        "pi" => "pi",
        "codex" => "codex",
        other => return Err(format!("unknown agent {other}")),
    };
    let (env, flags) = permission_flags(agent, perm)?;
    let mut line = format!("cd {} && {env}{program}{flags}", shq(cwd));
    for word in extra.split_whitespace() {
        line += &format!(" {}", shq(word));
    }
    if agent != "codex" && !session_id.is_empty() {
        line += &format!(" --session-id {}", shq(session_id));
    }
    if agent == "claude" && !name.trim().is_empty() {
        line += &format!(" -n {}", shq(name.trim()));
    }
    if !message.trim().is_empty() {
        line += &format!(" {}", shq(message.trim()));
    }
    Ok(line)
}

/// Whether Claude Code trusts `dir` already: it or a folder above it was accepted ("Yes, I trust this
/// folder"), as recorded in ~/.claude.json. If not, a session started there first asks in its terminal.
/// Unreadable config: assume it does (no false alarm).
pub fn claude_trusts(dir: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let Ok(text) = std::fs::read_to_string(format!("{home}/.claude.json")) else { return true };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return true };
    trusted_in(&v, dir)
}

/// Claude Code records trust under the folder's real path (/tmp is "/private/tmp" on a Mac), so a
/// folder counts as trusted if it, its real path, or a parent of either was accepted.
fn trusted_in(config: &serde_json::Value, dir: &str) -> bool {
    if trusted_as_written(config, dir) {
        return true;
    }
    match std::fs::canonicalize(dir) {
        Ok(real) if real.to_string_lossy() != dir => trusted_as_written(config, &real.to_string_lossy()),
        _ => false,
    }
}

fn trusted_as_written(config: &serde_json::Value, dir: &str) -> bool {
    let mut p = Some(std::path::Path::new(dir));
    while let Some(d) = p {
        if config.pointer(&format!("/projects/{}", d.to_string_lossy().replace('~', "~0").replace('/', "~1"))).and_then(|x| x.get("hasTrustDialogAccepted")).and_then(|x| x.as_bool()) == Some(true) {
            return true;
        }
        p = d.parent();
    }
    false
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

// ---------- sessions in tmux ----------
/// The tmux session Cue starts sessions in (Settings → New sessions in tmux): one tmux window each, so
/// one terminal window shows them all (in iTerm, attached with `tmux -CC`, as ordinary tabs). They keep
/// running when the terminal, or Cue, quits.
pub const TMUX_SESSION: &str = "cue";
/// The tmux session for sessions you chose to see in a terminal tab ("Runs in iTerm"): tmux underneath
/// all the same (they outlive the terminal and Cue, and Cue's terminal can show them), with iTerm attached
/// to this session alone (`tmux -CC`, each window an ordinary tab), so "Runs in Cue" sessions never show
/// up there.
pub const TMUX_TABS_SESSION: &str = "cue-tabs";

/// The terminal a new session's tab opens in when it isn't in tmux: iTerm if it's installed, else Terminal.
pub fn terminal_name() -> &'static str {
    if std::path::Path::new("/Applications/iTerm.app").exists() { "iTerm" } else { "Terminal" }
}

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

/// tmux's answer, trimmed; None if it failed.
fn tmux_out(args: &[&str]) -> Option<String> {
    let out = Command::new(bin("tmux")).args(args).output().ok()?;
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
    let out = Command::new(bin("tmux")).args(["capture-pane", "-p", "-J", "-t", pane]).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("tmux pane {pane} is gone"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// The keys Cue's screen view offers: enough to answer any menu or prompt (move, pick, confirm, back
/// out), nothing that types text.
pub const SCREEN_KEYS: [&str; 15] = ["Up", "Down", "Left", "Right", "Enter", "Escape", "Tab", "BTab", "Space", "1", "2", "3", "4", "y", "n"];

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

/// tmux for a session: this Mac's, or the one on the machine it runs on (over SSH).
fn tmux_for(o: &Origin, args: &[&str]) -> bool {
    if o.machine.is_empty() {
        return run("tmux", args);
    }
    crate::machines::get(&o.machine).is_some_and(|m| crate::machines::tmux(&m.host, args).is_ok())
}

/// What a session's tmux pane shows right now (this Mac's or a machine's).
pub fn session_screen(o: &Origin) -> Result<String, String> {
    if o.machine.is_empty() {
        return tmux_screen(&o.tmux_pane);
    }
    let m = crate::machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", o.machine))?;
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
fn tmux_name(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c == '.' || c == ':' || c.is_whitespace() { '-' } else { c }).collect();
    if s.is_empty() { "cue".into() } else { s }
}

/// Whether a tmux session is one Cue made for a session (its terminal is Cue's own, not a window to open).
fn tmux_is_cues(session: &str) -> bool {
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

fn open_in_tmux_session(session: &str, line: &str, dir: &str, name: &str) -> Result<NewTab, String> {
    // "|", not a tab: some tmux versions print a tab in -F output as "_" ("%1_/dev/ttys003", one pane id
    // that isn't there). Neither a pane id nor a tty path has a "|". (The start script on a machine does the same.)
    const FMT: &str = "#{pane_id}|#{pane_tty}";
    let exists = run("tmux", &["has-session", "-t", session]);
    let out = if exists {
        tmux_out(&["new-window", "-t", &format!("{session}:"), "-n", name, "-c", dir, "-P", "-F", FMT])
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

/// Which agents this Mac can start (a login shell's PATH: the one your terminal has), asked once.
pub fn agents_installed() -> Vec<String> {
    static FOUND: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            ["claude", "pi", "codex"]
                .into_iter()
                .filter(|a| Command::new(&shell).args(["-lic", &format!("command -v {a}")]).output().map(|o| o.status.success() && !o.stdout.is_empty()).unwrap_or(false))
                .map(String::from)
                .collect()
        })
        .clone()
}

/// End an agent's process so its tab can close without a "processes are running" prompt: SIGTERM
/// (Claude Code, Pi and Codex save as they go and exit on it), then wait up to 3s.
pub fn end_agent(pid: i32) -> Result<(), String> {
    if pid <= 0 {
        return Err("Cue doesn't know this session's process".into());
    }
    let alive = || unsafe { libc::kill(pid, 0) == 0 };
    if !alive() {
        return Ok(());
    }
    unsafe { libc::kill(pid, libc::SIGTERM) };
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !alive() {
            return Ok(());
        }
    }
    Err("it didn't quit within 3 seconds (its tab is left open)".into())
}

/// End a session's agent on its machine, where it runs in tmux and Cue knows no pid. One SIGTERM to
/// every process in its pane, not one process group: the agent can be in a group of its own (Claude
/// Code makes one), and its shell ignores SIGTERM while it waits on its child. The shell then stays,
/// for close_tab's kill-pane to sweep up, as it does for a pane here. One script on the machine does
/// the walk, the signal and the wait: nothing to poll over SSH.
pub fn end_machine_agent(o: &Origin) -> Result<(), String> {
    let m = crate::machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", o.machine))?;
    let pane_pid = match crate::machines::tmux(&m.host, &["display-message", "-p", "-t", &o.tmux_pane, "#{pane_pid}"]) {
        Ok(p) => p.trim().to_string(),
        Err(_) => return Ok(()), // its pane is gone already: nothing to end
    };
    // Its pane's own pid only (list-panes would name every pane in its window).
    if pane_pid.is_empty() || pane_pid == "0" || !pane_pid.chars().all(|c| c.is_ascii_digit()) {
        return Ok(());
    }
    let script = r#"
p='PANE_PID'
# The pane's processes, the first one and everything below it (its shell is the first).
next=" $p"; all=""
while [ -n "$next" ]; do
  prev="$next"; next=""
  for q in $prev; do
    all="$all $q"
    for r in $(ps -eo pid=,ppid= | awk -v q="$q" '$2==q {print $1}'); do next="$next $r"; done
  done
done
# All of them at once, so nothing is reparented before it gets the signal.
kill -TERM $all 2>/dev/null
# Wait for everything but the shell to go (its children are gone when the agent is), up to 3 s.
i=0
while [ $i -lt 15 ]; do
  n=0
  for q in $all; do
    for r in $(ps -eo pid=,ppid= | awk -v q="$q" '$2==q {print $1}'); do n=$((n+1)); done
  done
  [ "$n" -eq 0 ] && { echo GONE; exit 0; }
  sleep 0.2 2>/dev/null || sleep 1
  i=$((i+1))
done
echo STILL
"#.replace("PANE_PID", &pane_pid);
    match crate::machines::run(&m.host, &script)?.trim() {
        "GONE" => Ok(()),
        _ => Err("it didn't quit within 3 seconds (its pane is left open)".into()),
    }
}

/// Close a session's terminal tab (after its agent has ended): its tmux pane, its iTerm session (by
/// id, else tty), or a Terminal window whose only tab it is. Terminal can't close one tab of several
/// from a script: that's an error the caller reports.
pub fn close_tab(o: &Origin) -> Result<String, String> {
    if !o.tmux_pane.is_empty() {
        return if tmux_for(o, &["kill-pane", "-t", &o.tmux_pane]) { Ok(format!("tmux pane {}", o.tmux_pane)) } else { Err(format!("tmux pane {} is gone", o.tmux_pane)) };
    }
    let uuid = o.iterm_session_id.split(':').nth(1).unwrap_or("");
    if app_running("iTerm2") && (!uuid.is_empty() || !o.tty.is_empty()) {
        let pred = if uuid.is_empty() { format!("(tty of s) is \"{}\"", osa(&o.tty)) } else { format!("(id of s) is \"{}\"", osa(uuid)) };
        let script = format!(
            "tell application \"iTerm\"\n\
               repeat with w in windows\n\
                 repeat with t in tabs of w\n\
                   repeat with s in sessions of t\n\
                     if {pred} then\n\
                       close s\n\
                       return \"true\"\n\
                     end if\n\
                   end repeat\n\
                 end repeat\n\
               end repeat\n\
             end tell\n\
             return \"false\""
        );
        if run_osascript(&script)? == "true" {
            return Ok("iTerm tab".into());
        }
    }
    if app_running("Terminal") && !o.tty.is_empty() {
        let script = format!(
            "tell application \"Terminal\"\n\
               repeat with w in windows\n\
                 repeat with t in tabs of w\n\
                   if (tty of t) is \"{}\" then\n\
                     if (count of tabs of w) is 1 then\n\
                       close w\n\
                       return \"true\"\n\
                     end if\n\
                     return \"shared\"\n\
                   end if\n\
                 end repeat\n\
               end repeat\n\
             end tell\n\
             return \"false\"",
            osa(&o.tty)
        );
        match run_osascript(&script)?.as_str() {
            "true" => return Ok("Terminal window".into()),
            "shared" => return Err("Terminal can't close one tab of several from a script: close it yourself".into()),
            _ => {}
        }
    }
    Err("couldn't find its tab".into())
}

fn terminal_app(tty: &str) -> Result<bool, String> {
    let script = format!(
        "tell application \"Terminal\"\n\
           repeat with w in windows\n\
             repeat with t in tabs of w\n\
               if (tty of t) is \"{}\" then\n\
                 activate\n\
                 set selected of t to true\n\
                 set index of w to 1\n\
                 return \"true\"\n\
               end if\n\
             end repeat\n\
           end repeat\n\
         end tell\n\
         return \"false\"",
        osa(tty)
    );
    run_osascript(&script).map(|r| r == "true")
}

fn app_running(name: &str) -> bool {
    Command::new("pgrep").arg("-xq").arg(name).status().map(|s| s.success()).unwrap_or(false)
}

fn by_tty(tty: &str) -> Result<String, String> {
    if tty.is_empty() {
        return Err("no tty recorded for this session".into());
    }
    if app_running("iTerm2") && iterm(&format!("(tty of s) is \"{}\"", osa(tty)))? {
        return Ok(format!("iTerm tab {tty}"));
    }
    if app_running("Terminal") && terminal_app(tty)? {
        return Ok(format!("Terminal tab {tty}"));
    }
    Err(format!("no iTerm or Terminal tab has {tty}"))
}

fn activate_app(term_program: &str) -> Result<String, String> {
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

pub fn focus(o: &Origin) -> Result<String, String> {
    if !o.machine.is_empty() {
        return Err(format!("It runs on {}: open its terminal in Cue (Runs in Cue, over the reply box)", o.machine));
    }
    if !o.tmux_pane.is_empty() {
        let pane = o.tmux_pane.as_str();
        run("tmux", &["select-window", "-t", pane]);
        run("tmux", &["select-pane", "-t", pane]);
        let ask = |fmt: &str| {
            Command::new(bin("tmux"))
                .args(["display-message", "-p", "-t", pane, fmt])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default()
        };
        let client = ask("#{client_tty}");
        if client.is_empty() {
            // No window shows this tmux session (it's detached): open one attached to it. Not for a
            // "Runs in Cue" session: its terminal is the one in Cue's window, and attaching iTerm to that
            // session would mirror every such session into iTerm tabs.
            let name = ask("#{session_name}");
            if name.is_empty() {
                return Err(format!("tmux pane {pane} is gone"));
            }
            if tmux_is_cues(&name) {
                return Err("it runs in Cue: its terminal is here, above the reply box (⌃`)".into());
            }
            return attach_in_new_window(&name).map(|app| format!("tmux session \"{name}\" (wasn't open in any window; opened it in {app})"));
        }
        return by_tty(&client).map(|d| format!("tmux pane {pane} via {d}"));
    }
    if o.term_program == "iTerm.app" {
        if let Some(uuid) = o.iterm_session_id.split(':').nth(1) {
            if iterm(&format!("(id of s) is \"{}\"", osa(uuid)))? {
                return Ok("iTerm session".into());
            }
        }
        return by_tty_now(o);
    }
    if o.term_program == "Apple_Terminal" {
        return by_tty_now(o);
    }
    if !o.wezterm_pane.is_empty() && run("wezterm", &["cli", "activate-pane", "--pane-id", &o.wezterm_pane]) {
        let _ = activate_app("WezTerm");
        return Ok(format!("WezTerm pane {}", o.wezterm_pane));
    }
    if !o.kitty_window_id.is_empty() && kitty(o, &["focus-window", "--match", &format!("id:{}", o.kitty_window_id)]) {
        let _ = activate_app("kitty");
        return Ok(format!("kitty window {}", o.kitty_window_id));
    }
    by_tty(&o.tty).or_else(|_| activate_app(&o.term_program))
}

/// A new terminal window attached to a detached tmux session.
fn attach_in_new_window(session: &str) -> Result<String, String> {
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

/// The command line that picks an older Claude Code session back up: `cd <folder> && claude --resume <id>`.
pub fn resume_line(cwd: &str, session_id: &str) -> Result<String, String> {
    if session_id.is_empty() || !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("that isn't a Claude Code session id".into());
    }
    Ok(format!("cd {} && claude --resume {}", shq(cwd), shq(session_id)))
}

/// The command line that picks a parked session back up, for its agent: `claude --resume <id>`,
/// `codex resume <id>`, `pi --session <id>`, each in its folder.
pub fn resume_line_for(harness: &str, cwd: &str, session_id: &str) -> Result<String, String> {
    if session_id.is_empty() || !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Cue doesn't have a session id it can resume".into());
    }
    let program = match harness {
        "claude" => return resume_line(cwd, session_id),
        "codex" => "codex resume",
        "pi" => "pi --session",
        other => return Err(format!("Cue can't resume a {other} session")),
    };
    Ok(format!("cd {} && {program} {}", shq(cwd), shq(session_id)))
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
fn kitty(o: &Origin, args: &[&str]) -> bool {
    let mut full: Vec<&str> = vec!["@"];
    if !o.kitty_listen_on.is_empty() {
        full.extend(["--to", o.kitty_listen_on.as_str()]);
    }
    full.extend(args);
    run("kitten", &full)
}

fn terminal_app_script(tty: &str, text: &str) -> Result<bool, String> {
    let script = format!(
        "tell application \"Terminal\"\n\
           repeat with w in windows\n\
             repeat with t in tabs of w\n\
               if (tty of t) is \"{}\" then\n\
                 do script \"{}\" in t\n\
                 return \"true\"\n\
               end if\n\
             end repeat\n\
           end repeat\n\
         end tell\n\
         return \"false\"",
        osa(tty),
        osa(text)
    );
    run_osascript(&script).map(|r| r == "true")
}

// ---------- typing a reply into the agent's terminal ----------
// Learned the hard way: text + newline in one burst reads as a paste and sits
// unsubmitted in Claude Code's input box. So: put the text in as one paste with no newline, pause
// (longer for longer text), then send a separate Enter. One paste means bracketed (the markers the
// agent asked its terminal for): everything between them is text, however it arrives in pieces, so
// an Enter can't land in the middle of a long one and send half of it.

/// Pause between typing and Enter, in seconds: 0.6s plus 0.05s per 100 characters, at most 4s.
pub fn enter_gap(text: &str) -> f64 {
    (0.6 + 0.05 * text.chars().count() as f64 / 100.0).min(4.0)
}

/// In tmux Cue can see the pane, so it doesn't sleep the whole gap: it watches until the paste shows in
/// the agent's input box (the end of its text, or Claude Code's "[Pasted text …]" for a long one), then
/// Enter goes straight away. Most messages go in a few tens of milliseconds; a machine's pane is read over
/// SSH, so there it's as long as the paste really takes. Never longer than `enter_gap`.
fn wait_for_paste(o: &Origin, text: &str, before: &str) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs_f64(enter_gap(text));
    let tail = paste_tail(text);
    while std::time::Instant::now() < end {
        if let Ok(now) = session_screen(o) {
            if paste_shows(before, &now, &tail) {
                // Drawn means taken in; a moment more for the agent to be ready for a key.
                std::thread::sleep(std::time::Duration::from_millis(40));
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// A screen without spaces or the input box's side borders: how a wrapped line reads joined up again.
fn squash(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace() && *c != '│').collect()
}

/// The last characters of a message as its input box shows them.
fn paste_tail(text: &str) -> String {
    let t: Vec<char> = squash(text).chars().collect();
    t[t.len().saturating_sub(24)..].iter().collect()
}

/// Has the paste landed on screen since `before`? Its end shows once more than it did, or one more
/// "[Pasted text" placeholder does (the text may be on screen already, from an earlier message).
fn paste_shows(before: &str, now: &str, tail: &str) -> bool {
    let n = |s: &str, pat: &str| if pat.is_empty() { 0 } else { s.matches(pat).count() };
    let (b, a) = (squash(before), squash(now));
    n(&a, tail) > n(&b, tail) || n(&a, "[Pastedtext") > n(&b, "[Pastedtext")
}

fn iterm_predicate(o: &Origin) -> Option<String> {
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

/// The tab with the session's terminal; when that terminal's gone (the conversation moved to another
/// tab, or the terminal app restarted), the one its process is in now.
fn by_tty_now(o: &Origin) -> Result<String, String> {
    by_tty(&o.tty).or_else(|e| match current_tty(o) {
        Some(now) if now != o.tty => by_tty(&now),
        _ => Err(e),
    })
}

/// Do `action` in the agent's iTerm session: the one Cue remembers, else (that one's gone because the
/// conversation moved to another tab with `claude --resume`, or iTerm restarted) the tab its process
/// is in now. Claude Code's list of running sessions has the process; its terminal comes from that.
fn iterm_session_do(o: &Origin, predicate: &str, action: &str) -> Result<bool, String> {
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
fn current_tty(o: &Origin) -> Option<String> {
    let from_list = crate::live::claude().into_iter().find(|q| q.session_id == o.session_id).map(|q| q.pid);
    from_list.into_iter().chain(o.agent_pid).filter(|p| *p > 1).map(crate::live::tty_of).find(|t| !t.is_empty())
}

fn iterm_write(predicate: &str, action: &str) -> Result<bool, String> {
    let script = format!(
        "tell application \"iTerm\"\n\
           repeat with w in windows\n\
             repeat with t in tabs of w\n\
               repeat with s in sessions of t\n\
                 if {predicate} then\n\
                   {action}\n\
                   return \"true\"\n\
                 end if\n\
               end repeat\n\
             end repeat\n\
           end repeat\n\
         end tell\n\
         return \"false\""
    );
    run_osascript(&script).map(|r| r == "true")
}

/// Type `text` into the agent's terminal and submit it: tmux, iTerm, Terminal.app, WezTerm or kitty.
pub fn type_into(o: &Origin, text: &str) -> Result<String, String> {
    let pause = || std::thread::sleep(std::time::Duration::from_secs_f64(enter_gap(text)));
    if !o.tmux_pane.is_empty() {
        let pane = o.tmux_pane.as_str();
        let before = session_screen(o).unwrap_or_default();
        // One paste (bracketed when the app asked for it), so a multi-line reply can't submit early.
        if !tmux_for(o, &["set-buffer", "-b", "cue", text]) || !tmux_for(o, &["paste-buffer", "-p", "-d", "-b", "cue", "-t", pane]) {
            return Err(format!("tmux pane {pane} is gone"));
        }
        wait_for_paste(o, text, &before);
        press_enter(o)?;
        return Ok(format!("tmux pane {pane}"));
    }
    if let Some(pred) = iterm_predicate(o) {
        // iTerm types what it's given: the paste markers go around it by hand (ESC [200~ … ESC [201~), as
        // tmux, kitty and WezTerm do themselves.
        let action = format!(
            "tell s to write text ((character id 27) & \"[200~\" & \"{}\" & (character id 27) & \"[201~\") newline NO\n                   delay {:.2}\n                   tell s to write text \"\"",
            osa(text),
            enter_gap(text)
        );
        if iterm_session_do(o, &pred, &action)? {
            return Ok("iTerm session".into());
        }
        return Err("that iTerm session is gone (its tab was closed?)".into());
    }
    if !o.wezterm_pane.is_empty() {
        if !run("wezterm", &["cli", "send-text", "--pane-id", &o.wezterm_pane, text]) {
            return Err(format!("WezTerm pane {} is gone", o.wezterm_pane));
        }
        pause();
        press_enter(o)?;
        return Ok(format!("WezTerm pane {}", o.wezterm_pane));
    }
    if !o.kitty_window_id.is_empty() {
        let m = format!("id:{}", o.kitty_window_id);
        if !kitty(o, &["send-text", "--bracketed-paste", "enable", "--match", &m, text]) {
            return Err("kitty refused. Turn on allow_remote_control in kitty.conf".into());
        }
        pause();
        press_enter(o)?;
        return Ok(format!("kitty window {}", o.kitty_window_id));
    }
    if o.term_program == "Apple_Terminal" && !o.tty.is_empty() {
        // `do script` sends the text plus a newline; Claude Code reads that burst as a paste, so a
        // second, empty `do script` is the Enter that submits it.
        // Terminal.app has no paste API: newlines would submit early, so lines are joined.
        if !terminal_app_script(&o.tty, &text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" "))? {
            return Err("that Terminal tab is gone".into());
        }
        pause();
        press_enter(o)?;
        return Ok("Terminal tab".into());
    }
    Err("Cue can't type into this terminal (it works with iTerm, Terminal, tmux, WezTerm and kitty). Use Go to session".into())
}

/// One bare Enter: submits a typed reply, and is the retry when one didn't submit.
/// Esc, the key that interrupts Claude Code and Codex mid-turn.
pub fn press_escape(o: &Origin) -> Result<String, String> {
    if !o.tmux_pane.is_empty() {
        return if tmux_for(o, &["send-keys", "-t", &o.tmux_pane, "Escape"]) { Ok(format!("tmux pane {}", o.tmux_pane)) } else { Err("tmux pane is gone".into()) };
    }
    if let Some(pred) = iterm_predicate(o) {
        return match iterm_session_do(o, &pred, "tell s to write text (ASCII character 27) newline NO")? {
            true => Ok("iTerm session".into()),
            false => Err("that iTerm session is gone (its tab was closed?)".into()),
        };
    }
    if !o.wezterm_pane.is_empty() {
        return if run("wezterm", &["cli", "send-text", "--no-paste", "--pane-id", &o.wezterm_pane, "\x1b"]) { Ok(format!("WezTerm pane {}", o.wezterm_pane)) } else { Err("WezTerm pane is gone".into()) };
    }
    if !o.kitty_window_id.is_empty() {
        let m = format!("id:{}", o.kitty_window_id);
        return if kitty(o, &["send-text", "--match", &m, "\x1b"]) { Ok(format!("kitty window {}", o.kitty_window_id)) } else { Err("kitty refused".into()) };
    }
    // Terminal.app only takes whole lines from scripts, never a bare key.
    Err("Cue can't press Esc in Terminal.app: use Go to tab and press Esc there".into())
}

pub fn press_enter(o: &Origin) -> Result<(), String> {
    if !o.tmux_pane.is_empty() {
        return if tmux_for(o, &["send-keys", "-t", &o.tmux_pane, "Enter"]) { Ok(()) } else { Err("tmux pane is gone".into()) };
    }
    if let Some(pred) = iterm_predicate(o) {
        return iterm_session_do(o, &pred, "tell s to write text \"\"").map(|_| ());
    }
    if !o.wezterm_pane.is_empty() {
        return if run("wezterm", &["cli", "send-text", "--no-paste", "--pane-id", &o.wezterm_pane, "\r"]) { Ok(()) } else { Err("WezTerm pane is gone".into()) };
    }
    if !o.kitty_window_id.is_empty() {
        let m = format!("id:{}", o.kitty_window_id);
        return if kitty(o, &["send-text", "--match", &m, "\r"]) { Ok(()) } else { Err("kitty refused".into()) };
    }
    if o.term_program == "Apple_Terminal" && !o.tty.is_empty() {
        return terminal_app_script(&o.tty, "").map(|_| ());
    }
    Err("no terminal to press Enter in".into())
}

#[cfg(test)]
mod start_tests {
    use super::*;

    #[test]
    fn a_paste_shows_once_its_end_is_on_screen_once_more() {
        let before = "> earlier: fix the login\n────\n> ";
        let tail = paste_tail("now fix the login");
        assert!(!paste_shows(before, before, &tail), "nothing new yet");
        assert!(!paste_shows(before, "> earlier: fix the login\n────\n> now fix", &tail), "only part of it");
        assert!(paste_shows(before, "> earlier: fix the login\n────\n> now fix the\n  login", &tail), "wrapped is fine");
        // The same words as an earlier message: only a second copy counts.
        let tail = paste_tail("fix the login");
        assert!(!paste_shows(before, "> earlier: fix the login\n────\n> ", &tail));
        assert!(paste_shows(before, "> earlier: fix the login\n────\n> fix the login", &tail));
    }

    #[test]
    fn a_long_paste_shows_as_its_placeholder() {
        let tail = paste_tail(&"a long message ".repeat(80));
        assert!(paste_shows("> ", "> [Pasted text #1 +12 lines]", &tail));
        assert!(!paste_shows("> [Pasted text #1 +3 lines]", "> [Pasted text #1 +3 lines]", &tail));
    }

    #[test]
    fn a_folder_is_trusted_if_it_or_a_parent_was_accepted() {
        let c = serde_json::json!({ "projects": { "/a/b": { "hasTrustDialogAccepted": true }, "/x": { "hasTrustDialogAccepted": false } } });
        assert!(trusted_in(&c, "/a/b"));
        assert!(trusted_in(&c, "/a/b/c/d"));
        assert!(!trusted_in(&c, "/a"));
        assert!(!trusted_in(&c, "/x/y"));
    }

    #[test]
    fn a_folder_reached_through_a_symlink_is_trusted_under_its_real_path() {
        // Claude Code stores /private/tmp for a session started in /tmp: the link must still count.
        let base = std::env::temp_dir().join(format!("cue-trust-{}", std::process::id()));
        let real = base.join("real");
        let link = base.join("link");
        std::fs::create_dir_all(real.join("deeper")).unwrap();
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let real_s = std::fs::canonicalize(&real).unwrap().to_string_lossy().into_owned();
        let c = serde_json::json!({ "projects": { real_s.clone(): { "hasTrustDialogAccepted": true } } });
        assert!(trusted_in(&c, &link.to_string_lossy()));
        assert!(trusted_in(&c, &link.join("deeper").to_string_lossy()), "a child of the link resolves through it too");
        let none = serde_json::json!({ "projects": {} });
        assert!(!trusted_in(&none, &link.to_string_lossy()));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_start_line_quotes_everything_it_types() {
        assert_eq!(start_line("claude", "/x/my repo", "S1", "fix it's bug", "bugfix").unwrap(), "cd '/x/my repo' && claude --session-id 'S1' -n 'bugfix' 'fix it'\\''s bug'");
        assert_eq!(start_line("claude", "/x", "", "", "").unwrap(), "cd '/x' && claude");
        assert_eq!(start_line("pi", "/x", "P1", "hello $(rm -rf ~)", "ignored").unwrap(), "cd '/x' && pi --session-id 'P1' 'hello $(rm -rf ~)'");
        assert_eq!(start_line("codex", "/x", "C1", "go", "").unwrap(), "cd '/x' && codex 'go'");
        assert!(start_line("bash", "/x", "", "", "").is_err());
    }

    #[test]
    fn a_parked_session_resumes_with_its_own_agents_command() {
        assert_eq!(resume_line_for("claude", "/x/my repo", "3f2a-91").unwrap(), "cd '/x/my repo' && claude --resume '3f2a-91'");
        assert_eq!(resume_line_for("codex", "/x", "a91f").unwrap(), "cd '/x' && codex resume 'a91f'");
        assert_eq!(resume_line_for("pi", "/x", "P1").unwrap(), "cd '/x' && pi --session 'P1'");
        assert!(resume_line_for("pi", "/x", "a; rm -rf ~").is_err(), "only an id, never more shell");
        assert!(resume_line_for("codex", "/x", "").is_err());
        assert!(resume_line_for("bash", "/x", "S1").is_err());
    }

    #[test]
    fn a_new_sessions_permissions_and_extra_flags_go_on_its_command_line() {
        let l = |agent, perm, extra| start_line_with(agent, "/x", "", "", "", perm, extra);
        assert_eq!(l("claude", "skip", "").unwrap(), "cd '/x' && claude --dangerously-skip-permissions");
        assert_eq!(l("claude", "plan", "--model opus").unwrap(), "cd '/x' && claude --permission-mode plan '--model' 'opus'");
        assert_eq!(l("codex", "skip", "").unwrap(), "cd '/x' && codex --dangerously-bypass-approvals-and-sandbox");
        assert_eq!(l("codex", "edits", "").unwrap(), "cd '/x' && codex --sandbox workspace-write");
        // Pi asks through Cue's gate: skipping turns it off for this session only.
        assert_eq!(l("pi", "skip", "").unwrap(), "cd '/x' && CUE_PI_GATE=off pi");
        assert_eq!(l("pi", "ask", "").unwrap(), "cd '/x' && pi");
        assert!(l("pi", "plan", "").is_err(), "a mode the agent doesn't have");
        // Extra flags are words, quoted: never more shell.
        assert_eq!(l("claude", "", "; rm -rf ~").unwrap(), "cd '/x' && claude ';' 'rm' '-rf' '~'");
        let id = new_session_id();
        assert_eq!((id.len(), &id[14..15]), (36, "4"), "a UUID v4: {id}");
        assert_ne!(id, new_session_id());
    }
}

#[cfg(test)]
mod iterm_tests {
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

    use super::*;

    #[test]
    fn a_terminal_app_session_is_not_looked_for_in_iterm() {
        let o = Origin { term_program: "Apple_Terminal".into(), tty: "/dev/ttys004".into(), ..Default::default() };
        assert_eq!(iterm_predicate(&o), None);
        let o = Origin { term_program: "iTerm.app".into(), iterm_session_id: "w0t1p0:ABC".into(), tty: "/dev/ttys004".into(), ..Default::default() };
        assert_eq!(iterm_predicate(&o).as_deref(), Some("(id of s) is \"ABC\""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
