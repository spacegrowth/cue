//! An item whose holder's turn ended on it with no DONE: it waits on you (shown with the holder's last line), and Cue
//! asks the holder once, "Done? Write DONE: #N, or say what's left". The turn that answers isn't asked again, nor one
//! that ends on an ask; once a turn says what's left or what it waits on, none is while the item is unchanged.

use super::desk::{ask_of, at_work, crew_parts, Effects};
use super::*;
use crate::desk::{DeskItem, Holder};
use crate::sessions::Session;

/// The live session of `it`'s holder when its turn ended on `it` and it's at its prompt: its last reply came after
/// the item last went to it, and none of its crew is still at work. None for a crew member or a project's lead.
pub(super) fn stalled<'a>(st: &'a Store, it: &DeskItem, h: &Holder) -> Option<&'a Session> {
    // A crew member's turns aren't its lead's; a project's lead's turns are the Desk's routing.
    if crew_parts(h).is_some() || st.desk.lead_of(&h.sid).is_some() {
        return None;
    }
    let s = st.sessions.get(&h.sid).filter(|s| matches!(s.state.as_str(), "waiting" | "stopped") && !crew_at_work(s))?;
    let held = it.held_from.as_ref().filter(|(by, _)| *by == h.sid).map_or(it.updated * 1000, |(_, at)| *at);
    let from = st.desk.sent_at.get(&it.id).filter(|(by, _)| *by == h.sid).map_or(held, |(_, at)| held.max(*at));
    s.thread.iter().rev().find(|e| e.role == "agent").filter(|e| e.at_ms > from).map(|_| s)
}

/// One of session `s`'s crew is still at work: its turn ended, but the work handed to them goes on.
fn crew_at_work(s: &Session) -> bool {
    s.team.iter().any(|t| t.working())
}

/// Session `sid` (itself, not a crew member of it) is still at it as its turn ends: its background agents run on, or
/// one of its crew is at work. A DONE it says then waits (the item stays at work), and it isn't asked if it's done.
pub(super) fn still_at_it(st: &Store, sid: &str) -> bool {
    let h = Holder { kind: "session".into(), sid: sid.to_string(), ..Default::default() };
    at_work(st, &h) || st.sessions.get(sid).is_some_and(crew_at_work)
}

/// The last line of a session's last words.
pub(super) fn last_line(s: &Session) -> String {
    s.thread.iter().rev().find(|e| e.role == "agent").and_then(|e| e.text.lines().map(str::trim).rfind(|l| !l.is_empty())).unwrap_or("").chars().take(160).collect()
}

/// The items at work with session `sid` itself (not a crew member of it) since before this turn's end: the ones its
/// turn could stall on. None for a project's lead: its turns are the Desk's routing, not an item's work.
pub(super) fn at_work_with(st: &Store, sid: &str) -> Vec<String> {
    if st.desk.lead_of(sid).is_some() {
        return Vec::new();
    }
    st.desk
        .items
        .iter()
        .filter(|i| i.state == "working" && i.holder.as_ref().is_some_and(|h| h.sid == sid && h.lead_sid.is_empty()))
        .filter(|i| i.held_from.as_ref().is_some_and(|(by, _)| by == sid))
        .map(|i| i.id.clone())
        .collect()
}

/// What Cue asks a holder whose turn ended on item `id` with no DONE.
pub(super) fn stall_words(id: &str) -> String {
    format!("Done? Write DONE: #{id}, or say what's left")
}

/// The turn says what's left of its work, or what it waits on: nothing to ask it while its item is unchanged.
fn says_whats_left(text: &str) -> bool {
    let t = text.to_lowercase().replace('\u{2019}', "'");
    ["what's left", "left to do", "still to do", "remaining:", "not done yet", "blocked on", "waiting on", "waits on", "waiting for", "waits for"].iter().any(|w| t.contains(w))
}

/// The turn ends on a question for you: nothing to ask it this time.
fn ends_on_a_question(text: &str) -> bool {
    text.lines().map(str::trim).rfind(|l| !l.is_empty()).is_some_and(|l| l.ends_with('?'))
}

/// Session `sid`'s turn ended, saying `text`, with items `on` at work with it before: each still at work with it (no
/// DONE for it) stalled there, and Cue asks it once, unless it's still at it (its agents or crew run on), it asks you, or the
/// turn says what's left. A stall already asked of is answered by this turn: not asked again.
pub(super) fn stalls_asked(st: &mut Store, sid: &str, text: &str, on: &[String], fx: &mut Effects) {
    let still: Vec<(String, u64)> = on
        .iter()
        .filter_map(|id| st.desk.item(id).filter(|i| i.state == "working" && i.holder.as_ref().is_some_and(|h| h.sid == sid && h.lead_sid.is_empty())).map(|i| (i.id.clone(), i.updated)))
        .collect();
    // This holder's asks for items that aren't stalled with it now are over.
    st.desk.nudged.retain(|id, (by, _)| by != sid || still.iter().any(|(s, _)| s == id));
    st.desk.left_said.retain(|id, (by, _)| by != sid || still.iter().any(|(s, _)| s == id));
    if still.is_empty() {
        return;
    }
    let h = Holder { kind: "session".into(), sid: sid.to_string(), ..Default::default() };
    let left = says_whats_left(text);
    let quiet = still_at_it(st, sid) || ask_of(st, &h).is_some() || ends_on_a_question(text);
    let mut ask = Vec::new();
    for (id, updated) in still {
        // It said what's left of it, or what it waits on: quiet on it until the item changes (sent again, routed; #168).
        if st.desk.left_said.get(&id).is_some_and(|(by, at)| by == sid && *at == updated) {
            continue;
        }
        if left {
            st.desk.left_said.insert(id.clone(), (sid.to_string(), updated));
            st.desk.nudged.remove(&id);
            continue;
        }
        // Asked of this stall already: this turn is its answer.
        if st.desk.nudged.get(&id).is_some_and(|(_, at)| *at == updated) {
            st.desk.nudged.remove(&id);
            continue;
        }
        if quiet {
            continue;
        }
        st.desk.nudged.insert(id.clone(), (sid.to_string(), updated));
        ask.push(stall_words(&id));
    }
    if !ask.is_empty() {
        fx.sends.push((sid.to_string(), ask.join("\n")));
    }
}
