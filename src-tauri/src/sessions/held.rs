//! Messages you sent while it was busy: kept in Cue for when its turn ends, or queued in its box.

use super::delivery::record;
use super::Sessions;
use crate::model::{now_ms, Exchange};

impl Sessions {
    /// Keep a message for when its turn ends; one already kept gets this one added after it.
    pub fn hold(&mut self, session_id: &str, text: &str, images: Vec<String>) {
        let Some(s) = self.0.get_mut(session_id) else { return };
        match &mut s.held {
            Some(h) => {
                h.text = [h.text.as_str(), text].iter().filter(|t| !t.trim().is_empty()).cloned().collect::<Vec<_>>().join("\n\n");
                h.images.extend(images);
            }
            None => s.held = Some(Exchange { role: "you".into(), text: text.to_string(), at_ms: now_ms(), images, from: String::new(), unsent: false, pending: false, joined: false, delivery: vec![], voice: false, decision: false, status_ask: None }),
        }
    }

    /// Put back a kept message that couldn't be sent: before anything kept since, as it was first.
    pub fn hold_back(&mut self, session_id: &str, text: &str, images: Vec<String>) {
        let Some(s) = self.0.get_mut(session_id) else { return };
        match &mut s.held {
            Some(h) => {
                h.text = [text, h.text.as_str()].iter().filter(|t| !t.trim().is_empty()).cloned().collect::<Vec<_>>().join("\n\n");
                h.images = images.into_iter().chain(std::mem::take(&mut h.images)).collect();
            }
            None => self.hold(session_id, text, images),
        }
    }

    /// The kept message couldn't be sent (its terminal gone, it's asking something first): marked so, with why,
    /// so its bubble says it instead of promising it goes when the turn ends. Sent later, the mark goes with it.
    pub fn held_failed(&mut self, session_id: &str, why: &str) {
        if let Some(h) = self.0.get_mut(session_id).and_then(|s| s.held.as_mut()) {
            h.unsent = true;
            record(h, format!("couldn't send: {why}"));
        }
    }

    pub fn held(&self, session_id: &str) -> Option<Exchange> {
        self.0.get(session_id)?.held.clone()
    }

    /// The kept message, taken (to send it, or back into your box).
    pub fn take_held(&mut self, session_id: &str) -> Option<Exchange> {
        self.0.get_mut(session_id)?.held.take()
    }

    /// What you sent it while it was busy, if it hasn't been read yet.
    pub fn queued_text(&self, session_id: &str) -> Option<String> {
        self.0.get(session_id)?.queued.as_ref().map(|q| q.text.clone())
    }

    pub fn queued(&self, session_id: &str) -> Option<Exchange> {
        self.0.get(session_id)?.queued.clone()
    }

    /// Move your message `q` (sent while it was busy, unread when the turn ended) below the reply just
    /// noted: that's where it's read, as the next turn's prompt.
    pub fn below_reply(&mut self, session_id: &str, q: &Exchange) {
        let Some(s) = self.0.get_mut(session_id) else { return };
        let head = |t: &str| t.trim().chars().take(200).collect::<String>();
        let Some(i) = s.thread.iter().rposition(|e| e.role == "you" && e.at_ms.abs_diff(q.at_ms) < 15_000 && head(&e.text) == head(&q.text)) else { return };
        if i + 1 == s.thread.len() {
            return;
        }
        let mut e = s.thread.remove(i);
        e.at_ms = s.thread.last().map_or(e.at_ms, |l| l.at_ms + 1).max(e.at_ms);
        s.thread.push(e);
    }

    pub fn set_queued(&mut self, session_id: &str, q: Option<Exchange>) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.queued = q;
        }
    }
}
