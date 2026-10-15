//! Cue's own shells: the floating terminal's tabs. Each is your login shell (the one a new terminal tab
//! opens, your profile loaded) in a folder you pick, in a pseudo-terminal of Cue's own, streamed to the
//! window as the built-in terminal is (term.rs: the same "term" events, written to, resized and closed
//! by its id). It starts in its folder even when your setup `cd`s elsewhere as it loads. Whether each is
//! running a command (and which) is looked at every 0.8 s and sent as "shell" events when it changes,
//! for the live dot.
//!
//! With tmux installed, each runs in a tmux session of its own ("cue-shell-" and its tab's name), and
//! the tab is a tmux client attached to it: quitting or restarting Cue ends only the client, so the
//! shell, what runs in it and what it shows carry on, and the next Cue puts each back in its own tab
//! (`list`), its name with it (kept on the session). Closing the tab or `exit` ends the session. Without
//! tmux, a shell is a pseudo-terminal of Cue's own and lives until it's closed, `exit`ed, or Cue quits.
use portable_pty::CommandBuilder;
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};

/// Every shell's id starts with this (the window picks the rest), so it's never a session's.
pub const PREFIX: &str = "shell:";

/// Marks of the terminal Cue may have been started from: a shell of Cue's own isn't in it (an agent
/// started there would report the wrong terminal to Cue).
const NOT_HERE: &[&str] = &["TERM_PROGRAM", "TERM_PROGRAM_VERSION", "TERM_SESSION_ID", "TMUX", "TMUX_PANE", "ITERM_SESSION_ID", "ITERM_PROFILE", "KITTY_WINDOW_ID", "WEZTERM_PANE", "VSCODE_PID"];

/// A shell's id: the prefix, then a short name of letters, digits, `-` and `_`.
fn id_ok(id: &str) -> bool {
    id.strip_prefix(PREFIX).is_some_and(|n| !n.is_empty() && n.len() <= 48 && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
}

/// Where it starts: `cwd` when that's a folder, else your home.
fn folder(cwd: &str) -> String {
    let cwd = cwd.trim();
    if !cwd.is_empty() && std::path::Path::new(cwd).is_dir() {
        return cwd.to_string();
    }
    std::env::var("HOME").unwrap_or_else(|_| "/".into())
}

/// Your login shell in `dir`, as a terminal of its own.
fn command(dir: &str) -> CommandBuilder {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    command_for(&shell, dir, std::env::var("ZDOTDIR").ok(), &crate::server::cue_dir().join("shell"))
}

/// `shell` in `dir`, its own setup loaded as in any new terminal tab, and only then into `dir`: a
/// profile or rc file that `cd`s somewhere of its own as it loads (many do) would otherwise leave every
/// shell there. zsh reads Cue's startup files (in `wrappers`, through ZDOTDIR), each loading yours
/// from where they are (`zdotdir`, else ~), the last going into the folder; bash, an rc file that loads
/// your profile then does the same; fish, a command run after its config. Any other shell: just `dir`.
fn command_for(shell: &str, dir: &str, zdotdir: Option<String>, wrappers: &std::path::Path) -> CommandBuilder {
    let mut c = CommandBuilder::new(shell);
    let name = shell.rsplit('/').next().unwrap_or(shell);
    let wrote = |files: &[(&str, String)]| -> bool { std::fs::create_dir_all(wrappers).is_ok() && files.iter().all(|(f, body)| std::fs::write(wrappers.join(f), body).is_ok()) };
    match name {
        "zsh" if wrote(&ZSH.iter().map(|f| (*f, zsh_wrapper(f))).collect::<Vec<_>>()) => {
            c.arg("-l");
            c.env("ZDOTDIR", wrappers);
            c.env("CUE_HOME_ZDOTDIR", zdotdir.unwrap_or_default());
        }
        "bash" if wrote(&[("bashrc", BASHRC.to_string())]) => {
            c.args(["--rcfile", &wrappers.join("bashrc").to_string_lossy(), "-i"]);
        }
        "fish" => {
            c.args(["-l", "-C", "cd $CUE_CWD; set -e CUE_CWD"]);
        }
        _ => {
            c.arg("-l");
        }
    }
    c.env("CUE_CWD", dir);
    c.cwd(dir);
    for k in NOT_HERE {
        c.env_remove(k);
    }
    // A shell of its own, not one nested in whatever started Cue (whose level it would count up from,
    // and a prompt that shows the level would say so): level 1, as a new terminal tab's.
    c.env_remove("SHLVL");
    c.env("TERM", "xterm-256color");
    c.env("COLORTERM", "truecolor");
    // Not a Terminal.app session (its "Restored session…" lines, when Cue was started from one).
    c.env("SHELL_SESSIONS_DISABLE", "1");
    c
}

/// zsh's startup files, in the order a login shell reads them.
const ZSH: [&str; 4] = [".zshenv", ".zprofile", ".zshrc", ".zlogin"];

/// Cue's `file` for zsh: yours, read from your ZDOTDIR (or ~) as zsh would, then back to Cue's for the
/// next. Yours may set ZDOTDIR (in .zshenv): the rest are read from there. The last puts ZDOTDIR back
/// as you had it, goes into the folder, and keeps your prompt to one line: a theme's prompt of several
/// (a blank line, the folder on one, `%` alone on the next) is folded into one before each prompt, as
/// the floating terminal is short, and starts with the host when it doesn't show it already (many show
/// it only over SSH). Only in Cue's shells; your other terminals keep it as it is.
fn zsh_wrapper(file: &str) -> String {
    let mut s = format!("# Cue's shells: your {file}, as ever (from ZDOTDIR or ~). Written by Cue; changes are lost.\n_cue_z=\"$ZDOTDIR\"; ZDOTDIR=\"${{CUE_HOME_ZDOTDIR:-$HOME}}\"\n[[ -r \"$ZDOTDIR/{file}\" ]] && builtin source \"$ZDOTDIR/{file}\"\n");
    if file == ".zshenv" {
        s.push_str("[[ \"$ZDOTDIR\" != \"${CUE_HOME_ZDOTDIR:-$HOME}\" ]] && CUE_HOME_ZDOTDIR=\"$ZDOTDIR\"\n");
    }
    if file == ".zlogin" {
        s.push_str("if [[ -n \"$CUE_HOME_ZDOTDIR\" ]]; then ZDOTDIR=\"$CUE_HOME_ZDOTDIR\"; else unset ZDOTDIR; fi\nunset _cue_z CUE_HOME_ZDOTDIR\n[[ -n \"$CUE_CWD\" ]] && builtin cd -- \"$CUE_CWD\" 2>/dev/null; unset CUE_CWD\n");
        s.push_str("_cue_one_line() { while [[ $PS1 == $'\\n'* ]]; do PS1=${PS1#$'\\n'}; done; PS1=${PS1//$'\\n'/ }; print -v _cue_p -P -- \"$PS1\"; [[ $_cue_p == *${HOST%%.*}* ]] || PS1=\"%B%F{green}%m%f%b $PS1\"; unset _cue_p; }\nautoload -Uz add-zsh-hook && add-zsh-hook precmd _cue_one_line\n");
    } else {
        s.push_str("ZDOTDIR=\"$_cue_z\"; unset _cue_z\n");
    }
    s
}

/// bash: your login setup (as a new terminal tab has it), then into the folder.
const BASHRC: &str = "# Cue's shells: your profile, as ever. Written by Cue; changes are lost.\n[ -r /etc/profile ] && . /etc/profile\nfor f in ~/.bash_profile ~/.bash_login ~/.profile; do [ -r \"$f\" ] && { . \"$f\"; break; }; done\n[ -n \"$CUE_CWD\" ] && builtin cd -- \"$CUE_CWD\" 2>/dev/null; unset CUE_CWD\n";

/// The start of every shell's tmux session's name.
const SESSION: &str = "cue-shell-";

/// Shell `id`'s tmux session: "shell:3a" is "cue-shell-3a".
fn session_of(id: &str) -> String {
    format!("{SESSION}{}", id.strip_prefix(PREFIX).unwrap_or(id))
}

/// The shells in tmux this launch of Cue knows of (opened, or found by `list`): Cue's quitting doesn't end them.
static KEPT: std::sync::Mutex<BTreeSet<String>> = std::sync::Mutex::new(BTreeSet::new());

fn kept(id: &str) -> bool {
    crate::locking::lock(&KEPT, "KEPT").contains(id)
}

/// tmux, its output when it worked.
fn tmux(args: &[&str]) -> Option<String> {
    let o = std::process::Command::new(crate::terminal::focus::tmux_bin()).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).to_string())
}

/// Open shell `id` in `cwd` at `cols`×`rows`: in tmux, its session (started, or carried on from before
/// and attached again), else a pseudo-terminal of Cue's own. Its output arrives as "term" events (as a
/// session's terminal's do); what it runs, as "shell" events ({id, busy, what}). Its output's
/// generation, and whether it's in tmux (Cue's quitting doesn't end it).
pub fn open(app: AppHandle, id: &str, cwd: &str, cols: u16, rows: u16) -> Result<(u64, bool), String> {
    let a = app.clone();
    let opened = open_with(id, cwd, cols, rows, move |v| {
        let _ = a.emit("term", v);
    })?;
    watch(app);
    Ok(opened)
}

/// Open shell `id` in `cwd` as a pseudo-terminal of Cue's own, never in tmux: for a shell whose screen is
/// read from its output (an add-on's), where tmux's redrawing would keep the scrollback to itself. It ends
/// when it's closed, or when Cue quits. Its output's generation.
pub fn open_plain(app: AppHandle, id: &str, cwd: &str, cols: u16, rows: u16) -> Result<u64, String> {
    if !id_ok(id) {
        return Err(format!("“{id}” isn't a shell's id"));
    }
    let a = app.clone();
    let g = crate::terminal::term::spawn(id, command(&folder(cwd)), cols, rows, move |v| {
        let _ = a.emit("term", v);
    })?;
    watch(app);
    Ok(g)
}

fn open_with(id: &str, cwd: &str, cols: u16, rows: u16, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<(u64, bool), String> {
    if !id_ok(id) {
        return Err(format!("“{id}” isn't a shell's id"));
    }
    if !crate::terminal::focus::tmux_installed() {
        return crate::terminal::term::spawn(id, command(&folder(cwd)), cols, rows, out).map(|g| (g, false));
    }
    let name = session_of(id);
    if tmux(&["has-session", "-t", &format!("={name}")]).is_none() {
        start(id, &name, &folder(cwd), cols, rows)?;
    }
    crate::locking::lock(&KEPT, "KEPT").insert(id.to_string());
    crate::terminal::term::attach_local(id, &name, cols, rows, out).map(|g| (g, true))
}

/// Shell `id`'s tmux session `name`, started in `dir`: your shell as Cue starts one (`command`), through
/// a script that hands it Cue's environment for it and tmux's own terminal (TERM, TMUX, TMUX_PANE), then
/// removes itself (the environment can hold secrets). Set up as a tab of Cue's: no status bar, no prefix
/// key (every key reaches the shell), the mouse on (the wheel scrolls back), its title passed on to the
/// tab, and never ended by your tmux settings when no client is attached.
fn start(id: &str, name: &str, dir: &str, cols: u16, rows: u16) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let dir_files = crate::server::cue_dir().join("shell");
    std::fs::create_dir_all(&dir_files).map_err(|e| e.to_string())?;
    let script = dir_files.join(format!("run-{}.sh", id.strip_prefix(PREFIX).unwrap_or(id)));
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&script).and_then(|mut f| f.write_all(launcher(&command(dir)).as_bytes())).map_err(|e| e.to_string())?;
    let run = format!("exec /bin/sh {}", crate::terminal::focus::shq(&script.to_string_lossy()));
    // (set-option's target is a pane's: the session as "=name:", its whole name.)
    let (s, c, r) = (format!("={name}:"), cols.max(10).to_string(), rows.max(2).to_string());
    let mut args = vec!["new-session", "-d", "-s", name, "-x", &c, "-y", &r, "-c", dir, &run];
    for (k, v) in [("status", "off"), ("prefix", "None"), ("prefix2", "None"), ("mouse", "on"), ("set-titles", "on"), ("set-titles-string", "#{pane_title}"), ("destroy-unattached", "off"), ("detach-on-destroy", "on")] {
        args.extend([";", "set-option", "-t", &s, k, v]);
    }
    args.extend([";", "set-option", "-w", "-t", &s, "remain-on-exit", "off"]);
    tmux(&args).map(|_| ()).ok_or_else(|| {
        let _ = std::fs::remove_file(&script);
        "tmux didn't start a shell".to_string()
    })
}

/// The script a shell's tmux session runs: `c` (program, arguments, environment) under tmux's terminal.
fn launcher(c: &CommandBuilder) -> String {
    let q = crate::terminal::focus::shq;
    let mut s = String::from("#!/bin/sh\n# A shell of Cue's terminal, in tmux. Written by Cue; it removes itself.\nrm -f -- \"$0\"\nexec /usr/bin/env -i TERM=\"$TERM\" TMUX=\"$TMUX\" TMUX_PANE=\"$TMUX_PANE\"");
    for (k, v) in c.iter_full_env_as_str().filter(|(k, _)| !matches!(*k, "TERM" | "TMUX" | "TMUX_PANE")) {
        s.push(' ');
        s.push_str(&q(&format!("{k}={v}")));
    }
    for a in c.get_argv() {
        s.push(' ');
        s.push_str(&q(&a.to_string_lossy()));
    }
    s.push('\n');
    s
}

/// Close shell `id`: its tab's gone, so the shell ends too (its tmux session with it).
pub fn close(id: &str) {
    if crate::locking::lock(&KEPT, "KEPT").remove(id) {
        let _ = tmux(&["kill-session", "-t", &format!("={}", session_of(id))]);
    }
    crate::terminal::term::close(id);
}

/// The shells still running in tmux (from before Cue last quit, or open now), oldest first: {id, name,
/// cwd}, `name` what you named its tab ("" for none), `cwd` the folder it's in now. Each is opened again
/// by its id.
pub fn list(app: AppHandle) -> Vec<serde_json::Value> {
    if !crate::terminal::focus::tmux_installed() {
        return vec![];
    }
    let out = tmux(&["list-sessions", "-F", LISTED]).unwrap_or_default();
    let mut found = listed(&out);
    found.sort();
    crate::locking::lock(&KEPT, "KEPT").extend(found.iter().map(|(_, id, ..)| id.clone()));
    if !found.is_empty() {
        watch(app);
    }
    found.into_iter().map(|(_, id, name, cwd)| serde_json::json!({ "id": id, "name": name, "cwd": cwd })).collect()
}

/// What `list` asks tmux of each session: when it started, its name, its tab's name (kept with no "|" in
/// it: `name_kept`), then its folder (anything, so last). "|", not a tab: some tmux versions print a tab as "_".
const LISTED: &str = "#{session_created}|#{session_name}|#{@cue_name}|#{pane_current_path}";

/// Cue's shells in `tmux list-sessions` lines (LISTED): (created, id, tab name, folder).
fn listed(out: &str) -> Vec<(u64, String, String, String)> {
    out.lines()
        .filter_map(|l| {
            let mut p = l.splitn(4, '|');
            let (created, name, tab, cwd) = (p.next()?.parse().ok()?, p.next()?, p.next().unwrap_or(""), p.next().unwrap_or(""));
            let id = format!("{PREFIX}{}", name.strip_prefix(SESSION)?);
            id_ok(&id).then(|| (created, id, name_read(tab), cwd.to_string()))
        })
        .collect()
}

/// A tab's name as kept on its session: "%" and "|" (and line breaks) spelled out, so `listed` can split.
fn name_kept(name: &str) -> String {
    name.replace('%', "%25").replace('|', "%7C").replace('\n', "%0A").replace('\r', "%0D")
}

fn name_read(kept: &str) -> String {
    kept.replace("%7C", "|").replace("%0A", "\n").replace("%0D", "\r").replace("%25", "%")
}

/// Its tab's name, kept on its tmux session for the next Cue ("" for none: its host and folder).
pub fn rename(id: &str, name: &str) {
    if kept(id) {
        let s = format!("={}:", session_of(id));
        let _ = if name.is_empty() { tmux(&["set-option", "-u", "-t", &s, "@cue_name"]) } else { tmux(&["set-option", "-t", &s, "@cue_name", &name_kept(name)]) };
    }
}

/// ⌘K in a shell in tmux: its screen and what it scrolled back cleared, by tmux (which redraws the tab),
/// the prompt drawn again.
pub fn clear(id: &str) -> Result<(), String> {
    let p = format!("={}:", session_of(id));
    tmux(&["send-keys", "-R", "-t", &p, "C-l", ";", "clear-history", "-t", &p]).map(|_| ()).ok_or("its shell isn't running".into())
}

/// The command a shell is running: the process group in its terminal's foreground, when that isn't the
/// shell itself (at its prompt it is). None at its prompt.
fn running(shell: Option<u32>, fg: Option<i32>) -> Option<i32> {
    match (shell, fg) {
        (Some(s), Some(g)) if g > 0 && g as u32 != s => Some(g),
        _ => None,
    }
}

/// What process `pid` is running, as `ps` says it (its own title: "npm test", "sleep 5"), the first
/// word without its folder, cut to a tab's width.
fn what(pid: i32) -> String {
    let out = std::process::Command::new("/bin/ps").args(["-o", "args=", "-p", &pid.to_string()]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let (first, rest) = out.split_once(' ').unwrap_or((&out, ""));
    let first = first.rsplit('/').next().unwrap_or(first);
    let line = if rest.is_empty() { first.to_string() } else { format!("{first} {rest}") };
    if line.chars().count() > 40 {
        format!("{}…", line.chars().take(39).collect::<String>())
    } else {
        line
    }
}

/// Each shell in tmux: its shell's process, and the process group in the foreground of its terminal
/// (one tmux trip and one `ps` for all of them).
fn kept_foreground() -> Vec<(String, Option<u32>, Option<i32>)> {
    let out = tmux(&["list-panes", "-a", "-F", "#{session_name}|#{pane_pid}"]).unwrap_or_default();
    let mut panes: Vec<(String, u32)> = vec![];
    for (name, pid) in out.lines().filter_map(|l| l.split_once('|')) {
        let (Some(n), Ok(pid)) = (name.strip_prefix(SESSION), pid.parse()) else { continue };
        let id = format!("{PREFIX}{n}");
        if !panes.iter().any(|(i, _)| *i == id) {
            panes.push((id, pid));
        }
    }
    if panes.is_empty() {
        return vec![];
    }
    let pids = panes.iter().map(|(_, p)| p.to_string()).collect::<Vec<_>>().join(",");
    let ps = std::process::Command::new("/bin/ps").args(["-o", "pid=,tpgid=", "-p", &pids]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let fg: HashMap<u32, i32> = ps.lines().filter_map(|l| { let mut w = l.split_whitespace(); Some((w.next()?.parse().ok()?, w.next()?.parse().ok()?)) }).collect();
    panes.into_iter().map(|(id, pid)| (id, Some(pid), fg.get(&pid).copied())).collect()
}

/// Every shell's foreground, every 0.8 s, from the first shell on: a "shell" event for each that started
/// or finished running something (or was just opened). A shell in tmux, from tmux (its tab's terminal is
/// tmux's client, always in its own foreground).
fn watch(app: AppHandle) {
    static ON: OnceLock<()> = OnceLock::new();
    if ON.set(()).is_err() {
        return;
    }
    std::thread::spawn(move || {
        let mut seen: HashMap<String, Option<i32>> = HashMap::new();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(800));
            let any_kept = !crate::locking::lock(&KEPT, "KEPT").is_empty();
            let mut now: HashMap<String, Option<i32>> = crate::terminal::term::foreground(PREFIX).into_iter().filter(|(id, ..)| !kept(id)).map(|(id, pid, fg)| (id, running(pid, fg))).collect();
            if any_kept {
                now.extend(kept_foreground().into_iter().map(|(id, pid, fg)| (id, running(pid, fg))));
            }
            for (id, g) in &now {
                if seen.get(id) != Some(g) {
                    let _ = app.emit("shell", serde_json::json!({ "id": id, "busy": g.is_some(), "what": g.map(what).unwrap_or_default() }));
                }
            }
            seen = now;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::sync::{Arc, Mutex};

    fn until(mut ok: impl FnMut() -> bool) -> bool {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < end {
            if ok() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        ok()
    }

    #[test]
    fn a_shells_id_is_its_prefix_and_a_plain_name() {
        assert!(id_ok("shell:1") && id_ok("shell:a-b_2"));
        for bad in ["", "shell:", "1", "relay", "shell:a b", "shell:../x", "shell:$(id)", &format!("shell:{}", "x".repeat(49))] {
            assert!(!id_ok(bad), "{bad:?}");
        }
        assert!(open_with("not-a-shell", "/", 80, 12, |_| {}).is_err(), "a session's id is never opened as a shell");
    }

    #[test]
    fn a_folder_that_isnt_one_starts_at_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(folder("/no/such/folder"), home);
        assert_eq!(folder("  "), home);
        assert_eq!(folder("/tmp"), "/tmp");
    }

    /// Run `shell` as a Cue shell would start, with a setup (in `home`) that `cd`s to / and defines
    /// `cuetest`; type a line; what it printed.
    fn started(shell: &str, home: &std::path::Path, dir: &str, zdotdir: Option<String>) -> String {
        let id = format!("shell:w{}{}", shell.len(), std::process::id());
        let wrappers = home.join("wrappers");
        let mut c = command_for(shell, dir, zdotdir, &wrappers);
        c.env("HOME", home);
        // (Cue itself started from inside other shells.)
        assert!(c.get_env("SHLVL").is_none(), "Cue's own level isn't passed on");
        let seen = Arc::new(Mutex::new(String::new()));
        let s2 = seen.clone();
        crate::terminal::term::spawn(&id, c, 100, 20, move |v| {
            if let Some(d) = v["data"].as_str() {
                s2.lock().unwrap().push_str(&String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(d).unwrap()));
            }
        })
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        crate::terminal::term::write(&id, "echo \"at:$PWD\" \"$(cuetest)\" \"z:${ZDOTDIR-unset}\" \"lvl:$SHLVL\" \"ps:[$PS1]\"\r").unwrap();
        until(|| seen.lock().unwrap().contains("from-rc"));
        crate::terminal::term::close(&id);
        let out = seen.lock().unwrap().clone();
        out
    }

    #[test]
    fn its_folder_wins_over_a_setup_that_cds_elsewhere_and_the_setup_still_loads() {
        let home = std::env::temp_dir().join(format!("cue-shell-home-{}", std::process::id()));
        let dir = home.join("project");
        std::fs::create_dir_all(&dir).unwrap();
        let home = home.canonicalize().unwrap();
        let dir = dir.canonicalize().unwrap().to_string_lossy().to_string();
        // zsh, the setup in ~: its .zshrc cds away, defines a function, and has a prompt of three lines.
        std::fs::write(home.join(".zshrc"), "cd /\ncuetest() { echo from-rc; }\nsetopt promptsubst; PS1=$'\\n${SSH_TTY:+%m }where\\n%% '\n").unwrap();
        let z = started("/bin/zsh", &home, &dir, None);
        // zsh, the setup in a ZDOTDIR of its own.
        let own = home.join("zdot");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join(".zshrc"), "cd /\ncuetest() { echo from-rc; }\n").unwrap();
        std::fs::write(home.join(".zshrc"), "").unwrap();
        let zd = started("/bin/zsh", &home, &dir, Some(own.to_string_lossy().to_string()));
        // bash: a .bash_profile that does the same.
        std::fs::write(home.join(".bash_profile"), "cd /\ncuetest() { echo from-rc; }\n").unwrap();
        let b = started("/bin/bash", &home, &dir, None);
        let _ = std::fs::remove_dir_all(&home);
        let at = format!("at:{dir} from-rc");
        assert!(z.contains(&at) && z.contains("z:unset"), "zsh: in its folder, the setup loaded, ZDOTDIR as it was: {z:?}");
        assert!(zd.contains(&at) && zd.contains(&format!("z:{}", own.display())), "zsh with a ZDOTDIR: the same, that ZDOTDIR kept: {zd:?}");
        assert!(b.contains(&at), "bash: the same: {b:?}");
        // (Its host shows only over SSH, as many themes': not here, so Cue puts it first.)
        assert!(z.contains("ps:[%B%F{green}%m%f%b ${SSH_TTY:+%m }where %% ]"), "zsh: the prompt on one line, the host first: {z:?}");
        assert!(z.contains("lvl:1") && b.contains("lvl:1"), "a shell of its own, level 1, not nested: {z:?} {b:?}");
    }

    #[test]
    fn at_its_prompt_it_runs_nothing_and_another_foreground_group_is_a_command() {
        assert_eq!(running(Some(100), Some(100)), None);
        assert_eq!(running(Some(100), Some(250)), Some(250));
        assert_eq!(running(None, Some(250)), None);
        assert_eq!(running(Some(100), None), None);
    }

    #[test]
    fn a_shell_without_tmux_runs_in_its_folder_shows_what_it_runs_and_ends_when_closed() {
        // Its shell's files go in Cue's folder (CUE_HOME): not while another test has pointed that elsewhere.
        let _one = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-shell-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap().to_string_lossy().to_string();
        let id = format!("shell:test{}", std::process::id());
        let seen = Arc::new(Mutex::new(String::new()));
        let ended = Arc::new(Mutex::new(false));
        let (s2, e2) = (seen.clone(), ended.clone());
        // (Cue's own pseudo-terminal, as without tmux.)
        crate::terminal::term::spawn(&id, command(&folder(&dir)), 100, 20, move |v| {
            if let Some(d) = v["data"].as_str() {
                s2.lock().unwrap().push_str(&String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(d).unwrap()));
            }
            if v["end"] == true {
                *e2.lock().unwrap() = true;
            }
        })
        .unwrap();
        // Typed once it's up (its profile loaded): where it is, and a sum only a shell would work out.
        let up = until(|| !seen.lock().unwrap().is_empty());
        crate::terminal::term::write(&id, "echo \"at:$PWD\" cue-$((40+2))\r").unwrap();
        let answered = until(|| seen.lock().unwrap().contains("cue-42"));
        let in_folder = seen.lock().unwrap().contains(&format!("at:{dir}"));
        let idle = crate::terminal::term::foreground(PREFIX).into_iter().find(|(i, ..)| *i == id).map(|(_, p, g)| running(p, g));
        // A command running: another group in its foreground, named as ps names it.
        crate::terminal::term::write(&id, "sleep 3\r").unwrap();
        let mut busy = None;
        let caught = until(|| {
            busy = crate::terminal::term::foreground(PREFIX).into_iter().find(|(i, ..)| *i == id).and_then(|(_, p, g)| running(p, g));
            busy.is_some()
        });
        let name = busy.map(what).unwrap_or_default();
        crate::terminal::term::close(&id);
        let closed = until(|| *ended.lock().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(up, "the shell started");
        assert!(answered, "it ran what was typed: {:?}", seen.lock().unwrap());
        assert!(in_folder, "in the folder it was opened in: {:?}", seen.lock().unwrap());
        assert_eq!(idle, Some(None), "at its prompt it runs nothing");
        assert!(caught, "sleep running is seen");
        assert!(name.starts_with("sleep"), "named: {name:?}");
        assert!(closed, "closing it ends it");
        assert!(crate::terminal::term::write(&id, "x").is_err(), "nothing open any more");
    }

    #[test]
    fn cues_shells_in_tmux_are_found_by_their_sessions_names() {
        let tab = name_kept("50% | done");
        let out = format!("1700000002|cue-shell-b||/tmp\n1700000001|cue-shell-a|{tab}|/Users/me/a|b\n1700000000|work||/\n1700000003|cue-shell-||/\n");
        assert_eq!(listed(&out), vec![(1700000002, "shell:b".into(), "".into(), "/tmp".into()), (1700000001, "shell:a".into(), "50% | done".into(), "/Users/me/a|b".into())]);
    }

    #[test]
    fn a_shell_in_tmux_carries_on_when_its_tab_lets_go_and_comes_back_as_it_was() {
        if !crate::terminal::focus::tmux_installed() {
            return;
        }
        let _one = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-kept-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap().to_string_lossy().to_string();
        let id = format!("shell:kept{}", std::process::id());
        let name = session_of(&id);
        let tab = |seen: Arc<Mutex<String>>| move |v: serde_json::Value| {
            if let Some(d) = v["data"].as_str() {
                seen.lock().unwrap().push_str(&String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(d).unwrap()));
            }
        };
        let first = Arc::new(Mutex::new(String::new()));
        let (_, in_tmux) = open_with(&id, &dir, 100, 20, tab(first.clone())).unwrap();
        until(|| !first.lock().unwrap().is_empty());
        std::thread::sleep(std::time::Duration::from_millis(300));
        crate::terminal::term::write(&id, "echo \"at:$PWD\" cue-$((40+2)); export CUE_KEPT=still-here\r").unwrap();
        let answered = until(|| first.lock().unwrap().contains("cue-42"));
        let script_gone = !std::path::Path::new(&crate::server::cue_dir().join("shell").join(format!("run-{}.sh", id.trim_start_matches(PREFIX)))).exists();
        // Cue quits: the tab's client goes, the shell doesn't.
        crate::terminal::term::close(&id);
        let detached = until(|| tmux(&["list-clients", "-t", &format!("={name}"), "-F", "#{client_name}"]).is_some_and(|c| c.trim().is_empty()));
        let alive = tmux(&["has-session", "-t", &format!("={name}")]).is_some();
        let found = listed(&tmux(&["list-sessions", "-F", LISTED]).unwrap_or_default()).into_iter().any(|(_, i, ..)| i == id);
        rename(&id, "my | tab");
        let named = listed(&tmux(&["list-sessions", "-F", LISTED]).unwrap_or_default()).into_iter().any(|(_, i, n, _)| i == id && n == "my | tab");
        // The next Cue: the same shell, what it showed and what was set in it.
        let again = Arc::new(Mutex::new(String::new()));
        open_with(&id, "/", 100, 20, tab(again.clone())).unwrap();
        let redrawn = until(|| again.lock().unwrap().contains("cue-42"));
        crate::terminal::term::write(&id, "echo \"kept:$CUE_KEPT\"\r").unwrap();
        let same_shell = until(|| again.lock().unwrap().contains("kept:still-here"));
        let foreground = kept_foreground().into_iter().find(|(i, ..)| *i == id).map(|(_, p, g)| running(p, g));
        // Its tab closed: the shell ends.
        close(&id);
        let ended = until(|| tmux(&["has-session", "-t", &format!("={name}")]).is_none());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(in_tmux, "in tmux");
        let first = first.lock().unwrap().clone();
        assert!(answered && first.contains(&format!("at:{dir}")), "it ran in its folder: {first:?}");
        assert!(script_gone, "the script with its environment removed itself");
        assert!(detached && alive && found, "the shell carried on with no tab attached, and is found");
        assert!(named, "its tab's name kept on it");
        assert!(redrawn, "attached again, it shows what it showed: {:?}", again.lock().unwrap());
        assert!(same_shell, "the same shell: {:?}", again.lock().unwrap());
        assert_eq!(foreground, Some(None), "at its prompt, from tmux");
        assert!(ended, "closing its tab ends it");
    }
}
