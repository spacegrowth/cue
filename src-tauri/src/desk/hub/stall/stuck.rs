//! A Desk item's holder stuck mid-turn (#502): marked working but with no new output for `SILENT_MS`, or sitting on a
//! permission prompt nobody answered for `PROMPT_MS`. Its project's lead is told in its chat, with the item and what
//! it's stuck on, once per stuck spell; the spell is over (and a later one told again) once the session moves.
//! Output is anything it writes: its conversation, its steps, and its background commands and agents (a long
//! build that still prints isn't stuck). A turn that ended is desk/hub/stall/stall.rs's; a crew member's quiet is team.rs's.

use crate::hub::activity::written_ms;
use crate::desk::hub::core::desk::{ask_of, crew_parts, Effects};
use crate::desk::hub::stall::stall::lead_live;
use crate::desk::hub::prelude::*;
use crate::desk::Holder;

/// No new output for this long while marked working: stuck.
pub(crate) const SILENT_MS: u64 = 20 * 60_000;
/// A permission prompt unanswered this long: stuck on it.
pub(crate) const PROMPT_MS: u64 = 5 * 60_000;

/// One working item's holder as read from the store: what may show it's stuck.
struct Held {
    id: String,
    title: String,
    sid: String,
    name: String,
    /// "working" or "deciding".
    state: String,
    /// Its conversation's file ("" when it has none to read).
    path: String,
    /// Its latest signs of life Cue holds: its turn's start, its latest step, its last words.
    seen: u64,
    doing: String,
    /// Handed to helpers Cue can't read the output of (an Agent call under way), or its crew at work: theirs to watch.
    helped: bool,
    /// The prompt it sits on (not a question for you): (its id, since when, what it asks to do).
    prompt: Option<(String, u64, String)>,
    told: Option<String>,
}

/// What a stuck holder is stuck on: the spell (told once), and what the lead is told.
struct Stuck {
    spell: String,
    words: String,
}

impl Hub {
    /// Each second's look (server::tick): every Desk item at work whose holder is stuck, its lead told once a spell.
    pub(crate) fn desk_stuck_tick(&self) {
        self.desk_stuck_check(now_ms());
    }

    /// The stuck holders as of `now` (the tick's work).
    pub(crate) fn desk_stuck_check(&self, now: u64) {
        let held = self.read(|st| holders(st));
        if held.is_empty() && self.read(|st| st.desk.run.stuck_told.is_empty()) {
            return;
        }
        // Files looked at with the store let go (a stat or a log read can be slow).
        let stuck: Vec<(String, String, Option<Stuck>)> = held.iter().map(|h| (h.id.clone(), h.sid.clone(), stuck_on(h, now))).collect();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            // Items no longer at work with that holder: their spell is over.
            st.desk.run.stuck_told.retain(|id, (sid, _)| stuck.iter().any(|(i, s, _)| i == id && s == sid));
            for (id, sid, s) in &stuck {
                let Some(s) = s else {
                    // Moving again: cleared, so a later spell is told.
                    st.desk.run.stuck_told.remove(id);
                    continue;
                };
                if st.desk.run.stuck_told.get(id).is_some_and(|(by, spell)| by == sid && *spell == s.spell) {
                    continue;
                }
                let Some(project) = st.desk.item(id).map(|i| i.project.clone()) else { continue };
                let Some(lead) = st.desk.lead_sid(&project).filter(|l| *l != sid.as_str() && lead_live(st, &project)).map(str::to_string) else { continue };
                st.desk.run.stuck_told.insert(id.clone(), (sid.clone(), s.spell.clone()));
                fx.sends.push((lead, s.words.clone()));
            }
            fx
        });
        self.desk_done(fx);
    }
}

/// The items at work now with a session of their own (not a crew member, not a project's lead) that's mid-turn or on
/// a prompt, on a Desk that isn't paused, the item not paused.
fn holders(st: &Store) -> Vec<Held> {
    st.desk
        .items
        .iter()
        .filter(|i| i.state == "working" && i.paused.is_none() && !crate::desk::hub::away::pause::is_paused(&i.project))
        .filter_map(|i| {
            let h = i.holder.as_ref().filter(|h| crew_parts(h).is_none() && h.lead_sid.is_empty())?;
            if st.desk.lead_of(&h.sid).is_some() {
                return None;
            }
            let s = st.sessions.get(&h.sid).filter(|s| matches!(s.state.as_str(), "working" | "deciding"))?;
            let said = s.thread.iter().rev().find(|e| e.role == "agent").map_or(0, |e| e.at_ms);
            let doing = if s.state == "working" { s.doing.clone() } else { String::new() };
            let seen = s.since_ms.max(if doing.is_empty() { 0 } else { s.doing_ms }).max(said);
            let helped = doing.starts_with("Delegating") || s.team.iter().any(|t| t.working());
            let prompt = prompt_of(st, h);
            Some(Held {
                id: i.id.clone(),
                title: i.title.clone(),
                sid: h.sid.clone(),
                name: crate::desk::hub::core::desk::shown_name(s),
                state: s.state.clone(),
                path: s.origin.transcript_path.clone(),
                seen,
                doing,
                helped,
                prompt,
                told: st.desk.run.stuck_told.get(&i.id).filter(|(by, _)| *by == h.sid).map(|(_, spell)| spell.clone()),
            })
        })
        .collect()
}

/// The prompt holder `h` sits on, if it's one asking leave (a tool, a command), not a question for you.
fn prompt_of(st: &Store, h: &Holder) -> Option<(String, u64, String)> {
    let a = ask_of(st, h).filter(|a| a.kind != "question")?;
    let what = crate::hub::summary(a);
    let what = if a.tool_name.is_empty() || what == a.tool_name { what } else { format!("{}: {what}", a.tool_name) };
    Some((a.id.clone(), a.created_ms, what.chars().take(160).collect()))
}

/// Whether holder `h` is stuck as of `now`, and on what.
fn stuck_on(h: &Held, now: u64) -> Option<Stuck> {
    if h.state == "deciding" {
        let (ask, since, what) = h.prompt.as_ref()?;
        let waited = now.saturating_sub(*since);
        if waited < PROMPT_MS {
            return None;
        }
        return Some(Stuck { spell: format!("prompt:{ask}"), words: prompt_words(h, what, waited) });
    }
    if h.helped {
        return None;
    }
    // Its newest output: what the store holds, its conversation's file, and (only when those are quiet) what its
    // background commands and agents still running last wrote.
    let mut last = h.seen;
    if !h.path.is_empty() {
        last = last.max(written_ms(&h.path).unwrap_or(0));
        if now.saturating_sub(last) >= SILENT_MS {
            last = last.max(crate::steps::background_heard_ms(&h.path));
        }
    }
    let quiet = now.saturating_sub(last);
    if quiet < SILENT_MS || last == 0 {
        return None;
    }
    // A spell already told keeps its key while nothing new came (the minutes go on; it isn't told again).
    let spell = format!("silent:{last}");
    if h.told.as_deref() == Some(spell.as_str()) {
        return Some(Stuck { spell, words: String::new() });
    }
    Some(Stuck { spell, words: silent_words(h, quiet) })
}

/// What the lead is told of a holder silent for `quiet` ms.
fn silent_words(h: &Held, quiet: u64) -> String {
    let last = if h.doing.trim().is_empty() { String::new() } else { format!(" Its last step: {}.", crate::desk::hub::stall::stall::act_words(&h.doing, 0, false).now) };
    format!(
        "#{id} ({title}) looks stuck: {name} has shown no new output for {m} minutes while marked working on it.{last}\n\
         Step in: look at its chat, then nudge it (NOTE: #{id} | …), PAUSE: #{id} or ROUTE it on; only if the user must \
         decide, ASK: #{id} | <question> | choices: … Cue tells you again only if it goes quiet again after moving.",
        id = h.id,
        title = h.title,
        name = h.name,
        m = quiet / 60_000,
    )
}

/// What the lead is told of a holder that has sat `waited` ms on a prompt asking to do `what`.
fn prompt_words(h: &Held, what: &str, waited: u64) -> String {
    format!(
        "#{id} ({title}) is stuck on a permission prompt: {name} has waited {m} minutes for leave to run {what}, and \
         nobody has answered it.\nStep in: if it's safe and the user should allow it, ASK: #{id} | Allow {name} to run \
         that? | choices: Allow / Deny; if the work can go another way, NOTE: #{id} | … or ROUTE it on.",
        id = h.id,
        title = h.title,
        name = h.name,
        m = waited / 60_000,
    )
}
