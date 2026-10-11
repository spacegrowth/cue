//! Cue's own shells: the floating terminal's tabs. Each is your login shell (the one a new terminal tab
//! opens, your profile loaded) in a folder you pick, in a pseudo-terminal of Cue's own, streamed to the
//! window as the built-in terminal is (term.rs: the same "term" events, written to, resized and closed
//! by its id). It starts in its folder even when your setup `cd`s elsewhere as it loads. One lives until it's closed, `exit`ed, or Cue quits. Whether each is running a command
//! (and which) is looked at every 0.8 s and sent as "shell" events when it changes, for the live dot.
use portable_pty::CommandBuilder;
use std::collections::HashMap;
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

/// Whether any shell is open: quitting Cue ends them, and what they showed with them (the window asks first).
pub fn any_open() -> bool {
    !crate::terminal::term::foreground(PREFIX).is_empty()
}

/// Open shell `id` in `cwd` at `cols`×`rows`. Its output arrives as "term" events (as a session's
/// terminal's do); what it runs, as "shell" events ({id, busy, what}).
pub fn open(app: AppHandle, id: &str, cwd: &str, cols: u16, rows: u16) -> Result<u64, String> {
    let a = app.clone();
    let gen = open_with(id, cwd, cols, rows, move |v| {
        let _ = a.emit("term", v);
    })?;
    watch(app);
    Ok(gen)
}

fn open_with(id: &str, cwd: &str, cols: u16, rows: u16, out: impl Fn(serde_json::Value) + Send + 'static) -> Result<u64, String> {
    if !id_ok(id) {
        return Err(format!("“{id}” isn't a shell's id"));
    }
    crate::terminal::term::spawn(id, command(&folder(cwd)), cols, rows, out)
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

/// Every shell's foreground, every 0.8 s, from the first shell on: a "shell" event for each that started
/// or finished running something (or was just opened).
fn watch(app: AppHandle) {
    static ON: OnceLock<()> = OnceLock::new();
    if ON.set(()).is_err() {
        return;
    }
    std::thread::spawn(move || {
        let mut seen: HashMap<String, Option<i32>> = HashMap::new();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(800));
            let now: HashMap<String, Option<i32>> = crate::terminal::term::foreground(PREFIX).into_iter().map(|(id, pid, fg)| (id, running(pid, fg))).collect();
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
    fn a_shell_runs_in_its_folder_shows_what_it_runs_and_ends_when_closed() {
        let dir = std::env::temp_dir().join(format!("cue-shell-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap().to_string_lossy().to_string();
        let id = format!("shell:test{}", std::process::id());
        let seen = Arc::new(Mutex::new(String::new()));
        let ended = Arc::new(Mutex::new(false));
        let (s2, e2) = (seen.clone(), ended.clone());
        open_with(&id, &dir, 100, 20, move |v| {
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
}
