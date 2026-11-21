//! Reopen and Send back go back to the session that held the item (#577, docs/item-chat-plan.md option A, step 3):
//! Reopen on a done item sends your words to that session when it's free, as a new piece of its chat; when it's
//! closed, a fresh session takes it with a hand-off. A done item whose session is at work on another item waits in
//! the queue for it (#576), and goes to a fresh session with that hand-off after 20 minutes.

use super::desk::desk;
use super::desk_commit::{ends, item, ready};
use crate::testing::World;

/// The session holding item `title`: (sid, name).
fn holder_of(w: &World, project: &str, title: &str) -> (String, String) {
    let it = item(w, project, title);
    (it["holder"]["sid"].as_str().unwrap_or_default().to_string(), it["holder"]["name"].as_str().unwrap_or_default().to_string())
}

fn sent_to(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

/// An item done (Looks right) by cue-teams: (project, id, its holder's sid).
fn done(w: &World) -> (String, String, String) {
    let (_, project, id) = ready(w);
    let (h, _) = holder_of(w, &project, "Header jumps");
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(item(w, &project, "Header jumps")["state"], "done");
    (project, id, h)
}

#[test]
fn reopen_on_a_done_item_goes_back_to_its_free_session_as_a_new_piece_of_its_chat() {
    let w = World::new();
    let (project, id, h) = done(&w);
    let e = w.hub().desk_item_act(&id, "send_back", " ").unwrap_err();
    assert!(e.contains("then Reopen"), "{e}");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done", "unchanged");

    let before = sent_to(&w, &h).len();
    w.advance(5_000);
    let it = w.hub().desk_item_act(&id, "send_back", "It still jumps on a slow scroll").unwrap();
    w.settle();
    assert_eq!(it.holder.as_ref().map(|x| x.sid.as_str()), Some(h.as_str()), "the same session");
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "working", "{shown}");
    let sent = sent_to(&w, &h)[before..].to_vec();
    assert!(sent.iter().any(|t| t.contains("isn't done (you said it looks right, and now it's reopened): It still jumps on a slow scroll") && t.contains(&format!("DONE: #{id}"))), "{sent:?}");
    // Two stints with the same session: its page draws the new piece under "Reopened".
    let stints: Vec<&str> = shown["stints"].as_array().unwrap().iter().filter_map(|s| s["sid"].as_str()).collect();
    assert_eq!(stints, vec![h.as_str(), h.as_str()], "{shown}");
    assert!(shown["stints"][0]["until"].as_u64().is_some() && shown["stints"][1]["until"].is_null(), "{shown}");
}

/// A done item, its session now on Footer: (project, Header's id, its session, Footer's id).
fn reopened_while_busy(w: &World) -> (String, String, String, String) {
    let (project, id, h) = done(w);
    let lead = desk(w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    ends(w, &lead, "ITEM: bug | Footer is cut off | done when: it shows\nROUTE: #new -> cue-teams | why: it knows the page");
    assert_eq!(item(w, &project, "Footer is cut off")["state"], "working");
    let other = item(w, &project, "Footer is cut off")["id"].as_str().unwrap().to_string();
    w.hub().desk_item_act(&id, "send_back", "It still jumps on a slow scroll").unwrap();
    w.settle();
    (project, id, h, other)
}

/// #576 with #577: its session is at work on another item, so the reopened item waits for it (one item per session),
/// and goes back to it, with your words, once that one gets Looks right.
#[test]
fn reopen_waits_for_its_session_while_it_is_on_another_item() {
    let w = World::new();
    let before = w.hosted.sent().len();
    let (project, id, h, other) = reopened_while_busy(&w);
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["next_for"]["sid"].as_str()), (Some("queued"), Some(h.as_str())), "{it}");
    assert!(it["holder"].is_null() && it["fresh_at"].as_u64().is_some(), "{it}");
    assert!(!w.hosted.sent()[before..].iter().any(|(s, t)| *s == h && t.contains("isn't done")), "cue-teams keeps to its own item");

    let at = sent_to(&w, &h).len();
    ends(&w, &h, &format!("Fixed.\nDONE: #{other}"));
    w.hub().desk_item_act(&other, "looks_right", "").unwrap();
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["holder"]["sid"].as_str()), (Some("working"), Some(h.as_str())), "{shown}");
    let sent = sent_to(&w, &h)[at..].to_vec();
    assert!(sent.iter().any(|t| t.contains("isn't done (you said it looks right, and now it's reopened): It still jumps on a slow scroll") && t.contains(&format!("DONE: #{id}"))), "{sent:?}");
    let stints: Vec<&str> = shown["stints"].as_array().unwrap().iter().filter_map(|s| s["sid"].as_str()).collect();
    assert_eq!(stints.first().copied(), Some(h.as_str()), "{shown}");
    assert_eq!(stints.last().copied(), Some(h.as_str()), "a new piece of its chat: {shown}");
    let it = w.hub().read(|st| st.desk.item(&id).cloned()).unwrap();
    assert!(it.reopen_note.is_empty() && it.back_message.is_empty() && it.moved_from.is_none(), "taken, so cleared");
}

/// #576 with #577: still busy 20 minutes on, a fresh session takes the reopened item with the hand-off.
#[test]
fn reopen_goes_to_a_fresh_session_with_a_hand_off_when_its_session_stays_on_another_item() {
    let w = World::new();
    let (project, id, h, other) = reopened_while_busy(&w);
    let at = sent_to(&w, &h).len();
    w.advance(20 * 60_000 + 1_000);
    let (fresh, name) = holder_of(&w, &project, "Header jumps");
    assert!(!fresh.is_empty() && fresh != h, "a fresh session holds it: {}", item(&w, &project, "Header jumps"));
    assert_ne!(name, "cue-teams");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working");
    assert!(!sent_to(&w, &h)[at..].iter().any(|t| t.contains("isn't done")), "cue-teams keeps to its own item");
    let told = sent_to(&w, &fresh).join("\n");
    assert!(told.contains(&format!("Hand-off: #{id} was done by cue-teams and is reopened; cue-teams is at work on #{other}")), "{told}");
    assert!(told.contains("in the user's words: It still jumps on a slow scroll"), "{told}");
    assert!(told.contains("Its work so far is on main:"), "its commit named: {told}");
    let it = w.hub().read(|st| st.desk.item(&id).cloned()).unwrap();
    assert!(it.reopen_note.is_empty() && it.back_message.is_empty() && it.moved_from.is_none(), "taken, so cleared");
}

#[test]
fn send_back_goes_to_a_fresh_session_when_its_session_is_closed() {
    let w = World::new();
    let (_, project, id) = ready(&w);
    let (h, _) = holder_of(&w, &project, "Header jumps");
    w.hub().forget(&h);
    w.settle();

    w.hub().desk_item_act(&id, "send_back", "The shadow is gone too").unwrap();
    w.settle();
    let (fresh, _) = holder_of(&w, &project, "Header jumps");
    assert!(!fresh.is_empty() && fresh != h, "{}", item(&w, &project, "Header jumps"));
    let told = sent_to(&w, &fresh).join("\n");
    assert!(told.contains(&format!("Hand-off: #{id} was done by cue-teams and is reopened; cue-teams is closed")), "{told}");
    assert!(told.contains("in the user's words: The shadow is gone too"), "{told}");
}

#[test]
fn send_back_on_an_item_to_check_stays_with_its_session_while_it_works_on_another() {
    // Under today's cap a session holds one to check and one at work: Send back stays with it (rule 7 queues behind).
    let w = World::new();
    let (_, project, id) = ready(&w);
    let (h, _) = holder_of(&w, &project, "Header jumps");
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    ends(&w, &lead, "ITEM: bug | Footer is cut off | done when: it shows\nROUTE: #new -> cue-teams | why: it knows the page");
    let it = w.hub().desk_item_act(&id, "send_back", "It still jumps").unwrap();
    w.settle();
    assert_eq!(it.holder.as_ref().map(|x| x.sid.as_str()), Some(h.as_str()));
}
