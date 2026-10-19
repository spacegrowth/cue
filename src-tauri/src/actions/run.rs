//! Running a project's actions. Each runs in a tmux session of its own, `cue-act-<project>-<name>-<id>`
//! (on this Mac, or on the session's machine over Cue's SSH), so Cue owns the process: quitting or
//! restarting Cue never ends a server it started, and the next Cue finds it again by its name. Its pane
//! runs your login shell in the project's folder (`-lic`, as a new terminal tab would, so your PATH and
//! version managers apply), through a script that hands it Cue's environment without the marks of
//! whatever terminal Cue was started from, then removes itself. When the command ends, the script writes
//! its exit code and end time on the session, and the pane stays (remain-on-exit) so its output can be
//! read. What Cue knows of it lives on the session (tmux options `@cue_*`): one `list-sessions` reads
//! every action's state.
//!
//! Without tmux on this Mac, an action is a plain child process of Cue's, its output in a file in Cue's
//! folder; it ends when Cue quits, and its state says so (`noTmux`).

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// The start of every action's tmux session's name.
pub const PREFIX: &str = "cue-act-";

/// What starting one takes.
pub struct Spec<'a> {
    pub folder: &'a str,
    pub machine: &'a str,
    pub id: &'a str,
    pub name: &'a str,
    pub run: &'a str,
    pub keep: bool,
}

/// A short, stable hash (FNV-1a): the same on every Mac and in every version of Cue.
pub fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

/// The part of a session's name that says which project it's for.
pub fn project_tag(folder: &str, machine: &str) -> String {
    format!("{PREFIX}{:08x}-", fnv(&format!("{machine}|{folder}")) as u32)
}

/// The action's name as tmux takes it: lower case letters, digits and `-`.
fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s: String = s.trim_end_matches('-').chars().take(24).collect();
    if s.is_empty() { "action".into() } else { s.trim_end_matches('-').to_string() }
}

/// Action `id`'s tmux session: its project, its name, and a bit of its id (two names can slug alike).
pub fn session_name(folder: &str, machine: &str, id: &str, name: &str) -> String {
    format!("{}{}-{:04x}", project_tag(folder, machine), slug(name), fnv(id) & 0xffff)
}

/// A value kept on a session: "%", "|" and line breaks spelled out, so a listing can be split on "|".
fn enc(s: &str) -> String {
    s.replace('%', "%25").replace('|', "%7C").replace('\n', "%0A").replace('\r', "%0D")
}
fn dec(s: &str) -> String {
    s.replace("%7C", "|").replace("%0A", "\n").replace("%0D", "\r").replace("%25", "%")
}

/// One action's session as tmux lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
    pub session: String,
    pub running: bool,
    /// When Cue started it (ms, this Mac's clock).
    pub start_ms: Option<u64>,
    /// When its command started and ended (s, the clock where it runs).
    pub t0: Option<u64>,
    pub end: Option<u64>,
    pub exit: Option<i32>,
    pub port: Option<u16>,
    pub stopped: bool,
    pub keep: bool,
    pub machine: String,
    pub folder: String,
    pub id: String,
    pub name: String,
    pub no_tmux: bool,
}

impl Seen {
    /// Its state for the window: {running, since, port, exit, secs, at, noTmux, stopped}.
    pub fn state(&self) -> Value {
        let mut s = json!({ "running": self.running });
        if self.running {
            if let Some(t) = self.start_ms {
                s["since"] = json!(t);
                s["secs"] = json!(crate::model::now_ms().saturating_sub(t) / 1000);
            }
            if let Some(p) = self.port {
                s["port"] = json!(p);
            }
        } else {
            s["exit"] = json!(self.exit);
            if let (Some(a), Some(b)) = (self.t0, self.end) {
                s["secs"] = json!(b.saturating_sub(a));
            }
            if let Some(e) = self.end {
                s["at"] = json!(e * 1000);
            }
            if self.stopped {
                s["stopped"] = json!(true);
            }
        }
        if self.no_tmux {
            s["noTmux"] = json!(true);
        }
        s
    }
}

/// What `list` asks tmux of each session (its options as `enc` keeps them: no "|" in any).
const FORMAT: &str = "#{session_name}|#{pane_dead}|#{pane_dead_status}|#{@cue_start}|#{@cue_t0}|#{@cue_end}|#{@cue_exit}|#{@cue_port}|#{@cue_stopped}|#{@cue_keep}|#{@cue_machine}|#{@cue_folder}|#{@cue_id}|#{@cue_name}";

/// Actions' sessions in `tmux list-sessions -F FORMAT` lines.
pub fn parse_listing(out: &str) -> Vec<Seen> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('|').collect();
            if f.len() != 14 || !f[0].starts_with(PREFIX) {
                return None;
            }
            let dead = f[1] == "1";
            let exit = f[6].parse().ok().or_else(|| if dead { f[2].parse().ok() } else { None });
            Some(Seen {
                session: f[0].into(),
                running: !dead,
                start_ms: f[3].parse().ok(),
                t0: f[4].parse().ok(),
                end: f[5].parse().ok(),
                exit,
                port: f[7].parse().ok(),
                stopped: f[8] == "1",
                keep: f[9] == "1",
                machine: dec(f[10]),
                folder: dec(f[11]),
                id: dec(f[12]),
                name: dec(f[13]),
                no_tmux: false,
            })
        })
        .collect()
}

/// tmux on this Mac: its output, or what it said.
fn tmux_here(args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new(crate::terminal::focus::tmux_bin()).args(args).stdin(std::process::Stdio::null()).output().map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

/// The SSH host of `machine` ("" is this Mac: None).
fn host_of(machine: &str) -> Result<Option<String>, String> {
    if machine.is_empty() {
        return Ok(None);
    }
    crate::machines::get(machine).map(|m| Some(m.host)).ok_or_else(|| format!("{machine} isn't one of your machines any more (+ New → Machine)"))
}

/// tmux where the project is: here, or on its machine.
fn tmux_at(machine: &str, args: &[&str]) -> Result<String, String> {
    match host_of(machine)? {
        None => tmux_here(args),
        Some(h) => crate::machines::tmux(&h, args),
    }
}

/// Every action's session on `machine` ("" this Mac; with the plain ones there when there's no tmux).
pub fn list(machine: &str) -> Vec<Seen> {
    let mut out = if machine.is_empty() && !crate::terminal::focus::tmux_installed() { vec![] } else { tmux_at(machine, &["list-sessions", "-F", FORMAT]).map(|o| parse_listing(&o)).unwrap_or_default() };
    if machine.is_empty() {
        out.extend(plain_list());
    }
    out
}

/// The command run in the folder by your login shell, from the start of its line: into the folder first
/// (your setup may `cd` elsewhere as it loads).
fn in_folder(dir_word: &str, run: &str) -> String {
    format!("cd -- {dir_word} || exit 1\n{run}")
}

/// Marks of the terminal Cue may have been started from (as for Cue's shells), and the shell's level.
fn not_here(k: &str) -> bool {
    crate::terminal::shells::NOT_HERE.contains(&k) || k == "SHLVL"
}

/// The script an action's pane runs on this Mac: Cue's environment (cleaned), your shell running the
/// command in the folder, then its exit code and end time on the session. It removes itself first (the
/// environment can hold secrets). Ctrl-C (Stop) ends the command, not the script, so the end is written.
fn launcher(tmux: &str, session: &str, shell: &str, dir: &str, run: &str) -> String {
    let q = crate::terminal::focus::shq;
    let mut s = format!("#!/bin/sh\n# An action of Cue's, in tmux. Written by Cue; it removes itself.\nrm -f -- \"$0\"\ntrap : INT\nT={}; S={}\n\"$T\" set-option -t \"$S\" @cue_t0 \"$(date +%s)\" 2>/dev/null\n/usr/bin/env -i TERM=xterm-256color COLORTERM=truecolor SHELL_SESSIONS_DISABLE=1", q(tmux), q(&format!("={session}:")));
    for (k, v) in std::env::vars().filter(|(k, _)| !not_here(k) && !matches!(k.as_str(), "TERM" | "COLORTERM" | "SHELL_SESSIONS_DISABLE")) {
        s.push(' ');
        s.push_str(&q(&format!("{k}={v}")));
    }
    s.push_str(&format!(" {} -lic {}\n", q(shell), q(&in_folder(&q(dir), run))));
    s.push_str(&end_lines());
    s
}

/// The end of a launcher: the command's exit code and the time, on the session.
fn end_lines() -> String {
    "c=$?\n\"$T\" set-option -t \"$S\" @cue_exit \"$c\" \\; set-option -t \"$S\" @cue_end \"$(date +%s)\" 2>/dev/null\nexit $c\n".into()
}

/// The same on a machine, as one command for tmux there: its own environment, your shell there.
fn remote_launcher(tmux: &str, session: &str, dir: &str, run: &str) -> String {
    let q = crate::terminal::focus::shq;
    let script = format!(
        "trap : INT\nT={}; S={}\n\"$T\" set-option -t \"$S\" @cue_t0 \"$(date +%s)\" 2>/dev/null\n\"${{SHELL:-/bin/sh}}\" -lic {}\n{}",
        q(tmux),
        q(&format!("={session}:")),
        q(&in_folder(&crate::machines::remote_dir(dir), run)),
        end_lines()
    );
    format!("exec /bin/sh -c {}", q(&script))
}

fn scripts_dir() -> PathBuf {
    crate::server::cue_dir().join("actions")
}

/// Start action `s` (again, when its session is there: a fresh run in the same session, its old output
/// cleared). A running one is stopped first (`stop`), so a server's port is free again.
pub fn start(s: &Spec) -> Result<Seen, String> {
    let name = session_name(s.folder, s.machine, s.id, s.name);
    if s.machine.is_empty() && !crate::terminal::focus::tmux_installed() {
        return plain_start(s, &name);
    }
    let host = host_of(s.machine)?;
    if host.is_none() && !std::path::Path::new(s.folder).is_dir() {
        return Err(format!("{} isn't a folder", s.folder));
    }
    let target = format!("={name}:");
    let there = pane(s.machine, &name, "#{pane_dead}");
    if there.as_deref() == Some("0") {
        stop(s.folder, s.machine, &name)?;
    }
    let cmd = match &host {
        None => {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let dir = scripts_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let script = dir.join(format!("run-{name}.sh"));
            let shell = std::env::var("SHELL").ok().filter(|s| s.starts_with('/')).unwrap_or_else(|| "/bin/zsh".into());
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&script)
                .and_then(|mut f| f.write_all(launcher(&crate::terminal::focus::tmux_bin(), &name, &shell, s.folder, s.run).as_bytes()))
                .map_err(|e| e.to_string())?;
            format!("exec /bin/sh {}", crate::terminal::focus::shq(&script.to_string_lossy()))
        }
        Some(h) => remote_launcher(&crate::machines::tmux_path(h).unwrap_or_else(|| "tmux".into()), &name, s.folder, s.run),
    };
    let start_ms = crate::model::now_ms().to_string();
    let keep = if s.keep { "1" } else { "0" };
    let (machine, folder, id, label) = (enc(s.machine), enc(s.folder), enc(s.id), enc(s.name));
    let mut args: Vec<&str> = vec![];
    if there.is_some() {
        // Its last run's marks cleared, its output too; then the pane runs again.
        for k in ["@cue_exit", "@cue_end", "@cue_t0", "@cue_port", "@cue_stopped"] {
            args.extend(["set-option", "-t", &target, k, "", ";"]);
        }
        args.extend(["clear-history", "-t", &target, ";", "respawn-pane", "-k", "-t", &target, &cmd]);
    } else {
        args.extend(["new-session", "-d", "-s", &name, "-x", "200", "-y", "50"]);
        if host.is_none() {
            args.extend(["-c", s.folder]);
        }
        args.push(&cmd);
        for (k, v) in [("status", "off"), ("prefix", "None"), ("mouse", "on"), ("destroy-unattached", "off"), ("detach-on-destroy", "on")] {
            args.extend([";", "set-option", "-t", &target, k, v]);
        }
        args.extend([";", "set-option", "-w", "-t", &target, "remain-on-exit", "on"]);
    }
    for (k, v) in [("@cue_start", start_ms.as_str()), ("@cue_keep", keep), ("@cue_machine", &machine), ("@cue_folder", &folder), ("@cue_id", &id), ("@cue_name", &label)] {
        args.extend([";", "set-option", "-t", &target, k, v]);
    }
    tmux_at(s.machine, &args).map_err(|e| if e.is_empty() { "tmux didn't start it".to_string() } else { e })?;
    Ok(Seen { session: name, running: true, start_ms: start_ms.parse().ok(), keep: s.keep, machine: s.machine.into(), folder: s.folder.into(), id: s.id.into(), name: s.name.into(), ..Default::default() })
}

/// `format` of session `name`'s pane; None when there's no such session (display-message would answer
/// for another).
fn pane(machine: &str, name: &str, format: &str) -> Option<String> {
    tmux_at(machine, &["list-panes", "-t", &format!("={name}:"), "-F", format]).ok().and_then(|o| o.lines().next().map(|l| l.trim().to_string()))
}

/// Whether session `name` is still running its command.
fn alive(machine: &str, name: &str) -> bool {
    pane(machine, name, "#{pane_dead}").as_deref() == Some("0")
}

/// A shell line that kills every process under `pid` (not `pid` itself: the launcher then writes the end).
fn kill_under(pid: &str) -> String {
    format!("ps -A -o pid= -o ppid= | awk -v root={pid} '{{p[$1]=$2}} END {{for (x in p) {{y=x; while ((y in p) && y != root && y > 1) y=p[y]; if (y == root && x != root) print x}}}}' | xargs kill -KILL 2>/dev/null; true")
}

/// Stop session `name`: Ctrl-C, as you'd press it; what still runs 3 s later is killed. Its session and
/// output stay, for its log.
pub fn stop(_folder: &str, machine: &str, name: &str) -> Result<(), String> {
    if plain_stop(name) {
        return Ok(());
    }
    let target = format!("={name}:");
    if !alive(machine, name) {
        return Ok(());
    }
    let _ = tmux_at(machine, &["set-option", "-t", &target, "@cue_stopped", "1"]);
    tmux_at(machine, &["send-keys", "-t", &target, "C-c"])?;
    let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(150));
        if !alive(machine, name) {
            return Ok(());
        }
    }
    let pid = pane(machine, name, "#{pane_pid}").unwrap_or_default();
    if !pid.chars().all(|c| c.is_ascii_digit()) || pid.is_empty() {
        return Ok(());
    }
    match host_of(machine)? {
        None => {
            let _ = std::process::Command::new("/bin/sh").args(["-c", &kill_under(&pid)]).status();
        }
        Some(h) => {
            let _ = crate::machines::run(&h, &kill_under(&pid));
        }
    }
    let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < end && alive(machine, name) {
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    Ok(())
}

/// The last `lines` lines of its output, plain text (no colors or cursor moves), tmux's own "Pane is
/// dead" line left out.
pub fn log(machine: &str, name: &str, lines: usize) -> Result<String, String> {
    let raw = match plain_log(name) {
        Some(t) => t,
        None => tmux_at(machine, &["capture-pane", "-p", "-J", "-S", "-", "-t", &format!("={name}:")]).map_err(|_| "It hasn't run yet".to_string())?,
    };
    Ok(last_lines(&strip_ansi(&raw), lines))
}

/// The last `n` lines of `text`, blank ones at the end and tmux's own line dropped.
pub fn last_lines(text: &str, n: usize) -> String {
    let mut l: Vec<&str> = text.lines().filter(|l| !l.starts_with("Pane is dead (")).collect();
    while l.last().is_some_and(|x| x.trim().is_empty()) {
        l.pop();
    }
    l[l.len().saturating_sub(n.max(1))..].join("\n")
}

/// Text as a terminal would leave it: escape sequences (colors, cursor moves, titles) taken out, and a
/// line rewritten with "\r" (a progress bar) only as it was last drawn.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\x1b' {
            if c != '\x07' && (c == '\n' || c == '\t' || c == '\r' || !c.is_control()) {
                out.push(c);
            }
            continue;
        }
        match it.next() {
            // CSI: parameters, then one final byte.
            Some('[') => {
                for d in it.by_ref() {
                    if ('\x40'..='\x7e').contains(&d) {
                        break;
                    }
                }
            }
            // OSC and the like: up to BEL or ESC \.
            Some(']') | Some('P') | Some('_') | Some('^') => {
                while let Some(d) = it.next() {
                    if d == '\x07' {
                        break;
                    }
                    if d == '\x1b' && it.peek() == Some(&'\\') {
                        it.next();
                        break;
                    }
                }
            }
            // A character-set choice takes one more.
            Some('(') | Some(')') => {
                it.next();
            }
            _ => {}
        }
    }
    out.split('\n').map(|l| l.trim_end_matches('\r').rsplit('\r').next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

/// The port a server says it listens on: the first `http(s)://localhost:N` (or 127.0.0.1, 0.0.0.0, [::])
/// in its output, or `listening on …:N`.
pub fn port_in(text: &str) -> Option<u16> {
    let digits = |s: &str| -> Option<u16> {
        let d: String = s.chars().take_while(char::is_ascii_digit).collect();
        d.parse().ok().filter(|p| *p > 0)
    };
    for line in text.lines() {
        let low = line.to_ascii_lowercase();
        for scheme in ["http://", "https://"] {
            let mut from = 0;
            while let Some(i) = low[from..].find(scheme) {
                let rest = &low[from + i + scheme.len()..];
                for host in ["localhost:", "127.0.0.1:", "0.0.0.0:", "[::]:", "[::1]:"] {
                    if let Some(p) = rest.strip_prefix(host).and_then(digits) {
                        return Some(p);
                    }
                }
                from += i + scheme.len();
            }
        }
        if let Some(i) = low.find("listening on ") {
            let rest = &low[i..];
            if let Some(p) = rest.rfind(':').and_then(|c| digits(&rest[c + 1..])) {
                return Some(p);
            }
        }
    }
    None
}

/// A running action's port, read from its output once it prints one, and kept on its session.
pub fn find_port(machine: &str, name: &str) -> Option<u16> {
    let out = match plain_log(name) {
        Some(t) => t,
        None => tmux_at(machine, &["capture-pane", "-p", "-J", "-S", "-300", "-t", &format!("={name}:")]).ok()?,
    };
    let p = port_in(&strip_ansi(&out))?;
    if plain_set_port(name, p) {
        return Some(p);
    }
    let _ = tmux_at(machine, &["set-option", "-t", &format!("={name}:"), "@cue_port", &p.to_string()]);
    Some(p)
}

// ---------- without tmux: a child of Cue's ----------

struct Plain {
    child: std::process::Child,
    seen: Seen,
    out: PathBuf,
}

static PLAIN: Mutex<Option<HashMap<String, Plain>>> = Mutex::new(None);

fn plain_out(name: &str) -> PathBuf {
    scripts_dir().join("out").join(format!("{name}.log"))
}

/// Run it as a child of Cue's, its output in a file (ended when Cue quits: `end_plain`).
fn plain_start(s: &Spec, name: &str) -> Result<Seen, String> {
    use std::os::unix::process::CommandExt;
    if !std::path::Path::new(s.folder).is_dir() {
        return Err(format!("{} isn't a folder", s.folder));
    }
    let _ = plain_stop(name);
    let out = plain_out(name);
    std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
    let f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let shell = std::env::var("SHELL").ok().filter(|s| s.starts_with('/')).unwrap_or_else(|| "/bin/zsh".into());
    let mut c = std::process::Command::new(shell);
    c.args(["-l", "-c", &in_folder(&crate::terminal::focus::shq(s.folder), s.run)]).current_dir(s.folder).stdin(std::process::Stdio::null()).stdout(f.try_clone().map_err(|e| e.to_string())?).stderr(f).process_group(0);
    for (k, _) in std::env::vars().filter(|(k, _)| not_here(k)) {
        c.env_remove(k);
    }
    c.env("SHELL_SESSIONS_DISABLE", "1");
    let child = c.spawn().map_err(|e| e.to_string())?;
    let seen = Seen { session: name.into(), running: true, start_ms: Some(crate::model::now_ms()), t0: Some(crate::model::now_ms() / 1000), keep: s.keep, machine: String::new(), folder: s.folder.into(), id: s.id.into(), name: s.name.into(), no_tmux: true, ..Default::default() };
    let mut all = crate::locking::lock(&PLAIN, "PLAIN");
    all.get_or_insert_with(HashMap::new).insert(name.into(), Plain { child, seen: seen.clone(), out });
    Ok(seen)
}

/// The plain ones, each looked at (an ended one gets its exit code and end time).
fn plain_list() -> Vec<Seen> {
    let mut all = crate::locking::lock(&PLAIN, "PLAIN");
    let Some(map) = all.as_mut() else { return vec![] };
    let mut out = vec![];
    for p in map.values_mut() {
        if p.seen.running {
            if let Ok(Some(st)) = p.child.try_wait() {
                use std::os::unix::process::ExitStatusExt;
                p.seen.running = false;
                p.seen.exit = st.code().or_else(|| st.signal().map(|s| 128 + s));
                p.seen.end = Some(crate::model::now_ms() / 1000);
            }
        }
        out.push(p.seen.clone());
    }
    out
}

/// Stop a plain one (Ctrl-C to its group, then kill after 3 s). False when it isn't a plain one.
fn plain_stop(name: &str) -> bool {
    let pid = {
        let mut all = crate::locking::lock(&PLAIN, "PLAIN");
        match all.as_mut().and_then(|m| m.get_mut(name)) {
            Some(p) => {
                p.seen.stopped = true;
                p.child.id() as i32
            }
            None => return false,
        }
    };
    unsafe { libc::kill(-pid, libc::SIGINT) };
    let end = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if plain_list().iter().any(|s| s.session == name && !s.running) {
            return true;
        }
    }
    unsafe { libc::kill(-pid, libc::SIGKILL) };
    std::thread::sleep(std::time::Duration::from_millis(200));
    let _ = plain_list();
    true
}

fn plain_log(name: &str) -> Option<String> {
    let path = {
        let all = crate::locking::lock(&PLAIN, "PLAIN");
        all.as_ref()?.get(name)?.out.clone()
    };
    let b = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&b[b.len().saturating_sub(512 * 1024)..]).to_string())
}

fn plain_set_port(name: &str, port: u16) -> bool {
    let mut all = crate::locking::lock(&PLAIN, "PLAIN");
    match all.as_mut().and_then(|m| m.get_mut(name)) {
        Some(p) => {
            p.seen.port = Some(port);
            true
        }
        None => false,
    }
}

/// Cue is quitting: its plain actions end with it (the ones in tmux carry on).
pub fn end_plain() {
    let mut all = crate::locking::lock(&PLAIN, "PLAIN");
    for p in all.as_mut().map(|m| m.values_mut()).into_iter().flatten() {
        unsafe { libc::kill(-(p.child.id() as i32), libc::SIGTERM) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_servers_port_is_read_from_what_it_prints() {
        assert_eq!(port_in("  VITE v5  ready\n  ➜  Local:   http://localhost:5173/\n"), Some(5173));
        assert_eq!(port_in("Server running at https://127.0.0.1:8443"), Some(8443));
        assert_eq!(port_in("Uvicorn running on http://0.0.0.0:8000 (Press CTRL+C to quit)"), Some(8000));
        assert_eq!(port_in("serving on http://[::]:3000"), Some(3000));
        assert_eq!(port_in("Listening on 0.0.0.0:4000"), Some(4000));
        assert_eq!(port_in("API listening on port :9090"), Some(9090));
        assert_eq!(port_in("see https://example.com:443/docs\nthen http://localhost:3001"), Some(3001), "only this Mac's addresses");
        assert_eq!(port_in("compiled in 300ms"), None);
        assert_eq!(port_in("http://localhost:/x"), None);
        assert_eq!(port_in(&strip_ansi("\x1b[32m➜\x1b[39m  Local: \x1b[36mhttp://localhost:\x1b[1m5174\x1b[22m/\x1b[39m")), Some(5174));
    }

    #[test]
    fn colors_cursor_moves_and_titles_are_taken_out_of_a_log() {
        assert_eq!(strip_ansi("\x1b[1;31merror\x1b[0m: x"), "error: x");
        assert_eq!(strip_ansi("\x1b]0;title\x07after"), "after");
        assert_eq!(strip_ansi("\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\"), "link");
        assert_eq!(strip_ansi("10%\r50%\r100%\ndone\r\n"), "100%\ndone\n");
        assert_eq!(strip_ansi("\x1b(Bplain\x1b[?25l"), "plain");
        assert_eq!(last_lines("a\nb\nc\n\nPane is dead (status 3, now)\n\n", 2), "b\nc");
    }

    #[test]
    fn session_names_are_tmux_safe_and_kept_values_come_back() {
        let n = session_name("/p/web app", "", "shared:Dev API", "Dev API");
        assert!(n.starts_with(&project_tag("/p/web app", "")) && n.contains("-dev-api-"), "{n}");
        assert!(n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'), "{n}");
        assert_ne!(n, session_name("/p/web app", "", "local:Dev API", "Dev API"), "two sources, two sessions");
        assert_ne!(project_tag("/p/x", ""), project_tag("/p/x", "box"), "a machine's folder is another project");
        assert_eq!(slug("!!!"), "action");
        for s in ["a|b", "100%|x\ny", "%7C"] {
            assert_eq!(dec(&enc(s)), s);
            assert!(!enc(s).contains('|'));
        }
        let line = format!("{n}|1|3|1000|10|12||5173||1||%2Fp|shared:Dev API|Dev API\nnot-ours|0||||||||||||\n");
        let seen = parse_listing(&line);
        assert_eq!(seen.len(), 1);
        assert_eq!((seen[0].running, seen[0].exit, seen[0].port, seen[0].keep), (false, Some(3), Some(5173), true), "tmux's own exit status when the script couldn't write one");
        let st = seen[0].state();
        assert_eq!((st["secs"].as_u64(), st["at"].as_u64()), (Some(2), Some(12000)));
    }

    /// `ok` within 15 s.
    fn until(mut ok: impl FnMut() -> bool) -> bool {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < end {
            if ok() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        ok()
    }

    #[test]
    fn without_tmux_an_action_is_a_child_of_cues_with_its_output_in_a_file() {
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join(format!("cue-act-plain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("CUE_HOME", &home);
        let folder = home.join("proj");
        std::fs::create_dir_all(&folder).unwrap();
        let f = folder.to_string_lossy().to_string();
        let name = format!("cue-act-plaintest-{}", std::process::id());
        let s = Spec { folder: &f, machine: "", id: "shared:Once", name: "Once", run: "echo hi; pwd; exit 3", keep: false };
        let started = plain_start(&s, &name).unwrap();
        assert!(started.running && started.state()["noTmux"] == true);
        let ended = until(|| plain_list().iter().any(|x| x.session == name && !x.running));
        let seen = plain_list().into_iter().find(|x| x.session == name).unwrap();
        let out = plain_log(&name).unwrap_or_default();
        let k = Spec { folder: &f, machine: "", id: "shared:Wait", name: "Wait", run: "sleep 30", keep: true };
        let kname = format!("{name}-k");
        plain_start(&k, &kname).unwrap();
        assert!(plain_stop(&kname));
        let stopped = plain_list().into_iter().find(|x| x.session == kname).unwrap();
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&home);
        assert!(ended, "it ended");
        assert_eq!(seen.exit, Some(3));
        assert!(out.contains("hi") && out.contains("proj"), "in its folder: {out}");
        assert!(!stopped.running && stopped.stopped, "stopped");
    }
}
