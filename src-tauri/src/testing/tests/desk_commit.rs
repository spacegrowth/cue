//! Looks right with a commit (hub/desk_commit.rs): the item's files git says changed are committed at once (never
//! pushed); a word never committed stops it; the item is done with its commit's short hash, and Undo takes the commit
//! back. When another session at work is editing some of them, the item is done at once and the commit waits, made on
//! its own once they're free (across a restart too).

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

/// Looks right on `id`: { commit, files, waiting }.
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
    assert_eq!((r["files"].as_u64(), r["waiting"].as_array().map(Vec::len)), (Some(2), Some(0)), "{r}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "Header jumps\n\na.txt\nb.txt", "its title the message");
    assert_eq!(r["commit"].as_str().unwrap(), git(&dir, &["rev-parse", "--short", "HEAD"]).trim());
    assert!(git(&dir, &["remote"]).trim().is_empty() && git(&dir, &["branch", "--format=%(upstream)"]).trim().is_empty(), "nothing to push to, nothing pushed");
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["commit"].as_str()), (Some("done"), r["commit"].as_str()));
}

#[test]
fn a_file_already_taken_out_with_git_rm_is_committed_as_removed() {
    let w = World::new();
    let (dir, _, id) = ready_with(
        &w,
        |_, _, dir| {
            std::fs::write(dir.join("shared.txt"), "old\n").unwrap();
            git(dir, &["add", "shared.txt"]);
            git(dir, &["commit", "-q", "-m", "shared"]);
        },
        |w, h, dir| {
            let f = |n: &str| dir.join(n).to_string_lossy().to_string();
            says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("shared.txt")]));
            git(dir, &["rm", "-q", "-f", "shared.txt"]);
        },
    );
    let r = looks_right(&w, &id);
    assert_eq!(r["files"].as_u64(), Some(2), "{r}");
    assert_eq!(git(&dir, &["show", "--name-status", "--format=", "HEAD"]).trim(), "A\ta.txt\nD\tshared.txt", "the removal goes in with the rest");
}

#[test]
fn the_leads_done_on_an_item_to_check_leaves_it_to_you_and_tells_the_lead_why_once() {
    let w = World::new();
    let (_, project, id) = ready(&w);
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let told = |w: &World| w.hosted.sent().into_iter().filter(|(s, t)| s == &lead && t.contains("not closed")).map(|(_, t)| t).collect::<Vec<_>>();
    ends(&w, &lead, &format!("Checked it.\nDONE: #{id}, #zz9"));
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check", "still yours to check");
    let said = told(&w);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].starts_with(&format!("#{id} not closed: it waits on your check\n#zz9 not closed: this Desk has no item #zz9\n")), "{}", said[0]);
    assert!(said[0].ends_with("Don't say DONE for these again."), "its words for the lead last");
    // Said again of the same item, unchanged: not told again.
    ends(&w, &lead, &format!("DONE: #{id}"));
    w.settle();
    assert_eq!(told(&w).len(), 1);
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

/// A session "tidier" at work editing b.txt in `dir`: its id.
fn tidier_editing(w: &World, dir: &std::path::Path) -> String {
    let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "tidy b", "tidier", &opts).unwrap();
    let o = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&o, HostedEvent::Taken { text: "tidy b".into() });
    w.advance(1000);
    log(w, &o).tool_use("toolu_9", "Edit", json!({ "file_path": dir.join("b.txt") }));
    w.settle();
    o
}

#[test]
fn a_file_another_session_is_editing_makes_it_done_at_once_and_the_commit_waits_till_its_free() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    let o = tidier_editing(&w, &dir);
    let r = looks_right(&w, &id);
    assert_eq!((r["commit"].as_str(), r["files"].as_u64()), (Some(""), Some(2)), "{r}");
    assert_eq!(r["waiting"], json!([{ "path": "b.txt", "by": "tidier" }]));
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "done", "done at once");
    assert_eq!(shown["commit_wait"]["busy"], json!([{ "path": "b.txt", "by": "tidier" }]));
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "nothing committed yet");
    w.tick(true);
    w.advance(20_000);
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "still waiting while it's edited");

    // The tidier's turn ends: its files are free, and the commit is made on its own.
    ends(&w, &o, "Tidied.");
    let shown = item(&w, &project, "Header jumps");
    assert!(shown["commit_wait"].is_null(), "{shown}");
    assert_eq!(shown["commit"].as_str().unwrap(), git(&dir, &["rev-parse", "--short", "HEAD"]).trim());
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "Header jumps\n\na.txt\nb.txt");
    assert!(git(&dir, &["remote"]).trim().is_empty(), "never pushed");

    // Undo takes it back as any other.
    let it = w.hub().desk_commit_undo(&id).unwrap();
    assert_eq!((it.state.as_str(), it.commit.as_str()), ("to_check", ""));
}

#[test]
fn a_waiting_commit_survives_a_restart_and_is_made_after_it() {
    let mut w = World::new();
    let (dir, project, id) = ready(&w);
    tidier_editing(&w, &dir);
    looks_right(&w, &id);
    w.relaunch();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["commit_wait"]["busy"][0]["path"].as_str()), (Some("done"), Some("b.txt")), "{shown}");
    // Its sessions ended with the restart: nobody edits b.txt now, so Cue's tick commits it.
    w.tick(true);
    w.advance(20_000);
    let shown = item(&w, &project, "Header jumps");
    assert!(shown["commit_wait"].is_null() && !shown["commit"].as_str().unwrap().is_empty(), "{shown}");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "2");
}

#[test]
fn a_word_never_committed_keeps_a_waiting_commit_back_and_says_why() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    let o = tidier_editing(&w, &dir);
    looks_right(&w, &id);
    w.hub().set_setting("project.never", json!({ project.clone(): ["Fixer"] })).unwrap();
    ends(&w, &o, "Tidied.");
    let shown = item(&w, &project, "Header jumps");
    assert!(shown["commit_wait"]["error"].as_str().unwrap().contains("\"Fixer\""), "{shown}");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "nothing committed");
    // Undo while it waits: no commit to take back, the item is to check again.
    let it = w.hub().desk_commit_undo(&id).unwrap();
    assert_eq!(it.state, "to_check");
    assert!(item(&w, &project, "Header jumps")["commit_wait"].is_null());
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

    // Once another commit came after it: the item still goes back to check, its commit stays as it is (kept on it,
    // so the window says so), and no file or commit changes.
    looks_right(&w, &id);
    let sha = item(&w, &project, "Header jumps")["commit"].as_str().unwrap().to_string();
    git(&dir, &["commit", "-q", "-m", "yours"]);
    let before = (git(&dir, &["status", "--porcelain"]), std::fs::read_to_string(dir.join("a.txt")).unwrap());
    let it = w.hub().desk_commit_undo(&id).unwrap();
    assert_eq!((it.state.as_str(), it.commit.as_str()), ("to_check", sha.as_str()));
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "3");
    assert_eq!((git(&dir, &["status", "--porcelain"]), std::fs::read_to_string(dir.join("a.txt")).unwrap()), before, "code untouched");
}

#[test]
fn undo_on_an_item_done_with_nothing_to_commit_puts_it_back_to_check() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "already in"]);
    let r = looks_right(&w, &id);
    assert_eq!(r["commit"].as_str(), Some(""), "{r}");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    let it = w.hub().desk_commit_undo(&id).unwrap();
    assert_eq!(it.state, "to_check");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "2", "no commit touched");
    assert!(w.hub().desk_commit_undo(&id).is_err(), "it isn't done now");
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

#[test]
fn send_back_takes_your_words_to_its_holder_and_puts_it_to_work_with_none_it_asks_for_a_reason() {
    let w = World::new();
    let (_, project, id) = ready(&w);
    let e = w.hub().desk_item_act(&id, "send_back", "  ").unwrap_err();
    assert!(e.contains("Say what isn't right"), "{e}");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check", "unchanged");

    let it = w.hub().desk_item_act(&id, "send_back", "It still jumps on scroll").unwrap();
    w.settle();
    assert_eq!((it.state.as_str(), it.holder.as_ref().map(|h| h.name.as_str())), ("working", Some("cue-teams")));
    let snap = w.snapshot();
    let h = snap.session_ids().into_iter().find(|s| snap.session(s)["name"] == "cue-teams").unwrap();
    let sent: Vec<String> = w.hosted.sent().into_iter().filter(|(s, _)| *s == h).map(|(_, t)| t).collect();
    assert!(sent.iter().any(|t| t.contains("isn't done: It still jumps on scroll") && t.contains(&format!("DONE: #{id}"))), "{sent:?}");
    assert!(!sent.iter().any(|t| t.contains("have another look")), "no made-up words");
}

#[test]
fn a_card_sent_back_keeps_why_and_how_many_pictures_went_with_it_across_a_restart() {
    let mut w = World::new();
    let (_, project, id) = ready(&w);
    w.hub().desk_item_act(&id, "send_back", "It still jumps\nPictures with it (open each to see it): `/u/a.png` `/u/b.png`").unwrap();
    w.settle();
    w.relaunch();
    let card = desk(&w, &project)["checks"].as_array().unwrap().iter().find(|c| c["id"] == id.as_str()).cloned().unwrap();
    assert_eq!((card["back"].as_str(), card["back_pics"].as_u64(), card["state"].as_str()), (Some("It still jumps"), Some(2), Some("working")), "{card}");
}

#[test]
fn files_changed_to_check_is_what_looks_right_will_commit_and_done_its_commit() {
    let mut w = World::new();
    let (dir, _, id) = ready(&w);
    std::fs::write(dir.join("yours.txt"), "not the item's\n").unwrap();
    let files = |w: &World| -> Vec<String> {
        let d = w.hub().desk_item_diff(&id).unwrap();
        d["diff"].as_str().unwrap().lines().filter_map(|l| l.strip_prefix("+++ b/")).map(str::to_string).collect()
    };
    assert_eq!(files(&w), ["a.txt", "b.txt"], "its files, not yours");
    assert!(git(&dir, &["diff", "--cached", "--name-only"]).trim().is_empty(), "your index untouched");
    looks_right(&w, &id);
    assert_eq!(files(&w), ["a.txt", "b.txt"], "done: its commit");
    w.relaunch();
    assert_eq!(files(&w), ["a.txt", "b.txt"], "after a restart too");
}
