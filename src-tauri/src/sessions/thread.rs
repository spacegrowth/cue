//! The thread: the last few exchanges of a session, and how an echo of a message is folded into it.

use super::{Session, Sessions};
use crate::model::{now_ms, Exchange};

/// Longest text kept per agent message: enough for a full one.
const EXCHANGE_CHARS: usize = 6000;
/// Your own messages are kept whole (up to this): a copy cut short can't be matched with what the
/// agent got, so a long paste used to show twice.
const YOURS_CHARS: usize = 200_000;
/// A long message's echo can come well after it was sent (a big paste takes a while to land).
const LONG_ECHO_MS: u64 = 120_000;

impl Sessions {
    /// Record one exchange, keeping only the last `keep`.
    pub fn note(&mut self, session_id: &str, role: &str, text: &str, keep: usize) {
        self.note_with(session_id, role, text, &[], keep)
    }

    /// How you answered a permission or a question, as yours in the thread, marked as a decision: it's no
    /// message you wrote (talking by voice goes on through it).
    pub fn note_decision(&mut self, session_id: &str, outcome: &str, keep: usize) {
        let before = self.0.get(session_id).map_or(0, |s| s.thread.len());
        self.note(session_id, "you", outcome, keep);
        if let Some(e) = self.0.get_mut(session_id).filter(|s| s.thread.len() > before || before >= keep).and_then(|s| s.thread.last_mut()) {
            e.decision = true;
        }
    }

    /// Same, with images. A reply sent from Cue comes back moments later as the agent's prompt
    /// (with the image paths typed after it): that echo is folded into the entry already noted.
    pub fn note_with(&mut self, session_id: &str, role: &str, text: &str, images: &[String], keep: usize) {
        let text = text.trim();
        let Some(s) = self.0.get_mut(session_id) else { return };
        if text.is_empty() && images.is_empty() {
            return;
        }
        // The message of yours before the last one: the one you stopped, if this turns out glued to it.
        let above = if role == "you" { s.thread.len().checked_sub(2).and_then(|i| s.thread.get(i)).filter(|e| e.role == "you").map(|e| e.text.clone()) } else { None };
        if let Some(last) = s.thread.last_mut() {
            let paths: Vec<&str> = images.iter().chain(last.images.iter()).map(String::as_str).collect();
            let age = now_ms().saturating_sub(last.at_ms);
            // The agent's own report of a message Cue sent and has just seen it take, however late that was
            // (a paste that sat in its box for minutes): the same message, not a second one.
            let just_taken = last.delivery.last().is_some_and(|d| d.what.starts_with("taken") && now_ms().saturating_sub(d.at_ms) < 10_000);
            if last.role == role && role == "you" && just_taken && same_message(text, &last.text, &paths) {
                return;
            }
            if last.role == role && (age < 15_000 || (role == "you" && age < LONG_ECHO_MS && is_long(text))) {
                if same_message(text, &last.text, &paths) {
                    // Keep the first copy; Cue's own copy carries the images, so they move onto it.
                    if last.images.is_empty() && !images.is_empty() {
                        last.images = images.to_vec();
                    }
                    return;
                }
                // Typed into a box that still held something (a message you stopped comes back into
                // Claude Code's box): the agent got both, glued. When what was there is your message just
                // above, that one is already in the chat: this one stays what you typed, marked as gone
                // with it. Otherwise (something left in the box Cue never saw) one bubble with all it got.
                if role == "you" && glued_after(text, &last.text, &paths) {
                    if above.as_deref().is_some_and(|a| glued_onto(text, a)) {
                        last.joined = true;
                    } else {
                        last.text = text.to_string();
                    }
                    return;
                }
            }
        }
        Self::push(s, role, text, images, keep);
    }

    /// Add an entry to the thread as it is (no folding into the one before).
    fn push(s: &mut Session, role: &str, text: &str, images: &[String], keep: usize) {
        let cap = if role == "you" { YOURS_CHARS } else { EXCHANGE_CHARS };
        let text = if text.chars().count() > cap { format!("{}…", text.chars().take(cap).collect::<String>()) } else { text.to_string() };
        // Never the same moment as the entry before: two noted at once (a teammate's messages, then the
        // reply to them) are saved by moment and speaker, and the second would overwrite the first.
        let at_ms = now_ms().max(s.thread.last().map_or(0, |l| l.at_ms + 1));
        s.thread.push(Exchange { role: role.into(), text, at_ms, images: images.to_vec(), from: String::new(), unsent: false, pending: false, joined: false, delivery: vec![], voice: false, decision: false, status_ask: None });
        let excess = s.thread.len().saturating_sub(keep);
        s.thread.drain(..excess);
    }

    /// Cue's own copy of a message it just sent (at `since`). The agent reports what it got (Claude Code's
    /// prompt hook) usually before the send even returns: any message of yours noted since then is that
    /// one, whatever its text (what the agent got is what counts), so it only takes the images. Matched
    /// by the send, not by comparing text: comparing is what let a long paste show twice. Otherwise
    /// noted as usual (the agent's report, if it comes later, folds into it).
    pub fn note_sent(&mut self, session_id: &str, text: &str, images: &[String], since: u64, keep: usize) {
        if let Some(e) = self.0.get_mut(session_id).and_then(|s| s.thread.iter_mut().rev().find(|e| e.role == "you" && e.at_ms >= since)) {
            if e.images.is_empty() && !images.is_empty() {
                e.images = images.to_vec();
            }
            return;
        }
        // Cue sent it: a message of its own, even the same words as one a moment ago ("yes", twice): the agent got
        // two. (The agent's report of it, noted later, folds into this one.)
        let text = text.trim();
        if let Some(s) = self.0.get_mut(session_id).filter(|_| !text.is_empty() || !images.is_empty()) {
            Self::push(s, "you", text, images, keep);
        }
    }

    /// A message another agent session sent this one, labelled with who sent it.
    pub fn note_peer(&mut self, session_id: &str, from: &str, text: &str, keep: usize) {
        self.note(session_id, "peer", text, keep);
        if let Some(last) = self.0.get_mut(session_id).and_then(|s| s.thread.last_mut()) {
            if last.role == "peer" && last.from.is_empty() {
                last.from = from.to_string();
            }
        }
    }

    /// Its teammates' messages read from its log, oldest first (`teammate_messages_after`): each as a
    /// message from that teammate, and the log read up to the newest.
    pub fn note_teammates(&mut self, session_id: &str, heard: &[(u64, String, String)], keep: usize) {
        for (_, who, said) in heard {
            self.note_peer(session_id, who, said, keep);
        }
        if let (Some(s), Some(last)) = (self.0.get_mut(session_id), heard.iter().map(|h| h.0).max()) {
            s.teammates_read_ms = s.teammates_read_ms.max(last);
        }
    }

    /// Up to when (by its log's clock) its teammates' messages have been read; 0 = not yet.
    pub fn teammates_read(&self, session_id: &str) -> u64 {
        self.0.get(session_id).map_or(0, |s| s.teammates_read_ms)
    }

    pub fn thread(&self, session_id: &str) -> Vec<Exchange> {
        self.0.get(session_id).map(|s| s.thread.clone()).unwrap_or_default()
    }

    /// Its latest reply in the thread is `text` (a message of yours may sit after it). A turn that ends a
    /// second time (a Stop hook sent the agent back to work, and it stopped again) reports the same answer,
    /// noted at its first end already.
    pub fn last_reply_is(&self, session_id: &str, text: &str) -> bool {
        let head = |t: &str| t.trim().trim_end_matches('…').chars().take(2000).collect::<String>();
        self.0.get(session_id).and_then(|s| s.thread.iter().rev().find(|e| e.role == "agent")).is_some_and(|e| !e.text.trim().is_empty() && head(&e.text) == head(text))
    }
}

/// The same message twice: what Cue typed, then the prompt the agent reports back. They can
/// differ at the edges: stray keys already in the terminal, image paths typed after it, line
/// joins. Paths are ignored, and the texts must be nearly the same length, so "ok" and
/// "ok, ship it" sent back to back stay two messages.
fn same_message(a: &str, b: &str, paths: &[&str]) -> bool {
    // Your own image paths come out first, as typed (escaped) or as reported back (spaces and all):
    // a path with a space in it would otherwise split into words that don't look like a path.
    let strip = |t: &str| {
        paths.iter().fold(t.to_string(), |acc, p| acc.replace(&crate::uploads::escape_path(p), " ").replace(p, " "))
    };
    let squash = |t: &str| strip(t).split_whitespace().filter(|w| !w.starts_with('/')).collect::<Vec<_>>().join(" ");
    // Nothing but a command ("/handoff"): its words are the message, the same when they're the same.
    let bare = |t: &str| strip(t).split_whitespace().collect::<Vec<_>>().join(" ");
    let (a, b) = match (squash(a), squash(b)) {
        (x, y) if x.is_empty() && y.is_empty() => return !bare(a).is_empty() && bare(a) == bare(b),
        xy => xy,
    };
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // Long ones (a paste): the same start is the same message, whatever became of its whitespace (a
    // terminal may join its lines) or its end (an older copy was cut short, with "…").
    if is_long(&a) && is_long(&b) {
        return crate::logs::transcript::is_message(&a, &b) || crate::logs::transcript::is_message(&b, &a);
    }
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    long.contains(short.as_str()) && short.len() * 5 >= long.len() * 4
}

/// Long enough that its start alone tells it apart: 300 characters that aren't spaces.
pub(super) fn is_long(t: &str) -> bool {
    t.chars().filter(|c| !c.is_whitespace()).nth(299).is_some()
}

/// `long` starts with `head`, whatever became of their whitespace (the agent may get a blank line between
/// them, or none), and `head` is more than a word or two.
fn glued_onto(long: &str, head: &str) -> bool {
    let squash = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
    let (long, head) = (squash(long), squash(head));
    head.len() >= 8 && long.len() > head.len() && long.starts_with(&head)
}

/// `long` is `short` with something typed before it (and `short` is more than a word or two).
pub(super) fn glued_after(long: &str, short: &str, paths: &[&str]) -> bool {
    let squash = |t: &str| t.split_whitespace().filter(|w| !w.starts_with('/') && !paths.contains(w)).collect::<Vec<_>>().join(" ");
    let (long, short) = (squash(long), squash(short));
    short.len() >= 8 && long.len() > short.len() && long.ends_with(&short)
}
