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
