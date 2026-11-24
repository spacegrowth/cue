//! A session reaches its Desk's lead with no lookups (#474): every hand-off names the lead and the TO LEAD: line, and
//! Cue delivers that line to whoever leads the item's Desk when it's written, after a lead's hand-off and across Desks.

use super::desk::{desk, ends, holder, new_lead, second, sent_to};
use crate::desk::parse::{lines, Line};
use crate::testing::World;

/// The Desk's lead as the window names it.
fn lead_name(w: &World, project: &str) -> String {
    desk(w, project)["lead"]["name"].as_str().unwrap().to_string()
}

/// What `sid` was sent that came from Cue's TO LEAD delivery.
fn from_sessions(w: &World, sid: &str) -> Vec<String> {
    sent_to(w, sid).into_iter().filter_map(|t| t.split_once("</desk>\n\n").map(|(_, m)| m.to_string()).or(Some(t))).filter(|t| t.starts_with("From ")).collect()
}

#[test]
fn to_lead_lines_are_read_with_or_without_an_item() {
    assert_eq!(
        lines("TO LEAD: #474 | Should it wrap?\n- **TO LEAD:** #12 which first?\nTO LEAD: I'm free\nTO LEAD: <your words>\nTO LEAD: #5 |  \n`TO LEAD: example`"),
        vec![
            Line::ToLead { id: Some("474".into()), text: "Should it wrap?".into() },
            Line::ToLead { id: Some("12".into()), text: "which first?".into() },
            Line::ToLead { id: None, text: "I'm free".into() },
        ]
    );
}

#[test]
fn a_hand_off_names_the_lead_and_how_to_reach_it() {
    let w = World::new();
    let (sid, project) = holder(&w);
    new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "bug", "Header wraps", "", None).unwrap();
    w.hub().desk_give(&it.id, &sid).unwrap();
    w.settle();
    let lead = lead_name(&w, &project);
    let hand = sent_to(&w, &sid).into_iter().find(|t| t.starts_with(&format!("Desk item #{}", it.id))).expect("handed the item");
    assert!(hand.contains(&format!("Your Desk lead is {lead}. To message it, write a line of its own: TO LEAD: <your words>")), "{hand}");
    assert!(hand.contains("no lookups needed"), "{hand}");
}

#[test]
fn to_lead_reaches_the_lead_as_from_the_session_and_item_and_is_not_a_stop() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let lead = new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "bug", "Header wraps", "", None).unwrap();
    w.hub().desk_give(&it.id, &sid).unwrap();
    w.settle();
    let name = w.snapshot().session(&sid)["name"].as_str().unwrap_or("").to_string();
    let name = if name.is_empty() { "cue-teams".to_string() } else { name };
    ends(&w, &sid, "Looked at it.\nTO LEAD: Should it wrap on narrow windows?");
    w.settle();
    let got = from_sessions(&w, &lead);
    assert_eq!(got, vec![format!("From {name} on #{} (Header wraps):\nShould it wrap on narrow windows?\nAnswer it with NOTE: #{} | <your words>.", it.id, it.id)], "{:?}", sent_to(&w, &lead));
    // Waiting on the lead's answer, not stopped: the lead isn't told it stopped, the session isn't asked "Done?".
    assert!(!sent_to(&w, &lead).iter().any(|t| t.contains("stopped")), "{:?}", sent_to(&w, &lead));
    let item = desk(&w, &project)["items"].as_array().unwrap().iter().find(|i| i["id"] == it.id.as_str()).cloned().unwrap();
    assert_eq!(item["state"], "working");
    // An item it isn't on comes back to it, unsent.
    ends(&w, &sid, "TO LEAD: #zz99 | what about this?");
    w.settle();
    assert_eq!(from_sessions(&w, &lead).len(), 1);
    assert!(sent_to(&w, &sid).iter().any(|t| t.starts_with("TO LEAD: not sent: #zz99")), "{:?}", sent_to(&w, &sid));
}

#[test]
fn after_the_lead_hands_off_to_lead_goes_to_the_new_one_and_hand_offs_name_it() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let old = new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "bug", "Header wraps", "", None).unwrap();
    w.hub().desk_give(&it.id, &sid).unwrap();
    w.settle();
    let next = second(&w, "successor");
    w.hub().desk_carry(&old, &next, "successor");
    w.settle();
    assert_eq!(desk(&w, &project)["lead"]["sid"], next);
    ends(&w, &sid, &format!("TO LEAD: #{} which side first?", it.id));
    w.settle();
    assert!(from_sessions(&w, &old).is_empty(), "{:?}", from_sessions(&w, &old));
    let got = from_sessions(&w, &next);
    assert!(got.len() == 1 && got[0].contains("which side first?") && got[0].contains(&format!("on #{} (Header wraps)", it.id)), "{got:?}");
    // A hand-off from now names the new lead.
    let two = w.hub().desk_add(&project, "task", "Footer", "", None).unwrap();
    w.hub().desk_give(&two.id, &sid).unwrap();
    w.settle();
    let hand = sent_to(&w, &sid).into_iter().find(|t| t.starts_with(&format!("Desk item #{}", two.id))).expect("handed the item");
    assert!(hand.contains("Your Desk lead is successor."), "{hand}");
}

#[test]
fn to_lead_goes_to_the_lead_of_the_items_desk() {
    let w = World::new();
    let start = |name: &str| {
        let r = crate::ipc::desk_start_named(w.hub(), &w.home().to_string_lossy(), true, Some(name)).unwrap();
        (r["id"].as_str().unwrap().to_string(), r["sid"].as_str().unwrap().to_string())
    };
    let (a, lead_a) = start("Feature A");
    let (b, lead_b) = start("Feature B");
    let worker = super::team::lead(&w, "hello");
    ends(&w, &worker, "Hi.");
    let one = w.hub().desk_add(&a, "bug", "Only A", "", None).unwrap();
    let two = w.hub().desk_add(&b, "bug", "Only B", "", None).unwrap();
    w.hub().desk_give(&one.id, &worker).unwrap();
    w.hub().desk_give(&two.id, &worker).unwrap();
    w.settle();
    ends(&w, &worker, &format!("TO LEAD: #{} | about B\nTO LEAD: #{} | about A", two.id, one.id));
    w.settle();
    let (to_a, to_b) = (from_sessions(&w, &lead_a), from_sessions(&w, &lead_b));
    assert!(to_a.len() == 1 && to_a[0].contains("about A") && to_a[0].contains("Only A"), "{to_a:?}");
    assert!(to_b.len() == 1 && to_b[0].contains("about B") && to_b[0].contains("Only B"), "{to_b:?}");
}

/// Your words to a lead at work and a session's TO LEAD line kept for it meanwhile never go as one message (#831): yours
/// go alone when its turn ends, and the lead's answer is under them in its chat; the session's words go after, as a
/// message of their own, never your words. Both stay in Cue's log once newer messages push them out of the few the
/// chat keeps live.
#[test]
fn your_words_and_a_sessions_question_go_to_the_lead_apart() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let lead = new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "bug", "Header wraps", "", None).unwrap();
    w.hub().desk_give(&it.id, &sid).unwrap();
    w.settle();
    let words = |t: &str| t.rsplit("</desk>").next().unwrap_or(t).trim().to_string();
    // The lead at work on your first message.
    crate::desk::hub::lead::desk_send_now(w.hub(), &project, "Status?", &[], false).unwrap();
    w.settle();
    let first = sent_to(&w, &lead).last().cloned().unwrap();
    w.hosted_says(&lead, crate::hosted::HostedEvent::Taken { text: first });
    w.settle();
    // Meanwhile the holder asks the lead something, and you write again: both kept for it.
    ends(&w, &sid, "Looked at it.\nTO LEAD: Should it wrap on narrow windows?");
    w.settle();
    crate::desk::hub::lead::desk_send_now(w.hub(), &project, "are we moving ?", &[], false).unwrap();
    ends(&w, &lead, "Two items are at work.");
    let yours = sent_to(&w, &lead).last().cloned().unwrap();
    let asked = format!("From cue-teams on #{} (Header wraps):\nShould it wrap on narrow windows?\nAnswer it with NOTE: #{} | <your words>.", it.id, it.id);
    assert_eq!(words(&yours), "are we moving ?", "your words alone");
    // The session's words stay kept for the next turn, not yours.
    let held = w.snapshot().session(&lead)["held"]["text"].as_str().map(|t| words(t));
    assert_eq!(held.as_deref(), Some(asked.as_str()));
    w.hosted_says(&lead, crate::hosted::HostedEvent::Taken { text: yours });
    w.settle();
    ends(&w, &lead, "Yes, we're moving: two items landed.");
    let next = sent_to(&w, &lead).last().cloned().unwrap();
    assert_eq!(words(&next), asked, "the session's words, a message of their own");
    w.hosted_says(&lead, crate::hosted::HostedEvent::Taken { text: next });
    w.settle();
    ends(&w, &lead, &format!("Answered it.\nNOTE: #{} | Yes, wrap it.", it.id));
    let chat: Vec<(String, String)> = w.snapshot().session(&lead)["thread"].as_array().unwrap().iter().map(|e| (e["role"].as_str().unwrap().to_string(), words(e["text"].as_str().unwrap()))).collect();
    let at = chat.iter().position(|(r, t)| r == "you" && t == "are we moving ?").unwrap_or_else(|| panic!("your words alone in its chat: {chat:?}"));
    assert_eq!(chat.get(at + 1).map(|(r, t)| (r.as_str(), t.starts_with("Yes, we're moving"))), Some(("agent", true)), "the answer right under them: {chat:?}");
    assert_eq!(chat.get(at + 2).map(|(r, t)| (r.as_str(), t.as_str())), Some(("you", asked.as_str())), "{chat:?}");
    assert!(!chat.iter().any(|(r, t)| r == "you" && t.contains("are we moving") && t.contains("From cue-teams")), "{chat:?}");
    // Five newer messages: the chat keeps only the last few live; Cue's log still has both, once each.
    for k in 0..3 {
        crate::desk::hub::lead::desk_send_now(w.hub(), &project, &format!("next {k}"), &[], false).unwrap();
        w.settle();
        let t = sent_to(&w, &lead).last().cloned().unwrap();
        w.hosted_says(&lead, crate::hosted::HostedEvent::Taken { text: t });
        ends(&w, &lead, &format!("Answer {k}."));
    }
    let live = w.snapshot().session(&lead)["thread"].as_array().unwrap().len();
    let log: Vec<(String, String)> = crate::db::session_log(&lead).iter().map(|e| (e["role"].as_str().unwrap().to_string(), words(e["text"].as_str().unwrap()))).collect();
    assert!(!w.snapshot().session(&lead)["thread"].as_array().unwrap().iter().any(|e| words(e["text"].as_str().unwrap()).starts_with("are we moving")), "pushed out of the {live} kept live");
    let at = log.iter().position(|(r, t)| r == "you" && t == "are we moving ?").unwrap_or_else(|| panic!("your words are in Cue's log: {log:?}"));
    assert!(log[at + 1].0 == "agent" && log[at + 1].1.starts_with("Yes, we're moving"), "{log:?}");
    assert_eq!(log.iter().filter(|(_, t)| t.starts_with("Yes, we're moving")).count(), 1, "{log:?}");
}

/// Esc on what's kept for a lead gives back only your words (#831): a session's words kept with them stay kept, and
/// with none of yours, nothing comes back.
#[test]
fn taking_back_a_kept_message_leaves_a_sessions_words_kept() {
    let w = World::new();
    let (sid, project) = holder(&w);
    let lead = new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "bug", "Header wraps", "", None).unwrap();
    w.hub().desk_give(&it.id, &sid).unwrap();
    w.settle();
    let words = |t: &str| t.rsplit("</desk>").next().unwrap_or(t).trim().to_string();
    crate::desk::hub::lead::desk_send_now(w.hub(), &project, "Status?", &[], false).unwrap();
    w.settle();
    let first = sent_to(&w, &lead).last().cloned().unwrap();
    w.hosted_says(&lead, crate::hosted::HostedEvent::Taken { text: first });
    w.settle();
    ends(&w, &sid, "TO LEAD: Should it wrap?");
    w.settle();
    assert!(w.hub().unhold(&lead).is_none(), "only the session's words kept: nothing of yours to take back");
    crate::desk::hub::lead::desk_send_now(w.hub(), &project, "are we moving ?", &[], false).unwrap();
    let back = w.hub().unhold(&lead).expect("your words back");
    assert_eq!(words(&back.text), "are we moving ?");
    let held = w.snapshot().session(&lead)["held"]["text"].as_str().map(|t| words(t)).unwrap_or_default();
    assert!(held.starts_with("From cue-teams on #") && held.contains("Should it wrap?"), "{held}");
}

#[test]
fn relays_come_apart_from_your_words() {
    let r = crate::desk::relay_words("cue-1", Some(("5", "A (b)")), "why?");
    let none = crate::desk::relay_words("cue-2", None, "free");
    let text = format!("<desk x>\nsum\n</desk>\n\n{r}\n\nhello\n\nthere\n\n{none}");
    let (mine, relays) = crate::desk::relays_apart(&text).unwrap();
    assert_eq!(mine, "<desk x>\nsum\n</desk>\n\nhello\n\nthere");
    assert_eq!(relays, format!("{r}\n\n{none}"));
    assert!(crate::desk::relays_apart(&r).is_none() && crate::desk::relays_apart("hello").is_none());
    assert!(crate::desk::only_relays(&r) && !crate::desk::only_relays("hello") && !crate::desk::only_relays(""));
}
