//! An item to check goes back to work when there's more to do on it (#627): its holder says NOT DONE, a route back to
//! it while it answers your question isn't undone by that answer, and a message kept for a busy lead reaches it with
//! its Desk's summary as it is then, not as it was when kept.

use super::desk::{desk, sent_to};
use super::desk_asked::to_check;
use super::desk_commit::{ends, item};
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;

fn lead_of(w: &World, project: &str) -> String {
    desk(w, project)["lead"]["sid"].as_str().unwrap().to_string()
}

#[test]
fn not_done_lines_are_read_with_or_without_what_is_left() {
    assert_eq!(
        lines("NOT DONE: #627 | the footer still jumps\n- **NOT DONE:** #a1\nNOT DONE: nothing"),
        vec![
            Line::NotDone { id: "627".into(), text: "the footer still jumps".into() },
            Line::NotDone { id: "a1".into(), text: String::new() },
        ]
    );
}

#[test]
fn its_holder_saying_not_done_puts_an_item_to_check_back_at_work() {
    let w = World::new();
    let (_, project, id, h) = to_check(&w);
    // Your note on its page tells it how to say so.
    w.hub().desk_item_act(&id, "note", "Go ahead with the footer too").unwrap();
    w.settle();
    let note = sent_to(&w, &h).into_iter().find(|t| t.starts_with(&format!("About #{id}"))).expect("note sent");
    assert!(note.contains(&format!("NOT DONE: #{id} | <what's left>")), "{note}");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check", "a note alone doesn't reopen it");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: note });
    w.settle();
    w.advance(1000);
    ends(&w, &h, &format!("On it.\nNOT DONE: #{id} | the footer still jumps"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "{it}");
    assert_eq!(it["checks"].as_array().and_then(|c| c.last()).map(|c| c["back"].clone()), Some(serde_json::json!("cue-teams: the footer still jumps")), "{it}");
    // #779: under its holder, who said so, not under you.
    assert_eq!(it["checks"].as_array().and_then(|c| c.last()).map(|c| c["back_by"].clone()), Some(serde_json::json!("cue-teams")), "{it}");
    // Said again on an item at work: nothing more changes.
    ends(&w, &h, &format!("NOT DONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working");
}

#[test]
fn another_sessions_not_done_leaves_the_item_to_check() {
    let w = World::new();
    let (_, project, id, _) = to_check(&w);
    let lead = lead_of(&w, &project);
    ends(&w, &lead, &format!("NOT DONE: #{id} | more"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
}

/// #595: the lead sent the item back while its holder answered your question; the answer's end put it back to check.
#[test]
fn a_route_back_while_its_holder_answers_your_question_stays() {
    let w = World::new();
    let (_, project, id, h) = to_check(&w);
    let lead = lead_of(&w, &project);
    w.hub().desk_item_act(&id, "note", "Is the footer done too?").unwrap();
    w.settle();
    let q = sent_to(&w, &h).into_iter().find(|t| t.starts_with(&format!("A question about Desk item #{id}"))).expect("question sent");
    assert!(q.contains(&format!("NOT DONE: #{id}")), "{q}");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: q });
    w.settle();
    ends(&w, &lead, &format!("ROUTE: #{id} -> cue-teams | why: the footer is left"));
    assert_eq!(item(&w, &project, "Header jumps")["asked"], serde_json::Value::Null);
    w.advance(1000);
    ends(&w, &h, "Not yet: the footer still needs the same fix.");
    let it = item(&w, &project, "Header jumps");
    assert_ne!(it["state"], "to_check", "the send back held: {it}");
}

/// #721: your Send back while its holder answers your question on it (a window that hadn't redrawn yet) holds too.
#[test]
fn your_send_back_while_its_holder_answers_your_question_stays() {
    let w = World::new();
    let (_, project, id, h) = to_check(&w);
    w.hub().desk_item_act(&id, "note", "Is the footer done too?").unwrap();
    w.settle();
    let q = sent_to(&w, &h).into_iter().find(|t| t.starts_with(&format!("A question about Desk item #{id}"))).expect("question sent");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: q });
    w.settle();
    w.hub().desk_item_act(&id, "send_back", "The footer still jumps").unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["asked"], serde_json::Value::Null);
    w.advance(1000);
    ends(&w, &h, "Not yet: the footer still needs the same fix.");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "working", "the send back held: {it}");
}

/// #595: a message kept for the lead while it worked went with the summary from when it was kept, so the lead read its
/// own send back as not taken.
#[test]
fn a_message_kept_for_a_busy_lead_goes_with_the_desk_as_it_is_now() {
    let w = World::new();
    let (_, project, id, h) = to_check(&w);
    let lead = lead_of(&w, &project);
    w.advance(1000);
    w.hosted_says(&lead, HostedEvent::Taken { text: "look at the desk".into() });
    w.settle();
    ends(&w, &h, &format!("TO LEAD: #{id} | it isn't ready to check: the footer is left"));
    let kept = w.snapshot().session(&lead)["held"]["text"].as_str().unwrap_or_default().to_string();
    assert!(kept.starts_with("<desk") && kept.contains("isn't ready to check"), "kept for the lead: {kept}");
    let before = sent_to(&w, &lead).len();
    w.advance(1000);
    ends(&w, &lead, &format!("ROUTE: #{id} -> cue-teams | why: the footer is left"));
    assert_ne!(item(&w, &project, "Header jumps")["state"], "to_check");
    let got = sent_to(&w, &lead)[before..].iter().find(|t| t.contains("isn't ready to check")).cloned().expect("delivered at the turn's end");
    assert_eq!(got.matches("<desk").count(), 1, "one summary: {got}");
    let (block, _) = got.split_once("\n</desk>").unwrap();
    let (old, _) = kept.split_once("\n</desk>").unwrap();
    let line = |b: &str| b.lines().find(|l| l.contains("Header jumps")).unwrap_or_default().to_string();
    assert!(!line(old).contains("| working |"), "kept while it was to check: {old}");
    assert!(line(block).contains(&format!("#{id} bug: Header jumps | working |")), "the Desk as it is now, sent back: {got}");
}

/// Two messages kept together each start with a summary: one fresh one goes, first; a `<desk` in the words stays.
#[test]
fn kept_messages_get_one_fresh_summary() {
    use crate::desk::{has_block, with_fresh_block};
    let kept = "<desk project=\"p\">\nold one\n</desk>\n\nFrom a: hi\n\n<desk project=\"p\">\nold two\n</desk>\n\nFrom b: see <desk> tags";
    assert!(has_block(kept) && !has_block("From a: see <desk> tags"));
    assert_eq!(with_fresh_block(kept, "<desk project=\"p\">\nnew\n</desk>"), "<desk project=\"p\">\nnew\n</desk>\n\nFrom a: hi\n\nFrom b: see <desk> tags");
}
