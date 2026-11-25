//! A question you ask on an item's page while it's to check or done (#475): its holder answers it without the item
//! reopening. The turn that answers puts the item back where it was (to check, or done), never stopped or at work;
//! one that changed its code, or said DONE for it, lands again as a new round.

use crate::desk::hub::prelude::*;
use crate::desk::hub::core::desk::{Effects, secs};
use crate::desk::hub::turn::act::you_wrote;
use crate::desk::hub::turn::route::send_step;
use crate::desk::parse;

/// How far back a message the Desk saved for a session still counts as one it sent (#811): its queue can hold one a while.
const SENT_LATELY_MS: u64 = 6 * 60 * 60 * 1000;

/// Your words on an item's page ask something, rather than say what isn't right: they end on a question mark (the
/// pictures listed after them left out).
pub(crate) fn is_question(text: &str) -> bool {
    crate::desk::your_words(text).0.trim_end().ends_with('?')
}

/// Your question `text` on item `id` (to check or done, held): sent to its holder as a question about the item, and
/// the item at work with it while it answers, remembering where it was.
pub(crate) fn ask(st: &mut Store, id: &str, text: &str, now: u64) -> Result<Effects, String> {
    let mut fx = Effects::default();
    let words = text.trim().to_string();
    let it = st.desk.item_mut(id).ok_or("no such item")?;
    let h = it.holder.clone().ok_or("nobody holds it: it goes to the lead")?;
    let stage = if it.state == "done" { "done" } else { "with the user to check" };
    let question = crate::desk::your_words(&words).0.trim().to_string();
    // Asked again before it answered: still where it first was.
    let was = it.asked.as_ref().map_or_else(|| it.state.clone(), |(was, _)| was.clone());
    it.asked = Some((was, question.clone()));
    (it.state, it.stopped, it.updated) = ("working".into(), false, secs(now));
    let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
    let message = format!(
        "{crew}A question about Desk item #{id} ({}), which is {stage}: {}\nAnswer it. If you change code for it, say DONE: #{id} once that's done; if you don't, the item stays as it was.{}\n{}",
        it.title,
        crate::desk::with_your_pictures(&words),
        if it.asked.as_ref().is_some_and(|(was, _)| was == "to_check") { format!("\n{}", crate::desk::not_done_words(id)) } else { String::new() },
        crate::desk::check_words(id)
    );
    let project = it.project.clone();
    you_wrote(st, &project, id, &question, &format!("asked {}", h.name), now, &mut fx);
    send_step(st, id, h.to(), message, now, &mut fx);
    fx.items.push(id.to_string());
    Ok(fx)
}

/// Session `sid`'s turn ended (saying `text`; `loose`: each item's files git says aren't committed): each item it
/// was answering your question on is settled. No code changed and no DONE for it: back where it was. Else a new
/// round: its earlier steps kept as one, and it's finishing again (its DONE line, if it said one, does that itself).
/// Its crew or background agents still at work: settled at a later turn's end.
pub(crate) fn answered(st: &mut Store, sid: &str, text: &str, loose: &[(String, Vec<String>)], now: u64, fx: &mut Effects) {
    if crate::desk::hub::stall::stall::still_at_it(st, sid) {
        return;
    }
    let done: Vec<String> = parse::lines(text).into_iter().filter_map(|l| if let parse::Line::Done { ids } = l { Some(ids) } else { None }).flatten().map(|i| st.desk.canon(&i)).collect();
    let ids: Vec<String> = st.desk.items.iter().filter(|i| i.asked.is_some() && i.holder.as_ref().is_some_and(|h| h.is(sid))).map(|i| i.id.clone()).collect();
    for id in ids {
        let changed = loose.iter().any(|(i, files)| *i == id && !files.is_empty());
        let said_done = done.contains(&id);
        let Some(it) = st.desk.item_mut(&id) else { continue };
        let Some((was, question)) = it.asked.take() else { continue };
        it.updated = secs(now);
        fx.items.push(id.clone());
        // Answered while it waited for a copy (#567: your question went at once): no code changed and no DONE, so it
        // goes back where it was and leaves the line; else it waits on for its copy, to build there.
        if it.state == "queued" && it.tree_wait && it.after.is_none() {
            if !changed && !said_done {
                (it.state, it.stopped, it.waits, it.tree_wait) = (was, false, None, false);
                it.tree_wait_repo.clear();
            }
            continue;
        }
        if it.state != "working" {
            // Moved on meanwhile (sent back, dropped, routed): nothing to put back.
            continue;
        }
        if !changed && !said_done {
            it.state = was;
            (it.stopped, it.waits) = (false, None);
            continue;
        }
        // A new round: the steps it had reached kept, as a send back keeps them (#421), and building again from now.
        use crate::desk::hub::core::steps::{BUILDING, STEPS, TO_CHECK};
        if it.steps.len() != STEPS {
            it.steps.resize(STEPS, 0);
        }
        let from = it.steps.iter().rposition(|s| *s != 0).unwrap_or(TO_CHECK);
        let before = it.steps.clone();
        it.backs.push(crate::desk::model::StepBack { from, why: format!("Your question: {question}"), at_ms: now, steps: before });
        it.steps[BUILDING..].iter_mut().for_each(|s| *s = 0);
        it.steps[BUILDING] = now;
        (it.done_at, it.close_on_check, it.stopped) = (None, false, false);
        if said_done {
            // holder_said's DONE takes it to finishing, with its check card.
            continue;
        }
        let by = if it.crew.is_empty() { it.holder.as_ref().map(|h| h.name.clone()).unwrap_or_default() } else { it.crew.clone() };
        it.checks.push(crate::desk::model::DeskCheck { at: now, by, ..Default::default() });
        fx.checked.push(id.clone());
        it.state = "finishing".into();
        it.finish = Some(crate::desk::hub::landing::finish::begun(now, it.finish.as_ref()));
    }
}

impl Hub {
    /// Session `sid` took `prompt` as a new turn's: a message the Desk didn't send it, nor a task's notice or a
    /// teammate's, is yours, typed into it, so it answers what it asked you.
    pub(crate) fn desk_prompt_taken(&self, sid: &str, prompt: &str) {
        use crate::logs::transcript::{harness_text, is_message, peer_text};
        if prompt.trim().is_empty() || harness_text(prompt) || peer_text(prompt) {
            return;
        }
        let m = crate::sessions::unwrap_pasted(prompt);
        let desks = self.read(|st| st.desk.run.sent.get(sid).is_some_and(|sent| sent.iter().any(|t| is_message(&m, t))));
        // Not one this Cue sent since it started: one saved with the Desk's steps still counts (#811: #790 reopened by
        // the lead's note, sent by another Cue on the same data). Looked up only when the prompt could reopen
        // or answer something.
        let desks = desks || (self.read(|st| st.desk.items.iter().any(|i| i.holder.as_ref().is_some_and(|h| h.to() == sid) && (i.state == "to_check" || i.asks.is_some())))
            && crate::db::desk_sent_to(sid, now_ms().saturating_sub(SENT_LATELY_MS)).iter().any(|t| is_message(&m, &crate::desk::hub::core::desk::as_delivered(t))));
        // Only words typed into it yourself put an item to check back at work. The Desk's own messages don't (#584):
        // a Send back or a route back reopened it as it was sent, and a note (yours from its page, the lead's) is
        // only a note: "ok testing now" isn't a send-back.
        if !desks {
            self.desk_you_answered(sid);
            self.desk_back_at_work(sid, &m);
        }
    }

    /// Session `sid` took `prompt`, words you typed into it about items of its that are to check (#484): it's at work
    /// on them again, so they are, until it says DONE again. One you asked a question on is
    /// settled as its answer ends (desk_asked's `answered`), not here.
    fn desk_back_at_work(&self, sid: &str, prompt: &str) {
        // One of the Desk's own messages is about the item it opens with ("Desk item #513 …", a note on #472): an
        // item to check it only mentions (a brief for the item queued after it, #515) stays as it is.
        let on = desk_message_item(prompt);
        let about = |i: &crate::desk::DeskItem| {
            i.state == "to_check" && i.asked.is_none() && i.holder.as_ref().is_some_and(|h| h.to() == sid) && on.as_ref().map_or_else(|| names_item(prompt, &i.id), |o| *o == i.id)
        };
        if !self.read(|st| st.desk.items.iter().any(about)) {
            return;
        }
        let now = now_ms();
        let why = why_back(prompt);
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            for it in st.desk.items.iter_mut().filter(|i| about(i)) {
                back_at_work(it, &why, "You", now);
                fx.items.push(it.id.clone());
            }
            fx
        });
        self.desk_done(fx);
    }

    /// You wrote to session `sid` yourself: the questions it asked you on the items it holds are answered (#475).
    /// Only read unless there's one.
    pub(crate) fn desk_you_answered(&self, sid: &str) {
        let asked = |i: &crate::desk::DeskItem| i.asks.is_some() && i.holder.as_ref().is_some_and(|h| h.to() == sid);
        if !self.read(|st| st.desk.items.iter().any(asked)) {
            return;
        }
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            for it in st.desk.items.iter_mut().filter(|i| asked(i)) {
                it.asks = None;
                fx.items.push(it.id.clone());
            }
            fx
        });
        self.desk_done(fx);
    }
}

/// Item `it`, with you to check, at work again (`why`: what reopened it, kept on the DONE it reopens, so its card and
/// Progress say why). Saved with it in `fx.items`, it gets its copy, or waits in line for one (desk_trees).
pub(crate) fn back_at_work(it: &mut crate::desk::DeskItem, why: &str, by: &str, now: u64) {
    (it.state, it.done_at, it.finish, it.close_on_check, it.updated) = ("working".into(), None, None, false, secs(now));
    (it.check, it.asked) = (None, None);
    it.mark_reopened();
    if let Some(c) = it.checks.last_mut().filter(|c| c.back.is_empty()) {
        (c.back, c.back_by) = (why.to_string(), by.to_string());
    }
}

/// Holder `sid` said NOT DONE on items it holds that are with you to check (#627): each is at work again, in line for
/// a copy when none is free, until it says DONE again. One it doesn't hold, or not to check, stays as it is.
pub(crate) fn not_done(st: &mut Store, sid: &str, text: &str, now: u64, fx: &mut Effects) {
    for line in parse::lines(text) {
        let parse::Line::NotDone { id, text: left } = line else { continue };
        let id = st.desk.canon(&id);
        let Some(it) = st.desk.item_mut(&id).filter(|i| i.state == "to_check" && i.holder.as_ref().is_some_and(|h| h.is(sid))) else { continue };
        let name = it.holder.as_ref().map(|h| h.name.clone()).unwrap_or_default();
        let why = if left.is_empty() { format!("{name}: not done yet") } else { format!("{name}: {left}") };
        back_at_work(it, &why, &name, now);
        fx.items.push(id);
    }
}

/// The item one of the Desk's own messages is about: the one its first "Desk item #<id>" names. None for words that
/// aren't the Desk's (yours, typed to the session).
fn desk_message_item(text: &str) -> Option<String> {
    let (_, rest) = text.split_once("Desk item #")?;
    let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    (!id.is_empty()).then_some(id)
}

/// `text` is about item `id`: it names "#id", not as the start of a longer number.
fn names_item(text: &str, id: &str) -> bool {
    let tag = format!("#{id}");
    text.match_indices(&tag).any(|(at, _)| !text[at + tag.len()..].starts_with(|c: char| c.is_ascii_alphanumeric()))
}

/// Why an item to check is at work again, from the prompt its holder took: its words after the Desk's "… #id (title…):"
/// opening, on one line, kept short.
fn why_back(prompt: &str) -> String {
    let first = prompt.lines().find(|l| !l.trim().is_empty()).unwrap_or_default().trim();
    let words = if first.contains("Desk item #") { first.split_once("): ").map_or(first, |(_, w)| w) } else { first };
    let mut out: String = words.chars().take(160).collect();
    if words.chars().count() > 160 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_is_about_the_items_it_names() {
        assert!(names_item("Note from the Desk lead on Desk item #12 (Header): x", "12"));
        assert!(names_item("#12: shorter", "12"));
        assert!(!names_item("#123: shorter", "12"));
        assert!(!names_item("make it shorter", "12"));
        assert_eq!(why_back("Note from the Desk lead on Desk item #12 (Header, with the user to check): fix the footer too"), "fix the footer too");
        assert_eq!(why_back("#12: make it shorter"), "#12: make it shorter");
    }
}
