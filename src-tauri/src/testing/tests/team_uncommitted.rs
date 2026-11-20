//! The Desk's "not committed" tag on a crew member: from git (hub/team/tidy.rs `uncommitted`), never from its state.

use super::team::{git, lead, mate, repo, says, writes_then_stops};
use crate::testing::World;

#[test]
fn a_crew_member_is_marked_uncommitted_only_while_git_says_its_files_have_changes() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "two parts");
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    says(&w, &sid, writes_then_stops("toolu_A", "agentA", "rows", &[f("a.txt"), f("shared.txt")]));
    says(&w, &sid, writes_then_stops("toolu_N", "agentN", "talk", &[]));
    let marked = |name: &str| mate(&w, &sid, name)["uncommitted"].as_bool().unwrap_or(false);
    assert!(!marked("rows"), "not asked yet");
    w.hub().mark_uncommitted();
    assert!(marked("rows"), "its files have changes git hasn't committed");
    assert!(!marked("talk"), "no files: nothing of its to commit");
    // Committed (by hand here; Looks right or its lead the same): the mark goes, though it's still in the crew.
    git(&dir, &["add", "a.txt", "shared.txt"]);
    git(&dir, &["commit", "-q", "-m", "rows by hand"]);
    assert!(w.hub().mark_uncommitted());
    assert!(!marked("rows"));
    assert!(!w.hub().mark_uncommitted(), "nothing changed since");
    // Changed again: marked again.
    std::fs::write(dir.join("shared.txt"), "more").unwrap();
    w.hub().mark_uncommitted();
    assert!(marked("rows"));
}
