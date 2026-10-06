//! The slash commands a Claude Code session can run, for the "/" menu in its message box. Nothing is
//! picked by hand: Claude Code says which are enabled in a folder (its built-ins, your skills and
//! commands, plugins'), in the setup line it prints first when run with `-p --output-format
//! stream-json`. Cue runs it with `/usage`, a command that needs no model (no tokens, nothing
//! saved), reads that line and stops it. Once per folder, again after 10 minutes.
//!
//! What each skill does comes from the session's transcript: Claude Code lists them there, one line
//! each. Built-ins get Cue's own short line.

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cmd {
    pub name: String,
    /// "skill": it becomes a prompt (a turn starts, like a message). "command": Claude Code runs it itself.
    pub kind: String,
    pub desc: String,
}

/// How long a folder's list is kept before it's asked again (a new skill or plugin shows up then).
const KEEP_MS: u64 = 10 * 60_000;

/// By folder: when it was read, and the list.
static LISTS: Mutex<Option<HashMap<String, (u64, Vec<Cmd>)>>> = Mutex::new(None);
/// Folders being read now.
static READING: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// Skill descriptions by transcript: (its size when read, name -> line).
static DESCS: Mutex<Option<HashMap<String, (u64, HashMap<String, String>)>>> = Mutex::new(None);

/// The commands for a Claude session in `cwd`, or None while they're being read (ask again shortly).
pub fn for_session(cwd: &str, transcript: &str) -> Option<Vec<Cmd>> {
    if cwd.is_empty() {
        return Some(vec![]);
    }
    let now = crate::model::now_ms();
    let cached = LISTS.lock().unwrap().get_or_insert_with(HashMap::new).get(cwd).cloned();
    let fresh = cached.as_ref().is_some_and(|(at, _)| now.saturating_sub(*at) < KEEP_MS);
    if !fresh {
        let started = READING.lock().unwrap().get_or_insert_with(HashSet::new).insert(cwd.to_string());
        if started {
            let cwd = cwd.to_string();
            std::thread::spawn(move || {
                let list = read_folder(&cwd);
                // A failed read keeps the old list (if any), and is tried again next time.
                if let Some(list) = list {
                    LISTS.lock().unwrap().get_or_insert_with(HashMap::new).insert(cwd.clone(), (crate::model::now_ms(), list));
                }
                READING.lock().unwrap().get_or_insert_with(HashSet::new).remove(&cwd);
            });
        }
    }
    let (_, list) = cached?;
    let descs = skill_descs(transcript);
    Some(list.into_iter().map(|mut c| {
        if let Some(d) = descs.get(&c.name) {
            c.desc = d.clone();
        }
        c
    }).collect())
}

/// Ask Claude Code in `cwd` (through your login shell, so it's found as in your terminal).
fn read_folder(cwd: &str) -> Option<Vec<Cmd>> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let line = r#"exec claude -p /usage --output-format stream-json --verbose --no-session-persistence --settings '{"disableAllHooks":true}'"#;
    let mut child = Command::new(shell).args(["-lic", line]).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let out = child.stdout.take()?;
    // Never hang on it: stop it after 30s if it's still going.
    let child = std::sync::Arc::new(Mutex::new(child));
    let watch = child.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        let _ = watch.lock().unwrap().kill();
    });
    let init = BufReader::new(out).lines().map_while(Result::ok).filter_map(|l| serde_json::from_str::<Value>(&l).ok()).find(|v| v.get("subtype").and_then(Value::as_str) == Some("init"));
    {
        let mut c = child.lock().unwrap();
        let _ = c.kill();
        let _ = c.wait();
    }
    Some(from_init(&init?))
}

/// The list from Claude Code's setup line: `slash_commands`, with `skills` marking which are skills.
fn from_init(init: &Value) -> Vec<Cmd> {
    let names = |k: &str| -> Vec<String> { init.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default() };
    let skills: HashSet<String> = names("skills").into_iter().collect();
    names("slash_commands")
        .into_iter()
        // Claude Code's own plumbing ("__remote-workflow"): not for you to type.
        .filter(|n| !n.starts_with('_'))
        .map(|n| {
            let skill = skills.contains(&n);
            Cmd { desc: if skill { String::new() } else { builtin_desc(&n).to_string() }, kind: if skill { "skill" } else { "command" }.into(), name: n }
        })
        .collect()
}

/// What each skill does, from the session's transcript ("- name: what it does", one per line, in its
/// skill listing). Read again when the transcript has grown.
fn skill_descs(transcript: &str) -> HashMap<String, String> {
    let Ok(size) = std::fs::metadata(transcript).map(|m| m.len()) else { return HashMap::new() };
    let mut cache = DESCS.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((at, d)) = cache.get(transcript) {
        // A listing is written when the session starts (and when skills change): growing alone needn't re-read.
        if *at == size || size.saturating_sub(*at) < 5_000_000 {
            return d.clone();
        }
    }
    let mut d = HashMap::new();
    if let Ok(f) = std::fs::File::open(transcript) {
        for l in BufReader::new(f).lines().map_while(Result::ok).filter(|l| l.contains("\"skill_listing\"")) {
            let Ok(v) = serde_json::from_str::<Value>(&l) else { continue };
            if let Some(text) = v.pointer("/attachment/content").and_then(Value::as_str) {
                d.extend(parse_listing(text));
            }
        }
    }
    cache.insert(transcript.to_string(), (size, d.clone()));
    d
}

fn parse_listing(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("- "))
        .filter_map(|l| l.split_once(": "))
        .map(|(n, d)| (n.trim().to_string(), d.trim().to_string()))
        .collect()
}

/// A line for Claude Code's built-ins (unknown ones show none).
fn builtin_desc(name: &str) -> &'static str {
    match name {
        "compact" => "Summarize the conversation to free up context",
        "clear" => "Start over: clear the conversation",
        "context" => "How full the context is",
        "model" => "Switch the model (add its name, e.g. /model sonnet)",
        "effort" => "How hard it thinks (add low, medium or high)",
        "fast" => "Fast mode on or off",
        "rename" => "Name this session",
        "usage" => "Your plan's usage and limits",
        "init" => "Write a CLAUDE.md for this project",
        "review" | "code-review" => "Review code changes",
        "security-review" => "Look for security problems in the changes",
        "config" => "Settings (opens in its terminal)",
        "agents" => "Manage subagents (opens in its terminal)",
        "mcp" => "MCP servers (opens in its terminal)",
        "output-style" => "How it writes its answers",
        "color" => "This session's color",
        "recap" => "A recap of the session so far",
        "insights" => "A report on how you use Claude Code",
        "doctor" => "Check Claude Code's install",
        "reload-plugins" => "Load plugin changes",
        "reload-skills" => "Load skill changes",
        "autocompact" => "Automatic compacting on or off",
        "goal" => "Set a goal it works toward",
        "advisor" => "Ask a stronger model for advice",
        "loop" => "Repeat a prompt on an interval",
        "schedule" => "Run a prompt on a schedule",
        "batch" => "Run a task across many files in parallel",
        "ultrareview" => "A multi-agent review of this branch",
        "extra-usage" | "usage-credits" => "Extra usage",
        "heapdump" => "Save a memory snapshot (for debugging Claude Code)",
        "import" => "Import from another tool",
        "focus" => "Focus mode",
        "list-agents" => "List the agents you can message",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_claude_codes_setup_line() {
        let init = serde_json::json!({ "type": "system", "subtype": "init", "slash_commands": ["compact", "relay:spawn", "__remote-workflow", "mystery"], "skills": ["relay:spawn"] });
        let got = from_init(&init);
        let row = |n: &str| got.iter().find(|c| c.name == n).cloned();
        assert_eq!(got.len(), 3, "plumbing left out: {got:?}");
        assert_eq!(row("compact").map(|c| (c.kind, c.desc.is_empty())), Some(("command".into(), false)));
        assert_eq!(row("relay:spawn").map(|c| c.kind), Some("skill".into()));
        assert_eq!(row("mystery").map(|c| c.desc), Some(String::new()));
    }

    #[test]
    fn reads_skill_lines() {
        let d = parse_listing("- relay:spawn: Start an executor: one: two\n- merge-main: Bring main in\nnot a row");
        assert_eq!(d.get("relay:spawn").map(String::as_str), Some("Start an executor: one: two"));
        assert_eq!(d.get("merge-main").map(String::as_str), Some("Bring main in"));
        assert_eq!(d.len(), 2);
    }
}
