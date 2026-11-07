//! Relay's crews as relay's files would give them to Cue (leads::index::INDEX), for the World's tests: a lead and its
//! executors, each reported unless said otherwise, installed with `install`.

use crate::leads::{install_index, reported_exec, Member};
use std::collections::HashMap;
use std::path::PathBuf;

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

    /// As relay's files say now: Cue's next look at them sees this. Relay's own command is a stand-in in the
    /// test's home (testing::sandbox), so what Cue asks relay to do (close an executor, stop a lead) is recorded,
    /// never run by the relay you have installed.
    pub fn install(self) {
        stand_in_relay();
        install_index(self.0);
    }
}

/// The stand-in's record of what relay was asked: one line of arguments per call.
fn relay_log() -> PathBuf {
    home().join("relay-calls.log")
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("the test's home (testing::sandbox)"))
}

/// Relay's command as the installed plugin gives it (`~/.claude/plugins/installed_plugins.json` → `bin/relay`),
/// in the test's home: a script that writes down its arguments and does nothing else.
fn stand_in_relay() {
    let plugin = home().join(".claude/plugins/cache/claude-relay/relay/0.0.0-test");
    let bin = plugin.join("bin/relay");
    if bin.is_file() {
        return;
    }
    std::fs::create_dir_all(plugin.join("bin")).unwrap();
    let plugins = serde_json::json!({ "plugins": { "relay@claude-relay": [{ "installPath": plugin }] } });
    std::fs::write(home().join(".claude/plugins/installed_plugins.json"), plugins.to_string()).unwrap();
    std::fs::write(&bin, format!("#!/bin/sh\necho \"$@\" >> '{}'\n", relay_log().display())).unwrap();
    std::fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
}

/// What relay was asked to do since the tests started, one call's arguments per line.
pub fn relay_calls() -> Vec<String> {
    std::fs::read_to_string(relay_log()).unwrap_or_default().lines().map(str::to_string).collect()
}
