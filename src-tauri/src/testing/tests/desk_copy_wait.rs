//! An item waiting for a free copy of its repository (hub/desk_trees.rs) starts on Cue's tick once one is free, however
//! the copy was let go of, and the Desk lead's summary says what it waits on (#340).

use super::desk_commit::{item, routed_with};
use crate::testing::World;

#[test]
fn an_item_waiting_for_a_copy_starts_on_the_tick_when_a_copy_was_let_go_of_without_handing_it_on() {
    // #340: a copy let go of by a way that doesn't hand it on left the item queued for good, and its lead saw
    // nothing it waited on.
    let w = World::new();
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    let (_, project, id, h) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[1, 2, 3]), |_, _, _| {});
    let summary = w.hub().desk_summary(&project, "", false).0;
    let line = summary.lines().find(|l| l.starts_with(&format!("- #{id} "))).unwrap_or_else(|| panic!("{summary}"));
    assert!(line.contains("| queued") && line.contains("waiting for a free copy of its repository (3 of 3 held, by #held1, #held2, #held3)"), "{line}");
    w.hub().desk_let_go_copy_quietly(2);
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "queued", "nothing handed the copy on");
    w.hub().desk_tick_now();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(2)), "{it}");
    let path = it["tree"]["path"].as_str().unwrap().to_string();
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(&format!("Desk item #{id}")) && t.contains(&path)), "sent now, with its copy");
    let summary = w.hub().desk_summary(&project, "", false).0;
    assert!(!summary.contains("waiting for a free copy"), "{summary}");
}
