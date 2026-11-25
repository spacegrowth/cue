//! Idle sessions at rest (#773): a session Cue runs itself keeps its agent's process, a few hundred megabytes of it,
//! for as long as it sits at its prompt. Once one has been idle for Settings → Agents' "Free an idle session's memory
//! after" (config::rest_minutes: 30 minutes unless you say, 0 never), Cue ends that process and nothing else: the
//! session stays where it is, paused with its conversation, as one whose agent exited by itself, and the next
//! message, note or item sent to it starts it again on that conversation (hosted.rs wake_hosted).
//!
//! Only the process Cue itself started for the session, through the port that started it (`Hosted::close`): never
//! a session in a terminal (its process is the terminal's), never one on another machine, never by name or pattern.
//! Never one at work, asking you something, compacting, with a message on its way or kept for it (or just woken for one), with a teammate,
//! a background agent or a background command still going (they would end with it), or holding a Desk item that is
//! landing. One whose cache is still warm waits until it runs out, so coming back to it soon costs what it would have.
//! On the ~10 s tick, off the main thread (usage.rs).

use super::*;
use crate::sessions::Session;

/// Whether `s` can be put to rest now: idle for `after_ms`, with nothing that would be lost or cut short.
pub(crate) fn can_rest(st: &Store, s: &Session, now: u64, after_ms: u64) -> bool {
    let sid = s.origin.session_id.as_str();
    let ours = s.origin.hosted && s.origin.agent_pid.is_some() && s.origin.machine.is_empty();
    let idle = matches!(s.state.as_str(), "waiting" | "stopped") && now.saturating_sub(s.since_ms) >= after_ms;
    if !ours || !idle {
        return false;
    }
    let going = s.helpers > 0 || s.background > 0 || !s.running.is_empty() || s.team.iter().any(|t| matches!(t.state.as_str(), "working" | "queued" | "reviewing"));
    // A message of yours on its way counts too, and so does a wake for one (#922): Cue started it again moments ago,
    // its agent hasn't said its turn began, so the session still reads as idle since its last reply hours back.
    let sending = s.queued.is_some() || s.held.is_some() || s.kept_on_way.is_some() || s.compacting_ms > 0
        || st.hosted_waking.contains_key(sid) || s.thread.iter().any(|e| e.role == "you" && e.pending);
    let asking = st.items.iter().any(|i| i.origin.session_id == sid && i.status == "pending" && i.kind != "waiting");
    let landing = st.desk.items.iter().any(|i| i.state == "finishing" && i.holder.as_ref().is_some_and(|h| h.is(sid)));
    let warm = st.insight.get(sid).and_then(|v| v["until_ms"].as_u64()).is_some_and(|until| until > now);
    !(going || sending || asking || landing || warm)
}

impl Hub {
    /// Put to rest every session that has been idle long enough (see the module's note).
    pub(crate) fn rest_idle(&self) {
        let after_ms = crate::config::rest_minutes() * 60_000;
        if after_ms == 0 {
            return;
        }
        let now = now_ms();
        let idle: Vec<String> = self.read(|st| st.sessions.all().iter().filter(|s| can_rest(st, s, now, after_ms)).map(|s| s.origin.session_id.clone()).collect());
        for sid in idle {
            self.rest(&sid, after_ms);
        }
    }

    /// End `sid`'s agent and leave the session paused. Under its typing gate, tried and never waited on: a message
    /// being sent to it right now means it isn't idle (the next tick looks again), and one sent while it's being
    /// ended waits here, then finds it paused and starts it again. Looked at once more under the gate.
    fn rest(&self, sid: &str, after_ms: u64) {
        let gate = self.typing_gate(sid);
        let Some(_typing) = super::send::turn_if_free(&gate) else { return };
        let still = self.read(|st| st.sessions.get(sid).is_some_and(|s| can_rest(st, s, now_ms(), after_ms)));
        if !still || !self.hosted().running(sid) {
            return;
        }
        self.hosted().close(sid);
        self.write(|st| {
            st.sessions.set_pid(sid, None);
            st.sessions.team_process_ended(sid, now_ms());
        });
        eprintln!("cue: {sid} was idle: its agent is ended, and its next message starts it again");
        self.changed();
    }
}
