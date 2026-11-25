//! New Desk's look at a folder's past sessions (#927, desk/look.rs): Claude Code's transcripts of the sessions that
//! worked there, newest first, read for the commands they ran: how they built and tested, whether their work went in
//! as pull requests or merges, whether they pushed what they landed, and what they ran once it had landed. Each
//! session goes by Cue's name for it when Cue has one. Only reads.

use crate::desk::look::{kind_of, transcripts_dir};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How many past sessions are read, newest first.
const TAKE: usize = 40;
/// The longest the reading may take: past it, the sessions read so far are what's found.
const CAP: Duration = Duration::from_secs(6);

/// Where Claude Code keeps its transcripts, a folder per project folder.
pub fn projects_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(&home).join(".claude/projects")
}

/// What one past session did.
#[derive(Default)]
struct Seen {
    name: String,
    /// Build, test and lint commands (as grouped by `short`), with what they're for.
    cmds: Vec<(String, &'static str)>,
    pushed: bool,
    pr_create: bool,
    pr_merge: bool,
    merged: bool,
    direct: bool,
    landed: bool,
    pushed_after_landing: bool,
    after: Vec<String>,
}

/// The past sessions in `dir`, from the transcripts under `root`: { count, sessions, commands, pushed, landed,
/// pushed_after_landing, pushed_names, pr_create, pr_names, pr_merge, merged, direct_commits, after }.
pub fn sessions(root: &Path, dir: &str, names: &dyn Fn(&str) -> Option<String>) -> (Vec<Value>, Value) {
    let folder = transcripts_dir(root, dir);
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&folder)
        .map(|r| r.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p))).collect())
        .unwrap_or_default();
    if files.is_empty() {
        return (vec![json!({ "text": "No past sessions here" })], json!({ "count": 0 }));
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    let until = Instant::now() + CAP;
    let mut seen: Vec<Seen> = vec![];
    for (_, p) in files.iter().take(TAKE) {
        if Instant::now() >= until {
            break;
        }
        if let Some(s) = read_one(p, names, until) {
            seen.push(s);
        }
    }

    let names_of = |f: &dyn Fn(&Seen) -> bool| -> Vec<String> { seen.iter().filter(|s| f(s)).map(|s| s.name.clone()).collect() };
    // Each command with the sessions that ran it.
    let mut by_cmd: BTreeMap<String, (&'static str, Vec<String>)> = BTreeMap::new();
    for s in &seen {
        for (c, k) in &s.cmds {
            let e = by_cmd.entry(c.clone()).or_insert((k, vec![]));
            if !e.1.contains(&s.name) {
                e.1.push(s.name.clone());
            }
        }
    }
    let commands: Vec<Value> = by_cmd.iter().map(|(c, (k, ns))| json!({ "kind": k, "cmd": c, "names": ns, "count": ns.len() })).collect();
    let mut after: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for s in &seen {
        for c in &s.after {
            let e = after.entry(c.clone()).or_default();
            if !e.contains(&s.name) {
                e.push(s.name.clone());
            }
        }
    }
    let after: Vec<Value> = after.iter().map(|(c, ns)| json!({ "cmd": c, "names": ns, "count": ns.len() })).collect();
    let tested = seen.iter().filter(|s| s.cmds.iter().any(|(_, k)| matches!(*k, "test" | "lint" | "typecheck" | "check"))).count();
    let pushed = seen.iter().filter(|s| s.pushed).count();
    let n = seen.len();
    let mut lines = vec![json!({ "text": format!("{n} past session{} here: {tested} ran tests, {pushed} pushed", if n == 1 { "" } else { "s" }) })];
    if !after.is_empty() {
        lines.push(json!({ "text": format!("Ran after landing: {}", after.iter().filter_map(|a| a["cmd"].as_str()).take(3).collect::<Vec<_>>().join(", ")) }));
    }
    let found = json!({
        "count": n,
        "sessions": seen.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "commands": commands,
        "pushed": pushed,
        "landed": seen.iter().filter(|s| s.landed).count(),
        "pushed_after_landing": seen.iter().filter(|s| s.pushed_after_landing).count(),
        "pushed_names": names_of(&|s| s.pushed_after_landing),
        "pr_create": seen.iter().filter(|s| s.pr_create).count(),
        "pr_names": names_of(&|s| s.pr_create),
        "pr_merge": seen.iter().filter(|s| s.pr_merge).count(),
        "merged": seen.iter().filter(|s| s.merged).count(),
        "direct_commits": seen.iter().filter(|s| s.direct).count(),
        "after": after,
    });
    (lines, found)
}

/// One transcript, read line by line (they can be large): None when it can't be opened or ran nothing.
fn read_one(p: &Path, names: &dyn Fn(&str) -> Option<String>, until: Instant) -> Option<Seen> {
    let id = p.file_stem()?.to_string_lossy().to_string();
    use std::io::{Read, Seek};
    let mut f = std::fs::File::open(p).ok()?;
    let len = f.metadata().ok()?.len();
    let mut s = Seen::default();
    let (mut custom, mut ai, mut first) = (String::new(), String::new(), String::new());
    let mut buf: Vec<u8> = vec![];
    // A long transcript: its head (the title, the first prompt) and its tail (the latest work), not all of it.
    let mut parts: Vec<(u64, u64)> = vec![(0, len.min(HEAD))];
    if len > HEAD + TAIL {
        parts.push((len - TAIL, TAIL));
    } else if len > HEAD {
        parts[0].1 = len;
    }
    for (at, take) in parts {
        if f.seek(std::io::SeekFrom::Start(at)).is_err() {
            break;
        }
        let mut r = std::io::BufReader::new((&mut f).take(take));
        if at > 0 {
            // The line cut in two at the start.
            buf.clear();
            let _ = r.read_until(b'\n', &mut buf);
        }
        while Instant::now() < until {
            buf.clear();
            match r.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if buf.len() <= LINE_MAX {
                see(&String::from_utf8_lossy(&buf), &mut s, &mut custom, &mut ai, &mut first);
            }
        }
    }
    s.name = names(&id).filter(|n| !n.trim().is_empty()).or((!custom.is_empty()).then_some(custom)).or((!ai.is_empty()).then_some(ai)).or((!first.is_empty()).then_some(first)).unwrap_or_else(|| id.chars().take(8).collect());
    Some(s)
}

/// How much of a long transcript's start is read.
const HEAD: u64 = 4 << 20;
/// How much of a long transcript's end is read.
const TAIL: u64 = 24 << 20;
/// A line longer than this (a file's whole text in a tool's answer) is passed over.
const LINE_MAX: usize = 1 << 20;

/// One transcript line: a title, the first prompt, or a Bash call.
fn see(l: &str, s: &mut Seen, custom: &mut String, ai: &mut String, first: &mut String) {
    if l.contains("\"custom-title\"") || l.contains("\"ai-title\"") || l.contains("\"type\":\"summary\"") {
        let v: Value = serde_json::from_str(l).unwrap_or(Value::Null);
        if let Some(t) = v["customTitle"].as_str().filter(|t| !t.trim().is_empty()) {
            *custom = t.trim().to_string();
        } else if let Some(t) = v["aiTitle"].as_str().or(v["summary"].as_str()).filter(|t| !t.trim().is_empty()) {
            *ai = t.trim().to_string();
        }
        return;
    }
    if first.is_empty() && l.contains("\"type\":\"user\"") && !l.contains("\"tool_result\"") {
        let v: Value = serde_json::from_str(l).unwrap_or(Value::Null);
        if v["type"] == "user" {
            let text = match &v["message"]["content"] {
                Value::String(t) => t.clone(),
                Value::Array(a) => a.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join(" "),
                _ => String::new(),
            };
            let t = text.trim();
            if !t.is_empty() && !t.starts_with('<') {
                *first = t.split_whitespace().take(6).collect::<Vec<_>>().join(" ");
            }
        }
        return;
    }
    if !(l.contains("\"tool_use\"") && l.contains("\"Bash\"")) {
        return;
    }
    let v: Value = serde_json::from_str(l).unwrap_or(Value::Null);
    let branch = v["gitBranch"].as_str().unwrap_or("");
    for c in v["message"]["content"].as_array().into_iter().flatten() {
        if c["type"] == "tool_use" && c["name"] == "Bash" {
            if let Some(cmd) = c["input"]["command"].as_str() {
                take(s, cmd, branch);
            }
        }
    }
}

/// The simple commands in a command line: split at `&&`, `||`, `;`, `|` and new lines, a leading `VAR=value` taken
/// off, `cd` and blank ones left out.
pub fn simple_commands(line: &str) -> Vec<String> {
    let l = line.replace("&&", "\n").replace("||", "\n");
    l.split(['\n', ';', '|'])
        .map(|seg| {
            let mut words: Vec<&str> = seg.split_whitespace().skip_while(|w| w.contains('=') && !w.starts_with('-') && w.split('=').next().is_some_and(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))).collect();
            // What only wraps the command: `timeout 300 npm test`, `do cargo test` in a loop, `time make`.
            loop {
                match words.first().copied() {
                    Some("timeout" | "gtimeout") if words.len() > 2 => words.drain(..2),
                    Some("do" | "then" | "else" | "time" | "exec" | "nice" | "env" | "{" | "(") if words.len() > 1 => words.drain(..1),
                    _ => break,
                };
            }
            words.into_iter().filter(|w| !matches!(*w, "2>&1" | ">/dev/null" | "2>/dev/null")).collect::<Vec<_>>().join(" ")
        })
        .filter(|s| !s.is_empty() && !s.starts_with("cd ") && s != "cd" && s.len() <= 200)
        .collect()
}

/// `cmd` cut to the words that say what it is ("cargo test --lib desk::look" → "cargo test"), so runs of one
/// command group together.
pub fn short(cmd: &str) -> String {
    let words: Vec<&str> = cmd.split_whitespace().collect();
    let kind = kind_of(cmd);
    for i in 0..words.len() {
        let pre = words[..=i].join(" ");
        if kind_of(&pre) == kind {
            return pre;
        }
    }
    cmd.to_string()
}

/// Where git's own command is in `w` ("push" in `git -C web push`): past git's switches and their values
/// (`-C dir`, `-c k=v`, `--git-dir x`, `--work-tree x`). None when `w` doesn't run git.
pub fn git_command(w: &[&str]) -> Option<usize> {
    let g = w.iter().position(|x| *x == "git" || x.ends_with("/git"))?;
    let mut i = g + 1;
    while i < w.len() {
        match w[i] {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--exec-path" => i += 2,
            x if x.starts_with('-') => i += 1,
            _ => return Some(i),
        }
    }
    None
}

/// The branch a `git merge` takes in, from the words after `merge`: past its switches and the values of `-m`, `-X`,
/// `-s`, `--strategy`, `--strategy-option`, `-F`, `--file`.
fn merge_source<'a>(w: &[&'a str]) -> &'a str {
    let mut i = 0;
    while i < w.len() {
        match w[i] {
            "-m" | "-X" | "-s" | "--strategy" | "--strategy-option" | "-F" | "--file" => {
                // A quoted value ("-m \"Sync main\"") is split on its spaces: skip to the word that closes its quote.
                let q = w.get(i + 1).and_then(|v| v.chars().next()).filter(|c| *c == '"' || *c == '\'');
                i += 1;
                if let Some(q) = q {
                    let first = w[i];
                    if !(first.len() > 1 && first.ends_with(q)) {
                        i += 1;
                        while i < w.len() && !w[i].ends_with(q) {
                            i += 1;
                        }
                    }
                }
                i += 1;
            }
            x if x.starts_with('-') => i += 1,
            x => return x,
        }
    }
    ""
}

/// What one Bash command of a session did, in order, on branch `branch`.
fn take(s: &mut Seen, line: &str, branch: &str) {
    for c in simple_commands(line) {
        let w: Vec<&str> = c.split_whitespace().collect();
        let git_at = git_command(&w);
        let git_sub = git_at.map(|i| w[i]);
        let gh = w.first() == Some(&"gh");
        if git_sub == Some("push") {
            s.pushed = true;
            if s.landed {
                s.pushed_after_landing = true;
            }
            continue;
        }
        if gh && w.get(1) == Some(&"pr") && w.get(2) == Some(&"create") {
            s.pr_create = true;
            continue;
        }
        if gh && w.get(1) == Some(&"pr") && w.get(2) == Some(&"merge") {
            s.pr_merge = true;
            s.landed = true;
            continue;
        }
        if git_sub == Some("merge") && !c.contains("--abort") && !c.contains("--continue") {
            // Work merged into main lands; a merge on any other branch, or main caught up with GitHub's
            // (`git merge origin/main`), doesn't.
            let from = git_at.map(|i| merge_source(&w[i + 1..])).unwrap_or("");
            let catch_up = matches!(from, "main" | "master") || from.starts_with("origin/");
            if matches!(branch, "main" | "master") && !catch_up {
                s.merged = true;
                s.landed = true;
            }
            continue;
        }
        if git_sub == Some("commit") {
            if matches!(branch, "main" | "master") {
                s.direct = true;
                s.landed = true;
            }
            continue;
        }
        if git_sub.is_some() || gh {
            continue;
        }
        match kind_of(&c) {
            Some("deploy") if s.landed => {
                let sh = short(&c);
                if !s.after.contains(&sh) {
                    s.after.push(sh);
                }
            }
            Some(k) if k != "deploy" => {
                let sh = short(&c);
                if !s.cmds.iter().any(|(x, _)| *x == sh) {
                    s.cmds.push((sh, k));
                }
            }
            _ => {}
        }
    }
}
