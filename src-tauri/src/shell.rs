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
    let out = out?;
    let timed_out = *timed_out.lock().unwrap();
    let mut output = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    let err = String::from_utf8_lossy(&out.stderr).trim_end().to_string();
    if !err.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&err);
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
    Ok(Ran { output, code: out.status.code().unwrap_or(-1), ms: started.elapsed().as_millis() as u64, cwd: cwd.to_string(), timed_out })
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
}
