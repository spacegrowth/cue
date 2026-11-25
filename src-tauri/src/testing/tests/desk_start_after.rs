//! An item queued after another (#515): starting it, or a Desk message about it, leaves the item it waits on as it
//! was; and a lead's `/loop` wake-up is a turn of its own, so lines it wrote a turn before aren't acted on again.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::hosted::HostedEvent;
use crate::testing::{Transcript, World};
use serde_json::json;

/// A holder with item "Band" (put in `state`) and "Older pages" queued after it, both its, and the Desk's lead:
/// (holder, project, lead, Band's id, Older pages' id).
fn band_and_older_pages(w: &World, state: &str) -> (String, String, String, String, String) {
    let (h, project) = holder(w);
    let l = new_lead(w, &project);
    let m = w.hub().desk_add(&project, "feature", "Band", "", None).unwrap();
    w.hub().desk_give(&m.id, &h).unwrap();
    w.settle();
    // Both the holder's, Older pages queued behind Band: as a Desk saved before one item per session (#576) has it
    // (a route now queues it with no holder).
    let n = w.hub().desk_add(&project, "bug", "Older pages", "", None).unwrap();
    w.hub().desk_give(&n.id, &h).unwrap();
    w.settle();
    w.hub().desk_queue_after_unchecked(&n.id, &m.id);
    w.hub().desk_put_in_state(&m.id, state);
    let d = desk(w, &project);
    let it = item(&d, "Older pages");
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(m.id.as_str())), "{it}");
    (h, project, l, m.id, n.id)
}

/// Tonight's case (START #513 while it waited on #472, to check): the started item's brief names the one it waited
/// on; its holder taking it reopened that one. For each state the item waited on can be in, it stays in it.
#[test]
fn starting_an_item_queued_after_another_leaves_that_one_as_it_was_in_every_state() {
    for state in ["new", "working", "finishing", "to_check", "held"] {
        let w = World::new();
        let (h, project, l, m, n) = band_and_older_pages(&w, state);
        let before = sent_to(&w, &h).len();
        ends(&w, &l, &format!("START: #{n} | why: no need to wait for #{m} now"));
        let brief = sent_to(&w, &h)[before..].iter().find(|t| t.contains(&format!("Desk item #{n}"))).cloned().unwrap_or_else(|| panic!("{state}: no brief"));
        assert!(brief.contains(&format!("#{m}")), "{state}: the brief names #{m}, as tonight's did: {brief}");
        w.hosted_says(&h, HostedEvent::Taken { text: brief });
        w.settle();
        let d = desk(&w, &project);
        assert_eq!(item(&d, "Older pages")["state"], "working", "{state}: started");
        let band = item(&d, "Band");
        assert_eq!(band["state"], state, "{state}: #{m} as it was: {band}");
        assert!(band.get("reopened").is_none_or(|r| r.is_null()), "{state}: not reopened: {band}");
    }
}

/// The lead's note on an item to check, taken by its holder, leaves it to check (#584: it was #484's "puts it back
/// to work", and #542 went round and round on it). Only a Send back reopens it.
#[test]
fn a_leads_note_on_an_item_to_check_leaves_it_to_check() {
    let w = World::new();
    let (h, project, l, m, _) = band_and_older_pages(&w, "to_check");
    let before = sent_to(&w, &h).len();
    ends(&w, &l, &format!("NOTE: #{m} | the band is a line too thin"));
    let note = sent_to(&w, &h)[before..].iter().find(|t| t.contains(&format!("Desk item #{m}"))).cloned().unwrap();
    w.hosted_says(&h, HostedEvent::Taken { text: note });
    w.settle();
    let d = desk(&w, &project);
    let band = item(&d, "Band");
    assert_eq!(band["state"], "to_check", "{band}");
    assert!(band.get("reopened").is_none_or(|r| r.is_null()), "not reopened: {band}");
}

/// #811: the lead's note sent by a Cue that has since restarted (or by another Cue on the same data, as #790's was)
/// is still the Desk's own message when its holder takes it: the item stays to check, never reopened as if you'd
/// typed it.
#[test]
fn a_leads_note_sent_before_a_restart_still_leaves_it_to_check() {
    let w = World::new();
    let (h, project, l, m, _) = band_and_older_pages(&w, "to_check");
    let before = sent_to(&w, &h).len();
    ends(&w, &l, &format!("NOTE: #{m} | let your test run finish"));
    let note = sent_to(&w, &h)[before..].iter().find(|t| t.contains(&format!("Desk item #{m}"))).cloned().unwrap();
    // What this Cue sent since it started is gone, as after a restart or when another Cue sent it.
    w.hub().write(|st| st.desk.run.sent.clear());
    // Taken twice over, as a queue hands two of them over in one prompt.
    w.hosted_says(&h, HostedEvent::Taken { text: format!("{note}\n{note}") });
    w.settle();
    let d = desk(&w, &project);
    let band = item(&d, "Band");
    assert_eq!(band["state"], "to_check", "{band}");
    assert!(band.get("reopened").is_none_or(|r| r.is_null()), "not reopened: {band}");
    // Words you type yourself still put it back to work.
    ends(&w, &h, "I'll let it finish.");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: format!("#{m}: the band is still too thin") });
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Band")["state"], "working");
}

/// Tonight's other half: the lead's note on #472 ("nothing more for you") came again with each `/loop` wake-up, as
/// Cue read the wake-up's turn back to the Desk's last message; delivered while #472 was to check, it reopened it.
/// The wake-up is a turn of its own: nothing it didn't say is acted on.
#[test]
fn a_leads_loop_wake_up_doesnt_send_its_earlier_note_again() {
    let w = World::new();
    let (h, project, l, m, n) = band_and_older_pages(&w, "finishing");
    let log = Transcript::at(w.hub().session_origin(&l).unwrap().transcript_path);
    let check = format!("<desk project=\"{project}\">\nNeeds routing:\nNone.\n</desk>\nCheck the Desk");
    let note = format!("Nothing is stuck.\n\nNOTE: #{m} | Thanks, all clear. Nothing more for you on #{m}. #{n} is yours next, once #{m} closes.");
    log.prompt(&check).assistant(&note);
    let before = sent_to(&w, &h).len();
    ends(&w, &l, &note);
    let sent: Vec<String> = sent_to(&w, &h)[before..].iter().filter(|t| t.contains(&format!("Desk item #{m}"))).cloned().collect();
    assert_eq!(sent.len(), 1, "the note: {sent:?}");
    // Delivered while it's finishing: it stays so.
    w.hosted_says(&h, HostedEvent::Taken { text: sent[0].clone() });
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Band")["state"], "finishing");
    ends(&w, &h, "Noted.");
    // It lands and waits for you to check; the lead's /loop wakes it, 16 minutes on, as tonight.
    w.hub().desk_put_in_state(&m, "to_check");
    w.advance(16 * 60 * 1000);
    log.prompt("<command-message>loop</command-message>\n<command-name>/loop</command-name>\n<command-args>check the Desk and unstick anything stuck</command-args>")
        .entry(json!({ "type": "user", "isMeta": true, "message": { "role": "user", "content": [{ "type": "text", "text": "# /loop — schedule a recurring or self-paced prompt" }] } }))
        .assistant("Next Desk check is at 02:22.");
    let before = sent_to(&w, &h).len();
    ends(&w, &l, "Next Desk check is at 02:22.");
    let again: Vec<String> = sent_to(&w, &h)[before..].to_vec();
    assert!(again.is_empty(), "nothing sent again: {again:?}");
    assert_eq!(item(&desk(&w, &project), "Band")["state"], "to_check");
}
