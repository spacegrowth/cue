//! Desk spec (#498): asks for you. The lead's ASK and a session's question wait on you, are answered once, and the
//! answer reaches whoever asked.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::json;

/// The lead's ASK card: its id.
fn lead_card(w: &World, lead: &str) -> String {
    let snap = w.snapshot();
    let card = snap.card(lead);
    assert_eq!(card["desk"], true, "{card}");
    card["id"].as_str().unwrap().to_string()
}

/// "Waiting on you" is answered once (desk.rs `desk_answered`): your answer to the lead's ASK reaches it once with
/// its question, and answering the same card again (a second click, or another window) sends nothing more.
#[test]
fn the_leads_ask_is_answered_once_and_the_answer_reaches_it_once() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ASK: Header or footer first? | choices: Header / Footer");
    let id = lead_card(&w, &l);
    let words = "Your ASK: Header or footer first?\nThe answer: Footer";
    w.answer(&id, json!({ "behavior": "allow", "answers": { "Header or footer first?": "Footer" } }));
    w.settle();
    w.answer(&id, json!({ "behavior": "allow", "answers": { "Header or footer first?": "Header" } }));
    w.settle();
    let sent = sent_to(&w, &l);
    assert_eq!(sent.iter().filter(|t| t.as_str() == words).count(), 1, "{sent:?}");
    assert!(!sent.iter().any(|t| t.contains("The answer: Header")), "a second answer isn't sent: {sent:?}");
    assert!(w.snapshot().cards_of(&l).is_empty());
}

/// The answer reaches who asked: the lead's ASK answered while the lead is busy on a turn goes in once that turn
/// ends, not lost.
#[test]
fn the_answer_to_the_leads_ask_reaches_it_after_a_busy_turn() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ASK: Header or footer first? | choices: Header / Footer");
    let id = lead_card(&w, &l);
    w.advance(1000);
    w.hosted_says(&l, HostedEvent::Taken { text: "Something else".into() });
    w.settle();
    w.answer(&id, json!({ "behavior": "allow", "answers": { "Header or footer first?": "Header" } }));
    w.settle();
    w.advance(1000);
    ends(&w, &l, "Done with that.");
    w.advance(1000);
    w.settle();
    let sent = sent_to(&w, &l);
    assert_eq!(sent.iter().filter(|t| t.contains("Your ASK: Header or footer first?\nThe answer: Header")).count(), 1, "{sent:?}");
}

/// #419 done when ("the item stays on Waiting on you after you ask for changes" fixed) and #475: a session's question
/// answered with a note on its item's page reaches the session and takes the item off Waiting on you.
#[test]
fn a_note_answering_a_holders_question_reaches_it_and_takes_the_item_off_waiting_on_you() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    w.advance(1000);
    ends(&w, &h, "I moved the header.\n\nShould the footer move too?");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "needs_you");
    w.advance(1000);
    w.hub().desk_item_act(&id, "note", "Yes, the footer too").unwrap();
    w.settle();
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("About #{id} (Header): Yes, the footer too"))), "{:?}", sent_to(&w, &h));
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!(it["state"], "working", "off Waiting on you: {it}");
    assert!(it["ask"].is_null() || it["ask"]["text"] != "Should the footer move too?", "its question is answered: {it}");
}

/// desk_stall.rs / #182: a holder's turn on its item that ends with no DONE asks it "Done?" once, naming the item; a
/// question it asks you instead is never met with "Done?", and the lead isn't told it stopped.
#[test]
fn a_holder_that_asks_you_is_not_asked_done_and_the_lead_is_not_told_it_stopped() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    for _ in 0..2 {
        w.advance(1000);
        w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
        w.advance(1000);
        ends(&w, &h, "Which colour do you want for it: blue or grey?");
    }
    assert!(!sent_to(&w, &h).iter().any(|t| t.starts_with("Done? Write DONE")), "{:?}", sent_to(&w, &h));
    assert!(!sent_to(&w, &l).iter().any(|t| t.contains(&format!("#{id} (Header) stopped"))), "{:?}", sent_to(&w, &l));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "needs_you");
}
