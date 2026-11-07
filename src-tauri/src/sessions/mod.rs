//! Every live agent session and what it has been doing — the source of the Working column and
//! the 30-minute activity bars (solid = working, striped = waiting on you).
//!
//! The record and its state changes are here; what a session said and how each message of yours
//! got to it are in `thread` and `delivery`, messages kept for later in `held`, names in `names`,
//! what the current turn is doing in `turn`, and your own marks on it (later, star, tell me) in `marks`.

mod delivery;
mod held;
mod marks;
mod names;
mod thread;
mod turn;

pub use delivery::unwrap_pasted;
use delivery::taken;

use crate::model::{now_ms, Exchange, Origin};
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
    /// Background tasks it has running: those agents and its shell commands in the background alike
    /// (the card counts them; `helpers` alone says what it waits on). 0 = none.
    #[serde(default)]
    pub background: u64,
    /// Its agents (background agents and teammates, any state) in the list its chat shows (steps::agent_count): its
    /// cards count these, with a lead's relay agents, so a card and its chat say the same "N agents".
    #[serde(default)]
    pub agents: u64,
    /// A background task of its that failed after running a while: when (0 = none), what it was, and its
    /// output's last line. Said once; cleared when the session's state changes.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub task_failed_ms: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub task_failed: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub task_failed_line: String,
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
    /// What Claude Code's own list of sessions last called it (None: not looked yet). A change there
    /// is a rename made in its terminal (/rename), and Cue's name follows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_name: Option<String>,
    /// A rename Cue typed into Claude Code, until its list shows it: the name, and by when it must.
    #[serde(skip)]
    pub rename_check: Option<(String, u64)>,
    /// Why the agent didn't take the name you gave it in Cue ("": it did, or nothing was tried).
    /// Cue keeps your name either way; the chat says so, with Try again.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rename_error: String,
    /// The usage limit its last turn ran into (state "limited"). Kept after the limit lifts, until
    /// the session runs again, so Cue can offer to resend what didn't run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<crate::logs::usage::Limit>,
    /// Working, but Claude Code can't reach the API and is trying again (usage::retrying): what failed,
    /// which try, when the next one is. Cleared when its state changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<crate::logs::usage::Retry>,
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
    /// Since when its terminal has shown a screen waiting for Enter before Claude Code starts
    /// ("Security notes … Press Enter to continue…"): no hook reaches Cue until it's past it. 0: it isn't.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub enter_ms: u64,
    /// The folder's git state when this turn started (to tell what the turn changed).
    #[serde(skip)]
    pub turn_base: Option<crate::changes::Base>,
    /// Where the latest turn started, kept after its changes are counted, and where the session's
    /// first turn Cue saw started: so an extension can show "this turn" and "the whole session".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_from: Option<crate::changes::Base>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_from: Option<crate::changes::Base>,
    /// What the turn that just ended changed (files, lines); none when it changed nothing. Cleared when
    /// the next turn starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<crate::changes::Changes>,
    /// Up to when (by its log's clock) its teammates' messages to it have been read into the chat; 0 = none yet.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub teammates_read_ms: u64,
    /// Working, but nothing new in its transcript since then (a command waiting for input, or hung); 0 = fine.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub stuck_ms: u64,
    /// You put it off ("Later"): since when; 0 = it isn't. Its finished turns
    /// wait under Need to decide without a notification, until you reply or put it back.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub later_ms: u64,
    /// You asked to be told when this turn is done ("Tell me"): since when; 0 = you didn't. Its finish
    /// always notifies, even with Cue in front, with a sound of its own; cleared once the turn ends.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub watch_ms: u64,
    /// The turn you asked to be told about ended: how long it ran (at least 1); 0 = none. Its chat says
    /// so until the next turn starts.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub told_ms: u64,
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
        s.retry = None;
        s.doing.clear();
        s.doing_ms = 0;
        s.compacting_ms = 0;
        s.compacted_ms = 0;
        s.trust_ms = 0;
        s.enter_ms = 0;
        s.stuck_ms = 0;
        s.later_ms = 0;
        s.watch_ms = 0;
        s.told_ms = 0;
        s.changes = None;
        s.turn_base = None;
        s.rename_pending = false;
        s.rename_check = None;
        s
    }
}

impl Session {
    /// A session's record as Cue first sees it: at `now`, with nothing known about it yet.
    fn first_seen(origin: &Origin, now: u64) -> Session {
        Session {
            origin: origin.clone(),
            project: crate::model::project_of_origin(origin),
            state: String::new(),
            since_ms: now,
            prompt: String::new(),
            segments: vec![],
            thread: vec![],
            driven_by: String::new(),
            helpers: 0,
            background: 0,
            agents: 0,
            task_failed_ms: 0,
            task_failed: String::new(),
            task_failed_line: String::new(),
            queued: None,
            name: String::new(),
            rename_pending: false,
            claude_name: None,
            rename_check: None,
            rename_error: String::new(),
            limit: None,
            retry: None,
            doing: String::new(),
            doing_ms: 0,
            compacting_ms: 0,
            compacted_ms: 0,
            later_ms: 0,
            watch_ms: 0,
            told_ms: 0,
            starred_ms: 0,
            trust_ms: 0,
            enter_ms: 0,
            turn_base: None,
            turn_from: None,
            session_from: None,
            changes: None,
            teammates_read_ms: 0,
            stuck_ms: 0,
            held: None,
        }
    }

    /// An event came from it: take what that tells about where it runs and how far it has started.
    fn heard_from(&mut self, origin: &Origin) {
        // Newer events carry the freshest terminal info (a resumed session may be in a new tab).
        if !origin.tty.is_empty() || !origin.tmux_pane.is_empty() || !origin.iterm_session_id.is_empty() {
            self.origin = origin.clone();
        }
        // A hook reached Cue (only hooks know the transcript): Claude Code runs none until the trust
        // question is answered, so it's through it, whatever ~/.claude.json says about the folder.
        if !origin.transcript_path.is_empty() {
            self.trust_ms = 0;
            self.enter_ms = 0;
        }
    }

    /// It started working: what belonged to its last turn, and waiting on its background work, are over.
    fn back_at_work(&mut self) {
        self.compacting_ms = 0;
        self.compacted_ms = 0;
        self.told_ms = 0;
        self.changes = None;
        // Back at work: no longer waiting on its background agents (their own list, from its log, says
        // which still run). Counted again if its turn ends with some still at work.
        if self.helpers > 0 {
            if self.driven_by == helpers_words(self.helpers) {
                self.driven_by.clear();
            }
            self.helpers = 0;
        }
        self.background = 0;
    }

    /// It entered `state` at `now`: a new state drops what only held for the old one.
    fn enter_state(&mut self, state: &str, now: u64) {
        if self.state != state {
            self.stuck_ms = 0;
            self.task_failed_ms = 0;
            self.task_failed.clear();
            self.task_failed_line.clear();
        }
        if self.state != state {
            self.state = state.to_string();
            self.doing.clear();
            self.retry = None;
            self.since_ms = now;
        }
        if state != "limited" {
            self.limit = None;
        }
    }

    /// The activity bar follows `state`: a new segment when its kind changes, old ones dropped.
    fn extend_bar(&mut self, state: &str, now: u64) {
        // Bar segments: solid = working, striped = waiting on you, faint = waiting on another agent.
        let kind = match state {
            "working" => "working",
            "agent" | "stopped" | "limited" => "idle",
            _ => "waiting",
        };
        match self.segments.last_mut() {
            Some(last) if last.end_ms.is_none() && last.kind == kind => {}
            Some(last) if last.end_ms.is_none() => {
                last.end_ms = Some(now);
                self.segments.push(Segment { kind: kind.into(), start_ms: now, end_ms: None });
            }
            _ => self.segments.push(Segment { kind: kind.into(), start_ms: now, end_ms: None }),
        }
        self.segments.retain(|g| g.end_ms.map_or(true, |e| now.saturating_sub(e) < KEEP_MS));
    }
}

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
        let s = self.0.entry(origin.session_id.clone()).or_insert_with(|| Session::first_seen(origin, now));
        s.heard_from(origin);
        if let Some(p) = prompt.filter(|p| !p.trim().is_empty()) {
            s.prompt = p.trim().to_string();
            taken(s, p, "its prompt hook");
        }
        if state == "working" {
            s.back_at_work();
        }
        s.enter_state(state, now);
        s.extend_bar(state, now);
    }

    /// Each session's project derived again from its folder (hub/projects.rs); true when one changed.
    pub fn reproject(&mut self) -> bool {
        let mut changed = false;
        for s in self.0.values_mut() {
            let p = crate::model::project_of_origin(&s.origin);
            if s.project != p {
                s.project = p;
                changed = true;
            }
        }
        changed
    }

    /// The project of the live session running as process `pid`.
    pub fn project_by_pid(&self, pid: i32) -> Option<String> {
        self.0.values().find(|s| s.origin.agent_pid == Some(pid)).map(|s| s.project.clone())
    }

    /// When its turn started, while it's at it: working, or waiting on its background agents (it works
    /// again when they report, the same turn to you). The first bar segment since it last waited on you.
    pub fn turn_started(&self, session_id: &str) -> Option<u64> {
        let s = self.0.get(session_id).filter(|s| matches!(s.state.as_str(), "working" | "agent"))?;
        let after = s.segments.iter().rposition(|g| g.kind == "waiting").map_or(0, |i| i + 1);
        Some(s.segments.get(after).map_or(s.since_ms, |g| g.start_ms))
    }

    pub fn state(&self, session_id: &str) -> Option<String> {
        self.0.get(session_id).map(|s| s.state.clone())
    }

    pub fn set_prompt(&mut self, session_id: &str, prompt: &str) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.prompt = prompt.trim().to_string();
        }
    }

    pub fn origin(&self, session_id: &str) -> Option<Origin> {
        self.0.get(session_id).map(|s| s.origin.clone())
    }

    pub fn remove(&mut self, session_id: &str) -> bool {
        self.take(session_id).is_some()
    }

    /// Take a session out, keeping its record (what Closed shows for it, its name with it).
    pub fn take(&mut self, session_id: &str) -> Option<Session> {
        self.0.remove(session_id)
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

#[cfg(test)]
mod tests;
