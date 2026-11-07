//! Messages of yours on their way: kept, queued, taken or not, and the delivery record.

use super::*;
use crate::sessions::delivery::DELIVERY_STEPS;

#[test]
fn a_kept_message_that_couldnt_be_sent_says_why_until_it_goes() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.hold("a", "run the tests", vec![]);
    assert!(!s.held("a").unwrap().unsent);
    s.held_failed("a", "that session is gone");
    let h = s.held("a").unwrap();
    assert!(h.unsent);
    assert_eq!(h.delivery.last().map(|d| d.what.as_str()), Some("couldn't send: that session is gone"));
    assert!(s.take_held("a").is_some(), "sent (or taken back to edit): the mark goes with it");
    assert!(s.held("a").is_none());
}

#[test]
fn an_image_sent_without_words_is_settled_by_the_image_in_what_it_got() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.note_sent("a", "", &["/Users/x/cue/uploads/1792-0.png".to_string()], 0, 50);
    s.set_pending("a", "", true);
    assert!(!s.got_message("a", "fix the login page", "its prompt hook"), "another prompt isn't it");
    assert!(s.is_pending("a", ""));
    assert!(s.got_message("a", "[Image: source: /Users/x/cue/uploads/1792-0.png]", "its prompt hook"));
    assert!(!s.is_pending("a", ""));
}

#[test]
fn a_message_cue_is_unsure_about_is_still_settled_after_three_more() {
    let mut s = Sessions::default();
    named(&mut s, "a", "claude", "");
    s.note_sent("a", "check the hook", &[], 0, 50);
    s.set_pending("a", "check the hook", true);
    for m in ["one", "two", "three"] {
        s.note_sent("a", m, &[], now_ms() + 1, 50);
    }
    assert!(s.is_pending("a", "check the hook"), "still found, three messages later");
    assert!(s.unsettled().iter().any(|(_, e)| e.text == "check the hook"));
    assert!(s.got_message("a", "check the hook", "its prompt hook"));
    assert!(!s.is_pending("a", "check the hook"));
    assert!(s.unsettled().is_empty());
}

#[test]
fn a_kept_message_put_back_goes_before_what_was_kept_since() {
    let mut s = Sessions::default();
    s.mark(&Origin { session_id: "s1".into(), ..Default::default() }, "working", None);
    s.hold("s1", "first", vec![]);
    let taken = s.take_held("s1").unwrap();
    s.hold("s1", "second", vec![]);   // kept while the first was being sent
    s.hold_back("s1", &taken.text, taken.images);   // the send failed
    assert_eq!(s.held("s1").unwrap().text, "first\n\nsecond");
    s.take_held("s1");
    s.hold_back("s1", "alone", vec![]);
    assert_eq!(s.held("s1").unwrap().text, "alone");
}

#[test]
fn a_joined_message_waiting_to_be_taken_is_seen_taken_as_the_agent_got_it() {
    let mut s = Sessions::default();
    s.mark(&origin("j"), "working", None);
    s.note("j", "you", "how do I keep the focus in the box", 9);
    s.note("j", "you", "and not when I click elsewhere", 9);
    s.note("j", "you", "how do I keep the focus in the box\n\nand not when I click elsewhere", 9);
    assert!(s.set_pending("j", "and not when I click elsewhere", true));
    // The agent's prompt is both, glued: the bubble holding only what you typed is that one.
    s.mark(&origin("j"), "working", Some("how do I keep the focus in the box\n\nand not when I click elsewhere"));
    assert!(!s.thread("j").last().unwrap().pending, "taken: the ring goes");
}

#[test]
fn a_message_queued_during_a_turn_goes_below_that_turns_reply() {
    let mut s = Sessions::default();
    s.mark(&origin("q"), "working", None);
    s.note("q", "you", "fix the header", 9);
    s.note("q", "you", "also the footer", 9); // sent while it worked on the header
    let q = s.thread("q")[1].clone();
    s.note("q", "agent", "Header fixed.", 9);
    s.below_reply("q", &q);
    let t = s.thread("q");
    let order: Vec<&str> = t.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(order, ["fix the header", "Header fixed.", "also the footer"]);
    assert!(t[2].at_ms > t[1].at_ms, "in time order, so a reload keeps it there");
    s.below_reply("q", &q); // already last: stays
    assert_eq!(s.thread("q").len(), 3);
}

#[test]
fn a_status_ask_goes_on_the_message_it_came_with() {
    let mut s = Sessions::default();
    s.mark(&origin("a"), "waiting", None);
    s.note("a", "you", "fix the bar", 5);
    s.note("a", "agent", "Fixed.", 5);
    s.note("a", "you", "and commit", 5);
    assert!(s.mark_status_ask("a", "fix the bar", "ask"));
    assert!(!s.mark_status_ask("a", "fix the bar", "ask"), "already there: nothing changed");
    assert!(!s.mark_status_ask("a", "a message never sent", "ask"));
    let thread = s.thread("a");
    let asks: Vec<Option<&str>> = thread.iter().map(|e| e.status_ask.as_deref()).collect();
    assert_eq!(asks, [Some("ask"), None, None]);
}

#[test]
fn a_message_that_went_through_late_stops_saying_it_didnt() {
    let mut s = Sessions::default();
    s.mark(&origin("a"), "waiting", None);
    s.note("a", "you", "run the tests", 10);
    s.mark_unsent("a", "run the tests", "still in its box");
    s.mark(&origin("a"), "working", Some("something else"));
    assert!(s.0["a"].thread.last().unwrap().unsent, "a different prompt leaves it flagged");
    s.mark(&origin("a"), "working", Some("run the tests"));
    assert!(!s.0["a"].thread.last().unwrap().unsent);
}

#[test]
fn a_late_message_counts_as_the_agent_got_it() {
    let mut s = Sessions::default();
    s.mark(&origin("a"), "waiting", None);
    s.note("a", "you", "look at this\nscreenshot", 10);
    s.mark_unsent("a", "look at this\nscreenshot", "still in its box");
    // Its line break joined up, an image's path after it: still the same message.
    s.mark(&origin("a"), "working", Some("look at this screenshot /Users/me/cue/uploads/1.png"));
    assert!(!s.0["a"].thread.last().unwrap().unsent);
}

/// The case that showed the flaw: a long paste sat in the agent's box past Cue's wait, the bubble
/// said it didn't go, and the prompt hook that then reported it (the text wrapped as a paste, with
/// the session marked working with no prompt) left the bubble as it was.
#[test]
fn the_prompt_hook_settles_a_message_said_not_to_have_gone_however_late() {
    let mut s = Sessions::default();
    s.mark(&origin("a"), "waiting", None);
    let long = format!("you can make the change, but after that\n\ncheck why {}\n\nWant me to do that now?", "the message did not show ".repeat(20));
    s.note("a", "you", &long, 10);
    s.set_pending("a", &long, true);
    s.mark_unsent("a", &long, "still in its box after 30s");
    let e = s.thread("a").pop().unwrap();
    assert!(e.unsent && !e.pending);
    assert_eq!(e.delivery.last().map(|d| d.what.as_str()), Some("not taken: still in its box after 30s"));
    // The hook path: working with no prompt, then the prompt as Claude Code hands it over.
    s.mark(&origin("a"), "working", None);
    let wrapped = format!("\n\n<pasted_content id=\"673a\">\n{}\n</pasted_content id=\"673a\">\n", long.replace("\n\n", "\r\n\r\n"));
    assert!(s.got_message("a", &wrapped, "its prompt hook"));
    let e = s.thread("a").pop().unwrap();
    assert!(!e.unsent && !e.pending, "taken, whoever said so and however late");
    assert_eq!(e.delivery.last().map(|d| d.what.as_str()), Some("taken: its prompt hook"));
    assert!(!s.got_message("a", &wrapped, "its prompt hook"), "nothing left to settle");
    assert!(!s.got_message("a", "a different message altogether", "its prompt hook"));
}

#[test]
fn a_verdict_lands_on_the_message_it_is_about_not_the_newest() {
    let mut s = Sessions::default();
    s.mark(&origin("a"), "waiting", None);
    s.note("a", "you", "first: run the tests", 10);
    s.set_pending("a", "first: run the tests", true);
    s.note("a", "you", "second: and commit", 10);
    s.set_pending("a", "second: and commit", true);
    s.mark_unsent("a", "first: run the tests", "still in its box");
    let t = s.thread("a");
    assert!(t[0].unsent && !t[0].pending, "the first has its verdict");
    assert!(!t[1].unsent && t[1].pending, "the second is still being watched");
    assert!(s.is_pending("a", "second: and commit") && !s.is_pending("a", "first: run the tests"));
    assert!(s.got_message("a", "second: and commit", "its prompt hook"));
    assert!(t[0].unsent, "the second's witness says nothing about the first");
    // A long message cut short in the thread, or with its spacing changed, is still found.
    let long = "word ".repeat(200);
    s.note("a", "you", &long, 10);
    assert!(s.set_pending("a", &long.replace(' ', "\n"), true));
    let open = s.unsettled();
    assert_eq!(open.len(), 2, "the first (not taken) and the long one (pending): {open:?}");
    assert!(open.iter().all(|(o, _)| o.session_id == "a"));
}

#[test]
fn a_delivery_record_keeps_only_its_last_steps() {
    let mut s = Sessions::default();
    s.mark(&origin("a"), "waiting", None);
    s.note("a", "you", "go", 10);
    for n in 0..12 {
        s.note_delivery("a", "go", &format!("step {n}"));
    }
    let d = s.thread("a").pop().unwrap().delivery;
    assert_eq!(d.len(), DELIVERY_STEPS);
    assert_eq!(d.last().unwrap().what, "step 11");
    s.note_delivery("a", "nothing like it", "lost");
    assert_eq!(s.thread("a").pop().unwrap().delivery.len(), DELIVERY_STEPS, "a step for a message that isn't there goes nowhere");
}
