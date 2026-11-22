//! Desk items worked by sessions on another machine (#500, desk/hub/landing/remote_item.rs): the holder there is told to
//! work on a branch of its own (`desk/<id>`) from the shared remote's main line; at its DONE Cue word-checks the
//! branch there, pushes it from there, fetches it here and lands it on the main line like any item. Played with a
//! machine ("box") whose git runs in place, on a clone of the same remote.

use super::desk_commit::{ends, item};
use super::team::{git, lead};
use crate::hosted::HostedEvent;
use crate::testing::World;
use std::path::{Path, PathBuf};

/// The main checkout pushed to a bare remote, and the machine's clone of it.
fn with_remote(dir: &Path) -> (PathBuf, PathBuf) {
    let bare = dir.join("shared.git");
    let there = dir.join("there");
    git(dir, &["init", "-q", "--bare", "-b", "main", &bare.to_string_lossy()]);
    git(dir, &["remote", "add", "origin", &bare.to_string_lossy()]);
    git(dir, &["push", "-q", "-u", "origin", "main"]);
    git(dir, &["clone", "-q", &bare.to_string_lossy(), &there.to_string_lossy()]);
    git(&there, &["config", "user.email", "t@example.com"]);
    git(&there, &["config", "user.name", "T"]);
    (bare, there)
}

/// An item routed to a session on "box", working in the machine's clone: (repo, bare remote, clone there, project,
/// id, holder), and the machine (kept for the test).
fn routed_there(w: &World) -> ((PathBuf, PathBuf, PathBuf, String, String, String), crate::testing::FakeSsh) {
    routed_there_with(w, |_, _, _| {})
}

/// `routed_there`, `before` run (with the project and the main checkout) once its remote is there, before the item is
/// routed.
fn routed_there_with(w: &World, before: impl Fn(&World, &str, &Path)) ->((PathBuf, PathBuf, PathBuf, String, String, String), crate::testing::FakeSsh) {
    let ssh = crate::testing::FakeSsh::new();
    ssh.host("box", crate::testing::RUNS_IT);
    crate::machines::add("box", "").unwrap();
    let dir = super::team::main_repo(w);
    let h = lead(w, "hello");
    ends(w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(w, &d, "I run the Desk now.");
    // The Desk's session that takes the item is on "box", in its clone there.
    let (bare, there) = with_remote(&dir);
    before(w, &project, &dir);
    let cwd = there.to_string_lossy().to_string();
    w.hub().write(|st| {
        if let Some(s) = st.sessions.get_mut(&h) {
            (s.origin.machine, s.origin.cwd) = ("box".into(), cwd);
        }
    });
    w.advance(1000);
    ends(w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    w.advance(1000);
    let id = item(w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
    ((dir, bare, there, project, id, h), ssh)
}

/// The holder's work on its branch there, as it was told.
fn works_there(there: &Path, id: &str, file: &str, text: &str, message: &str) {
    let branch = format!("desk/{id}");
    if git(there, &["branch", "--list", &branch]).trim().is_empty() {
        git(there, &["fetch", "-q", "origin"]);
        git(there, &["switch", "-q", "-c", &branch, "origin/main"]);
    }
    std::fs::write(there.join(file), text).unwrap();
    git(there, &["add", "-f", file]);
    git(there, &["commit", "-q", "-m", message]);
}

/// The holder's DONE, naming its branch there and the last commit on it (#596).
fn done_there(there: &Path, id: &str) -> String {
    let sha = git(there, &["rev-parse", &format!("desk/{id}")]).trim().to_string();
    format!("Fixed.\nDONE: #{id} | branch: desk/{id} | commit: {sha}")
}

#[test]
fn an_item_routed_to_a_session_on_another_machine_is_worked_on_a_branch_there_and_lands_here_at_its_done() {
    let w = World::new();
    let ((dir, bare, there, project, id, h), _ssh) = routed_there(&w);
    let before = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    // Told to work on a branch of its own there, and not to push; no copy here.
    let it = item(&w, &project, "Header jumps");
    assert!(it["tree"].is_null(), "no copy here for work on another machine: {it}");
    assert_eq!((it["remote"]["branch"].as_str(), it["remote"]["onto"].as_str(), it["machine"].as_str()), (Some(format!("desk/{id}").as_str()), Some("main"), Some("box")), "{it}");
    let told: Vec<String> = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains(&format!("Desk item #{id}"))).map(|(_, t)| t).collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains(&format!("git switch -c desk/{id} origin/main")) && told[0].contains("Don't push"), "{}", told[0]);
    assert!(told[0].contains(&format!("DONE: #{id} | branch: desk/{id} | commit: ")), "told to name its branch and commit: {}", told[0]);
    assert!(!told[0].contains(".claude/worktrees"), "no copy here named: {}", told[0]);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    works_there(&there, &id, "b.txt", "b\n", "second go");
    ends(&w, &h, &done_there(&there, &id));
    // Pushed from there (its own branch only), landed here as one commit.
    let pushed = git(&there, &["rev-parse", &format!("desk/{id}")]).trim().to_string();
    assert_eq!(git(&bare, &["rev-parse", &format!("refs/heads/desk/{id}")]).trim(), pushed, "its branch on the remote");
    assert_eq!(git(&bare, &["rev-parse", "main"]).trim(), before, "main on the remote untouched");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD~1"]).trim(), before, "one commit on the main line, no merge");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt\nb.txt");
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"]["step"].as_str()), (Some("to_check"), Some("checked")), "{it}");
    assert_eq!(it["commit"].as_str().unwrap(), git(&dir, &["rev-parse", "--short", "HEAD"]).trim());
    assert!(it["tree"].is_null(), "its copy here is gone: {it}");
    assert!(git(&dir, &["for-each-ref", &format!("refs/cue/remote/{id}")]).trim().is_empty(), "and what was fetched");
    assert!(git(&dir, &["worktree", "list"]).lines().count() == 1, "no copy left here");
}

#[test]
fn a_never_commit_word_on_the_branch_there_hands_it_back_before_anything_is_pushed() {
    let w = World::new();
    let ((dir, bare, there, project, id, h), _ssh) = routed_there(&w);
    w.hub().set_setting("project.never", serde_json::json!({ project.clone(): ["sekrit"] })).unwrap();
    works_there(&there, &id, "a.txt", "the sekrit plan\n", "first go");
    ends(&w, &h, &done_there(&there, &id));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"].as_str(), Some("working"), "back at work: {it}");
    assert!(git(&bare, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "nothing pushed");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "nothing landed");
    let back = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains("isn't done")).map(|(_, t)| t).last().unwrap_or_default();
    assert!(back.contains("sekrit") && back.contains("a.txt"), "{back}");
    assert!(back.contains("history"), "told every commit is pushed: {back}");
    // Taken out of the branch's history there, DONE again: it lands.
    git(&there, &["reset", "-q", "--hard", "origin/main"]);
    works_there(&there, &id, "a.txt", "the plan\n", "reworded");
    ends(&w, &h, &done_there(&there, &id));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
    assert_eq!(item(&w, &project, "Header jumps")["state"].as_str(), Some("to_check"));
}

#[test]
fn a_machine_that_doesnt_answer_at_done_waits_and_is_tried_again() {
    let w = World::new();
    let ((dir, _, there, project, id, h), ssh) = routed_there(&w);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    // The machine's gone quiet.
    ssh.unplug("box");
    ends(&w, &h, &done_there(&there, &id));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"]["wait"].as_str()), (Some("finishing"), Some("Machine not answering")), "{it}");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1");
    // Back: the tick a minute on lands it.
    ssh.host("box", crate::testing::RUNS_IT);
    w.advance(61_000);
    super::desk_commit::tick(&w);
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
}

#[test]
fn a_route_on_a_machine_cue_doesnt_know_is_refused() {
    let w = World::new();
    let ((_, _, _, project, _, _), _ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | on: nowhere | in: /src/app | why: there");
    let refused = w.hosted.sent().into_iter().any(|(s, t)| s == lead && t.contains("no machine called nowhere") && t.contains("box"));
    assert!(refused, "{:?}", w.hosted.sent());
    assert!(item(&w, &project, "Footer jumps")["on_machine"].is_null());
}

#[test]
fn a_commit_with_an_email_git_made_up_from_the_machine_is_handed_back_unpushed() {
    let w = World::new();
    let ((_, bare, there, project, id, h), _ssh) = routed_there(&w);
    git(&there, &["config", "user.email", "t@box"]);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    ends(&w, &h, &done_there(&there, &id));
    assert_eq!(item(&w, &project, "Header jumps")["state"].as_str(), Some("working"));
    assert!(git(&bare, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "nothing pushed");
    let back = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains("isn't done")).map(|(_, t)| t).last().unwrap_or_default();
    assert!(back.contains("t@box") && back.contains("user.email"), "{back}");
}

#[test]
fn a_holder_no_longer_on_that_machine_finishes_its_item_here_and_nothing_is_pushed_from_there() {
    let w = World::new();
    let ((_, bare, there, project, id, h), _ssh) = routed_there(&w);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    // Its session moved back to this Mac.
    w.hub().write(|st| {
        if let Some(s) = st.sessions.get_mut(&h) {
            s.origin.machine.clear();
        }
    });
    ends(&w, &h, &done_there(&there, &id));
    let it = item(&w, &project, "Header jumps");
    assert!(it["remote"].is_null() && it["machine"].as_str().unwrap_or("").is_empty(), "{it}");
    assert!(git(&bare, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "nothing pushed from there");
}

#[test]
fn a_done_that_names_no_commit_is_handed_back_with_the_line_to_say() {
    let w = World::new();
    let ((dir, bare, there, project, id, h), _ssh) = routed_there(&w);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["state"].as_str(), Some("working"));
    assert!(git(&bare, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "nothing pushed");
    let back = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains("isn't done")).map(|(_, t)| t).last().unwrap_or_default();
    assert!(back.contains("didn't name the last commit") && back.contains(&format!("DONE: #{id} | branch: desk/{id} | commit:")), "{back}");
    // Said as it was told: it lands.
    ends(&w, &h, &done_there(&there, &id));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
}

#[test]
fn a_done_naming_another_commit_waits_for_its_branch_and_lands_once_the_done_is_right() {
    let w = World::new();
    let ((dir, bare, there, project, id, h), _ssh) = routed_there(&w);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    let named = done_there(&there, &id);
    // A commit after the one its DONE names: the branch there isn't what it said.
    works_there(&there, &id, "b.txt", "b\n", "second go");
    ends(&w, &h, &named);
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"].as_str(), Some("finishing"), "{it}");
    assert!(it["finish"]["wait"].as_str().unwrap_or("").starts_with("Waiting for its branch"), "{it}");
    assert!(git(&bare, &["branch", "--list", &format!("desk/{id}")]).trim().is_empty(), "nothing pushed");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "1", "nothing landed");
    let told = || w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains("waits for its branch")).map(|(_, t)| t).collect::<Vec<_>>();
    assert_eq!(told().len(), 1, "{:?}", told());
    assert!(told()[0].contains(&format!("DONE: #{id} | branch: desk/{id} | commit:")), "{:?}", told());
    // Tried again a minute on: still waiting, its holder not told again.
    w.advance(61_000);
    super::desk_commit::tick(&w);
    assert!(item(&w, &project, "Header jumps")["finish"]["wait"].as_str().unwrap_or("").starts_with("Waiting for its branch"));
    assert_eq!(told().len(), 1, "told once: {:?}", told());
    // Its DONE again, naming its last commit: it lands at once.
    ends(&w, &h, &done_there(&there, &id));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt\nb.txt");
}

#[test]
fn a_missing_branch_waits_and_lands_once_its_there() {
    let w = World::new();
    let ((dir, _, there, project, id, h), _ssh) = routed_there(&w);
    works_there(&there, &id, "a.txt", "a\n", "first go");
    let named = done_there(&there, &id);
    // Its work is on a branch of another name there.
    git(&there, &["branch", "-q", "-m", &format!("desk/{id}"), "wip"]);
    ends(&w, &h, &named);
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"].as_str(), Some("finishing"), "not handed back: {it}");
    assert!(it["finish"]["wait"].as_str().unwrap_or("").starts_with("Waiting for its branch"), "{it}");
    let told = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains("waits for its branch")).map(|(_, t)| t).collect::<Vec<_>>();
    assert!(told.len() == 1 && told[0].contains(&format!("can't find the branch desk/{id}")), "{told:?}");
    // Its branch is back: the tick a minute on lands it.
    git(&there, &["branch", "-q", "-m", "wip", &format!("desk/{id}")]);
    w.advance(61_000);
    super::desk_commit::tick(&w);
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"));
}

#[test]
fn a_clash_goes_back_to_the_holder_on_the_machine_and_the_page_shows_what_was_fetched() {
    let w = World::new();
    let ((dir, _, there, project, id, h), _ssh) = routed_there(&w);
    // The main line here moved on in the same file.
    std::fs::write(dir.join("a.txt"), "here\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "here first"]);
    works_there(&there, &id, "a.txt", "there\n", "first go");
    ends(&w, &h, &done_there(&there, &id));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"].as_str(), Some("working"), "back to its holder: {it}");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), "here first", "nothing landed");
    let back = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains("isn't done")).map(|(_, t)| t).last().unwrap_or_default();
    assert!(back.contains("clashes") && back.contains(&format!("merge origin/main into desk/{id}")), "settled there: {back}");
    assert!(back.contains(&format!("DONE: #{id} | branch: desk/{id} | commit:")), "{back}");
    // Its page: the work fetched here, read here.
    let d = w.hub().desk_item_diff(&id).unwrap();
    assert_eq!(d["machine"].as_str(), Some(""), "{d}");
    assert!(d["diff"].as_str().unwrap_or("").contains("+there"), "{d}");
}

/// Two landings on main here the shared copy doesn't have.
fn two_ahead(_: &World, _: &str, dir: &Path) {
    for (f, m) in [("c.txt", "one landing"), ("d.txt", "another landing")] {
        std::fs::write(dir.join(f), "x\n").unwrap();
        git(dir, &["add", "-f", f]);
        git(dir, &["commit", "-q", "-m", m]);
    }
}

/// The push card waiting in the lead's chat, if any.
pub(super) fn push_card(w: &World, project: &str) -> Option<serde_json::Value> {
    let lead = w.hub().desk_lead_sid(project)?;
    let snap = w.snapshot();
    snap.cards_of(&lead).into_iter().find(|c| !c["tool_input"]["desk_push"].is_null() && c["status"] == "pending").cloned()
}

pub(super) fn answer(w: &World, id: &str, a: &str) {
    w.hub().respond(id, crate::model::Decision { behavior: "allow".into(), answers: Some(serde_json::json!({ "q": a })), ..Default::default() });
    w.settle();
    w.advance(1000);
}

/// Messages to `h` that start item `id` on its branch there.
fn starts(w: &World, h: &str, id: &str) -> Vec<String> {
    let made = format!("switch -c desk/{id}");
    w.hosted.sent().into_iter().filter(|(s, t)| s == h && t.contains(&made)).map(|(_, t)| t).collect()
}

fn same_main(dir: &Path, bare: &Path) -> bool {
    git(bare, &["rev-parse", "main"]).trim() == git(dir, &["rev-parse", "main"]).trim()
}

/// Main here ahead of the shared copy as an item starts on another machine (#599): you're asked to push first, its
/// holder's start held; a yes pushes main (checked) and then it starts from current work.
#[test]
fn main_ahead_asks_to_push_before_an_item_starts_on_another_machine_and_a_yes_pushes_then_starts() {
    let w = World::new();
    let ((dir, bare, _, project, id, h), _ssh) = routed_there_with(&w, two_ahead);
    let card = push_card(&w, &project).expect("asked to push first");
    assert!(card.to_string().contains("Main is 2 landings ahead of the shared copy; the other machine would start from old work. Push main first?"), "{card}");
    assert!(starts(&w, &h, &id).is_empty(), "held till you answer");
    assert!(!same_main(&dir, &bare));
    answer(&w, card["id"].as_str().unwrap(), "Yes");
    assert!(same_main(&dir, &bare), "main pushed");
    let told = starts(&w, &h, &id);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains(&format!("Desk item #{id}")) && !told[0].contains("older work"), "{}", told[0]);
    assert!(push_card(&w, &project).is_none());
}

/// Your own words typed on the push card (#651: "what does it mean? what do i do") are neither a yes nor a no: the card
/// stays open, nothing is pushed, the start stays held, and the words go to the lead to answer. A Yes after still pushes.
#[test]
fn words_typed_on_the_push_card_go_to_the_lead_and_the_card_stays_open() {
    let w = World::new();
    let ((dir, bare, _, project, id, h), _ssh) = routed_there_with(&w, two_ahead);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    let card = push_card(&w, &project).expect("asked to push first");
    let cid = card["id"].as_str().unwrap().to_string();
    answer(&w, &cid, "what does it mean? what do i do");
    let open = push_card(&w, &project).expect("the card stays open");
    assert_eq!(open["id"], card["id"]);
    assert_eq!(open["tool_input"]["desk_said"][0]["text"], "what does it mean? what do i do", "{open}");
    assert!(!same_main(&dir, &bare), "nothing pushed");
    assert!(starts(&w, &h, &id).is_empty(), "the start still waits on your Yes or No");
    let told: Vec<String> = w.hosted.sent().into_iter().filter(|(s, t)| *s == lead && t.contains("what does it mean? what do i do")).map(|(_, t)| t).collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].starts_with("On Cue's question in your chat (\"Main is 2 landings ahead") && told[0].contains("Cue took neither"), "{}", told[0]);
    // Typed in the card's reply box instead: the same.
    w.hub().reply(&cid, "and if I say no?", &[]).unwrap();
    w.settle();
    w.advance(1000);
    assert!(push_card(&w, &project).is_some(), "still open");
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == lead && t.contains("\"and if I say no?\"")));
    assert!(starts(&w, &h, &id).is_empty());
    answer(&w, &cid, "Yes");
    assert!(same_main(&dir, &bare), "main pushed on the Yes");
    assert_eq!(starts(&w, &h, &id).len(), 1);
    assert!(push_card(&w, &project).is_none());
}

#[test]
fn a_no_starts_the_item_anyway_saying_it_starts_from_older_work() {
    let w = World::new();
    let ((dir, bare, _, project, id, h), _ssh) = routed_there_with(&w, two_ahead);
    let card = push_card(&w, &project).expect("asked");
    answer(&w, card["id"].as_str().unwrap(), "No");
    assert!(!same_main(&dir, &bare), "nothing pushed");
    let told = starts(&w, &h, &id);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("starts from older work") && told[0].contains("2 landings"), "{}", told[0]);
}

#[test]
fn even_with_the_shared_copy_nothing_is_asked_before_a_remote_start() {
    let w = World::new();
    let ((_, _, _, project, id, h), _ssh) = routed_there(&w);
    assert!(push_card(&w, &project).is_none());
    assert_eq!(starts(&w, &h, &id).len(), 1);
}

#[test]
fn a_project_set_to_never_push_isnt_asked_before_a_remote_start() {
    let w = World::new();
    let ((dir, bare, _, project, id, h), _ssh) = routed_there_with(&w, |w, project, dir| {
        w.hub().set_setting("project.lands", serde_json::json!({ project: { "push": "never" } })).unwrap();
        two_ahead(w, project, dir);
    });
    assert!(push_card(&w, &project).is_none(), "never: nothing asked");
    assert!(!same_main(&dir, &bare), "nothing pushed");
    assert_eq!(starts(&w, &h, &id).len(), 1, "started at once");
}

/// "After each landing": main ahead is pushed (checked) before the item starts there, nothing asked.
#[test]
fn a_project_pushed_after_each_landing_pushes_main_before_a_remote_start_without_asking() {
    let w = World::new();
    let ((dir, bare, _, project, id, h), _ssh) = routed_there_with(&w, |w, project, dir| {
        w.hub().set_setting("project.lands", serde_json::json!({ project: { "push": "after" } })).unwrap();
        two_ahead(w, project, dir);
    });
    w.settle();
    assert!(push_card(&w, &project).is_none(), "after: nothing asked");
    assert!(same_main(&dir, &bare), "main pushed before it started");
    let told = starts(&w, &h, &id);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(!told[0].contains("older work"), "{}", told[0]);
}

/// A yes before a remote start still goes through the word check: refused with the find named, nothing pushed, and
/// the item starts saying it starts from older work.
#[test]
fn a_yes_before_a_remote_start_is_refused_when_the_leak_check_finds_a_word() {
    let w = World::new();
    let ((dir, bare, _, project, id, h), _ssh) = routed_there_with(&w, |w, project, dir| {
        w.hub().set_setting("project.never", serde_json::json!({ project: ["zorblax"] })).unwrap();
        std::fs::write(dir.join("shared.txt"), "the zorblax plan\n").unwrap();
        git(dir, &["add", "-f", "shared.txt"]);
        git(dir, &["commit", "-q", "-m", "a landing"]);
    });
    let card = push_card(&w, &project).expect("asked to push first");
    answer(&w, card["id"].as_str().unwrap(), "Yes");
    assert!(!same_main(&dir, &bare), "nothing pushed");
    let told = starts(&w, &h, &id);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("starts from older work"), "{}", told[0]);
    let said: Vec<String> = w.hub().read(|st| st.desk.decisions.iter().map(|d| d.text.clone()).collect());
    assert!(said.iter().any(|t| t.starts_with("Not pushed") && t.contains("zorblax")), "{said:?}");
}

/// The push card closed without an answer (the lead's session ended): the start it held still goes, saying it starts
/// from older work, so the item isn't left working with a holder that was never told.
#[test]
fn a_push_card_that_closes_unanswered_still_sends_the_held_start_with_the_note() {
    let w = World::new();
    let ((dir, bare, _, project, id, h), _ssh) = routed_there_with(&w, two_ahead);
    let card = push_card(&w, &project).expect("asked");
    assert!(starts(&w, &h, &id).is_empty(), "held");
    w.hub().finish(card["id"].as_str().unwrap(), "gone", "session ended");
    w.settle();
    w.advance(1000);
    assert!(!same_main(&dir, &bare), "nothing pushed");
    let told = starts(&w, &h, &id);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("starts from older work"), "{}", told[0]);
}

/// Two items starting on another machine while main is ahead share one push card; a yes pushes once and starts both.
#[test]
fn two_items_starting_together_share_one_push_card_and_a_yes_starts_both() {
    let w = World::new();
    let ((dir, bare, there, project, id, h), _ssh) = routed_there_with(&w, two_ahead);
    assert!(push_card(&w, &project).is_some(), "asked for the first");
    // A second session of the Desk, on the same machine's clone, given another item.
    let opts = crate::ipc::StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = crate::ipc::start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "hi", "cue-footer", &opts).unwrap();
    let h2 = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&h2, HostedEvent::Taken { text: "hi".into() });
    ends(&w, &h2, "Hi.");
    w.hub().desk_set_member(&project, &h2, true).unwrap();
    let cwd = there.to_string_lossy().to_string();
    w.hub().write(|st| {
        if let Some(s) = st.sessions.get_mut(&h2) {
            (s.origin.machine, s.origin.cwd) = ("box".into(), cwd);
        }
    });
    let d = w.hub().desk_lead_sid(&project).unwrap();
    ends(&w, &d, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> cue-footer | why: it made the footer");
    w.advance(1000);
    let id2 = item(&w, &project, "Footer jumps")["id"].as_str().unwrap().to_string();
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    let cards: Vec<serde_json::Value> = w.snapshot().cards_of(&lead).into_iter().filter(|c| !c["tool_input"]["desk_push"].is_null() && c["status"] == "pending").cloned().collect();
    assert_eq!(cards.len(), 1, "one card for the project: {cards:?}");
    assert_eq!(cards[0]["tool_input"]["desk_push"]["starts"].as_array().map(Vec::len), Some(2), "{}", cards[0]);
    assert!(starts(&w, &h2, &id2).is_empty(), "the second held too");
    answer(&w, cards[0]["id"].as_str().unwrap(), "Yes");
    assert!(same_main(&dir, &bare), "pushed");
    for (s, i) in [(&h, &id), (&h2, &id2)] {
        let told = starts(&w, s, i);
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(!told[0].contains("older work"), "{}", told[0]);
    }
}

/// A shared copy that doesn't answer isn't taken as even: you're still asked, saying Cue couldn't compare.
#[test]
fn a_shared_copy_that_cant_be_reached_is_still_asked_about_before_a_remote_start() {
    let w = World::new();
    let ((_, _, _, project, id, h), _ssh) = routed_there_with(&w, |_, _, dir| {
        let gone = dir.join("no-such-remote.git");
        git(dir, &["remote", "set-url", "origin", gone.to_str().unwrap()]);
    });
    let card = push_card(&w, &project).expect("asked although it couldn't compare");
    assert!(card.to_string().contains("couldn't compare main with the shared copy"), "{card}");
    assert!(starts(&w, &h, &id).is_empty(), "held till you answer");
    answer(&w, card["id"].as_str().unwrap(), "No");
    let told = starts(&w, &h, &id);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("starts from older work"), "{}", told[0]);
}

/// What the lead was told after it routed, the last message to it holding `words`.
fn told_lead(w: &World, lead: &str, words: &str) -> String {
    w.hosted.sent().into_iter().filter(|(s, t)| s == lead && t.contains(words)).map(|(_, t)| t).last().unwrap_or_default()
}

#[test]
fn a_desks_first_item_on_a_machine_waits_for_your_yes_and_its_folder_then_the_machine_is_the_desks() {
    let w = World::new();
    let ((_, _, _, project, _, _), _ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    // Never used here: refused, and told to ask first.
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | on: box | why: there");
    let back = told_lead(&w, &lead, "hasn't used");
    assert!(back.contains("ASK: #") && back.contains("in: <folder there>"), "{back}");
    assert!(item(&w, &project, "Footer jumps")["on_machine"].is_null());
    // A folder that isn't a full path there isn't taken either.
    let id = item(&w, &project, "Footer jumps")["id"].as_str().unwrap().to_string();
    ends(&w, &lead, &format!("ROUTE: #{id} -> new | on: box | in: code/app | why: you said yes"));
    assert!(told_lead(&w, &lead, "full path").contains("code/app"));
    // After your yes, with its folder: routed, and the Desk remembers the folder.
    ends(&w, &lead, &format!("ROUTE: #{id} -> new | on: box | in: /srv/app/ | why: you said yes"));
    let it = item(&w, &project, "Footer jumps");
    assert_eq!((it["on_machine"]["machine"].as_str(), it["on_machine"]["folder"].as_str()), (Some("box"), Some("/srv/app")), "{it}");
    // Once started there the folder is the Desk's; until then the next item there is asked about too.
    crate::desk::machine_pref::remember(&project, "box", "/srv/app");
    ends(&w, &lead, "ITEM: bug | Sidebar jumps | done when: it stays put\nROUTE: #new -> new | on: box | why: there too");
    assert_eq!(item(&w, &project, "Sidebar jumps")["on_machine"]["folder"].as_str(), Some("/srv/app"));
}

#[test]
fn an_item_nobody_has_shows_the_desks_machine_and_your_choice_beats_the_leads() {
    let w = World::new();
    let ((_, _, _, project, header, _), _ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    // No folder known there: you can't pick it yet.
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put");
    let id = item(&w, &project, "Footer jumps")["id"].as_str().unwrap().to_string();
    assert!(w.hub().desk_start_on(&id, "box").unwrap_err().contains("folder"));
    assert!(item(&w, &project, "Footer jumps")["machine"].as_str().unwrap_or("").is_empty(), "this Mac by default");
    // The Desk starts its sessions on box: its unstarted items show it.
    w.hub().desk_lands_set(&project, &serde_json::json!({ "machine": "box", "machine_folder": "/srv/app" })).unwrap();
    assert_eq!(item(&w, &project, "Footer jumps")["machine"].as_str(), Some("box"));
    // You chose this Mac for it: shown, and the lead can't send it elsewhere.
    let it = w.hub().desk_start_on(&id, "").unwrap();
    assert!(it.on_machine.is_none() && it.start_on.as_deref() == Some(""));
    assert!(item(&w, &project, "Footer jumps")["machine"].as_str().unwrap_or("").is_empty());
    ends(&w, &lead, &format!("ROUTE: #{id} -> new | on: box | why: there"));
    assert!(told_lead(&w, &lead, "chose to start").contains("this Mac"));
    assert!(item(&w, &project, "Footer jumps")["on_machine"].is_null());
    // On box: the lead's on: this Mac is refused too, and a route without on: is taken.
    w.hub().desk_start_on(&id, "box").unwrap();
    ends(&w, &lead, &format!("ROUTE: #{id} -> new | on: this Mac | why: here"));
    assert!(told_lead(&w, &lead, "chose to start").contains("box"));
    // An item already started stays where it is.
    assert!(w.hub().desk_start_on(&header, "").unwrap_err().contains("started"));
}

#[test]
fn on_this_mac_starts_it_here_whatever_the_desk_starts_on() {
    let w = World::new();
    let ((_, _, _, project, _, _), _ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    w.hub().desk_lands_set(&project, &serde_json::json!({ "machine": "box", "machine_folder": "/srv/app" })).unwrap();
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | on: this Mac | why: here");
    let it = item(&w, &project, "Footer jumps");
    assert_eq!(it["on_machine"]["machine"].as_str(), Some(""), "{it}");
    assert!(it["machine"].as_str().unwrap_or("").is_empty(), "{it}");
}

// A quiet machine (#598): the holder's branch there is pushed at each of its stops, so its work isn't only on that
// machine; after half an hour quiet the lead asks you to wait or move it; moved, its new holder starts from that push.

/// Item `id`'s machine "box" gone quiet since now: it doesn't answer, and Cue has marked it so.
fn box_quiet(w: &World, ssh: &crate::testing::FakeSsh) {
    ssh.unplug("box");
    let now = crate::clock::now_ms();
    w.hub().write(|st| crate::hub::machine_quiet_from(st, "box", now));
}

#[test]
fn a_holder_on_another_machine_has_its_branch_pushed_at_each_stop() {
    let w = World::new();
    let ((dir, bare, there, project, id, h), _ssh) = routed_there(&w);
    let branch = format!("desk/{id}");
    // A stop before any commit: nothing to push.
    ends(&w, &h, "Looking around.");
    w.advance(1000);
    assert!(git(&bare, &["branch", "--list", &branch]).trim().is_empty(), "nothing pushed before a commit");
    works_there(&there, &id, "a.txt", "a\n", "first go");
    ends(&w, &h, "Half way.");
    w.advance(1000);
    let head = git(&there, &["rev-parse", &branch]).trim().to_string();
    assert_eq!(git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]).trim(), head, "pushed at its stop");
    assert_eq!(git(&dir, &["rev-parse", &format!("refs/cue/remote/{id}")]).trim(), head, "and fetched here");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["remote"]["sha"].as_str(), Some(head.as_str()), "{it}");
    assert!(it["remote"]["pushed_at"].as_u64().unwrap_or(0) > 0, "{it}");
    assert_eq!(it["state"].as_str(), Some("working"), "still at work: {it}");
    // A never-commit word on it: that stop pushes nothing, and the remote keeps the last good push.
    w.hub().set_setting("project.never", serde_json::json!({ project.clone(): ["sekrit"] })).unwrap();
    works_there(&there, &id, "b.txt", "the sekrit plan\n", "second go");
    ends(&w, &h, "More.");
    w.advance(1000);
    assert_eq!(git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]).trim(), head, "not pushed past the word");
}

#[test]
fn a_machine_quiet_for_half_an_hour_has_the_lead_ask_you_to_wait_or_move_it() {
    let w = World::new();
    let ((_, _, there, project, id, h), ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    works_there(&there, &id, "a.txt", "a\n", "first go");
    ends(&w, &h, "Half way.");
    w.advance(1000);
    box_quiet(&w, &ssh);
    // The item says so, with how long.
    w.advance(12 * 60_000);
    let it = item(&w, &project, "Header jumps");
    assert!(it["remote"]["quiet_since"].as_u64().unwrap_or(0) > 0, "{it}");
    super::desk_commit::tick(&w);
    assert!(told_lead(&w, &lead, "hasn't answered").is_empty(), "not asked before half an hour");
    w.advance(20 * 60_000);
    super::desk_commit::tick(&w);
    let asked = told_lead(&w, &lead, "hasn't answered");
    assert!(asked.contains(&format!("ASK: #{id}")) && asked.contains("Wait") && asked.contains("Move it"), "{asked}");
    assert!(asked.contains(&format!("starts from desk/{id}, last pushed")), "{asked}");
}

#[test]
fn an_item_moved_off_a_quiet_machine_starts_its_new_holder_here_from_its_last_push() {
    let w = World::new();
    let ((dir, _, there, project, id, h), ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    works_there(&there, &id, "a.txt", "a\n", "first go");
    ends(&w, &h, "Half way.");
    w.advance(1000);
    let pushed = git(&there, &["rev-parse", &format!("desk/{id}")]).trim().to_string();
    // Not pushed: work after its last stop stays there.
    works_there(&there, &id, "b.txt", "b\n", "second go");
    box_quiet(&w, &ssh);
    w.advance(31 * 60_000);
    // You said move it: the lead routes it to a new session here.
    ends(&w, &lead, &format!("ROUTE: #{id} -> new | on: this Mac | why: you said move it"));
    w.advance(1000);
    let it = item(&w, &project, "Header jumps");
    assert!(it["remote"].is_null() && it["pushed_before"].is_null(), "taken as its new holder started: {it}");
    let copy = it["tree"]["path"].as_str().unwrap_or_else(|| panic!("a copy here: {it}")).to_string();
    let copy = std::path::Path::new(&copy);
    assert_eq!(git(copy, &["rev-parse", "HEAD"]).trim(), pushed, "its copy starts from the last push");
    assert!(copy.join("a.txt").is_file() && !copy.join("b.txt").exists());
    assert_eq!(it["tree"]["base"].as_str(), Some(git(&dir, &["rev-parse", "main"]).trim()), "{it}");
    let holder = it["holder"]["sid"].as_str().unwrap().to_string();
    assert_ne!(holder, h);
    let told = w.hosted.sent().into_iter().filter(|(s, t)| *s == holder && t.contains("moved here from box")).count();
    assert_eq!(told, 1, "its new holder is told where it starts");
}

#[test]
fn the_machine_picked_in_desk_settings_is_saved_to_disk_and_comes_back() {
    let w = World::new();
    let ((_, _, _, project, _, _), _ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put");
    // As the window's Save sends it (ui/lands.js landsSave): the machine and its folder, on their own.
    w.hub().desk_lands_set(&project, &serde_json::json!({ "machine": "box", "machine_folder": "/srv/app" })).unwrap();
    let on_disk = |w: &World| -> serde_json::Value {
        let _ = w;
        serde_json::from_str(&std::fs::read_to_string(crate::server::cue_dir().join("config.json")).unwrap()).unwrap()
    };
    let c = on_disk(&w);
    assert_eq!(c.pointer(&format!("/project/machine/{project}/machine")).and_then(|v| v.as_str()), Some("box"), "{c}");
    let d = super::desk::desk(&w, &project);
    assert_eq!(d["machines"]["machine"].as_str(), Some("box"), "{}", d["machines"]);
    assert_eq!(item(&w, &project, "Footer jumps")["machine"].as_str(), Some("box"));
}
