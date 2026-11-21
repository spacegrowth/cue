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
    let (_, project, a, _) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[1, 2, 3]), |_, _, _| {});
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    super::desk::ends(&w, &lead, "ITEM: Footer\nROUTE: #new -> cue-teams | why: footer");
    w.advance(1000);
    // cue-teams holds two open items, the most a session holds: Logo goes to another session.
    let t = super::desk::second(&w, "cue-two");
    super::desk::ends(&w, &lead, "ITEM: Logo\nROUTE: #new -> cue-two | why: logo");
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
    assert!(w.hosted.sent().iter().any(|(s, m)| *s == t && m.contains(&format!("Desk item #{c}"))));
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
fn your_send_back_on_an_item_whose_copy_is_gone_reaches_its_holder_and_lead_at_once_and_again_with_its_copy() {
    // #396: an item that landed has no copy any more. Sent back with every copy in use, it waited for one, and the
    // message with your words was dropped: once a copy was free its holder got only the item's own words (#386).
    // #567: and until then your words sat unseen: they go to its holder and its lead at once now.
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
    let to_holder = |w: &World, from: usize| -> Vec<String> { w.hosted.sent()[from..].iter().filter(|(s, t)| *s == h && t.contains(&format!("#{id}"))).map(|(_, t)| t.clone()).collect() };
    let now = to_holder(&w, before);
    assert!(now.len() == 1 && now[0].contains(words) && now[0].contains("Don't change any files until Cue sends you where its copy is"), "your words now, told to wait for its copy: {now:?}");
    let lead = super::desk_finish::lead_of(&w, &project);
    let heard: Vec<String> = w.hosted.sent().into_iter().filter(|(s, t)| *s == lead && t.contains("(Desk changes ") && t.contains(words)).map(|(_, t)| t).collect();
    assert!(heard.iter().any(|t| t.contains("It waits for a free copy")), "the lead hears your words and that it waits: {heard:?}");
    // A second Send back while it waits: it goes now too, and both go again with its copy.
    w.hub().desk_item_act(&id, "send_back", "Lets try A one last time").unwrap();
    w.settle();
    let now = to_holder(&w, before);
    assert!(now.len() == 2 && now[1].contains("Lets try A one last time"), "{now:?}");
    let mid = w.hosted.sent().len();
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(2)), "{it}");
    let path = it["tree"]["path"].as_str().unwrap().to_string();
    let sent = to_holder(&w, mid);
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
    // Your words went at once (#567); Start now sends them again, with where to work.
    assert!(w.hosted.sent()[before..].iter().any(|(s, t)| *s == h && t.contains("The dot is too big")), "your words, now");
    let before = w.hosted.sent().len();
    // Start now: at once, where its holder is, with your words; the wait is over.
    w.hub().desk_item_act(&id, "start_now", "").unwrap();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "{it}");
    for gone in ["copy_users", "back_waiting", "tree"] {
        assert!(it.get(gone).is_none_or(|v| v.is_null()), "{gone}: {it}");
    }
    // In line for the next free one all the same (#566).
    assert_eq!(it["tree_wait"], true, "{it}");
    let sent: Vec<String> = w.hosted.sent()[before..].iter().filter(|(s, t)| *s == h && t.contains(&format!("#{id}"))).map(|(_, t)| t.clone()).collect();
    assert_eq!(sent.len(), 1, "one message: {sent:?}");
    assert!(sent[0].contains("isn't done: The dot is too big"), "{}", sent[0]);
    // A working folder let go of later is its (#566): it stays at work, now in its own copy.
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(2)), "{it}");
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

/// #566 (#542's case): the lead STARTs an item while every copy is in use. It works where its holder is for now, in
/// line for the next free copy, and gets it once one is let go of, its holder told where it is.
#[test]
fn an_item_started_with_every_copy_in_use_gets_the_next_free_copy() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&super::desk_unstick::top(w), &[1, 2, 3]), |_, _, _| {});
    let repo = super::desk_unstick::top(&w);
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    super::desk_commit::ends(&w, &lead, &format!("START: #{id} | why: the user needs it now"));
    w.advance(1000);
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"].is_null(), it["tree_wait"].as_bool()), (Some("working"), true, Some(true)), "{it}");
    let summary = w.hub().desk_summary(&project, "", false).0;
    let line = summary.lines().find(|l| l.starts_with(&format!("- #{id} "))).unwrap_or_else(|| panic!("{summary}"));
    assert!(line.contains("in line for the next free copy"), "{line}");
    let before = w.hosted.sent().len();
    w.hub().desk_let_go_copy(&repo, 2);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64(), it["tree_wait"].as_bool().unwrap_or(false)), (Some("working"), Some(2), false), "{it}");
    let path = it["tree"]["path"].as_str().unwrap().to_string();
    let told: Vec<String> = w.hosted.sent().into_iter().skip(before).filter(|(s, _)| *s == h).map(|(_, t)| t).collect();
    assert!(told.iter().any(|t| t.contains(&format!("Desk item #{id}")) && t.contains("it's yours") && t.contains(&path)), "{told:?}");
    assert!(std::path::Path::new(&path).is_dir(), "its copy is made");
}

/// #566: an item an older Cue saved at work without a copy, not in line for one (#542 as it was), is put in line on
/// the tick and gets the next free copy.
#[test]
fn an_item_saved_at_work_without_a_copy_is_put_in_line_and_gets_the_next_free_one() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&super::desk_unstick::top(w), &[1, 2, 3]), |_, _, _| {});
    let repo = super::desk_unstick::top(&w);
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    super::desk_commit::ends(&w, &lead, &format!("START: #{id} | why: the user needs it now"));
    w.advance(1000);
    w.hub().desk_out_of_copy_line(&id);
    // A copy let go of with nothing in line for it: stays free.
    w.hub().desk_let_go_copy(&repo, 3);
    w.settle();
    assert!(item(&w, &project, "Header jumps")["tree"].is_null(), "not in line yet");
    w.hub().desk_tick_now();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(3)), "{it}");
    let path = it["tree"]["path"].as_str().unwrap().to_string();
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains("it's yours") && t.contains(&path)), "its holder is told where");
    // Nothing more on later ticks.
    let n = w.hosted.sent().len();
    w.hub().desk_tick_now();
    w.settle();
    assert_eq!(w.hosted.sent().len(), n);
}

#[test]
fn your_question_on_an_item_whose_copy_is_gone_reaches_its_holder_and_lead_at_once_and_an_answer_puts_it_back() {
    // #567 (#501's case): a question on an item to check, its copy gone as it landed, every copy in use. The item went
    // to the line for a copy and the question was dropped: its holder never got it, nor did the lead.
    let w = World::new();
    let (_, project, id) = super::desk_commit::ready(&w);
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    w.hub().desk_hold_copies(&top(&w), &[1, 2, 3]);
    let snap = w.snapshot();
    let h = snap.session_ids().into_iter().find(|s| snap.session(s)["name"] == "cue-teams").unwrap();
    let lead = super::desk_finish::lead_of(&w, &project);
    let before = w.hosted.sent().len();
    let question = "Wondering why Feature #542 stopped. Was it waiting for a copy?";
    w.hub().desk_item_act(&id, "note", question).unwrap();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree_wait"].as_bool()), (Some("queued"), Some(true)), "waits for a copy: {it}");
    let sent: Vec<String> = w.hosted.sent()[before..].iter().filter(|(s, _)| *s == h).map(|(_, t)| t.clone()).collect();
    assert!(
        sent.len() == 1 && sent[0].starts_with(&format!("A question about Desk item #{id}")) && sent[0].contains(question) && sent[0].contains("Don't change any files until Cue sends you where its copy is"),
        "your question, now: {sent:?}"
    );
    let heard: Vec<String> = w.hosted.sent()[before..].iter().filter(|(s, t)| *s == lead && t.contains("(Desk changes ")).map(|(_, t)| t.clone()).collect();
    assert!(heard.iter().any(|t| t.contains(question) && t.contains("It waits for a free copy")), "the lead hears it at once: {heard:?}");
    // Its holder answers without changing code: back to check, off the line for a copy.
    w.advance(1000);
    w.hosted_says(&h, crate::hosted::HostedEvent::Taken { text: question.into() });
    w.settle();
    w.advance(1000);
    super::desk_commit::ends(&w, &h, "It stopped because every copy was in use; it starts once one frees.");
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree_wait"].as_bool().unwrap_or(false)), (Some("to_check"), false), "back where it was: {it}");
    assert!(it["asked"].is_null(), "{it}");
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check", "a freed copy doesn't start it");
}

#[test]
fn the_lead_moves_an_item_to_the_front_of_the_line_for_a_free_copy_and_it_gets_the_next_one() {
    // #572: the lead had no line to move a queued item up the line for a free copy; only your drag could.
    let w = World::new();
    let top = super::desk_unstick::top;
    let (_, project, a, _) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[1, 2, 3]), |_, _, _| {});
    let lead = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    super::desk::ends(&w, &lead, "ITEM: Footer\nROUTE: #new -> cue-teams | why: footer");
    w.advance(1000);
    let t = super::desk::second(&w, "cue-two");
    super::desk::ends(&w, &lead, "ITEM: Logo\nROUTE: #new -> cue-two | why: logo");
    w.advance(1000);
    let b = item(&w, &project, "Footer")["id"].as_str().unwrap().to_string();
    let c = item(&w, &project, "Logo")["id"].as_str().unwrap().to_string();
    // You placed Footer above Header; Logo, the newest, is last in line.
    w.hub().desk_item_act(&b, "reorder", &format!("{b} {a}")).unwrap();
    w.settle();
    super::desk::ends(&w, &lead, &format!("COPY FIRST: #{c} | why: the user needs the logo today"));
    w.advance(1000);
    w.settle();
    // The Desk shows the new order: Logo, then yours as you left them.
    let order = |t: &str| item(&w, &project, t)["queue_order"].as_u64();
    assert_eq!((order("Logo"), order("Footer"), order("Header jumps")), (Some(1), Some(2), Some(3)));
    let d = super::desk::desk(&w, &project);
    assert!(
        d["decisions"].as_array().unwrap().iter().any(|s| s["item_id"] == c.as_str() && s["act"] == "copy_first" && s["text"] == format!("Moved #{c} to the front of the line for a free copy")),
        "{}",
        d["decisions"]
    );
    // The next copy that frees goes to it.
    w.hub().desk_let_go_copy(&top(&w), 2);
    w.settle();
    let logo = item(&w, &project, "Logo");
    assert_eq!((logo["state"].as_str(), logo["tree"]["slot"].as_u64()), (Some("working"), Some(2)), "{logo}");
    assert_eq!(item(&w, &project, "Footer")["state"], "queued");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "queued");
    assert!(w.hosted.sent().iter().any(|(s, m)| *s == t && m.contains(&format!("Desk item #{c}"))));
    // One at work isn't in that line: refused, said plainly.
    super::desk::ends(&w, &lead, &format!("COPY FIRST: #{c} | why: again"));
    w.advance(1000);
    w.settle();
    let told: Vec<String> = w.hosted.sent().into_iter().filter(|(s, _)| *s == lead).map(|(_, m)| m).collect();
    assert!(told.iter().any(|m| m.contains(&format!("#{c} not moved up: it isn't waiting for a free copy"))), "{told:?}");
}
