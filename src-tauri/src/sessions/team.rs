//! A hosted lead's team (docs/dev/cue-teams.md): the named agents (teammates) it started with its Agent tool, each
//! kept on the lead's record from what its stream says, so a restart or `--resume` (which never replays a teammate's
//! earlier lines) finds them as they were. Pure: the lead's stream events in, the team out; no process, no clock.
//!
//! A teammate is keyed by the Agent call that started it (`call`: every line of its carries it as its parent) and,
//! from its `task_started` on, by its agent id (the one its asks and a later `SendMessage` resume carry).

use super::Sessions;
use crate::hosted::HostedEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The tools a teammate changes files with, and where each names the file. Its Bash commands change files too: in a
/// worktree of its own, git says which (hub/team.rs reads its status after each and when it finishes); in the lead's
/// folder, `shell_writes` reads them off the command, marked as `via_shell`.
/// SHORTCUT: in the lead's folder a command's files are read off its words, not git (others may be changing the same
/// folder at once). Upgrade (docs/dev/cue-teams.md §7): git status before and after each of its Bash calls.
const EDITS: [(&str, &str); 4] = [("Edit", "file_path"), ("MultiEdit", "file_path"), ("Write", "file_path"), ("NotebookEdit", "notebook_path")];

/// One teammate of a lead.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Teammate {
    /// The lead's Agent call that started it: its lines carry it as their parent, before and after a resume.
    pub call: String,
    /// Its agent id (its `task_started`'s task id); "" until it has started.
    #[serde(default)]
    pub agent_id: String,
    /// What the lead named it (the Agent call's `name`).
    pub name: String,
    /// What it was asked to do: the Agent call's prompt, and its short description.
    #[serde(default)]
    pub asked: String,
    #[serde(default)]
    pub what: String,
    /// The folder it works in when the lead gave it a worktree of its own (`isolation: worktree`): Claude Code puts
    /// it at `<lead's folder>/.claude/worktrees/agent-<agent id>`, and says so nowhere on the stream. "" for one
    /// working in the lead's own folder.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub worktree: String,
    /// working · done (it finished) · stopped (it ended without finishing) · queued (your Review waits for the lead)
    /// · reviewing · reviewed · committed · aside (you set it aside). Needs you is its asks', not kept here.
    pub state: String,
    /// What it's doing now ("Running: cargo test", "Editing app.js"), and since when.
    #[serde(default)]
    pub step: String,
    #[serde(default)]
    pub step_ms: u64,
    /// What its last line on the stream was ("call": a tool call, its result not in; "result"; "said": its own
    /// words), for when it goes quiet (quiet.rs). "" until its first.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last: String,
    /// Gone quiet, as the hub's tick last judged it (quiet.rs): "tool" (a long tool call open) or "stuck"; "" while
    /// its lines come.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub quiet: String,
    /// The quiet stretch its lead was nudged about, by that stretch's last line (`step_ms`): one nudge a stretch.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub nudged_ms: u64,
    /// The files it changed with its own edit tools or its shell commands, relative to the folder it works in
    /// (absolute when outside it).
    #[serde(default)]
    pub files: Vec<String>,
    /// Those of `files` known only from reading its shell commands' words (in the lead's folder), not from an edit
    /// tool or git: shown as "via shell".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via_shell: Vec<String>,
    /// Its Bash calls under way in a worktree of its own: when one ends, git is asked what changed there.
    #[serde(skip)]
    pub bash_calls: Vec<String>,
    /// Its edit-tool calls under way, each with the file it names: the file is its own once the call succeeds (a
    /// refused or failed edit changed nothing).
    #[serde(skip)]
    pub edit_calls: Vec<(String, String)>,
    /// The other teammates (by name) with uncommitted changes in one of its files, in the same folder: Commit waits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
    /// The size of what it changed, read when it finished ("2 files +140 −0").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stat: String,
    /// Its last words (the finish's summary, else what it last said).
    #[serde(default)]
    pub summary: String,
    pub started_ms: u64,
    #[serde(default)]
    pub ended_ms: u64,
    /// Its review, once one came back: numbered findings and what the review recommends ("commit", "send fixes").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recommendation: String,
    /// When your Review went to the lead: the lead's next reply you started is its review.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub review_ms: u64,
    /// The commit its files went into.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit: String,
    /// The lead it was carried over from by a handoff (`carried`): its process ended with that lead, so nothing of it
    /// runs now; its files, state and review stay. "" for one this lead started.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from: String,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl Teammate {
    /// Still in the team: not committed or set aside.
    pub fn in_team(&self) -> bool {
        !matches!(self.state.as_str(), "committed" | "aside")
    }

    /// At work, by the stream.
    pub fn working(&self) -> bool {
        self.state == "working"
    }

    /// At work and quiet as of `now`: by its last line (quiet.rs). Active when not at work.
    pub fn quiet_now(&self, now: u64) -> super::quiet::Quiet {
        if !self.working() {
            return super::quiet::Quiet::Active;
        }
        super::quiet::judge(super::quiet::Last::from_word(&self.last), self.step_ms, now)
    }

    /// The folder its files are in: its worktree, else the lead's.
    pub fn folder<'a>(&'a self, lead_cwd: &'a str) -> &'a str {
        if self.worktree.is_empty() {
            lead_cwd
        } else {
            &self.worktree
        }
    }
}

/// What `e` (one event of the lead's stream) changes in `team`, at `now`: whether anything did. `cwd`: the lead's folder.
pub fn sees(team: &mut Vec<Teammate>, cwd: &str, e: &HostedEvent, now: u64) -> bool {
    let changed = match e {
        // The lead starts a teammate: an Agent call with a name.
        HostedEvent::ToolCall { id, name, input, agent: None } if name == "Agent" || name == "Task" => return started(team, id, input, now),
        HostedEvent::Task { what, detail } => task(team, cwd, what, detail, now),
        HostedEvent::ToolCall { id, name, input, agent: Some(call) } => {
            let Some(t) = team.iter_mut().find(|t| t.call == *call) else { return false };
            t.step = crate::logs::transcript::describe_tool(&serde_json::json!({ "name": name, "input": input }));
            heard(t, "call", now);
            back_at_work(t, now);
            if let Some((_, key)) = EDITS.iter().find(|(tool, _)| tool == name) {
                if let Some(path) = input.get(*key).and_then(Value::as_str).filter(|p| !p.is_empty()) {
                    let rel = relative(path, t.folder(cwd));
                    t.edit_calls.push((id.clone(), rel));
                }
            } else if name == "Bash" {
                if !t.worktree.is_empty() {
                    t.bash_calls.push(id.clone());
                } else if let Some(command) = input.get("command").and_then(Value::as_str) {
                    let folder = t.folder(cwd).trim_end_matches('/').to_string();
                    let files: Vec<String> = super::shell_writes::writes(command).into_iter().filter_map(|p| in_folder(&p, &folder)).collect();
                    add_files(t, files, true);
                }
            }
            true
        }
        // The lead's Agent call refused (a name Claude Code doesn't take, say): no teammate started.
        HostedEvent::ToolResult { id, failed: true, agent: None, .. } => refused(team, id),
        HostedEvent::ToolResult { id, failed, agent: Some(call), .. } => team.iter_mut().find(|t| t.call == *call).map(|t| {
            t.bash_calls.retain(|c| c != id);
            if let Some(i) = t.edit_calls.iter().position(|(c, _)| c == id) {
                let (_, rel) = t.edit_calls.remove(i);
                if !failed {
                    add_files(t, [rel], false);
                }
            }
            t.step = "Thinking…".into();
            heard(t, "result", now);
        }).is_some(),
        HostedEvent::Said { text, agent: Some(call) } => team.iter_mut().find(|t| t.call == *call).filter(|_| !text.trim().is_empty()).map(|t| {
            t.summary = first_line(text);
            t.step = "Writing…".into();
            heard(t, "said", now);
        }).is_some(),
        _ => false,
    };
    if changed {
        mark_shared(team);
    }
    changed
}

/// The lead's Agent call `id` with `input`: a teammate started when it names one (or one carried over a handoff,
/// started again under its name); whether the team changed.
fn started(team: &mut Vec<Teammate>, id: &str, input: &Value, now: u64) -> bool {
    let Some(mate) = input.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()) else { return false };
    if team.iter().any(|t| t.call == id) {
        return false;
    }
    let worktree = input.get("isolation").and_then(Value::as_str) == Some("worktree");
    // A teammate carried over a handoff, started again under its name (to fix what its review found): the
    // same row, at work again under its new call, its files kept.
    if let Some(t) = team.iter_mut().find(|t| !t.from.is_empty() && t.name == mate.trim() && t.in_team()) {
        t.call = id.to_string();
        t.agent_id.clear();
        t.from.clear();
        t.asked = str_of(input, "prompt");
        t.what = str_of(input, "description");
        t.worktree = if worktree { WORKTREE_PENDING.into() } else { String::new() };
        back_at_work(t, now);
        t.step = "Starting".into();
        t.step_ms = now;
        mark_shared(team);
        return true;
    }
    team.push(Teammate {
        call: id.to_string(),
        name: mate.trim().to_string(),
        asked: str_of(input, "prompt"),
        what: str_of(input, "description"),
        // Its folder once its agent id is known (task_started); marked meanwhile.
        worktree: if worktree { WORKTREE_PENDING.into() } else { String::new() },
        state: "working".into(),
        step: "Starting".into(),
        step_ms: now,
        started_ms: now,
        ..Default::default()
    });
    mark_shared(team);
    true
}

/// The lead's Agent call `id` came back an error before its agent started: a row made for it is dropped, as no
/// teammate ever ran. One carried over a handoff (it has files) is kept, stopped.
fn refused(team: &mut Vec<Teammate>, id: &str) -> bool {
    let Some(i) = team.iter().position(|t| t.call == id && t.agent_id.is_empty()) else { return false };
    if team[i].files.is_empty() {
        team.remove(i);
    } else {
        team[i].state = "stopped".into();
        team[i].step.clear();
    }
    true
}

/// Its folder is a worktree whose path waits for its agent id.
const WORKTREE_PENDING: &str = "?";

/// Claude's own task events: a teammate started (or resumed by `SendMessage`), or finished.
fn task(team: &mut [Teammate], cwd: &str, what: &str, d: &Value, now: u64) -> bool {
    let tool_use = d.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
    let task_id = d.get("task_id").and_then(Value::as_str).unwrap_or("");
    match what {
        // Only agents: a long command of a teammate's is a task of its own (local_bash), not a teammate.
        "task_started" if d.get("task_type").and_then(Value::as_str) == Some("local_agent") && !task_id.is_empty() => {
            // Its first start pairs its Agent call with its id; a resume (SendMessage) comes with the id it had.
            let Some(t) = team.iter_mut().find(|t| t.call == tool_use || t.agent_id == task_id) else { return false };
            t.agent_id = task_id.to_string();
            if t.worktree == WORKTREE_PENDING {
                t.worktree = format!("{}/.claude/worktrees/agent-{task_id}", cwd.trim_end_matches('/'));
            }
            back_at_work(t, now);
            heard(t, "result", now);
            true
        }
        "task_notification" if !task_id.is_empty() => {
            let Some(t) = team.iter_mut().find(|t| t.agent_id == task_id) else { return false };
            let status = d.get("status").and_then(Value::as_str).unwrap_or("completed");
            let summary = d.get("summary").and_then(Value::as_str).map(first_line).unwrap_or_default();
            if !summary.is_empty() {
                t.summary = summary;
            }
            if t.working() {
                t.state = if status == "completed" { "done" } else { "stopped" }.into();
            }
            t.step.clear();
            t.ended_ms = now;
            true
        }
        _ => false,
    }
}

/// A line from teammate `t` on the stream, of kind `last` ("call", "result", "said"): its quiet stretch, if any, ends.
fn heard(t: &mut Teammate, last: &str, now: u64) {
    t.last = last.to_string();
    t.quiet.clear();
    t.step_ms = now;
}

/// A teammate that does something again (Send fixes, a message through its lead) is at work again: its review, if it
/// had one, was of work it's now changing. Committed or set aside, it comes back to the team.
pub fn back_at_work(t: &mut Teammate, now: u64) {
    if t.working() {
        return;
    }
    t.state = "working".into();
    t.ended_ms = 0;
    t.findings.clear();
    t.recommendation.clear();
    t.review_ms = 0;
    t.commit.clear();
    t.stat.clear();
    if t.started_ms == 0 {
        t.started_ms = now;
    }
}

/// `files` (relative to its folder) are `t`'s: from its shell commands' words (`via_shell`), or from an edit tool or
/// git, which also takes a file off `via_shell`. Whether any was new to it.
pub fn add_files(t: &mut Teammate, files: impl IntoIterator<Item = String>, via_shell: bool) -> bool {
    let mut added = false;
    for f in files {
        if !via_shell {
            t.via_shell.retain(|v| *v != f);
        }
        if t.files.contains(&f) {
            continue;
        }
        if via_shell {
            t.via_shell.push(f.clone());
        }
        t.files.push(f);
        added = true;
    }
    added
}

/// `p` (a shell command's path: relative to `folder`, or absolute) relative to `folder`, when it's inside it and
/// not git's own.
fn in_folder(p: &str, folder: &str) -> Option<String> {
    let rel = if p.starts_with('/') { relative(p, folder) } else { p.to_string() };
    let outside = rel.starts_with('/') || rel == ".." || rel.starts_with("../");
    (!outside && !rel.is_empty() && rel != ".git" && !rel.starts_with(".git/")).then_some(rel)
}

/// Each teammate in the lead's own folder marked with the others (still in the team) that changed one of its files.
pub fn mark_shared(team: &mut [Teammate]) {
    let shared: Vec<(String, String, Vec<String>)> = team.iter().filter(|t| t.worktree.is_empty() && t.in_team()).map(|t| (t.call.clone(), t.name.clone(), t.files.clone())).collect();
    for t in team.iter_mut() {
        t.also = if t.worktree.is_empty() && t.in_team() {
            shared.iter().filter(|(call, _, files)| *call != t.call && files.iter().any(|f| t.files.contains(f))).map(|(_, name, _)| name.clone()).collect()
        } else {
            vec![]
        };
    }
}

/// `team` as the lead that carries on from its lead after a handoff gets it: every teammate still in the team, marked
/// as carried `from` that lead. Its process ended with that lead: one at work then is stopped (its files stay), and a
/// review that was on its way goes with it (the teammate waits for a Review again).
/// SHORTCUT: one still at work is marked stopped even when its old lead stays open (handoff keeps a lead whose
/// background agents run), where it may still finish; its row has left the old lead (`Sessions::carry_team`), so
/// that finish isn't shown. Upgrade path: carry a working teammate only once its old lead's process has ended, and
/// follow it there until then.
pub fn carried(team: &[Teammate], from: &str) -> Vec<Teammate> {
    team.iter()
        .filter(|t| t.in_team())
        .cloned()
        .map(|mut t| {
            t.from = from.to_string();
            match t.state.as_str() {
                "working" => {
                    t.state = "stopped".into();
                    t.step.clear();
                    t.summary = if t.summary.is_empty() { "Stopped by the handoff: its files stay".into() } else { t.summary };
                }
                "queued" | "reviewing" => {
                    t.state = "done".into();
                    t.review_ms = 0;
                }
                _ => {}
            }
            t
        })
        .collect()
}

/// The team as a few lines for the lead that carries on: who, in what state, what it was asked, its files.
pub fn carried_note(team: &[Teammate]) -> String {
    let lines: Vec<String> = team
        .iter()
        .map(|t| {
            let what = if t.what.is_empty() { first_line(&t.asked) } else { t.what.clone() };
            let files = if t.files.is_empty() { String::new() } else { format!(" Files: {}.", t.files.join(", ")) };
            let place = if t.worktree.is_empty() { String::new() } else { format!(" Worktree: {}.", t.worktree) };
            format!("- {}: {what} ({}).{files}{place}", t.name, t.state)
        })
        .collect();
    format!("The team from the session before you. Their processes ended with it, so they can't be messaged; the user reviews and commits their work in Cue.\n{}", lines.join("\n"))
}

/// A review's numbered findings (a few lines) and what it recommends, from the lead's reply.
pub fn read_review(text: &str) -> (Vec<String>, String) {
    let said = |l: &str| l.split_once(':').filter(|(k, _)| k.trim_matches(['*', ' ']).eq_ignore_ascii_case("recommendation")).map(|(_, v)| v.trim_matches(['*', ' ']).split(['.', '(', ',', ';']).next().unwrap_or("").trim().to_lowercase());
    let lines = || text.lines().map(str::trim).filter(|l| !l.is_empty());
    let recommendation = lines().find_map(said).unwrap_or_default();
    let numbered = |l: &&str| l.split_once('.').is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    let findings = lines().filter(numbered).take(10).map(String::from).collect();
    (findings, recommendation)
}

fn str_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn first_line(s: &str) -> String {
    let l = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    l.chars().take(200).collect()
}

/// `path` relative to `folder` when it's inside it. A Mac's /tmp and /var are /private/tmp and /private/var to the
/// agent (its own folder resolved), while Cue keeps the folder as it was given.
fn relative(path: &str, folder: &str) -> String {
    let folder = folder.trim_end_matches('/');
    if folder.is_empty() || folder == WORKTREE_PENDING {
        return path.to_string();
    }
    let inside = |p: &str| p.strip_prefix(folder).and_then(|r| r.strip_prefix('/')).map(str::to_string);
    inside(path).or_else(|| path.strip_prefix("/private").and_then(inside)).unwrap_or_else(|| path.to_string())
}

impl Sessions {
    /// `sid`'s team as it has it (none: not a lead with a team).
    pub fn team(&self, sid: &str) -> Vec<Teammate> {
        self.0.get(sid).map(|s| s.team.clone()).unwrap_or_default()
    }

    /// What `e` changes in `sid`'s team; whether anything did.
    pub fn team_sees(&mut self, sid: &str, e: &HostedEvent, now: u64) -> bool {
        let Some(s) = self.0.get_mut(sid) else { return false };
        let cwd = s.origin.cwd.clone();
        sees(&mut s.team, &cwd, e, now)
    }

    /// `f` on `sid`'s teammate `agent` (its agent id or its call): what it returns, None when there's no such teammate.
    pub fn with_teammate<R>(&mut self, sid: &str, agent: &str, f: impl FnOnce(&mut Teammate) -> R) -> Option<R> {
        let s = self.0.get_mut(sid)?;
        let t = s.team.iter_mut().find(|t| (!t.agent_id.is_empty() && t.agent_id == agent) || t.call == agent)?;
        let r = f(t);
        mark_shared(&mut s.team);
        Some(r)
    }

    /// `to` takes over `from`'s team (a handoff): its teammates still in the team, carried (`carried`), after any
    /// `to` started already. They leave `from`, so each shows once and is committed from one lead only (the ones
    /// out of the team, committed or set aside, stay with `from`). What it took.
    pub fn carry_team(&mut self, from: &str, to: &str) -> Vec<Teammate> {
        if from == to || !self.0.contains_key(to) {
            return vec![];
        }
        let took = self.0.get(from).map(|s| carried(&s.team, from)).unwrap_or_default();
        if let Some(old) = self.0.get_mut(from) {
            old.team.retain(|t| !t.in_team());
            mark_shared(&mut old.team);
        }
        let Some(s) = self.0.get_mut(to) else { return vec![] };
        let took: Vec<Teammate> = took.into_iter().filter(|t| !s.team.iter().any(|m| m.call == t.call)).collect();
        s.team.extend(took.iter().cloned());
        mark_shared(&mut s.team);
        took
    }

    /// The Team switch on `sid`'s box: "on", "off", or "" (auto: the lead decides).
    pub fn team_mode(&self, sid: &str) -> String {
        self.0.get(sid).map(|s| s.team_mode.clone()).unwrap_or_default()
    }

    pub fn set_team_mode(&mut self, sid: &str, mode: &str) -> bool {
        let Some(s) = self.0.get_mut(sid) else { return false };
        s.team_mode = if matches!(mode, "on" | "off") { mode.to_string() } else { String::new() };
        true
    }

    /// The name of `sid`'s teammate with agent id `agent_id`, when it's one.
    pub fn teammate_name(&self, sid: &str, agent_id: &str) -> Option<String> {
        self.0.get(sid)?.team.iter().find(|t| !agent_id.is_empty() && t.agent_id == agent_id).map(|t| t.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::relative;

    #[test]
    fn a_file_is_kept_relative_to_the_folder_the_agent_resolved_too() {
        assert_eq!(relative("/work/cue/ui/a.js", "/work/cue"), "ui/a.js");
        assert_eq!(relative("/private/tmp/repo/a.txt", "/tmp/repo"), "a.txt", "macOS's /tmp, as the agent sees it");
        assert_eq!(relative("/elsewhere/a.txt", "/work/cue"), "/elsewhere/a.txt", "outside its folder: whole");
        assert_eq!(relative("/work/cuex/a.txt", "/work/cue"), "/work/cuex/a.txt", "a folder beside it isn't inside it");
    }
}
