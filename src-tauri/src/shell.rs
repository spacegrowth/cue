//! "!" in a session's box: the rest of the line is a shell command Cue runs for you, in the session's
//! folder, in your own shell (the one macOS has on record for your account, the one a new terminal tab
//! opens), with its profile loaded, so your PATH and aliases apply. The agent's terminal isn't involved:
//! it works the same for a session in Cue, in iTerm, or anywhere else. What it printed comes back as a
//! card in the chat. Each line starts fresh in the folder: a `cd` or an export doesn't carry over.
use serde::Serialize;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The longest a line may run before Cue stops it.
const TIMEOUT_SECS: u64 = 120;
/// Output past this is cut (a `cat` of something huge), with a note saying how much.
const MAX_OUTPUT: usize = 64 * 1024;

#[derive(Serialize, Debug, PartialEq)]
pub struct Ran {
    /// stdout, then stderr if there was any (after a blank line), trimmed at the end.
    pub output: String,
    /// Exit status; -1 if it was stopped by a signal or the timeout.
    pub code: i32,
    pub ms: u64,
    pub cwd: String,
    pub timed_out: bool,
}

/// The card's `Ran`, from what a shell said: stdout, then stderr (after a blank line), trimmed and cut.
fn finish(stdout: &str, stderr: &str, code: i32, ms: u64, timed_out: bool, cwd: &str) -> Ran {
    let mut output = stdout.trim_end().to_string();
    if !stderr.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(stderr);
    }
    if output.len() > MAX_OUTPUT {
        let mut cut = MAX_OUTPUT;
        while !output.is_char_boundary(cut) {
            cut -= 1;
        }
        let more = output.len() - cut;
        output.truncate(cut);
        output.push_str(&format!("\n… {more} more bytes not shown"));
    }
    Ran { output, code, ms, cwd: cwd.to_string(), timed_out }
}

/// Wait for `child`, stopped after the longest a line may run: what it said, and whether Cue stopped it.
fn wait_capped(child: std::process::Child) -> Result<(std::process::Output, bool), String> {
    let pid = child.id();
    let done = Arc::new(Mutex::new(false));
    let timed_out = Arc::new(Mutex::new(false));
    let (watch, flag) = (done.clone(), timed_out.clone());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(TIMEOUT_SECS));
        if !*watch.lock().unwrap() {
            *flag.lock().unwrap() = true;
            let _ = Command::new("/bin/kill").arg(pid.to_string()).status();
        }
    });
    let out = child.wait_with_output().map_err(|e| e.to_string());
    *done.lock().unwrap() = true;
    let timed_out = *timed_out.lock().unwrap();
    Ok((out?, timed_out))
}

/// Run `line` in `cwd`. Err only when it couldn't start (no such folder, no shell): a command that fails
/// is an Ok with its code and what it said.
pub fn run(cwd: &str, line: &str) -> Result<Ran, String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("nothing to run".into());
    }
    if !std::path::Path::new(cwd).is_dir() {
        return Err(format!("{cwd} isn't a folder"));
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let started = Instant::now();
    // -l: your profile (PATH); -i: your aliases. Then into the folder (a profile may `cd` somewhere of
    // its own as it loads), by name through the environment: the folder and the line never meet a quote.
    let child = Command::new(&shell)
        .args(["-lic", &format!("cd -- \"$CUE_CWD\" && {line}")])
        .env("CUE_CWD", cwd)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start {shell}: {e}"))?;
    let (out, timed_out) = wait_capped(child)?;
    Ok(finish(&String::from_utf8_lossy(&out.stdout), &String::from_utf8_lossy(&out.stderr).trim_end(), out.status.code().unwrap_or(-1), started.elapsed().as_millis() as u64, timed_out, cwd))
}

/// The script `run_on_machine` sends: into the folder there (saying NODIR rather than failing, as
/// `open_in_tmux` does), the line in a subshell of its own (an `exit` in it only ends that), and Cue's
/// own last line with its exit code — the proof the script ran, an ssh failure having none. The line
/// is stopped there after `secs` (it and everything it started: ending ssh here wouldn't reach it),
/// and a stopped one says __CUE_TIMEOUT__ before its code.
fn remote_script(cwd: &str, line: &str, secs: u64) -> String {
    format!(
        r#"cd {dir} 2>/dev/null || {{ echo NODIR; exit 0; }}
t="${{TMPDIR:-/tmp}}/cue-ran-$$"
tree() {{ for c in $(ps -eo pid=,ppid= | awk -v q="$1" '$2==q {{print $1}}'); do tree "$c"; done; echo "$1"; }}
(
{line}
) </dev/null &
p=$!
( sleep {secs}; : > "$t"; kill -TERM $(tree "$p") 2>/dev/null ) >/dev/null 2>&1 &
w=$!
wait "$p"; c=$?
if [ -e "$t" ]; then rm -f "$t"; printf '\n__CUE_TIMEOUT__'; else kill $(tree "$w") 2>/dev/null; fi
printf '\n__CUE_EXIT__:%d\n' "$c""#,
        dir = crate::machines::remote_dir(cwd)
    )
}

/// `run`, on a machine Cue knows (the session runs there): the same timeout and cut, over Cue's shared
/// SSH connection, in the folder as it is there, with your login shell's setup there. ssh passes the
/// script's exit code through. Err only when it couldn't start there: a command that fails is an Ok.
pub fn run_on_machine(m: &crate::machines::Machine, cwd: &str, line: &str) -> Result<Ran, String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("nothing to run".into());
    }
    let started = Instant::now();
    let child = crate::machines::ssh_command_err(&m.host, &remote_script(cwd, line, TIMEOUT_SECS - 3))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start ssh: {e}"))?;
    let (out, timed_out) = wait_capped(child)?;
    let ms = started.elapsed().as_millis() as u64;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim_end().to_string();
    if timed_out {
        return Ok(finish(&stdout, &stderr, -1, ms, true, cwd));
    }
    // Cue's own last line, when the script ran (its code, so an `exit` in the line still counts).
    if let Some((head, code)) = stdout.rsplit_once("\n__CUE_EXIT__:") {
        // Stopped there, at the time limit: the card says so, as it does for a line here.
        if let Some(head) = head.strip_suffix("\n__CUE_TIMEOUT__") {
            return Ok(finish(head, &stderr, -1, ms, true, cwd));
        }
        return Ok(finish(head, &stderr, code.trim().parse().unwrap_or(-1), ms, false, cwd));
    }
    if stdout.trim() == "NODIR" {
        return Err(format!("{cwd} isn't a folder on {}", m.name));
    }
    let err = if stderr.is_empty() { format!("exit {}", out.status.code().unwrap_or(-1)) } else { stderr };
    if crate::machines::wants_login(&err) {
        return Err(format!("{err} — log in through + New → Machine"));
    }
    Err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_runs_in_the_folder_in_your_shell_and_comes_back_with_what_it_said() {
        let r = run("/tmp", "echo hi; pwd -P; echo oops >&2; exit 3").unwrap();
        assert_eq!(r.code, 3);
        assert!(r.output.starts_with("hi\n/private/tmp") || r.output.starts_with("hi\n/tmp"), "{:?}", r.output);
        assert!(r.output.ends_with("\n\noops"), "stderr after stdout: {:?}", r.output);
        assert!(!r.timed_out);
    }

    #[test]
    fn it_wont_start_in_a_folder_that_isnt_one_and_says_nothing_for_an_empty_line() {
        assert!(run("/no/such/folder", "true").is_err());
        assert!(run("/tmp", "   ").is_err());
    }

    #[test]
    fn long_output_is_cut_with_a_note() {
        let r = run("/tmp", "head -c 100000 /dev/zero | tr '\\0' x").unwrap();
        assert!(r.output.len() < MAX_OUTPUT + 64);
        assert!(r.output.ends_with("more bytes not shown"), "{:?}", &r.output[r.output.len() - 40..]);
    }

    #[test]
    fn a_line_on_a_machine_runs_in_the_folder_there_and_says_its_exit_code() {
        let script = remote_script("~", "echo hi; exit 3", 30);
        let out = std::process::Command::new("/bin/sh").args(["-c", &crate::machines::remote_command_err(&script)]).output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(stdout.starts_with("hi\n"), "{stdout:?}");
        assert!(stdout.ends_with("\n__CUE_EXIT__:3\n"), "an `exit` in the line still says its code: {stdout:?}");
        let (_, code) = stdout.rsplit_once("\n__CUE_EXIT__:").unwrap();
        assert_eq!(code.trim().parse::<i32>(), Ok(3), "the code as the card reads it");
        let c = std::process::Command::new("/bin/sh").args(["-c", &crate::machines::remote_command_err(&remote_script("~", "echo ok # a comment", 30))]).output().unwrap();
        assert!(String::from_utf8_lossy(&c.stdout).ends_with("\n__CUE_EXIT__:0\n"), "a comment at the end doesn't break the line");
    }

    #[test]
    fn a_line_on_a_machine_is_stopped_there_at_the_limit_with_what_it_started() {
        // dash, as /bin/sh is on most Linux machines, and this Mac's sh.
        for sh in ["/bin/sh", "/bin/dash"].into_iter().filter(|p| std::path::Path::new(p).exists()) {
            let mark = format!("cue-stop-test-{}-{}", std::process::id(), sh.len());
            let started = Instant::now();
            let out = std::process::Command::new(sh).args(["-c", &remote_script("~", &format!("echo begun; sleep 30 & sleep 31; echo {mark}"), 1)]).output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            assert!(started.elapsed().as_secs() < 10, "{sh}: stopped at the limit, not after 30 s");
            assert!(stdout.starts_with("begun\n") && stdout.contains("\n__CUE_TIMEOUT__\n__CUE_EXIT__:"), "{sh}: {stdout:?}");
            std::thread::sleep(std::time::Duration::from_millis(300));
            let left = std::process::Command::new("/bin/ps").args(["-eo", "command="]).output().unwrap();
            assert!(!String::from_utf8_lossy(&left.stdout).lines().any(|l| l.trim() == "sleep 30" || l.trim() == "sleep 31"), "{sh}: nothing it started is left running");
            // A quick line: no timeout mark, and its watcher is gone too.
            let quick = std::process::Command::new(sh).args(["-c", &remote_script("~", "echo fast", 77)]).output().unwrap();
            assert!(String::from_utf8_lossy(&quick.stdout).ends_with("fast\n\n__CUE_EXIT__:0\n"), "{sh}");
            let left = std::process::Command::new("/bin/ps").args(["-eo", "command="]).output().unwrap();
            assert!(!String::from_utf8_lossy(&left.stdout).lines().any(|l| l.trim() == "sleep 77"), "{sh}: the watcher doesn't outlive the line");
        }
        let bad = std::process::Command::new("/bin/sh").args(["-c", &crate::machines::remote_command_err(&remote_script("/no/such/dir", "true", 30))]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&bad.stdout).trim(), "NODIR", "a folder that isn't one there says so");
    }
}
