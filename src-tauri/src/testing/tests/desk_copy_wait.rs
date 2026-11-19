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

#[test]
fn a_dragged_order_picks_who_gets_the_next_free_copy_and_survives_a_restart() {
    // #387: you drag queued items up and down; the next free copy goes to the top one. One can't go above the item
    // it waits on; one at work can't move.
    let mut w = World::new();
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    let (_, project, a, h) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[1, 2, 3]), |_, _, _| {});
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    super::desk::ends(&w, &lead, "ITEM: Footer\nROUTE: #new -> cue-teams | why: footer");
    w.advance(1000);
    super::desk::ends(&w, &lead, "ITEM: Logo\nROUTE: #new -> cue-teams | why: logo");
    let b = item(&w, &project, "Footer")["id"].as_str().unwrap().to_string();
    let c = item(&w, &project, "Logo")["id"].as_str().unwrap().to_string();
    for t in ["Header jumps", "Footer", "Logo"] {
        assert_eq!(item(&w, &project, t)["tree_wait"], true, "{t} waits for a copy");
    }
    // Logo to the top: Logo, Header, Footer.
    w.hub().desk_item_act(&c, "reorder", &format!("{c} {a} {b}")).unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Logo")["queue_order"], 1);
    // A restart keeps the order.
    w.relaunch();
    // (The stand-in items holding the copies aren't saved: held again.)
    w.hub().desk_hold_copies(&top(&w), &[1, 2, 3]);
    assert_eq!(item(&w, &project, "Logo")["queue_order"], 1);
    assert_eq!(item(&w, &project, "Footer")["queue_order"], 3);
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    assert_eq!(item(&w, &project, "Logo")["state"], "working", "the top one gets the free copy");
    assert_eq!(item(&w, &project, "Logo")["tree"]["slot"], 2, "{}", item(&w, &project, "Logo"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "queued");
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(&format!("Desk item #{c}"))));
    // Logo is at work now: it can't move.
    let e = w.hub().desk_item_act(&a, "reorder", &format!("{a} {c} {b}")).unwrap_err();
    assert_eq!(e, format!("#{c} has started, so it can't move"));
    // Footer waits on Header: it can't go above it, and nothing moves.
    super::desk::ends(&w, &lead, &format!("ROUTE: #{b} -> cue-teams | after: #{a} | why: same file"));
    assert_eq!(item(&w, &project, "Footer")["after"], a.as_str(), "{}", item(&w, &project, "Header jumps"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "queued", "{}", item(&w, &project, "Header jumps"));
    let e = w.hub().desk_item_act(&b, "reorder", &format!("{b} {a}")).unwrap_err();
    assert_eq!(e, format!("#{b} waits on #{a}, so it can't go above it"));
    assert_eq!(item(&w, &project, "Header jumps")["queue_order"], 2);
    // Below it is fine.
    w.hub().desk_item_act(&a, "reorder", &format!("{a} {b}")).unwrap();
    assert_eq!(item(&w, &project, "Footer")["queue_order"], 2);
}
