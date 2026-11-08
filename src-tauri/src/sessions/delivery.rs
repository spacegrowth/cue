//! How each message of yours got to the agent: taken, not taken, still being looked for, and the
//! steps on its way.

use super::thread::{glued_after, is_long};
use super::{Session, Sessions};
use crate::model::{now_ms, Exchange, Origin};

impl Sessions {
    /// The agent got `received` (its prompt hook said so, its log has it, it answered it): if that's a
    /// message of yours Cue was still unsure about, it's settled now. The bubble stops saying it didn't
    /// go or that Cue is waiting, whichever path said so and however late this comes. True when one was.
    /// `witness` names who said so, for the message's delivery record.
    pub fn got_message(&mut self, session_id: &str, received: &str, witness: &str) -> bool {
        let Some(s) = self.0.get_mut(session_id) else { return false };
        let received = unwrap_pasted(received);
        taken(s, &received, witness)
    }

    /// Your message `text` to it never went through: Cue saw it still sitting in the agent's box (or the
    /// agent gone) after waiting. Its bubble says so, with Send again. `why`: what Cue saw, for the record.
    /// Only that message: a second one sent meanwhile has its own verdict coming.
    pub fn mark_unsent(&mut self, session_id: &str, text: &str, why: &str) {
        if let Some(e) = self.0.get_mut(session_id).and_then(|s| yours_mut(s, text)) {
            e.unsent = true;
            e.pending = false;
            record(e, format!("not taken: {why}"));
        }
    }

    /// Every message of yours it hadn't taken yet, marked not sent (`why`): its agent is gone (a hosted session's
    /// Claude exited), so none of them will be. Each bubble then offers Send again.
    pub fn unsend_pending(&mut self, session_id: &str, why: &str) {
        for e in self.0.get_mut(session_id).into_iter().flat_map(|s| s.thread.iter_mut()).filter(|e| e.role == "you" && e.pending) {
            e.unsent = true;
            e.pending = false;
            record(e, format!("not taken: {why}"));
        }
    }

    /// Your message `text` to it was said by voice: the bubble says Cue added a line for the agent (or will, when
    /// it's still kept in Cue or queued in the agent's box).
    pub fn mark_voice(&mut self, session_id: &str, text: &str) {
        let Some(s) = self.0.get_mut(session_id) else { return };
        for q in [&mut s.held, &mut s.queued].into_iter().flatten().filter(|q| is_yours(q, text)) {
            q.voice = true;
        }
        if let Some(e) = yours_mut(s, text) {
            e.voice = true;
        }
    }

    /// Your message `text` to it had a status list asked for by a prompt hook (`ask`: the hook's text): the bubble's note.
    /// True when the bubble changed.
    pub fn mark_status_ask(&mut self, session_id: &str, text: &str, ask: &str) -> bool {
        let Some(e) = self.0.get_mut(session_id).and_then(|s| yours_mut(s, text)) else { return false };
        let changed = e.status_ask.as_deref() != Some(ask);
        e.status_ask = Some(ask.to_string());
        changed
    }

    /// Your message `text` to it: Cue hasn't seen the agent take it yet (`on`: a ring on the bubble while
    /// it keeps looking), or has stopped looking without a verdict (`!on`, the ring goes). True when the
    /// bubble changed.
    pub fn set_pending(&mut self, session_id: &str, text: &str, on: bool) -> bool {
        match self.0.get_mut(session_id).and_then(|s| yours_mut(s, text)) {
            Some(e) if e.pending != on => {
                e.pending = on;
                true
            }
            _ => false,
        }
    }

    /// Whether Cue is still waiting to see your message `text` taken (nothing has settled it yet).
    pub fn is_pending(&self, session_id: &str, text: &str) -> bool {
        // However many messages came since: an unsettled one is never too old to be found.
        self.0.get(session_id).is_some_and(|s| s.thread.iter().rev().any(|e| e.role == "you" && e.pending && is_yours(e, text)))
    }

    /// One more step in your message's way to the agent ("Enter again: it was still in its box").
    pub fn note_delivery(&mut self, session_id: &str, text: &str, what: &str) {
        if let Some(e) = self.0.get_mut(session_id).and_then(|s| yours_mut(s, text)) {
            record(e, what.to_string());
        }
    }

    /// Every message of yours Cue was still unsure about when it last ran (waiting to see it taken, or
    /// flagged as not taken): with the session it went to, so the question can be asked again on launch.
    pub fn unsettled(&self) -> Vec<(Origin, Exchange)> {
        self.0.values().flat_map(|s| s.thread.iter().rev().filter(|e| e.role == "you" && (e.pending || e.unsent)).map(move |e| (s.origin.clone(), e.clone()))).collect()
    }
}

/// Claude Code hands a long paste to the prompt hook wrapped in `<pasted_content id="…">` tags, and
/// cut into chunks wherever the terminal split the paste (mid-word too). Take the text back out,
/// chunks rejoined, so it reads (and matches Cue's own copy of what it typed) as sent.
pub fn unwrap_pasted(text: &str) -> String {
    const OPEN: &str = "<pasted_content";
    const CLOSE: &str = "</pasted_content";
    let mut out = String::new();
    let mut rest = text;
    loop {
        let Some(i) = [rest.find(OPEN), rest.find(CLOSE)].into_iter().flatten().min() else { break };
        let Some(len) = rest[i..].find('>') else { break };
        out.push_str(&rest[..i]);
        let closing = rest[i..].starts_with(CLOSE);
        rest = &rest[i + len + 1..];
        if closing {
            // "…\n</pasted_content>\n\n\n<pasted_content>\n…": one chunk ends, the next carries on.
            if out.ends_with('\n') {
                out.pop();
            }
            if rest.trim_start().starts_with(OPEN) {
                rest = rest.trim_start();
            }
        } else if let Some(r) = rest.strip_prefix('\n') {
            rest = r;
        }
    }
    out.push_str(rest);
    out
}

/// The agent got `received`: the message of yours that is, among the last few, if Cue was still unsure
/// about it, is settled as taken. Compared as the agent got it (spacing changed, image paths after it,
/// a long paste's wrapper taken off by the caller), not character for character. A joined one holds
/// only what you typed: the agent got the message above glued before it. True when one was settled.
///
/// This is the one place a message becomes "taken". Whoever saw it (the prompt hook, the log, a dialog,
/// an agent's own word) comes here, however late: a verdict that it didn't go is overturned by it.
pub(super) fn taken(s: &mut Session, received: &str, witness: &str) -> bool {
    let received = received.trim();
    if received.is_empty() {
        return false;
    }
    // Just an image, no words: the agent got it if the image (its file, which goes in with the message) is in
    // what it got.
    let image_got = |e: &Exchange| e.text.trim().is_empty() && e.images.iter().filter_map(|p| p.rsplit('/').next()).any(|n| !n.is_empty() && received.contains(n));
    let got = |e: &Exchange| crate::logs::transcript::is_message(received, &e.text) || (e.joined && glued_after(received, &e.text, &[])) || image_got(e);
    match s.thread.iter_mut().rev().filter(|e| e.role == "you").find(|e| (e.unsent || e.pending) && got(e)) {
        Some(e) => {
            e.unsent = false;
            e.pending = false;
            record(e, format!("taken: {witness}"));
            true
        }
        None => false,
    }
}

/// Is `e` your message `text`, as Cue sent it? The same words; a long one may have been cut short in
/// the thread (YOURS_CHARS) or had its spacing changed, so for those the same start is enough.
fn is_yours(e: &Exchange, text: &str) -> bool {
    e.text.trim() == text.trim() || (is_long(text) && (crate::logs::transcript::is_message(&e.text, text) || crate::logs::transcript::is_message(text, &e.text)))
}

/// Your message `text` in the thread: one Cue is still unsure about, however many came since; else among the last
/// few of yours (a short one, "yes", mustn't be taken for the same words long before).
fn yours_mut<'a>(s: &'a mut Session, text: &str) -> Option<&'a mut Exchange> {
    let unsettled = s.thread.iter().rposition(|e| e.role == "you" && (e.pending || e.unsent) && is_yours(e, text));
    match unsettled {
        Some(i) => s.thread.get_mut(i),
        None => s.thread.iter_mut().rev().filter(|e| e.role == "you").take(3).find(|e| is_yours(e, text)),
    }
}

/// Keep the last few steps of a message's delivery record (enough for one send with its retries).
pub(super) const DELIVERY_STEPS: usize = 8;

pub(super) fn record(e: &mut Exchange, what: String) {
    e.delivery.push(crate::model::Delivery { at_ms: now_ms(), what });
    let excess = e.delivery.len().saturating_sub(DELIVERY_STEPS);
    e.delivery.drain(..excess);
}
