//! A hosted lead's team (docs/dev/cue-teams.md T1, T3): teammates kept on the lead's record from its stream, over lines
//! the real CLI wrote (hosted/fixtures/team_*.jsonl); and each teammate's Review, Commit, Send fixes and Set aside,
//! against a real git folder.

use crate::hosted::{fixture_events, Ask, AskKind, HostedEvent};
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::{Transcript, World};
use serde_json::{json, Value};

/// A hosted lead started with `message`, which Claude has taken (its turn under way): its id.
pub(super) fn lead(w: &World, message: &str) -> String {
    crate::config::set("hosted.on", json!(true)).unwrap();
    let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), message, "cue-teams", &opts).unwrap();
    let sid = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&sid, HostedEvent::Taken { text: message.into() });
    let log = w.hub().session_origin(&sid).unwrap().transcript_path;
    if !std::path::Path::new(&log).is_file() {
        Transcript::at(&log).prompt(message);
    }
    sid
}

/// The fixture's lines, each as Cue reads it: its events.
fn lines(fixture: &str) -> Vec<Vec<HostedEvent>> {
    fixture_events(fixture)
}

/// Lead `sid` says `events`, in order (a fixture's own Ask included).
pub(super) fn says(w: &World, sid: &str, events: impl IntoIterator<Item = HostedEvent>) {
    for e in events {
        w.hosted_says(sid, e);
    }
    w.settle();
}

fn team(w: &World, sid: &str) -> Vec<Value> {
    w.snapshot().session(sid)["team"].as_array().cloned().unwrap_or_default()
}

pub(super) fn mate(w: &World, sid: &str, name: &str) -> Value {
    team(w, sid).into_iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("no teammate {name}"))
}

/// The index of the first line in `ls` with an event `f` accepts.
fn at(ls: &[Vec<HostedEvent>], f: impl Fn(&HostedEvent) -> bool) -> usize {
    ls.iter().position(|es| es.iter().any(&f)).expect("seen")
}

fn task(e: &HostedEvent, what: &str) -> bool {
    matches!(e, HostedEvent::Task { what: w, .. } if w == what)
}

#[test]
fn a_named_agent_is_a_teammate_from_its_agent_call_its_row_titled_by_its_name_and_keyed_by_its_agent_id_once_started() {
    let w = World::new();
    let sid = lead(&w, "start a probe teammate");
    let ls = lines(include_str!("../../hosted/fixtures/team_name_id.jsonl"));
    let started = at(&ls, |e| task(e, "task_started"));
    says(&w, &sid, ls[..started].iter().flatten().cloned());
    let t = mate(&w, &sid, "probe");
    assert_eq!((t["state"].as_str(), t["agent_id"].as_str(), t["what"].as_str()), (Some("working"), Some(""), Some("Create probe file")), "started by its Agent call, its id not yet known");
    assert!(t["asked"].as_str().unwrap().starts_with("Run the bash command `touch by_probe.txt`"), "what it was asked kept: {t}");
    says(&w, &sid, ls[started].clone());
    assert_eq!(mate(&w, &sid, "probe")["agent_id"], "a71eed734c56b19aa", "its task_started pairs its call with its agent id");
    assert_eq!(team(&w, &sid).len(), 1);
}

#[test]
fn an_agent_call_claude_code_refuses_leaves_no_teammate_and_a_refused_write_is_not_its_file() {
    let w = World::new();
    let sid = lead(&w, "add slugify and daysBetween");
    // As the real CLI refused a name with a space (live, 2026-11-09): the call comes back an error, no agent starts,
    // and the lead tries again under a name it takes.
    says(&w, &sid, [
        HostedEvent::ToolCall { id: "c1".into(), name: "Agent".into(), input: json!({ "name": "days between", "prompt": "do it", "run_in_background": true }), agent: None },
        HostedEvent::ToolResult { id: "c1".into(), output: json!("<tool_use_error>InputValidationError: name must start with a letter or digit</tool_use_error>"), failed: true, agent: None },
        HostedEvent::ToolCall { id: "c2".into(), name: "Agent".into(), input: json!({ "name": "days-between", "prompt": "do it", "run_in_background": true }), agent: None },
        HostedEvent::ToolResult { id: "c2".into(), output: json!("Async agent launched successfully."), failed: false, agent: None },
        HostedEvent::Task { what: "task_started".into(), detail: json!({ "task_type": "local_agent", "tool_use_id": "c2", "task_id": "a2" }) },
    ]);
    let names: Vec<String> = team(&w, &sid).iter().map(|t| t["name"].as_str().unwrap_or("").to_string()).collect();
    assert_eq!(names, ["days-between"], "no row for the refused call");
    // A Write the harness refused changed nothing: not its file. One that went through is.
    let home = w.home().to_string_lossy().to_string();
    let files = |w: &World| mate(w, &sid, "days-between")["files"].as_array().cloned().unwrap_or_default();
    says(&w, &sid, [
        HostedEvent::ToolCall { id: "w1".into(), name: "Write".into(), input: json!({ "file_path": format!("{home}/x.js"), "content": "1" }), agent: Some("c2".into()) },
        HostedEvent::ToolResult { id: "w1".into(), output: json!("<tool_use_error>This agent is isolated in the worktree</tool_use_error>"), failed: true, agent: Some("c2".into()) },
        HostedEvent::ToolCall { id: "w2".into(), name: "Write".into(), input: json!({ "file_path": format!("{home}/y.js"), "content": "2" }), agent: Some("c2".into()) },
    ]);
    assert!(files(&w).is_empty(), "the refused one isn't, and the other not before it's done: {:?}", files(&w));
    says(&w, &sid, [HostedEvent::ToolResult { id: "w2".into(), output: json!("File created"), failed: false, agent: Some("c2".into()) }]);
    assert_eq!(files(&w), [json!("y.js")]);
}

#[test]
fn a_teammates_current_step_comes_live_its_long_command_is_no_second_teammate_and_its_finish_marks_it_done() {
    let w = World::new();
    let sid = lead(&w, "start a worker");
    w.hub().set_crew_review(&sid, "always").unwrap(); // a member whose files Cue never saw: reviewed only when asked to
    let ls = lines(include_str!("../../hosted/fixtures/team_live.jsonl"));
    // Up to its first step: the lead's turn ended ("started"), the teammate's Bash call on its way.
    let first = at(&ls, |e| matches!(e, HostedEvent::ToolCall { agent: Some(_), .. }));
    says(&w, &sid, ls[..=first].iter().flatten().cloned());
    let t = mate(&w, &sid, "worker");
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("working"), Some("Running: sleep 5 && echo one")), "its step as it happens");
    let done = at(&ls, |e| matches!(e, HostedEvent::Task { what, detail } if what == "task_notification" && detail["task_id"] == "a0d8fe7624f6fed65"));
    says(&w, &sid, ls[first + 1..done].iter().flatten().cloned());
    assert_eq!(team(&w, &sid).len(), 1, "its long command (local_bash) is a task of its own, not a teammate");
    assert_eq!(mate(&w, &sid, "worker")["state"], "working", "not done before it's said to be");
    says(&w, &sid, ls[done].clone());
    let t = mate(&w, &sid, "worker");
    assert_eq!(t["state"], "queued", "finished, its review queued by itself");
    assert!(t["summary"].as_str().unwrap().starts_with("Both commands ran in order"), "{t}");
    assert!(t["ended_ms"].as_u64().unwrap() > 0);
    // Skip on the queued review: none goes, and it's done again (its ⋯ has Review for later), not stuck as queued.
    w.hub().unqueue_review(&format!("team:{sid}:a0d8fe7624f6fed65")).unwrap();
    assert_eq!(mate(&w, &sid, "worker")["state"], "done", "skipped: done, with Review in ⋯");
    w.hub().drain_reviews();
    assert!(!w.hosted.sent().iter().any(|(_, t)| t.starts_with("Review your teammate worker")), "the skipped review never goes");
}

#[test]
fn a_teammates_ask_is_titled_with_its_name_and_its_lead() {
    let w = World::new();
    let sid = lead(&w, "start a probe teammate");
    let ls = lines(include_str!("../../hosted/fixtures/team_name_id.jsonl"));
    let asked = at(&ls, |e| matches!(e, HostedEvent::Ask(_)));
    says(&w, &sid, ls[..=asked].iter().flatten().cloned());
    let cards = w.snapshot().cards_of(&sid).into_iter().filter(|c| c["kind"] == "permission").cloned().collect::<Vec<_>>();
    assert_eq!(cards.len(), 1);
    assert_eq!((cards[0]["agent_id"].as_str(), cards[0]["agent_name"].as_str()), (Some("a71eed734c56b19aa"), Some("probe")), "matched to its teammate by agent id, named as its lead named it");
}

#[test]
fn a_worktree_teammate_works_in_the_folder_claude_code_makes_for_it() {
    let w = World::new();
    let sid = lead(&w, "start a builder in a worktree");
    w.hub().set_crew_review(&sid, "always").unwrap(); // a member whose files Cue never saw: reviewed only when asked to
    let ls = lines(include_str!("../../hosted/fixtures/team_worktree.jsonl"));
    says(&w, &sid, ls.iter().flatten().cloned());
    let t = mate(&w, &sid, "builder");
    let home = w.home().to_string_lossy().to_string();
    assert_eq!(t["worktree"], format!("{home}/.claude/worktrees/agent-a4043a0c4248969e1"), "derived from the lead's folder and its agent id");
    assert_eq!(t["state"], "queued", "finished, its review queued by itself");
}

#[test]
fn a_worktree_teammates_commit_is_made_in_its_worktree_and_the_leads_folder_is_left_as_it_was() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "start a builder in a worktree");
    let wt = dir.join(".claude/worktrees/agent-a4043a0c4248969e1");
    git(&dir, &["worktree", "add", "-q", "-b", "worktree-agent-a4043a0c4248969e1", &wt.to_string_lossy()]);
    // What its Bash did there (the fixture's `touch by_builder.txt`): no edit call says so, its worktree does.
    std::fs::write(wt.join("by_builder.txt"), "built\n").unwrap();
    says(&w, &sid, lines(include_str!("../../hosted/fixtures/team_worktree.jsonl")).into_iter().flatten());
    let r = w.hub().team_action(&sid, "a4043a0c4248969e1", "commit").unwrap();
    assert!(r.starts_with("Committed builder's 1 file"), "{r}");
    assert_eq!(git(&wt, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "Run pwd and touch file\n\nby_builder.txt", "on its worktree's branch");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "the lead's branch untouched");
    assert_eq!(mate(&w, &sid, "builder")["state"], "committed");
}

/// Teammate `name`, started by call `call` as agent `id` in the lead's own folder, writes `files` (absolute) and finishes.
pub(super) fn writes(call: &str, id: &str, name: &str, files: &[String]) -> Vec<HostedEvent> {
    let agent = Some(call.to_string());
    let mut es = vec![
        HostedEvent::ToolCall { id: call.into(), name: "Agent".into(), input: json!({ "name": name, "description": format!("{name}'s part"), "prompt": format!("Write {name}'s file"), "run_in_background": true }), agent: None },
        HostedEvent::Task { what: "task_started".into(), detail: json!({ "subtype": "task_started", "tool_use_id": call, "task_id": id, "task_type": "local_agent" }) },
    ];
    for (n, f) in files.iter().enumerate() {
        std::fs::write(f, format!("{name} wrote this\n")).unwrap();
        es.push(HostedEvent::ToolCall { id: format!("{call}-w{n}"), name: "Write".into(), input: json!({ "file_path": f, "content": "…" }), agent: agent.clone() });
        es.push(HostedEvent::ToolResult { id: format!("{call}-w{n}"), output: json!("ok"), failed: false, agent: agent.clone() });
    }
    es.push(HostedEvent::Task { what: "task_notification".into(), detail: json!({ "subtype": "task_notification", "tool_use_id": call, "task_id": id, "status": "completed", "summary": format!("{name} wrote its file") }) });
    es
}

pub(super) fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let o = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).to_string()
}

/// The World's folder as a git repository with one commit on main, switched to a branch of its own (where Commit goes).
pub(super) fn repo(w: &World) -> std::path::PathBuf {
    let dir = main_repo(w);
    git(&dir, &["switch", "-q", "-c", "team-work"]);
    dir
}

/// The same, left on main.
pub(super) fn main_repo(w: &World) -> std::path::PathBuf {
    let dir = w.home().to_path_buf();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.email", "t@example.com"]);
    git(&dir, &["config", "user.name", "T"]);
    std::fs::write(dir.join("README"), "hi\n").unwrap();
    // The World's own files stay out of what a teammate's commit could pick up.
    std::fs::write(dir.join(".gitignore"), "*\n!README\n!.gitignore\n!a.txt\n!b.txt\n!shared.txt\n!by_builder.txt\n").unwrap();
    git(&dir, &["add", "README", ".gitignore"]);
    git(&dir, &["commit", "-q", "-m", "start"]);
    dir
}

#[test]
fn two_teammates_on_one_file_are_both_marked_and_neither_commits_until_one_is_set_aside() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "two teammates");
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[f("a.txt"), f("shared.txt")]));
    says(&w, &sid, writes("toolu_B", "agentB", "hub", &[f("b.txt"), f("shared.txt")]));
    assert_eq!(mate(&w, &sid, "rows")["also"], json!(["hub"]));
    assert_eq!(mate(&w, &sid, "hub")["also"], json!(["rows"]));
    assert_eq!(mate(&w, &sid, "rows")["files"], json!(["a.txt", "shared.txt"]), "relative to the lead's folder");
    let held = w.hub().team_action(&sid, "agentA", "commit");
    assert!(held.as_ref().is_err_and(|e| e.contains("Held: hub")), "{held:?}");
    assert_eq!(git(&dir, &["log", "--oneline"]).lines().count(), 1, "nothing committed");
    w.hub().team_action(&sid, "agentB", "aside").unwrap();
    assert!(mate(&w, &sid, "rows")["also"].as_array().is_none_or(|a| a.is_empty()), "the mark goes with the other teammate");
    let r = w.hub().team_action(&sid, "agentA", "commit").unwrap();
    assert!(r.starts_with("Committed rows's 2 files"), "{r}");
}

#[test]
fn commit_wont_go_on_main_or_master_and_says_so_until_the_folder_is_on_a_branch_of_its_own() {
    let w = World::new();
    let dir = main_repo(&w);
    let sid = lead(&w, "one teammate");
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    let on_main = w.hub().team_action(&sid, "agentA", "commit");
    assert!(on_main.as_ref().is_err_and(|e| e.contains("is on main") && e.contains("branch of its own")), "{on_main:?}");
    git(&dir, &["branch", "-q", "-m", "master"]);
    assert!(w.hub().team_action(&sid, "agentA", "commit").is_err_and(|e| e.contains("is on master")));
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "nothing committed on the main line");
    assert_eq!(git(&dir, &["status", "--porcelain"]).trim(), "?? a.txt", "nothing staged either");
    assert_eq!(mate(&w, &sid, "rows")["state"], "queued", "still waiting for you (its review queued)");
    git(&dir, &["switch", "-q", "-c", "rows-work"]);
    let r = w.hub().team_action(&sid, "agentA", "commit").unwrap();
    assert!(r.starts_with("Committed rows's 1 file"), "{r}");
}

#[test]
fn commit_takes_only_that_teammates_files_and_leaves_the_others_untouched() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "two teammates");
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[f("a.txt")]));
    says(&w, &sid, writes("toolu_B", "agentB", "hub", &[f("b.txt")]));
    // The other's file staged too: a commit of the first still leaves it out.
    git(&dir, &["add", "b.txt"]);
    assert_eq!(w.hub().team_action(&sid, "agentA", "diff").unwrap().lines().filter(|l| l.starts_with("+++")).collect::<Vec<_>>(), ["+++ b/a.txt"], "its diff: its file only");
    let r = w.hub().team_action(&sid, "agentA", "commit").unwrap();
    assert!(r.starts_with("Committed rows's 1 file"), "{r}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "rows's part\n\na.txt");
    assert_eq!(git(&dir, &["status", "--porcelain"]).trim(), "A  b.txt", "the other's work as it was: staged, not committed");
    let t = mate(&w, &sid, "rows");
    assert_eq!(t["state"], "committed");
    assert!(!t["commit"].as_str().unwrap().is_empty());
    assert_eq!(mate(&w, &sid, "hub")["state"], "queued");
}

#[test]
fn review_goes_to_the_free_lead_its_reply_is_the_teammates_review_and_send_fixes_puts_it_back_to_work() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "one teammate");
    // Ask me: every review waits for your button (the default handles it: commit_when_clean_… below).
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "started rows".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    // Nobody clicks Review: it was queued as rows finished, and goes now the lead is free.
    w.hub().drain_reviews();
    w.settle();
    let sent = w.hosted.sent();
    let review = &sent.last().unwrap().1;
    assert!(review.starts_with("Review your teammate rows's work with a fork") && review.contains("Its files: a.txt"), "{review}");
    assert_eq!(mate(&w, &sid, "rows")["state"], "reviewing");
    // A teammate finishing meanwhile starts a turn nothing of yours did: not the review.
    w.hosted_says(&sid, HostedEvent::Taken { text: review.clone() });
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "hub finished".into(), failed: false, cost_usd: 0.0, unprompted: true });
    assert_eq!(mate(&w, &sid, "rows")["state"], "reviewing");
    // A prompted reply that isn't a review (a hook's feedback asked for an audit, in numbered lines): no Recommendation
    // line, so it's not taken as rows's review.
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "1. Shape: one comment line.\n2. Assumptions: none.".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), t["findings"].as_array().map(Vec::len)), (Some("reviewing"), None), "still waiting for its review: {t}");
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "1. a.txt:1 — should-fix — say more\n2. a.txt:1 — note — fine otherwise\nRecommendation: send fixes.".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), t["recommendation"].as_str()), (Some("reviewed"), Some("send fixes")));
    assert_eq!(t["findings"].as_array().unwrap().len(), 2);
    assert!(t["review"].as_str().unwrap().ends_with("Recommendation: send fixes."), "its whole review, for ⋯ → Full review");
    w.hub().team_action(&sid, "agentA", "fixes").unwrap();
    w.settle();
    let fixes = &w.hosted.sent().last().unwrap().1.clone();
    assert!(fixes.contains("SendMessage (to: \"rows\")") && fixes.contains("1. a.txt:1 — should-fix — say more"), "{fixes}");
    let t = mate(&w, &sid, "rows");
    assert_eq!(t["state"], "working");
    assert!(t["findings"].as_array().is_none_or(|a| a.is_empty()));
}

#[test]
fn by_default_only_a_member_with_code_in_a_git_repo_is_reviewed_by_itself_and_never_leaves_review_to_you() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "mocks and code");
    assert!(w.snapshot().session(&sid)["crew_review"].is_null(), "only code in a repo: the default, no setting");
    // Mocks written outside any repository: done at once, its row saying why; no review asked.
    let outside = w.home().join("mocks");
    std::fs::create_dir_all(&outside).unwrap();
    says(&w, &sid, writes("toolu_A", "agentA", "mocks", &[outside.join("mock-1.html").to_string_lossy().to_string()]));
    let t = mate(&w, &sid, "mocks");
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("done"), Some("Not reviewed: no changes Cue can see in a git repo")), "{t}");
    // Code in the repo: reviewed by itself.
    says(&w, &sid, writes("toolu_B", "agentB", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    assert_eq!(mate(&w, &sid, "rows")["state"], "queued");
    // Never: none by itself, even code in a repo; Review by hand still asks for one.
    w.hub().set_crew_review(&sid, "never").unwrap();
    assert_eq!(w.snapshot().session(&sid)["crew_review"], "never");
    says(&w, &sid, writes("toolu_C", "agentC", "docs", &[dir.join("b.txt").to_string_lossy().to_string()]));
    let t = mate(&w, &sid, "docs");
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("done"), Some("Not reviewed: reviews are off for this crew")), "{t}");
    w.hub().team_action(&sid, "agentC", "review").unwrap();
    assert_eq!(mate(&w, &sid, "docs")["state"], "queued", "asked by hand: it goes");
    // Always: the mocks too.
    w.hub().set_crew_review(&sid, "always").unwrap();
    says(&w, &sid, writes("toolu_D", "agentD", "more-mocks", &[outside.join("mock-2.html").to_string_lossy().to_string()]));
    assert_eq!(mate(&w, &sid, "more-mocks")["state"], "queued");
    w.hub().set_crew_review(&sid, "code").unwrap();
    assert!(w.snapshot().session(&sid)["crew_review"].is_null(), "only code in a repo is no setting");
    assert!(w.hub().set_crew_review(&sid, "sometimes").is_err());
}

#[test]
fn a_finished_crew_member_is_reviewed_by_itself_queued_while_its_lead_works_and_sent_when_it_is_free() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "one teammate");
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    w.settle();
    assert_eq!(mate(&w, &sid, "rows")["state"], "queued", "nobody clicked Review");
    assert_eq!(w.snapshot().0["review_queue"][&sid][0]["name"], "rows");
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "started rows".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    w.hub().drain_reviews();
    w.settle();
    assert_eq!(mate(&w, &sid, "rows")["state"], "reviewing");
    assert!(w.hosted.sent().last().unwrap().1.starts_with("Review your teammate rows"));
}

#[test]
fn after_a_restart_the_team_is_back_from_cue_db_and_a_resumed_teammates_new_lines_add_to_it() {
    let mut w = World::new();
    let sid = lead(&w, "start keeper");
    w.hub().set_crew_review(&sid, "always").unwrap(); // a member whose files Cue never saw: reviewed only when asked to
    let ls = lines(include_str!("../../hosted/fixtures/team_resume.jsonl"));
    // The first process: keeper started, answered "OK" and finished; the lead's own turns after it.
    let inits: Vec<usize> = include_str!("../../hosted/fixtures/team_resume.jsonl").lines().enumerate().filter(|(_, l)| l.contains("\"subtype\":\"init\"")).map(|(i, _)| i).collect();
    let second = inits[2];
    says(&w, &sid, ls[..second].iter().flatten().cloned());
    assert_eq!((mate(&w, &sid, "keeper")["state"].as_str(), mate(&w, &sid, "keeper")["summary"].as_str()), (Some("reviewing"), Some("OK")), "finished, its review sent by itself to the free lead");
    w.relaunch();
    let t = mate(&w, &sid, "keeper");
    assert_eq!((t["agent_id"].as_str(), t["state"].as_str()), (Some("ad7fdc6dbd6894ce9"), Some("reviewing")), "kept in cue.db with its lead: the resumed stream never says it again");
    // The second process (--resume): SendMessage resumes it under the same agent id; its new line, under its first call.
    let ls = lines(&include_str!("../../hosted/fixtures/team_resume.jsonl").lines().skip(second).collect::<Vec<_>>().join("\n"));
    let resumed = at(&ls, |e| task(e, "task_started"));
    says(&w, &sid, ls[..=resumed].iter().flatten().cloned());
    assert_eq!(mate(&w, &sid, "keeper")["state"], "working", "resumed: at work again");
    let done = at(&ls, |e| task(e, "task_notification"));
    says(&w, &sid, ls[resumed + 1..=done].iter().flatten().cloned());
    let t = mate(&w, &sid, "keeper");
    assert_eq!((t["state"].as_str(), t["summary"].as_str()), (Some("queued"), Some("MANGO")), "its new words added to what was kept, and reviewed again by itself");
    assert_eq!(team(&w, &sid).len(), 1, "the same teammate, not a second");
}

#[test]
fn the_sessions_own_asks_and_unnamed_agents_make_no_team() {
    let w = World::new();
    let sid = lead(&w, "plain");
    says(&w, &sid, [
        HostedEvent::ToolCall { id: "t1".into(), name: "Agent".into(), input: json!({ "description": "look around", "prompt": "…", "run_in_background": true }), agent: None },
        HostedEvent::Ask(Ask { request_id: "r1".into(), kind: AskKind::Permission, tool_name: "Bash".into(), input: json!({ "command": "ls" }), suggestions: json!([]), agent_id: String::new(), tool_use_id: "t2".into(), server: String::new(), message: String::new(), form: Value::Null }),
    ]);
    assert!(team(&w, &sid).is_empty());
    assert_eq!(w.snapshot().cards_of(&sid)[0]["agent_name"].as_str().unwrap_or(""), "");
}

// ---------- T4, T5, H22 (hub/team_out.rs): the team skill, the Team switch, a handoff carries the team, hand-out ----------

/// The arguments the session's last start was given.
fn launched_args(w: &World) -> Vec<String> {
    w.hosted.launched().last().map(|l| l.args.clone()).unwrap_or_default()
}

/// How the team skill (hub/team_out.rs) starts.
const SKILL_START: &str = "# Working as a crew lead (Cue)";

/// The team skill in `args`: right after `--append-system-prompt`.
fn skill_in(args: &[String]) -> Option<String> {
    args.iter().position(|a| a == "--append-system-prompt").and_then(|i| args.get(i + 1)).cloned()
}

#[test]
fn a_prompt_you_append_yourself_gets_the_skill_merged_in_not_a_second_option() {
    let w = World::new();
    crate::config::set("hosted.on", json!(true)).unwrap();
    let opts = StartOpts { hosted: true, perm: "auto".into(), extra: "--append-system-prompt Terse.".into(), ..Default::default() };
    start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "hi", "cue-teams", &opts).unwrap();
    let args = launched_args(&w);
    assert_eq!(args.iter().filter(|a| *a == "--append-system-prompt").count(), 1, "one option: {args:?}");
    let prompt = skill_in(&args).unwrap();
    assert!(prompt.starts_with("Terse.\n\n") && prompt.ends_with(crate::hub::team_out::TEAM_SKILL), "yours, then the skill: {prompt}");
}

#[test]
fn a_hosted_claude_lead_starts_with_the_team_skill_and_gets_it_again_when_resumed() {
    let mut w = World::new();
    let sid = lead(&w, "fix the typo in a.txt and add a line to b.txt");
    let skill = skill_in(&launched_args(&w)).expect("the team skill is among its launch options");
    assert!(skill.starts_with(SKILL_START), "{skill}");
    for habit in ["independent parts", "short plain `name`", "`run_in_background: true`", "`isolation: \"worktree\"`", "never two on the same files", "Don't set a `model`: teammates run on yours", "Don't commit their work"] {
        assert!(skill.contains(habit), "the skill says {habit:?}: {skill}");
    }
    // Plain: none of relay's machinery (packets, reports, status formats, gates, role tags).
    let lower = skill.to_lowercase();
    for ceremony in ["packet", "report", "tl;dr", "status:", "[lead]", "relay", "verify", "confirm", "review", "haiku", "sonnet", "opus", "test counts"] {
        assert!(!lower.contains(ceremony), "the skill asks for no {ceremony:?}: {skill}");
    }
    assert!(skill.lines().count() <= 15, "short: {} lines", skill.lines().count());
    w.relaunch();
    w.ask(json!({ "type": "send_to", "session_id": sid, "text": "carry on" }));
    let l = w.hosted.launched();
    assert_eq!(l.last().map(|l| l.start.clone()), Some(crate::hosted::Start::Resume(sid.clone())));
    assert_eq!(skill_in(&launched_args(&w)), Some(skill.clone()), "resumed, it starts with it again");
    assert_eq!(launched_args(&w).iter().filter(|a| *a == "--append-system-prompt").count(), 1, "once");
}

#[test]
fn the_team_switch_says_on_or_off_with_your_message_auto_says_nothing_and_slash_team_is_on_for_one() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "start");
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "ok".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    let last = |w: &World| w.hosted.sent().last().map(|(_, t)| t.clone()).unwrap_or_default();
    // Cue's own messages to the lead (a Review) never carry it.
    w.hub().set_team_mode(&sid, "on").unwrap();
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "rows is done".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    w.hub().drain_reviews();
    w.settle();
    assert!(last(&w).starts_with("Review your teammate rows") && !last(&w).contains("(Team:"), "{}", last(&w));
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "1. a.txt:1 — note — fine\nRecommendation: commit".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    w.hub().set_team_mode(&sid, "auto").unwrap();
    let ask = "fix the typo in a.txt and add a line to b.txt";
    w.hub().send_yours(&sid, ask, &[], false).unwrap();
    assert_eq!(last(&w), ask, "auto (its default): as typed, the lead decides");
    w.hub().set_team_mode(&sid, "on").unwrap();
    assert_eq!(w.snapshot().session(&sid)["team_mode"], "on", "the switch is the session's, in the window's state");
    w.hub().send_yours(&sid, ask, &[], false).unwrap();
    assert!(last(&w).starts_with(&format!("{ask}\n\n(Crew: on. Split this")), "{}", last(&w));
    w.hub().set_team_mode(&sid, "off").unwrap();
    w.hub().send_yours(&sid, ask, &[], false).unwrap();
    assert_eq!(last(&w), format!("{ask}\n\n(Crew: off. Do this yourself: start no teammates.)"));
    w.hub().send_yours(&sid, &format!("/crew {ask}"), &[], false).unwrap();
    assert!(last(&w).starts_with(&format!("{ask}\n\n(Crew: on.")), "/crew: on for this one, over the switch: {}", last(&w));
    w.hub().send_yours(&sid, "/compact", &[], false).unwrap();
    assert_eq!(last(&w), "/compact", "a command goes as typed");
    w.hub().set_team_mode(&sid, "auto").unwrap();
    assert!(w.snapshot().session(&sid)["team_mode"].is_null(), "auto is no setting");
    w.hub().send_yours(&sid, ask, &[], false).unwrap();
    assert_eq!(last(&w), ask);
    assert!(w.hub().set_team_mode(&sid, "maybe").is_err());
}

#[test]
fn a_reply_to_a_finished_lead_carries_its_team_switch_and_slash_team_as_the_box_does() {
    let w = World::new();
    repo(&w);
    let sid = lead(&w, "start");
    let last = |w: &World| w.hosted.sent().last().map(|(_, t)| t.clone()).unwrap_or_default();
    let ask = "fix the typo in a.txt and add a line to b.txt";
    let reply = |w: &World, text: &str| {
        w.hosted_says(&sid, HostedEvent::TurnEnded { text: "ok".into(), failed: false, cost_usd: 0.0, unprompted: false });
        w.settle();
        let card = w.snapshot().card(&sid)["id"].as_str().expect("a finished lead waits on you").to_string();
        w.hub().reply(&card, text, &[]).unwrap();
    };
    w.hub().set_team_mode(&sid, "on").unwrap();
    reply(&w, ask);
    assert!(last(&w).starts_with(&format!("{ask}\n\n(Crew: on. Split this")), "{}", last(&w));
    w.hub().set_team_mode(&sid, "auto").unwrap();
    reply(&w, &format!("/crew {ask}"));
    assert!(last(&w).starts_with(&format!("{ask}\n\n(Crew: on.")), "/crew in the reply box: {}", last(&w));
    reply(&w, ask);
    assert_eq!(last(&w), ask, "auto: as typed");
}

#[test]
fn a_carried_teammate_leaves_its_old_lead_so_it_shows_once_and_one_lead_commits_it() {
    let w = World::new();
    let dir = repo(&w);
    let old = lead(&w, "two parts");
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    says(&w, &old, writes("toolu_A", "agentA", "rows", &[f("a.txt")]));
    says(&w, &old, writes("toolu_C", "agentC", "docs", &[f("shared.txt")]));
    w.hub().team_action(&old, "agentC", "aside").unwrap();
    let next = lead(&w, "carry on");
    // A handoff that keeps the old lead open (its background agents still at work): both leads on the board.
    w.hub().carry_team(&old, &next);
    w.settle();
    let names = |sid: &str| team(&w, sid).iter().map(|t| t["name"].as_str().unwrap_or("").to_string()).collect::<Vec<_>>();
    assert_eq!(names(&next), ["rows"], "carried: in the team only");
    assert_eq!(names(&old), ["docs"], "the old lead keeps only the one set aside");
    assert_eq!(w.hub().team_action(&old, "agentA", "commit").unwrap_err(), "Cue doesn't know that teammate", "committed from one lead only");
    assert!(w.hub().team_action(&next, "agentA", "commit").unwrap().starts_with("Committed rows's 1 file"));
    w.hub().carry_team(&old, &next);
    assert_eq!(names(&next), ["rows"], "carrying again adds nothing");
}

#[test]
fn a_handoff_carries_the_team_and_the_successor_commits_an_earlier_teammates_work() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "two parts");
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[f("a.txt")]));
    // hub: still at work when the lead hands off, one file written.
    let mut hub = writes("toolu_B", "agentB", "hub", &[f("b.txt")]);
    hub.pop();
    says(&w, &sid, hub);
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "Handed out rows and hub. Note in HANDOFF.md.".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    crate::db::handoff_sent(&sid, crate::ipc::CUE_HANDOFF, false, 0);
    let r = crate::ipc::handoff_continue_now(w.hub(), &sid).unwrap();
    w.settle();
    let next = r["session_id"].as_str().unwrap().to_string();
    let l = w.hosted.launched().last().cloned().unwrap();
    assert!(matches!(l.start, crate::hosted::Start::New(ref id) if *id == next), "the successor is hosted too: {:?}", l.start);
    assert!(skill_in(&l.args).is_some_and(|s| s.starts_with(SKILL_START)), "with the team skill");
    let rows = mate(&w, &next, "rows");
    assert_eq!((rows["state"].as_str(), rows["from"].as_str(), rows["files"].clone()), (Some("done"), Some(sid.as_str()), json!(["a.txt"])), "carried: its state, its files, where from");
    let hub = mate(&w, &next, "hub");
    assert_eq!((hub["state"].as_str(), hub["files"].clone()), (Some("stopped"), json!(["b.txt"])), "at work then: stopped with its old lead's process, its files kept");
    let told = w.hosted.sent().into_iter().filter(|(s, _)| *s == next).map(|(_, t)| t).find(|t| t.starts_with("The team from the session before you")).expect("the successor is told its team");
    assert!(!told.contains('[') && !told.contains("TL;DR") && !told.to_lowercase().contains("relay"), "plain facts, no memo sections: {told}");
    assert!(told.contains("\n- rows: rows's part (done). Files: a.txt.") && told.contains("\n- hub: hub's part (stopped). Files: b.txt."), "{told}");
    // The successor reviews and commits rows's work.
    w.hosted_says(&next, HostedEvent::TurnEnded { text: "Read the note; rows is done, hub stopped.".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    w.hub().team_action(&next, "agentA", "review").unwrap();
    assert!(w.hosted.sent().last().is_some_and(|(s, t)| *s == next && t.starts_with("Review your teammate rows")));
    let c = w.hub().team_action(&next, "agentA", "commit").unwrap();
    assert!(c.starts_with("Committed rows's 1 file"), "{c}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "rows's part\n\na.txt");
    assert_eq!(git(&dir, &["status", "--porcelain"]).trim(), "?? b.txt", "hub's file left as it was");
    // hub started again under its name (Send fixes asks for that): the same row, at work again.
    w.hosted_says(&next, HostedEvent::ToolCall { id: "toolu_B2".into(), name: "Agent".into(), input: json!({ "name": "hub", "description": "finish hub", "prompt": "…", "run_in_background": true }), agent: None });
    w.settle();
    let hub = mate(&w, &next, "hub");
    assert_eq!((hub["state"].as_str(), hub["call"].as_str(), hub["files"].clone()), (Some("working"), Some("toolu_B2"), json!(["b.txt"])));
    assert!(hub["from"].is_null(), "its own lead's now");
    assert_eq!(team(&w, &next).iter().filter(|t| t["state"] != "committed").count(), 1, "rows committed and gone; hub one row, not two");
}

#[test]
fn a_task_dropped_on_a_lead_shows_the_files_a_teammate_already_changes_then_goes_as_a_new_teammate() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "one teammate");
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    let free = w.hub().hand_out_preview(&sid, "Add a line to `notes/c.txt`").unwrap();
    assert_eq!(free["overlap"], false);
    assert_eq!(free["files"], json!([{ "path": "notes/c.txt", "by": "" }]));
    let p = w.hub().hand_out_preview(&sid, "Fix the typo in a.txt and add a line to c.txt.").unwrap();
    assert_eq!(p["overlap"], true);
    assert_eq!(p["files"], json!([{ "path": "a.txt", "by": "rows is changing it" }, { "path": "c.txt", "by": "" }]));
    // Sent: the lead hands it out.
    w.hosted_says(&sid, HostedEvent::TurnEnded { text: "started rows".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
    let before = w.hosted.sent().len();
    w.hub().hand_out(&sid, "Fix the typo in a.txt").unwrap();
    let sent = w.hosted.sent();
    assert_eq!(sent.len(), before + 1);
    assert!(sent[before].1.starts_with("Hand this to a new teammate") && sent[before].1.ends_with("Fix the typo in a.txt"), "{}", sent[before].1);
    assert!(w.hub().hand_out(&sid, "  ").is_err());
}

#[test]
fn by_default_a_clean_review_is_committed_on_a_branch_of_its_own_and_findings_that_need_a_change_go_back_once_then_wait_for_you() {
    let w = World::new();
    let dir = main_repo(&w);
    let sid = lead(&w, "two parts");
    assert!(w.snapshot().session(&sid)["crew_commit"].is_null(), "handled by Cue: the default, no setting");
    let reviewed = |w: &World, text: &str| {
        w.hosted_says(&sid, HostedEvent::TurnEnded { text: "on it".into(), failed: false, cost_usd: 0.0, unprompted: false });
        w.settle();
        w.advance(60_000); // past the last review's settling time: one review at a time goes to a lead
        w.hub().drain_reviews();
        w.settle();
        w.hosted_says(&sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
        w.settle();
    };
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    // A commit with a finding that needs a change: it goes back first, then a review with only a note commits.
    reviewed(&w, "1. a.txt:1 — should-fix — say why\nRecommendation: commit");
    let t = mate(&w, &sid, "rows");
    assert_eq!((t["state"].as_str(), t["fix_rounds"].as_u64()), (Some("working"), Some(1)), "its finding sent back by itself: {t}");
    assert!(w.hosted.sent().last().unwrap().1.contains("1. a.txt:1 — should-fix — say why"));
    w.hosted_says(&sid, HostedEvent::Task { what: "task_notification".into(), detail: json!({ "task_id": "agentA", "status": "completed", "summary": "Said why." }) });
    w.settle();
    reviewed(&w, "1. a.txt:1 — note — reads well now\nRecommendation: commit");
    let t = mate(&w, &sid, "rows");
    assert_eq!(t["state"], "committed", "a review with only a note, committed by itself: {t}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "crew/rows"]).trim(), "rows's part\n\na.txt", "on a branch of its own");
    assert_eq!(git(&dir, &["symbolic-ref", "--short", "HEAD"]).trim(), "main", "the folder stays on main");
    says(&w, &sid, writes("toolu_B", "agentB", "hub", &[dir.join("b.txt").to_string_lossy().to_string()]));
    // Findings go back by themselves once; the next review waits for you.
    reviewed(&w, "1. b.txt:1 — should-fix — say more\nRecommendation: send fixes");
    assert_eq!(mate(&w, &sid, "hub")["fix_rounds"].as_u64(), Some(1), "round 1 sent back by itself");
    w.hosted_says(&sid, HostedEvent::Task { what: "task_notification".into(), detail: json!({ "task_id": "agentB", "status": "completed", "summary": "Fixed." }) });
    w.settle();
    reviewed(&w, "1. b.txt:1 — should-fix — say more\nRecommendation: send fixes");
    let t = mate(&w, &sid, "hub");
    assert_eq!((t["state"].as_str(), t["fix_rounds"].as_u64()), (Some("reviewed"), Some(1)), "after one round: yours: {t}");
    assert!(git(&dir, &["branch", "--list", "crew/hub"]).trim().is_empty());
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    assert_eq!(w.snapshot().session(&sid)["crew_commit"], "wait", "Ask me: the lead's setting, in the window's state");
    w.hub().set_crew_commit(&sid, "auto").unwrap();
    assert!(w.snapshot().session(&sid)["crew_commit"].is_null(), "handled by Cue is no setting");
    assert!(w.hub().set_crew_commit(&sid, "sometimes").is_err());
}

#[test]
fn commit_from_main_offers_make_a_branch_and_commit_which_commits_on_a_new_branch_and_leaves_the_folder_alone() {
    let w = World::new();
    let dir = main_repo(&w);
    let sid = lead(&w, "one teammate");
    // Someone else's work in the same checkout: an edit to a tracked file, and one staged.
    std::fs::write(dir.join("README"), "someone else's edit\n").unwrap();
    std::fs::write(dir.join("b.txt"), "staged by someone else\n").unwrap();
    git(&dir, &["add", "b.txt"]);
    let status = git(&dir, &["status", "--porcelain"]);
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    let status = format!("{status}?? a.txt\n");
    assert_eq!(git(&dir, &["status", "--porcelain"]), status);
    let refused = w.hub().team_action(&sid, "agentA", "commit").unwrap_err();
    assert!(refused.contains("is on main") && refused.contains("Make a branch and commit"), "{refused}");
    git(&dir, &["branch", "-q", "crew/rows"]);
    let index = std::fs::read(dir.join(".git/index")).unwrap();
    let r = w.hub().team_action(&sid, "agentA", "branch").unwrap();
    assert_eq!(std::fs::read(dir.join(".git/index")).unwrap(), index, "the real index untouched, byte for byte");
    let sha = git(&dir, &["rev-parse", "--short=7", "crew/rows-2"]).trim().to_string();
    assert_eq!(r, format!("Committed 1 file to crew/rows-2 ({sha}); your folder stays on main."), "a free name, said plainly");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "crew/rows-2"]).trim(), "rows's part\n\na.txt", "the teammate's file only");
    assert_eq!(git(&dir, &["rev-parse", "crew/rows-2^"]), git(&dir, &["rev-parse", "main"]), "made at HEAD");
    assert_eq!(git(&dir, &["show", "crew/rows-2:a.txt"]), "rows wrote this\n");
    // The folder as it was: on main, main unmoved, the same changes, no scratch index left.
    assert_eq!(git(&dir, &["branch", "--show-current"]).trim(), "main");
    assert_eq!(git(&dir, &["rev-list", "--count", "main"]).trim(), "1");
    assert_eq!(git(&dir, &["status", "--porcelain"]), status, "someone else's edits still there, staged and not");
    assert_eq!(git(&dir, &["diff", "--cached", "--name-only"]), "b.txt\n", "what was staged is still staged, nothing more");
    assert!(!std::fs::read_dir(dir.join(".git")).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("cue-team-index")));
    assert_eq!(mate(&w, &sid, "rows")["state"], "committed");
}

#[test]
fn make_a_branch_and_commit_that_fails_leaves_no_branch() {
    let w = World::new();
    let dir = main_repo(&w);
    let sid = lead(&w, "one teammate");
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    // A stale lock on crew/rows: everything up to the commit works, then the branch can't be made.
    std::fs::create_dir_all(dir.join(".git/refs/heads/crew")).unwrap();
    std::fs::write(dir.join(".git/refs/heads/crew/rows.lock"), "").unwrap();
    let r = w.hub().team_action(&sid, "agentA", "branch");
    assert!(r.as_ref().is_err_and(|e| e.contains("none was made")), "{r:?}");
    assert_eq!(git(&dir, &["branch", "--list", "crew/*"]).trim(), "", "no half-made branch");
    assert_eq!(git(&dir, &["branch", "--show-current"]).trim(), "main");
    assert_ne!(mate(&w, &sid, "rows")["state"], "committed");
}

/// Lead `sid`'s teammate `name` (call `call`, agent `id`) started, in a worktree of its own when `worktree`.
fn starts(w: &World, sid: &str, call: &str, id: &str, name: &str, worktree: bool) {
    let mut input = json!({ "name": name, "description": format!("{name}'s part"), "prompt": "do it", "run_in_background": true });
    if worktree {
        input["isolation"] = json!("worktree");
    }
    says(w, sid, [
        HostedEvent::ToolCall { id: call.into(), name: "Agent".into(), input, agent: None },
        HostedEvent::Task { what: "task_started".into(), detail: json!({ "subtype": "task_started", "tool_use_id": call, "task_id": id, "task_type": "local_agent" }) },
    ]);
}

/// Its Bash call `n` runs `command` and ends.
fn runs(w: &World, sid: &str, call: &str, n: u32, command: &str) {
    let agent = Some(call.to_string());
    says(w, sid, [
        HostedEvent::ToolCall { id: format!("{call}-b{n}"), name: "Bash".into(), input: json!({ "command": command }), agent: agent.clone() },
        HostedEvent::ToolResult { id: format!("{call}-b{n}"), output: json!(""), failed: false, agent },
    ]);
}

#[test]
fn a_worktree_teammates_shell_writes_are_its_files_once_each_command_ends_as_git_says() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "a builder in a worktree");
    let wt = dir.join(".claude/worktrees/agent-agentW");
    git(&dir, &["worktree", "add", "-q", "-b", "worktree-agent-agentW", &wt.to_string_lossy()]);
    starts(&w, &sid, "toolu_W", "agentW", "builder", true);
    // What its commands did there; no edit call says so.
    std::fs::write(wt.join("a.txt"), "x\n").unwrap();
    runs(&w, &sid, "toolu_W", 1, "printf 'x\\n' > a.txt");
    let t = mate(&w, &sid, "builder");
    assert_eq!((t["files"].clone(), t["state"].clone()), (json!(["a.txt"]), json!("working")), "on its row while it still works");
    assert!(t["via_shell"].is_null(), "git said so: not a guess");
    // A command whose words name no file (a script) counts too: git says what it changed.
    std::fs::write(wt.join("README"), "changed\n").unwrap();
    runs(&w, &sid, "toolu_W", 2, "./gen.sh");
    assert_eq!(mate(&w, &sid, "builder")["files"], json!(["a.txt", "README"]));
    assert_eq!(git(&dir, &["status", "--porcelain"]).trim(), "", "the lead's folder untouched");
}

#[test]
fn a_teammates_shell_write_in_the_leads_folder_is_its_file_via_shell_and_marks_the_teammate_sharing_it() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "two teammates");
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[f("a.txt"), f("shared.txt")]));
    starts(&w, &sid, "toolu_B", "agentB", "hub", false);
    std::fs::write(dir.join("shared.txt"), "hub's\n").unwrap();
    std::fs::write(dir.join("b.txt"), "hub's\n").unwrap();
    runs(&w, &sid, "toolu_B", 1, "printf 'hub\\n' > shared.txt && sed -i '' 's/x/y/' b.txt; rm -f gone.txt; echo ok > /tmp/scratch.log");
    let t = mate(&w, &sid, "hub");
    assert_eq!(t["files"], json!(["shared.txt", "b.txt", "gone.txt"]), "read off the command; a file outside its folder isn't its");
    assert_eq!(t["via_shell"], t["files"]);
    assert_eq!(t["also"], json!(["rows"]));
    assert_eq!(mate(&w, &sid, "rows")["also"], json!(["hub"]), "the shared-files mark, both ways");
    let held = w.hub().team_action(&sid, "agentA", "commit");
    assert!(held.as_ref().is_err_and(|e| e.contains("Held: hub")), "{held:?}");
    // When it finishes, git says which of them changed: a file the command didn't change after all is let go.
    says(&w, &sid, [HostedEvent::Task { what: "task_notification".into(), detail: json!({ "task_id": "agentB", "status": "completed", "summary": "Done." }) }]);
    let t = mate(&w, &sid, "hub");
    assert_eq!((t["files"].clone(), t["via_shell"].clone(), t["stat"].clone()), (json!(["shared.txt", "b.txt"]), json!(["shared.txt", "b.txt"]), json!("2 files +2 −0")));
    assert_eq!(t["also"], json!(["rows"]));
}

/// A teammate `name` of lead `sid`, started (its Agent call and Claude's task_started), with no notice yet.
fn started_mate(w: &World, sid: &str, call: &str, id: &str, name: &str) {
    says(w, sid, [
        HostedEvent::ToolCall { id: call.into(), name: "Agent".into(), input: json!({ "name": name, "description": format!("{name}'s part"), "prompt": "do your part", "run_in_background": true }), agent: None },
        HostedEvent::Task { what: "task_started".into(), detail: json!({ "subtype": "task_started", "tool_use_id": call, "task_id": id, "task_type": "local_agent" }) },
    ]);
}

/// What the lead was sent that's about a quiet teammate.
fn quiet_nudges(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, t)| s == sid && t.contains("has been quiet")).map(|(_, t)| t).collect()
}

#[test]
fn a_quiet_teammate_whose_last_line_was_its_own_words_finished_and_shows_done() {
    let w = World::new();
    let sid = lead(&w, "two parts");
    started_mate(&w, &sid, "toolu_A", "agentA", "rows");
    says(&w, &sid, [HostedEvent::Said { text: "Done: rows sort by name now.".into(), agent: Some("toolu_A".into()) }]);
    let quiet_from = mate(&w, &sid, "rows")["step_ms"].as_u64().unwrap();
    w.hub().team_quiet_check(quiet_from + crate::sessions::quiet::QUIET_MS - 1);
    assert_eq!(mate(&w, &sid, "rows")["state"], "working", "not quiet yet");
    w.hub().team_quiet_check(quiet_from + crate::sessions::quiet::QUIET_MS);
    let m = mate(&w, &sid, "rows");
    assert_eq!((m["state"].as_str(), m["summary"].as_str()), (Some("done"), Some("Done: rows sort by name now.")), "finished and lingering: done, not quiet");
    assert!(quiet_nudges(&w, &sid).is_empty(), "nothing to ask the lead");
}

#[test]
fn a_quiet_teammate_with_a_tool_call_open_is_left_to_it() {
    let w = World::new();
    let sid = lead(&w, "two parts");
    started_mate(&w, &sid, "toolu_A", "agentA", "suite");
    says(&w, &sid, [HostedEvent::ToolCall { id: "b1".into(), name: "Bash".into(), input: json!({ "command": "cargo test --release" }), agent: Some("toolu_A".into()) }]);
    let quiet_from = mate(&w, &sid, "suite")["step_ms"].as_u64().unwrap();
    w.hub().team_quiet_check(quiet_from + crate::sessions::quiet::QUIET_MS + 60_000);
    let m = mate(&w, &sid, "suite");
    assert_eq!((m["state"].as_str(), m["quiet"].as_str()), (Some("working"), Some("tool")), "a long run: still at work");
    assert!(m["step"].as_str().unwrap().contains("cargo test"), "{m}");
    assert!(quiet_nudges(&w, &sid).is_empty(), "left to it: no nudge");
}

#[test]
fn a_stuck_teammate_gets_its_lead_nudged_once_a_quiet_stretch_and_resume_asks_the_lead_to_wake_it() {
    let w = World::new();
    let sid = lead(&w, "two parts");
    started_mate(&w, &sid, "toolu_A", "agentA", "parser");
    // The lead waits for its teammate, its turn over (a message to it now goes at once, not queued).
    says(&w, &sid, [HostedEvent::TurnEnded { text: "parser is on it.".into(), failed: false, cost_usd: 0.0, unprompted: false }]);
    says(&w, &sid, [
        HostedEvent::ToolCall { id: "r1".into(), name: "Read".into(), input: json!({ "file_path": "/p/a.rs" }), agent: Some("toolu_A".into()) },
        HostedEvent::ToolResult { id: "r1".into(), output: json!("fn a() {}"), failed: false, agent: Some("toolu_A".into()) },
    ]);
    let quiet_from = mate(&w, &sid, "parser")["step_ms"].as_u64().unwrap();
    let past = quiet_from + crate::sessions::quiet::QUIET_MS;
    w.hub().team_quiet_check(past);
    assert_eq!(mate(&w, &sid, "parser")["quiet"], "stuck");
    let nudges = quiet_nudges(&w, &sid);
    assert_eq!(nudges.len(), 1, "sent at once, not kept for a turn that waits on this teammate: {nudges:?}");
    assert!(nudges[0].contains("Your teammate parser has been quiet for 15 min with nothing running") && nudges[0].contains("SendMessage (to: \"parser\")"), "{}", nudges[0]);
    // The same stretch: never a second ping.
    w.hub().team_quiet_check(past + 60_000);
    w.hub().team_quiet_check(past + 30 * 60_000);
    assert_eq!(quiet_nudges(&w, &sid).len(), 1);
    // Resume (the row's button): its lead is asked to wake it.
    assert_eq!(w.hub().team_action(&sid, "agentA", "resume").unwrap(), "Asked its lead to wake parser");
    assert!(w.hosted.sent().last().unwrap().1.contains("Ask it where it is and to carry on with its part, with SendMessage (to: \"parser\")"));
    // A line from it ends the stretch; quiet and stuck again later is a new one: one more nudge.
    says(&w, &sid, [HostedEvent::ToolResult { id: "r2".into(), output: json!("ok"), failed: false, agent: Some("toolu_A".into()) }]);
    assert_eq!(mate(&w, &sid, "parser")["quiet"], serde_json::Value::Null, "a line came: not quiet");
    let again = mate(&w, &sid, "parser")["step_ms"].as_u64().unwrap() + crate::sessions::quiet::QUIET_MS;
    w.hub().team_quiet_check(again);
    assert_eq!(quiet_nudges(&w, &sid).len(), 2);
}

/// `writes`, but the crew member ends stopped (by you, or with its lead's interrupt), not done.
pub(super) fn writes_then_stops(call: &str, id: &str, name: &str, files: &[String]) -> Vec<HostedEvent> {
    let mut es = writes(call, id, name, files);
    if let Some(HostedEvent::Task { detail, .. }) = es.last_mut() {
        detail["status"] = json!("stopped");
    }
    es
}

#[test]
fn a_crew_member_that_stopped_having_written_nothing_leaves_the_crew_by_itself() {
    let w = World::new();
    repo(&w);
    let sid = lead(&w, "three mocks");
    says(&w, &sid, writes_then_stops("toolu_A", "agentA", "mock-a", &[]));
    let t = mate(&w, &sid, "mock-a");
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("aside"), Some("Stopped having written nothing")));
}

#[test]
fn the_tidy_lets_work_committed_outside_cue_or_thrown_away_leave_the_crew_and_keeps_what_waits_on_you() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "three parts");
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    says(&w, &sid, writes_then_stops("toolu_A", "agentA", "rows", &[f("a.txt")]));
    says(&w, &sid, writes_then_stops("toolu_B", "agentB", "hub", &[f("b.txt")]));
    says(&w, &sid, writes_then_stops("toolu_C", "agentC", "docs", &[f("shared.txt")]));
    says(&w, &sid, writes_then_stops("toolu_D", "agentD", "notes", &[f("ignored.txt")]));
    assert!(!w.hub().team_tidy(), "all four still have their work: three uncommitted, one where git can't tell (ignored)");
    // You commit rows's file yourself, and throw hub's away.
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "rows by hand"]);
    std::fs::remove_file(dir.join("b.txt")).unwrap();
    assert!(w.hub().team_tidy());
    let rows = mate(&w, &sid, "rows");
    assert_eq!((rows["state"].as_str(), rows["step"].as_str()), (Some("committed"), Some("Committed outside Cue")));
    assert_eq!(rows["commit"].as_str().unwrap(), git(&dir, &["rev-parse", "--short", "HEAD"]).trim(), "the commit it went in");
    let hub = mate(&w, &sid, "hub");
    assert_eq!((hub["state"].as_str(), hub["step"].as_str()), (Some("aside"), Some("Its changes are gone: nothing left to commit")));
    assert_eq!(mate(&w, &sid, "docs")["state"], "stopped", "its work still waits on you");
    assert_eq!(mate(&w, &sid, "notes")["state"], "stopped", "an ignored file: git can't say whether its work was kept, so it stays");
    assert!(!w.hub().team_tidy(), "nothing more to tidy");
}
