//! Hosts on other machines (machine_host.rs puts cue-host there): one connection per machine to the host of Cue's own
//! build, made the first time a session there needs it and kept (it's made again when it drops, as this Mac's is);
//! and, once per machine, the older builds' hosts found there, let finish what they run (old_hosts.rs).

use super::host_link::HostLink;
use super::hosts::{rejoin_when_back, Remotes};
use super::machine_host;
use super::old_hosts::{self, OldHost};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(super) struct OnMachines {
    /// By machine name.
    links: Mutex<HashMap<String, Arc<HostLink>>>,
    /// Machines whose older hosts have been looked for.
    looked: Mutex<HashSet<String>>,
}

impl OnMachines {
    /// The connection to machine `name`'s host: the one there is, or a new one (cue-host put there first if it
    /// isn't, and the host started if it isn't running). Why not, when it can't be had.
    pub(super) fn link(&self, name: &str, remotes: &Remotes) -> Result<Arc<HostLink>, String> {
        if let Some(l) = crate::locking::lock(&self.links, "machine hosts: links").get(name).cloned() {
            return Ok(l);
        }
        let m = crate::machines::get(name).ok_or_else(|| format!("{name} isn't one of your machines any more (+ New → Machine)"))?;
        machine_host::ensure(&m)?;
        let link = HostLink::over(machine_host::dial(&m)).map_err(|e| format!("{e} on {}", m.name))?;
        // ssh starts whether or not the machine answers: a host that answers is what says it's there.
        if let Err(e) = old_hosts::hello(&link) {
            link.shut();
            // What the host there said last, if it said anything (its log), says more than "no answer".
            let log = crate::machines::run(&m.host, &format!("tail -3 \"$HOME/{}/host.log\" 2>/dev/null; true", machine_host::home())).unwrap_or_default();
            let said = log.lines().last().map(|l| format!("; its log: {}", l.trim())).unwrap_or_default();
            return Err(format!("cue-host on {} didn't answer ({e}{said})", m.name));
        }
        rejoin_when_back(&link, remotes);
        // Another thread may have made one meanwhile: theirs is kept (and this one closed, outside the lock).
        let theirs = {
            let mut all = crate::locking::lock(&self.links, "machine hosts: links");
            let theirs = all.get(name).cloned();
            if theirs.is_none() {
                all.insert(name.to_string(), link.clone());
            }
            theirs
        };
        match theirs {
            Some(l) => {
                link.shut();
                Ok(l)
            }
            None => Ok(link),
        }
    }

    /// The older builds' hosts on machine `name` still running, the first time it's asked of that machine (none
    /// after): for the caller to keep and let finish.
    pub(super) fn older(&self, name: &str) -> Vec<OldHost> {
        if !crate::locking::lock(&self.looked, "machine hosts: looked").insert(name.to_string()) {
            return vec![];
        }
        let Some(m) = crate::machines::get(name) else { return vec![] };
        old_hosts::find_there(&m.host, &m.name, cue_host::proto::BUILD)
    }

    pub(super) fn all(&self) -> Vec<(String, Arc<HostLink>)> {
        crate::locking::lock(&self.links, "machine hosts: links").iter().map(|(n, l)| (n.clone(), l.clone())).collect()
    }

    /// Cue quitting: each connection closed (the hosts and their agents go on).
    pub(super) fn shut_all(&self) {
        let all: Vec<Arc<HostLink>> = crate::locking::lock(&self.links, "machine hosts: links").drain().map(|(_, l)| l).collect();
        for l in all {
            l.shut();
        }
    }

    /// For Settings: each machine's host, reachable or not, and how many agents it runs.
    pub(super) fn status(&self) -> Vec<Value> {
        self.all()
            .into_iter()
            .map(|(name, l)| {
                let running = old_hosts::hello(&l).map(|h| h.sessions.iter().filter(|s| s.running).count()).unwrap_or(0);
                json!({ "machine": crate::machines::display_name(&name), "reachable": l.connected(), "running": running })
            })
            .collect()
    }
}
