//! The Desk's hand-off of a session grown too big: one with a part on a Desk with a live lead whose context reached
//! the Desk's mark (config::desk_handoff_tokens), or whose compacting failed, is asked for a hand-off note
//! (cache_tick.rs full_due, hand_off.rs). Once the ask's turn ends it goes on in a fresh session, started from the
//! note (from its last reply when it wrote none), which takes its place on the Desk: the lead, the items it holds,
//! the Desk's list. Each step is on the Desk, so nothing is handed off silently.

use crate::desk::hub::core::desk::{decide, secs, shown_name, Effects};
use crate::desk::hub::prelude::*;

impl Hub {
    /// Session `sid`'s turn ended (its log at `path`): when it was the Desk's ask for a note (AHEAD_FULL), it goes on
    /// in a fresh session a moment later (desk_hand_on_now).
    pub(crate) fn desk_hand_on(&self, sid: &str, path: &str) {
        let Some(k) = crate::db::handoff(sid).filter(|k| k.ahead == crate::hub::AHEAD_FULL && k.next.is_empty()) else { return };
        // The ask's own turn: the latest prompt in its log is Cue's ask.
        if path.is_empty() || !crate::steps::last_prompt(path).1 {
            return;
        }
        let (sid, noted) = (sid.to_string(), !k.note.is_empty());
        self.later(1500, move |h| h.desk_hand_on_now(&sid, noted));
    }

    /// Session `sid` goes on in a fresh session: from its note (`noted`), else from its last reply (a session whose
    /// context is too big may not manage a note). Refused (it couldn't start): the Desk says so.
    fn desk_hand_on_now(self: &Arc<Self>, sid: &str, noted: bool) {
        let r = if noted { crate::hub::handoff_continue_now(self, sid) } else { crate::hub::handoff_fresh_now(self, sid) };
        match r {
            Ok(v) => {
                // Linked as a hand-off that continued, note or not: Cue doesn't ask it again.
                if let Some(new) = v["session_id"].as_str().filter(|n| !n.is_empty() && !noted) {
                    crate::db::handoff_continued(sid, new);
                }
            }
            Err(e) => {
                crate::hub::record_insight(sid, "ahead", "held", &e, 0);
                self.desk_note(sid, "Couldn't hand {name} off to a fresh session", &e);
            }
        }
    }

    /// A step by Cue on session `sid`'s Desk (`text`'s "{name}": its name), on each item it holds (on the Desk itself
    /// when it holds none): nothing to undo.
    fn desk_note(&self, sid: &str, text: &str, why: &str) {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            let Some((project, name)) = st.sessions.get(sid).map(|s| (s.project.clone(), shown_name(s))) else { return fx };
            let text = text.replace("{name}", &name);
            for scope in st.desk.scopes(sid, &project) {
                for item in held_items(st, &scope, sid) {
                    note(st, &scope, item, &text, why, now, &mut fx);
                }
            }
            fx
        });
        self.desk_done(fx);
    }

    /// Session `old` goes on as `new` (named `name`; a hand-off, or a fresh start from its last reply): `new` takes its
    /// place on its project's Desk. The items it holds (or its crew does) are `new`'s, their work counted from when
    /// they went to `old`; `new` joins the Desk's list; a lead's Desk is `new`'s to run, and it's sent it. Each item
    /// says it was handed off, a call on the Desk's list.
    pub(crate) fn desk_carry(self: &Arc<Self>, old: &str, new: &str, name: &str) {
        let now = now_ms();
        let (fx, lead) = self.write(|st| {
            let mut fx = Effects::default();
            st.desk.run.compact_failed.remove(old);
            let Some(s) = st.sessions.get(old) else { return (fx, None) };
            let (project, old_name) = (s.project.clone(), shown_name(s));
            let name = if name.is_empty() { st.sessions.get(new).map(shown_name).unwrap_or_else(|| new.chars().take(8).collect()) } else { name.to_string() };
            let lead = st.desk.lead_of(old).map(|id| (id, name.clone()));
            for project in st.desk.scopes(old, &project) {
                let held = held_items(st, &project, old);
                let on_desk = st.desk.on_desk(&project, old) || lead.is_some() || held.iter().any(Option::is_some);
                if !on_desk {
                    continue;
                }
                crate::desk::hub::core::members::join(st, &project, new, &mut fx);
                let text = format!("Handed {old_name} off to {name}");
                let why = "a fresh session, from its hand-off";
                for item in held {
                    if let Some(it) = item.as_deref().and_then(|id| st.desk.item_mut(id)) {
                        it.remember_holder();
                        if let Some(h) = it.holder.as_mut() {
                            if h.lead_sid == old {
                                h.lead_sid = new.to_string();
                            } else if h.sid == old {
                                (h.sid, h.name) = (new.to_string(), name.clone());
                            }
                        }
                        // Its work so far counts for the one that carries on.
                        if let Some((by, _)) = it.held_from.as_mut().filter(|(by, _)| by == old) {
                            *by = new.to_string();
                        }
                        for (by, _, _) in it.held_before.iter_mut().filter(|(by, _, _)| by == old) {
                            *by = new.to_string();
                        }
                        it.updated = secs(now);
                        fx.items.push(it.id.clone());
                    }
                    note(st, &project, item, &text, why, now, &mut fx);
                    if let Some(d) = st.desk.decisions.last_mut() {
                        d.call = true;
                    }
                }
            }
            (fx, lead)
        });
        self.desk_done(fx);
        if let Some((project, name)) = lead {
            if let Err(e) = crate::desk::hub::lead::lead_carried_on(self, &project, new, &name) {
                eprintln!("cue: desk: {project}'s new lead wasn't sent the Desk: {e}");
            }
        }
    }
}

/// The items session `sid` holds on `project`'s Desk (itself, or its crew), not done; one None when it holds none
/// (a step then goes on the Desk itself).
fn held_items(st: &Store, project: &str, sid: &str) -> Vec<Option<String>> {
    let held: Vec<Option<String>> = st.desk.items.iter().filter(|i| i.project == project && i.state != "done" && i.holder.as_ref().is_some_and(|h| h.is(sid))).map(|i| Some(i.id.clone())).collect();
    if held.is_empty() {
        vec![None]
    } else {
        held
    }
}

/// A step by Cue: nothing to undo.
#[allow(clippy::too_many_arguments)]
fn note(st: &mut Store, project: &str, item: Option<String>, text: &str, why: &str, now: u64, fx: &mut Effects) {
    decide(st, project, item, "Cue", "", text, why, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.undo = "none".into();
    }
}
