//! Open: what a lead started and left loose, for the Inbox side's "Open N" (ui/inbox_open.js). Each row is built from
//! what Cue knows, never from a session's words: its asks waiting on you (the store's items), its crew's records
//! (sessions/team.rs: who finished, whose review says send fixes and wasn't sent), and git's word on the crew's
//! folders (uncommitted files, a merge stopped at conflicts).
//!
//! Git is read off the hub's threads only (`open_scan`): as a lead's turn ends and on a slow tick, one scan at a time,
//! and kept (`Store::open_git`). The snapshot builds the rows from what's kept, so a draw never waits on git. A remote
//! session's folders are never read from here (not git, not a stat): its rows come from its records alone.

use super::*;
use crate::sessions::team::{fix_findings, Teammate};
use serde::Serialize;

/// How often the slow tick reads the crew's folders again.
const SCAN_EVERY_MS: u64 = 60_000;
/// The files a merge row names, then "+N".
const NAMED_FILES: usize = 2;

/// What git says of one folder: its uncommitted paths (relative to the folder when inside it), those unmerged, and
/// whether a merge is under way there.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct GitLook {
    pub(super) dirty: Vec<String>,
    pub(super) conflicts: Vec<String>,
    pub(super) merging: bool,
}

/// One loose end: what it is (`kind` ask · merge · uncommitted · review), its line, whose it is, the session it opens
/// (and the teammate or ask in it), its one action (reply · open · fixes), and whether it waits on you.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(super) struct OpenRow {
    pub(super) key: String,
    pub(super) kind: &'static str,
    pub(super) title: String,
    pub(super) who: String,
    pub(super) sid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(super) agent: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(super) item: String,
    pub(super) act: &'static str,
    pub(super) needs_you: bool,
}

/// A lead's loose ends.
#[derive(Clone, Debug, Serialize)]
pub(super) struct OpenLead {
    pub(super) lead_sid: String,
    pub(super) lead: String,
    pub(super) project: String,
    pub(super) rows: Vec<OpenRow>,
}

/// What one lead's rows are made from.
pub(super) struct LeadFacts<'a> {
    pub(super) sid: &'a str,
    pub(super) name: &'a str,
    pub(super) cwd: &'a str,
    /// Its host isn't this Mac: nothing of git is known, its crew's own records stand.
    pub(super) remote: bool,
    pub(super) team: &'a [Teammate],
    /// Its asks waiting on you (its own and its teammates').
    pub(super) asks: Vec<&'a Item>,
}

/// A teammate that finished (or stopped, or is past its review) and is still in the crew.
fn finished(t: &Teammate) -> bool {
    t.in_team() && !t.working()
}

/// Its review says send its fixes back, and they haven't gone (nor wait on its lead to go).
fn fixes_unsent(t: &Teammate) -> bool {
    if t.state != "reviewed" || t.fixes_kept_ms > 0 {
        return false;
    }
    let rec = t.recommendation.to_lowercase();
    rec.contains("fix") || (rec.starts_with("commit") && fix_findings(&t.findings) > 0) || (rec.is_empty() && !t.findings.is_empty())
}

/// A finding's words, its number and kind taken off ("1. a.rs:3 — should-fix — stale comment" → "a.rs:3 — stale
/// comment"), as the crew's page shows it.
fn finding_text(line: &str) -> String {
    let mut s = line.trim();
    if let Some((n, rest)) = s.split_once('.') {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            s = rest.trim_start();
        }
    }
    let mut out = s.to_string();
    for k in ["blocker", "must-fix", "should-fix", "nit", "note", "question"] {
        for form in [k.to_string(), k.to_uppercase()] {
            out = out.replace(&format!(" — {form} — "), " — ");
        }
    }
    out.trim_start_matches(['—', ' ']).to_string()
}

/// The finding a "Review not sent" row names: the first that calls for a change, else the first.
fn first_finding(t: &Teammate) -> String {
    let fix = t.findings.iter().find(|f| fix_findings(std::slice::from_ref(f)) > 0);
    fix.or(t.findings.first()).map(|f| finding_text(f)).unwrap_or_else(|| "its fixes".into())
}

/// An ask in a few words: a question's first question, else what it wants to run or use.
fn ask_title(it: &Item) -> String {
    if it.kind == "question" {
        let q = it.tool_input.pointer("/questions/0/question").and_then(|v| v.as_str()).unwrap_or("");
        return if q.is_empty() { "A question".into() } else { q.to_string() };
    }
    let cmd = it.tool_input.get("command").and_then(|v| v.as_str()).unwrap_or("");
    match (it.tool_name.as_str(), cmd) {
        (_, c) if !c.is_empty() => format!("Run: {}", c.lines().next().unwrap_or(c)),
        ("", _) => "Wants your answer".into(),
        (tool, _) => format!("Use {tool}"),
    }
}

/// "in a.rs, b.rs" (+N past NAMED_FILES).
fn files_words(files: &[String]) -> String {
    let named = files.iter().take(NAMED_FILES).cloned().collect::<Vec<_>>().join(", ");
    let more = files.len().saturating_sub(NAMED_FILES);
    if more > 0 { format!("in {named} +{more}") } else { format!("in {named}") }
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// Teammate `t`'s uncommitted files, by git when its folder was read (its whole worktree; in the lead's folder, its
/// files git says changed); by its record alone for a remote lead. None: not known yet.
fn uncommitted(f: &LeadFacts, t: &Teammate, looks: &HashMap<String, GitLook>) -> Option<Vec<String>> {
    if f.remote {
        return Some(t.files.clone());
    }
    if !t.worktree.is_empty() {
        return looks.get(&t.worktree).map(|g| g.dirty.clone());
    }
    let g = looks.get(f.cwd)?;
    Some(t.files.iter().filter(|p| g.dirty.contains(p)).cloned().collect())
}

/// The folder a merge row is about, with whose it is and what it opens: a teammate's worktree, or the lead's folder.
fn merge_row(key: String, name: &str, sid: &str, agent: &str, g: &GitLook) -> Option<OpenRow> {
    if g.conflicts.is_empty() && !g.merging {
        return None;
    }
    let title = if g.conflicts.is_empty() { "Merge not finished".to_string() } else { format!("Merge stopped at {}", plural(g.conflicts.len(), "conflict")) };
    let who = if g.conflicts.is_empty() { name.to_string() } else { format!("{name} · {}", files_words(&g.conflicts)) };
    Some(OpenRow { key, kind: "merge", title, who, sid: sid.into(), agent: agent.into(), act: "open", ..Default::default() })
}

/// A lead's loose ends, in kinds' order: its asks (waiting on you), merges stopped, finished crew not committed, then
/// reviews whose fixes weren't sent. The window keeps each row where it first showed (rows stay put).
pub(super) fn lead_rows(f: &LeadFacts, looks: &HashMap<String, GitLook>) -> Vec<OpenRow> {
    let mut rows: Vec<OpenRow> = f.asks.iter().map(|it| ask_row(f, it)).collect();
    let (merges, merging) = merge_rows(f, looks);
    rows.extend(merges);
    let unsent: Vec<&Teammate> = f.team.iter().filter(|t| t.in_team() && fixes_unsent(t)).collect();
    rows.extend(loose_row(f, looks, &merging, &unsent));
    rows.extend(unsent.into_iter().map(|t| OpenRow {
        key: format!("review:{}:{}", f.sid, t.agent_id),
        kind: "review",
        title: format!("Review not sent: {}", first_finding(t)),
        who: format!("review · for {}", t.name),
        sid: f.sid.into(),
        agent: t.agent_id.clone(),
        act: "fixes",
        ..Default::default()
    }));
    rows
}

/// An ask, red: whose it is (the teammate that asked, else the lead) is waiting on you.
fn ask_row(f: &LeadFacts, it: &Item) -> OpenRow {
    let mate = (!it.agent_id.is_empty()).then(|| f.team.iter().find(|t| t.agent_id == it.agent_id)).flatten();
    let name = mate.map(|t| t.name.as_str()).or((!it.agent_name.is_empty()).then_some(it.agent_name.as_str())).unwrap_or(f.name);
    OpenRow {
        key: format!("ask:{}", it.id),
        kind: "ask",
        title: ask_title(it),
        who: format!("{name} · waiting on you"),
        sid: f.sid.into(),
        agent: it.agent_id.clone(),
        item: it.id.clone(),
        act: "reply",
        needs_you: true,
    }
}

/// Merges stopped: in the lead's own folder, and in each teammate's worktree, with the teammates (by call) that have
/// one. Never read for a remote lead.
fn merge_rows<'a>(f: &LeadFacts<'a>, looks: &HashMap<String, GitLook>) -> (Vec<OpenRow>, Vec<&'a str>) {
    let (mut rows, mut merging) = (vec![], vec![]);
    if f.remote {
        return (rows, merging);
    }
    if let Some(r) = looks.get(f.cwd).and_then(|g| merge_row(format!("merge:{}", f.cwd), f.name, f.sid, "", g)) {
        rows.push(r);
    }
    for t in f.team.iter().filter(|t| t.in_team() && !t.worktree.is_empty()) {
        if let Some(r) = looks.get(&t.worktree).and_then(|g| merge_row(format!("merge:{}", t.worktree), &t.name, f.sid, &t.agent_id, g)) {
            merging.push(t.call.as_str());
            rows.push(r);
        }
    }
    (rows, merging)
}

/// Finished and not committed: one row for the crew (one whose fixes are owed, or whose merge stopped, has its own).
fn loose_row(f: &LeadFacts, looks: &HashMap<String, GitLook>, merging: &[&str], unsent: &[&Teammate]) -> Option<OpenRow> {
    let loose: Vec<(&Teammate, usize)> = f.team.iter()
        .filter(|t| finished(t) && !merging.contains(&t.call.as_str()) && !unsent.iter().any(|u| u.call == t.call))
        .filter_map(|t| uncommitted(f, t, looks).filter(|files| !files.is_empty()).map(|files| (t, files.len())))
        .collect();
    if loose.is_empty() {
        return None;
    }
    let who = loose.iter().map(|(t, n)| format!("{} {}", t.name, plural(*n, "file"))).collect::<Vec<_>>().join(" · ");
    let agent = if loose.len() == 1 { loose[0].0.agent_id.clone() } else { String::new() };
    Some(OpenRow {
        key: format!("uncommitted:{}", f.sid),
        kind: "uncommitted",
        title: format!("{} done, not committed", plural(loose.len(), "teammate")),
        who,
        sid: f.sid.into(),
        agent,
        act: "open",
        ..Default::default()
    })
}

/// `dir`'s git state, read here (never a remote folder: the caller only hands this Mac's). None: not a repository, or
/// git couldn't say.
pub(super) fn look(dir: &str) -> Option<GitLook> {
    if !std::path::Path::new(dir).is_dir() {
        return None;
    }
    let (out, code) = crate::safe_git::run("", dir, &[], &["rev-parse", "--show-prefix", "--git-path", "MERGE_HEAD"]).ok()?;
    if code != 0 {
        return None;
    }
    let mut lines = out.lines();
    let prefix = lines.next().unwrap_or("").to_string();
    let head = lines.next().unwrap_or("");
    let head = if head.starts_with('/') { std::path::PathBuf::from(head) } else { std::path::Path::new(dir).join(head) };
    let filters = crate::safe_git::no_repo_filters("", dir);
    let (out, code) = crate::safe_git::run("", dir, &filters, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]).ok()?;
    if code != 0 {
        return None;
    }
    let mut g = GitLook { merging: head.is_file(), ..Default::default() };
    let mut parts = out.split('\0').filter(|p| p.len() > 3);
    while let Some(p) = parts.next() {
        let (xy, path) = p.split_at(3);
        let xy = xy.trim_end();
        if xy.starts_with('R') || xy.starts_with('C') {
            parts.next();
        }
        // Paths come from the repository's top: shown from the folder when inside it.
        let path = path.strip_prefix(prefix.as_str()).unwrap_or(path).to_string();
        if matches!(xy, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU") {
            g.conflicts.push(path.clone());
        }
        g.dirty.push(path);
    }
    Some(g)
}

impl Hub {
    /// Every lead's loose ends, for the snapshot: built from what the store keeps, git as last read. Leads with none
    /// are left out.
    pub(super) fn open_view(st: &Store) -> Vec<OpenLead> {
        let mut out: Vec<OpenLead> = vec![];
        for s in st.sessions.iter().filter(|s| !s.team.is_empty()) {
            let sid = s.origin.session_id.as_str();
            let name = crate::desk::hub::core::desk::shown_name(s);
            let asks = st.items.iter().filter(|i| i.origin.session_id == sid && i.status == "pending" && i.kind != "waiting" && i.lead_has.is_none()).collect();
            let f = LeadFacts { sid, name: &name, cwd: &s.origin.cwd, remote: !s.origin.machine.is_empty(), team: &s.team, asks };
            let rows = lead_rows(&f, &st.open_git);
            if !rows.is_empty() {
                out.push(OpenLead { lead_sid: sid.into(), lead: name.clone(), project: s.project.clone(), rows });
            }
        }
        out.sort_by(|a, b| a.lead_sid.cmp(&b.lead_sid));
        out
    }

    /// The folders a scan reads: each lead's on this Mac with a crew still in it, and its crew's own worktrees there.
    fn open_folders(st: &Store) -> Vec<String> {
        let mut dirs: Vec<String> = vec![];
        for s in st.sessions.iter().filter(|s| s.origin.machine.is_empty() && s.team.iter().any(Teammate::in_team)) {
            if !s.origin.cwd.is_empty() {
                dirs.push(s.origin.cwd.clone());
            }
            for t in s.team.iter().filter(|t| t.in_team() && t.worktree.starts_with('/')) {
                dirs.push(t.worktree.clone());
            }
        }
        dirs.sort();
        dirs.dedup();
        dirs
    }

    /// The slow tick's part: the crew's folders read again once SCAN_EVERY_MS has passed since the last read.
    pub(super) fn open_tick(&self, now: u64) {
        if self.read(|st| now.saturating_sub(st.open_scan_ms) >= SCAN_EVERY_MS) {
            self.open_scan();
        }
    }

    /// Read the crew's folders with git off this thread, one scan at a time (one asked meanwhile runs after it), and
    /// keep what it says; drawn when it changed.
    pub(super) fn open_scan(&self) {
        let start = self.write(|st| {
            if st.open_scanning {
                st.open_again = true;
                return false;
            }
            st.open_scanning = true;
            st.open_again = false;
            st.open_scan_ms = now_ms();
            true
        });
        if start {
            self.later(0, |h| h.open_scan_now());
        }
    }

    /// The scan itself, on this thread.
    fn open_scan_now(&self) {
        let dirs = self.read(Self::open_folders);
        let looks: HashMap<String, GitLook> = dirs.into_iter().filter_map(|d| look(&d).map(|g| (d, g))).collect();
        let (changed, again) = self.write(|st| {
            let changed = st.open_git != looks;
            st.open_git = looks;
            st.open_scanning = false;
            (changed, std::mem::take(&mut st.open_again))
        });
        if changed {
            self.changed();
        }
        if again {
            self.open_scan();
        }
    }
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
