//! Pausing a whole Desk (#366): Pause stops every session on it at a safe point and keeps queued items from
//! starting; Resume picks everything back up. The owner presses it on the Desk; its lead writes PAUSE: / RESUME:.
//! Kept per Desk in config.json (`desk.paused`: { <desk id>: { since, by, stopped, held, starts } }), so a pause
//! holds across a restart.
//!
//! A safe point: each session at work is sent the pause as a message Claude Code takes in between two steps (as
//! "Send now" does), so it finishes the step it's on, leaves its work as it is in its copy and ends its turn. Nothing
//! is pressed Esc: a command or an edit is never cut off half way. While paused, what Cue would send a Desk session
//! (an item routed to it, a note, a stall's "Done?", a usage limit's carry-on) is held, and new sessions for items
//! aren't started; the lead itself isn't paused, so it can plan and RESUME. Resume sends what was held, tells each
//! session it stopped to carry on, and starts the items that were queued after ones finished meanwhile.

use crate::desk::hub::prelude::*;
use crate::desk::hub::core::desk::{secs, Effects, NewStart};
use crate::desk::hub::turn::route::release_kept;

/// What a session at work on the Desk is told as it pauses.
pub(crate) const PAUSE_WORDS: &str = "The Desk is paused. Stop at the next safe point: finish the step you're on, leave your work as it is in your copy (don't commit, push or undo anything, and don't say DONE), say in one line where you stopped, and end your turn. Cue tells you when to go on.";

/// Desk `desk`'s saved pause (Null when it isn't paused).
fn saved(desk: &str) -> Value {
    crate::config::load().pointer("/desk/paused").and_then(|m| m.get(desk)).cloned().unwrap_or(Value::Null)
}

/// Whether Desk `desk` is paused.
pub(crate) fn is_paused(desk: &str) -> bool {
    saved(desk).is_object()
}

/// The paused Desks whose sessions (not their lead) `sid` is one of: on it, or holding one of its items.
fn paused_desks_of(st: &Store, sid: &str) -> Vec<String> {
    let all = crate::config::load().pointer("/desk/paused").and_then(Value::as_object).map(|m| m.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    all.into_iter()
        .filter(|desk| st.desk.lead_sid(desk) != Some(sid))
        .filter(|desk| st.desk.on_desk(desk, sid) || st.desk.items.iter().any(|i| &i.project == desk && i.state != "done" && i.holder.as_ref().is_some_and(|h| h.to() == sid)))
        .collect()
}

/// Whether what Cue would send session `sid` now waits for a Resume (it's a session of a paused Desk).
pub(crate) fn holds(st: &Store, sid: &str) -> bool {
    !paused_desks_of(st, sid).is_empty()
}

/// `state.desk.projects[i].paused`: { on, since, by, held } (held: messages and starts waiting for Resume).
pub(crate) fn pause_view(desk: &str) -> Value {
    let v = saved(desk);
    if !v.is_object() {
        return json!({ "on": false });
    }
    let n = |k: &str| v.get(k).and_then(Value::as_array).map_or(0, Vec::len);
    json!({ "on": true, "since": v.get("since").cloned().unwrap_or(json!(0)), "by": v.get("by").cloned().unwrap_or(json!("")), "held": n("held") + n("starts") })
}

/// The "Paused" part of Desk `desk`'s summary for its lead, or None while it runs.
pub(crate) fn pause_block(desk: &str, now: u64) -> Option<String> {
    let v = saved(desk);
    v.is_object().then(|| {
        let since = v.get("since").and_then(Value::as_u64).unwrap_or(now);
        let by = v.get("by").and_then(Value::as_str).unwrap_or("the owner");
        format!("The Desk is paused (by {by}, {}). Its sessions stopped at a safe point; their work is kept in their copies. Routes, notes and new sessions you make now are held, and queued items don't start, until it resumes. Write RESUME: why: <reason> to pick everything back up (only when the owner asked you to pause or would want it resumed).\n", crate::usage::when(since, now))
    })
}

/// Whether item `id` is over for what's queued after it: done (Looks right, #576), landed where its Desk starts them
/// then (#808), or gone.
fn over(st: &Store, id: &str) -> bool {
    crate::desk::hub::turn::route::let_go(st, id)
}

impl Hub {
    /// Pause (`on`) or resume Desk `desk`, as `by` (name, session: "" for you) decided, with `why`. Ok(false): it
    /// already was so.
    pub(crate) fn desk_pause_set(&self, desk: &str, on: bool, by: (&str, &str), why: &str) -> Result<bool, String> {
        if desk.trim().is_empty() || !self.read(|st| st.desk.lead.contains_key(desk) || st.desk.items.iter().any(|i| i.project == desk)) {
            return Err("no such Desk".into());
        }
        let now = now_ms();
        if on {
            if !self.desk_pause_on(desk, by.0, now)? {
                return Ok(false);
            }
        } else {
            let was = crate::config::update_with("desk.paused", |c| {
                let mut map = c.pointer("/desk/paused").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
                let Some(was) = map.as_object_mut().and_then(|m| m.remove(desk)) else { return Ok((None, None)) };
                Ok((Some(map), Some(was)))
            })?;
            let Some(was) = was else { return Ok(false) };
            self.desk_resumed(desk, &was, now);
        }
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            let text = if on { "Paused the Desk" } else { "Resumed the Desk" };
            crate::desk::hub::core::desk::decide(st, desk, None, by.0, by.1, text, why, now, &mut fx);
            if let Some(d) = st.desk.decisions.last_mut() {
                d.undo = "none".into();
            }
            fx
        });
        self.desk_done(fx);
        // Its lead hears of it, unless it said it itself.
        let lead = self.read(|st| st.desk.lead_sid(desk).map(str::to_string).filter(|l| l != by.1 && st.sessions.get(l).is_some_and(|s| s.state != "limited")));
        if let Some(lead) = lead {
            let words = if on { pause_block(desk, now).unwrap_or_default() } else { "The Desk is resumed: its sessions carry on and what was held is sent. Route as usual.".to_string() };
            self.later(0, move |h| {
                if let Err(e) = h.tell(&lead, &words) {
                    eprintln!("cue: desk: couldn't tell {lead} about the pause: {e}");
                }
            });
        }
        self.changed();
        Ok(true)
    }

    /// Pause Desk `desk` as `by` decided: its sessions at work are told to stop at a safe point. Ok(false): it
    /// already was paused.
    fn desk_pause_on(&self, desk: &str, by: &str, now: u64) -> Result<bool, String> {
        // Its sessions at work on its items now (not its lead): each told to stop at a safe point.
        let stopped: Vec<String> = self.read(|st| {
            let lead = st.desk.lead_sid(desk).unwrap_or("");
            let mut out: Vec<String> = Vec::new();
            for it in st.desk.items.iter().filter(|i| i.project == desk && i.state == "working" && i.paused.is_none()) {
                let Some(to) = it.holder.as_ref().map(|h| h.to().to_string()) else { continue };
                let working = st.sessions.get(&to).is_some_and(|s| s.state == "working");
                if to != lead && working && !out.contains(&to) {
                    out.push(to);
                }
            }
            out
        });
        let fresh = crate::config::update_with("desk.paused", |c| {
            let mut map = c.pointer("/desk/paused").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
            if map.get(desk).is_some_and(Value::is_object) {
                return Ok((None, false));
            }
            map[desk] = json!({ "since": now, "by": by, "stopped": stopped, "held": [], "starts": [] });
            Ok((Some(map), true))
        })?;
        if !fresh {
            return Ok(false);
        }
        for sid in stopped {
            self.later(0, move |h| {
                if let Err(e) = h.tell_now(&sid, PAUSE_WORDS, true) {
                    eprintln!("cue: desk: couldn't pause {sid}: {e}");
                }
            });
        }
        Ok(true)
    }

    /// Desk `desk` resumed from its saved pause `was`: what was held is sent, sessions it stopped are told to carry
    /// on, items held from starting start, and items queued after ones that finished meanwhile start.
    fn desk_resumed(&self, desk: &str, was: &Value, now: u64) {
        let held: Vec<(String, String)> = was.get("held").and_then(Value::as_array).into_iter().flatten()
            .filter_map(|p| Some((p.get(0)?.as_str()?.to_string(), p.get(1)?.as_str()?.to_string()))).collect();
        let stopped: Vec<String> = was.get("stopped").and_then(Value::as_array).into_iter().flatten().filter_map(|s| s.as_str().map(str::to_string)).collect();
        let starts: Vec<NewStart> = was.get("starts").and_then(Value::as_array).into_iter().flatten().filter_map(|s| {
            let f = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            (!f("id").is_empty()).then(|| NewStart { project: f("project"), id: f("id"), why: f("why"), by: f("by"), by_sid: f("by_sid") })
        }).collect();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            // A pause doesn't count toward an item's 20 minutes for its session (#576): its clock moves on by the
            // pause, or to now for one queued during it.
            let gap = now.saturating_sub(was.get("since").and_then(Value::as_u64).unwrap_or(now));
            for it in st.desk.items.iter_mut().filter(|i| i.project == desk && i.state == "queued" && i.holder.is_none() && i.next_for.is_some()) {
                it.queued_at = (it.queued_at + gap).min(now);
            }
            // Queued after an item that finished while paused: they start now.
            let mut afters: Vec<String> = st.desk.items.iter().filter(|i| i.project == desk && i.state == "queued" && !i.tree_wait).filter_map(|i| i.after.clone()).collect();
            afters.dedup();
            afters.retain(|a| over(st, a));
            for a in afters {
                release_kept(st, &a, now, &mut fx);
            }
            fx.sends.extend(held);
            for sid in stopped {
                if fx.sends.iter().any(|(to, _)| *to == sid) {
                    continue;
                }
                let ids: Vec<String> = st.desk.items.iter().filter(|i| i.project == desk && i.state == "working" && i.paused.is_none() && i.holder.as_ref().is_some_and(|h| h.to() == sid)).map(|i| format!("#{}", i.id)).collect();
                if !ids.is_empty() {
                    fx.sends.push((sid, format!("The Desk is resumed: carry on with {} from where you stopped.", ids.join(", "))));
                }
            }
            // Items held from starting: still waiting for a session (nobody took them meanwhile).
            for s in starts {
                if st.desk.item(&s.id).is_some_and(|i| i.holder.is_none() && matches!(i.state.as_str(), "new" | "queued")) {
                    if let Some(it) = st.desk.item_mut(&s.id) {
                        it.updated = secs(now);
                    }
                    fx.starts.push(s);
                }
            }
            fx
        });
        self.desk_done(fx);
    }

    /// What a Desk change would send or start, with its sessions' Desk paused: kept for its Resume instead. Leaves in
    /// `fx` only what goes now.
    pub(crate) fn desk_pause_hold(&self, fx: &mut Effects) {
        let paused = crate::config::load().pointer("/desk/paused").and_then(Value::as_object).is_some_and(|m| !m.is_empty());
        if !paused {
            return;
        }
        let (held, sends): (Vec<_>, Vec<_>) = self.read(|st| std::mem::take(&mut fx.sends).into_iter().map(|(sid, text)| (paused_desks_of(st, &sid), (sid, text))).partition(|(desks, _)| !desks.is_empty()));
        fx.sends = sends.into_iter().map(|(_, s)| s).collect();
        let (late, starts): (Vec<NewStart>, Vec<NewStart>) = std::mem::take(&mut fx.starts).into_iter().partition(|s| is_paused(&s.project));
        fx.starts = starts;
        if held.is_empty() && late.is_empty() {
            return;
        }
        let saved = crate::config::update_with("desk.paused", |c| {
            let mut map = c.pointer("/desk/paused").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
            for (desks, (sid, text)) in &held {
                // One copy, with the first paused Desk it's on: Resume of that one sends it.
                if let Some(list) = desks.first().and_then(|d| map.get_mut(d)).and_then(|d| d.get_mut("held")).and_then(Value::as_array_mut) {
                    list.push(json!([sid, text]));
                }
            }
            for s in &late {
                if let Some(list) = map.get_mut(&s.project).and_then(|d| d.get_mut("starts")).and_then(Value::as_array_mut) {
                    list.push(json!({ "project": s.project, "id": s.id, "why": s.why, "by": s.by, "by_sid": s.by_sid }));
                }
            }
            Ok((Some(map), ()))
        });
        if let Err(e) = saved {
            eprintln!("cue: desk: couldn't hold messages for a paused Desk: {e}");
        }
    }
}
