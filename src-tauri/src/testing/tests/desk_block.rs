//! The Desk block a lead reads (#818, desk/hub/lead/run.rs, block.rs): items to route, at work, finishing or paused in
//! full and the rest one line; cold sessions too big for new work that hold nothing counted; the rules only in a lead's
//! first message and after a compaction; then only what changed, the whole block again every 20 messages or when asked.

use super::desk::{ends, holder, new_lead, second, sent_to};
use crate::clock::now_ms;
use crate::testing::World;
use serde_json::json;

const DONE_WHEN: &str = "the header keeps its place while the list scrolls under it on every Desk page, in light and dark, with a test that scrolls a 200-item list and checks the header's top edge each frame; cargo test for desk_view passes and nothing else in the window moves";
const SEE: &str = "The header stays at the top as you scroll a long list of items, and nothing under it jumps or flickers.";

/// The last message lead `d` was sent.
fn last(w: &World, d: &str) -> String {
    sent_to(w, d).pop().unwrap_or_default()
}

/// Message `m` carries only what changed.
fn changes_only(m: &str) -> bool {
    m.lines().next().is_some_and(|l| l.ends_with(" changes=\"only\">"))
}

/// Message `m` carries the whole block.
fn is_whole(m: &str) -> bool {
    m.starts_with("<desk ") && !changes_only(m)
}

/// The block of message `m`.
fn block(m: &str) -> &str {
    m.split_once("</desk>").map_or(m, |(b, _)| b)
}

/// You write `text` to `project`'s lead `d`; the message it was sent.
fn say(w: &World, project: &str, d: &str, text: &str) -> String {
    crate::ipc::desk_send_with(w.hub(), project, text).unwrap();
    w.settle();
    // With no block when nothing changed (#820): the words alone.
    let m = sent_to(w, d).into_iter().rev().find(|m| m == text || m.ends_with(&format!("</desk>\n\n{text}"))).unwrap_or_default();
    ends(w, d, "Noted.");
    m
}

/// Session `sid`'s cache ran out (`cold`) or holds for an hour, at `tokens`.
fn cache(w: &World, sid: &str, cold: bool, tokens: u64) {
    let until = if cold { now_ms() - 60_000 } else { now_ms() + 3_600_000 };
    w.hub().write(|st| st.insight.insert(sid.to_string(), json!({ "tokens": tokens, "until_ms": until })));
}

/// Item `id`'s state set to `state`, held by `sid` (or nobody).
fn set_item(w: &World, id: &str, state: &str, sid: Option<&str>) {
    w.hub().write(|st| {
        let holder = sid.map(|s| crate::desk::model::Holder { kind: "session".into(), sid: s.into(), name: st.sessions.get(s).map(crate::desk::hub::core::desk::shown_name).unwrap_or_default(), ..Default::default() });
        let it = st.desk.item_mut(id).unwrap();
        it.state = state.into();
        it.holder = holder;
    });
}

#[test]
fn a_busy_desk_of_forty_items_and_seventy_sessions_fits_in_twelve_thousand_characters() {
    let w = World::new();
    let (first, project) = holder(&w);
    let mut sids = vec![first];
    for i in 1..70 {
        sids.push(second(&w, &format!("cue-s{i}")));
    }
    let d = new_lead(&w, &project);
    // 47 cold and too big for new work, 23 warm.
    for (i, s) in sids.iter().enumerate() {
        cache(&w, s, i < 47, 120_000 + i as u64 * 1000);
    }
    // 40 items: 2 to route, 3 at work, 15 queued, 20 to check; the ones held go to the first sessions, 12 of them cold.
    let mut ids = vec![];
    for i in 0..40 {
        let it = w.hub().desk_add_with(&project, "feature", &format!("Item {i}: the header stays put when the list scrolls"), SEE, DONE_WHEN, None, &[]).unwrap();
        ids.push(it.id);
    }
    for (i, id) in ids.iter().enumerate() {
        match i {
            0..2 => {}
            2..5 => set_item(&w, id, "working", Some(&sids[35 + i])),
            5..20 => set_item(&w, id, "queued", None),
            _ => set_item(&w, id, "to_check", Some(&sids[if i < 32 { 35 + i - 20 } else { 47 + i - 32 }])),
        }
    }
    let m = say(&w, &project, &d, "What's next?");
    let b = block(&m);
    assert!(b.len() < 12_000, "{} characters:\n{b}", b.len());
    // To route and at work: in full. Queued and to check: one line.
    for id in &ids[0..5] {
        let line = b.lines().find(|l| l.starts_with(&format!("- #{id} "))).unwrap_or_else(|| panic!("no #{id}: {b}"));
        assert!(line.contains(DONE_WHEN) && line.contains(SEE), "{line}");
    }
    for id in &ids[5..] {
        let line = b.lines().find(|l| l.starts_with(&format!("- #{id} "))).unwrap_or_else(|| panic!("no #{id}: {b}"));
        assert!(!line.contains(DONE_WHEN) && !line.contains(SEE), "{line}");
    }
    // Cold sessions holding an item stay listed; cold ones holding nothing are a count.
    assert!(b.contains("Also 35 cold sessions too big for new work, holding nothing"), "{b}");
    for s in &sids[35..70] {
        let id: String = s.chars().take(8).collect();
        assert!(b.contains(&format!("sid:{id}")), "{id} listed: {b}");
    }
    for s in &sids[..35] {
        let id: String = s.chars().take(8).collect();
        assert!(!b.contains(&format!("sid:{id}")), "{id} counted, not listed");
    }
    assert!(!b.contains("crew:") && !b.contains("cache ") , "{b}");
}

#[test]
fn a_cold_session_small_enough_to_wake_stays_listed() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let id: String = h.chars().take(8).collect();
    cache(&w, &h, true, 40_000);
    assert!(block(&say(&w, &project, &d, "hi")).contains(&format!("sid:{id}")));
    cache(&w, &h, true, 90_000);
    let m = say(&w, &project, &d, "hi");
    assert!(!m.contains(&format!("sid:{id}")) && m.contains("Also 1 cold sessions"), "{m}");
}

#[test]
fn the_rules_go_in_the_first_message_and_again_after_a_compaction() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let started = sent_to(&w, &d).pop().unwrap();
    assert!(started.contains(crate::desk::ROUTING_GUIDANCE), "{started}");
    // Its first message from you: the whole block with the rules (it was started with the block for no session yet).
    let one = say(&w, &project, &d, "one");
    assert!(one.contains(crate::desk::ROUTING_GUIDANCE) && !changes_only(&one), "{one}");
    let two = say(&w, &project, &d, "two");
    assert!(!two.contains(crate::desk::ROUTING_GUIDANCE) && changes_only(&two), "{two}");
    w.hub().write(|st| st.sessions.set_compacted(&d, now_ms() + 1));
    let three = say(&w, &project, &d, "three");
    assert!(three.contains(crate::desk::ROUTING_GUIDANCE) && !changes_only(&three), "{three}");
}

#[test]
fn the_name_line_says_never_the_owner_or_the_user() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    crate::owner::test_name::set(Some("Robin"));
    let m = say(&w, &project, &d, "hi");
    crate::owner::test_name::set(None);
    assert!(m.contains("call them Robin, or \"you\" when speaking to them; never \"the owner\" or \"the user\"."), "{m}");
    assert!(!m.contains("never \"Robin\""), "{m}");
}

#[test]
fn a_change_shows_in_the_very_next_message_and_nothing_else_does() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let a = w.hub().desk_add_with(&project, "bug", "Header jumps", SEE, DONE_WHEN, None, &[]).unwrap().id;
    let b = w.hub().desk_add_with(&project, "bug", "Footer cut off", SEE, DONE_WHEN, None, &[]).unwrap().id;
    set_item(&w, &b, "queued", None);
    say(&w, &project, &d, "first");
    let quiet = say(&w, &project, &d, "anything?");
    assert_eq!(quiet, "anything?", "nothing changed and it's about no item: no block at all");

    // A state change.
    set_item(&w, &b, "to_check", Some(&h));
    let m = say(&w, &project, &d, "and now?");
    assert!(block(&m).contains(&format!("- #{b} bug: Footer cut off | to check")), "{m}");
    assert!(!block(&m).contains(&format!("#{a} ")), "unchanged items aren't said again: {m}");

    // A new item.
    let c = w.hub().desk_add_with(&project, "task", "Dark mode", "", "", None, &[]).unwrap().id;
    let m = say(&w, &project, &d, "more?");
    assert!(block(&m).contains(&format!("Needs routing:\n")) && block(&m).contains(&format!("- #{c} task: Dark mode | new")), "{m}");

    // A session newly idle.
    let s = second(&w, "cue-late");
    let id: String = s.chars().take(8).collect();
    let m = say(&w, &project, &d, "who's free?");
    assert!(block(&m).contains(&format!("cue-late (sid:{id}, idle")), "{m}");

    // An item gone.
    w.hub().write(|st| st.desk.item_mut(&a).unwrap().state = "done".into());
    let m = say(&w, &project, &d, "done?");
    assert!(block(&m).contains(&format!("No longer listed: #{a} (done, dropped or moved)")), "{m}");
}

#[test]
fn the_item_a_message_is_about_comes_in_full() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let a = w.hub().desk_add_with(&project, "bug", "Header jumps", SEE, DONE_WHEN, None, &[]).unwrap().id;
    set_item(&w, &a, "to_check", Some(&h));
    let m = say(&w, &project, &d, "plain");
    assert!(!m.contains(DONE_WHEN), "{m}");
    let m = say(&w, &project, &d, &format!("Is #{a} right?"));
    assert!(block(&m).contains("In full, as this message is about them:") && block(&m).contains(DONE_WHEN), "{m}");
}

#[test]
fn the_whole_block_comes_every_twenty_messages_and_when_the_lead_asks() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let a = w.hub().desk_add_with(&project, "bug", "Header jumps", SEE, DONE_WHEN, None, &[]).unwrap().id;
    set_item(&w, &a, "to_check", Some(&h));
    say(&w, &project, &d, "first");
    let mut whole = 0;
    for i in 0..crate::desk::hub::lead::block::FULL_EVERY {
        if is_whole(&say(&w, &project, &d, &format!("m{i}"))) {
            whole += 1;
        }
    }
    assert_eq!(whole, 1, "one whole block in twenty");

    // SHOW: #<id> tells it the item in full; SHOW: desk sends the whole block at once.
    ends(&w, &d, &format!("SHOW: #{a}"));
    let told = last(&w, &d);
    assert!(told.contains(&format!("#{a} in full, as you asked:")) && told.contains(DONE_WHEN), "{told}");
    ends(&w, &d, "SHOW: desk");
    let told = last(&w, &d);
    assert!(told.contains("Your whole Desk block, as you asked") && !changes_only(&told) && told.contains("Other open items:"), "{told}");
    assert!(!is_whole(&say(&w, &project, &d, "after")));
}

#[test]
fn show_lines_are_read() {
    use crate::desk::parse::{lines, Line};
    assert_eq!(lines("SHOW: #12 #13"), vec![Line::Show { ids: vec!["12".into(), "13".into()], desk: false }]);
    assert_eq!(lines("SHOW: desk"), vec![Line::Show { ids: vec![], desk: true }]);
    assert!(lines("SHOW: me everything").is_empty());
}
