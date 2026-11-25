//! An item reaches your check only with its work on main (#830): its session said DONE with its change left
//! uncommitted (written by a shell command, so Cue never saw the file change), and the item came to check with
//! nothing on main. Now it goes back to its session to commit its own lines; with a commit naming it, it goes on.

use super::desk_commit::{ends, item, routed_with};
use super::team::git;
use crate::desk::hub::landing::committed::NO_COMMIT;
use crate::testing::World;

#[test]
fn a_done_with_its_change_left_uncommitted_goes_back_to_its_session_and_on_once_it_commits_it() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, dir| std::fs::write(dir.join("b.txt"), "fixed by a shell command\n").unwrap());
    let before = git(&dir, &["rev-parse", "HEAD"]);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "never to check with nothing on main: {it}");
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), before, "nothing committed for it");
    let told = w.hosted.sent().into_iter().filter(|(s, _)| *s == h).map(|(_, t)| t).find(|t| t.contains(NO_COMMIT));
    let told = told.unwrap_or_else(|| panic!("its session is told: {:?}", w.hosted.sent()));
    assert!(told.contains(&format!("Desk item #{id}")) && told.contains(&format!("with #{id} in the commit's message")), "{told}");
    // Its session commits its own change, naming the item: on main now, so its DONE goes on to check.
    git(&dir, &["add", "b.txt"]);
    git(&dir, &["commit", "-q", "-m", &format!("Header stays put (#{id})")]);
    w.advance(1000);
    ends(&w, &h, &format!("Committed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
}

#[test]
fn a_done_whose_done_when_says_the_repo_doesnt_change_goes_to_check_with_no_commit() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    w.hub().write(|st| st.desk.item_mut(&id).unwrap().done_when = "a plan on the canvas; nothing in the repo changes".into());
    ends(&w, &h, &format!("The plan is on the canvas.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert!(!w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(NO_COMMIT)), "{:?}", w.hosted.sent());
}

#[test]
fn a_second_done_with_still_nothing_on_main_goes_to_check_as_it_is() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    super::desk_commit::sent_back_then_done_again(&w, &project, &id, &h);
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
}

/// #810: its first holder changed the file, then the item went to another session, which changed nothing itself and
/// said DONE. The first holder's change is the item's: committed with it, so it reaches your check with it on main.
#[test]
fn a_change_its_former_holder_made_is_committed_at_the_next_holders_done() {
    use super::desk::desk;
    use super::desk_commit::log;
    use crate::hosted::HostedEvent;
    use crate::ipc::{start_session_with, StartOpts};
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |w, h, dir| {
        let a = dir.join("a.txt");
        std::fs::write(&a, "the first holder wrote this\n").unwrap();
        log(w, h).tool_use("toolu_w1", "Write", serde_json::json!({ "file_path": a, "content": "the first holder wrote this\n" }));
        w.settle();
    });
    let d = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    ends(&w, &h, "Over to someone else.");
    let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &dir.to_string_lossy(), "hello", "checker", &opts).unwrap();
    let b = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&b, HostedEvent::Taken { text: "hello".into() });
    ends(&w, &b, "Hi.");
    w.hub().desk_set_member(&project, &b, true).unwrap();
    ends(&w, &d, &format!("ROUTE: #{id} -> checker | why: a second look"));
    assert_eq!(item(&w, &project, "Header jumps")["holder"]["sid"].as_str(), Some(b.as_str()));
    w.advance(1000);
    ends(&w, &b, &format!("Checked it: it's right.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "the first holder wrote this\n", "the first holder's change is on main");
    assert_eq!(git(&dir, &["status", "--porcelain", "--", "a.txt"]).trim(), "", "nothing of it left uncommitted");
}
