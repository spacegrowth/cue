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
    let out = asked(child, &side(question))?;
    if String::from_utf8_lossy(&out.stdout).trim() == "NODIR" {
        return Err(format!("{} isn't a folder on {}", origin.cwd, m.name));
    }
    reply(&out)
}

/// "Catch me up": what happened since you last looked, from only those messages (`since`, oldest first), read
/// by a small model with no conversation behind it. A few cents however long the session is: forking the whole
/// conversation would re-read all of it, at the session's own model's price, its cache long gone by then.
/// `seen`: the opening of the last reply you read, so it knows where you left off. Asked only when you click (or say) it.
pub fn catch_up(seen: Option<&str>, since: &[crate::model::Exchange]) -> Result<CatchUp, String> {
    run(CATCH_UP_LINE, &std::env::temp_dir(), &catch_up_prompt(seen, since)).map(|a| CatchUp::from_answer(&a))
}

/// The model that catches you up, with no tools, no hooks and nothing saved. Its own system prompt, so it reads
/// only the messages (Claude Code's would have it act as an agent in whatever folder it's in).
const CATCH_UP_LINE: &str = r#"exec claude -p --model haiku --no-session-persistence --tools "" --settings '{"disableAllHooks":true}' --system-prompt "You summarize a coding agent's messages for its owner. Use only the messages given." --output-format json"#;
/// The most of the messages it reads: the newest ones, each cut to MSG_CHARS.
const SINCE_CHARS: usize = 60_000;
const MSG_CHARS: usize = 4_000;

fn catch_up_prompt(seen: Option<&str>, since: &[crate::model::Exchange]) -> String {
    let who = |r: &str| match r { "agent" => "Agent", "you" => "Me", _ => "Another session" };
    let mut msgs: Vec<String> = vec![];
    let mut total = 0;
    for e in since.iter().rev() {
        let m = format!("{}: {}", who(&e.role), crate::logs::transcript::cut(e.text.trim(), MSG_CHARS));
        if total + m.len() > SINCE_CHARS && !msgs.is_empty() {
            msgs.push("(earlier messages left out)".into());
            break;
        }
        total += m.len();
        msgs.push(m);
    }
    msgs.reverse();
    let from = match seen.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => format!("The last reply of the agent's I read began: \"{}\".\n\n", s.chars().take(160).collect::<String>()),
        None => String::new(),
    };
    format!(
        "I've been away from a coding agent's session. {from}Here is what was said since, oldest first:\n\n{}\n\n\
         Catch me up. Answer with 2 to 4 short lines, each starting \"• \", on what the agent did and where it stands, then one line \
         starting \"Needs you: \" with what it needs from me, or \"Needs you: nothing\". Plain words for someone listening: no code, no file paths.",
        msgs.join("\n\n")
    )
}

/// A catch-up as it's shown: what it did, a line each, and what it needs from you (none: nothing).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CatchUp {
    pub points: Vec<String>,
    pub needs: Option<String>,
}

impl CatchUp {
    /// Its "• " lines and its "Needs you:" line; an answer in another shape is kept whole, as one point.
    fn from_answer(a: &str) -> CatchUp {
        let mut points = vec![];
        let mut needs = None;
        for l in a.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if let Some(n) = l.trim_start_matches(['*', '_']).strip_prefix("Needs you:") {
                let n = n.trim_matches(|c: char| c == '*' || c == '_' || c.is_whitespace());
                needs = (!n.is_empty() && !n.trim_end_matches('.').eq_ignore_ascii_case("nothing")).then(|| n.to_string());
            } else if let Some(p) = l.strip_prefix("• ").or_else(|| l.strip_prefix("- ")).or_else(|| l.strip_prefix("* ")) {
                points.push(p.trim().to_string());
            }
        }
        if points.is_empty() && needs.is_none() && !a.trim().is_empty() {
            points.push(a.trim().to_string());
        }
        CatchUp { points, needs }
    }
}

/// The Claude Code line that answers: the session resumed as a throwaway fork, no tools, no hooks.
/// Both ids go into a shell line: only the characters ids and model names use.
fn claude_line(session_id: &str, model: Option<&str>) -> Result<String, String> {
    let safe = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.[]".contains(c));
    if !safe(session_id) {
        return Err("not a Claude Code session".into());
    }
    // Quoted: a 1M model's name ends "[1m]", which zsh would take as a file pattern ("no matches found").
    let model = model.filter(|m| safe(m)).map(|m| format!(" --model '{m}'")).unwrap_or_default();
    Ok(format!(
        r#"exec claude -p --resume {session_id} --fork-session --no-session-persistence --tools "" --settings '{{"disableAllHooks":true}}' --output-format json{model}"#
    ))
}

/// Ask `question` about the Claude Code session `session_id` (started in `cwd`), with its own model when
/// known. The answer, or why there isn't one.
pub fn ask(session_id: &str, cwd: &str, model: Option<&str>, question: &str) -> Result<String, String> {
    run(&claude_line(session_id, model)?, std::path::Path::new(cwd), &side(question))
}

/// Run a Claude Code `line` in `cwd` through your login shell, so `claude` is found as in your terminal, with
/// `prompt` on its stdin (no quoting). The answer, or why there isn't one.
fn run(line: &str, cwd: &std::path::Path, prompt: &str) -> Result<String, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let child = Command::new(shell)
        .args(["-lic", line])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start Claude Code: {e}"))?;
    reply(&asked(child, prompt)?)
}

/// A side question as the forked session reads it.
fn side(question: &str) -> String {
    format!("(A side question while you work. Answer it briefly from what you know of this conversation; don't start any work.)\n\n{question}")
}

/// Hand the running Claude Code its prompt on its stdin and wait for it, never longer than TIMEOUT_SECS.
fn asked(mut child: std::process::Child, prompt: &str) -> Result<std::process::Output, String> {
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
    fn a_1m_model_s_name_is_quoted_so_the_shell_takes_it_as_it_is() {
        let line = claude_line("abc-123", Some("claude-opus-5-5[1m]")).unwrap();
        assert!(line.ends_with(" --model 'claude-opus-5-5[1m]'"), "{line}");
        let out = std::process::Command::new("/bin/zsh").args(["-c", &format!("print -r -- {}", line.split_once(" --model ").unwrap().1)]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "claude-opus-5-5[1m]", "zsh: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn a_model_name_that_could_leave_its_quotes_is_left_off() {
        for bad in ["x' ; rm -rf ~ '", "a b", "a;b", "a$(b)"] {
            let line = claude_line("abc-123", Some(bad)).unwrap();
            assert!(!line.contains("--model"), "{bad}: {line}");
        }
    }

    #[test]
    fn a_catch_up_is_its_points_and_what_it_needs() {
        let c = CatchUp::from_answer("• Built the tabs.\n- Fixed Wrong line.\n\n**Needs you:** say install near the Mac.");
        assert_eq!(c, CatchUp { points: vec!["Built the tabs.".into(), "Fixed Wrong line.".into()], needs: Some("say install near the Mac.".into()) });
        assert_eq!(CatchUp::from_answer("• Merged it.\nNeeds you: nothing.").needs, None, "nothing needed");
        assert_eq!(CatchUp::from_answer("It merged and pushed.").points, vec!["It merged and pushed.".to_string()], "another shape: kept whole");
    }

    fn ex(role: &str, text: &str) -> crate::model::Exchange {
        serde_json::from_value(serde_json::json!({ "role": role, "text": text, "at_ms": 1 })).unwrap()
    }

    #[test]
    fn the_catch_up_reads_only_what_came_after_the_reply_you_read() {
        let p = catch_up_prompt(Some("I've turned Calls into tabs"), &[ex("you", "ship it"), ex("agent", "Shipped.")]);
        assert!(p.contains("began: \"I've turned Calls into tabs\""), "{p}");
        assert!(p.contains("Me: ship it\n\nAgent: Shipped."), "{p}");
        assert!(!catch_up_prompt(None, &[ex("agent", "Done.")]).contains("began:"));
    }

    #[test]
    fn a_long_stretch_keeps_its_newest_messages() {
        let big = "x".repeat(MSG_CHARS);
        let since: Vec<_> = (0..40).map(|_| ex("agent", &big)).chain([ex("agent", "the newest")]).collect();
        let p = catch_up_prompt(None, &since);
        assert!(p.contains("(earlier messages left out)") && p.contains("Agent: the newest"), "kept the end");
        assert!(p.len() < SINCE_CHARS + MSG_CHARS + 1_000, "{}", p.len());
    }

    #[test]
    fn an_id_that_isnt_one_never_reaches_the_shell() {
        assert!(ask("x; rm -rf ~", "/tmp", None, "hi").is_err());
    }
}
