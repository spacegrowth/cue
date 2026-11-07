//! Relay's crews as relay's files would give them to Cue (leads::index::INDEX), for the World's tests: a lead and its
//! executors, each reported unless said otherwise, installed with `install`.

use crate::leads::{install_index, reported_exec, Member};
use std::collections::HashMap;

#[derive(Default)]
pub struct Crew(HashMap<String, Member>);

impl Crew {
    /// Relay lead `sid`, its project named after it.
    pub fn lead(mut self, sid: &str) -> Self {
        self.0.insert(sid.into(), Member { role: "lead".into(), plugin: "relay".into(), name: sid.into(), ..Default::default() });
        self
    }

    /// Lead `sid`, which took over from `predecessor` (a relay hand-off) at `started_ms`.
    pub fn successor(mut self, sid: &str, predecessor: &str, started_ms: u64) -> Self {
        self = self.lead(sid);
        let m = self.0.get_mut(sid).unwrap();
        (m.predecessor, m.started_ms) = (predecessor.into(), started_ms);
        self
    }

    /// Executor `sid` of `lead`, reported on its packet 1, topic "<sid>-topic", relay id "relay-<sid>".
    pub fn exec(self, sid: &str, lead: &str) -> Self {
        self.exec_as(sid, lead, |_| {})
    }

    /// The same, with `f` changing what relay says of it (its status, packet, machine…).
    pub fn exec_as(mut self, sid: &str, lead: &str, f: impl FnOnce(&mut Member)) -> Self {
        let mut m = Member { name: format!("{sid}-topic"), id: format!("relay-{sid}"), lead: lead.into(), lead_name: lead.into(), ..reported_exec() };
        f(&mut m);
        self.0.insert(sid.into(), m);
        self
    }

    /// As relay's files say now: Cue's next look at them sees this.
    pub fn install(self) {
        install_index(self.0);
    }
}
