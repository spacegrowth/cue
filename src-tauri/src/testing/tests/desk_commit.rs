//! Looks right with a commit (hub/desk_commit.rs): the item's files git says changed are committed at once (never
//! pushed), not those another session at work is editing; a word never committed stops it; the item is done with its
//! commit's short hash, and Undo takes the commit back.

use super::desk::desk;
use super::team::{git, lead, repo, says, writes};
use crate::hosted::HostedEvent;
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::World;
use serde_json::{json, Value};

fn ends(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

fn item(w: &World, project: &str, title: &str) -> Value {
    desk(w, project)["items"].as_array().unwrap().iter().find(|i| i["title"] == title).cloned().unwrap_or_else(|| panic!("no item {title}"))
}

/// A repository, a session whose crew wrote a.txt and b.txt for an item now to check: (repo, project, item id).
fn ready(w: &World) -> (std::path::PathBuf, String, String) {
    ready_with(w, |_, _, _| {}, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    })
}

/// The same, the item's work being what `work` does (with the session and the repository) once it's routed, and
/// `before`'s before it was.
fn ready_with(w: &World, before: impl Fn(&World, &str, &std::path::Path), work: impl Fn(&World, &str, &std::path::Path)) -> (std::path::PathBuf, String, String) {
    let dir = repo(w);
    let h = lead(w, "hello");
    ends(w, &h, "Hi.");
    before(w, &h, &dir);
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(w, &d, "I run the Desk now.");
    w.advance(1000);
    ends(w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    w.advance(1000);
    work(w, &h, &dir);
    let id = item(w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
    ends(w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(w, &project, "Header jumps")["state"], "to_check");
    (dir, project, id)
}

/// Session `sid`'s log, as Claude Code writes it.
fn log(w: &World, sid: &str) -> crate::testing::transcript::Transcript {
    crate::testing::transcript::Transcript::at(w.hub().session_origin(sid).unwrap().transcript_path)
}

/// Looks right on `id`: { commit, files, skipped }.
fn looks_right(w: &World, id: &str) -> Value {
    let r = w.hub().desk_looks_right(id).unwrap();
    w.settle();
    r
}

#[test]
fn looks_right_commits_the_items_changed_files_at_once_never_pushes_and_the_item_shows_its_commit() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    let r = looks_right(&w, &id);
    assert_eq!((r["files"].as_u64(), r["skipped"].as_array().map(Vec::len)), (Some(2), Some(0)), "{r}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "Header jumps\n\na.txt\nb.txt", "its title the message");
    assert_eq!(r["commit"].as_str().unwrap(), git(&dir, &["rev-parse", "--short", "HEAD"]).trim());
    assert!(git(&dir, &["remote"]).trim().is_empty() && git(&dir, &["branch", "--format=%(upstream)"]).trim().is_empty(), "nothing to push to, nothing pushed");
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["commit"].as_str()), (Some("done"), r["commit"].as_str()));
}

#[test]
fn a_word_never_committed_stops_the_commit_and_says_where() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    w.hub().set_setting("project.never", json!({ project.clone(): ["Fixer"] })).unwrap();
    let e = w.hub().desk_looks_right(&id).unwrap_err();
    assert!(e.contains("\"Fixer\"") && e.contains(": fixer wrote this"), "{e}");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "nothing committed");
    assert!(git(&dir, &["diff", "--cached", "--name-only"]).trim().is_empty(), "the index untouched");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
}

#[test]
fn an_item_with_no_changed_files_is_just_done() {
    let w = World::new();
    let (dir, project, id) = ready_with(&w, |_, _, _| {}, |_, _, _| {});
    let r = looks_right(&w, &id);
    assert_eq!((r["commit"].as_str(), r["files"].as_u64()), (Some(""), Some(0)));
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
}

#[test]
fn a_file_another_session_at_work_is_editing_is_left_out_and_said() {
    let w = World::new();
    let (dir, _, id) = ready(&w);
    let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "tidy b", "tidier", &opts).unwrap();
    let o = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&o, HostedEvent::Taken { text: "tidy b".into() });
    w.advance(1000);
    log(&w, &o).tool_use("toolu_9", "Edit", json!({ "file_path": dir.join("b.txt") }));
    w.settle();
    let r = looks_right(&w, &id);
    assert_eq!(r["files"], 1, "{r}");
    assert_eq!(r["skipped"], json!([{ "path": "b.txt", "by": "tidier" }]));
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt");
    assert_eq!(git(&dir, &["status", "--porcelain"]).trim(), "?? b.txt", "left as it was");
}

#[test]
fn undo_takes_the_commit_back_its_changes_stay_and_the_item_is_to_check_again() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    std::fs::write(dir.join("shared.txt"), "staged by you\n").unwrap();
    git(&dir, &["add", "shared.txt"]);
    looks_right(&w, &id);
    let it = w.hub().desk_commit_undo(&id).unwrap();
    assert_eq!((it.state.as_str(), it.commit.as_str()), ("to_check", ""));
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "the commit is gone");
    assert_eq!(git(&dir, &["status", "--porcelain"]).trim(), "A  shared.txt\n?? a.txt\n?? b.txt", "its files changed as before, unstaged; yours still staged");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    assert!(w.hub().desk_commit_undo(&id).is_err(), "nothing to undo now");

    // Once another commit came after it: refused, nothing changed.
    looks_right(&w, &id);
    git(&dir, &["commit", "-q", "-m", "yours"]);
    assert_eq!(w.hub().desk_commit_undo(&id).unwrap_err(), "Can't undo it: another commit came after it");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "3");
}

#[test]
fn a_session_doing_the_item_itself_commits_what_it_changed_while_it_held_it() {
    let w = World::new();
    let earlier = |w: &World, h: &str, dir: &std::path::Path| {
        std::fs::write(dir.join("shared.txt"), "earlier work\n").unwrap();
        log(w, h).tool_use("toolu_0", "Edit", json!({ "file_path": dir.join("shared.txt") }));
    };
    let (dir, _, id) = ready_with(&w, earlier, |w, h, dir| {
        std::fs::write(dir.join("a.txt"), "edited\n").unwrap();
        std::fs::write(dir.join("b.txt"), "from the shell\n").unwrap();
        std::fs::write(dir.join("by_builder.txt"), "read only\n").unwrap();
        log(w, h)
            .tool_use("toolu_1", "Edit", json!({ "file_path": dir.join("a.txt") }))
            .tool_use("toolu_2", "Bash", json!({ "command": "echo hi > b.txt" }))
            .tool_use("toolu_3", "Read", json!({ "file_path": dir.join("by_builder.txt") }))
            .tool_use("toolu_4", "Edit", json!({ "file_path": "/elsewhere/x.txt" }));
    });
    let r = looks_right(&w, &id);
    assert_eq!(r["files"], 2, "{r}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt\nb.txt", "its own edits while it held the item; not one from before, a read, or one outside its folder");
}

#[test]
fn each_done_leaves_a_card_for_the_lead_with_who_what_to_test_and_its_files_kept_after_a_restart() {
    let w = World::new();
    let (_, project, id) = ready(&w);
    let cards = |w: &World| desk(w, &project)["checks"].as_array().unwrap().clone();
    let c = &cards(&w)[0];
    assert_eq!((c["id"].as_str(), c["title"].as_str(), c["kind"].as_str()), (Some(id.as_str()), Some("Header jumps"), Some("bug")));
    assert_eq!((c["by"].as_str(), c["done_when"].as_str(), c["state"].as_str(), c["last"].as_bool()), (Some("cue-teams"), Some("it stays put"), Some("to_check"), Some(true)));
    assert_eq!(c["files"], json!(["a.txt", "b.txt"]), "the files Looks right would offer: {c}");
    assert_eq!(c["cue"], false, "not made in this Cue's source");
    assert!(w.snapshot().0["desk"]["installed_ms"].as_u64().is_some_and(|t| t > 0));

    // Kept: what Cue loads as it starts has it.
    let (items, _, _) = crate::db::desk_load(crate::clock::now_ms());
    let kept = items.iter().find(|i| i.id == id).unwrap();
    assert_eq!((kept.checks.len(), kept.checks[0].by.as_str(), kept.checks[0].files.len()), (1, "cue-teams", 2));

    // Sent back and done again: a second card; the first is no longer the one to act on.
    w.hub().desk_item_act(&id, "send_back", "It still jumps").unwrap();
    w.settle();
    let snap = w.snapshot();
    let h = snap.session_ids().into_iter().find(|s| snap.session(s)["name"] == "cue-teams").unwrap();
    ends(&w, &h, &format!("Fixed it properly.\nDONE: #{id}"));
    let all = cards(&w);
    assert_eq!(all.len(), 2, "{all:?}");
    assert_eq!((all[0]["last"].as_bool(), all[1]["last"].as_bool()), (Some(false), Some(true)));
}
