//! Open's rows (open.rs) against real git folders made in a temp folder: a lead's crew finished and not committed,
//! a worktree stopped mid-merge, a review whose fixes weren't sent, an ask waiting on you; and a remote lead whose
//! folders are never read.

use super::*;
use std::path::{Path, PathBuf};

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new(crate::safe_git::git_bin()).args(args).current_dir(dir).env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_NOSYSTEM", "1").output().unwrap();
    assert!(out.status.success() || args[0] == "merge", "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A scratch folder under the temp folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cue-open-{name}-{}-{}", std::process::id(), now_ms()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    fn path(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().to_string()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A repository at `dir` with one commit on main: `a.txt` and `ui/b.js`.
fn repo(dir: &Path) {
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "T"]);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    std::fs::write(dir.join("ui/b.js"), "one\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "start"]);
}

/// A worktree of `lead` at `at` on a new branch `branch`.
fn worktree(lead: &Path, at: &str, branch: &str) {
    git(lead, &["worktree", "add", "-q", "-b", branch, at]);
}

fn mate(name: &str, state: &str, worktree: &str, files: &[&str]) -> Teammate {
    Teammate {
        call: format!("call-{name}"),
        agent_id: format!("a-{name}"),
        name: name.into(),
        state: state.into(),
        worktree: worktree.into(),
        files: files.iter().map(|f| f.to_string()).collect(),
        ..Default::default()
    }
}

fn facts<'a>(cwd: &'a str, team: &'a [Teammate], asks: Vec<&'a Item>, remote: bool) -> LeadFacts<'a> {
    LeadFacts { sid: "lead-1", name: "desk-lead", cwd, remote, team, asks }
}

/// Git's look at each folder, as a scan keeps it.
fn looks(dirs: &[&str]) -> HashMap<String, GitLook> {
    dirs.iter().filter_map(|d| look(d).map(|g| (d.to_string(), g))).collect()
}

fn kinds(rows: &[OpenRow]) -> Vec<&str> {
    rows.iter().map(|r| r.kind).collect()
}

#[test]
fn finished_teammates_with_uncommitted_work_are_one_row_by_git_and_a_committed_one_isnt_in_it() {
    let s = Scratch::new("uncommitted");
    let lead = s.0.join("lead");
    repo(&lead);
    let (ui, core, clean) = (s.path("wt-ui"), s.path("wt-core"), s.path("wt-clean"));
    worktree(&lead, &ui, "crew/ui");
    worktree(&lead, &core, "crew/core");
    worktree(&lead, &clean, "crew/clean");
    // desk-lead-ui changed two files and wrote a new one; desk-lead-core one; desk-lead-clean committed its own.
    std::fs::write(Path::new(&ui).join("a.txt"), "two\n").unwrap();
    std::fs::write(Path::new(&ui).join("ui/b.js"), "two\n").unwrap();
    std::fs::write(Path::new(&ui).join("ui/new.js"), "x\n").unwrap();
    std::fs::write(Path::new(&core).join("a.txt"), "three\n").unwrap();
    std::fs::write(Path::new(&clean).join("a.txt"), "four\n").unwrap();
    git(Path::new(&clean), &["commit", "-q", "-am", "its work"]);
    let cwd = lead.to_string_lossy().to_string();
    let team = vec![mate("desk-lead-ui", "done", &ui, &[]), mate("desk-lead-core", "stopped", &core, &[]), mate("desk-lead-clean", "done", &clean, &[]), mate("busy", "working", &s.path("nowhere"), &[])];
    let rows = lead_rows(&facts(&cwd, &team, vec![], false), &looks(&[&cwd, &ui, &core, &clean]));
    assert_eq!(kinds(&rows), ["uncommitted"]);
    assert_eq!(rows[0].title, "2 teammates done, not committed");
    assert_eq!(rows[0].who, "desk-lead-ui 3 files · desk-lead-core 1 file");
    assert_eq!((rows[0].act, rows[0].needs_you, rows[0].sid.as_str()), ("open", false, "lead-1"));
    // Not read yet (no scan has run): nothing is said.
    assert!(lead_rows(&facts(&cwd, &team, vec![], false), &HashMap::new()).is_empty());
}

#[test]
fn a_teammate_in_the_leads_own_folder_counts_only_its_own_files_that_git_says_changed() {
    let s = Scratch::new("own-folder");
    let lead = s.0.join("lead");
    repo(&lead);
    // Its files: ui/b.js (changed) and a.txt (put back); the lead's own change elsewhere isn't its.
    std::fs::write(lead.join("ui/b.js"), "two\n").unwrap();
    std::fs::write(lead.join("other.txt"), "the lead's\n").unwrap();
    let cwd = lead.to_string_lossy().to_string();
    let team = vec![mate("helper", "done", "", &["ui/b.js", "a.txt"])];
    let rows = lead_rows(&facts(&cwd, &team, vec![], false), &looks(&[&cwd]));
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].title.as_str(), rows[0].who.as_str(), rows[0].agent.as_str()), ("1 teammate done, not committed", "helper 1 file", "a-helper"));
    // A lead in a subfolder of its repository: git's paths are read from the folder.
    let sub = lead.join("ui").to_string_lossy().to_string();
    let team = vec![mate("helper", "done", "", &["b.js"])];
    assert_eq!(lead_rows(&facts(&sub, &team, vec![], false), &looks(&[&sub]))[0].who, "helper 1 file");
}

#[test]
fn a_worktree_stopped_mid_merge_says_how_many_conflicts_and_where_and_isnt_also_counted_uncommitted() {
    let s = Scratch::new("merge");
    let lead = s.0.join("lead");
    repo(&lead);
    let wt = s.path("wt-merge");
    worktree(&lead, &wt, "crew/merge");
    let w = Path::new(&wt);
    std::fs::write(w.join("a.txt"), "theirs\n").unwrap();
    std::fs::write(w.join("ui/b.js"), "theirs\n").unwrap();
    git(w, &["commit", "-q", "-am", "theirs"]);
    std::fs::write(lead.join("a.txt"), "ours\n").unwrap();
    std::fs::write(lead.join("ui/b.js"), "ours\n").unwrap();
    git(&lead, &["commit", "-q", "-am", "ours"]);
    git(w, &["merge", "-q", "main"]);
    let cwd = lead.to_string_lossy().to_string();
    let g = look(&wt).unwrap();
    assert!(g.merging);
    assert_eq!(g.conflicts, ["a.txt", "ui/b.js"]);
    let team = vec![mate("desk-merge", "stopped", &wt, &[])];
    let rows = lead_rows(&facts(&cwd, &team, vec![], false), &looks(&[&cwd, &wt]));
    assert_eq!(kinds(&rows), ["merge"]);
    assert_eq!(rows[0].title, "Merge stopped at 2 conflicts");
    assert_eq!(rows[0].who, "desk-merge · in a.txt, ui/b.js");
    assert_eq!((rows[0].act, rows[0].agent.as_str(), rows[0].key.as_str()), ("open", "a-desk-merge", format!("merge:{wt}").as_str()));
    // A clean folder: no merge.
    assert_eq!(look(&cwd).unwrap(), GitLook::default());
}

#[test]
fn a_review_saying_send_fixes_that_wasnt_sent_is_its_own_row_naming_its_first_fix() {
    let mut t = mate("desk-lead-core", "reviewed", "", &["a.txt"]);
    t.recommendation = "send fixes".into();
    t.findings = vec!["1. desk/view.rs:3 — note — fine as is".into(), "2. desk/model.rs:40 — should-fix — stale comment".into()];
    let team = vec![t.clone()];
    let rows = lead_rows(&facts("/nowhere", &team, vec![], true), &HashMap::new());
    assert_eq!(kinds(&rows), ["review"]);
    assert_eq!(rows[0].title, "Review not sent: desk/model.rs:40 — stale comment");
    assert_eq!((rows[0].who.as_str(), rows[0].act, rows[0].agent.as_str()), ("review · for desk-lead-core", "fixes", "a-desk-lead-core"));
    // Kept for its busy lead, sent (back at work), or a clean commit: none.
    for f in [|t: &mut Teammate| t.fixes_kept_ms = 5, |t: &mut Teammate| t.state = "working".into(), |t: &mut Teammate| { t.recommendation = "commit".into(); t.findings.truncate(1); }] {
        let mut m = t.clone();
        f(&mut m);
        let team = vec![m];
        assert!(!lead_rows(&facts("/nowhere", &team, vec![], true), &HashMap::new()).iter().any(|r| r.kind == "review"));
    }
}

#[test]
fn an_ask_waiting_on_you_is_a_red_row_by_who_asks_it() {
    let mut own = Item { id: "n1".into(), kind: "question".into(), status: "pending".into(), ..test_item() };
    own.tool_input = serde_json::json!({ "questions": [{ "question": "Merge to main?" }] });
    let mut mates = Item { id: "n2".into(), kind: "permission".into(), status: "pending".into(), tool_name: "Bash".into(), agent_id: "a-desk-merge".into(), ..test_item() };
    mates.tool_input = serde_json::json!({ "command": "git push origin main" });
    let team = vec![mate("desk-merge", "working", "", &[])];
    let rows = lead_rows(&facts("/nowhere", &team, vec![&own, &mates], true), &HashMap::new());
    assert_eq!(kinds(&rows), ["ask", "ask"]);
    assert_eq!((rows[0].title.as_str(), rows[0].who.as_str(), rows[0].act, rows[0].needs_you, rows[0].item.as_str()), ("Merge to main?", "desk-lead · waiting on you", "reply", true, "n1"));
    assert_eq!((rows[1].title.as_str(), rows[1].who.as_str()), ("Run: git push origin main", "desk-merge · waiting on you"));
}

#[test]
fn a_remote_leads_folders_are_never_read_its_crews_records_stand() {
    // A folder that exists here would be read for a lead on this Mac; for a remote lead nothing is looked up.
    let s = Scratch::new("remote");
    let lead = s.0.join("lead");
    repo(&lead);
    let cwd = lead.to_string_lossy().to_string();
    let team = vec![mate("far", "done", "/home/me/wt", &["a.rs", "b.rs"]), mate("far-clean", "done", "", &[])];
    let rows = lead_rows(&facts(&cwd, &team, vec![], true), &looks(&[&cwd]));
    assert_eq!(kinds(&rows), ["uncommitted"]);
    assert_eq!(rows[0].who, "far 2 files");
}

#[test]
fn the_scan_reads_only_this_macs_crew_folders_and_the_snapshot_carries_each_leads_rows() {
    let s = Scratch::new("hub");
    let lead = s.0.join("lead");
    repo(&lead);
    let wt = s.path("wt");
    worktree(&lead, &wt, "crew/x");
    std::fs::write(Path::new(&wt).join("a.txt"), "two\n").unwrap();
    let _w = crate::testing::World::new();
    let hub = Hub::new(None);
    let cwd = lead.to_string_lossy().to_string();
    hub.write(|st| {
        let here = Origin { session_id: "here".into(), cwd: cwd.clone(), ..Default::default() };
        let far = Origin { session_id: "far".into(), cwd: "/home/me/repo".into(), machine: "box".into(), ..Default::default() };
        st.sessions.mark(&here, "idle", None);
        st.sessions.mark(&far, "idle", None);
        st.sessions.add_teammate("here", mate("x", "done", &wt, &[]));
        st.sessions.add_teammate("far", mate("y", "done", "/home/me/wt", &["z.rs"]));
    });
    let dirs = hub.read(Hub::open_folders);
    assert_eq!(dirs, [cwd.clone(), wt.clone()], "never the remote lead's folders");
    hub.open_scan_now();
    let open = hub.read(Hub::open_view);
    let by: Vec<(&str, &str)> = open.iter().map(|l| (l.lead_sid.as_str(), l.rows[0].who.as_str())).collect();
    assert_eq!(by, [("far", "y 1 file"), ("here", "x 1 file")]);
}

fn test_item() -> Item {
    serde_json::from_value(serde_json::json!({
        "id": "", "kind": "", "origin": {}, "project": "", "tool_name": "", "tool_input": null, "suggestions": null, "context": [],
        "message": "", "created_ms": 0, "status": "", "outcome": "", "resolved_ms": null
    }))
    .unwrap()
}
