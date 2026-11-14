//! What a crew decided on its own (sessions/crew_log.rs): its job, one line per member's review that grows as Cue
//! sends fixes back and commits, Go back on this (the lead told, once), and a gone worktree that isn't reviewed.

use super::team::{git, lead, main_repo, mate, repo, says, writes};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::{json, Value};

fn log(w: &World, sid: &str) -> Vec<Value> {
    w.snapshot().session(sid)["crew_log"].as_array().cloned().unwrap_or_default()
}

/// Lead `sid`'s review of `name` filed: its ask sent once the lead is free, then the reply that names it.
fn reviewed(w: &World, sid: &str, name: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: "on it".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    w.advance(60_000);
    w.hub().drain_reviews();
    w.settle();
    w.hosted_says(sid, HostedEvent::TurnEnded { text: format!("Review of {name}:\n{text}"), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

/// A lead whose member rows was reviewed (one should-fix), sent back, fixed and committed, all by Cue.
fn committed_by_itself(w: &World) -> (String, std::path::PathBuf) {
    let dir = main_repo(w);
    let sid = lead(w, "Two parts: rows and docs\nmore words");
    says(w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    reviewed(w, &sid, "rows", "1. a.txt:1 — should-fix — say why\nRecommendation: commit");
    assert_eq!(log(w, &sid).len(), 1);
    assert_eq!(log(w, &sid)[0]["what"], "Reviewed: 1 should-fix. Sent back");
    w.hosted_says(&sid, HostedEvent::Task { what: "task_notification".into(), detail: json!({ "task_id": "agentA", "status": "completed", "summary": "Said why." }) });
    w.settle();
    assert_eq!(log(w, &sid)[0]["what"], "Reviewed: 1 should-fix. Sent back, fixed");
    reviewed(w, &sid, "rows", "1. a.txt:1 — note — reads well now\nRecommendation: commit");
    assert_eq!(mate(w, &sid, "rows")["state"], "committed");
    (sid, dir)
}

#[test]
fn a_review_filed_by_itself_is_one_line_that_grows_through_its_fixes_to_its_commit() {
    let w = World::new();
    let (sid, dir) = committed_by_itself(&w);
    assert_eq!(w.snapshot().session(&sid)["crew_job"], "Two parts: rows and docs", "the lead's prompt, its first line");
    let l = log(&w, &sid);
    assert_eq!(l.len(), 1, "{l:?}");
    let sha = git(&dir, &["rev-parse", "--short=7", "crew/rows"]).trim().to_string();
    let d = &l[0];
    assert_eq!(d["what"], "Reviewed: 1 should-fix. Sent back, fixed, committed to `crew/rows`");
    assert_eq!((d["who"].as_str(), d["kind"].as_str(), d["sha"].as_str(), d["one_way"].as_bool(), d["back"].as_str()), (Some("rows"), Some("commit"), Some(sha.as_str()), Some(false), Some("")));
    assert!(d["id"].as_str().unwrap().starts_with('d'));
    // A second member started while rows is still in the crew? rows left (committed): no split. One started while
    // another works is.
    says(&w, &sid, writes("toolu_B", "agentB", "docs", &[dir.join("b.txt").to_string_lossy().to_string()]));
    assert_eq!(log(&w, &sid).len(), 1, "rows had left the crew");
    let mut start = writes("toolu_C", "agentC", "hub", &[dir.join("shared.txt").to_string_lossy().to_string()]);
    start.truncate(2);
    says(&w, &sid, start);
    let l = log(&w, &sid);
    let split = l.iter().find(|d| d["kind"] == "split").expect("a split");
    assert_eq!((split["who"].as_str(), split["what"].as_str(), split["why"].as_str()), (Some("lead"), Some("Started a new teammate `hub`"), Some("hub's part")));
}

#[test]
fn going_back_on_a_decision_tells_the_lead_once_and_marks_it() {
    let w = World::new();
    let (sid, _) = committed_by_itself(&w);
    let id = log(&w, &sid)[0]["id"].as_str().unwrap().to_string();
    let before = w.hosted.sent().len();
    let r = w.hub().crew_go_back(&sid, &id).unwrap();
    w.settle();
    assert_eq!(r, "Asked its lead to go back on this");
    let sent = w.hosted.sent();
    assert_eq!(sent.len(), before + 1);
    assert_eq!(sent.last().unwrap(), &(sid.clone(), "Go back on this: rows: Reviewed: 1 should-fix. Sent back, fixed, committed to `crew/rows`. Undo it, then carry on: send it back as a fix if a teammate should redo it.".to_string()));
    assert_eq!(log(&w, &sid)[0]["back"], "asked");
    assert!(w.hub().crew_go_back(&sid, &id).is_err(), "asked already");
    assert!(w.hub().crew_go_back(&sid, "d1-1").is_err(), "no such decision");
    assert_eq!(w.hosted.sent().len(), before + 1, "nothing more sent");
}

#[test]
fn a_member_whose_worktree_claude_code_removed_has_nothing_to_review() {
    let w = World::new();
    let dir = repo(&w);
    // The lead's own folder has changes of its own: none of them the member's.
    std::fs::write(dir.join("README"), "the lead's edit\n").unwrap();
    let sid = lead(&w, "one part in a worktree");
    let input = json!({ "name": "probe", "description": "probe's part", "prompt": "look around", "run_in_background": true, "isolation": "worktree" });
    says(&w, &sid, [
        HostedEvent::ToolCall { id: "toolu_W".into(), name: "Agent".into(), input, agent: None },
        HostedEvent::Task { what: "task_started".into(), detail: json!({ "subtype": "task_started", "tool_use_id": "toolu_W", "task_id": "agentW", "task_type": "local_agent" }) },
        // It changed nothing, so Claude Code removed its worktree as it finished: the folder was never there.
        HostedEvent::Task { what: "task_notification".into(), detail: json!({ "subtype": "task_notification", "tool_use_id": "toolu_W", "task_id": "agentW", "status": "completed", "summary": "Nothing to change." }) },
    ]);
    let t = mate(&w, &sid, "probe");
    assert!(!std::path::Path::new(t["worktree"].as_str().unwrap()).exists());
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("done"), Some("Not reviewed: no changes Cue can see in a git repo")), "{t}");
    assert!(t["files"].as_array().is_some_and(|f| f.is_empty()) && t["stat"].is_null(), "the lead's own changes aren't its: {t}");
}

#[test]
fn a_member_with_edits_in_a_worktree_not_where_cue_guessed_it_is_still_reviewed() {
    let w = World::new();
    repo(&w);
    let sid = lead(&w, "one part in a worktree");
    let input = json!({ "name": "builder", "description": "builder's part", "prompt": "build it", "run_in_background": true, "isolation": "worktree" });
    let agent = Some("toolu_X".to_string());
    // Its edits are in its worktree (relative paths), but its folder isn't at the path Cue guessed: it lives elsewhere.
    let guessed = w.home().join(".claude/worktrees/agent-agentX/src/a.rs").to_string_lossy().to_string();
    says(&w, &sid, [
        HostedEvent::ToolCall { id: "toolu_X".into(), name: "Agent".into(), input, agent: None },
        HostedEvent::Task { what: "task_started".into(), detail: json!({ "subtype": "task_started", "tool_use_id": "toolu_X", "task_id": "agentX", "task_type": "local_agent" }) },
        HostedEvent::ToolCall { id: "toolu_X-w".into(), name: "Write".into(), input: json!({ "file_path": guessed, "content": "…" }), agent: agent.clone() },
        HostedEvent::ToolResult { id: "toolu_X-w".into(), output: json!("ok"), failed: false, agent },
        HostedEvent::Task { what: "task_notification".into(), detail: json!({ "subtype": "task_notification", "tool_use_id": "toolu_X", "task_id": "agentX", "status": "completed", "summary": "Built it." }) },
    ]);
    let t = mate(&w, &sid, "builder");
    assert_eq!(t["files"], json!(["src/a.rs"]), "{t}");
    assert_eq!(t["state"], "queued", "git can't say where its edits are: reviewed: {t}");
}
