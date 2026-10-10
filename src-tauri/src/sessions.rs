//! Every live agent session and what it has been doing — the source of the Working column and
//! the 30-minute activity bars (solid = working, striped = waiting on you).

use crate::model::{now_ms, project_of, Exchange, Origin};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How far back the activity bar reaches.
pub const WINDOW_MS: u64 = 30 * 60 * 1000;
/// Segments older than this are dropped from memory.
const KEEP_MS: u64 = 2 * WINDOW_MS;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Segment {
    /// "working" | "waiting"
    pub kind: String,
    pub start_ms: u64,
    /// None while it's still going.
    pub end_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    #[serde(flatten)]
    pub origin: Origin,
    pub project: String,
    /// "working" | "waiting" (finished, your turn) | "deciding" (asked for a permission or answer)
    /// | "agent" (finished, but another agent drives it: waiting on that agent, not you)
    /// | "stopped" (you interrupted it from Cue; it's at its prompt)
    pub state: String,
    /// Who drives it when it's in "agent" state, e.g. "its relay lead (…)", or "2 background agents".
    pub driven_by: String,
    /// Background agents it started that are still at work (Claude Code's Agent tool): it's at its
    /// prompt meanwhile, in "agent" state, and Claude Code wakes it when they report. 0 = none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub helpers: u64,
    /// What you sent while it was busy, until it picks it up (its next turn starts or ends).
    pub queued: Option<Exchange>,
    pub since_ms: u64,
    /// What you last asked it to do, when the harness tells us.
    pub prompt: String,
    pub segments: Vec<Segment>,
    /// The last few exchanges (how many: Settings → Context).
    pub thread: Vec<Exchange>,
    /// The name you gave it in Cue ("" = none: Cue shows the project).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// A rename Claude Code hasn't been told yet: it was mid-turn, so it's typed when the turn ends.
    #[serde(default, skip_serializing)]
    pub rename_pending: bool,
    /// The usage limit its last turn ran into (state "limited"). Kept after the limit lifts, until
    /// the session runs again, so Cue can offer to resend what didn't run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<crate::usage::Limit>,
    /// What it's doing right now ("Running: cargo test"): a working Claude Code session's latest step,
    /// read from its transcript every second; cleared when its state changes. And since when.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doing: String,
    #[serde(default)]
    pub doing_ms: u64,
    /// Since when it's compacting (Compact in Cue's ⋯ menu typed `/compact`); 0 = it isn't. Cleared
    /// when its transcript says the compaction finished, or when it starts a turn.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub compacting_ms: u64,
    /// When that compaction finished, so the chat can say so; cleared when the next turn starts.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub compacted_ms: u64,
    /// Started by Cue in a folder Claude Code doesn't trust yet: since when its terminal has been asking
    /// "do you trust this folder?". 0 once it's trusted.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub trust_ms: u64,
    /// The folder's git state when this turn started (to tell what the turn changed).
    #[serde(skip)]
    pub turn_base: Option<crate::changes::Base>,
    /// What the turn that just ended changed (files, lines); none when it changed nothing. Cleared when
    /// the next turn starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<crate::changes::Changes>,
    /// Working, but nothing new in its transcript since then (a command waiting for input, or hung); 0 = fine.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub stuck_ms: u64,
    /// You put it off ("Later"): since when; 0 = it isn't. Its finished turns
    /// wait under Need to decide without a notification, until you reply or put it back.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub later_ms: u64,
    /// You starred it (you're following it): since when; 0 = it isn't. Starred sessions come first
    /// wherever Cue lists them.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub starred_ms: u64,
    /// What you sent while it was working, kept in Cue (not typed into its terminal) and sent when the
    /// turn ends: until then you can take it back into the box (Esc, Edit) or send it now. Its images
    /// are already saved (their paths).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<Exchange>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// A session you parked: its agent ended and its pane closed, its record kept as it was (name, star,
/// recent thread) so Resume brings it back where it left off. It stays until you resume or unpark it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Parked {
    pub parked_ms: u64,
    #[serde(flatten)]
    pub session: Session,
}

impl Parked {
    /// Its record back on a new terminal (`origin`), at its prompt: nothing it was doing or holding
    /// carries over (that ended with its agent).
    pub fn into_session(self, origin: Origin) -> Session {
        let mut s = self.session;
        let now = now_ms();
        s.origin = origin;
        s.state = "waiting".into();
        s.since_ms = now;
        s.segments = vec![Segment { kind: "waiting".into(), start_ms: now, end_ms: None }];
        s.queued = None;
        s.held = None;
        s.limit = None;
        s.doing.clear();
        s.doing_ms = 0;
        s.compacting_ms = 0;
        s.compacted_ms = 0;
        s.trust_ms = 0;
        s.stuck_ms = 0;
        s.later_ms = 0;
        s.changes = None;
        s.turn_base = None;
        s.rename_pending = false;
        s
    }
}

/// Longest text kept per agent message: enough for a full one.
const EXCHANGE_CHARS: usize = 6000;
/// Your own messages are kept whole (up to this): a copy cut short can't be matched with what the
/// agent got, so a long paste used to show twice.
const YOURS_CHARS: usize = 200_000;
/// A long message's echo can come well after it was sent (a big paste takes a while to land).
const LONG_ECHO_MS: u64 = 120_000;

#[derive(Default)]
pub struct Sessions(HashMap<String, Session>);

/// "2 background agents" / "1 background agent"; "" for none.
pub fn helpers_words(n: u64) -> String {
    match n {
        0 => String::new(),
        1 => "1 background agent".into(),
        n => format!("{n} background agents"),
    }
}

impl Sessions {
    /// Record that a session entered `state`. Opens a new bar segment when the kind changes.
    pub fn mark(&mut self, origin: &Origin, state: &str, prompt: Option<&str>) {
        if origin.session_id.is_empty() {
            return;
        }
        let now = now_ms();
        let s = self.0.entry(origin.session_id.clone()).or_insert_with(|| Session {
            origin: origin.clone(),
            project: project_of(&origin.cwd),
            state: String::new(),
            since_ms: now,
            prompt: String::new(),
            segments: vec![],
            thread: vec![],
            driven_by: String::new(),
            helpers: 0,
            queued: None,
            name: String::new(),
            rename_pending: false,
            limit: None,
            doing: String::new(),
            doing_ms: 0,
            compacting_ms: 0,
            compacted_ms: 0,
            later_ms: 0,
            starred_ms: 0,
            trust_ms: 0,
            turn_base: None,
            changes: None,
            stuck_ms: 0,
            held: None,
        });
        // Newer events carry the freshest terminal info (a resumed session may be in a new tab).
        if !origin.tty.is_empty() || !origin.tmux_pane.is_empty() || !origin.iterm_session_id.is_empty() {
            s.origin = origin.clone();
        }
        // A hook reached Cue (only hooks know the transcript): Claude Code runs none until the trust
        // question is answered, so it's through it, whatever ~/.claude.json says about the folder.
        if !origin.transcript_path.is_empty() {
            s.trust_ms = 0;
        }
        if let Some(p) = prompt.filter(|p| !p.trim().is_empty()) {
            s.prompt = p.trim().to_string();
            // It went through after all (it waited behind a command, or the agent's word came late, as it can
            // from a machine): its bubble stops saying it didn't. Compared as the agent got it (spacing
            // changed, image paths after it), not character for character.
            if let Some(e) = s.thread.iter_mut().rev().filter(|e| e.role == "you").take(3).find(|e| e.unsent && crate::transcript::is_message(&s.prompt, &e.text)) {
                e.unsent = false;
            }
        }
        if state == "working" {
            s.compacting_ms = 0;
            s.compacted_ms = 0;
            s.changes = None;
        }
        if s.state != state {
            s.stuck_ms = 0;
        }
        if s.state != state {
            s.state = state.to_string();
            s.doing.clear();
            s.since_ms = now;
        }
        if state != "limited" {
            s.limit = None;
        }
        // Bar segments: solid = working, striped = waiting on you, faint = waiting on another agent.
        let kind = match state {
            "working" => "working",
            "agent" | "stopped" | "limited" => "idle",
            _ => "waiting",
        };
        match s.segments.last_mut() {
            Some(last) if last.end_ms.is_none() && last.kind == kind => {}
            Some(last) if last.end_ms.is_none() => {
                last.end_ms = Some(now);
                s.segments.push(Segment { kind: kind.into(), start_ms: now, end_ms: None });
            }
            _ => s.segments.push(Segment { kind: kind.into(), start_ms: now, end_ms: None }),
        }
        s.segments.retain(|g| g.end_ms.map_or(true, |e| now.saturating_sub(e) < KEEP_MS));
    }

    /// Record one exchange, keeping only the last `keep`.
    pub fn note(&mut self, session_id: &str, role: &str, text: &str, keep: usize) {
        self.note_with(session_id, role, text, &[], keep)
    }

    /// Same, with images. A reply sent from Cue comes back moments later as the agent's prompt
    /// (with the image paths typed after it): that echo is folded into the entry already noted.
    pub fn note_with(&mut self, session_id: &str, role: &str, text: &str, images: &[String], keep: usize) {
        let text = text.trim();
        let Some(s) = self.0.get_mut(session_id) else { return };
        if text.is_empty() && images.is_empty() {
            return;
        }
        if let Some(last) = s.thread.last_mut() {
            let paths: Vec<&str> = images.iter().chain(last.images.iter()).map(String::as_str).collect();
            let age = now_ms().saturating_sub(last.at_ms);
            if last.role == role && (age < 15_000 || (role == "you" && age < LONG_ECHO_MS && is_long(text))) {
                if same_message(text, &last.text, &paths) {
                    // Keep the first copy; Cue's own copy carries the images, so they move onto it.
                    if last.images.is_empty() && !images.is_empty() {
                        last.images = images.to_vec();
                    }
                    return;
                }
                // Typed into a box that still held something (a message you stopped comes back into
                // Claude Code's box): the agent got both, glued. One bubble, saying what it got.
                if role == "you" && glued_after(text, &last.text, &paths) {
                    last.text = text.to_string();
                    return;
                }
            }
        }
        let cap = if role == "you" { YOURS_CHARS } else { EXCHANGE_CHARS };
        let text = if text.chars().count() > cap { format!("{}…", text.chars().take(cap).collect::<String>()) } else { text.to_string() };
        s.thread.push(Exchange { role: role.into(), text, at_ms: now_ms(), images: images.to_vec(), from: String::new(), unsent: false });
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
        self.note_with(session_id, "you", text, images, keep);
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

    /// The project of the live session running as process `pid`.
    pub fn project_by_pid(&self, pid: i32) -> Option<String> {
        self.0.values().find(|s| s.origin.agent_pid == Some(pid)).map(|s| s.project.clone())
    }

    pub fn thread(&self, session_id: &str) -> Vec<Exchange> {
        self.0.get(session_id).map(|s| s.thread.clone()).unwrap_or_default()
    }

    pub fn state(&self, session_id: &str) -> Option<String> {
        self.0.get(session_id).map(|s| s.state.clone())
    }

    pub fn set_prompt(&mut self, session_id: &str, prompt: &str) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.prompt = prompt.trim().to_string();
        }
    }

    /// Your last message to it never went through as a message: flag it, so its bubble says so.
    pub fn mark_unsent(&mut self, session_id: &str) {
        if let Some(e) = self.0.get_mut(session_id).and_then(|s| s.thread.iter_mut().rev().find(|e| e.role == "you")) {
            e.unsent = true;
        }
    }

    /// Working sessions whose log Cue reads for their latest step: (id, harness, log). Claude Code's
    /// transcript or Codex's rollout log; Pi reports its steps itself.
    pub fn working_transcripts(&self) -> Vec<(String, String, String)> {
        self.0.values()
            .filter(|s| s.state == "working" && matches!(s.origin.harness.as_str(), "claude" | "codex") && !s.origin.transcript_path.is_empty())
            .map(|s| (s.origin.session_id.clone(), s.origin.harness.clone(), s.origin.transcript_path.clone()))
            .collect()
    }

    pub fn set_turn_base(&mut self, session_id: &str, base: Option<crate::changes::Base>) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.turn_base = base;
        }
    }

    /// The turn's starting point, taken (each turn's changes are worked out once).
    pub fn take_turn_base(&mut self, session_id: &str) -> Option<crate::changes::Base> {
        self.0.get_mut(session_id)?.turn_base.take()
    }

    /// The finished turn's changes, if it's still that turn (no new one started meanwhile).
    pub fn set_changes(&mut self, session_id: &str, changes: Option<crate::changes::Changes>) -> bool {
        match self.0.get_mut(session_id) {
            Some(s) if s.state != "working" && s.changes != changes => {
                s.changes = changes;
                true
            }
            _ => false,
        }
    }

    /// Stuck since `at_ms` (0: not, or no longer). True when it changed.
    pub fn set_stuck(&mut self, session_id: &str, at_ms: u64) -> bool {
        match self.0.get_mut(session_id) {
            Some(s) if s.stuck_ms != at_ms && (at_ms == 0 || s.state == "working") => {
                s.stuck_ms = at_ms;
                true
            }
            _ => false,
        }
    }

    /// Compacting since `at_ms` (0: done).
    pub fn set_compacting(&mut self, session_id: &str, at_ms: u64) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.compacting_ms = at_ms;
        }
    }

    /// Its compaction finished at `at_ms`.
    pub fn set_compacted(&mut self, session_id: &str, at_ms: u64) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.compacting_ms = 0;
            s.compacted_ms = at_ms;
        }
    }

    /// Waiting for you to trust its folder in its terminal since `at_ms` (0: it isn't).
    pub fn set_trust(&mut self, session_id: &str, at_ms: u64) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.trust_ms = at_ms;
        }
    }

    /// The sessions waiting on a trust question: (session id, folder, since when).
    pub fn asking_trust(&self) -> Vec<(String, String, u64)> {
        self.0.values().filter(|s| s.trust_ms > 0).map(|s| (s.origin.session_id.clone(), s.origin.cwd.clone(), s.trust_ms)).collect()
    }

    /// Since when it's been compacting (0: it isn't).
    pub fn compacting_since(&self, session_id: &str) -> u64 {
        self.0.get(session_id).map_or(0, |s| s.compacting_ms)
    }

    /// The sessions compacting now: (session id, transcript, since when).
    pub fn compacting(&self) -> Vec<(String, String, u64)> {
        self.0.values().filter(|s| s.compacting_ms > 0).map(|s| (s.origin.session_id.clone(), s.origin.transcript_path.clone(), s.compacting_ms)).collect()
    }

    /// Its latest step, if it changed and belongs to this turn. `at_ms` is when the step was written
    /// (0 = now): a step from before it started working is the last turn's, read before the new
    /// prompt reached the log, so it's ignored. True when it changed.
    pub fn set_doing(&mut self, session_id: &str, doing: &str, at_ms: u64) -> bool {
        match self.0.get_mut(session_id) {
            Some(s) if s.state == "working" && s.doing != doing && (at_ms == 0 || at_ms + 1500 >= s.since_ms) => {
                s.doing = doing.to_string();
                s.doing_ms = if at_ms == 0 { now_ms() } else { at_ms };
                true
            }
            _ => false,
        }
    }

    /// Name a session in Cue. Returns what delivering it needs: (harness, state, where it is).
    pub fn set_name(&mut self, session_id: &str, name: &str) -> Option<(String, String, Origin)> {
        let s = self.0.get_mut(session_id)?;
        s.name = name.to_string();
        Some((s.origin.harness.clone(), s.state.clone(), s.origin.clone()))
    }

    /// Remember that the agent still has to be told its new name (it was busy).
    pub fn defer_rename(&mut self, session_id: &str) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.rename_pending = true;
        }
    }

    /// The deferred rename, if any (taken: it's delivered once).
    pub fn take_rename(&mut self, session_id: &str) -> Option<(String, Origin)> {
        let s = self.0.get_mut(session_id)?;
        std::mem::take(&mut s.rename_pending).then(|| (s.name.clone(), s.origin.clone()))
    }

    /// Keep a message for when its turn ends; one already kept gets this one added after it.
    pub fn hold(&mut self, session_id: &str, text: &str, images: Vec<String>) {
        let Some(s) = self.0.get_mut(session_id) else { return };
        match &mut s.held {
            Some(h) => {
                h.text = [h.text.as_str(), text].iter().filter(|t| !t.trim().is_empty()).cloned().collect::<Vec<_>>().join("\n\n");
                h.images.extend(images);
            }
            None => s.held = Some(Exchange { role: "you".into(), text: text.to_string(), at_ms: now_ms(), images, from: String::new(), unsent: false }),
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

    pub fn set_limit(&mut self, session_id: &str, limit: Option<crate::usage::Limit>) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.limit = limit;
        }
    }

    /// Put off for later, or back (`on` false). True if that changed anything.
    pub fn set_later(&mut self, session_id: &str, on: bool) -> bool {
        let Some(s) = self.0.get_mut(session_id) else { return false };
        if (s.later_ms > 0) == on {
            return false;
        }
        s.later_ms = if on { crate::model::now_ms() } else { 0 };
        true
    }

    /// Star it, or unstar it (`on` false). True if that changed anything.
    /// A star carried over to a session's new record (Move to Cue), with when it was first starred.
    pub fn restore_star(&mut self, session_id: &str, starred_ms: u64) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.starred_ms = starred_ms;
        }
    }

    pub fn set_starred(&mut self, session_id: &str, on: bool) -> bool {
        let Some(s) = self.0.get_mut(session_id) else { return false };
        if (s.starred_ms > 0) == on {
            return false;
        }
        s.starred_ms = if on { crate::model::now_ms() } else { 0 };
        true
    }

    pub fn is_later(&self, session_id: &str) -> bool {
        self.0.get(session_id).is_some_and(|s| s.later_ms > 0)
    }

    pub fn set_driven_by(&mut self, session_id: &str, by: &str) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.driven_by = by.to_string();
        }
    }

    /// How many background agents it waits on (see `helpers`); as "agent" state's driver when some.
    pub fn set_helpers(&mut self, session_id: &str, n: u64) -> bool {
        match self.0.get_mut(session_id) {
            Some(s) if s.helpers != n => {
                s.helpers = n;
                s.driven_by = helpers_words(n);
                true
            }
            _ => false,
        }
    }

    /// Sessions at their prompt while their background agents work: (id, origin).
    pub fn with_helpers(&self) -> Vec<Origin> {
        self.0.values().filter(|s| s.state == "agent" && s.helpers > 0).map(|s| s.origin.clone()).collect()
    }

    pub fn origin(&self, session_id: &str) -> Option<Origin> {
        self.0.get(session_id).map(|s| s.origin.clone())
    }

    pub fn remove(&mut self, session_id: &str) -> bool {
        self.0.remove(session_id).is_some()
    }

    /// Everything, unclipped: what Cue saves so a restart doesn't forget live sessions.
    pub fn all(&self) -> Vec<Session> {
        self.0.values().cloned().collect()
    }

    /// Put saved sessions back after a restart. The watcher drops any whose agent has since exited.
    pub fn restore(&mut self, saved: Vec<Session>) {
        for s in saved {
            if !s.origin.session_id.is_empty() {
                self.0.insert(s.origin.session_id.clone(), s);
            }
        }
    }

    pub fn pids(&self) -> Vec<(String, i32)> {
        self.0.values().filter_map(|s| s.origin.agent_pid.map(|p| (s.origin.session_id.clone(), p))).collect()
    }

    /// Sessions for the window, with segments clipped to the last 30 minutes.
    pub fn snapshot(&self) -> Vec<Session> {
        let now = now_ms();
        let from = now.saturating_sub(WINDOW_MS);
        let mut out: Vec<Session> = self
            .0
            .values()
            .map(|s| {
                let mut s = s.clone();
                s.segments = s
                    .segments
                    .into_iter()
                    .filter(|g| g.end_ms.map_or(true, |e| e > from))
                    .map(|mut g| {
                        g.start_ms = g.start_ms.max(from);
                        g
                    })
                    .collect();
                s
            })
            .collect();
        out.sort_by_key(|s| s.since_ms);
        out
    }
}

/// The same message twice: what Cue typed, then the prompt the agent reports back. They can
/// differ at the edges: stray keys already in the terminal, image paths typed after it, line
/// joins. Paths are ignored, and the texts must be nearly the same length, so "ok" and
/// "ok, ship it" sent back to back stay two messages.
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

fn same_message(a: &str, b: &str, paths: &[&str]) -> bool {
    // Your own image paths come out first, as typed (escaped) or as reported back (spaces and all):
    // a path with a space in it would otherwise split into words that don't look like a path.
    let strip = |t: &str| {
        paths.iter().fold(t.to_string(), |acc, p| acc.replace(&crate::uploads::escape_path(p), " ").replace(p, " "))
    };
    let squash = |t: &str| strip(t).split_whitespace().filter(|w| !w.starts_with('/')).collect::<Vec<_>>().join(" ");
    let (a, b) = (squash(a), squash(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // Long ones (a paste): the same start is the same message, whatever became of its whitespace (a
    // terminal may join its lines) or its end (an older copy was cut short, with "…").
    if is_long(&a) && is_long(&b) {
        return crate::transcript::is_message(&a, &b) || crate::transcript::is_message(&b, &a);
    }
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    long.contains(short.as_str()) && short.len() * 5 >= long.len() * 4
}

/// Long enough that its start alone tells it apart: 300 characters that aren't spaces.
fn is_long(t: &str) -> bool {
    t.chars().filter(|c| !c.is_whitespace()).nth(299).is_some()
}

/// `long` is `short` with something typed before it (and `short` is more than a word or two).
fn glued_after(long: &str, short: &str, paths: &[&str]) -> bool {
    let squash = |t: &str| t.split_whitespace().filter(|w| !w.starts_with('/') && !paths.contains(w)).collect::<Vec<_>>().join(" ");
    let (long, short) = (squash(long), squash(short));
    short.len() >= 8 && long.len() > short.len() && long.ends_with(&short)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_text() -> String {
        (0..1600).map(|n| format!("word{n} ")).collect::<String>() + "\nthe end"   // ~10k characters
    }

    #[test]
    fn a_long_message_is_kept_whole_and_shows_once() {
        let mut s = Sessions::default();
        s.mark(&Origin { session_id: "f".into(), ..Default::default() }, "waiting", None);
        let long = long_text();
        let t0 = now_ms();
        s.note("f", "you", &long, 50); // the agent's report of what it got, during the send
        s.note_sent("f", &long, &["/u/1.png".into()], t0, 50); // Cue's own copy, once the send returns
        let t = s.thread("f");
        assert_eq!(t.len(), 1, "one bubble for one message");
        assert_eq!(t[0].text, long.trim(), "kept whole, not cut at 6000");
        assert_eq!(t[0].images, ["/u/1.png"], "Cue's copy brings its images");
    }

    #[test]
    fn what_the_agent_got_is_the_message_however_its_text_came_out() {
        let mut s = Sessions::default();
        s.mark(&Origin { session_id: "g".into(), ..Default::default() }, "waiting", None);
        let t0 = now_ms();
        // Typed into a box that still held something: the agent got both, glued.
        s.note("g", "you", "leftover from before and then what you sent", 50);
        s.note_sent("g", "what you sent", &[], t0, 50);
        let t = s.thread("g");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].text, "leftover from before and then what you sent");
    }

    #[test]
    fn a_long_message_reported_after_cues_copy_folds_in_even_with_its_lines_joined() {
        let mut s = Sessions::default();
        s.mark(&Origin { session_id: "h".into(), ..Default::default() }, "waiting", None);
        let long = long_text();
        s.note_sent("h", &long, &[], now_ms(), 50); // nothing reported yet: Cue's copy goes in
        s.note("h", "you", &long.replace('\n', " "), 50); // then the agent's report, lines joined
        assert_eq!(s.thread("h").len(), 1);
        // An older copy cut short ("…") is still the same message.
        let mut s = Sessions::default();
        s.mark(&Origin { session_id: "i".into(), ..Default::default() }, "waiting", None);
        let cut = format!("{}…", long.chars().take(6000).collect::<String>());
        s.note("i", "you", &cut, 50);
        s.note("i", "you", &long, 50);
        assert_eq!(s.thread("i").len(), 1);
    }

    #[test]
    fn the_same_short_reply_twice_is_two_messages() {
        let mut s = Sessions::default();
        s.mark(&Origin { session_id: "j".into(), ..Default::default() }, "waiting", None);
        s.note("j", "you", "yes", 50);
        s.note("j", "agent", "done", 50);
        s.note_sent("j", "yes", &[], now_ms() + 1, 50); // a second send, after the reply
        assert_eq!(s.thread("j").iter().filter(|e| e.role == "you").count(), 2);
    }

    #[test]
    fn a_paste_split_into_chunks_reads_as_sent() {
        let wrapped = "<pasted_content id=\"bcfe\">\nIs it ready? ✅ **Naming:** c\n</pasted_content id=\"bcfe\">\n\n\n<pasted_content id=\"bcfe\">\nlaude-relay is public.\n\nNext line.\n</pasted_content id=\"bcfe\">\n";
        assert_eq!(unwrap_pasted(wrapped).trim(), "Is it ready? ✅ **Naming:** claude-relay is public.\n\nNext line.");
        assert_eq!(unwrap_pasted("no tags <here>"), "no tags <here>");
        assert_eq!(unwrap_pasted("cut <pasted_content id=\"x\""), "cut <pasted_content id=\"x\"");
    }

    fn origin(sid: &str) -> Origin {
        Origin { session_id: sid.into(), cwd: "/x/proj".into(), harness: "claude".into(), ..Default::default() }
    }

    #[test]
    fn state_changes_open_and_close_segments() {
        let mut s = Sessions::default();
        let o = origin("a");
        s.mark(&o, "working", Some("fix the bug"));
        s.mark(&o, "working", None); // same kind: no new segment
        s.mark(&o, "waiting", None);
        s.mark(&o, "deciding", None); // waiting and deciding share the striped kind
        let snap = s.snapshot();
        assert_eq!(snap.len(), 1);
        let a = &snap[0];
        assert_eq!(a.state, "deciding");
        assert_eq!(a.prompt, "fix the bug");
        assert_eq!(a.project, "proj");
        let kinds: Vec<&str> = a.segments.iter().map(|g| g.kind.as_str()).collect();
        assert_eq!(kinds, vec!["working", "waiting"]);
        assert!(a.segments[0].end_ms.is_some() && a.segments[1].end_ms.is_none());
    }

    #[test]
    fn thread_keeps_the_last_n_and_skips_repeats() {
        let mut s = Sessions::default();
        let o = origin("t");
        s.mark(&o, "working", None);
        for n in 0..7 {
            s.note("t", "agent", &format!("msg {n}"), 5);
        }
        s.note("t", "agent", "msg 6", 5); // repeat of the last: ignored
        s.note("t", "you", "   ", 5); // blank: ignored
        let t = s.thread("t");
        assert_eq!(t.len(), 5);
        assert_eq!(t.first().unwrap().text, "msg 2");
        assert_eq!(t.last().unwrap().text, "msg 6");
        s.note("nobody", "you", "x", 5); // unknown session: no-op, no panic
    }

    #[test]
    fn a_message_glued_to_one_you_stopped_is_one_bubble() {
        let mut s = Sessions::default();
        s.mark(&origin("g"), "working", None);
        s.note("g", "you", "ok commit , push , release", 9);
        s.note("g", "you", "after esc esc we are losing text box focus", 9); // sent from Cue
        // Claude Code had put the stopped message back in its box; it got both, glued.
        s.note("g", "you", "ok commit , push , releaseafter esc esc we are losing text box focus", 9);
        let t: Vec<String> = s.thread("g").iter().map(|e| e.text.clone()).collect();
        assert_eq!(t, ["ok commit , push , release", "ok commit , push , releaseafter esc esc we are losing text box focus"]);
        // A short reply isn't matched by chance.
        s.note("g", "you", "yes", 9);
        s.note("g", "you", "I said yes", 9);
        assert_eq!(s.thread("g").len(), 4);
    }

    #[test]
    fn a_message_queued_during_a_turn_goes_below_that_turns_reply() {
        let mut s = Sessions::default();
        s.mark(&origin("q"), "working", None);
        s.note("q", "you", "fix the header", 9);
        s.note("q", "you", "also the footer", 9); // sent while it worked on the header
        let q = s.thread("q")[1].clone();
        s.note("q", "agent", "Header fixed.", 9);
        s.below_reply("q", &q);
        let t = s.thread("q");
        let order: Vec<&str> = t.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(order, ["fix the header", "Header fixed.", "also the footer"]);
        assert!(t[2].at_ms > t[1].at_ms, "in time order, so a reload keeps it there");
        s.below_reply("q", &q); // already last: stays
        assert_eq!(s.thread("q").len(), 3);
    }

    #[test]
    fn a_reply_and_its_echoed_prompt_are_one_exchange() {
        let mut s = Sessions::default();
        s.mark(&origin("e"), "waiting", None);
        s.note_with("e", "you", "look at this", &["/u/1.png".into()], 5);
        s.note("e", "you", "look at this /u/1.png", 5); // the prompt Claude reports back
        s.note("e", "you", "o,look at  this", 5); // stray keys already in its prompt: still the same message
        let t = s.thread("e");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].images, vec!["/u/1.png".to_string()]);
    }

    #[test]
    fn a_long_forward_and_its_wrapped_echo_are_one_exchange() {
        let mut s = Sessions::default();
        s.mark(&origin("f"), "waiting", None);
        s.note("f", "you", "check this\n\n[Forwarded from the backend session]\nIt's well built. claude-relay is public.", 5);
        let echo = "<pasted_content id=\"a\">\ncheck this\n\n[Forwarded from the backend session]\nIt's well built. c\n</pasted_content id=\"a\">\n\n\n<pasted_content id=\"a\">\nlaude-relay is public.\n</pasted_content id=\"a\">";
        s.note("f", "you", &unwrap_pasted(echo), 5);
        assert_eq!(s.thread("f").len(), 1);
    }

    #[test]
    fn an_image_path_with_a_space_still_folds_into_one_message() {
        let mut s = Sessions::default();
        s.mark(&origin("g"), "waiting", None);
        let p = "/Users/v/Library/Application Support/dev.spacegrowth.cue/uploads/1-0.png";
        s.note_with("g", "you", "can you try showing time above the line", &[p.into()], 5);
        // What Claude reports back: the text plus the path, its space unescaped.
        s.note("g", "you", &format!("can you try showing time above the line {p}"), 5);
        // ...or still escaped, as it was typed.
        s.note("g", "you", &format!("can you try showing time above the line {}", crate::uploads::escape_path(p)), 5);
        let t = s.thread("g");
        assert_eq!(t.len(), 1, "{t:?}");
        assert_eq!(t[0].images, vec![p.to_string()]);
    }

    #[test]
    fn an_echo_that_lands_first_still_gets_the_images_and_short_replies_stay_apart() {
        let mut s = Sessions::default();
        s.mark(&origin("f"), "waiting", None);
        s.note("f", "you", "o,show me the diff /u/2.png", 5); // the agent's report arrives first
        s.note_with("f", "you", "show me the diff", &["/u/2.png".into()], 5);
        let t = s.thread("f");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].text, "o,show me the diff /u/2.png", "keeps what the agent actually got");
        assert_eq!(t[0].images, vec!["/u/2.png".to_string()]);
        s.note("f", "you", "ok", 5);
        s.note("f", "you", "ok, ship it", 5);
        assert_eq!(s.thread("f").len(), 3, "two real messages, not an echo");
    }

    #[test]
    fn sessions_without_an_id_are_ignored_and_remove_works() {
        let mut s = Sessions::default();
        s.mark(&origin(""), "working", None);
        assert!(s.snapshot().is_empty());
        s.mark(&origin("b"), "waiting", None);
        assert!(s.remove("b"));
        assert!(s.snapshot().is_empty());
    }

    #[test]
    fn a_hook_from_the_session_means_its_past_the_trust_question() {
        let mut s = Sessions::default();
        let started = Origin { session_id: "a".into(), cwd: "/tmp".into(), ..Default::default() };
        s.mark(&started, "waiting", None);
        s.set_trust("a", 5);
        // Cue's own bookkeeping (no transcript) leaves it asking.
        s.mark(&started, "waiting", None);
        assert_eq!(s.asking_trust().len(), 1);
        let hook = Origin { session_id: "a".into(), cwd: "/private/tmp".into(), transcript_path: "/t/a.jsonl".into(), ..Default::default() };
        s.mark(&hook, "working", Some("hi"));
        assert!(s.asking_trust().is_empty());
    }

    #[test]
    fn a_message_that_went_through_late_stops_saying_it_didnt() {
        let mut s = Sessions::default();
        s.mark(&origin("a"), "waiting", None);
        s.note("a", "you", "run the tests", 10);
        s.mark_unsent("a");
        s.mark(&origin("a"), "working", Some("something else"));
        assert!(s.0["a"].thread.last().unwrap().unsent, "a different prompt leaves it flagged");
        s.mark(&origin("a"), "working", Some("run the tests"));
        assert!(!s.0["a"].thread.last().unwrap().unsent);
    }

    #[test]
    fn a_late_message_counts_as_the_agent_got_it() {
        let mut s = Sessions::default();
        s.mark(&origin("a"), "waiting", None);
        s.note("a", "you", "look at this\nscreenshot", 10);
        s.mark_unsent("a");
        // Its line break joined up, an image's path after it: still the same message.
        s.mark(&origin("a"), "working", Some("look at this screenshot /Users/me/cue/uploads/1.png"));
        assert!(!s.0["a"].thread.last().unwrap().unsent);
    }
}
