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

/// What an answer cost: every token it read and wrote (cache reads too), and what Claude Code says that came to.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Spent {
    pub tokens: u64,
    pub usd: f64,
}

/// An answer and what it cost.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub text: String,
    pub spent: Spent,
}

/// Ask `question` about a session Cue knows (its origin): Claude Code only, with the model it last used.
pub fn ask_about(origin: &crate::model::Origin, question: &str) -> Result<String, String> {
    ask_copy(origin, &side(question)).map(|a| a.text)
}

/// `prompt`, as it is, to a throwaway copy of a Claude Code session (its origin), with the model it last used:
/// the answer and what it cost. The session never sees it and nothing is saved.
pub fn ask_copy(origin: &crate::model::Origin, prompt: &str) -> Result<Answer, String> {
    if origin.harness != "claude" {
        return Err("By the way works with Claude Code sessions".into());
    }
    let model = (!origin.transcript_path.is_empty()).then(|| crate::logs::steps::model_of(&origin.transcript_path)).flatten();
    if origin.machine.is_empty() {
        return run_full(&claude_line(&origin.session_id, model.as_deref())?, std::path::Path::new(&origin.cwd), prompt);
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
    let out = asked(child, prompt)?;
    if String::from_utf8_lossy(&out.stdout).trim() == "NODIR" {
        return Err(format!("{} isn't a folder on {}", origin.cwd, m.name));
    }
    reply_full(&out)
}

/// The small model that keeps a session's list of what's left (tasks.rs), with no tools, no hooks and nothing
/// saved, and its own system prompt, so it reads only what it's given.
const TASKS_LINE: &str = r#"exec claude -p --model haiku --no-session-persistence --tools "" --settings '{"disableAllHooks":true}' --system-prompt "You keep the list of what is left to do in a coding agent's session, for its owner. Use only what you are given. Answer with only JSON." --output-format json"#;

/// `prompt` to the small model that keeps the lists of what's left, about the session `session_id`: its answer
/// and what it cost. Every call about one session goes under one fixed id (side_id), nothing saved.
pub fn ask_small(prompt: &str, session_id: &str) -> Result<Answer, String> {
    run_full(&format!("{TASKS_LINE} --session-id {}", side_id(&format!("tasks:{session_id}"))), &std::env::temp_dir(), prompt)
}

/// The session id Cue's small-model calls about one session go under, the same every time: a UUID worked out from
/// `key` (what they're for and the session's id). Claude Code sends a session's id with every request, so with a
/// new one each call anything counting sessions (a proxy, a usage report) would see thousands; this way it sees one
/// per session. It's only a label: nothing is saved under it (--no-session-persistence), so each call still reads
/// only what it's given, and costs the same.
pub fn side_id(key: &str) -> String {
    // FNV-1a, twice with different seeds: the same on every Mac and every build (std's hasher isn't promised to be).
    let fnv = |seed: u64| key.bytes().fold(seed, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3));
    let x = format!("{:016x}{:016x}", fnv(0xcbf2_9ce4_8422_2325), fnv(0x9e37_79b9_7f4a_7c15));
    format!("{}-{}-4{}-a{}-{}", &x[0..8], &x[8..12], &x[13..16], &x[17..20], &x[20..32])
}

/// "Catch me up": what happened since you last looked, from only those messages (`since`, oldest first), read
/// by a small model with no conversation behind it. A few cents however long the session is: forking the whole
/// conversation would re-read all of it, at the session's own model's price, its cache long gone by then.
/// `seen`: the opening of the last reply you read, so it knows where you left off. Asked only when you click (or say) it.
pub fn catch_up(seen: Option<&str>, since: &[crate::model::Exchange], session_id: &str) -> Result<CatchUp, String> {
    let line = format!("{CATCH_UP_LINE} --session-id {}", side_id(&format!("catch-up:{session_id}")));
    run(&line, &std::env::temp_dir(), &catch_up_prompt(seen, since)).map(|a| CatchUp::from_answer(&a))
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

/// Run a Claude Code `line` in `cwd` through your login shell, so `claude` is found as in your terminal, with
/// `prompt` on its stdin (no quoting). The answer, or why there isn't one.
fn run(line: &str, cwd: &std::path::Path, prompt: &str) -> Result<String, String> {
    run_full(line, cwd, prompt).map(|a| a.text)
}

/// `run`, with what the answer cost.
fn run_full(line: &str, cwd: &std::path::Path, prompt: &str) -> Result<Answer, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let child = Command::new(shell)
        .args(["-lic", line])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("couldn't start Claude Code: {e}"))?;
    reply_full(&asked(child, prompt)?)
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
        if !*crate::locking::lock(&watch, "btw: asked") {
            let _ = Command::new("/bin/kill").arg(pid.to_string()).status();
        }
    });
    let out = child.wait_with_output().map_err(|e| e.to_string());
    *crate::locking::lock(&done, "btw: asked") = true;
    out
}

/// What it said: the answer and its cost, or why there isn't one.
fn reply_full(out: &std::process::Output) -> Result<Answer, String> {
    answer_full(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| {
        let err = String::from_utf8_lossy(&out.stderr).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
        if err.is_empty() { "no answer (it took too long, or Claude Code stopped)".into() } else { err }
    })?
}

/// The answer in Claude Code's JSON result (its last line); an error result as Err.
#[cfg(test)]
fn answer(stdout: &str) -> Option<Result<String, String>> {
    answer_full(stdout).map(|r| r.map(|a| a.text))
}

/// The answer in Claude Code's JSON result (its last line) with what it cost (its usage and total_cost_usd);
/// an error result as Err.
fn answer_full(stdout: &str) -> Option<Result<Answer, String>> {
    let v: Value = stdout.lines().rev().find_map(|l| serde_json::from_str(l.trim()).ok())?;
    let text = v.get("result").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if v.get("is_error").and_then(Value::as_bool).unwrap_or(false) {
        return Some(Err(if text.is_empty() { "Claude Code couldn't answer".into() } else { text }));
    }
    let u = |k: &str| v.pointer(&format!("/usage/{k}")).and_then(Value::as_u64).unwrap_or(0);
    let tokens = u("input_tokens") + u("output_tokens") + u("cache_creation_input_tokens") + u("cache_read_input_tokens");
    let usd = v.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0);
    Some(Ok(Answer { text, spent: Spent { tokens, usd } }))
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
    fn calls_about_one_session_go_under_one_fixed_uuid() {
        let a = side_id("tasks:11cd1602-0504-4ad0-ad40-7c25f40885aa");
        assert_eq!(a, side_id("tasks:11cd1602-0504-4ad0-ad40-7c25f40885aa"), "the same every time");
        assert_ne!(a, side_id("tasks:other"));
        assert_ne!(a, side_id("catch-up:11cd1602-0504-4ad0-ad40-7c25f40885aa"));
        let parts: Vec<&str> = a.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), vec![8, 4, 4, 4, 12], "{a}");
        assert!(a.chars().all(|c| c == '-' || c.is_ascii_hexdigit()) && parts[2].starts_with('4'), "{a}");
    }

    #[test]
    fn an_answer_says_what_it_cost_cache_reads_included() {
        let a = answer_full(r#"{"result":"ok","is_error":false,"total_cost_usd":0.0123,"usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":100,"cache_read_input_tokens":2000}}"#).unwrap().unwrap();
        assert_eq!(a.spent, Spent { tokens: 2115, usd: 0.0123 });
        assert_eq!(answer_full(r#"{"result":"ok"}"#).unwrap().unwrap().spent, Spent::default(), "no usage: nothing counted");
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
        assert!(claude_line("x; rm -rf ~", None).is_err());
    }
}
