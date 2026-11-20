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

/// A holder's turn that asks you something waits on you with that question (the list, Progress and the header count
/// all read the item's state), through its later turns that ask nothing, until you answer it in its chat.
#[test]
fn a_question_to_you_waits_on_you_through_later_turns_until_you_answer() {
    use super::desk::{desk, holder, item as item_in, new_lead};
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Queued grips\nROUTE: #new -> cue-teams | why: it knows");
    let id = item_in(&desk(&w, &project), "Queued grips")["id"].as_str().unwrap().to_string();
    let stall = format!("Done? Write DONE: #{id}, or say what's left");
    let turn = |prompt: &str, reply: &str| {
        w.advance(1000);
        w.hosted_says(&h, HostedEvent::Taken { text: prompt.into() });
        w.settle();
        w.advance(1000);
        ends(&w, &h, reply);
    };
    let shown = || item_in(&desk(&w, &project), "Queued grips").clone();
    turn("Queued grips", GRIP);
    let it = shown();
    assert_eq!((it["state"].as_str(), it["latest"].as_str()), (Some("needs_you"), Some("Which one: A, B or C?")), "{it}");
    assert!(!sent_to(&w, &h).contains(&stall), "not asked Done?");
    // A later turn about something else (the lead's note) asks nothing: still waiting on you with the question.
    ends(&w, &l, &format!("Telling it.\nNOTE: #{id} | also check the queue test still passes"));
    let note = sent_to(&w, &h).into_iter().find(|t| t.starts_with(&format!("Note from the Desk lead on Desk item #{id}"))).expect("the note sent");
    turn(&note, "The queue test passes.");
    let it = shown();
    assert_eq!((it["state"].as_str(), it["latest"].as_str(), it["stopped"].as_bool().unwrap_or(false)), (Some("needs_you"), Some("Which one: A, B or C?"), false), "{it}");
    assert!(!sent_to(&w, &h).contains(&stall), "not asked Done?");
    // Saved: still so after a restart.
    let mut w = w;
    w.relaunch();
    assert_eq!(item_in(&desk(&w, &project), "Queued grips")["state"], "needs_you");
    // You answer in its chat: no longer waiting on you.
    w.hub().send_yours(&h, "B", &[], false).unwrap();
    w.settle();
    let it = item_in(&desk(&w, &project), "Queued grips").clone();
    assert_ne!(it["state"], "needs_you", "{it}");
    assert!(it["asks"].is_null(), "{it}");
    // Asked again, and answered by typing into the session itself: the prompt it takes isn't the Desk's, so it's yours.
    w.advance(1000);
    ends(&w, &h, "Built B. Should the grip show on hover only?");
    assert_eq!(item_in(&desk(&w, &project), "Queued grips")["state"], "needs_you");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "No, always".into() });
    w.settle();
    let it = item_in(&desk(&w, &project), "Queued grips").clone();
    assert_eq!(it["state"], "working", "{it}");
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
