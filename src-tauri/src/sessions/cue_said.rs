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

/// What kind of message of Cue's it is (#837), set where it's sent, never read from its words. The window shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An item handed to a session ("Desk item #id (kind): …").
    Brief,
    /// Where an item goes, told to its holder: moved, a copy given or taken back, parked, paused, resumed, closed,
    /// dropped, open again, waiting for its branch.
    Route,
    /// The lead's NOTE, an option picked, an item folded into another.
    Note,
    /// Done? stuck? a teammate gone quiet, Carry on after a usage limit, continue; Resume or Stop of a background agent.
    Nudge,
    /// DONE and CHECK follow-ups: it isn't done, a check line to write again.
    FollowUp,
    /// To a lead: its Desk's changes, blocks and feed, the Desk paused or resumed, Away notices.
    Desk,
    /// To a lead: a session's TO LEAD, a cleanup question, an ASK passed on, a wait circle.
    Question,
    /// To a lead: Cue's answers to its own lines (route results, SHOW, refusals, Queued:, lands answered).
    Answer,
    /// To a crew's lead: its teammates' reviews, fixes, stop, commit, hand out, carried over.
    Crew,
    /// A hand-off note asked for, a fresh session's start from that note, a new lead's hello or hand-off.
    HandOff,
    /// Another Desk's lead (TELL DESK).
    Peer,
    /// Anything else of Cue's.
    Notice,
}

impl Kind {
    pub const ALL: [Kind; 12] = [Kind::Brief, Kind::Route, Kind::Note, Kind::Nudge, Kind::FollowUp, Kind::Desk, Kind::Question, Kind::Answer, Kind::Crew, Kind::HandOff, Kind::Peer, Kind::Notice];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Brief => "brief",
            Kind::Route => "route",
            Kind::Note => "note",
            Kind::Nudge => "nudge",
            Kind::FollowUp => "followup",
            Kind::Desk => "desk",
            Kind::Question => "question",
            Kind::Answer => "answer",
            Kind::Crew => "crew",
            Kind::HandOff => "handoff",
            Kind::Peer => "peer",
            Kind::Notice => "notice",
        }
    }

    /// The kind `name` says (`as_str`); one Cue doesn't know is a Notice.
    pub fn named(name: &str) -> Kind {
        Kind::ALL.into_iter().find(|k| k.as_str() == name).unwrap_or(Kind::Notice)
    }
}

/// Who a message to a session is from, said where it's sent: yours, or Cue's of a kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Said {
    You,
    Cue(Kind),
}

/// What Cue sent lately, each as `plain` makes it, its kind, and when.
#[derive(Default)]
pub struct CueSaid(Vec<(String, Kind, u64)>);

/// `text` as Cue's words are compared: a Desk lead's `<desk>` block left out, every run of whitespace one space.
fn plain(text: &str) -> String {
    crate::desk::without_block(text).split_whitespace().collect::<Vec<_>>().join(" ")
}

impl CueSaid {
    fn add(&mut self, text: &str, kind: Kind, now: u64) {
        let t = plain(text);
        if t.is_empty() {
            return;
        }
        self.0.retain(|(_, _, at)| now.saturating_sub(*at) < KEEP_MS);
        self.0.push((t, kind, now));
        let excess = self.0.len().saturating_sub(MOST);
        self.0.drain(..excess);
    }

    /// You sent `text` yourself: the same words Cue sent before are yours this time, not Cue's.
    fn forget(&mut self, text: &str) {
        let t = plain(text);
        self.0.retain(|(k, _, _)| *k != t);
    }

    /// `text` is Cue's words alone: one message it sent, or several it sent kept and gone together.
    pub fn all_cue(&self, text: &str) -> bool {
        self.kind_of(text).is_some()
    }

    /// The kind `text` was sent as when it's Cue's words alone (`all_cue`): several kept and gone together take the
    /// first one's kind; the same words sent twice, the latest's.
    pub fn kind_of(&self, text: &str) -> Option<Kind> {
        let mut rest = plain(text);
        if rest.is_empty() {
            return None;
        }
        if let Some((_, k, _)) = self.0.iter().rev().find(|(t, _, _)| *t == rest) {
            return Some(*k);
        }
        let mut known: Vec<(&str, Kind)> = self.0.iter().rev().map(|(t, k, _)| (t.as_str(), *k)).collect();
        known.sort_by_key(|(t, _)| std::cmp::Reverse(t.len()));
        let mut first: Option<(usize, Kind)> = None;
        for (t, k) in known {
            if let Some(i) = rest.find(t) {
                rest.replace_range(i..i + t.len(), "");
                if first.is_none_or(|(at, _)| i < at) {
                    first = Some((i, k));
                }
            }
        }
        first.filter(|_| rest.trim().is_empty()).map(|(_, k)| k)
    }
}

/// `text` (a kept message) without Cue's `parts` in it: your words alone, blank lines between them as you wrote them.
fn without_parts(text: &str, parts: &[String]) -> String {
    let mut rest = text.to_string();
    for p in parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        if let Some(i) = rest.find(p) {
            rest.replace_range(i..i + p.len(), "");
        }
    }
    rest.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")
}

impl Sessions {
    /// Cue is about to send `text` to a session as a message of kind `kind`: it's Cue's, not yours, wherever it
    /// shows up next.
    pub fn cue_says(&mut self, text: &str, kind: Kind) {
        self.1.add(text, kind, now_ms());
    }

    /// You are about to send `text`: yours, even when Cue sent the same words a while ago.
    pub fn you_say(&mut self, text: &str) {
        self.1.forget(text);
    }

    /// The kind Cue sent `text` as, when it's Cue's words alone.
    pub fn cue_kind(&self, text: &str) -> Option<Kind> {
        self.1.kind_of(text)
    }

    /// Session `session_id`'s latest message of yours marked Cue's when it's Cue's words alone.
    pub(super) fn mark_cue(&mut self, session_id: &str) {
        let Some(s) = self.0.get_mut(session_id) else { return };
        let Some(e) = s.thread.iter_mut().rev().find(|e| e.role == "you") else { return };
        if e.cue || e.decision {
            return;
        }
        if let Some(k) = self.1.kind_of(&e.text) {
            e.cue = true;
            e.kind = k.as_str().to_string();
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
        if mine.is_empty() {
            return None;
        }
        Some((mine, h.cue_parts.join("\n\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_words_are_known_alone_or_kept_together_never_with_yours() {
        let mut c = CueSaid::default();
        c.add("Your DONE for #832 is in, but its check line needs another go.\nWrite it again.", Kind::FollowUp, 1);
        c.add("<desk v=\"1\">\nitems\n</desk>\n\nThe Desk is paused: stop at a safe point.", Kind::Desk, 2);
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
        c.add("Carry on where you stopped.", Kind::Nudge, 1);
        c.add("Something else.", Kind::Notice, 1 + KEEP_MS);
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
        hub.tell("s6", nudge, Kind::Nudge).unwrap();
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
        assert_eq!(last_you(&hub).kind, "nudge", "of the kind it was sent as");
        hub.write(|st| st.sessions.note("s6", "agent", "Asked docs.", 50));
        hub.write(|st| st.sessions.note("s6", "you", "fix the footer", 50));
        assert!(!last_you(&hub).cue, "yours");
        assert_eq!(last_you(&hub).kind, "", "yours have no kind");
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_kind_cue_sends_is_that_kind_in_the_chat_and_the_same_words_from_you_are_yours() {
        let dir = std::env::temp_dir().join(format!("cue-kinds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = crate::model::Origin { session_id: "s7".into(), harness: "claude".into(), ..Default::default() };
        let hub = crate::hub::Hub::new(None);
        hub.event(origin, "active", "refactor the router".into(), vec![], "");
        let last_you = |h: &crate::hub::Hub| h.read(|st| st.sessions.thread("s7")).into_iter().rev().find(|e| e.role == "you").unwrap();
        // Kept while it works; each comes back in its chat as the agent reports it.
        for k in Kind::ALL {
            let words = format!("A message of kind {}.", k.as_str());
            hub.tell("s7", &words, k).unwrap();
            hub.write(|st| st.sessions.note("s7", "you", &words, 50));
            let e = last_you(&hub);
            assert!(e.cue, "{words} is Cue's");
            assert_eq!(e.kind, k.as_str(), "{words}");
            hub.write(|st| st.sessions.note("s7", "agent", "OK.", 50));
        }
        // The same words, sent by you: yours, with no kind.
        let words = format!("A message of kind {}.", Kind::Nudge.as_str());
        hub.send_to_session("s7", &words, &[], false).unwrap();
        hub.write(|st| st.sessions.note("s7", "you", &words, 50));
        let e = last_you(&hub);
        assert!(!e.cue && e.kind.is_empty(), "yours: {e:?}");
        // A kind the window names that Cue doesn't know is a notice.
        assert_eq!(Kind::named("nudge"), Kind::Nudge);
        assert_eq!(Kind::named("whatever"), Kind::Notice);
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
