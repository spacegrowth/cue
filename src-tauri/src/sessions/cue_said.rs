//! Cue's own words to a session (#836): known as Cue's when Cue sends them, so the chat never shows them as yours.
//!
//! Whatever sends them (Hub::tell, the Desk's messages, a lead's hello…) says so first (`cue_says`); the message of
//! yours the thread then notes with those words, Cue's copy or the agent's report of it, is marked `cue`. A kept
//! message holding Cue's words with yours knows which parts are Cue's (`cue_parts`), so yours go first, alone.

use super::Sessions;
use crate::model::now_ms;

/// How long Cue's words are known as its own: the agent's report of a message comes long before this.
const KEEP_MS: u64 = 30 * 60_000;
/// At most this many known at once (the oldest go first).
const MOST: usize = 64;

/// What Cue sent lately, each as `plain` makes it, and when.
#[derive(Default)]
pub struct CueSaid(Vec<(String, u64)>);

/// `text` as Cue's words are compared: a Desk lead's `<desk>` block left out, every run of whitespace one space.
fn plain(text: &str) -> String {
    crate::desk::without_block(text).split_whitespace().collect::<Vec<_>>().join(" ")
}

impl CueSaid {
    fn add(&mut self, text: &str, now: u64) {
        let t = plain(text);
        if t.is_empty() {
            return;
        }
        self.0.retain(|(_, at)| now.saturating_sub(*at) < KEEP_MS);
        self.0.push((t, now));
        let excess = self.0.len().saturating_sub(MOST);
        self.0.drain(..excess);
    }

    /// `text` is Cue's words alone: one message it sent, or several it sent kept and gone together.
    pub fn all_cue(&self, text: &str) -> bool {
        let mut rest = plain(text);
        if rest.is_empty() {
            return false;
        }
        if self.0.iter().any(|(t, _)| *t == rest) {
            return true;
        }
        let mut known: Vec<&str> = self.0.iter().map(|(t, _)| t.as_str()).collect();
        known.sort_by_key(|t| std::cmp::Reverse(t.len()));
        let mut any = false;
        for t in known {
            if let Some(i) = rest.find(t) {
                rest.replace_range(i..i + t.len(), "");
                any = true;
            }
        }
        any && rest.trim().is_empty()
    }
}

/// `text` (a kept message) without Cue's `parts` in it: your words alone, blank lines between them as you wrote them.
/// A part's Desk block is left out as it's looked for: a lead's kept message has its block made fresh as it goes
/// (hub/send.rs desk_fresh_held), so the block it was kept with is no longer in it (#868).
fn without_parts(text: &str, parts: &[String]) -> String {
    let mut rest = text.to_string();
    for p in parts.iter().map(|p| crate::desk::without_block(p).trim()).filter(|p| !p.is_empty()) {
        if let Some(i) = rest.find(p) {
            rest.replace_range(i..i + p.len(), "");
        }
    }
    rest.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")
}

impl Sessions {
    /// Cue is about to send `text` to a session: it's Cue's, not yours, wherever it shows up next.
    pub fn cue_says(&mut self, text: &str) {
        self.1.add(text, now_ms());
    }

    /// `text` is Cue's words alone (see `CueSaid::all_cue`).
    pub fn is_cue(&self, text: &str) -> bool {
        self.1.all_cue(text)
    }

    /// Session `session_id`'s latest message of yours marked Cue's when it's Cue's words alone.
    pub(super) fn mark_cue(&mut self, session_id: &str) {
        let Some(s) = self.0.get_mut(session_id) else { return };
        let Some(e) = s.thread.iter_mut().rev().find(|e| e.role == "you") else { return };
        if !e.cue && !e.decision && self.1.all_cue(&e.text) {
            e.cue = true;
        }
    }

    /// Kept message `text` just kept for `session_id` (or put back): noted as Cue's part of it when it's Cue's.
    pub(super) fn held_part(&mut self, session_id: &str, text: &str) {
        if text.trim().is_empty() || !self.1.all_cue(text) {
            return;
        }
        if let Some(h) = self.0.get_mut(session_id).and_then(|s| s.held.as_mut()) {
            h.cue_parts.push(text.trim().to_string());
        }
    }

    /// The kept message for `session_id` to send now when it holds Cue's words and yours (#836): yours go now, alone;
    /// Cue's stay kept, after them, for its next turn, a message of their own (as a Desk lead's are, #831). None when
    /// it doesn't hold both.
    pub fn held_cue_apart(&mut self, session_id: &str) -> Option<(String, String)> {
        let h = self.0.get(session_id)?.held.as_ref()?;
        if h.cue_parts.is_empty() {
            return None;
        }
        let mine = without_parts(&h.text, &h.cue_parts);
        // Nothing of yours left but a Desk block, Cue's too (#868): all Cue's, one message.
        if crate::desk::without_block(&mine).trim().is_empty() {
            return None;
        }
        let cues: Vec<&str> = h.cue_parts.iter().map(|p| crate::desk::without_block(p).trim()).filter(|p| !p.is_empty()).collect();
        Some((mine, cues.join("\n\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_words_are_known_alone_or_kept_together_never_with_yours() {
        let mut c = CueSaid::default();
        c.add("Your DONE for #832 is in, but its check line needs another go.\nWrite it again.", 1);
        c.add("<desk v=\"1\">\nitems\n</desk>\n\nThe Desk is paused: stop at a safe point.", 2);
        assert!(c.all_cue("Your DONE for #832 is in, but its check line needs another go.  Write it again."), "spacing aside");
        assert!(c.all_cue("The Desk is paused: stop at a safe point."), "its Desk block aside");
        assert!(c.all_cue("Your DONE for #832 is in, but its check line needs another go.\nWrite it again.\n\nThe Desk is paused: stop at a safe point."), "two of Cue's, kept together");
        assert!(!c.all_cue("Your DONE for #832 is in, but its check line needs another go.\nWrite it again.\n\nand also fix the header"), "with words of yours");
        assert!(!c.all_cue("fix the header"));
        assert!(!c.all_cue("  "));
    }

    #[test]
    fn cues_words_are_forgotten_after_a_while() {
        let mut c = CueSaid::default();
        c.add("Carry on where you stopped.", 1);
        c.add("Something else.", 1 + KEEP_MS);
        assert!(!c.all_cue("Carry on where you stopped."));
    }

    #[test]
    fn your_words_are_what_is_left_without_cues_parts() {
        let parts = vec!["Your teammate docs has gone quiet.".to_string()];
        assert_eq!(without_parts("fix the header\n\nYour teammate docs has gone quiet.\n\nand the footer", &parts), "fix the header\n\nand the footer");
        assert_eq!(without_parts("Your teammate docs has gone quiet.", &parts), "");
    }

    #[test]
    fn cues_words_to_a_session_are_cues_in_its_chat_and_go_after_yours() {
        let dir = std::env::temp_dir().join(format!("cue-own-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = crate::model::Origin { session_id: "s6".into(), harness: "claude".into(), ..Default::default() };
        let hub = crate::hub::Hub::new(None);
        hub.event(origin, "active", "refactor the router".into(), vec![], "");
        let nudge = "Your teammate docs has gone quiet. Ask it where it is.";
        // Kept while it works: yours and Cue's in one kept message, Cue's known as its part.
        hub.send_to_session("s6", "fix the footer", &[], false).unwrap();
        hub.tell("s6", nudge).unwrap();
        let held = |h: &crate::hub::Hub| h.read(|st| st.sessions.held("s6")).unwrap();
        assert_eq!(held(&hub).text, format!("fix the footer\n\n{nudge}"));
        assert_eq!(held(&hub).cue_parts, vec![nudge.to_string()]);
        // Sent when the turn ends: yours go first, alone; Cue's stay kept for the next turn.
        let now = hub.write(|st| crate::desk::hub::lead::held_yours_first(st, "s6")).unwrap();
        assert_eq!(now.text, "fix the footer");
        assert!(now.cue_parts.is_empty());
        // Esc / Cancel takes back what's kept, as before: Cue's words with yours (a Send fixes you change your mind on).
        assert_eq!(hub.unhold("s6").map(|e| e.text), Some(format!("fix the footer\n\n{nudge}")));
        // In the chat: Cue's words as the agent reports them are Cue's; yours are yours, the same words typed by you
        // never Cue's unless Cue sent them.
        let last_you = |h: &crate::hub::Hub| h.read(|st| st.sessions.thread("s6")).into_iter().rev().find(|e| e.role == "you").unwrap();
        hub.write(|st| st.sessions.note("s6", "you", &format!("{nudge}\n"), 50));
        assert!(last_you(&hub).cue, "Cue's");
        hub.write(|st| st.sessions.note("s6", "agent", "Asked docs.", 50));
        hub.write(|st| st.sessions.note("s6", "you", "fix the footer", 50));
        assert!(!last_you(&hub).cue, "yours");
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
