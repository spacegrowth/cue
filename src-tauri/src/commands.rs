//! The slash commands a Claude Code session can run, for the "/" menu in its message box. Nothing is
//! picked by hand: Claude Code says which are enabled in a folder (its built-ins, your skills and
//! commands, plugins'), with what each does and what it takes, when it's asked to set up over its SDK
//! protocol (`-p --input-format stream-json`, an `initialize` request). That needs no model and starts
//! no conversation. Cue asks, reads the reply and stops it. Once per folder, again after 10 minutes.
//!
//! The same reply lists the models and output styles there are, so `/model` and `/output-style` offer
//! them as choices (a bare `/model` opens a picker in its terminal, where Cue can't show it).

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cmd {
    pub name: String,
    /// "skill": it becomes a prompt (a turn starts, like a message). "command": Claude Code runs it itself.
    pub kind: String,
    pub desc: String,
    /// What it takes, in Claude Code's words ("[name]", "<model>"), else "".
    pub hint: String,
    /// The values it takes, when there's a set to pick from ("/model sonnet", "/effort high").
    pub args: Vec<Arg>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Arg {
    pub value: String,
    pub label: String,
    pub desc: String,
}

/// How long a folder's list is kept before it's asked again (a new skill or plugin shows up then).
const KEEP_MS: u64 = 10 * 60_000;

/// By folder: when it was read, and the list.
static LISTS: Mutex<Option<HashMap<String, (u64, Vec<Cmd>)>>> = Mutex::new(None);
/// Folders being read now.
static READING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// The commands for a Claude session in `cwd`, or None while they're being read (ask again shortly).
pub fn for_session(cwd: &str) -> Option<Vec<Cmd>> {
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
    cached.map(|(_, list)| list)
}

/// Ask Claude Code in `cwd` (through your login shell, so it's found as in your terminal).
fn read_folder(cwd: &str) -> Option<Vec<Cmd>> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let line = r#"exec claude -p --input-format stream-json --output-format stream-json --verbose --no-session-persistence --settings '{"disableAllHooks":true}'"#;
    let mut child = Command::new(shell).args(["-lic", line]).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let out = child.stdout.take()?;
    let mut input = child.stdin.take()?;
    // Never hang on it: stop it after 30s if it's still going.
    let child = std::sync::Arc::new(Mutex::new(child));
    let watch = child.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        let _ = watch.lock().unwrap().kill();
    });
    let _ = writeln!(input, r#"{{"type":"control_request","request_id":"cue-commands","request":{{"subtype":"initialize"}}}}"#);
    let reply = BufReader::new(out)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<Value>(&l).ok())
        .find(|v| v.get("type").and_then(Value::as_str) == Some("control_response"));
    drop(input);
    {
        let mut c = child.lock().unwrap();
        let _ = c.kill();
        let _ = c.wait();
    }
    let r = reply?;
    Some(from_setup(r.pointer("/response/response").unwrap_or(&Value::Null)))
}

/// The list from Claude Code's setup reply: `commands` (name, description, argumentHint, builtin), with
/// `models` and `available_output_styles` as the choices for /model and /output-style.
fn from_setup(r: &Value) -> Vec<Cmd> {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let models: Vec<Arg> = r.get("models").and_then(Value::as_array).into_iter().flatten()
        .map(|m| Arg { value: s(m, "value"), label: s(m, "displayName"), desc: s(m, "description") })
        .filter(|a| !a.value.is_empty())
        .collect();
    let styles: Vec<Arg> = r.get("available_output_styles").and_then(Value::as_array).into_iter().flatten()
        .filter_map(Value::as_str)
        .map(|v| Arg { value: v.into(), label: v.into(), desc: String::new() })
        .collect();
    r.get("commands").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|c| {
            let name = s(c, "name");
            // Claude Code's own plumbing ("__remote-workflow"): not for you to type.
            if name.is_empty() || name.starts_with('_') {
                return None;
            }
            let hint = s(c, "argumentHint");
            let args = match name.as_str() {
                "model" => models.clone(),
                "output-style" => styles.clone(),
                _ => choices(&hint),
            };
            let builtin = c.get("builtin").and_then(Value::as_bool) == Some(true);
            Some(Cmd { kind: if builtin { "command" } else { "skill" }.into(), desc: s(c, "description"), hint, args, name })
        })
        .collect()
}

/// The values in an argument hint that lists them ("[on|off]", "<low|medium|high>"); a free-text hint
/// ("[name]") or a value with more after it ("ultracode [on|off]") isn't one to pick.
fn choices(hint: &str) -> Vec<Arg> {
    // One pair of brackets around the whole hint, no more: the "]" of "[on|off]" inside it stays.
    let h = hint.trim();
    let inner = h.strip_prefix('[').and_then(|x| x.strip_suffix(']')).or_else(|| h.strip_prefix('<').and_then(|x| x.strip_suffix('>'))).unwrap_or(h);
    if !inner.contains('|') {
        return vec![];
    }
    inner.split('|')
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .map(|v| Arg { value: v.into(), label: v.into(), desc: String::new() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_claude_codes_setup_reply() {
        let r = serde_json::json!({
            "commands": [
                { "name": "model", "description": "Set the AI model", "argumentHint": "<model>", "builtin": true },
                { "name": "effort", "description": "Set effort", "argumentHint": "<low|medium|high|ultracode [on|off]>", "builtin": true },
                { "name": "rename", "description": "Rename it", "argumentHint": "[name]", "builtin": true },
                { "name": "__remote-workflow", "description": "", "argumentHint": "", "builtin": true },
                { "name": "merge-main", "description": "Bring main in (user)", "argumentHint": "" }
            ],
            "models": [{ "value": "sonnet", "displayName": "Sonnet 5.5", "description": "Most efficient" }],
            "available_output_styles": ["default", "Concise"]
        });
        let got = from_setup(&r);
        let row = |n: &str| got.iter().find(|c| c.name == n).cloned().unwrap();
        assert_eq!(got.len(), 4, "plumbing left out: {got:?}");
        assert_eq!(row("model").args, vec![Arg { value: "sonnet".into(), label: "Sonnet 5.5".into(), desc: "Most efficient".into() }]);
        assert_eq!(row("effort").args.iter().map(|a| a.value.as_str()).collect::<Vec<_>>(), ["low", "medium", "high"]);
        assert!(row("rename").args.is_empty() && row("rename").hint == "[name]");
        assert_eq!((row("merge-main").kind.as_str(), row("model").kind.as_str()), ("skill", "command"));
    }
}
