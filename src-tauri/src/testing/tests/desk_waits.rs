//! Waiting isn't stopping (#249): a holder's turn that ends saying it waits on a build, tests, a helper or another
//! commit shows its item as waiting on that, isn't asked "Done?", and doesn't send the Desk lead a stop notice. The lead
//! hears of an item's stop once at most; a later stop of it waits on you at once.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::hosted::HostedEvent;
use crate::testing::World;

/// A holder at work on "Header" for the lead `l`, and the item's id; `turn` gives it a prompt and ends its turn.
fn routed(w: &World) -> (String, String, String, String) {
    let (h, project) = holder(w);
    let l = new_lead(w, &project);
    ends(w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(w, &project), "Header")["id"].as_str().unwrap().to_string();
    (h, project, l, id)
}

fn turn(w: &World, h: &str, prompt: &str, reply: &str) {
    w.advance(1000);
    w.hosted_says(h, HostedEvent::Taken { text: prompt.into() });
    w.settle();
    w.advance(1000);
    ends(w, h, reply);
}

#[test]
fn a_turn_that_waits_on_a_build_tests_a_helper_or_a_commit_is_waiting_not_stopped() {
    for (reply, line) in [
        ("Started the build.\nThe build for 57d72d4e has started.", "The build for 57d72d4e has started."),
        ("Fixed the header.\n\nI'm waiting on the test run.", "I'm waiting on the test run."),
        ("Most of it is in.\n- still compiling", "still compiling"),
        ("Handed the footer over.\nWaiting for the helper to finish its part.", "Waiting for the helper to finish its part."),
        ("Ready.\n**Blocked on** cue-161 committing desk.rs.", "Blocked on cue-161 committing desk.rs."),
    ] {
        let w = World::new();
        let (h, project, l, _) = routed(&w);
        turn(&w, &h, "Header", reply);
        let it = item(&desk(&w, &project), "Header").clone();
        assert_eq!((it["state"].as_str(), it["stopped"].as_bool(), it["waiting"].as_str(), it["latest"].as_str()), (Some("working"), None, Some(line), Some(line)), "{reply}: {it}");
        assert!(sent_to(&w, &h).iter().all(|t| !t.starts_with("Done? Write DONE")), "{reply}: not asked");
        assert!(sent_to(&w, &l).iter().all(|t| !t.contains(") stopped")), "{reply}: no notice to the lead: {:?}", sent_to(&w, &l));
        // Its time to settle passing changes nothing: it still waits, and nothing waits on you.
        w.advance(11 * 60_000);
        assert_eq!(item(&desk(&w, &project), "Header")["state"], "working", "{reply}");
    }
}

#[test]
fn a_plain_line_with_a_build_word_is_still_a_stop() {
    let w = World::new();
    let (h, project, _, id) = routed(&w);
    turn(&w, &h, "Header", "The build passed and the header is in.");
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["stopped"].as_bool(), it["waiting"].as_str()), (Some(true), None), "{it}");
    assert_eq!(sent_to(&w, &h).iter().filter(|t| **t == format!("Done? Write DONE: #{id}, or say what's left")).count(), 1);
}

#[test]
fn the_lead_hears_of_an_items_stop_once_and_a_later_stop_waits_on_you() {
    let w = World::new();
    let (h, project, l, id) = routed(&w);
    let told = || sent_to(&w, &l).iter().filter(|t| t.starts_with(&format!("#{id} (Header) stopped"))).count();
    let words = format!("Done? Write DONE: #{id}, or say what's left");
    turn(&w, &h, "Header", "Looked at it.");
    turn(&w, &h, &words, "The footer still overlaps it.");
    assert_eq!(told(), 1);
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool()), (Some("working"), Some(true)), "{it}");
    // The lead sends it back; it stops again, and again after Cue's ask: no second notice, it's yours now.
    ends(&w, &l, &format!("ROUTE: #{id} -> {h} | why: fix the footer"));
    turn(&w, &h, "Fix the footer", "Moved things around.");
    turn(&w, &h, &words, "Still not right.");
    assert_eq!(told(), 1, "{:?}", sent_to(&w, &l));
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["latest"].as_str()), (Some("needs_you"), Some("It stopped again after the Desk lead had it once: say what's next")), "{it}");
    // Many turns in a row: still one.
    ends(&w, &l, &format!("ROUTE: #{id} -> {h} | why: once more"));
    for n in 0..4 {
        turn(&w, &h, "Go on", &format!("Try {n}."));
    }
    assert_eq!(told(), 1);
}
