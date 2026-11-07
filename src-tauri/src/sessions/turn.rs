//! What a session's current turn is doing: its changes, being stuck, compacting, a folder to trust,
//! retries, its latest step, and the background work it waits on.

use super::{helpers_words, Sessions};
use crate::model::{now_ms, Origin};

impl Sessions {
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
            if let Some(b) = &base {
                s.turn_from = Some(b.clone());
                if s.session_from.is_none() {
                    s.session_from = Some(b.clone());
                }
            }
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

    /// What its terminal shows before Claude Code starts, as last read off its screen (None: neither
    /// screen). True when that changed.
    pub fn set_setup(&mut self, session_id: &str, setup: Option<crate::terminal::dialog::Setup>, now: u64) -> bool {
        use crate::terminal::dialog::Setup;
        let Some(s) = self.0.get_mut(session_id) else { return false };
        let was = (s.trust_ms, s.enter_ms);
        let keep = |since: u64, on: bool| if !on { 0 } else if since > 0 { since } else { now };
        s.trust_ms = keep(s.trust_ms, setup == Some(Setup::Trust));
        s.enter_ms = keep(s.enter_ms, setup == Some(Setup::Enter));
        was != (s.trust_ms, s.enter_ms)
    }

    /// Claude Code sessions in a tmux pane Cue can read that no hook has come from yet: they may be on a
    /// screen before it starts (trust, Enter to continue).
    pub fn before_start(&self) -> Vec<Origin> {
        self.0.values().filter(|s| s.origin.harness == "claude" && s.origin.transcript_path.is_empty() && !s.origin.tmux_pane.is_empty()).map(|s| s.origin.clone()).collect()
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
    /// A working session's retry (None: it isn't retrying). True if that changed anything.
    pub fn set_retry(&mut self, session_id: &str, retry: Option<crate::logs::usage::Retry>) -> bool {
        match self.0.get_mut(session_id) {
            Some(s) if s.retry != retry && (retry.is_none() || s.state == "working") => {
                s.retry = retry;
                true
            }
            _ => false,
        }
    }

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

    pub fn set_limit(&mut self, session_id: &str, limit: Option<crate::logs::usage::Limit>) {
        if let Some(s) = self.0.get_mut(session_id) {
            s.limit = limit;
        }
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
    /// How many background tasks it runs (agents and shell commands). True when it changed.
    pub fn set_background(&mut self, session_id: &str, n: u64) -> bool {
        match self.0.get_mut(session_id) {
            Some(s) if s.background != n => {
                s.background = n;
                true
            }
            _ => false,
        }
    }

    /// A background task of its failed after a while (`at_ms`: when): noted on the session. True when new.
    pub fn set_task_failed(&mut self, session_id: &str, at_ms: u64, desc: &str, line: &str) -> bool {
        match self.0.get_mut(session_id) {
            Some(s) if s.task_failed_ms != at_ms => {
                s.task_failed_ms = at_ms;
                s.task_failed = desc.to_string();
                s.task_failed_line = line.to_string();
                true
            }
            _ => false,
        }
    }

    /// Claude Code sessions with a transcript, for their agent count: (id, transcript, at work: working or waiting on
    /// its agents).
    pub fn agent_transcripts(&self) -> Vec<(String, String, bool)> {
        self.0.values().filter(|s| s.origin.harness == "claude" && !s.origin.transcript_path.is_empty())
            .map(|s| (s.origin.session_id.clone(), s.origin.transcript_path.clone(), s.state == "working" || s.state == "agent")).collect()
    }

    /// Its agent count (see `agents`). True when it changed.
    pub fn set_agents(&mut self, session_id: &str, n: u64) -> bool {
        match self.0.get_mut(session_id) {
            Some(s) if s.agents != n => {
                s.agents = n;
                true
            }
            _ => false,
        }
    }

    pub fn with_helpers(&self) -> Vec<Origin> {
        self.0.values().filter(|s| s.state == "agent" && s.helpers > 0).map(|s| s.origin.clone()).collect()
    }
}
