//! "By the way": a side question about a Claude Code session, answered without touching it (as Claude
//! Code's own /btw is, but shown in Cue). Cue asks a copy of the conversation: resumed and forked,
//! never saved, with no tools and no hooks. The session keeps working, its transcript and chat never
//! show the question, and no new session turns up anywhere. Costs about one turn.

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// The longest a side question may take before Cue gives up on it.
const TIMEOUT_SECS: u64 = 180;

/// Ask `question` about a session Cue knows (its origin): Claude Code only, with the model it last used.
pub fn ask_about(origin: &crate::model::Origin, question: &str) -> Result<String, String> {
    if origin.harness != "claude" {
        return Err("By the way works with Claude Code sessions".into());
    }
    let model = (!origin.transcript_path.is_empty()).then(|| crate::logs::steps::model_of(&origin.transcript_path)).flatten();
    if origin.machine.is_empty() {
        return ask(&origin.session_id, &origin.cwd, model.as_deref(), question);
    }
    // On a machine: its conversation and its folder are there, so Claude Code runs there, over SSH.
    let m = crate::machines::get(&origin.machine).ok_or(format!("{} isn't one of your machines any more (+ New → Machine)", origin.machine))?;
    let line = claude_line(&origin.session_id, model.as_deref())?;
    let cd = if origin.cwd.trim().is_empty() { String::new() } else { format!("cd {} 2>/dev/null || {{ echo NODIR; exit 0; }}\n", crate::machines::remote_dir(&origin.cwd)) };
    let script = format!("{cd}{line}");
    let child = crate::machines::ssh_command_err(&m.host, &script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start ssh: {e}"))?;
    let out = asked(child, question)?;
    if String::from_utf8_lossy(&out.stdout).trim() == "NODIR" {
        return Err(format!("{} isn't a folder on {}", origin.cwd, m.name));
    }
    reply(&out)
}

/// The Claude Code line that answers: the session resumed as a throwaway fork, no tools, no hooks.
/// Both ids go into a shell line: only the characters ids and model names use.
fn claude_line(session_id: &str, model: Option<&str>) -> Result<String, String> {
    let safe = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.[]".contains(c));
    if !safe(session_id) {
        return Err("not a Claude Code session".into());
    }
    let model = model.filter(|m| safe(m)).map(|m| format!(" --model {m}")).unwrap_or_default();
    Ok(format!(
        r#"exec claude -p --resume {session_id} --fork-session --no-session-persistence --tools "" --settings '{{"disableAllHooks":true}}' --output-format json{model}"#
    ))
}

/// Ask `question` about the Claude Code session `session_id` (started in `cwd`), with its own model when
/// known. The answer, or why there isn't one.
pub fn ask(session_id: &str, cwd: &str, model: Option<&str>, question: &str) -> Result<String, String> {
    let line = claude_line(session_id, model)?;
    // Through your login shell, so `claude` is found as in your terminal; the question on stdin (no quoting).
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let child = Command::new(shell)
        .args(["-lic", &line])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start Claude Code: {e}"))?;
    reply(&asked(child, question)?)
}

/// Hand the running Claude Code the question on its stdin and wait for it, never longer than TIMEOUT_SECS.
fn asked(mut child: std::process::Child, question: &str) -> Result<std::process::Output, String> {
    let prompt = format!("(A side question while you work. Answer it briefly from what you know of this conversation; don't start any work.)\n\n{question}");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
    }
    // Never hang on it.
    let pid = child.id();
    let done = Arc::new(Mutex::new(false));
    let watch = done.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(TIMEOUT_SECS));
        if !*watch.lock().unwrap() {
            let _ = Command::new("/bin/kill").arg(pid.to_string()).status();
        }
    });
    let out = child.wait_with_output().map_err(|e| e.to_string());
    *done.lock().unwrap() = true;
    out
}

/// What it said: the answer, or why there isn't one.
fn reply(out: &std::process::Output) -> Result<String, String> {
    answer(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| {
        let err = String::from_utf8_lossy(&out.stderr).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
        if err.is_empty() { "no answer (it took too long, or Claude Code stopped)".into() } else { err }
    })?
}

/// The answer in Claude Code's JSON result (its last line); an error result as Err.
fn answer(stdout: &str) -> Option<Result<String, String>> {
    let v: Value = stdout.lines().rev().find_map(|l| serde_json::from_str(l.trim()).ok())?;
    let text = v.get("result").and_then(Value::as_str).unwrap_or("").trim().to_string();
    Some(if v.get("is_error").and_then(Value::as_bool).unwrap_or(false) { Err(if text.is_empty() { "Claude Code couldn't answer".into() } else { text }) } else { Ok(text) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_the_result_line_and_an_error_result_says_why() {
        assert_eq!(answer("noise\n{\"type\":\"result\",\"result\":\" Because X. \",\"is_error\":false}\n"), Some(Ok("Because X.".into())));
        assert_eq!(answer("{\"result\":\"No conversation found\",\"is_error\":true}"), Some(Err("No conversation found".into())));
        assert_eq!(answer("not json"), None);
    }

    #[test]
    fn an_id_that_isnt_one_never_reaches_the_shell() {
        assert!(ask("x; rm -rf ~", "/tmp", None, "hi").is_err());
    }
}
