//! A question you ask on an item's page while it's to check or done (#475): its holder answers it without the item
//! reopening. The turn that answers puts the item back where it was (to check, or done), never stopped or at work;
//! one that changed its code, or said DONE for it, lands again as a new round.

use super::*;
use super::desk::{Effects, secs};
use super::desk_act::you_wrote;
use super::desk_route::send_step;
use crate::desk::parse;

/// Your words on an item's page ask something, rather than say what isn't right: they end on a question mark (the
/// pictures listed after them left out).
pub(super) fn is_question(text: &str) -> bool {
    let words = text.split_once("\nPictures with it").map_or(text, |(w, _)| w);
    words.trim_end().ends_with('?')
}

/// Your question `text` on item `id` (to check or done, held): sent to its holder as a question about the item, and
/// the item at work with it while it answers, remembering where it was.
pub(super) fn ask(st: &mut Store, id: &str, text: &str, now: u64) -> Result<Effects, String> {
    let mut fx = Effects::default();
    let words = text.trim().to_string();
    let it = st.desk.item_mut(id).ok_or("no such item")?;
    let h = it.holder.clone().ok_or("nobody holds it: it goes to the lead")?;
    let stage = if it.state == "done" { "done" } else { "with the user to check" };
    let question = words.split_once("\nPictures with it").map_or(words.as_str(), |(w, _)| w).trim().to_string();
    // Asked again before it answered: still where it first was.
    let was = it.asked.as_ref().map_or_else(|| it.state.clone(), |(was, _)| was.clone());
    it.asked = Some((was, question.clone()));
    (it.state, it.stopped, it.updated) = ("working".into(), false, secs(now));
    let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
    let message = format!(
        "{crew}A question about Desk item #{id} ({}), which is {stage}: {words}\nAnswer it. If you change code for it, say DONE: #{id} once that's done; if you don't, the item stays as it was.",
        it.title
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
pub(super) fn answered(st: &mut Store, sid: &str, text: &str, loose: &[(String, Vec<String>)], now: u64, fx: &mut Effects) {
    if super::desk_stall::still_at_it(st, sid) {
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
        use super::desk_steps::{BUILDING, STEPS, TO_CHECK};
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
        it.finish = Some(super::desk_finish::begun(now, it.finish.as_ref()));
    }
}

impl Hub {
    /// Session `sid` took `prompt` as a new turn's: a message the Desk didn't send it, nor a task's notice or a
    /// teammate's, is yours, typed into it, so it answers what it asked you.
    pub(super) fn desk_prompt_taken(&self, sid: &str, prompt: &str) {
        use crate::logs::transcript::{harness_text, is_message, peer_text};
        if prompt.trim().is_empty() || harness_text(prompt) || peer_text(prompt) {
            return;
        }
        let m = crate::sessions::unwrap_pasted(prompt);
        let desks = self.read(|st| st.desk.sent.get(sid).is_some_and(|sent| sent.iter().any(|t| is_message(&m, t))));
        if !desks {
            self.desk_you_answered(sid);
        }
    }

    /// You wrote to session `sid` yourself: the questions it asked you on the items it holds are answered (#475).
    /// Only read unless there's one.
    pub(super) fn desk_you_answered(&self, sid: &str) {
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
