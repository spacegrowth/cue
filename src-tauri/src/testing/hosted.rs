//! The hosted sessions' stand-in (the hub's `Hosted` port): it starts nothing, records what Cue asked of each
//! session (started or resumed with what, sent, answered, stopped, closed) and fails a start when a test says
//! so. What a session reports, a test hands the hub itself (`World::hosted_says`).

use crate::hosted::{Agent, Answer, Hosted, Report, Spec, Start, Started};
use std::collections::HashSet;
use std::sync::Mutex;

/// One start: the agent, the conversation (new or resumed), its permissions and arguments.
#[derive(Clone, Debug)]
pub struct HostedLaunch {
    pub agent: Agent,
    pub start: Start,
    pub cwd: String,
    pub permissions: String,
    pub args: Vec<String>,
}

#[derive(Default)]
pub struct FakeHosted {
    launched: Mutex<Vec<HostedLaunch>>,
    running: Mutex<HashSet<String>>,
    sent: Mutex<Vec<(String, String)>>,
    answers: Mutex<Vec<(String, String, Answer)>>,
    interrupted: Mutex<Vec<String>>,
    closed: Mutex<Vec<String>>,
    ended: Mutex<Vec<i32>>,
    refusal: Mutex<Option<String>>,
    /// Sessions run by cue-host (they go on when Cue quits); the rest are Cue's own children.
    in_host: Mutex<HashSet<String>>,
}

/// The process ids the stand-in hands out: none a real process has (they're past the system's largest).
const FIRST_PID: i32 = 9_000_000;

impl FakeHosted {
    /// Session `id` runs under cue-host: it doesn't end when Cue quits.
    pub fn under_host(&self, id: &str) {
        crate::locking::lock(&self.in_host, "fake hosted under the host").insert(id.into());
    }

    /// Process ids Cue asked to end as left by the Cue before it.
    pub fn ended_left(&self) -> Vec<i32> {
        crate::locking::lock(&self.ended, "fake hosted leftovers").clone()
    }

    pub fn launched(&self) -> Vec<HostedLaunch> {
        crate::locking::lock(&self.launched, "fake hosted launches").clone()
    }

    /// Messages sent: (session, text).
    pub fn sent(&self) -> Vec<(String, String)> {
        crate::locking::lock(&self.sent, "fake hosted sends").clone()
    }

    /// Answers given: (session, request, answer).
    pub fn answers(&self) -> Vec<(String, String, Answer)> {
        crate::locking::lock(&self.answers, "fake hosted answers").clone()
    }

    pub fn interrupted(&self) -> Vec<String> {
        crate::locking::lock(&self.interrupted, "fake hosted stops").clone()
    }

    pub fn closed(&self) -> Vec<String> {
        crate::locking::lock(&self.closed, "fake hosted closes").clone()
    }

    /// Its process exited (Claude died, or Cue quit): no longer running.
    pub fn exits(&self, session_id: &str) {
        crate::locking::lock(&self.running, "fake hosted running").remove(session_id);
    }

    /// Starts fail with `why` from now on.
    pub fn refuse(&self, why: &str) {
        *crate::locking::lock(&self.refusal, "fake hosted refusal") = Some(why.to_string());
    }

    fn live(&self, session_id: &str) -> Result<(), String> {
        if self.running(session_id) { Ok(()) } else { Err("it isn't running".into()) }
    }
}

impl Hosted for FakeHosted {
    /// Codex picks a new conversation's id: the stand-in's is `codex-<n>`, its rollout file named by it.
    fn start(&self, spec: &Spec, _report: Report) -> Result<Started, String> {
        if let Some(why) = crate::locking::lock(&self.refusal, "fake hosted refusal").clone() {
            return Err(why);
        }
        let launch = HostedLaunch { agent: spec.agent, start: spec.start.clone(), cwd: spec.cwd.to_string_lossy().into_owned(), permissions: spec.permissions.clone(), args: spec.args.clone() };
        let n = {
            let mut l = crate::locking::lock(&self.launched, "fake hosted launches");
            l.push(launch);
            l.len() as i32
        };
        let (session_id, transcript) = match (&spec.agent, &spec.start) {
            (Agent::Codex, Start::New(_)) => (format!("codex-{n}"), Some(format!("/codex/rollout-codex-{n}.jsonl"))),
            _ => (spec.start.session_id().to_string(), None),
        };
        crate::locking::lock(&self.running, "fake hosted running").insert(session_id.clone());
        Ok(Started { pid: FIRST_PID + n, session_id, transcript, in_turn: false })
    }

    fn send(&self, session_id: &str, text: &str) -> Result<(), String> {
        self.live(session_id)?;
        crate::locking::lock(&self.sent, "fake hosted sends").push((session_id.into(), text.into()));
        Ok(())
    }

    fn answer(&self, session_id: &str, request_id: &str, answer: &Answer) -> Result<(), String> {
        self.live(session_id)?;
        crate::locking::lock(&self.answers, "fake hosted answers").push((session_id.into(), request_id.into(), answer.clone()));
        Ok(())
    }

    fn interrupt(&self, session_id: &str) -> Result<(), String> {
        self.live(session_id)?;
        crate::locking::lock(&self.interrupted, "fake hosted stops").push(session_id.into());
        Ok(())
    }

    fn close(&self, session_id: &str) {
        self.exits(session_id);
        crate::locking::lock(&self.closed, "fake hosted closes").push(session_id.into());
    }

    fn running(&self, session_id: &str) -> bool {
        crate::locking::lock(&self.running, "fake hosted running").contains(session_id)
    }

    fn end_left(&self, pids: &[i32]) {
        crate::locking::lock(&self.ended, "fake hosted leftovers").extend_from_slice(pids);
    }

    /// As Cue's own children: every one running ends with Cue (`in_host` takes them out).
    fn in_cue(&self) -> Vec<String> {
        let host = crate::locking::lock(&self.in_host, "fake hosted under the host").clone();
        crate::locking::lock(&self.running, "fake hosted running").iter().filter(|s| !host.contains(*s)).cloned().collect()
    }

    fn close_all(&self) {
        let all: Vec<String> = crate::locking::lock(&self.running, "fake hosted running").drain().collect();
        crate::locking::lock(&self.closed, "fake hosted closes").extend(all);
    }
}
