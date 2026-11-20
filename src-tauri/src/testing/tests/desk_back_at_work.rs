//! An item to check whose holder goes back to work on it (#484): a send-back, the lead's note, a route back to it, or
//! any turn about it. It shows at work (the list, Progress, the header count and its page all read its state), its
//! page saying what its holder is doing, and it's to check again only when its holder says DONE again.

use super::desk::sent_to;
use super::desk_asked::to_check;
use super::desk_commit::{ends, item};
use crate::hosted::HostedEvent;
use crate::testing::World;

/// Holder `h` takes `prompt` as its next turn, and works a step on the item (editing a.txt in `dir`).
fn takes(w: &World, h: &str, prompt: &str) {
    w.advance(1000);
    w.hosted_says(h, HostedEvent::Taken { text: prompt.into() });
    w.settle();
}

/// The item is at work again, not to check: a new round on Progress (to check no longer stamped), its old round kept.
fn at_work(w: &World, project: &str) -> serde_json::Value {
    let it = item(w, project, "Header jumps").clone();
    assert_eq!(it["state"], "working", "at work, not to check: {it}");
    let steps: Vec<u64> = it["steps"].as_array().map(|s| s.iter().map(|v| v.as_u64().unwrap_or(0)).collect()).unwrap_or_default();
    assert_eq!(steps.get(5), Some(&0), "Progress: not at To check: {it}");
    it
}

/// It changes a.txt where it works on the item, then says DONE again: to check again.
fn done_again(w: &World, h: &str, id: &str, project: &str) {
    let it = item(w, project, "Header jumps");
    let dir = std::path::PathBuf::from(it["tree"]["path"].as_str().expect("its copy"));
    std::fs::write(dir.join("a.txt"), format!("round two of #{id}\n")).unwrap();
    super::desk_commit::log_edit(w, h, &dir.join("a.txt"));
    w.advance(1000);
    ends(w, h, &format!("Fixed that too.\nDONE: #{id}"));
    let it = item(w, project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
}

#[test]
fn an_item_sent_back_while_to_check_shows_at_work_until_its_done_again() {
    let w = World::new();
    let (_, project, id, h) = to_check(&w);
    w.hub().desk_item_act(&id, "send_back", "It still jumps on a small window").unwrap();
    w.settle();
    at_work(&w, &project);
    let back = sent_to(&w, &h).into_iter().find(|t| t.starts_with(&format!("Desk item #{id} (Header jumps) isn't done"))).expect("sent back");
    takes(&w, &h, &back);
    let it = at_work(&w, &project);
    assert_ne!(it["latest"], "Fixed.", "not what it said last round: {it}");
    let it = at_work(&w, &project);
    assert_ne!(it["act"]["now"], "DONE: #1", "not what it said last round: {it}");
    done_again(&w, &h, &id, &project);
}

#[test]
fn an_item_to_check_whose_holder_takes_the_leads_note_shows_at_work_until_its_done_again() {
    let w = World::new();
    let (_, project, id, h) = to_check(&w);
    let l = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    w.advance(1000);
    ends(&w, &l, &format!("Telling it.\nNOTE: #{id} | the footer jumps too, fix it with the header"));
    // Sent, not yet taken: still to check.
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    let note = sent_to(&w, &h).into_iter().find(|t| t.starts_with(&format!("Note from the Desk lead on Desk item #{id}"))).expect("the note sent");
    takes(&w, &h, &note);
    let it = at_work(&w, &project);
    assert_ne!(it["latest"], "Fixed.", "not what it said last round: {it}");
    // A turn that ends without DONE: still at work (asked Done?), not back to check.
    w.advance(1000);
    ends(&w, &h, "Moved the footer.");
    assert_ne!(item(&w, &project, "Header jumps")["state"], "to_check");
    done_again(&w, &h, &id, &project);
}

/// #402's case: to check, routed to its holder again by the lead (what's left), and at work on it.
#[test]
fn an_item_to_check_routed_back_to_its_holder_shows_at_work_until_its_done_again() {
    let w = World::new();
    let (_, project, id, h) = to_check(&w);
    let l = super::desk::desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    w.advance(1000);
    ends(&w, &l, &format!("ROUTE: #{id} -> cue-teams | why: first check what's still cut off in the header; fix only that"));
    at_work(&w, &project);
    let sent = sent_to(&w, &h).last().cloned().unwrap();
    takes(&w, &h, &sent);
    at_work(&w, &project);
    done_again(&w, &h, &id, &project);
}

/// You type into its session about the item while it's to check: at work on it again.
#[test]
fn an_item_to_check_whose_holder_you_ask_to_change_it_shows_at_work() {
    let w = World::new();
    let (_, project, id, h) = to_check(&w);
    takes(&w, &h, &format!("#{id}: make the header 4px shorter too"));
    at_work(&w, &project);
    done_again(&w, &h, &id, &project);
}
