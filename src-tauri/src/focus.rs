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
fn shq(s: &str) -> String {
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
pub fn start_line(agent: &str, cwd: &str, session_id: &str, message: &str, name: &str) -> Result<String, String> {
    let program = match agent {
        "claude" => "claude",
        "pi" => "pi",
        "codex" => "codex",
        other => return Err(format!("unknown agent {other}")),
    };
    let mut line = format!("cd {} && {program}", shq(cwd));
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
pub struct NewTab {
    pub what: String,
    pub term_program: String,
    pub iterm_session_id: String,
    pub tty: String,
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
        return Ok(NewTab { what: "a new iTerm tab".into(), term_program: "iTerm.app".into(), iterm_session_id: format!("cue:{id}"), tty: tty.to_string() });
    }
    let tty = run_osascript(&format!("tell application \"Terminal\"\nset t to do script \"{}\"\nreturn tty of t\nend tell", osa(line)))?;
    Ok(NewTab { what: "a new Terminal window".into(), term_program: "Apple_Terminal".into(), iterm_session_id: String::new(), tty })
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

/// Close a session's terminal tab (after its agent has ended): its tmux pane, its iTerm session (by
/// id, else tty), or a Terminal window whose only tab it is. Terminal can't close one tab of several
/// from a script: that's an error the caller reports.
pub fn close_tab(o: &Origin) -> Result<String, String> {
    if !o.tmux_pane.is_empty() {
        return if run("tmux", &["kill-pane", "-t", &o.tmux_pane]) { Ok(format!("tmux pane {}", o.tmux_pane)) } else { Err(format!("tmux pane {} is gone", o.tmux_pane)) };
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
            // No window shows this tmux session (it's detached): open one attached to it.
            let name = ask("#{session_name}");
            if name.is_empty() {
                return Err(format!("tmux pane {pane} is gone"));
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
// unsubmitted in Claude Code's input box. So: type the text with no newline, pause (longer for
// longer text), then send a separate Enter.

/// Pause between typing and Enter, in seconds: 0.6s plus 0.05s per 100 characters, at most 2s.
pub fn enter_gap(text: &str) -> f64 {
    (0.6 + 0.05 * text.chars().count() as f64 / 100.0).min(2.0)
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
        // One paste (bracketed when the app asked for it), so a multi-line reply can't submit early.
        if !run("tmux", &["set-buffer", "-b", "cue", text]) || !run("tmux", &["paste-buffer", "-p", "-d", "-b", "cue", "-t", pane]) {
            return Err(format!("tmux pane {pane} is gone"));
        }
        pause();
        press_enter(o)?;
        return Ok(format!("tmux pane {pane}"));
    }
    if let Some(pred) = iterm_predicate(o) {
        let action = format!(
            "tell s to write text \"{}\" newline NO\n                   delay {:.2}\n                   tell s to write text \"\"",
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
        return if run("tmux", &["send-keys", "-t", &o.tmux_pane, "Escape"]) { Ok(format!("tmux pane {}", o.tmux_pane)) } else { Err("tmux pane is gone".into()) };
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
        return if run("tmux", &["send-keys", "-t", &o.tmux_pane, "Enter"]) { Ok(()) } else { Err("tmux pane is gone".into()) };
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
        let id = new_session_id();
        assert_eq!((id.len(), &id[14..15]), (36, "4"), "a UUID v4: {id}");
        assert_ne!(id, new_session_id());
    }
}

#[cfg(test)]
mod iterm_tests {
    use super::*;

    #[test]
    fn a_terminal_app_session_is_not_looked_for_in_iterm() {
        let o = Origin { term_program: "Apple_Terminal".into(), tty: "/dev/ttys004".into(), ..Default::default() };
        assert_eq!(iterm_predicate(&o), None);
        let o = Origin { term_program: "iTerm.app".into(), iterm_session_id: "w0t1p0:ABC".into(), tty: "/dev/ttys004".into(), ..Default::default() };
        assert_eq!(iterm_predicate(&o).as_deref(), Some("(id of s) is \"ABC\""));
    }
}
