//! "What the lead moved" (#374, db/desk.rs desk_moves): every move the lead made on its own (closed, started, sent
//! back, moved up, took back a copy, answered a session) is kept, newest first, with its item, why and time, past
//! the days decisions are kept; older ones load a page at a time; it stays until you clear it.

use super::desk::desk;
use super::desk_commit::{ends, item, routed_with};
use crate::testing::World;

#[test]
fn the_leads_own_moves_are_kept_newest_first_page_by_page_until_you_clear_them() {
    let w = World::new();
    let (_, project, id, _) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&super::desk_unstick::top(w), &[1, 2, 3]), |_, _, _| {});
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "queued");
    ends(&w, &lead, &format!("START: #{id} | why: the user needs it now"));
    w.advance(2000);
    w.settle();
    // Sent back to the session that has it, with what's left: a move too.
    ends(&w, &lead, &format!("ROUTE: #{id} -> cue-teams | why: the header still jumps on scroll"));
    w.advance(2000);
    w.settle();
    let moves = w.hub().desk_moves_page(&project, None, 10);
    let got: Vec<(&str, &str, &str)> = moves.iter().map(|m| (m.act.as_str(), m.item_id.as_deref().unwrap_or(""), m.why.as_str())).collect();
    assert_eq!(got, vec![("send_back", id.as_str(), "the header still jumps on scroll"), ("start", id.as_str(), "the user needs it now")], "newest first, each with its item and why");
    assert!(moves[0].at >= moves[1].at && moves[1].at > 0, "each with its time");
    // Older ones a page at a time: after the newest, the next.
    let next = w.hub().desk_moves_page(&project, Some((moves[0].at * 1000, moves[0].id.as_str())), 1);
    assert_eq!(next.iter().map(|m| m.act.as_str()).collect::<Vec<_>>(), vec!["start"]);
    // Lasting: long after decisions are let go, the moves are still listed.
    crate::db::desk_load(crate::clock::now_ms() + 90 * 24 * 60 * 60 * 1000);
    assert_eq!(w.hub().desk_moves_page(&project, None, 10).len(), 2, "kept past the decisions' 30 days");
    // Cleared: the list is empty, and the Desk says from when.
    w.hub().desk_moves_clear(&project).unwrap();
    assert!(w.hub().desk_moves_page(&project, None, 10).is_empty());
    assert!(desk(&w, &project)["moves_cleared"].as_u64().unwrap() > 0);
}
