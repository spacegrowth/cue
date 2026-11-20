//! A Desk session's question about clearing the way for a landing (#349): leftover edits, stray copies, uncommitted
//! files in the way. It goes to its project's Desk lead first, not to you: the lead answers it (`ANSWER: #<id> | …`)
//! or hands it on (`PASS: #<id> | why`). You see it only then, or once the lead has had it `STOP_WAIT_MS` without a
//! word.

use crate::hub::{Hub, Store};
use crate::model::{Decision, Item};
use serde_json::{json, Value};

/// Words that say something's in the way, and words that say it's about landing: a question needs one of each.
/// SHORTCUT: read from its words, so a clean-up question worded otherwise still comes to you (as before #349); if
/// that happens often, have Cue's own landing ask its holder in a set form instead.
const IN_THE_WAY: &[&str] = &["leftover", "left-over", "left over", "stray", "uncommitted", "untracked", "not mine", "another session", "other session", "dirty", "stale cop", "extra cop"];
const LANDING: &[&str] = &["land", "commit", "main", "merge", "rebase", "push"];

/// Its one question's words, header and choices, lower case.
fn words(it: &Item) -> String {
    let q = &it.tool_input["questions"][0];
    let mut out = format!("{} {}", q["question"].as_str().unwrap_or(""), q["header"].as_str().unwrap_or(""));
    for o in q["options"].as_array().into_iter().flatten() {
        out.push_str(&format!(" {} {}", o["label"].as_str().unwrap_or(""), o["description"].as_str().unwrap_or("")));
    }
    out.to_lowercase()
}

/// Whether card `it` asks how to clear the way for a landing.
pub(crate) fn is_cleanup(it: &Item) -> bool {
    let w = words(it);
    IN_THE_WAY.iter().any(|x| w.contains(x)) && LANDING.iter().any(|x| w.contains(x))
}

/// Where card `it` goes first: (the Desk item its session is on, that Desk's lead's session), for a landing clean-up
/// question (one) from a session holding an item whose lead is live. None: it's yours, as any card.
pub(crate) fn lead_for(st: &Store, it: &Item) -> Option<(String, String)> {
    if it.kind != "question" || !it.agent_id.is_empty() || it.desk || it.tool_input["questions"].as_array().is_none_or(|q| q.len() != 1) {
        return None;
    }
    let sid = it.origin.session_id.as_str();
    if st.desk.lead_of(sid).is_some() {
        return None;
    }
    let holds = |i: &&crate::desk::DeskItem| i.state != "done" && i.holder.as_ref().is_some_and(|h| h.sid == sid && h.lead_sid.is_empty());
    let on = crate::desk::hub::stall::stall::now_on(st, sid).and_then(|id| st.desk.items.iter().find(|i| i.id == id));
    let held = on.or_else(|| st.desk.items.iter().find(holds))?;
    let lead = st.desk.lead_sid(&held.project)?;
    (crate::desk::hub::stall::stall::lead_live(st, &held.project) && is_cleanup(it)).then(|| (held.id.clone(), lead.to_string()))
}

/// Card `it`, about to open, given to its Desk lead first when it's one for it (`lead_for`): marked so, and (the
/// lead's session, what it's told).
pub(crate) fn give_lead(st: &Store, it: &mut Item) -> Option<(String, String)> {
    let (item, lead) = lead_for(st, it)?;
    let name = st.sessions.get(&it.origin.session_id).map(crate::desk::hub::core::desk::shown_name).unwrap_or_default();
    let words = lead_words(it, &name, &item);
    it.lead_has = Some(item);
    Some((lead, words))
}

/// What the lead is told of card `it`, from the session named `name`, on item `id`.
pub(crate) fn lead_words(it: &Item, name: &str, id: &str) -> String {
    let q = &it.tool_input["questions"][0];
    let mut out = format!("{name}, on #{id}, asks about clearing the way for its landing (it's waiting on the answer):\n{}", q["question"].as_str().unwrap_or(""));
    for (n, o) in q["options"].as_array().into_iter().flatten().enumerate() {
        let d = o["description"].as_str().unwrap_or("");
        out.push_str(&format!("\n{}. {}{}", n + 1, o["label"].as_str().unwrap_or(""), if d.is_empty() { String::new() } else { format!(": {d}") }));
    }
    let mins = crate::desk::hub::stall::stall::STOP_WAIT_MS / 60_000;
    out.push_str(&format!(
        "\nAnswer it yourself: ANSWER: #{id} | <a choice's number, or your own words>. Only if the user must decide: PASS: #{id} | <why>. Without either in {mins} minutes, it goes to the user."
    ));
    out
}

/// The open card of item `id` (of `project`) its lead has.
pub(crate) fn card_for(st: &Store, project: &str, id: &str) -> Option<String> {
    st.desk.item(id).filter(|i| i.project == project)?;
    st.items.iter().find(|i| i.status == "pending" && i.lead_has.as_deref() == Some(id)).map(|i| i.id.clone())
}

/// The lead's answer `text` to card `it`, as the card's answer: a choice's number or label is that choice, other
/// words go as they are.
fn decision(it: &Item, text: &str) -> Decision {
    let q = &it.tool_input["questions"][0];
    let labels: Vec<&str> = q["options"].as_array().into_iter().flatten().filter_map(|o| o["label"].as_str()).collect();
    let t = text.trim().trim_end_matches('.').trim();
    let picked = t.trim_start_matches('#').parse::<usize>().ok().and_then(|n| labels.get(n.wrapping_sub(1)).copied()).or_else(|| labels.iter().find(|l| l.eq_ignore_ascii_case(t)).copied());
    let answer = picked.map(str::to_string).unwrap_or_else(|| text.trim().to_string());
    let question = q["question"].as_str().unwrap_or("").to_string();
    Decision { behavior: "allow".into(), message: None, answers: Some(Value::Object([(question, json!(answer))].into_iter().collect())), permission: None }
}

impl Hub {
    /// Card `id` just given to its lead: the lead is told; yours if it isn't answered in time.
    pub(crate) fn cleanup_to_lead(&self, id: &str, lead: String, words: String) {
        self.later(0, move |h| {
            if let Err(e) = h.tell(&lead, &words) {
                eprintln!("cue: desk: couldn't give the lead a question: {e}");
            }
        });
        let card = id.to_string();
        self.later(crate::desk::hub::stall::stall::STOP_WAIT_MS, move |h| h.cleanup_to_you(&card));
    }

    /// The lead answered card `id` (ANSWER:): its answer goes to the session as the card's.
    pub(crate) fn cleanup_answered(&self, id: &str, text: &str) {
        let Some(d) = self.read(|st| st.items.iter().find(|i| i.id == id && i.status == "pending").map(|i| decision(i, text))) else { return };
        if !self.respond(id, d) {
            eprintln!("cue: desk: the lead's answer to {id} reached no one");
        }
    }

    /// Card `id` the lead passed on (PASS:), or didn't answer in time: it's yours now, shown and told like any card.
    pub(crate) fn cleanup_to_you(&self, id: &str) {
        let title = self.write(|st| {
            let it = st.items.iter_mut().find(|i| i.id == id && i.status == "pending" && i.lead_has.is_some())?;
            it.lead_has = None;
            Some(crate::hub::asks::notify_title(it))
        });
        let Some(title) = title else { return };
        self.changed();
        if crate::config::flag("/notify/decisions") {
            self.notify(id, &title.0, &title.1, "");
        }
    }
}
