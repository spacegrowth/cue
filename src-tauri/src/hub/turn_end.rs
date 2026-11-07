//! A turn's end: the reply into its chat, the card that waits for you (or the agent driving it), what follows
//! (its numbers, what it changed, its About, a queued review) and the notification.

use super::*;
use super::asks::notify_title;

/// A queued message taken from the queue this recently was taken as the turn before it ended: it's
/// the next turn's prompt, not the prompt of the turn now ending.
pub(super) const TAKEN_SETTLE_MS: u64 = 3000;

/// Context this full (percent) at a turn's end: the notification says so and offers Compact.
const CONTEXT_HIGH: u8 = 80;

/// What a turn that just ended said, and what came with it, for the chat and its card.
struct TurnEnd {
    message: String,
    /// What a Stop hook's follow-up added after the answer.
    followup: String,
    /// What you sent during the turn that it hasn't read yet: the next turn's prompt, so it goes below this
    /// turn's reply (it was noted when you sent it, above where the reply lands).
    unread: Option<Exchange>,
    /// Its teammates' messages to it since the chat last read them (when, who, what).
    heard: Vec<(u64, String, String)>,
    /// A Stop hook sent it back to work and it stopped again: this turn ended once before, and Cue noted its
    /// answer then. The same answer isn't a second reply (note's own folding misses it when a message of
    /// yours, sent as the turn seemed over, sits between).
    ended_before: bool,
}

/// The card a finished turn put up, and what its notification needs.
struct EndedCard {
    id: String,
    title: (String, String),
    /// You asked to be told ("Tell me").
    watched: bool,
    /// How long the turn took, when Cue saw it start.
    took: Option<u64>,
    /// The same answer as its turn's first end (a Stop hook sent it back): told once already.
    again: bool,
}

impl Hub {
    /// Your message queued during the turn that just ended, unless the turn read it (absorbed between
    /// steps) or it's this turn's own prompt (taken from the queue a while ago).
    fn unread_queued(&self, origin: &Origin) -> Option<crate::model::Exchange> {
        let q = self.read(|st| st.sessions.queued(&origin.session_id))?;
        if origin.transcript_path.is_empty() {
            return Some(q);
        }
        match transcript::read_state(&origin.harness, &origin.transcript_path, &q.text, q.at_ms) {
            Some(transcript::Queued::Absorbed) => None,
            Some(transcript::Queued::Taken(Some(at))) if now_ms().saturating_sub(at) > TAKEN_SETTLE_MS => None,
            _ => Some(q),
        }
    }

    /// Its teammates' messages to it that the chat hasn't shown yet, from its log (where Cue can read it).
    /// The first time, from the start of the turn that just ended (else the last 10 minutes).
    fn teammates_heard(&self, origin: &Origin) -> Vec<(u64, String, String)> {
        let Some(log) = super::send::log_here(origin) else { return vec![] };
        let since = self.read(|st| {
            let read = st.sessions.teammates_read(&origin.session_id);
            if read > 0 { read } else { st.sessions.turn_started(&origin.session_id).unwrap_or_else(|| now_ms().saturating_sub(10 * 60 * 1000)) }
        });
        transcript::teammate_messages_after(log, since)
    }

    pub(super) fn on_stopped(&self, origin: Origin, message: String, turn: &[TurnPart], driven_by: &str) {
        let unread = self.turn_over(&origin);
        let (message, followup) = split_turn(message, turn, &crate::config::turn_mode());
        // What its teammates said to it since the chat last read them: those come mid-turn with no prompt
        // hook, so only its log has them. They go in above the reply that answers them.
        let heard = self.teammates_heard(&origin);
        let end = TurnEnd { message, followup, unread, heard, ended_before: turn.iter().any(|p| p.hooked) };
        // Its turn ended with background agents still at work (Claude Code's Agent tool): it's at its
        // prompt, but its answer isn't in yet; Claude Code wakes it when they report. So it waits on
        // them, not on you, and the chat shows them at work. (A message you send goes straight in.)
        let (helpers, background) = if driven_by.is_empty() && origin.harness == "claude" && !origin.transcript_path.is_empty() {
            (crate::steps::running_helpers(&origin.transcript_path).len() as u64, crate::steps::running_background(&origin.transcript_path) as u64)
        } else {
            (0, 0)
        };
        let driven_by = if helpers > 0 { crate::sessions::helpers_words(helpers) } else { driven_by.to_string() };
        // Another agent drives this session (a helper its lead started, labelled CUE_DRIVEN_BY): its finished turn is for that
        // agent, so it stays in Working as "waiting on its lead" instead of becoming your card.
        if !driven_by.is_empty() && (helpers > 0 || !crate::config::flag("/agents/show_driven")) {
            return self.stopped_for_driver(&origin, end, &driven_by, (helpers, background));
        }
        self.stopped_for_you(origin, end);
    }

    /// A turn is over: a rename it missed while busy is typed, whatever you queued during it has been read
    /// (or is read next), a plan limit is behind us, and a hand-off note it wrote itself is noticed. Returns
    /// what you queued that it hasn't read yet.
    fn turn_over(&self, origin: &Origin) -> Option<Exchange> {
        self.deliver_deferred_rename(&origin.session_id);
        let unread = self.unread_queued(origin);
        self.write(|st| st.sessions.set_queued(&origin.session_id, None));
        if origin.harness == "claude" {
            self.limits_lifted(&origin.session_id);
        }
        // It wrote its hand-off note itself: its card offers "Continue in a new session".
        if crate::ipc::notice_handoff(self, &origin.session_id) {
            self.changed();
        }
        unread
    }

    /// A finished turn that's for the agent driving it (`helpers`: its own background agents, and how many
    /// run in the background), not for you: in Working, waiting on it.
    fn stopped_for_driver(&self, origin: &Origin, end: TurnEnd, driven_by: &str, (helpers, background): (u64, u64)) {
        self.write(|st| {
            let sid = origin.session_id.clone();
            st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
            st.sessions.mark(origin, "agent", None);
            st.sessions.set_driven_by(&sid, driven_by);
            st.sessions.set_helpers(&sid, helpers);
            st.sessions.set_background(&sid, background);
            st.sessions.note_teammates(&sid, &end.heard, crate::config::context_keep());
            if !(end.ended_before && st.sessions.last_reply_is(&sid, &end.message)) {
                st.sessions.note(&sid, "agent", &end.message, crate::config::context_keep());
            }
            if let Some(q) = &end.unread {
                st.sessions.below_reply(&sid, q);
            }
        });
        self.changed();
    }

    /// A finished turn that waits for you: its card, then what follows a turn's end, then the notification.
    fn stopped_for_you(&self, origin: Origin, end: TurnEnd) {
        let (sid, path) = (origin.session_id.clone(), origin.transcript_path.clone());
        let context = if !end.message.trim().is_empty() {
            vec![Ctx { role: "assistant".into(), text: end.message.clone() }]
        } else if !path.is_empty() {
            transcript::recent_context(&path, 2)
        } else {
            vec![]
        };
        let Some(card) = self.write(|st| Self::waiting_for_you(st, origin, end, context)) else { return };
        self.after_turn_ended(&sid, &path);
        self.tell_turn_ended(&sid, &path, card);
    }

    /// The finished turn's card, put up in the store, with the session marked waiting and its reply in its
    /// chat. None when the session has a decision open already: that card is the one in the queue (its turn
    /// can end with one still open: a background agent of it asking), and a "Tell me" stays for the turn end
    /// that does count.
    fn waiting_for_you(st: &mut Store, origin: Origin, end: TurnEnd, context: Vec<Ctx>) -> Option<EndedCard> {
        let sid = origin.session_id.clone();
        if st.items.iter().any(|i| i.kind != "waiting" && i.origin.session_id == sid) {
            return None;
        }
        // A message you kept for it goes now and the turn carries on: a "Tell me" waits for that to end too.
        let watched = st.sessions.held(&sid).is_none() && st.sessions.take_watch(&sid);
        let took = st.sessions.turn_started(&sid).map(|at| now_ms().saturating_sub(at));
        st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
        st.sessions.mark(&origin, "waiting", None);
        st.sessions.set_queued(&sid, None);
        if watched {
            st.sessions.set_told(&sid, took.unwrap_or(0));
        }
        st.sessions.note_teammates(&sid, &end.heard, crate::config::context_keep());
        let thread = st.sessions.thread(&sid);
        let again = end.ended_before && st.sessions.last_reply_is(&sid, &end.message);
        if !again {
            st.sessions.note(&sid, "agent", &end.message, crate::config::context_keep());
        }
        if let Some(q) = &end.unread {
            st.sessions.below_reply(&sid, q);
        }
        let mut it = Self::new_item(st, "waiting", origin);
        it.thread = thread;
        it.context = context;
        it.message = end.message;
        it.followup = end.followup;
        let title = notify_title(&it);
        let id = it.id.clone();
        st.items.push(it);
        Some(EndedCard { id, title, watched, took, again })
    }

    /// What follows a turn's end: its card's numbers, what it changed, the status lists it was asked for, its
    /// About, and a review queued for it if it's a lead.
    fn after_turn_ended(&self, sid: &str, path: &str) {
        // Its card lands with its cache and context numbers, not at the next tick.
        self.cache_tick();
        self.changed();
        self.turn_changes(sid);
        if !path.is_empty() {
            self.status_asks_from(sid, path);
        }
        // A followed session: its About reads this turn (tasks.rs). With Settings' "For every session" on, one
        // that isn't followed yet starts now, from the chat so far. About off: neither.
        if let Some(me) = self.shared().filter(|_| crate::tasks::on()) {
            if crate::tasks::is_tracked(sid) {
                crate::tasks::turn_ended(me, sid);
            } else if crate::config::flag("/tasks/auto") {
                let _ = crate::tasks::start(me, sid);
            }
        }
        // A lead's turn ended: its next queued review goes (after a moment, for its hooks to settle).
        if self.read(|st| st.review_queue.iter().any(|q| q.lead == sid)) {
            self.later(1500, |me| me.drain_reviews());
        }
    }

    /// The finished turn's notification, unless something says you needn't be called back for it.
    fn tell_turn_ended(&self, sid: &str, path: &str, card: EndedCard) {
        // Context high (80%+): the notification says so and offers Compact.
        let full = (!path.is_empty()).then(|| crate::steps::context_pct(path)).flatten().filter(|p| *p >= CONTEXT_HIGH);
        // Put off for later: it waits under Need to decide, without a notification. A message you kept for it
        // goes now: nothing to tell you, it's working again in a moment.
        let (later, held) = self.read(|st| (st.sessions.is_later(sid), st.sessions.held(sid).is_some()));
        // The note Cue asked for while you're away (its tick, or Settings): no need to call you back for it.
        let noted = !path.is_empty() && crate::steps::last_prompt(path).1 && crate::db::handoff(sid).is_some_and(|k| k.ahead > crate::ipc::AHEAD_YOU);
        let EndedCard { id, title, watched, took, again } = card;
        // You asked to be told ("Tell me"): it tells you whatever else would keep it quiet, Cue in front too.
        if watched {
            let took = took.map(|ms| format!("Took {} · ", took_words(ms))).unwrap_or_default();
            self.notify_watched(&id, &title.0, &format!("{took}you asked to be told\n{}", title.1), if full.is_some() { "turn-full" } else { "turn" });
        // The same answer at its turn's second end (a Stop hook sent it back): told once already.
        } else if crate::config::flag("/notify/finished") && !later && !held && !noted && !again {
            match full {
                Some(p) => self.notify(&id, &format!("{} · context {p}%", title.0), &title.1, "turn-full"),
                None => self.notify(&id, &title.0, &title.1, "turn"),
            }
        }
    }
}

/// How long a turn ran, as a notification says it: "45s", "38m", "1h 52m".
fn took_words(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        _ => format!("{}h {}m", s / 3600, s % 3600 / 60),
    }
}

