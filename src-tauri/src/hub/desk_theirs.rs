//! Other sessions' changes in an item's files, kept out of its commit. A file the item changed that another session
//! (here, not the item's holder) has changed too since it was last committed is committed with only the item's part:
//! the other session's edits, read from its log, taken back out of what's committed (newest first), while the file
//! itself stays as it is, so their edits stay there uncommitted. When their part can't be told apart (they wrote the
//! whole file, a shell command wrote it, or their text isn't found once and only once), the commit waits for it.
//! The item's part is what its own sessions' logs show (its holder's, its former holders', their crews'): a change
//! no log shows is nobody's, left out too, or waited on as "someone else also changed <path>" (#248).

use super::*;
use crate::desk::DeskItem;
use serde_json::Value;

/// One change a session made to a file: when, who (as the Desk shows it), and what. `old`/`new` are an Edit's
/// strings (`all`: its replace_all); `blind` is a change whose text isn't known (Write, NotebookEdit, a shell command).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TheirEdit {
    at: u64,
    by: String,
    old: String,
    new: String,
    all: bool,
    blind: bool,
}

/// Changes read from sessions' logs, by file (absolute): other sessions' (`edits`), and the item's own (`mine`: its
/// holder's, its former holders' and their crews'), with when the item's sessions last ran a command in a folder
/// (`ran`). `mine_known`: each of the item's sessions has its log here, so a change none of them shows isn't the
/// item's (#248).
#[derive(Default)]
pub(super) struct Theirs {
    edits: Vec<(String, TheirEdit)>,
    mine: Vec<(String, TheirEdit)>,
    ran: Vec<(String, u64)>,
    mine_known: bool,
}

/// What of one file is the item's to commit.
#[derive(Debug, PartialEq)]
pub(super) enum Own {
    /// No other session's change is in it: the file as it is.
    Whole,
    /// The file with others' changes taken out.
    Part(Vec<u8>),
    /// Only others' changes: nothing of the item's.
    Nothing,
    /// Others' changes that can't be told apart from the item's: who made them.
    Mixed(String),
}

/// A file committed with only the item's part: its folder, its path there, and what's committed.
#[derive(Clone, Debug)]
pub(super) struct Part {
    pub folder: String,
    pub path: String,
    pub content: Vec<u8>,
}

/// How long an ended session's log still counts (its saved record read back): a change older than that is in a
/// commit by now, or nobody's to take out (#248).
const ENDED_KEPT_MS: u64 = 3 * 24 * 3600 * 1000;
/// At most this many ended sessions read back.
const ENDED_KEPT_MAX: usize = 100;
/// The saved ended sessions are read back at most this often: each waiting item is checked every 10 s (#248).
const ENDED_READ_EVERY_MS: u64 = 60_000;
/// What one log keeps of what it changed, newest kept: this many changes, this much of their text (#248).
const KEPT_EDITS: usize = 1000;
const KEPT_TEXT: usize = 256 * 1024;
/// A file the item's sessions changed only by writing it whole (or by a command) is taken to hold a change no log
/// shows when it was changed this long after the last change a log shows: a command's write comes after its line.
const UNSHOWN_AFTER_MS: u64 = 120_000;

/// Who changed a file when no session's log shows it: Cue names a session only when its log shows the change (#248).
pub(super) const SOMEONE: &str = "someone else";

/// A session Cue knows of: id, log, folder, name as the Desk shows it, whether it's here, and its agent logs when
/// they were listed already.
#[derive(Clone)]
struct Known {
    id: String,
    log: String,
    cwd: String,
    name: String,
    here: bool,
    agents: Option<Vec<String>>,
}

fn known_of(s: &crate::sessions::Session) -> Known {
    let o = &s.origin;
    Known { id: o.session_id.clone(), log: o.transcript_path.clone(), cwd: o.cwd.clone(), name: super::desk::shown_name(s), here: o.machine.is_empty(), agents: None }
}

/// The agent logs of `log`, as paths.
fn agents_of(log: &str) -> Vec<String> {
    crate::logs::steps::agent_logs(log).into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

/// Sessions that ended within ENDED_KEPT_MS as saved (at most ENDED_KEPT_MAX), with their agent logs: read back at
/// most every ENDED_READ_EVERY_MS. In tests each World has a store of its own, so it's read each time there.
fn ended_saved() -> Vec<Known> {
    static SAVED: std::sync::OnceLock<std::sync::Mutex<Option<(u64, Vec<Known>)>>> = std::sync::OnceLock::new();
    let saved = SAVED.get_or_init(Default::default);
    let now = crate::clock::now_mono_ms();
    if !cfg!(test) {
        let kept = crate::locking::lock(saved, "ended sessions read for their edits");
        if let Some((at, list)) = kept.as_ref().filter(|(at, _)| now.saturating_sub(*at) < ENDED_READ_EVERY_MS) {
            let _ = at;
            return list.clone();
        }
    }
    let list: Vec<Known> = crate::db::closed(now_ms().saturating_sub(ENDED_KEPT_MS), ENDED_KEPT_MAX)
        .iter()
        .map(|(_, s)| Known { agents: Some(agents_of(&s.origin.transcript_path)), ..known_of(s) })
        .collect();
    let mut kept = crate::locking::lock(saved, "ended sessions read for their edits");
    *kept = Some((now, list.clone()));
    list
}

impl Hub {
    /// The changes sessions here made to files, read from their logs (and their background agents'), for `wanted`
    /// (absolute paths): none of a file not in it. Those of `it`'s own sessions (its holder, its former holders and
    /// their crews) are its own; everyone else's are others'. Claude Code's logs, Codex's rollouts and Pi's alike, and
    /// a session that has ended still owns what it changed: parked, ended while Cue ran, or ended within
    /// ENDED_KEPT_MS as saved (#248).
    /// SHORTCUT: the holder's whole session counts as the item's, so its other teammates' edits (not on this item) in
    /// the same file count as the item's; fine while one lead's crew splits work by file (as Cue tells it to).
    /// Upgrade: read those teammates' own logs and take their edits out too.
    pub(super) fn theirs(&self, it: &DeskItem, wanted: &[String]) -> Theirs {
        if wanted.is_empty() {
            return Theirs::default();
        }
        let mut mine_ids: Vec<String> = it.former_holders.clone();
        if let Some(h) = it.holder.as_ref() {
            mine_ids.push(h.sid.clone());
            mine_ids.push(h.to().to_string());
        }
        mine_ids.retain(|i| !i.is_empty());
        // Live first, then ended.
        let mut known: Vec<Known> = self.read(|st| {
            st.sessions.iter().map(known_of)
                .chain(st.parked.iter().map(|p| known_of(&p.session)))
                .chain(st.ended.iter().map(|(_, s)| known_of(s)))
                .collect()
        });
        known.extend(ended_saved());
        // One of the item's own sessions that ended longer ago is looked up by itself.
        let missing: Vec<String> = mine_ids.iter().filter(|i| !known.iter().any(|k| k.id == **i)).cloned().collect();
        known.extend(missing.iter().filter_map(|i| crate::db::ended_session(i)).map(|(_, s)| known_of(&s)));
        let mut seen = std::collections::HashSet::new();
        known.retain(|k| k.here && !k.log.is_empty() && seen.insert(k.id.clone()));
        let all: Vec<String> = known.iter().map(|k| k.log.trim_end_matches(".jsonl").to_string()).collect();
        let mine_known = !mine_ids.is_empty() && mine_ids.iter().all(|i| known.iter().any(|k| k.id == *i && std::path::Path::new(&k.log).is_file()));
        let mut out = Theirs { mine_known, ..Theirs::default() };
        for k in known {
            let mine = mine_ids.contains(&k.id);
            let agents = k.agents.clone().unwrap_or_else(|| agents_of(&k.log));
            for path in std::iter::once(k.log.clone()).chain(agents) {
                let (edits, ran) = read_edits(&path, &k.cwd, wanted);
                if mine {
                    out.mine.extend(edits);
                    out.ran.extend(ran);
                } else {
                    out.edits.extend(edits.into_iter().map(|(f, e)| (f, TheirEdit { by: k.name.clone(), ..e })));
                }
            }
        }
        // A log (or agent log) of a session gone from Cue isn't kept.
        let mut so_far = crate::locking::lock(read_so_far(), "logs read for their edits");
        so_far.retain(|path, _| all.iter().any(|a| path.starts_with(a.as_str())));
        out
    }

    /// Of `todo` (each folder's changed files), what's the item's to commit: the files to commit (their part in
    /// `parts` when others' changes are taken out of them; one with only others' changes left out), and the files
    /// whose others' changes can't be told apart ("<who> also changed <path>").
    pub(super) fn own_of(&self, it: &DeskItem, todo: Vec<(String, Vec<String>)>) -> (Vec<(String, Vec<String>)>, Vec<Part>, Vec<(String, String)>) {
        let abs = |folder: &str, p: &str| format!("{}/{p}", folder.trim_end_matches('/'));
        let wanted: Vec<String> = todo.iter().flat_map(|(f, ps)| ps.iter().map(|p| abs(f, p)).collect::<Vec<_>>()).collect();
        let theirs = self.theirs(it, &wanted);
        let mut keep: Vec<(String, Vec<String>)> = Vec::new();
        let mut parts: Vec<Part> = Vec::new();
        let mut mixed: Vec<(String, String)> = Vec::new();
        for (folder, paths) in todo {
            let mut left: Vec<String> = Vec::new();
            for p in paths {
                match own_part(&theirs, &folder, &p) {
                    Own::Whole => left.push(p),
                    Own::Part(content) => {
                        parts.push(Part { folder: folder.clone(), path: p.clone(), content });
                        left.push(p);
                    }
                    Own::Nothing => {}
                    Own::Mixed(by) => mixed.push((p, if by.is_empty() { SOMEONE.to_string() } else { by })),
                }
            }
            if !left.is_empty() {
                keep.push((folder, left));
            }
        }
        (keep, parts, mixed)
    }
}

/// What one log said so far: how far it was read (whole lines), the folder it was read in, whose log it is
/// ("claude", "codex" or "pi"), the folder its last line ran in, every change it made to a file (`by` left empty:
/// the session's name is put on when they're handed out; the newest KEPT_EDITS and KEPT_TEXT kept), when it last
/// ran a command in each folder, and the patches it sent whose outcome it hasn't said yet (by call id).
#[derive(Default)]
struct SoFar {
    len: u64,
    cwd: String,
    kind: &'static str,
    at: String,
    edits: Vec<(String, TheirEdit)>,
    ran: HashMap<String, u64>,
    pending: HashMap<String, Vec<(String, TheirEdit)>>,
}

impl SoFar {
    fn new(cwd: &str, kind: &'static str) -> SoFar {
        SoFar { cwd: cwd.to_string(), at: cwd.to_string(), kind, ..SoFar::default() }
    }

    /// A shell command `cmd` run in `dir` at `at`: what it writes, as changes whose text isn't known, and that it ran.
    fn shell(&mut self, cmd: &str, dir: &str, at: u64) {
        if cmd.trim().is_empty() {
            return;
        }
        self.edits.extend(shell_changes(cmd, dir, at));
        let last = self.ran.entry(dir.trim_end_matches('/').to_string()).or_default();
        *last = (*last).max(at);
    }

    /// A change that's only made once its call says it worked: kept by call id until then (none: made now).
    fn held(&mut self, id: Option<&str>, changes: Vec<(String, TheirEdit)>) {
        match id.filter(|i| !i.is_empty()) {
            // Some calls never say: a few are kept, not all.
            Some(i) if self.pending.len() < 64 => {
                self.pending.insert(i.to_string(), changes);
            }
            Some(_) => {}
            None => self.edits.extend(changes),
        }
    }

    /// Call `id` said whether it worked: its changes are made, or dropped.
    fn settled(&mut self, id: &str, worked: bool) -> bool {
        match self.pending.remove(id) {
            Some(changes) => {
                if worked {
                    self.edits.extend(changes);
                }
                true
            }
            None => false,
        }
    }

    /// Only the newest KEPT_EDITS changes and KEPT_TEXT of their text are kept.
    fn trim(&mut self) {
        let mut text = 0;
        let keep = self.edits.iter().rev().take_while(|(_, e)| {
            text += e.old.len() + e.new.len();
            text <= KEPT_TEXT
        }).count().min(KEPT_EDITS);
        let cut = self.edits.len() - keep;
        self.edits.drain(..cut);
    }
}

/// Each log's changes so far, by path, so a finish check every 10 s reads only what each log added since (#284):
/// reading every session's whole log each time cost seconds of CPU a minute. Logs no session has any more are dropped.
fn read_so_far() -> &'static std::sync::Mutex<std::collections::HashMap<String, SoFar>> {
    static SO_FAR: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, SoFar>>> = std::sync::OnceLock::new();
    SO_FAR.get_or_init(Default::default)
}

/// The changes `log` (a session working in `cwd`) made to `wanted` files (absolute), and when it last ran a command
/// in a folder holding one of them: what it said before, kept, and the whole lines it added since. A log shorter than
/// what was read, or read in another folder, is read again. A relative path is taken in the folder its own line says
/// it ran in (a helper in a separate copy of the repo writes there, not in the shared folder: #248), else in the last
/// one said, else in `cwd`.
fn read_edits(log: &str, cwd: &str, wanted: &[String]) -> (Vec<(String, TheirEdit)>, Vec<(String, u64)>) {
    let kept = {
        let mut so_far = crate::locking::lock(read_so_far(), "logs read for their edits");
        so_far.remove(log)
    };
    let fresh = || SoFar::new(cwd, crate::logs::steps::harness_of(log));
    let mut so_far = kept.filter(|k| k.cwd == cwd).unwrap_or_else(fresh);
    let Ok(mut file) = std::fs::File::open(log) else { return (vec![], vec![]) };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len < so_far.len {
        so_far = fresh();
    }
    let mut added: Vec<u8> = Vec::new();
    use std::io::{Read, Seek};
    if len > so_far.len && file.seek(std::io::SeekFrom::Start(so_far.len)).is_ok() && file.read_to_end(&mut added).is_ok() {
        // Only whole lines: a line still being written is read next time.
        if let Some(end) = added.iter().rposition(|b| *b == b'\n') {
            for line in String::from_utf8_lossy(&added[..=end]).lines() {
                let worth = match so_far.kind {
                    "codex" => CODEX_MARKS.iter().any(|m| line.contains(m)) || so_far.pending.keys().any(|i| line.contains(i.as_str())),
                    "pi" => ["\"toolCall\"", "\"toolResult\"", "\"cwd\""].iter().any(|m| line.contains(m)),
                    _ => line.contains("tool_use"),
                };
                let Some(e) = worth.then(|| serde_json::from_str::<Value>(line).ok()).flatten() else { continue };
                match so_far.kind {
                    "codex" => codex_line(&e, &mut so_far),
                    "pi" => pi_line(&e, &mut so_far),
                    _ => claude_line(&e, &mut so_far),
                }
            }
            so_far.len += end as u64 + 1;
            so_far.edits.retain(|(f, _)| !f.is_empty());
            so_far.trim();
        }
    }
    let edits = so_far.edits.iter().filter(|(f, _)| wanted.contains(f)).cloned().collect();
    let ran = so_far.ran.iter().filter(|(d, _)| wanted.iter().any(|f| f.starts_with(&format!("{d}/")))).map(|(d, at)| (d.clone(), *at)).collect();
    let mut all = crate::locking::lock(read_so_far(), "logs read for their edits");
    all.insert(log.to_string(), so_far);
    (edits, ran)
}

/// `f` made absolute in folder `dir`.
fn resolve(dir: &str, f: &str) -> String {
    if f.starts_with('/') {
        f.to_string()
    } else {
        format!("{}/{}", dir.trim_end_matches('/'), f.trim_start_matches("./"))
    }
}

/// A change whose text is known (`old` to `new`) or isn't (`blind`), at `at`, nobody's yet.
fn change(at: u64, old: &str, new: &str, all: bool, blind: bool) -> TheirEdit {
    TheirEdit { at, by: String::new(), old: old.to_string(), new: new.to_string(), all, blind }
}

/// One Claude Code log line: the folder it ran in, and the files its tool calls changed, and how (a relative path
/// taken in that folder).
fn claude_line(e: &Value, so: &mut SoFar) {
    if let Some(here) = e.get("cwd").and_then(Value::as_str).filter(|c| c.starts_with('/')) {
        so.at = here.to_string();
    }
    let at = crate::logs::transcript::stamp(e);
    let dir = so.at.clone();
    let edit = |old: &str, new: &str, all: bool, blind: bool| change(at, old, new, all, blind);
    // An edit without its old and new text changes nothing (Claude Code refuses it): it isn't a change of anyone's.
    // An empty old text makes the file: a write.
    let one = |m: &Value| match (m.get("old_string").and_then(Value::as_str), m.get("new_string").and_then(Value::as_str)) {
        (Some(""), Some(_)) => Some(edit("", "", false, true)),
        (Some(old), Some(new)) => Some(edit(old, new, m.get("replace_all").and_then(Value::as_bool).unwrap_or(false), false)),
        _ => None,
    };
    for b in e.pointer("/message/content").and_then(Value::as_array).into_iter().flatten().filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use")) {
        let input = &b["input"];
        let file = input.get("file_path").or_else(|| input.get("notebook_path")).and_then(Value::as_str).map(|f| resolve(&dir, f)).unwrap_or_default();
        match b.get("name").and_then(Value::as_str).unwrap_or("") {
            "Edit" => so.edits.extend(one(input).map(|c| (file, c))),
            "MultiEdit" => {
                for m in input.get("edits").and_then(Value::as_array).into_iter().flatten() {
                    so.edits.extend(one(m).map(|c| (file.clone(), c)));
                }
            }
            "Write" | "NotebookEdit" => so.edits.push((file, edit("", "", false, true))),
            "Bash" => so.shell(input.get("command").and_then(Value::as_str).unwrap_or(""), &dir, at),
            _ => {}
        }
    }
}

/// One Pi log line (#248): its session line's folder; an assistant's edit (each of its `edits`, or its one
/// oldText/newText) and write, made once their result says they worked; its bash commands' writes.
fn pi_line(e: &Value, so: &mut SoFar) {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    if e.get("type").and_then(Value::as_str) == Some("session") {
        if let Some(c) = e.get("cwd").and_then(Value::as_str).filter(|c| c.starts_with('/')) {
            so.at = c.to_string();
        }
        return;
    }
    let at = crate::logs::transcript::stamp(e);
    let m = e.get("message").unwrap_or(&Value::Null);
    match m.get("role").and_then(Value::as_str) {
        Some("assistant") => {
            let dir = so.at.clone();
            for b in m.get("content").and_then(Value::as_array).into_iter().flatten().filter(|b| b.get("type").and_then(Value::as_str) == Some("toolCall")) {
                let args = b.get("arguments").unwrap_or(&Value::Null);
                let file = resolve(&dir, &s(args, "path"));
                let id = s(b, "id");
                match b.get("name").and_then(Value::as_str).unwrap_or("") {
                    "edit" => {
                        let one = [args.clone()];
                        let list = args.get("edits").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&one);
                        let changes = list.iter().map(|x| (file.clone(), match (x.get("oldText").and_then(Value::as_str), x.get("newText").and_then(Value::as_str)) {
                            (Some(old), Some(new)) => change(at, old, new, false, false),
                            _ => change(at, "", "", false, true),
                        })).collect();
                        so.held(Some(&id), changes);
                    }
                    "write" => so.held(Some(&id), vec![(file, change(at, "", "", false, true))]),
                    "bash" => so.shell(&s(args, "command"), &dir, at),
                    _ => {}
                }
            }
        }
        Some("toolResult") => {
            so.settled(&s(m, "toolCallId"), m.get("isError").and_then(Value::as_bool) != Some(true));
        }
        _ => {}
    }
}

/// What a Codex rollout line must say to be read for changes (the rest is skipped unparsed, but for a patch's
/// outcome, found by its call id).
const CODEX_MARKS: [&str; 10] = ["FileChange", "CommandExecution", "apply_patch", "patch_apply", "session_meta", "turn_context", "exec_command", "\"name\":\"shell", "\"name\":\"container.exec\"", "local_shell_call"];

/// One Codex rollout line (#248): its patches (a FileChange item, a patch_apply event, an apply_patch call) by their
/// text, each hunk an edit, counted only when it worked; a file added, deleted or moved, and what its shell commands
/// write, as changes whose text isn't known. The folder is what its session_meta and turn_context lines say: a
/// relative path is taken there (a command's own folder, when it says one). Typing into a running command
/// (write_stdin) isn't read: what it writes isn't known.
fn codex_line(e: &Value, so: &mut SoFar) {
    let at = crate::logs::transcript::stamp(e);
    let p = e.get("payload").unwrap_or(&Value::Null);
    let ptype = p.get("type").and_then(Value::as_str).unwrap_or("");
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let dir = so.at.clone();
    match (e.get("type").and_then(Value::as_str).unwrap_or(""), ptype) {
        ("session_meta" | "turn_context", _) => {
            if let Some(c) = p.get("cwd").and_then(Value::as_str).filter(|c| c.starts_with('/')) {
                so.at = c.to_string();
            }
        }
        ("event_msg", "item_completed") => {
            let item = p.get("item").unwrap_or(&Value::Null);
            match item.get("type").and_then(Value::as_str) {
                // A patch that failed or was declined changed nothing.
                Some("FileChange") if matches!(item.get("status").and_then(Value::as_str), None | Some("completed")) => {
                    so.edits.extend(codex_changes(item.get("changes").unwrap_or(&Value::Null), &dir, at));
                }
                Some("CommandExecution") => {
                    let here = folder_of_url(&s(item, "cwd")).unwrap_or(dir);
                    so.shell(&command_of(item.get("command").unwrap_or(&Value::Null)), &here, at);
                }
                _ => {}
            }
        }
        ("event_msg", "patch_apply_end") => {
            let worked = p.get("success").and_then(Value::as_bool) != Some(false);
            // The call it ends has its changes already; else they're its own.
            if !so.settled(&s(p, "call_id"), worked) && worked {
                so.edits.extend(codex_changes(p.get("changes").unwrap_or(&Value::Null), &dir, at));
            }
        }
        ("response_item", "function_call" | "custom_tool_call") => {
            let args: Value = p.get("arguments").and_then(Value::as_str).and_then(|a| serde_json::from_str(a).ok()).unwrap_or(Value::Null);
            let here = args.get("workdir").and_then(Value::as_str).map(|w| resolve(&dir, w)).unwrap_or(dir);
            match p.get("name").and_then(Value::as_str).unwrap_or("") {
                "apply_patch" => {
                    let raw = p.get("input").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| s(&args, "input"));
                    let changes = patch_changes(&raw, &here, at);
                    so.held(Some(&s(p, "call_id")), changes);
                }
                "exec_command" => so.shell(&s(&args, "cmd"), &here, at),
                "shell" | "shell_command" | "local_shell" | "container.exec" => so.shell(&command_of(args.get("command").unwrap_or(&Value::Null)), &here, at),
                _ => {}
            }
        }
        ("response_item", "local_shell_call") => {
            let action = p.get("action").unwrap_or(&Value::Null);
            let here = action.get("working_directory").and_then(Value::as_str).map(|w| resolve(&dir, w)).unwrap_or(dir);
            so.shell(&command_of(action.get("command").unwrap_or(&Value::Null)), &here, at);
        }
        ("response_item", "function_call_output" | "custom_tool_call_output") => {
            so.settled(&s(p, "call_id"), patch_worked(p.get("output").unwrap_or(&Value::Null)));
        }
        _ => {}
    }
}

/// Whether an apply_patch call's output says it worked: its exit code 0, or "Success." (as its text says).
fn patch_worked(output: &Value) -> bool {
    let text = match output {
        Value::String(t) => t.clone(),
        Value::Array(a) => a.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        v => v.to_string(),
    };
    if let Ok(j) = serde_json::from_str::<Value>(&text) {
        if let Some(code) = j.pointer("/metadata/exit_code").and_then(Value::as_i64) {
            return code == 0;
        }
    }
    let t = text.trim_start();
    t.starts_with("Exit code: 0") || t.starts_with("Success.") || t.contains("\"output\":\"Success.")
}

/// A Codex command as one line: a string as it is, an argv as its script (`zsh -lc "<script>"`) or its words.
fn command_of(v: &Value) -> String {
    if let Some(c) = v.as_str() {
        return c.to_string();
    }
    let argv: Vec<&str> = v.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if argv.len() >= 3 && matches!(argv[1], "-lc" | "-c") { argv[2].to_string() } else { argv.join(" ") }
}

/// A folder as a `file://` URL says it (its %-escapes put back), or a plain absolute path.
fn folder_of_url(u: &str) -> Option<String> {
    let path = u.strip_prefix("file://").unwrap_or(u);
    if !path.starts_with('/') {
        return None;
    }
    let (b, mut out, mut i) = (path.as_bytes(), Vec::new(), 0);
    while i < b.len() {
        match (b[i], path.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(x)) => {
                out.push(x);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// What a shell command run in `dir` writes, as changes whose text isn't known.
fn shell_changes(cmd: &str, dir: &str, at: u64) -> Vec<(String, TheirEdit)> {
    crate::sessions::shell_writes::writes(cmd).iter().map(|f| (resolve(dir, f), change(at, "", "", false, true))).collect()
}

/// A Codex change list (by path: `{type, unified_diff, move_path}`, or `{update: {unified_diff, move_path}}` and
/// the like; or a list of `{path, kind, diff}`): an update's hunks as edits, anything else not known by its text.
fn codex_changes(changes: &Value, dir: &str, at: u64) -> Vec<(String, TheirEdit)> {
    let listed: Vec<(String, &Value)> = match changes {
        Value::Object(o) => o.iter().map(|(k, v)| (k.clone(), v)).collect(),
        Value::Array(a) => a.iter().map(|c| (c.get("path").and_then(Value::as_str).unwrap_or("").to_string(), c)).collect(),
        _ => vec![],
    };
    let mut found: Vec<(String, TheirEdit)> = Vec::new();
    for (path, c) in listed.into_iter().filter(|(p, _)| !p.is_empty()) {
        let file = resolve(dir, &path);
        let kind = c.get("type").or_else(|| c.pointer("/kind/type")).and_then(Value::as_str).map(str::to_string)
            .or_else(|| c.as_object().and_then(|o| o.keys().find(|k| matches!(k.as_str(), "update" | "add" | "delete")).cloned()))
            .unwrap_or_default();
        let body = c.get(kind.as_str()).filter(|v| v.is_object()).unwrap_or(c);
        let moved = body.get("move_path").and_then(Value::as_str).filter(|m| !m.is_empty());
        let diff = body.get("unified_diff").or_else(|| body.get("diff")).and_then(Value::as_str);
        match (kind.as_str(), moved, diff) {
            ("update", None, Some(d)) => found.extend(hunks(d).into_iter().map(|(old, new)| (file.clone(), change(at, &old, &new, false, false)))),
            _ => {
                found.push((file, change(at, "", "", false, true)));
                if let Some(m) = moved {
                    found.push((resolve(dir, m), change(at, "", "", false, true)));
                }
            }
        }
    }
    found
}

/// An apply_patch patch's changes (paths taken in `dir`): an updated file's hunks as edits; a file added, deleted or
/// moved not known by its text.
fn patch_changes(patch: &str, dir: &str, at: u64) -> Vec<(String, TheirEdit)> {
    let mut found: Vec<(String, TheirEdit)> = Vec::new();
    let mut updating: Option<(String, Vec<&str>)> = None;
    let done = |u: Option<(String, Vec<&str>)>, found: &mut Vec<(String, TheirEdit)>| {
        if let Some((file, body)) = u {
            found.extend(hunks(&body.join("\n")).into_iter().map(|(old, new)| (file.clone(), change(at, &old, &new, false, false))));
        }
    };
    for l in patch.lines() {
        if let Some(f) = l.strip_prefix("*** Update File: ") {
            done(updating.take(), &mut found);
            updating = Some((resolve(dir, f.trim()), Vec::new()));
        } else if let Some(f) = l.strip_prefix("*** Move to: ") {
            // Moved: neither end known by its text.
            if let Some((file, _)) = updating.take() {
                found.push((file, change(at, "", "", false, true)));
            }
            found.push((resolve(dir, f.trim()), change(at, "", "", false, true)));
        } else if let Some(f) = ["*** Add File: ", "*** Delete File: "].iter().find_map(|m| l.strip_prefix(m)) {
            done(updating.take(), &mut found);
            found.push((resolve(dir, f.trim()), change(at, "", "", false, true)));
        } else if let Some((_, body)) = updating.as_mut() {
            // "*** End of File" and the like end a hunk, as "@@" does.
            body.push(if l.starts_with("*** ") { "@@" } else { l });
        }
    }
    done(updating, &mut found);
    found
}

/// A diff's hunks (split at "@@"; a hunk's context and removed lines its old text, context and added lines its new):
/// each (old, new), whole lines, those that change nothing left out.
fn hunks(diff: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let (mut old, mut new): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let joined = |v: &[&str]| if v.is_empty() { String::new() } else { v.join("\n") + "\n" };
    let mut flush = |old: &mut Vec<&str>, new: &mut Vec<&str>| {
        if old != new {
            out.push((joined(old), joined(new)));
        }
        old.clear();
        new.clear();
    };
    for l in diff.lines() {
        if l.starts_with("@@") || l.starts_with("*** ") {
            flush(&mut old, &mut new);
            continue;
        }
        match l.as_bytes().first() {
            // "\ No newline at end of file"
            Some(b'\\') => {}
            Some(b'+') => new.push(&l[1..]),
            Some(b'-') => old.push(&l[1..]),
            Some(b' ') => {
                old.push(&l[1..]);
                new.push(&l[1..]);
            }
            // A blank line: an empty context line.
            _ => {
                old.push(l);
                new.push(l);
            }
        }
    }
    flush(&mut old, &mut new);
    out
}

/// Of `path` in `folder`, the item's part: its text now with others' changes taken out, newest first. An Edit of
/// theirs counts while its new text is in the file more often than in HEAD (else it's committed already, or undone);
/// a change whose text isn't known counts when made since the file's last commit. Then, when the item's own sessions'
/// logs are all here, what's left must be theirs too (`mine_part`): a change no log shows is left out, or the commit
/// waits on "someone else" (#248).
pub(super) fn own_part<'a>(theirs: &'a Theirs, folder: &str, path: &str) -> Own {
    let abs = format!("{}/{path}", folder.trim_end_matches('/'));
    let of = |list: &'a [(String, TheirEdit)]| -> Vec<&'a TheirEdit> {
        let mut edits: Vec<&TheirEdit> = list.iter().filter(|(f, _)| *f == abs).map(|(_, e)| e).collect();
        edits.sort_by_key(|e| std::cmp::Reverse(e.at));
        edits
    };
    let (others, mine) = (of(&theirs.edits), of(&theirs.mine));
    if others.is_empty() && (!theirs.mine_known || mine.is_empty()) {
        return Own::Whole;
    }
    let git = |args: &[&str]| super::desk_commit::git("", folder, &[], args);
    let committed = git(&["log", "-1", "--format=%ct", "--", path]).and_then(|t| t.trim().parse::<u64>().ok()).map_or(0, |s| s * 1000);
    let head = git(&["show", &format!("HEAD:./{path}")]).unwrap_or_default();
    let Ok(now) = std::fs::read_to_string(&abs) else {
        // Gone, or not text: theirs can't be taken out of it. Only a change since its last commit is named (#248).
        return match others.iter().find(|e| e.at >= committed) {
            Some(e) => Own::Mixed(e.by.clone()),
            None => Own::Whole,
        };
    };
    let rest = match take_out(&now, &head, &others, committed) {
        Err(by) => return Own::Mixed(by),
        Ok(t) => t.unwrap_or_else(|| now.clone()),
    };
    let rest = if theirs.mine_known {
        let ran = theirs.ran.iter().filter(|(d, _)| abs.starts_with(&format!("{d}/"))).map(|(_, at)| *at).max();
        match mine_part(&abs, &head, rest, &mine, &others, ran, committed) {
            Some(t) => t,
            None => return Own::Mixed(SOMEONE.to_string()),
        }
    } else {
        rest
    };
    if rest == now {
        Own::Whole
    } else if rest == head {
        Own::Nothing
    } else {
        Own::Part(rest.into_bytes())
    }
}

/// The item's part of `rest` (file `abs` with others' changes taken out), by its own sessions' changes `mine` (newest
/// first; `ran`: when they last ran a command in a folder holding it): None when a change no log shows is in it and
/// can't be left out (#248).
/// The item's changes all known by their text: taken out of `rest` they must give HEAD; when they don't, what's left
/// is a change no log shows, and HEAD with only the item's changes put on is the item's part (or None, when the two
/// overlap, or nothing of the item's is left). A change of the item's not known by its text (a whole-file write, a
/// command) can't be taken out: then the file changed well after the last change a log shows (UNSHOWN_AFTER_MS)
/// holds a change no log shows.
/// SHORTCUT: a command that rewrites files without saying so in its words (a formatter) reads as the item's only
/// while the file changed within UNSHOWN_AFTER_MS of it starting; a longer one reads as someone else's. Upgrade: take
/// a command's end from its result line.
fn mine_part(abs: &str, head: &str, rest: String, mine: &[&TheirEdit], others: &[&TheirEdit], ran: Option<u64>, committed: u64) -> Option<String> {
    // Nothing of the item's left, or none of its changes read (its files came from its log: no telling then).
    if rest == head || mine.is_empty() {
        return Some(rest);
    }
    let blind = mine.iter().any(|e| e.blind && e.at >= committed) || ran.is_some_and(|at| at >= committed);
    if !blind {
        if let Ok(base) = take_out(&rest, head, mine, committed) {
            let base = base.unwrap_or_else(|| rest.clone());
            if base == head {
                return Some(rest);
            }
            // None of the item's changes left on HEAD: the change no log shows went over them.
            return merged(head, &base, &rest).filter(|t| t != head);
        }
    }
    let latest = mine.iter().chain(others).map(|e| e.at).chain(ran).max().unwrap_or(0);
    let changed = std::fs::metadata(abs).and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64);
    (changed <= latest + UNSHOWN_AFTER_MS).then_some(rest)
}

/// `head` with what changed from `base` to `ours` put on (git merge-file): None when the two overlap.
fn merged(head: &str, base: &str, ours: &str) -> Option<String> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("cue-own-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).ok()?;
    let put = |n: &str, t: &str| std::fs::write(dir.join(n), t).ok().map(|_| dir.join(n).to_string_lossy().into_owned());
    let out = (|| {
        let (h, b, o) = (put("head", head)?, put("base", base)?, put("ours", ours)?);
        let r = std::process::Command::new("git").args(["merge-file", "-p", "-q", &h, &b, &o]).output().ok()?;
        r.status.success().then(|| String::from_utf8_lossy(&r.stdout).into_owned())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// `now` with `edits` (newest first) taken out: None when none of them is in it, Err(who) when one can't be.
fn take_out(now: &str, head: &str, edits: &[&TheirEdit], committed: u64) -> Result<Option<String>, String> {
    let mut text = now.to_string();
    let mut took = false;
    for e in edits {
        if e.blind {
            if e.at >= committed {
                return Err(e.by.clone());
            }
            continue;
        }
        if e.new.is_empty() {
            // A cut: where it was can't be found again. Still in effect when its old text is in HEAD more often.
            if !e.old.is_empty() && head.matches(e.old.as_str()).count() > text.matches(e.old.as_str()).count() {
                return Err(e.by.clone());
            }
            continue;
        }
        let (here, before) = (text.matches(e.new.as_str()).count(), head.matches(e.new.as_str()).count());
        if here <= before {
            continue;
        }
        if e.all && before == 0 {
            text = text.replace(e.new.as_str(), &e.old);
        } else if here == 1 {
            text = text.replacen(e.new.as_str(), &e.old, 1);
        } else {
            return Err(e.by.clone());
        }
        took = true;
    }
    Ok(took.then_some(text))
}

#[cfg(test)]
mod tests {
    use super::{read_edits, take_out, TheirEdit};

    fn edit(at: u64, old: &str, new: &str) -> TheirEdit {
        TheirEdit { at, by: "tidier".into(), old: old.into(), new: new.into(), all: false, blind: false }
    }

    #[test]
    fn their_edit_is_taken_out_and_the_items_stays() {
        let head = "one\ntwo\nthree\n";
        let now = "ONE\ntwo\nthree!\n";
        let theirs = edit(1, "three\n", "three!\n");
        assert_eq!(take_out(now, head, &[&theirs], 0), Ok(Some("ONE\ntwo\nthree\n".into())));
    }

    #[test]
    fn chained_edits_come_out_newest_first_and_a_committed_one_is_left() {
        let head = "a\nb\n";
        let now = "a\nb3\n";
        let (first, second) = (edit(1, "b\n", "b2\n"), edit(2, "b2\n", "b3\n"));
        assert_eq!(take_out(now, head, &[&second, &first], 0), Ok(Some(head.into())));
        assert_eq!(take_out("a\nb\n", head, &[&edit(1, "x", "a\n")], 0), Ok(None), "its text is in HEAD: committed");
    }

    #[test]
    fn text_found_twice_or_a_write_since_the_commit_cant_be_told_apart() {
        let twice = edit(1, "x", "y");
        assert_eq!(take_out("y y", "", &[&twice], 0), Err("tidier".into()));
        let write = TheirEdit { blind: true, ..edit(5, "", "") };
        assert_eq!(take_out("z", "", &[&write], 1), Err("tidier".into()));
        assert_eq!(take_out("z", "", &[&write], 9), Ok(None), "written before its last commit");
    }

    /// A log read again gives what it said before plus only its new whole lines: its old lines aren't read again
    /// (here they're blanked on disk and their edit is still there), a half-written line waits, and a log cut
    /// shorter is read from the start.
    #[test]
    fn a_log_is_read_once_and_then_only_what_it_adds() {
        let dir = std::env::temp_dir().join(format!("cue-theirs-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("s.jsonl");
        let line = |file: &str, new: &str| format!("{{\"timestamp\":\"2026-11-18T12:00:00Z\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"Edit\",\"input\":{{\"file_path\":\"{file}\",\"old_string\":\"a\",\"new_string\":\"{new}\"}}}}]}}}}\n");
        let (first, other) = (line("x.rs", "b"), line("/elsewhere/y.rs", "c"));
        std::fs::write(&log, format!("{first}{other}")).unwrap();
        let (path, cwd) = (log.to_string_lossy().into_owned(), dir.to_string_lossy().into_owned());
        let wanted = vec![format!("{cwd}/x.rs")];
        let news = |got: Vec<(String, TheirEdit)>| got.into_iter().map(|(_, e)| e.new).collect::<Vec<_>>();
        assert_eq!(news(read_edits(&path, &cwd, &wanted).0), ["b"]);
        let blanked = " ".repeat(first.len() - 1) + "\n";
        let second = line("x.rs", "d");
        std::fs::write(&log, format!("{blanked}{other}{second}{}", &second[..20])).unwrap();
        assert_eq!(news(read_edits(&path, &cwd, &wanted).0), ["b", "d"], "the old line came from what was read before");
        std::fs::write(&log, format!("{blanked}{other}{second}{second}")).unwrap();
        assert_eq!(news(read_edits(&path, &cwd, &wanted).0), ["b", "d", "d"], "the half line, once whole");
        std::fs::write(&log, &second).unwrap();
        assert_eq!(news(read_edits(&path, &cwd, &wanted).0), ["d"], "cut shorter: read from the start");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// (file, old, new, blind) of each change.
    fn shown(got: Vec<(String, TheirEdit)>) -> Vec<(String, String, String, bool)> {
        got.into_iter().map(|(f, e)| (f, e.old, e.new, e.blind)).collect()
    }

    /// A Codex apply_patch: each hunk of an updated file is an edit by its text (relative paths taken in the
    /// session's folder); an added, deleted or moved file is a change whose text isn't known (#248).
    #[test]
    fn a_codex_patch_reads_as_one_edit_per_hunk_and_an_added_or_moved_file_as_unknown_text() {
        let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@ fn a\n keep\n-old\n+new\n@@\n-gone\n*** Add File: b.rs\n+x\n*** Update File: c.rs\n*** Move to: d.rs\n@@\n-c\n+d\n*** End Patch";
        assert_eq!(shown(super::patch_changes(patch, "/r/", 1)), [
            ("/r/src/a.rs".into(), "keep\nold\n".into(), "keep\nnew\n".into(), false),
            ("/r/src/a.rs".into(), "gone\n".into(), "".into(), false),
            ("/r/b.rs".into(), "".into(), "".into(), true),
            ("/r/c.rs".into(), "".into(), "".into(), true),
            ("/r/d.rs".into(), "".into(), "".into(), true),
        ]);
    }

    /// The Codex app's rollout: a FileChange item's diff by its text, a command's writes taken in the folder it ran
    /// in (its file:// URL), and a relative path in the folder its session_meta says.
    #[test]
    fn a_codex_rollouts_file_changes_and_shell_writes_are_read_with_their_folders() {
        let mut so = super::SoFar::new("/start", "codex");
        let meta = serde_json::json!({ "timestamp": "2026-11-18T12:00:00Z", "type": "session_meta", "payload": { "cwd": "/repo" } });
        super::codex_line(&meta, &mut so);
        assert!(so.edits.is_empty());
        assert_eq!(so.at, "/repo");
        let change = serde_json::json!({ "timestamp": "2026-11-18T12:00:01Z", "type": "event_msg", "payload": { "type": "item_completed", "item": {
            "type": "FileChange", "status": "completed", "changes": { "/repo/x.rs": { "type": "update", "unified_diff": "@@ -1,2 +1,3 @@\n a\n+b\n c\n", "move_path": null }, "y.rs": { "type": "add", "content": "z" } } } } });
        super::codex_line(&change, &mut so);
        assert_eq!(shown(std::mem::take(&mut so.edits)), [
            ("/repo/x.rs".into(), "a\nc\n".into(), "a\nb\nc\n".into(), false),
            ("/repo/y.rs".into(), "".into(), "".into(), true),
        ]);
        let shell = serde_json::json!({ "timestamp": "2026-11-18T12:00:02Z", "type": "event_msg", "payload": { "type": "item_completed", "item": {
            "type": "CommandExecution", "command": ["/bin/zsh", "-lc", "echo hi > out.txt"], "cwd": "file:///other%20copy" } } });
        super::codex_line(&shell, &mut so);
        assert_eq!(shown(std::mem::take(&mut so.edits)), [("/other copy/out.txt".into(), "".into(), "".into(), true)]);
        assert!(so.ran.contains_key("/other copy"), "it ran a command there");
    }

    /// A Codex log written to disk and read through the quick filter: every kind of shell call it may make is read
    /// (shell, shell_command, container.exec, local_shell_call, exec_command), each in its own folder (#248).
    #[test]
    fn every_codex_shell_call_is_read_through_the_quick_filter() {
        let dir = std::env::temp_dir().join(format!("cue-theirs-cxsh-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("rollout.jsonl");
        let call = |name: &str, args: serde_json::Value| serde_json::json!({ "timestamp": "2026-11-18T12:00:01Z", "type": "response_item",
            "payload": { "type": "function_call", "name": name, "arguments": args.to_string(), "call_id": format!("c-{name}") } }).to_string();
        let lines = [
            serde_json::json!({ "timestamp": "2026-11-18T12:00:00Z", "type": "session_meta", "payload": { "cwd": "/repo" } }).to_string(),
            call("shell", serde_json::json!({ "command": ["bash", "-lc", "echo 1 > a.txt"], "workdir": "/repo" })),
            call("shell_command", serde_json::json!({ "command": "echo 2 > b.txt", "workdir": "/repo/sub" })),
            call("container.exec", serde_json::json!({ "command": ["bash", "-lc", "echo 3 > c.txt"] })),
            call("exec_command", serde_json::json!({ "cmd": "echo 4 > d.txt" })),
            serde_json::json!({ "timestamp": "2026-11-18T12:00:02Z", "type": "response_item", "payload": { "type": "local_shell_call",
                "action": { "type": "exec", "command": ["bash", "-lc", "echo 5 > e.txt"], "working_directory": "/repo" } } }).to_string(),
        ];
        std::fs::write(&log, lines.join("\n") + "\n").unwrap();
        let wanted: Vec<String> = ["/repo/a.txt", "/repo/sub/b.txt", "/repo/c.txt", "/repo/d.txt", "/repo/e.txt"].iter().map(|s| s.to_string()).collect();
        let (edits, ran) = read_edits(&log.to_string_lossy(), "/repo", &wanted);
        assert_eq!(edits.into_iter().map(|(f, e)| (f, e.blind)).collect::<Vec<_>>(), wanted.iter().map(|f| (f.clone(), true)).collect::<Vec<_>>());
        assert!(ran.iter().any(|(d, _)| d == "/repo") && ran.iter().any(|(d, _)| d == "/repo/sub"), "{ran:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Codex patch that failed changed nothing: a FileChange item that failed, an apply_patch call whose output says
    /// it failed, a patch_apply_end that says so. One that worked counts, once its outcome is read (#248).
    #[test]
    fn a_codex_patch_that_failed_is_no_change() {
        let mut so = super::SoFar::new("/repo", "codex");
        let failed = serde_json::json!({ "timestamp": "2026-11-18T12:00:01Z", "type": "event_msg", "payload": { "type": "item_completed", "item": {
            "type": "FileChange", "status": "failed", "changes": { "/repo/x.rs": { "type": "update", "unified_diff": "@@\n-a\n+b\n" } } } } });
        super::codex_line(&failed, &mut so);
        assert!(so.edits.is_empty(), "a failed FileChange");
        let patch = |id: &str| serde_json::json!({ "timestamp": "2026-11-18T12:00:02Z", "type": "response_item", "payload": { "type": "custom_tool_call",
            "name": "apply_patch", "call_id": id, "input": "*** Begin Patch\n*** Update File: x.rs\n@@\n-a\n+b\n*** End Patch" } });
        let output = |id: &str, text: &str| serde_json::json!({ "timestamp": "2026-11-18T12:00:03Z", "type": "response_item", "payload": { "type": "custom_tool_call_output", "call_id": id, "output": text } });
        super::codex_line(&patch("p1"), &mut so);
        assert!(so.edits.is_empty(), "not yet: its outcome isn't read");
        super::codex_line(&output("p1", "apply_patch verification failed: Failed to find expected lines"), &mut so);
        assert!(so.edits.is_empty() && so.pending.is_empty(), "its output says it failed");
        super::codex_line(&patch("p2"), &mut so);
        let end = serde_json::json!({ "timestamp": "2026-11-18T12:00:03Z", "type": "event_msg", "payload": { "type": "patch_apply_end", "call_id": "p2", "success": false, "changes": {} } });
        super::codex_line(&end, &mut so);
        assert!(so.edits.is_empty(), "patch_apply_end says it failed");
        super::codex_line(&patch("p3"), &mut so);
        super::codex_line(&output("p3", "Exit code: 0\nWall time: 0 seconds\nOutput:\nSuccess. Updated the following files:\nM x.rs\n"), &mut so);
        assert_eq!(shown(std::mem::take(&mut so.edits)), [("/repo/x.rs".into(), "a\n".into(), "b\n".into(), false)]);
    }

    /// A Pi log: an edit (each of its edits) by its text and a write, once their result says they worked; a bash
    /// command's write; relative paths in the folder its session line says (#248).
    #[test]
    fn a_pi_logs_edits_writes_and_commands_are_read() {
        let mut so = super::SoFar::new("/start", "pi");
        let mut line = |v: serde_json::Value| super::pi_line(&v, &mut so);
        line(serde_json::json!({ "type": "session", "version": 3, "id": "P", "cwd": "/a" }));
        line(serde_json::json!({ "type": "message", "timestamp": "2026-11-18T12:00:01Z", "message": { "role": "assistant", "content": [
            { "type": "toolCall", "id": "c1", "name": "edit", "arguments": { "path": "x.ts", "edits": [{ "oldText": "a", "newText": "b" }] } },
            { "type": "toolCall", "id": "c2", "name": "edit", "arguments": { "path": "/a/y.ts", "oldText": "c", "newText": "d" } },
            { "type": "toolCall", "id": "c3", "name": "write", "arguments": { "path": "z.ts", "content": "z" } },
            { "type": "toolCall", "id": "c4", "name": "bash", "arguments": { "command": "echo hi > w.txt" } } ] } }));
        line(serde_json::json!({ "type": "message", "message": { "role": "toolResult", "toolCallId": "c1", "isError": false } }));
        line(serde_json::json!({ "type": "message", "message": { "role": "toolResult", "toolCallId": "c2", "isError": true } }));
        line(serde_json::json!({ "type": "message", "message": { "role": "toolResult", "toolCallId": "c3", "isError": false } }));
        assert_eq!(shown(std::mem::take(&mut so.edits)), [
            ("/a/w.txt".into(), "".into(), "".into(), true),
            ("/a/x.ts".into(), "a".into(), "b".into(), false),
            ("/a/z.ts".into(), "".into(), "".into(), true),
        ], "the failed edit of y.ts isn't one");
    }

    /// A Claude Code Edit without its old and new text changed nothing and isn't read; one with an empty old text
    /// made the file, read as a write (#248).
    #[test]
    fn a_claude_edit_without_its_text_is_no_change_and_one_with_empty_old_text_is_a_write() {
        let mut so = super::SoFar::new("/r", "claude");
        let edit = |input: serde_json::Value| serde_json::json!({ "message": { "content": [{ "type": "tool_use", "name": "Edit", "input": input }] } });
        super::claude_line(&edit(serde_json::json!({ "file_path": "/r/a.txt" })), &mut so);
        super::claude_line(&edit(serde_json::json!({ "file_path": "/r/b.txt", "old_string": "", "new_string": "b" })), &mut so);
        super::claude_line(&edit(serde_json::json!({ "file_path": "/r/c.txt", "old_string": "c", "new_string": "d" })), &mut so);
        assert_eq!(shown(std::mem::take(&mut so.edits)), [
            ("/r/b.txt".into(), "".into(), "".into(), true),
            ("/r/c.txt".into(), "c".into(), "d".into(), false),
        ]);
    }

    /// A log keeps only its newest changes: KEPT_EDITS of them, and KEPT_TEXT of their text (#248).
    #[test]
    fn a_log_keeps_only_its_newest_changes() {
        let mut so = super::SoFar::new("/r", "claude");
        so.edits = (0..super::KEPT_EDITS as u64 + 5).map(|i| ("/r/a".to_string(), edit(i, "", "x"))).collect();
        so.trim();
        assert_eq!((so.edits.len(), so.edits[0].1.at), (super::KEPT_EDITS, 5));
        so.edits = (0..4u64).map(|i| ("/r/a".to_string(), edit(i, "", &"y".repeat(super::KEPT_TEXT / 3)))).collect();
        so.trim();
        assert_eq!(so.edits.iter().map(|(_, e)| e.at).collect::<Vec<_>>(), [1, 2, 3]);
    }

    /// A repo with a.txt committed as `head` (at time 2000 s), now `now`: (repo, its folder).
    fn repo_with(name: &str, head: &str, now: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("cue-theirs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args)
            .env("GIT_COMMITTER_DATE", "@2000 +0000").env("GIT_AUTHOR_DATE", "@2000 +0000").output().unwrap().status.success(), "{args:?}");
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "T"]);
        std::fs::write(dir.join("a.txt"), head).unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-q", "-m", "a"]);
        std::fs::write(dir.join("a.txt"), now).unwrap();
        let folder = dir.to_string_lossy().into_owned();
        (dir, folder)
    }

    /// The item's own change by its text, and a line no log shows: only the item's change is committed, the other line
    /// stays in the file (#248). When the two touch the same line, the commit waits on "someone else".
    #[test]
    fn a_change_no_log_shows_is_left_out_or_waited_on_as_someone_elses() {
        let (dir, folder) = repo_with("unshown", "one\ntwo\nthree\nfour\nfive\n", "ONE\ntwo\nthree\nfour\nfive\nby hand\n");
        let mine = |old: &str, new: &str| super::Theirs { mine: vec![(format!("{folder}/a.txt"), edit(3_000_000, old, new))], mine_known: true, ..Default::default() };
        assert_eq!(super::own_part(&mine("one\n", "ONE\n"), &folder, "a.txt"), super::Own::Part(b"ONE\ntwo\nthree\nfour\nfive\n".to_vec()));
        assert_eq!(super::own_part(&super::Theirs { mine_known: false, ..mine("one\n", "ONE\n") }, &folder, "a.txt"), super::Own::Whole, "the item's logs not all here: as it is");
        std::fs::write(dir.join("a.txt"), "ONE!\ntwo\nthree\nfour\nfive\n").unwrap();
        assert_eq!(super::own_part(&mine("one\n", "ONE\n"), &folder, "a.txt"), super::Own::Mixed("someone else".into()), "the same line changed again by hand");
        std::fs::write(dir.join("a.txt"), "ONE\ntwo\nthree\nfour\nfive\n").unwrap();
        assert_eq!(super::own_part(&mine("one\n", "ONE\n"), &folder, "a.txt"), super::Own::Whole, "only the item's change");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The item wrote the file whole: a change no log shows is told by the file's time, well after the item's write
    /// (#248).
    #[test]
    fn after_a_whole_file_write_a_later_change_is_someone_elses() {
        let (dir, folder) = repo_with("unshown-w", "a\n", "b\n");
        let wrote = |at: u64| super::Theirs { mine: vec![(format!("{folder}/a.txt"), TheirEdit { blind: true, ..edit(at, "", "") })], mine_known: true, ..Default::default() };
        let changed = std::fs::metadata(dir.join("a.txt")).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        assert_eq!(super::own_part(&wrote(changed - 1000), &folder, "a.txt"), super::Own::Whole, "changed as it wrote it");
        assert_eq!(super::own_part(&wrote(changed - 10 * 60_000), &folder, "a.txt"), super::Own::Mixed("someone else".into()), "changed ten minutes after");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Claude Code line says the folder it ran in: a helper working in a separate copy of the repo writes there,
    /// so its relative write isn't one in the session's own folder (#248).
    #[test]
    fn a_relative_write_is_taken_in_the_folder_its_line_ran_in() {
        let dir = std::env::temp_dir().join(format!("cue-theirs-cwd-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("agent-h.jsonl");
        let bash = |cwd: &str| format!("{{\"timestamp\":\"2026-11-18T12:00:00Z\",\"cwd\":\"{cwd}\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"Bash\",\"input\":{{\"command\":\"echo hi > a.txt\"}}}}]}}}}\n");
        std::fs::write(&log, bash("/copy")).unwrap();
        let path = log.to_string_lossy().into_owned();
        let wanted = vec!["/shared/a.txt".to_string(), "/copy/a.txt".to_string()];
        let files = |got: Vec<(String, TheirEdit)>| got.into_iter().map(|(f, _)| f).collect::<Vec<_>>();
        assert_eq!(files(read_edits(&path, "/shared", &wanted).0), ["/copy/a.txt"], "the copy's file, not the shared one");
        std::fs::write(&log, format!("{}{}", bash("/copy"), bash("/shared"))).unwrap();
        assert_eq!(files(read_edits(&path, "/shared", &wanted).0), ["/copy/a.txt", "/shared/a.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file gone (or not text) names only a change made since its last commit: an older edit of theirs isn't
    /// shown as the reason it waits (#248).
    #[test]
    fn a_gone_file_names_only_a_change_since_its_last_commit() {
        let dir = std::env::temp_dir().join(format!("cue-theirs-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args)
            .env("GIT_COMMITTER_DATE", "@2000 +0000").env("GIT_AUTHOR_DATE", "@2000 +0000").output().unwrap().status.success(), "{args:?}");
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "T"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-q", "-m", "a"]);
        std::fs::remove_file(dir.join("a.txt")).unwrap();
        let folder = dir.to_string_lossy().into_owned();
        let theirs = |at: u64| super::Theirs { edits: vec![(format!("{folder}/a.txt"), edit(at, "a\n", "b\n"))], ..Default::default() };
        assert_eq!(super::own_part(&theirs(1_000_000), &folder, "a.txt"), super::Own::Whole, "made before its commit");
        assert_eq!(super::own_part(&theirs(3_000_000), &folder, "a.txt"), super::Own::Mixed("tidier".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
