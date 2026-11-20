//! An item waiting for a free copy of its repository (desk/hub/landing/trees.rs) starts on Cue's tick once one is free, however
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

#[test]
fn your_send_back_on_an_item_whose_copy_is_gone_waits_with_it_and_reaches_its_holder_word_for_word() {
    // #396: an item that landed has no copy any more. Sent back with every copy in use, it waited for one, and the
    // message with your words was dropped: once a copy was free its holder got only the item's own words (#386).
    let w = World::new();
    let (_, project, id) = super::desk_commit::ready(&w);
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    assert!(item(&w, &project, "Header jumps")["tree"].is_null(), "its copy went as it landed");
    w.hub().desk_hold_copies(&top(&w), &[1, 2, 3]);
    let snap = w.snapshot();
    let h = snap.session_ids().into_iter().find(|s| snap.session(s)["name"] == "cue-teams").unwrap();
    let before = w.hosted.sent().len();
    let words = "Thinking to try A once and keep the mocks. Also, is it possible to show the chat like the threads?";
    w.hub().desk_item_act(&id, "send_back", words).unwrap();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree_wait"].as_bool()), (Some("queued"), Some(true)), "waits for a copy: {it}");
    assert!(!w.hosted.sent()[before..].iter().any(|(s, t)| *s == h && t.contains(&format!("#{id}"))), "nothing sent yet");
    // A second Send back while it waits: both go.
    w.hub().desk_item_act(&id, "send_back", "Lets try A one last time").unwrap();
    w.settle();
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(2)), "{it}");
    let path = it["tree"]["path"].as_str().unwrap().to_string();
    let sent: Vec<String> = w.hosted.sent()[before..].iter().filter(|(s, t)| *s == h && t.contains(&format!("#{id}"))).map(|(_, t)| t.clone()).collect();
    assert_eq!(sent.len(), 1, "one message: {sent:?}");
    let t = &sent[0];
    assert!(t.contains(&format!("isn't done: {words}")) && t.contains("isn't done: Lets try A one last time"), "your words, as you wrote them: {t}");
    assert!(t.contains(&path), "with its copy: {t}");
    assert!(it.get("back_waiting").is_none_or(|b| b.is_null()), "{it}");
}

#[test]
fn a_sent_back_item_waiting_for_a_working_folder_names_who_uses_them_and_start_now_starts_it_at_once() {
    // #401: sent back with every working folder in use, it only said "waiting for a free copy", with no way to go now.
    let w = World::new();
    let (_, project, id) = super::desk_commit::ready(&w);
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    w.hub().desk_hold_copies(&top(&w), &[1, 2, 3]);
    let snap = w.snapshot();
    let h = snap.session_ids().into_iter().find(|s| snap.session(s)["name"] == "cue-teams").unwrap();
    let before = w.hosted.sent().len();
    w.hub().desk_item_act(&id, "send_back", "The dot is too big").unwrap();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree_wait"].as_bool()), (Some("queued"), Some(true)), "{it}");
    assert_eq!(it["copy_users"], serde_json::json!(["held1", "held2", "held3"]), "who uses the working folders: {it}");
    assert!(it["back_waiting"].as_str().is_some_and(|b| b.contains("The dot is too big")), "sent back, and says so: {it}");
    // Start now: at once, where its holder is, with your words; the wait is over.
    w.hub().desk_item_act(&id, "start_now", "").unwrap();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "{it}");
    for gone in ["tree_wait", "copy_users", "back_waiting", "tree"] {
        assert!(it.get(gone).is_none_or(|v| v.is_null()), "{gone}: {it}");
    }
    let sent: Vec<String> = w.hosted.sent()[before..].iter().filter(|(s, t)| *s == h && t.contains(&format!("#{id}"))).map(|(_, t)| t.clone()).collect();
    assert_eq!(sent.len(), 1, "one message: {sent:?}");
    assert!(sent[0].contains("isn't done: The dot is too big"), "{}", sent[0]);
    // A working folder let go of later goes to nobody here: it's already at work.
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working");
}

#[test]
fn a_sent_back_item_waiting_for_a_working_folder_never_waits_on_you() {
    // #419: sent back with every working folder in use, its holder's last turn ending on a question put it in Waiting
    // on you, though your Send back was the answer and goes with it once a folder is free.
    let w = World::new();
    let (_, project, id) = super::desk_commit::ready(&w);
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    w.hub().desk_hold_copies(&top(&w), &[1, 2, 3]);
    let snap = w.snapshot();
    let h = snap.session_ids().into_iter().find(|s| snap.session(s)["name"] == "cue-teams").unwrap();
    w.advance(1000);
    w.hub().desk_item_act(&id, "send_back", "The dot is too big").unwrap();
    w.settle();
    // Its holder, asked about it meanwhile, ends its turn on a question.
    w.advance(1000);
    w.hub().tell(&h, &format!("About #{id} (Header jumps): is the dot the only thing left?")).unwrap();
    w.settle();
    w.advance(1000);
    super::desk_commit::ends(&w, &h, &format!("Looked at #{id} again.\n\nShould the dot blink too?"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree_wait"].as_bool()), (Some("queued"), Some(true)), "still waiting for a folder, not Waiting on you: {it}");
    assert!(it["back_waiting"].as_str().is_some_and(|b| b.contains("The dot is too big")), "{it}");
}

#[test]
fn an_item_queued_after_another_while_it_waits_for_a_copy_never_starts_until_that_one_is_done() {
    // #512: an item waiting for a free copy was queued after another; once a copy was free it started, took the copy and
    // got its brief, the item it waited on not done.
    let w = World::new();
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    let (_, project, a, h) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[1, 2, 3]), |_, _, _| {});
    assert_eq!(item(&w, &project, "Header jumps")["tree_wait"], true, "waits for a copy");
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    super::desk::ends(&w, &lead, "ITEM: bug | Footer moves | done when: it stays put");
    let b = item(&w, &project, "Footer moves")["id"].as_str().unwrap().to_string();
    w.advance(1000);
    super::desk::ends(&w, &lead, &format!("ROUTE: #{a} -> cue-teams | after: #{b} | why: same file"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(b.as_str())), "{it}");
    let before = w.hosted.sent().len();
    let briefed = |w: &World, from: usize| w.hosted.sent()[from..].iter().any(|(s, t)| *s == h && t.contains(&format!("Desk item #{a}")));
    // A copy frees up, by a way that hands it on, then by one that doesn't, and the tick runs.
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    w.hub().desk_let_go_copy_quietly(3);
    w.hub().desk_tick_now();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(b.as_str())), "still waits on #{b}: {it}");
    assert!(it["tree"].is_null(), "took no copy: {it}");
    assert!(!briefed(&w, before), "got no brief: {:?}", &w.hosted.sent()[before..]);
    // #b done (here: dropped): now it starts, in a free copy, with its brief.
    w.hub().desk_item_act(&b, "drop", "").unwrap();
    w.settle();
    w.hub().desk_tick_now();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "{it}");
    let path = it["tree"]["path"].as_str().unwrap_or_else(|| panic!("a copy: {it}")).to_string();
    let sent: Vec<String> = w.hosted.sent()[before..].iter().filter(|(s, t)| *s == h && t.contains(&format!("Desk item #{a}"))).map(|(_, t)| t.clone()).collect();
    assert_eq!(sent.len(), 1, "one brief: {sent:?}");
    assert!(sent[0].contains(&path), "with its copy: {}", sent[0]);
}

#[test]
fn a_free_copy_goes_to_an_item_that_waits_only_for_a_copy_not_to_one_still_queued_after_another() {
    // #512: of two items waiting for a copy, the one queued longest also waits on an item not done (as a Desk saved
    // before #512 loads it, both marks on). The free copy goes to the other.
    let w = World::new();
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    let (_, project, a, h) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[1, 2, 3]), |_, _, _| {});
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    super::desk::ends(&w, &lead, "ITEM: bug | Footer moves | done when: it stays put");
    let b = item(&w, &project, "Footer moves")["id"].as_str().unwrap().to_string();
    w.advance(1000);
    super::desk::ends(&w, &lead, "ITEM: Logo\nROUTE: #new -> cue-teams | why: logo");
    let c = item(&w, &project, "Logo")["id"].as_str().unwrap().to_string();
    assert_eq!(item(&w, &project, "Logo")["tree_wait"], true, "waits for a copy");
    w.hub().desk_queue_after_unchecked(&a, &b);
    assert_eq!(item(&w, &project, "Header jumps")["tree_wait"], true, "both marks on");
    let before = w.hosted.sent().len();
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    w.hub().desk_tick_now();
    w.settle();
    let header = item(&w, &project, "Header jumps");
    assert_eq!(header["state"], "queued", "{header}");
    assert!(header["tree"].is_null(), "{header}");
    let logo = item(&w, &project, "Logo");
    assert_eq!((logo["state"].as_str(), logo["tree"]["slot"].as_u64()), (Some("working"), Some(2)), "{logo}");
    assert!(!w.hosted.sent()[before..].iter().any(|(s, t)| *s == h && t.contains(&format!("Desk item #{a}"))), "no brief for #{a}");
    assert!(w.hosted.sent()[before..].iter().any(|(s, t)| *s == h && t.contains(&format!("Desk item #{c}"))), "a brief for #{c}");
}
