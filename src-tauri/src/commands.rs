//! The slash commands a Claude Code session can run, for the "/" menu in its message box. Nothing is
//! picked by hand: Claude Code says which are enabled in a folder (its built-ins, your skills and
//! commands, plugins'), with what each does and what it takes, when it's asked to set up over its SDK
//! protocol (`-p --input-format stream-json`, an `initialize` request). That needs no model and starts
//! no conversation. Cue asks, reads the reply and stops it: once per folder, the first time its "/"
//! menu is opened, then kept for a week. Sooner when you add a skill, command or plugin for yourself
//! (a list read before that is out of date), or when you ask: Refresh under the "/" menu, or Reload
//! beside + New's model list, either of which puts every folder's list out of date (`reread_all`).
//! An out-of-date list is still what's shown while Claude Code is asked again, and the new one takes
//! its place when it's in (`reading` says one is on its way). (The handshake runs in the folder, so
//! that folder's Claude settings load then, which is why it isn't done on a timer.)
//!
//! The same reply lists the models and output styles there are, so `/model` and `/output-style` offer
//! them as choices (a bare `/model` opens a picker in its terminal, where Cue can't show it).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Arg {
    pub value: String,
    pub label: String,
    pub desc: String,
}

/// How long a folder's list is kept before it's asked again on its own (a new skill or plugin shows
/// up then, or sooner with Refresh).
const KEEP_MS: u64 = 7 * 24 * 3_600_000;

/// By folder: when it was read, and the list. What's here is also in the database (`db::commands`),
/// where it outlives this run of Cue; this is only the copy in hand.
static LISTS: Mutex<Option<HashMap<String, (u64, Vec<Cmd>)>>> = Mutex::new(None);
/// Folders being read now.
static READING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// The commands for a Claude session in `cwd` on one of your machines (`machine` empty: this Mac), or
/// None while they're read for the first time (ask again shortly). Another machine's are read there,
/// over its connection: the folder isn't on this Mac, and its skills and plugins are the machine's. Kept
/// under "machine:folder", apart from a Mac folder of the same name.
pub fn for_folder(cwd: &str, machine: &str) -> Option<Vec<Cmd>> {
    if cwd.is_empty() {
        return Some(vec![]);
    }
    let key = if machine.is_empty() { cwd.to_string() } else { format!("{machine}:{cwd}") };
    let cwd = key.as_str();
    let now = crate::model::now_ms();
    // Two separate locks, never one inside the other: the guard of a lookup used in a longer expression
    // would still be held while the database's copy is put in.
    let in_hand = crate::locking::lock(&LISTS, "LISTS").get_or_insert_with(HashMap::new).get(cwd).cloned();
    let cached = match in_hand {
        Some(c) => Some(c),
        // Not in hand (Cue was relaunched): what the last run kept, if it's there.
        None => crate::db::commands(cwd).inspect(|kept| {
            crate::locking::lock(&LISTS, "LISTS").get_or_insert_with(HashMap::new).insert(cwd.to_string(), kept.clone());
        }),
    };
    // Out of date after a week, or once you've added something of yours since (on this Mac: another
    // machine's folders are never looked at from here).
    let since = if machine.is_empty() { yours_changed_ms(&std::env::var("HOME").unwrap_or_default()) } else { 0 };
    let fresh = cached.as_ref().is_some_and(|(at, _)| now.saturating_sub(*at) < KEEP_MS && *at > since);
    if !fresh {
        let started = crate::locking::lock(&READING, "READING").get_or_insert_with(HashSet::new).insert(cwd.to_string());
        if started {
            let (cwd, folder, machine) = (cwd.to_string(), key_folder(cwd, machine).to_string(), machine.to_string());
            std::thread::spawn(move || {
                let list = if machine.is_empty() { read_folder(&folder) } else { read_on_machine(&machine, &folder) };
                // A failed read keeps the old list (if any), and is tried again next time.
                if let Some(list) = list {
                    let at = crate::model::now_ms();
                    crate::db::set_commands(&cwd, at, &list);
                    crate::locking::lock(&LISTS, "LISTS").get_or_insert_with(HashMap::new).insert(cwd.clone(), (at, list));
                }
                crate::locking::lock(&READING, "READING").get_or_insert_with(HashSet::new).remove(&cwd);
            });
        }
    }
    cached.map(|(_, list)| list)
}

/// Whether a folder's list (`machine` empty: this Mac) is being read now: the one `for_folder` gave
/// is out of date and a new one is on its way, so ask again shortly to show it.
pub fn reading(cwd: &str, machine: &str) -> bool {
    let key = if machine.is_empty() { cwd.to_string() } else { format!("{machine}:{cwd}") };
    crate::locking::lock(&READING, "READING").get_or_insert_with(HashSet::new).contains(&key)
}

/// When your own skills, commands or plugins on this Mac (under `home`) last changed (a skill added or
/// removed, a plugin installed): they apply in every folder, so a list read before then is out of date.
fn yours_changed_ms(home: &str) -> u64 {
    if home.is_empty() {
        return 0;
    }
    let claude = std::path::Path::new(home).join(".claude");
    [claude.join("skills"), claude.join("commands"), claude.join("plugins").join("installed_plugins.json")]
        .iter()
        .filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .max()
        .unwrap_or(0)
}

/// The folder in a key made by `for_folder`.
fn key_folder<'a>(key: &'a str, machine: &str) -> &'a str {
    if machine.is_empty() { key } else { &key[machine.len() + 1..] }
}

/// The setup request Claude Code answers with its commands.
const SETUP: &str = r#"{"type":"control_request","request_id":"cue-commands","request":{"subtype":"initialize"}}"#;
/// Claude Code taking requests on its input, with nothing of yours (hooks) run and no session kept.
const CLAUDE_SETUP: &str = r#"claude -p --input-format stream-json --output-format stream-json --verbose --no-session-persistence --settings '{"disableAllHooks":true}'"#;

/// Ask Claude Code in `cwd` on a machine: one trip over its connection. It answers the setup request
/// before it sees its input end. Stopped after 30s where `timeout` is there.
fn read_on_machine(machine: &str, cwd: &str) -> Option<Vec<Cmd>> {
    let host = crate::machines::get(machine)?.host;
    let script = format!(
        "cd {} || exit 1; t=; command -v timeout >/dev/null 2>&1 && t='timeout 30'; printf '%s\\n' {} | $t {CLAUDE_SETUP} 2>/dev/null",
        crate::focus::shq(cwd),
        crate::focus::shq(SETUP)
    );
    setup_reply(&crate::machines::run(&host, &script).ok()?)
}

/// The commands in Claude Code's reply to the setup request, from its output lines.
fn setup_reply(out: &str) -> Option<Vec<Cmd>> {
    let r = out.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|v| v.get("type").and_then(Value::as_str) == Some("control_response"))?;
    Some(from_setup(r.pointer("/response/response").unwrap_or(&Value::Null)))
}

/// Ask Claude Code in `cwd` (through your login shell, so it's found as in your terminal).
fn read_folder(cwd: &str) -> Option<Vec<Cmd>> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let line = format!("exec {CLAUDE_SETUP}");
    let mut child = Command::new(shell).args(["-lic", &line]).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let out = child.stdout.take()?;
    let mut input = child.stdin.take()?;
    // Never hang on it: stop it after 30s if it's still going.
    let child = std::sync::Arc::new(Mutex::new(child));
    let watch = child.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        let _ = watch.lock().unwrap().kill();
    });
    let _ = writeln!(input, "{SETUP}");
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

/// The hand-off skill Claude Code has for you on a host (`machine` empty: this Mac): its command named
/// exactly "handoff" (one with "handoff" elsewhere in its name may be something else), from its lists (`on_host`: your home folder's, where skills you install for yourself apply in
/// every folder, and each of its folders' own). None while it's read for the first time. To read them again after adding a skill on
/// another machine, `reread_all` first: every folder's list is read again, so each session's "/" menu
/// and insights see it too.
pub fn handoff_skills(machine: &str) -> Option<Vec<String>> {
    let list = on_host(machine)?;
    Some(list.into_iter().filter(|c| c.name == "handoff").map(|c| c.name).collect())
}

/// Every command Claude Code has on a host (`machine` empty: this Mac): your home folder's (yours, in every
/// folder), then each of its folders' Cue has read (a project's own skills, as its sessions' "/" menus
/// have them), each name once. None while the home folder's is read for the first time.
pub fn on_host(machine: &str) -> Option<Vec<Cmd>> {
    let mut list = for_folder(&home_folder(machine), machine)?;
    let prefix = format!("{machine}:");
    let mine = |k: &str| if machine.is_empty() { k.starts_with('/') } else { k.starts_with(&prefix) };
    let mut seen: HashSet<String> = list.iter().map(|c| c.name.clone()).collect();
    for (_, l) in crate::db::all_commands().into_iter().filter(|(k, _)| mine(k)) {
        for c in l {
            if seen.insert(c.name.clone()) {
                list.push(c);
            }
        }
    }
    Some(list)
}

/// Your home folder on a host: this Mac's, or where a machine's connection starts.
pub fn home_folder(machine: &str) -> String {
    if machine.is_empty() { std::env::var("HOME").unwrap_or_default() } else { ".".to_string() }
}

/// Put one folder's list (`machine` empty: this Mac) out of date, so its next "/" menu asks Claude Code
/// again, showing this one until the answer's in: a restarted session's, which may have skills or
/// plugins added since its list was read.
pub fn reread(cwd: &str, machine: &str) {
    let key = if machine.is_empty() { cwd.to_string() } else { format!("{machine}:{cwd}") };
    if let Some(kept) = crate::locking::lock(&LISTS, "LISTS").get_or_insert_with(HashMap::new).get_mut(&key) {
        kept.0 = 0;
    }
    crate::db::stale_commands(Some(&key));
}

/// Put every folder's list out of date: the next ask, for any folder, reads it from Claude Code again,
/// showing the old one until then. Refresh under a "/" menu and Reload beside + New's model list both do
/// this, so one press brings a new skill, plugin or model everywhere, not in one place.
pub fn reread_all() {
    for kept in crate::locking::lock(&LISTS, "LISTS").get_or_insert_with(HashMap::new).values_mut() {
        kept.0 = 0;
    }
    crate::db::stale_commands(None);
}

/// The models Claude Code offers (the choices /model takes), for picking one before a session starts.
/// Read in `cwd` on a host (`machine` empty: this Mac), whose Claude Code may name them its own way (a
/// proxy's `claude-opus-5-5[1m]`); `fresh` puts every folder's list out of date first, so Claude Code is
/// asked again (`reading` says when that's in). None while it's read for the first time; empty when it
/// lists none.
pub fn models_for(cwd: &str, machine: &str, fresh: bool) -> Option<Vec<Arg>> {
    if fresh {
        reread_all();
    }
    for_folder(cwd, machine).map(|l| l.into_iter().find(|c| c.name == "model").map(|c| c.args).unwrap_or_default())
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

/// A Pi session's "/" menu: Pi's own commands, then what its extension reported (its extensions'
/// commands, prompt templates and skills, as Pi's `getCommands` lists them).
/// SHORTCUT: Pi's built-ins are a fixed list, from pi-coding-agent 0.87.1: Pi doesn't export it to
/// extensions. A command Pi adds later is missing here until this list is updated; typed in full it
/// still runs (the box sends what you type).
pub fn for_pi(reported: &[crate::model::AgentCmd]) -> Vec<Cmd> {
    let fixed = |name: &str, desc: &str, hint: &str| Cmd { name: name.into(), kind: "command".into(), desc: desc.into(), hint: hint.into(), args: vec![] };
    let mut list = vec![
        fixed("settings", "Open settings menu", ""),
        fixed("model", "Select model (opens selector UI)", "<provider/model>"),
        fixed("thinking", "Set thinking level", "<level>"),
        fixed("scoped-models", "Enable/disable models for Ctrl+P cycling", ""),
        fixed("name", "Set session display name", ""),
        fixed("session", "Show session info and stats", ""),
        fixed("tree", "Navigate session tree (switch branches)", ""),
        fixed("fork", "Create a new fork from a previous user message", ""),
        fixed("clone", "Duplicate the current session at the current position", ""),
        fixed("compact", "Manually compact the session context", ""),
        fixed("new", "Start a new session", ""),
        fixed("resume", "Resume a different session", ""),
        fixed("export", "Export session (HTML default, or specify path: .html/.jsonl)", ""),
        fixed("import", "Import and resume a session from a JSONL file", ""),
        fixed("share", "Share session as a secret GitHub gist", ""),
        fixed("copy", "Copy last agent message to clipboard", ""),
        fixed("trust", "Save project trust decision for future sessions", ""),
        fixed("login", "Configure provider authentication", "<provider>"),
        fixed("logout", "Remove provider authentication", ""),
        fixed("reload", "Reload keybindings, extensions, skills, prompts, themes, and context files", ""),
        fixed("changelog", "Show changelog entries", ""),
        fixed("hotkeys", "Show all keyboard shortcuts", ""),
        fixed("bug", "Report a bug to the Pi developers", "<description>"),
        fixed("quit", "Quit Pi", ""),
    ];
    for c in reported {
        let name = c.name.trim().trim_start_matches('/');
        if name.is_empty() || list.iter().any(|k| k.name == name) {
            continue;
        }
        // A prompt template or a skill becomes a prompt (a turn starts); an extension's command runs itself.
        let kind = if c.source == "extension" { "command" } else { "skill" };
        list.push(Cmd { name: name.to_string(), kind: kind.into(), desc: c.description.trim().to_string(), hint: String::new(), args: vec![] });
    }
    list
}

/// A Codex session's "/" menu.
/// SHORTCUT: a fixed list, from Codex's own source (codex-rs/tui/src/slash_command.rs) as of October
/// 2026, without its debug-only and platform-hidden entries: Codex has no way to ask for the list. A
/// command Codex adds later is missing here until this list is updated; typed in full it still runs.
pub fn for_codex() -> Vec<Cmd> {
    let fixed = |name: &str, desc: &str| Cmd { name: name.into(), kind: "command".into(), desc: desc.into(), hint: String::new(), args: vec![] };
    vec![
        fixed("model", "Choose what model and reasoning effort to use"),
        fixed("permissions", "Choose what Codex is allowed to do"),
        fixed("review", "Review my current changes and find issues"),
        fixed("plan", "Switch to Plan mode"),
        fixed("compact", "Summarize conversation to prevent hitting the context limit"),
        fixed("recap", "Summarize the current conversation now"),
        fixed("rename", "Rename the current thread"),
        fixed("new", "Start a new chat during a conversation"),
        fixed("resume", "Resume a saved chat"),
        fixed("fork", "Fork the current chat"),
        fixed("worktree", "Start or continue a conversation in a new worktree"),
        fixed("archive", "Archive this session"),
        fixed("init", "Create an AGENTS.md file with instructions for Codex"),
        fixed("goal", "Set or view the goal for a long-running task"),
        fixed("side", "Start a side conversation in an ephemeral fork"),
        fixed("btw", "Start a side conversation in an ephemeral fork"),
        fixed("agents", "Open the agent command center"),
        fixed("subagents", "Switch between this session's subagents"),
        fixed("skills", "Use skills to improve how Codex performs specific tasks"),
        fixed("hooks", "View and manage lifecycle hooks"),
        fixed("memories", "Configure memory use and generation"),
        fixed("mcp", "List MCP tools; use /mcp verbose or /mcp login <name>"),
        fixed("apps", "Manage apps"),
        fixed("plugins", "Browse plugins"),
        fixed("diff", "Show git diff (including untracked files)"),
        fixed("mention", "Mention a file"),
        fixed("status", "Show current session configuration and token usage"),
        fixed("usage", "View account usage or use a usage limit reset"),
        fixed("export", "Export the conversation as markdown"),
        fixed("copy", "Copy the last response or part of it"),
        fixed("cd", "Change the current working directory"),
        fixed("pwd", "Show the current working directory"),
        fixed("ps", "List background terminals"),
        fixed("stop", "Stop all background terminals"),
        fixed("voice", "Start or stop voice; use /voice settings to choose a voice"),
        fixed("theme", "Choose a syntax highlighting theme"),
        fixed("experimental", "Toggle experimental features"),
        fixed("feedback", "Send logs to maintainers"),
        fixed("clear", "Clear the terminal and start a new chat"),
        fixed("logout", "Log out of Codex"),
        fixed("quit", "Exit Codex"),
    ]
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
    fn pi_and_codex_lists_are_pis_own_plus_what_its_extension_reports_and_codexs_own() {
        let reported = vec![
            crate::model::AgentCmd { name: "pilead:review".into(), description: "Review an executor".into(), source: "extension".into() },
            crate::model::AgentCmd { name: "/tidy".into(), description: "A prompt template".into(), source: "prompt".into() },
            crate::model::AgentCmd { name: "compact".into(), description: "Pi has this itself".into(), source: "extension".into() },
            crate::model::AgentCmd { name: " ".into(), description: String::new(), source: "skill".into() },
        ];
        let pi = for_pi(&reported);
        assert_eq!(pi.iter().filter(|c| c.name == "compact").count(), 1, "Pi's own wins, once");
        let ext = pi.iter().find(|c| c.name == "pilead:review").unwrap();
        assert_eq!((ext.kind.as_str(), ext.desc.as_str()), ("command", "Review an executor"));
        assert_eq!(pi.iter().find(|c| c.name == "tidy").unwrap().kind, "skill", "a template is a prompt, the slash dropped");
        assert!(pi.iter().all(|c| !c.name.trim().is_empty()));
        assert!(for_pi(&[]).iter().any(|c| c.name == "model" && c.hint == "<provider/model>"));
        let codex = for_codex();
        assert!(codex.iter().any(|c| c.name == "review") && codex.iter().all(|c| c.kind == "command"));
    }

    #[test]
    fn a_list_kept_from_an_earlier_run_is_used_without_asking_again() {
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-cmds-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let kept = vec![Cmd { name: "merge-main".into(), kind: "skill".into(), desc: "Bring main in".into(), hint: String::new(), args: vec![] }];
        crate::db::set_commands("/x/kept", crate::model::now_ms(), &kept);
        // On the calling thread, as the window's thread calls it: it answers from the file, at once,
        // without the lock being taken inside its own lookup (that hung Cue once).
        assert_eq!(for_folder("/x/kept", ""), Some(kept.clone()));
        assert_eq!(for_folder("/x/kept", ""), Some(kept), "and from memory the second time");
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_out_of_date_list_is_still_given_until_the_new_one_is_in() {
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-cmds-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let kept = vec![Cmd { name: "compact".into(), kind: "command".into(), desc: "Summarize".into(), hint: String::new(), args: vec![] }];
        crate::db::set_commands("/x/stale", crate::model::now_ms(), &kept);
        assert_eq!(for_folder("/x/stale", ""), Some(kept.clone()));
        reread_all();
        // Asked again (the folder isn't there, so that read fails), and the old list is what's given meanwhile.
        assert_eq!(for_folder("/x/stale", ""), Some(kept.clone()), "Refresh doesn't empty the menu");
        assert_eq!(crate::db::commands("/x/stale"), Some((0, kept.clone())), "kept, out of date");
        reread("/x/stale", "");
        assert_eq!(for_folder("/x/stale", ""), Some(kept));
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_skill_added_for_yourself_puts_lists_read_before_it_out_of_date() {
        let home = std::env::temp_dir().join(format!("cue-cmds-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(yours_changed_ms(home.to_str().unwrap()), 0, "nothing of yours yet");
        let before = crate::model::now_ms() - 1_000;
        std::fs::create_dir_all(home.join(".claude/skills/handoff")).unwrap();
        assert!(yours_changed_ms(home.to_str().unwrap()) > before, "the skills folder changed when one was added");
        assert_eq!(yours_changed_ms(""), 0);
        let _ = std::fs::remove_dir_all(&home);
    }

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

    /// On a real machine, by hand: CUE_HOME=<a Cue folder listing it> CUE_MACHINE=name CUE_MACHINE_DIR=folder
    /// cargo test -q commands_from_a_machine -- --ignored --nocapture. The list comes from Claude Code there,
    /// in that folder (a name with a space in it is fine).
    #[test]
    #[ignore]
    fn commands_from_a_machine() {
        let (Ok(m), Ok(dir)) = (std::env::var("CUE_MACHINE"), std::env::var("CUE_MACHINE_DIR")) else { return };
        let list = read_on_machine(&m, &dir).expect("Claude Code there answered");
        println!("{} commands: {:?}", list.len(), list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>());
        let model = list.iter().find(|c| c.name == "model").expect("/model is there");
        assert!(!model.args.is_empty(), "with its models to pick from");
    }
}
