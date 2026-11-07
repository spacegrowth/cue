//! A feed's background work: the agents and shell commands its session started in the background,
//! taken up when they launch or are messaged, and marked finished when their task notice comes.

use super::feed::Feed;
use super::helper::{helper_path, notifications, Helper, TaskFailure, FAILURE_WORTH_MS};
use super::queries::helper_done;
use serde_json::Value;

impl Feed {
    /// A background agent just started (its launch result came in): remember it and open its log. What
    /// its launch doesn't say (its type; a teammate's description) its meta file does, read with its log.
    pub(super) fn add_helper(&mut self, step: &str, extra: &Value, at_ms: u64) {
        let s = |k: &str| extra.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        self.add_agent(step, s("agentId"), s("description"), s("resolvedModel"), at_ms);
        if s("status") == "teammate_spawned" {
            if let Some(h) = self.helpers.iter_mut().find(|h| h.id == s("agentId")) {
                h.teammate = true;
                h.name = s("name");
            }
        }
    }

    /// A background agent taken up (None when it's known already, or has no id). No file is read here
    /// (the feed's lock is held): its log and meta file are read on the catch-up's next round.
    fn add_agent(&mut self, step: &str, id: String, desc: String, model: String, at_ms: u64) -> Option<&mut Helper> {
        if id.is_empty() || self.helpers.iter().any(|h| h.id == id) {
            return None;
        }
        // A session on another machine: its agent's log is copied over (to where it's read from here).
        crate::machines::logs::copy_helper(&self.path, &id);
        let feed = helper_path(&self.path, &id).and_then(|p| p.to_str().map(|p| Box::new(Feed::open_helper(p))));
        self.helpers.push(Helper { id: id.clone(), step: step.to_string(), desc, kind: String::new(), model, started_ms: at_ms, finished_ms: 0, last_ms: at_ms, tokens: 0, items: Vec::new(), more: 0, feed, script: false, command: String::new(), output: Vec::new(), lines: 0, failed: false, stopped: false, out_path: None, out_len: 0, closed: false, resumed_ms: 0, teammate: false, name: String::new(), meta_read: false, seen: false, unconfirmed: false });
        self.trim_helpers();
        self.helpers.iter_mut().find(|h| h.id == id)
    }

    /// The session sent one of its background agents a message (Claude Code's SendMessage), which put it back
    /// to work: it writes on to the same log, and its task notifies again when it's done. At work again, now
    /// under the step that sent it; one not known here (read from mid-file, or dropped) is taken up from here.
    pub(super) fn resume_helper(&mut self, step: &str, id: &str, at_ms: u64) {
        if id.is_empty() {
            return;
        }
        let Some(h) = self.helpers.iter_mut().find(|h| h.id == id) else {
            // Not one of its agents Cue knows: kept only once its log is found (a message to anything else, or an
            // agent whose log can't be read, would read as at work for good). Named by its meta file, read with it.
            if let Some(h) = self.add_agent(step, id.to_string(), String::new(), String::new(), at_ms) {
                h.resumed_ms = at_ms;
                h.unconfirmed = true;
            }
            return;
        };
        if h.script {
            return;
        }
        h.finished_ms = 0;
        h.last_ms = at_ms;
        h.resumed_ms = at_ms;
        h.step = step.to_string();
        // Its copy from another machine was stopped when it reported: on again, from where it was.
        crate::machines::logs::copy_helper(&self.path, id);
    }

    /// A shell command went to the background (Claude Code's Bash tool, run in the background or moved
    /// there on a timeout): remember it, with its output file from the launch text. Named by the
    /// description the session gave it, else the command.
    pub(super) fn add_script(&mut self, step: &str, desc: &str, subject: &str, command: &str, launch: &str, extra: &Value, at_ms: u64) {
        let id = extra.get("backgroundTaskId").and_then(Value::as_str).unwrap_or("").to_string();
        if id.is_empty() || self.helpers.iter().any(|h| h.id == id) {
            return;
        }
        let out_path = launch.split("Output is being written to: ").nth(1).and_then(|r| r.split_whitespace().next()).map(|p| std::path::PathBuf::from(p.trim_end_matches('.')));
        // A session on another machine: its output file is there, and is copied over (to where it's read from here), like an agent's log.
        let out_path = out_path.and_then(|remote| match crate::machines::logs::copy_task(&self.path, &id, &remote.to_string_lossy()) {
            Some(local) => Some(local),
            None if crate::machines::logs::is_copy(&self.path) => None,
            None => Some(remote),
        });
        let desc = if desc.trim().is_empty() { subject.to_string() } else { desc.trim().to_string() };
        self.helpers.push(Helper { id, step: step.to_string(), desc, kind: "script".into(), model: String::new(), started_ms: at_ms, finished_ms: 0, last_ms: at_ms, tokens: 0, items: Vec::new(), more: 0, feed: None, script: true, command: command.trim().to_string(), output: Vec::new(), lines: 0, failed: false, stopped: false, out_path, out_len: 0, closed: false, resumed_ms: 0, teammate: false, name: String::new(), meta_read: false, seen: false, unconfirmed: false });
        self.trim_helpers();
    }

    /// The oldest finished ones go first; what's running is never dropped.
    fn trim_helpers(&mut self) {
        while self.helpers.len() > self.max_helpers {
            match self.helpers.iter().position(|h| !h.running()) {
                Some(i) => { self.helpers.remove(i); }
                None => break,
            }
        }
    }

    /// Claude Code told the session a background task finished: every `<task-notification>` in `text`
    /// (it comes as the prompt of a queued command, or wrapped in a system notice in a user message,
    /// two at a time sometimes). True when one of this session's tasks ended on it.
    pub(super) fn helpers_finished(&mut self, text: &str, at_ms: u64) -> bool {
        notifications(text).into_iter().fold(false, |any, n| self.helper_finished(n, at_ms) || any)
    }

    /// One task notice: mark its task finished (once; a notice can be read twice).
    fn helper_finished(&mut self, notification: &str, at_ms: u64) -> bool {
        let tag = |t: &str| notification.split(&format!("<{t}>")).nth(1).and_then(|r| r.split(&format!("</{t}>")).next()).map(str::trim).unwrap_or("").to_string();
        let (id, status) = (tag("task-id"), tag("status"));
        let Some(h) = self.helpers.iter_mut().find(|h| h.id == id) else { return false };
        if status == "running" || h.finished_ms != 0 {
            return false;
        }
        // An agent that stopped to wait on background work of its own: "completed", but not done. Its
        // task notifies again when it is.
        if tag("summary").contains("waiting on its own background work") || tag("note").contains("background work of its own still running") {
            return false;
        }
        h.finished_ms = at_ms.max(1);
        if h.script {
            h.stopped = status == "killed";
            h.failed = !h.stopped && status != "completed";
            // Its last lines, written before the notice: its output file is read after the log that says so.
            return true;
        }
        // Copied from another machine: its log is complete once it reports (a moment for its last lines),
        // unless its session sent it a message meanwhile.
        let (copy, id) = (self.path.clone(), h.id.clone());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if helper_done(&copy, &id) {
                crate::machines::logs::stop_helper(&copy, &id);
            }
        });
        true
    }

    /// A script that ended (its notice came, or its output file ends on the exit line), seen to once its
    /// output has been read (its last line says how a failure went): its copy from another machine stopped, and a failure after a while noted for the hub.
    pub(super) fn close_scripts(&mut self) {
        for h in self.helpers.iter_mut().filter(|h| h.script && h.seen && h.finished_ms > 0 && !h.closed) {
            h.closed = true;
            let (copy, id) = (self.path.clone(), h.id.clone());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(5));
                crate::machines::logs::stop_task(&copy, &id);
            });
            if h.failed && h.finished_ms.saturating_sub(h.started_ms) > FAILURE_WORTH_MS {
                self.failures.push(TaskFailure { path: self.path.clone(), at_ms: h.finished_ms, desc: h.desc.clone(), line: h.last_line() });
            }
        }
    }
}
