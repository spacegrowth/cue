//! Other machines Cue runs sessions on, over SSH (+ New → Machine). Cue never asks for or keeps a
//! password or key: it uses your own SSH (keys, an agent, ~/.ssh/config), always non-interactively, and
//! shares one connection per machine (OpenSSH's ControlMaster). When a machine needs a login, Cue opens
//! `ssh` in a terminal tab and you log in there yourself; from then on Cue uses the connection you opened.
//! It never installs anything there: it uses the agents you installed, and says how to install the rest.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Machine {
    /// What Cue calls it ("build-box").
    pub name: String,
    /// What `ssh` connects to: an alias from ~/.ssh/config, or user@host (for a port, use an alias).
    pub host: String,
    /// What you renamed it to, shown in its place everywhere (empty: its name, shortened). The name stays
    /// as it was: sessions and their log copies know the machine by it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// The tools Cue looks for on a machine (it installs none of them).
pub const TOOLS: [&str; 6] = ["claude", "codex", "pi", "tmux", "node", "npm"];

pub fn list() -> Vec<Machine> {
    crate::config::load().pointer("/machines/list").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn save(list: &[Machine]) -> Result<(), String> {
    crate::config::remember("machines.list", serde_json::to_value(list).map_err(|e| e.to_string())?)
}

pub fn get(name: &str) -> Option<Machine> {
    list().into_iter().find(|m| m.name == name)
}

/// A host `ssh` can take as its destination, and nothing it would read as an option or more shell.
fn valid_host(h: &str) -> bool {
    !h.is_empty() && !h.starts_with('-') && h.len() <= 255 && h.chars().all(|c| c.is_ascii_alphanumeric() || "@._-".contains(c))
}

/// Add a machine: `host` as you'd type after `ssh`; `name` defaults to the host (without user@).
pub fn add(host: &str, name: &str) -> Result<Machine, String> {
    let host = host.trim();
    if !valid_host(host) {
        return Err("Use an SSH alias from ~/.ssh/config, or user@host (letters, digits, @ . _ - only)".into());
    }
    let name = Some(name.trim()).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| host.rsplit('@').next().unwrap_or(host).to_string());
    let mut all = list();
    if all.iter().any(|m| m.name == name || m.host == host) {
        return Err(format!("{name} is added already"));
    }
    let m = Machine { name, host: host.to_string(), label: String::new() };
    all.push(m.clone());
    save(&all)?;
    Ok(m)
}

/// Rename it: what Cue shows for it from now on (empty: back to its name).
pub fn rename(name: &str, label: &str) -> Result<Machine, String> {
    let label = label.trim();
    if label.chars().count() > 40 {
        return Err("Keep it under 40 characters".into());
    }
    let mut all = list();
    let m = all.iter_mut().find(|m| m.name == name).ok_or("that machine isn't added")?;
    m.label = label.to_string();
    let out = m.clone();
    save(&all)?;
    Ok(out)
}

/// What Cue shows for a machine: what you renamed it to, else its name shortened (`short_name`).
pub fn display_name(name: &str) -> String {
    short_name(&get(name).map(|m| m.label).filter(|l| !l.is_empty()).unwrap_or_else(|| name.to_string()))
}

pub fn remove(name: &str) -> Result<(), String> {
    let mut all = list();
    all.retain(|m| m.name != name);
    save(&all)
}

/// The shared connection: one per machine, kept 15 minutes after its last use. Its socket lives in
/// ~/.ssh (a short path: macOS caps socket paths near 104 characters).
pub fn shared() -> [&'static str; 6] {
    ["-o", "ControlMaster=auto", "-o", "ControlPath=~/.ssh/cue-%C", "-o", "ControlPersist=15m"]
}

/// Single-quoted for a POSIX shell (the one helper Cue has for it).
pub use crate::focus::shq;

/// A machine's name, short enough for a notification: its first part (no user@, no domain; an IP address
/// stays whole), and past 10 characters its first 5 and last 5. (The window shortens it the same way: machShort.)
pub fn short_name(name: &str) -> String {
    let h = name.split_once('@').map_or(name, |(_, h)| h);
    let n = if h.chars().all(|c| c.is_ascii_digit() || c == '.') { h } else { h.split('.').next().filter(|p| !p.is_empty()).unwrap_or(h) };
    let c: Vec<char> = n.chars().collect();
    if c.len() > 10 { format!("{}…{}", c[..5].iter().collect::<String>(), c[c.len() - 5..].iter().collect::<String>()) } else { n.to_string() }
}

/// A long-running `script` on `host` (a stream that keeps going until it's stopped or the line drops),
/// never asking for anything, with no input. Callers set where its output goes, then spawn it.
pub fn stream_command(host: &str, script: &str) -> Command {
    let mut c = Command::new("/usr/bin/ssh");
    c.args(shared())
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-o", "ServerAliveInterval=15", "-T", host, &remote_command(script)])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    c
}

/// End streams an earlier Cue left running: the ssh processes whose script carries `mark`, nothing else.
pub fn end_streams(mark: &str) {
    let _ = Command::new("/usr/bin/pkill").args(["-f", &format!("^/usr/bin/ssh .*{mark}")]).status();
}

/// Run `script` (POSIX sh) on `host`, never asking for anything: your login shell there sets things up
/// (so your PATH applies: nvm, ~/.local/bin…), then /bin/sh runs the script, whatever that shell is (zsh,
/// say, splits words differently). Ok(stdout) or Err(what ssh said).
pub fn run(host: &str, script: &str) -> Result<String, String> {
    if !valid_host(host) {
        return Err("that isn't an SSH host".into());
    }
    let out = ssh_command(host, script).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { format!("exit {}", out.status.code().unwrap_or(-1)) } else { err })
    }
}

/// The same, with `input` on the script's standard input (a file to write there).
pub fn run_with_input(host: &str, script: &str, input: &str) -> Result<String, String> {
    use std::io::Write;
    if !valid_host(host) {
        return Err("that isn't an SSH host".into());
    }
    let mut child = ssh_command(host, script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child.stdin.take().ok_or("no input")?.write_all(input.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// The `ssh` behind `run` and its sibling, for you to spawn yourself (ssh passes the script's exit code
/// through): the shared connection, never asking for anything, the script's stderr silenced.
pub(crate) fn ssh_command(host: &str, script: &str) -> Command {
    let mut c = Command::new("/usr/bin/ssh");
    c.args(shared())
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-T", host, &remote_command(script)]);
    c
}

/// `ssh_command` keeping the script's stderr: a "!" line's card shows it, as it does on this Mac.
pub(crate) fn ssh_command_err(host: &str, script: &str) -> Command {
    let mut c = Command::new("/usr/bin/ssh");
    c.args(shared())
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-T", host, &remote_command_err(script)]);
    c
}

/// What ssh runs on the machine for `script`: your login shell there sets things up (your PATH), then
/// /bin/sh runs the script. The login shell's own noise is silenced, so only ssh's failures come back.
pub fn remote_command(script: &str) -> String {
    format!("{} 2>/dev/null", remote_command_err(script))
}

/// `remote_command` keeping the script's stderr, for a script whose output is the point.
pub(crate) fn remote_command_err(script: &str) -> String {
    format!("exec \"${{SHELL:-/bin/sh}}\" -lic {}", shq(&format!("exec /bin/sh -c {}", shq(script))))
}

/// `tmux` with these arguments on the machine (each one quoted).
pub fn tmux(host: &str, args: &[&str]) -> Result<String, String> {
    run(host, &format!("tmux {}", args.iter().map(|a| shq(a)).collect::<Vec<_>>().join(" ")))
}

/// A folder on the machine as the shell there should read it: `~` stays your home there.
pub(crate) fn remote_dir(dir: &str) -> String {
    match dir.trim().strip_prefix('~') {
        Some(rest) => format!("\"$HOME\"{}", shq(rest)),
        None => shq(dir.trim()),
    }
}

/// Start `line` in a new window of Cue's tmux session on the machine, in `dir` there: (pane, its tty,
/// the folder's full path). Typed into its shell, so the window stays when the agent exits.
pub fn open_in_tmux(m: &Machine, line: &str, dir: &str, name: &str) -> Result<(String, String, String), String> {
    let out = run(&m.host, &open_script(crate::focus::TMUX_SESSION, line, dir, name))?;
    if out.contains("NODIR") {
        return Err(format!("{dir} isn't a folder on {}", m.name));
    }
    if out.contains("NOTMUX") {
        return Err(format!("tmux isn't installed on {}: pick it in + New to see how to install it", m.name));
    }
    let field = |k: &str| out.lines().find_map(|l| l.strip_prefix(k)).unwrap_or("").trim().to_string();
    let p = field("PANE:");
    let (pane, tty) = p.split_once('|').unwrap_or((p.as_str(), ""));
    if pane.is_empty() {
        return Err(format!("tmux on {} didn't say which pane it opened", m.name));
    }
    Ok((pane.to_string(), tty.to_string(), field("DIR:")))
}

/// The script behind open_in_tmux, for tmux session `s`.
fn open_script(s: &str, line: &str, dir: &str, name: &str) -> String {
    format!(
        r#"cd {dir} 2>/dev/null || {{ echo "NODIR"; exit 0; }}
command -v tmux >/dev/null 2>&1 || {{ echo "NOTMUX"; exit 0; }}
if tmux has-session -t {s} 2>/dev/null; then p=$(tmux new-window -t {s}: -n {name} -c "$PWD" -P -F '#{{pane_id}}|#{{pane_tty}}'); else p=$(tmux new-session -d -s {s} -n {name} -c "$PWD" -x 200 -y 50 -P -F '#{{pane_id}}|#{{pane_tty}}'); fi
pane=${{p%%|*}}
tmux send-keys -t "$pane" -l {line} && tmux send-keys -t "$pane" Enter
echo "PANE:$p"
echo "DIR:$PWD""#,
        dir = remote_dir(dir),
        s = shq(s),
        name = shq(name),
        line = shq(line),
    )
}

/// Whether ssh failed for want of a login (a password, a new host key to accept, a locked key): the
/// kind you fix by logging in once yourself.
pub(crate) fn wants_login(err: &str) -> bool {
    let e = err.to_lowercase();
    ["permission denied", "host key verification failed", "authentication", "passphrase", "keyboard-interactive"].iter().any(|k| e.contains(k))
}

/// Can Cue reach it, and what's installed there: {ok, needs_login, error, os, tools: {claude: true, …}}.
pub fn check(m: &Machine) -> Value {
    let script = format!("uname -s; for c in {}; do command -v $c >/dev/null 2>&1 && echo \"has $c\"; done; true", TOOLS.join(" "));
    match run(&m.host, &script) {
        Ok(out) => {
            let os = out.lines().next().unwrap_or("").trim().to_string();
            let tools: serde_json::Map<String, Value> = TOOLS.iter().map(|t| (t.to_string(), json!(out.lines().any(|l| l.trim() == format!("has {t}"))))).collect();
            // For what's missing: how you'd install it there yourself.
            let help: serde_json::Map<String, Value> = tools.iter().filter(|(_, has)| *has == &json!(false)).map(|(t, _)| (t.clone(), json!(how_to_install(t)))).collect();
            json!({ "ok": true, "os": os, "tools": tools, "help": help })
        }
        Err(e) => json!({ "ok": false, "needs_login": wants_login(&e), "error": e.lines().last().unwrap_or("").to_string() }),
    }
}

/// How to install a tool that isn't on a machine, for you to run there yourself.
pub fn how_to_install(tool: &str) -> &'static str {
    match tool {
        "claude" => "curl -fsSL https://claude.ai/install.sh | bash",
        "codex" => "npm install -g @openai/codex",
        "pi" => "npm install -g @earendil-works/pi-coding-agent",
        "tmux" => "sudo apt install tmux (or your system's package manager)",
        "node" | "npm" => "install Node.js (nvm is easiest)",
        _ => "",
    }
}

/// Log in yourself: `ssh` in a terminal tab, opening the shared connection Cue then uses. Cue sees
/// nothing you type there.
pub fn login(m: &Machine) -> Result<String, String> {
    if !valid_host(&m.host) {
        return Err("that isn't an SSH host".into());
    }
    let line = format!("ssh {} {}", shared().join(" "), m.host);
    crate::focus::open_tab_with(&line).map(|t| t.what)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_name_is_shortened_for_labels() {
        assert_eq!(short_name("ubuntu"), "ubuntu");
        assert_eq!(short_name("me@build.home.lan"), "build");
        assert_eq!(short_name("my-long-ubuntu-box.lan"), "my-lo…u-box");
        assert_eq!(short_name("10.0.0.12"), "10.0.0.12");
        assert_eq!(short_name("exactly10c"), "exactly10c");
    }

    #[test]
    fn a_machine_is_an_ssh_destination_and_nothing_more() {
        assert!(valid_host("build-box"));
        assert!(valid_host("me@10.0.0.4"));
        assert!(!valid_host("-oProxyCommand=evil"), "never an option");
        assert!(!valid_host("box; rm -rf ~"), "never more shell");
        assert!(!valid_host("box host2"));
        assert!(!valid_host(""));
    }

    #[test]
    fn ssh_failures_that_a_login_fixes_say_so() {
        assert!(wants_login("me@box: Permission denied (publickey,password)."));
        assert!(wants_login("Host key verification failed."));
        assert!(!wants_login("ssh: Could not resolve hostname nope: nodename nor servname provided"));
        assert!(!wants_login("ssh: connect to host box port 22: Operation timed out"));
    }

    #[test]
    fn a_folder_on_the_machine_keeps_its_tilde_as_home_there() {
        assert_eq!(remote_dir("~/code/my app"), "\"$HOME\"'/code/my app'");
        assert_eq!(remote_dir("/srv/x"), "'/srv/x'");
        assert_eq!(remote_dir("~"), "\"$HOME\"''");
    }

    #[test]
    fn starting_a_session_there_opens_a_tmux_window_in_the_folder_and_types_its_line() {
        if !crate::focus::tmux_installed() {
            return;
        }
        let s = format!("cue-mtest-{}", std::process::id());
        let script = open_script(&s, "echo started from cue", "~", "my app");
        let out = std::process::Command::new("/bin/sh").args(["-c", &remote_command(&script)]).output().unwrap();
        let out = String::from_utf8_lossy(&out.stdout).to_string();
        std::thread::sleep(std::time::Duration::from_millis(900));
        let pane = out.lines().find_map(|l| l.strip_prefix("PANE:")).unwrap_or("").split('|').next().unwrap_or("").to_string();
        let screen = std::process::Command::new(crate::focus::tmux_bin()).args(["capture-pane", "-p", "-t", &pane]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let windows = std::process::Command::new(crate::focus::tmux_bin()).args(["list-windows", "-t", &s, "-F", "#{window_name}"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let _ = std::process::Command::new(crate::focus::tmux_bin()).args(["kill-session", "-t", &s]).status();
        assert!(pane.starts_with('%'), "it says which pane: {out:?}");
        assert_eq!(out.lines().find_map(|l| l.strip_prefix("DIR:")), Some(std::env::var("HOME").unwrap().as_str()), "~ is home there");
        assert!(screen.contains("started from cue"), "its line was typed and ran: {screen:?}");
        assert_eq!(windows.trim(), "my app");
        let bad = std::process::Command::new("/bin/sh").args(["-c", &remote_command(&open_script(&s, "x", "/no/such/dir", "n"))]).output().unwrap();
        assert!(String::from_utf8_lossy(&bad.stdout).contains("NODIR"));
    }

    #[test]
    fn what_ssh_runs_there_survives_the_remote_shells_quoting() {
        // ssh hands the command to the remote shell as `$SHELL -c <it>`: the same, here.
        let script = "printf '%s|' \"it's\" $((1 + 1)); for w in a b; do printf '%s|' \"$w\"; done";
        for sh in ["/bin/sh", "/bin/zsh", "/bin/bash"] {
            let out = std::process::Command::new(sh).args(["-c", &remote_command(script)]).output().unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout), "it's|2|a|b|", "through {sh}");
        }
        let tm = format!("tmux {}", ["send-keys", "-l", "say \"hi\"; rm -rf ~ $(x)"].iter().map(|a| shq(a)).collect::<Vec<_>>().join(" "));
        let echo = tm.replacen("tmux", "printf '%s\\n'", 1);
        let out = std::process::Command::new("/bin/sh").args(["-c", &remote_command(&echo)]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "send-keys\n-l\nsay \"hi\"; rm -rf ~ $(x)\n", "each tmux argument arrives whole, nothing runs");
    }

    #[test]
    fn a_script_is_one_quoted_word_for_the_remote_shell() {
        assert_eq!(shq("echo 'hi'; ls"), "'echo '\\''hi'\\''; ls'");
    }
}
