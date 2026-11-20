//! A question you ask on an item's page while it's to check or done (#475, desk/hub/turn/asked.rs): its holder answers
//! it; a turn that only answers puts the item back where it was, one that changed its code lands as a new round.

use super::desk::sent_to;
use super::desk_commit::{commits, ends, item, routed_with};
use super::team::{says, writes};
use crate::hosted::HostedEvent;
use crate::testing::World;

/// An item to check, its holder's crew having written a.txt and b.txt: (repo, project, item id, holder).
pub(super) fn to_check(w: &World) -> (std::path::PathBuf, String, String, String) {
    let (dir, project, id, h) = routed_with(w, |_, _, _| {}, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    });
    ends(w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(w, &project, "Header jumps")["state"], "to_check");
    (dir, project, id, h)
}

/// You ask `question` on its page, and its holder takes it.
fn ask(w: &World, h: &str, id: &str, question: &str) {
    w.hub().desk_item_act(id, "note", question).unwrap();
    w.settle();
    let sent = sent_to(w, h);
    assert!(sent.iter().any(|t| t.starts_with(&format!("A question about Desk item #{id} (Header jumps)")) && t.contains(question)), "{sent:?}");
    w.advance(1000);
    w.hosted_says(h, HostedEvent::Taken { text: question.into() });
    w.settle();
}

#[test]
fn a_question_answered_without_changing_code_puts_the_item_back_where_it_was() {
    let w = World::new();
    let (dir, project, id, h) = to_check(&w);
    let made = commits(&dir);
    let stall = format!("Done? Write DONE: #{id}, or say what's left");
    ask(&w, &h, &id, "Why does it stay put now?");
    w.advance(1000);
    ends(&w, &h, "It's pinned to the top of the page now, so scrolling doesn't move it.");
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool().unwrap_or(false)), (Some("to_check"), false), "back to check, not stopped: {it}");
    assert!(it["asked"].is_null() && it["backs"].as_array().map_or(true, |b| b.is_empty()), "no new round: {it}");
    assert!(!sent_to(&w, &h).contains(&stall), "not asked Done?");
    assert_eq!(commits(&dir), made, "nothing new committed");
    // Done, the same: asked and answered, it's done again.
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    ask(&w, &h, &id, "Does it work on the small window too?");
    w.advance(1000);
    ends(&w, &h, "Yes, the small window uses the same header.");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "done", "{it}");
    assert!(!sent_to(&w, &h).contains(&stall));
}

#[test]
fn a_question_whose_answer_changed_code_lands_again_as_a_new_round() {
    let w = World::new();
    let (dir, project, id, h) = to_check(&w);
    let made: u32 = commits(&dir).parse().unwrap();
    ask(&w, &h, &id, "Could the header be a bit shorter?");
    let f = dir.join("a.txt").to_string_lossy().to_string();
    says(&w, &h, writes("toolu_B", "agentB", "shorter", &[f]));
    w.advance(1000);
    ends(&w, &h, "Yes: I made it 8px shorter.");
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "landed again: {it}");
    assert!(it["asked"].is_null(), "{it}");
    let backs = it["backs"].as_array().cloned().unwrap_or_default();
    assert_eq!(backs.len(), 1, "a new round: {it}");
    assert_eq!(backs[0]["why"], "Your question: Could the header be a bit shorter?");
    assert_eq!(it["checks"].as_array().map(|c| c.len()), Some(2), "a card for the new round: {it}");
    assert_eq!(commits(&dir).parse::<u32>().unwrap(), made + 1, "its new work committed");
}

/// What #455's holder said as it put up its grip mocks: the question isn't its last words.
const GRIP: &str = "I've put three new grip looks next to today's on the canvas. Pick one and I'll build it.\n\n\
- **A:** six dots in the middle of the right end.\n\
- **B:** three short lines stacked into a ridged bar.\n\
- **C:** the six dots on a soft tab the height of the row.\n\n\
DECIDED (minor): Mac only, on a new canvas page.\n\n\
Which one: A, B or C? Once it's built and installed I'll say DONE.";

/// An item `title` routed to a Desk holder: (lead, holder, project, item id).
fn routed(w: &World, title: &str) -> (String, String, String, String) {
    use super::desk::{desk, holder, item as item_in, new_lead};
    let (h, project) = holder(w);
    let l = new_lead(w, &project);
    ends(w, &l, &format!("ITEM: {title}\nROUTE: #new -> cue-teams | why: it knows"));
    let id = item_in(&desk(w, &project), title)["id"].as_str().unwrap().to_string();
    (l, h, project, id)
}

/// Holder `h` takes `prompt` and its turn ends saying `reply`.
fn turn(w: &World, h: &str, prompt: &str, reply: &str) {
    w.advance(1000);
    w.hosted_says(h, HostedEvent::Taken { text: prompt.into() });
    w.settle();
    w.advance(1000);
    ends(w, h, reply);
}

fn shown(w: &World, project: &str, title: &str) -> serde_json::Value {
    super::desk::item(&super::desk::desk(w, project), title).clone()
}

/// A holder's turn that asks you something waits on you with that question (the list, Progress and the header count
/// all read the item's state), saved, until you answer it in its chat (#475).
#[test]
fn a_question_to_you_waits_on_you_until_you_answer() {
    let w = World::new();
    let (_, h, project, id) = routed(&w, "Queued grips");
    let stall = format!("Done? Write DONE: #{id}, or say what's left");
    turn(&w, &h, "Queued grips", GRIP);
    let it = shown(&w, &project, "Queued grips");
    assert_eq!((it["state"].as_str(), it["latest"].as_str()), (Some("needs_you"), Some("Which one: A, B or C?")), "{it}");
    assert!(!sent_to(&w, &h).contains(&stall), "not asked Done?");
    // A live question: still so a while on, and after a restart.
    w.advance(60 * 60 * 1000);
    assert_eq!(shown(&w, &project, "Queued grips")["state"], "needs_you");
    let mut w = w;
    w.relaunch();
    assert_eq!(shown(&w, &project, "Queued grips")["state"], "needs_you");
    // You answer in its chat: no longer waiting on you.
    w.hub().send_yours(&h, "B", &[], false).unwrap();
    w.settle();
    let it = shown(&w, &project, "Queued grips");
    assert_ne!(it["state"], "needs_you", "{it}");
    assert!(it["asks"].is_null(), "{it}");
    // Asked again, and answered by typing into the session itself: the prompt it takes isn't the Desk's, so it's yours.
    w.advance(1000);
    ends(&w, &h, "Built B. Should the grip show on hover only?");
    assert_eq!(shown(&w, &project, "Queued grips")["state"], "needs_you");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "No, always".into() });
    w.settle();
    let it = shown(&w, &project, "Queued grips");
    assert_eq!(it["state"], "working", "{it}");
    assert!(it["asks"].is_null(), "{it}");
}

/// #559: the lead's NOTE to its holder replies to what it asked you: the item stops waiting on you at once, and stays
/// so through the turn that takes the note, unless that turn asks you again.
#[test]
fn the_leads_note_settles_the_question() {
    let w = World::new();
    let (l, h, project, id) = routed(&w, "Queued grips");
    turn(&w, &h, "Queued grips", GRIP);
    assert_eq!(shown(&w, &project, "Queued grips")["state"], "needs_you");
    ends(&w, &l, &format!("You picked B earlier today.\nNOTE: #{id} | the user picked B; build it"));
    let it = shown(&w, &project, "Queued grips");
    assert_ne!(it["state"], "needs_you", "the note answers it: {it}");
    assert!(it["asks"].is_null(), "{it}");
    let note = sent_to(&w, &h).into_iter().find(|t| t.starts_with(&format!("Note from the Desk lead on Desk item #{id}"))).expect("the note sent");
    assert!(note.contains("the user picked B; build it"), "{note}");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: note.clone() });
    w.settle();
    assert_eq!(shown(&w, &project, "Queued grips")["state"], "working");
    w.advance(1000);
    ends(&w, &h, "Building B now; the tests are still running.");
    assert_ne!(shown(&w, &project, "Queued grips")["state"], "needs_you");
    // A later turn that asks you again waits on you again.
    turn(&w, &h, "go on", "Built B. Should the grip show on hover only?");
    assert_eq!(shown(&w, &project, "Queued grips")["latest"], "Should the grip show on hover only?");
}

/// #559: the lead's ANSWER to an item waiting on you with its holder's question (no clean-up card): the answer goes to
/// the holder, and the item no longer waits on you. With nothing asked, it's still refused.
#[test]
fn the_leads_answer_settles_the_question() {
    let w = World::new();
    let (l, h, project, id) = routed(&w, "Queued grips");
    ends(&w, &l, &format!("ANSWER: #{id} | B"));
    assert!(sent_to(&w, &l).iter().any(|t| t.contains(&format!("No question from #{id} waits on you"))), "nothing asked yet: refused");
    turn(&w, &h, "Queued grips", GRIP);
    assert_eq!(shown(&w, &project, "Queued grips")["state"], "needs_you");
    ends(&w, &l, &format!("ANSWER: #{id} | B, the user said so on the canvas"));
    let it = shown(&w, &project, "Queued grips");
    assert_ne!(it["state"], "needs_you", "{it}");
    assert!(it["asks"].is_null(), "{it}");
    let sent = sent_to(&w, &h);
    assert!(sent.iter().any(|t| t.starts_with(&format!("Note from the Desk lead on Desk item #{id}")) && t.contains("Answer to your question: B, the user said so on the canvas")), "{sent:?}");
}

/// #559, #384's case: its holder asked "should I wait for the clean-up?", then carried on and started the work. While
/// it's at work on a newer turn it isn't waiting on you; its turn ending on something else settles the question; one
/// that asks again waits on you with the new question.
#[test]
fn a_holder_that_carries_on_past_its_question_no_longer_waits_on_you() {
    let w = World::new();
    let (_, h, project, id) = routed(&w, "Change feed");
    let title = "Change feed";
    turn(&w, &h, "Change feed", "Refactor 6 is still landing. Should I wait for the clean-up before I start?");
    let it = shown(&w, &project, title);
    assert_eq!((it["state"].as_str(), it["latest"].as_str()), (Some("needs_you"), Some("Should I wait for the clean-up before I start?")), "{it}");
    // Something wakes it (the clean-up landed, a hook, a teammate's report) and it starts the work: at work, not on you.
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "Refactor 6 landed on main.".into() });
    w.settle();
    let it = shown(&w, &project, title);
    assert_eq!(it["state"], "working", "carrying on past it: {it}");
    // Its turn ends on something else: the question is settled for good, and the item is read like any other turn.
    w.advance(1000);
    ends(&w, &h, "The clean-up is on main, so I started the change feed. Tests are running now.");
    let it = shown(&w, &project, title);
    assert_ne!(it["state"], "needs_you", "{it}");
    assert!(it["asks"].is_null(), "{it}");
    assert!(!sent_to(&w, &h).contains(&format!("Done? Write DONE: #{id}, or say what's left")), "it said what it waits on: not asked Done?");
    let mut w = w;
    w.relaunch();
    assert_ne!(shown(&w, &project, title)["state"], "needs_you", "settled, saved");
    // Asking again: waiting on you with that.
    turn(&w, &h, "go on", "Feed is built. Should old changes show too?");
    assert_eq!(shown(&w, &project, title)["latest"], "Should old changes show too?");
}

/// #559: your note on its page answers what its holder asked you.
#[test]
fn your_note_on_its_page_settles_the_question() {
    let w = World::new();
    let (_, h, project, id) = routed(&w, "Queued grips");
    turn(&w, &h, "Queued grips", GRIP);
    assert_eq!(shown(&w, &project, "Queued grips")["state"], "needs_you");
    w.hub().desk_item_act(&id, "note", "B please").unwrap();
    w.settle();
    let it = shown(&w, &project, "Queued grips");
    assert_ne!(it["state"], "needs_you", "{it}");
    assert!(it["asks"].is_null(), "{it}");
}

/// A turn that only says it waits on you ("I'm waiting for your pick") asks you too; what's left of its own work, or a
/// question it answers itself, doesn't.
#[test]
fn what_a_turn_asks_you_is_read_from_its_words() {
    use crate::desk::hub::stall::stall::question_for_you;
    assert_eq!(question_for_you(GRIP).as_deref(), Some("Which one: A, B or C?"));
    assert_eq!(question_for_you("Not done yet. I'm waiting for your pick of grip A, B or C on the canvas.").as_deref(), Some("Not done yet. I'm waiting for your pick of grip A, B or C on the canvas."));
    assert_eq!(question_for_you("Moved it.\n\nShould the footer move too?").as_deref(), Some("Should the footer move too?"));
    assert_eq!(question_for_you("Why did it jump? Because the cache was stale."), None);
    assert_eq!(question_for_you("Still waiting on the test run."), None);
    assert_eq!(question_for_you("Moved some of it."), None);
    // #511: only what it ends on. A line before its last naming Cue's Waiting on you list isn't it waiting on you.
    assert_eq!(question_for_you("Started them:\n- ui-tests: the Waiting on you list\n\nI'll report back when they finish."), None);
    assert_eq!(question_for_you("Next I'll check the Waiting on you list."), None);
    assert_eq!(question_for_you("Waiting on your pick of A or B."), Some("Waiting on your pick of A or B.".into()));
}
