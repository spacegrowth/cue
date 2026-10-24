//! Other machines Cue runs sessions on, over SSH (+ New → Machine). Cue never asks for or keeps a
//! password or key: it uses your own SSH (keys, an agent, ~/.ssh/config), always non-interactively, and
//! shares one connection per machine (OpenSSH's ControlMaster). When a machine needs a login, Cue opens
//! `ssh` in a terminal tab and you log in there yourself; from then on Cue uses the connection you opened.
//! It never installs anything there: it uses the agents you installed, and says how to install the rest.

pub(crate) mod hooks;
pub(crate) mod logs;

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
/// ~/.ssh (a short path: macOS caps socket paths near 104 characters). It checks the line every 15 s and drops
/// it after 3 missed answers (45 s): keepalives belong to whichever ssh opened the connection, so they go on
/// every call, else a line that died in sleep or a Wi-Fi change hangs every call through it.
pub fn shared() -> [&'static str; 10] {
    ["-o", "ControlMaster=auto", "-o", "ControlPath=~/.ssh/cue-%C", "-o", "ControlPersist=15m", "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3"]
}

/// Single-quoted for a POSIX shell (the one helper Cue has for it).
pub use crate::terminal::focus::shq;

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
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-T", host, &remote_command(script)])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    c
}

/// End the streams an earlier Cue left running whose script carries `mark`: ssh processes with that mark whose
/// Cue has gone (they're left to launchd, parent 1), each by its exact pid. Never by a pattern kill, and never
/// a running Cue's: two Cues side by side (a build beside the installed one) keep their own.
pub fn end_streams(mark: &str) {
    let Ok(out) = Command::new("/bin/ps").args(["-axo", "pid=,ppid=,command="]).output() else { return };
    for (pid, ppid, cmd) in String::from_utf8_lossy(&out.stdout).lines().filter_map(ps_line) {
        if ppid == 1 && left_stream(&cmd, mark) {
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }
}

/// A line of `ps -o pid=,ppid=,command=`: pid, parent, command.
fn ps_line(line: &str) -> Option<(i32, i32, String)> {
    let mut f = line.split_whitespace();
    let pid = f.next()?.parse().ok()?;
    let ppid = f.next()?.parse().ok()?;
    Some((pid, ppid, f.collect::<Vec<_>>().join(" ")))
}

/// One of Cue's streams: ssh itself (not something that merely mentions the mark) running a script with it.
fn left_stream(cmd: &str, mark: &str) -> bool {
    cmd.starts_with("/usr/bin/ssh ") && cmd.contains(mark)
}

/// Run `script` (POSIX sh) on `host`, never asking for anything: your login shell there sets things up
/// (so your PATH applies: nvm, ~/.local/bin…), then /bin/sh runs the script, whatever that shell is (zsh,
/// say, splits words differently). Ok(stdout) or Err(what ssh said).
pub fn run(host: &str, script: &str) -> Result<String, String> {
    if !valid_host(host) {
        return Err("that isn't an SSH host".into());
    }
    let out = output_retrying(|| ssh_command(host, script))?;
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
    let out = wait_capped(child)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// `run_with_input` for a file's bytes, written in pieces: `sent` hears how many have gone so far.
pub fn run_with_bytes(host: &str, script: &str, bytes: &[u8], sent: &dyn Fn(u64)) -> Result<String, String> {
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
    {
        let mut input = child.stdin.take().ok_or("no input")?;
        let mut done = 0u64;
        for piece in bytes.chunks(64 * 1024) {
            if let Err(e) = input.write_all(piece) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
            done += piece.len() as u64;
            sent(done);
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else if out.status.code() == Some(255) {
        Err(cant_connect(host))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Run an ssh trip; once more, a moment later, when ssh itself said the machine didn't take it (exit 255
/// and a refusal or a connect timeout: nothing ran there, so running it again can't do anything twice).
/// The shared connection takes 10 trips at once (sshd's MaxSessions), and a burst of them can find it full.
fn output_retrying(mk: impl Fn() -> Command) -> Result<std::process::Output, String> {
    let out = capped_output(mk())?;
    if out.status.code() == Some(255) && not_taken(&String::from_utf8_lossy(&out.stderr)) {
        std::thread::sleep(std::time::Duration::from_millis(400));
        return capped_output(mk());
    }
    Ok(out)
}

/// The longest a call to a machine may take before Cue gives up on it: the connection's keepalives notice a dead
/// line within 45 s; this is the backstop for one that's alive but stuck (a setup that installs things has room).
const CALL_CAP: std::time::Duration = std::time::Duration::from_secs(300);

/// `cmd`'s output, as `Command::output` gives it, but never waiting past CALL_CAP: it's ended then.
fn capped_output(mut cmd: Command) -> Result<std::process::Output, String> {
    use std::process::Stdio;
    let child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    wait_capped(child)
}

/// Wait for `child`'s output, ending it (by its pid) once CALL_CAP has passed.
fn wait_capped(child: std::process::Child) -> Result<std::process::Output, String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let pid = child.id() as i32;
    let done = std::sync::Arc::new(AtomicBool::new(false));
    let watch = done.clone();
    std::thread::spawn(move || {
        std::thread::sleep(CALL_CAP);
        if !watch.load(Ordering::SeqCst) {
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    });
    let out = child.wait_with_output().map_err(|e| e.to_string());
    done.store(true, Ordering::SeqCst);
    out
}

/// ssh's words for a trip that never started on the machine.
fn not_taken(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    ["refused", "administratively prohibited", "mux_client", "timed out", "kex_exchange_identification"].iter().any(|w| e.contains(w))
}

/// The `ssh` behind `run` and its sibling, for you to spawn yourself (ssh passes the script's exit code
/// through): the shared connection, never asking for anything, the script's stderr silenced. Once Cue
/// has written the machine's environment there (`env_file`), the script runs in /bin/sh with that file
/// sourced, and the login shell is skipped: it costs about 300 ms a call (your .bashrc), against 50 for
/// ssh itself. Should the file be gone (you cleaned ~/.cue), the script still runs, with sh's own PATH.
pub(crate) fn ssh_command(host: &str, script: &str) -> Command {
    let command = if env_file(host) { env_command(script) } else { remote_command(script) };
    let mut c = Command::new("/usr/bin/ssh");
    c.args(shared())
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-T", host, &command]);
    c
}

/// Where the machine's environment is kept on the machine, under your home there.
const ENV_FILE: &str = ".cue/env.sh";

/// What ssh runs on the machine for `script` once ENV_FILE is there: /bin/sh sources it, then runs the
/// script. Guarded: a non-interactive sh that can't source a file ends there, and the script with it.
fn env_command(script: &str) -> String {
    format!("exec /bin/sh -c {} 2>/dev/null", shq(&format!("[ -r \"$HOME/{ENV_FILE}\" ] && . \"$HOME/{ENV_FILE}\"\n{script}")))
}

/// `ssh host`, through your login shell there (`remote_command`): for the two calls that must not depend
/// on the environment file, reading the environment and writing the file.
fn ssh_login(host: &str, script: &str) -> Command {
    let mut c = Command::new("/usr/bin/ssh");
    c.args(shared())
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-T", host, &remote_command(script)]);
    c
}

/// Whether your login shell's environment on the machine (PATH, CLAUDE_CONFIG_DIR, whatever it exports)
/// is written to ENV_FILE there, as `export` lines /bin/sh sources. It's asked of the login shell and
/// written once (readable by you only, put in place whole), remembered while Cue runs, and asked again
/// when the machine is reached again (`forget`: a reboot may have changed it). A file, not part of each
/// command: an environment in the ssh command line shows in `ps` on both ends, and Linux refuses a
/// single argument past 128 KiB. False until it's written, or when the machine can't say or won't take
/// the file: then each call goes through the login shell, as before.
fn env_file(host: &str) -> bool {
    if crate::locking::lock(caches(), "caches").env.contains(host) {
        return true;
    }
    let Ok(out) = ssh_login(host, "env -0").output() else { return false };
    if !out.status.success() {
        return false;
    }
    let Some(exports) = env_exports(&out.stdout) else { return false };
    let write = format!(r#"umask 077; mkdir -p "$HOME/.cue" && cat >"$HOME/{ENV_FILE}.tmp" && mv -f "$HOME/{ENV_FILE}.tmp" "$HOME/{ENV_FILE}""#);
    let written = (|| {
        use std::io::Write;
        let mut child = ssh_login(host, &write).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok()?;
        child.stdin.take()?.write_all(exports.as_bytes()).ok()?;
        child.wait().ok().filter(|st| st.success())
    })()
    .is_some();
    if written {
        crate::locking::lock(caches(), "caches").env.insert(host.to_string());
    }
    written
}

/// `env -0` output as `export` lines a POSIX shell takes. What's the connection's own (SSH_*), the shell's
/// (BASH_*, ZSH_*, prompts, history, SHLVL, PWD) or a function stays out: /bin/sh must not inherit those
/// from another shell. Nor what bash keeps read-only (UID, EUID, PPID, SHELLOPTS, BASHOPTS, BASH_VERSINFO)
/// or IFS: where /bin/sh is bash, exporting one of those fails and ends the whole script. None when
/// there's nothing usable (no PATH: `env -0` didn't work there).
fn env_exports(raw: &[u8]) -> Option<String> {
    let skip = |k: &str| {
        matches!(
            k,
            "SHLVL" | "PWD" | "OLDPWD" | "_" | "TERM" | "LINES" | "COLUMNS" | "HISTFILE" | "HISTSIZE" | "HISTFILESIZE" | "LS_COLORS" | "UID" | "EUID" | "PPID" | "SHELLOPTS" | "IFS" | "BASHOPTS" | "BASH_VERSINFO"
        ) || ["SSH_", "BASH", "ZSH", "PS", "PROMPT", "HIST"].iter().any(|p| k.starts_with(p))
    };
    let name_ok = |k: &str| !k.is_empty() && !k.starts_with(|c: char| c.is_ascii_digit()) && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let mut lines = String::new();
    let mut has_path = false;
    for entry in raw.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let Ok(entry) = std::str::from_utf8(entry) else { continue };
        let Some((k, v)) = entry.split_once('=') else { continue };
        if !name_ok(k) || skip(k) {
            continue;
        }
        has_path |= k == "PATH";
        lines.push_str(&format!("export {k}={}\n", shq(v)));
    }
    has_path.then_some(lines)
}

/// What Cue keeps about a machine while it runs, by host: that its environment file is written there,
/// and where tmux is.
#[derive(Default)]
struct Caches {
    env: std::collections::HashSet<String>,
    tmux: std::collections::HashMap<String, String>,
}

fn caches() -> &'static std::sync::Mutex<Caches> {
    static C: std::sync::OnceLock<std::sync::Mutex<Caches>> = std::sync::OnceLock::new();
    C.get_or_init(Default::default)
}

/// The machine was reached again (Cue started, or it came back from a reboot or a dropped line): what Cue
/// kept about it is asked again next time.
pub fn forget(host: &str) {
    let mut c = crate::locking::lock(caches(), "caches");
    c.env.remove(host);
    c.tmux.remove(host);
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

/// `tmux` with these arguments on the machine (each one quoted). Run straight from tmux's full path once
/// Cue knows it: your login shell there can take a second to start, and the window asks often.
pub fn tmux(host: &str, args: &[&str]) -> Result<String, String> {
    let args = args.iter().map(|a| shq(a)).collect::<Vec<_>>().join(" ");
    match tmux_path(host) {
        Some(p) => tmux_trip(host, &format!("{} {args}", shq(&p)), true),
        None => tmux_trip(host, &format!("tmux {args}"), false),
    }
}

/// A tmux trip to the machine. When ssh itself failed (exit 255: tmux never exits so), the machine wasn't
/// reached: that says so, in general terms (whatever sits in front of it, its login may just have
/// expired), never as a pane that's gone.
fn tmux_trip(host: &str, script: &str, direct: bool) -> Result<String, String> {
    if !valid_host(host) {
        return Err("that isn't an SSH host".into());
    }
    let out = output_retrying(|| if direct { direct_command(host, script) } else { ssh_command(host, script) })?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).to_string());
    }
    if out.status.code() == Some(255) {
        eprintln!("cue: {host} not reached: {}", String::from_utf8_lossy(&out.stderr).trim());
        return Err(cant_connect(host));
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if err.is_empty() { format!("exit {}", out.status.code().unwrap_or(-1)) } else { err })
}

/// What Cue says when a machine can't be reached.
pub fn cant_connect(host: &str) -> String {
    let name = list().into_iter().find(|m| m.host == host).map_or(host.to_string(), |m| display_name(&m.name));
    format!("Can't connect to {name}. Check that you can ssh to it from a terminal, then try again.")
}

/// `tmux` with `args` on the machine, then `also` in the same trip, whose failure is ignored: what the
/// first said, whatever came of the second (tmux before 3.1 refuses a value it doesn't know, and a
/// refusal in one command line would lose the first one's answer).
pub fn tmux_then(host: &str, args: &[&str], also: &[&str]) -> Result<String, String> {
    let q = |a: &[&str]| a.iter().map(|a| shq(a)).collect::<Vec<_>>().join(" ");
    match tmux_path(host) {
        Some(p) => tmux_trip(host, &format!("{p} {}; {p} {} >/dev/null 2>&1; true", q(args), q(also), p = shq(&p)), true),
        None => tmux_trip(host, &format!("tmux {}; tmux {} >/dev/null 2>&1; true", q(args), q(also)), false),
    }
}

/// Where tmux is on the machine (a full path), asked once of your login shell there (your PATH) and kept
/// while Cue runs. None when it can't say (not reachable, no tmux): ask again next time.
pub fn tmux_path(host: &str) -> Option<String> {
    if let Some(p) = crate::locking::lock(caches(), "caches").tmux.get(host) {
        return Some(p.clone());
    }
    let p = run(host, "command -v tmux").ok()?.lines().last()?.trim().to_string();
    if !p.starts_with('/') {
        return None;
    }
    crate::locking::lock(caches(), "caches").tmux.insert(host.to_string(), p.clone());
    Some(p)
}

/// ssh running the script in /bin/sh, no login shell: for a script that names its programs by full path.
fn direct_command(host: &str, script: &str) -> Command {
    let mut c = Command::new("/usr/bin/ssh");
    c.args(shared()).args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-T", host, &format!("exec /bin/sh -c {}", shq(script))]);
    c
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
    let out = run(&m.host, &open_script(crate::terminal::focus::TMUX_SESSION, line, dir, name))?;
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
if tmux has-session -t {xs} 2>/dev/null; then p=$(tmux new-window -t {xs}: -n {name} -c "$PWD" -P -F '#{{pane_id}}|#{{pane_tty}}'); else p=$(tmux new-session -d -s {s} -n {name} -c "$PWD" -x 200 -y 50 -P -F '#{{pane_id}}|#{{pane_tty}}'); fi
pane=${{p%%|*}}
tmux send-keys -t "$pane" -l {line} && tmux send-keys -t "$pane" Enter
echo "PANE:$p"
echo "DIR:$PWD""#,
        dir = remote_dir(dir),
        s = shq(s),
        // "=cue": that session exactly (tmux would take "cue" to mean "cue-smoke", say, when there's no "cue").
        xs = shq(&format!("={s}")),
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
    crate::terminal::focus::open_tab_with(&line).map(|t| t.what)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_ssh_stream_left_by_a_cue_that_quit_is_ended() {
        let l = |s: &str| ps_line(s).unwrap();
        let (pid, ppid, cmd) = l("  4242     1 /usr/bin/ssh -o ControlMaster=auto -T box sh -c '# cue-watch exec x'");
        assert_eq!((pid, ppid), (4242, 1));
        assert!(left_stream(&cmd, "# cue-watch"));
        assert!(left_stream("/usr/bin/ssh -T box # cue-watch", "# cue-watch"));
        assert!(!left_stream("/bin/zsh -c grep '# cue-watch'", "# cue-watch"), "something that only mentions the mark");
        assert!(!left_stream("/usr/bin/ssh -T box tail -F x", "# cue-watch"), "another ssh");
        assert!(ps_line("garbage").is_none());
    }

    #[test]
    fn a_machine_that_cant_be_reached_says_so_not_that_its_pane_is_gone() {
        // ssh can't get there (no such name): exit 255, as when a login in front of the machine expired.
        let e = tmux("cue-test-nowhere.invalid", &["display-message", "-p", "-t", "%104", "#{pane_id}"]).unwrap_err();
        assert!(e.starts_with("Can't connect to cue-test-nowhere.invalid."), "{e}");
        assert!(!e.contains("gone"), "{e}");
        assert!(not_taken("ssh: connect to host x port 22: Connection timed out"));
        assert!(not_taken("mux_client_request_session: session request failed: Session open refused by peer"));
        assert!(!not_taken("can't find pane: %104"));
    }

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
        if !crate::terminal::focus::tmux_installed() {
            return;
        }
        let s = format!("cue-mtest-{}", std::process::id());
        let script = open_script(&s, "echo started from cue", "~", "my app");
        let out = std::process::Command::new("/bin/sh").args(["-c", &remote_command(&script)]).output().unwrap();
        let out = String::from_utf8_lossy(&out.stdout).to_string();
        std::thread::sleep(std::time::Duration::from_millis(900));
        let pane = out.lines().find_map(|l| l.strip_prefix("PANE:")).unwrap_or("").split('|').next().unwrap_or("").to_string();
        let screen = std::process::Command::new(crate::terminal::focus::tmux_bin()).args(["capture-pane", "-p", "-t", &pane]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let windows = std::process::Command::new(crate::terminal::focus::tmux_bin()).args(["list-windows", "-t", &s, "-F", "#{window_name}"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let _ = std::process::Command::new(crate::terminal::focus::tmux_bin()).args(["kill-session", "-t", &s]).status();
        assert!(pane.starts_with('%'), "it says which pane: {out:?}");
        assert_eq!(out.lines().find_map(|l| l.strip_prefix("DIR:")), Some(std::env::var("HOME").unwrap().as_str()), "~ is home there");
        assert!(screen.contains("started from cue"), "its line was typed and ran: {screen:?}");
        assert_eq!(windows.trim(), "my app");
        let bad = std::process::Command::new("/bin/sh").args(["-c", &remote_command(&open_script(&s, "x", "/no/such/dir", "n"))]).output().unwrap();
        assert!(String::from_utf8_lossy(&bad.stdout).contains("NODIR"));
    }

    #[test]
    fn a_session_whose_name_only_starts_the_same_is_never_used() {
        if !crate::terminal::focus::tmux_installed() {
            return;
        }
        let tmux = |args: &[&str]| std::process::Command::new(crate::terminal::focus::tmux_bin()).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        let s = format!("cue-ptest-{}", std::process::id());
        let other = format!("{s}-smoke");
        tmux(&["new-session", "-d", "-s", &other, "-n", "theirs", "sleep 30"]);
        let out = std::process::Command::new("/bin/sh").args(["-c", &remote_command(&open_script(&s, "true", "~", "mine"))]).output().unwrap();
        let theirs = tmux(&["list-windows", "-t", &format!("={other}"), "-F", "#{window_name}"]);
        let mine = tmux(&["list-windows", "-t", &format!("={s}"), "-F", "#{window_name}"]);
        tmux(&["kill-session", "-t", &format!("={s}")]);
        tmux(&["kill-session", "-t", &format!("={other}")]);
        assert!(String::from_utf8_lossy(&out.stdout).contains("PANE:"), "it opened");
        assert_eq!(theirs, "theirs", "nothing added to {other}");
        assert_eq!(mine, "mine", "a session of its own");
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

    #[test]
    fn the_machines_environment_becomes_exports_sh_takes_and_keeps_the_shells_own_out() {
        let raw = b"PATH=/home/u/.nvm/v20/bin:/usr/bin\0HOME=/home/u\0CLAUDE_CONFIG_DIR=/home/u/.claude\0SHLVL=7\0PWD=/home/u\0SSH_CONNECTION=1.2.3.4 5 6.7.8.9 22\0BASH_FUNC_x%%=() {  echo\n}\0PS1=\\u$ \0IT'S=a 'quoted' one\0";
        let pre = env_exports(raw).unwrap();
        assert_eq!(pre, "export PATH='/home/u/.nvm/v20/bin:/usr/bin'\nexport HOME='/home/u'\nexport CLAUDE_CONFIG_DIR='/home/u/.claude'\n");
        // Written to a file and sourced, as on the machine: the script sees that environment, from
        // /bin/sh and from bash alike. The shell counts its own SHLVL from ours (started at 0 here, so
        // the terminal running the tests doesn't matter); the file's 7 never reaches it.
        let file = std::env::temp_dir().join(format!("cue-env-{}.sh", std::process::id()));
        std::fs::write(&file, &pre).unwrap();
        for sh in ["/bin/sh", "/bin/bash"] {
            let out = std::process::Command::new(sh).env("SHLVL", "0").args(["-c", &format!(". {}\necho \"$PATH|$CLAUDE_CONFIG_DIR|${{SHLVL:-unset}}\"", shq(&file.to_string_lossy()))]).output().unwrap();
            let seen = String::from_utf8_lossy(&out.stdout).trim().to_string();
            assert!(seen.starts_with("/home/u/.nvm/v20/bin:/usr/bin|/home/u/.claude|") && !seen.ends_with("|7"), "{sh}: {seen}");
        }
        let _ = std::fs::remove_file(&file);
        assert!(env_exports(b"HOME=/home/u\0").is_none(), "no PATH: env -0 didn't work there");
    }

    #[test]
    fn what_bash_keeps_read_only_stays_out_so_sourcing_the_file_never_aborts() {
        let raw = b"PATH=/usr/bin\0UID=501\0EUID=501\0PPID=7\0SHELLOPTS=braceexpand\0BASHOPTS=checkwinsize\0BASH_VERSINFO=5\0IFS= \t\n\0KEEP=yes\0";
        let pre = env_exports(raw).unwrap();
        assert_eq!(pre, "export PATH='/usr/bin'\nexport KEEP='yes'\n");
        // Bash as /bin/sh runs the whole file and the line after it (an `export UID=` would end it there).
        let out = std::process::Command::new("/bin/bash").args(["-c", &format!("{pre}echo \"after:$KEEP\"")]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "after:yes");
    }

    #[test]
    fn what_ssh_runs_with_the_environment_file_sources_it_and_still_runs_without_it() {
        // The command `ssh_command` sends once the file is written, run here with HOME at a scratch
        // folder: with the file, the script sees what it exports; without it, the script still runs.
        let home = std::env::temp_dir().join(format!("cue-envhome-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue")).unwrap();
        let command = env_command("echo \"ran:${CUE_T:-none}\"");
        let run = || std::process::Command::new("/bin/sh").args(["-c", &command]).env("HOME", &home).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap();
        assert_eq!(run(), "ran:none", "no file yet: the script runs all the same");
        std::fs::write(home.join(ENV_FILE), "export CUE_T='set'\n").unwrap();
        assert_eq!(run(), "ran:set");
        let _ = std::fs::remove_dir_all(&home);
    }
}
