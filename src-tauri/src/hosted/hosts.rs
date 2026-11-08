//! Hosted sessions run by cue-host (host/): the agents keep running when Cue quits, restarts or is updated, and
//! a Cue that comes back picks each one up where it left off. Behind the same `Hosted` port as `Running` (the
//! agents as Cue's own children), which is what's used while the host can't be reached (tried again before each
//! start, at most every 30 s). Those end if Cue quits (`in_cue`): Cue asks first (hub/quitting.rs).
//!
//! Each session's records from the host (its lines, Cue's lines to it, its exit) are handled in order on a thread
//! of the session's own, never on the connection's: what they set off can write to the session again, and that
//! write's answer comes over the connection.
//!
//! Cue keeps, per session, the last record it handled (`seen/`). Started again, it reads the current run's records
//! up to there with nothing reported (`Driver::catch_up`: the protocol as it was, and each ask still waiting
//! reported again, its card having gone with the Cue that had it), then attaches for what came after.
//!
//! New sessions always go to the host of Cue's own build (`home`); hosts of other builds still running agents are
//! let finish them, then stopped (old_hosts.rs). Restart cue-host ends them all at once (`restart_host`).

use super::driver::Driver;
use super::host_link::{Failed, HostLink};
use super::old_hosts::{self, OldHost};
use super::on_machines::OnMachines;
use super::remote::{logged, sink, Cursor, Remote};
use super::running::{Hosted, Reattached, Report, Running, Started};
use super::session::wait_open;
use super::{launch, Answer, Spec};
use cue_host::proto::{Event, Record, SessionInfo};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

/// How often a host that couldn't be reached is tried again (before a session starts).
const RETRY_EVERY: Duration = Duration::from_secs(30);
/// How often older hosts are looked at, to stop each once it runs nothing.
const DRAIN_EVERY: Duration = Duration::from_secs(15);

pub(super) type Remotes = Arc<Mutex<HashMap<String, Arc<Remote>>>>;
type Olds = Arc<Mutex<Vec<OldHost>>>;

pub struct Hosts {
    /// Where the host of Cue's own build lives, and how to start it there.
    home: PathBuf,
    serve: Vec<String>,
    /// None while the host can't be reached: sessions started meanwhile run as Cue's own children (`in_cue`).
    link: Mutex<Option<Arc<HostLink>>>,
    /// When reaching it was last tried and failed.
    failed_at: Mutex<Option<Instant>>,
    /// Hosts of other builds, finishing what they run.
    old: Olds,
    direct: Running,
    /// The sessions run by a host, by the conversation's id.
    remotes: Remotes,
    /// Where each session's last handled record is kept.
    seen: PathBuf,
    /// The hosts on other machines.
    machines: OnMachines,
}

impl Hosts {
    /// Through the host of Cue's own build in `home` (started with `serve` if it isn't running), with hosts of other
    /// builds under `root` let finish; what's been read kept in `seen`. While it can't be reached, sessions run as
    /// Cue's own children, as before the host, and it's tried again.
    pub fn new(root: PathBuf, home: PathBuf, serve: Vec<String>, seen: PathBuf) -> Hosts {
        Cursor::prune(&seen);
        let old: Olds = Arc::new(Mutex::new(old_hosts::find(&root, &home)));
        let hosts = Hosts { home, serve, link: Mutex::new(None), failed_at: Mutex::new(None), old, direct: Running::default(), remotes: Arc::default(), seen, machines: OnMachines::default() };
        hosts.link();
        hosts
    }

    /// The connection to the host: the one there is, or a new one (tried at most every RETRY_EVERY once it failed).
    fn link(&self) -> Option<Arc<HostLink>> {
        if let Some(l) = crate::locking::lock(&self.link, "hosts: link").clone() {
            return Some(l);
        }
        let recently = crate::locking::lock(&self.failed_at, "hosts: link tried").is_some_and(|t| t.elapsed() < RETRY_EVERY);
        if recently {
            return None;
        }
        match HostLink::connect(self.home.clone(), self.serve.clone()) {
            Ok(l) => {
                rejoin_when_back(&l, &self.remotes);
                *crate::locking::lock(&self.link, "hosts: link") = Some(l.clone());
                Some(l)
            }
            Err(e) => {
                eprintln!("cue: {e}; hosted sessions run inside Cue until it can be reached");
                *crate::locking::lock(&self.failed_at, "hosts: link tried") = Some(Instant::now());
                None
            }
        }
    }

    /// Every host on this Mac Cue is connected to: its own build's first, then older ones.
    fn links(&self) -> Vec<Arc<HostLink>> {
        let current = crate::locking::lock(&self.link, "hosts: link").clone();
        current.into_iter().chain(crate::locking::lock(&self.old, "hosts: older hosts").iter().filter(|o| o.machine.is_none()).map(|o| o.link.clone())).collect()
    }

    fn remote(&self, session_id: &str) -> Option<Arc<Remote>> {
        crate::locking::lock(&self.remotes, "hosts: sessions").get(session_id).cloned()
    }

    /// The session's records go to its own thread, handled in order, and come from the host from now on.
    fn listen(&self, r: &Arc<Remote>, link: &HostLink) {
        let (tx, rx) = channel::<Event>();
        let handler = r.clone();
        std::thread::spawn(move || {
            for e in rx {
                handler.handle(e);
            }
        });
        *crate::locking::lock(&r.queue, "hosts: a session's queue") = Some(tx.clone());
        link.route(&r.host_id, Arc::new(move |e| {
            let _ = tx.send(e);
        }));
    }

    /// The session under `session_id` taken out: the host stops sending its records here.
    fn drop_remote(&self, session_id: &str) -> Option<Arc<Remote>> {
        take_remote(&self.remotes, session_id)
    }

    fn start_hosted(&self, link: &Arc<HostLink>, spec: &Spec, report: Report) -> Result<Started, String> {
        let host_id = spec.start.session_id().to_string();
        if let Some(old) = self.drop_remote(&host_id) {
            old.close();
        }
        // On a machine, the agent runs with your environment there, which only the machine knows.
        let there = !spec.machine.is_empty();
        let launch::HostCommand { argv, env, cwd } = if there { launch::for_machine(spec)? } else { launch::for_host(spec)? };
        let meta = json!({ "agent": spec.agent.program(), "cwd": spec.cwd, "session": host_id, "machine": spec.machine });
        let r = Arc::new(Remote::new(&host_id, Driver::new(spec, sink(report)), link.clone(), &self.seen));
        let started = link.request(json!({ "op": "start", "session": host_id, "argv": argv, "env": env, "cwd": cwd, "meta": meta, "inherit": there }))?;
        let pid = started["pid"].as_i64().unwrap_or(0) as i32;
        r.pid.store(pid, Ordering::SeqCst);
        r.running.store(true, Ordering::SeqCst);
        r.set_cursor(started["after"].as_u64().unwrap_or(0));
        self.listen(&r, link);
        let opened = r.attach().map_err(|e| e.to_string()).and_then(|()| r.write_lines(&r.driver.opening())).and_then(|()| wait_open(&r.driver, || !r.running.load(Ordering::SeqCst)));
        if let Err(e) = opened {
            // One that exited at once says why on its error output (a folder that isn't there, a program that
            // isn't): its last words, once its records are in (its exit comes after them).
            let until = Instant::now() + Duration::from_millis(1500);
            while r.running.load(Ordering::SeqCst) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(25));
            }
            let words = r.driver.last_words();
            r.close();
            r.link.unroute(&host_id);
            return Err(match words.lines().map(str::trim).filter(|l| !l.is_empty()).last() {
                Some(why) if !r.running.load(Ordering::SeqCst) => format!("{} exited: {why}", r.driver.name),
                _ => e,
            });
        }
        let session_id = r.driver.session_id();
        r.named(&session_id);
        crate::locking::lock(&self.remotes, "hosts: sessions").insert(session_id.clone(), r.clone());
        Ok(Started { pid, session_id, transcript: r.driver.transcript(), in_turn: false })
    }

    /// Session `info` on `link`, as Cue last read it (its cursor), picked up: its current run's records up to there
    /// read again with nothing reported; then (`go`) each ask it still waits on reported again, and what came after.
    /// None when there's nothing to pick up (it ended, and Cue read all of it).
    fn pick_up(&self, link: &Arc<HostLink>, info: SessionInfo, spec: &Spec, report: Report) -> Option<Reattached> {
        let sid = spec.start.session_id().to_string();
        let host_id = info.session.clone();
        let cursor = Cursor::find(&self.seen, &sid).map_or(0, |(_, c)| c);
        if !info.running && cursor >= info.last_seq {
            return None;
        }
        let r = Arc::new(Remote::new(&host_id, Driver::new(spec, sink(report)), link.clone(), &self.seen));
        r.named(&sid);
        let from = info.run_first.saturating_sub(1);
        let upto = cursor.max(from);
        let mut waiting = vec![];
        if upto > from {
            let read: Vec<Record> = link.request(json!({ "op": "read", "session": host_id, "after": from, "upto": upto })).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
            waiting = r.driver.catch_up(read.into_iter().filter_map(|rec| logged(rec.what)));
        }
        r.set_cursor(upto);
        r.running.store(info.running, Ordering::SeqCst);
        r.pid.store(info.pid.unwrap_or(0), Ordering::SeqCst);
        self.listen(&r, link);
        crate::locking::lock(&self.remotes, "hosts: sessions").insert(sid.clone(), r.clone());
        let started = info.running.then(|| Started { pid: info.pid.unwrap_or(0), session_id: r.driver.session_id(), transcript: r.driver.transcript(), in_turn: r.driver.in_turn() });
        let go = Box::new(move || {
            r.driver.ask_again(waiting);
            if let Err(e) = r.attach() {
                eprintln!("cue: couldn't attach {}: {e}", r.host_id);
            }
        });
        Some(Reattached { started, go })
    }

    /// The hosts on machine `name` that may run a session of Cue's: its host of Cue's build (none if it can't be
    /// reached now), then any older builds' found there (kept, let finish what they run, like this Mac's).
    fn links_on(&self, name: &str) -> Vec<Arc<HostLink>> {
        let current = self.machines.link(name, &self.remotes).map_err(|e| eprintln!("cue: {e}")).ok();
        let older = self.machines.older(name);
        let links: Vec<Arc<HostLink>> = older.iter().map(|o| o.link.clone()).collect();
        if !older.is_empty() {
            crate::locking::lock(&self.old, "hosts: older hosts").extend(older);
            self.watch_old();
        }
        current.into_iter().chain(links).collect()
    }

    /// The connection to its own build's host dropped, as the network would drop it (it's made again).
    #[cfg(test)]
    pub(super) fn drop_connection(&self) {
        if let Some(l) = crate::locking::lock(&self.link, "hosts: link").clone() {
            l.drop_connection();
        }
    }

    /// Whether it's connected to its own build's host now.
    #[cfg(test)]
    pub(super) fn connected(&self) -> bool {
        crate::locking::lock(&self.link, "hosts: link").as_ref().is_some_and(|l| l.connected())
    }

    /// Looking at the older hosts every DRAIN_EVERY until none is left (each stopped once it runs nothing).
    fn watch_old(&self) {
        let (old, remotes): (Weak<Mutex<Vec<OldHost>>>, Weak<Mutex<HashMap<String, Arc<Remote>>>>) = (Arc::downgrade(&self.old), Arc::downgrade(&self.remotes));
        std::thread::spawn(move || loop {
            std::thread::sleep(DRAIN_EVERY);
            let (Some(old), Some(remotes)) = (old.upgrade(), remotes.upgrade()) else { return };
            if drain(&old, &remotes) == 0 {
                return;
            }
        });
    }
}

/// The sessions on `link`.
fn on_link(remotes: &Remotes, link: &Arc<HostLink>) -> Vec<Arc<Remote>> {
    crate::locking::lock(remotes, "hosts: sessions").values().filter(|r| Arc::ptr_eq(&r.link, link)).cloned().collect()
}

/// Once `link`'s dropped connection is back, its sessions attach again after what they've read, what was held for
/// them goes (`back`), and the files copied over it go on (tails.rs).
pub(super) fn rejoin_when_back(link: &Arc<HostLink>, remotes: &Remotes) {
    let (all, me) = (Arc::downgrade(remotes), Arc::downgrade(link));
    link.on_back(move || {
        let (Some(all), Some(me)) = (all.upgrade(), me.upgrade()) else { return };
        for r in on_link(&all, &me) {
            back(&all, &r);
        }
        super::tails::resume(&me);
    });
}

/// Session `r`, its connection made again: attached after what it's read, then what was held for it sent. A host
/// that no longer has it (it died, and this is a fresh one) took its agent with it: it's let go, said to have exited
/// (it shows paused, and a message starts it again), and what was held for it said not taken.
fn back(remotes: &Remotes, r: &Arc<Remote>) {
    match r.attach() {
        Ok(()) => r.flush(),
        Err(Failed::Refused(e)) => {
            eprintln!("cue: cue-host no longer has {} ({e}): its agent went with the host it ran on", r.host_id);
            take_remote(remotes, &r.driver.session_id());
            r.drop_pending("its agent stopped when cue-host did; send it again to start it");
            if r.running.swap(false, Ordering::SeqCst) {
                r.driver.exited(None);
            }
        }
        // Dropped again already: this is done again once it's back.
        Err(Failed::Unreachable(e)) => eprintln!("cue: couldn't attach {} again: {e}", r.host_id),
    }
}

/// The session the host on `link` runs under `id`, while it runs.
fn running_there(link: &HostLink, id: &str) -> Option<SessionInfo> {
    old_hosts::hello(link).ok()?.sessions.into_iter().find(|s| s.session == id && s.running)
}

fn take_remote(remotes: &Remotes, session_id: &str) -> Option<Arc<Remote>> {
    let r = crate::locking::lock(remotes, "hosts: sessions").remove(session_id)?;
    r.link.unroute(&r.host_id);
    crate::locking::lock(&r.queue, "hosts: a session's queue").take();
    Some(r)
}

/// Every session on `link` let go. `gone`: its host went with them (it died): each still running is said to have
/// exited, so it shows paused and a message starts it on the current host.
fn let_go(remotes: &Remotes, link: &Arc<HostLink>, gone: bool) {
    let all: Vec<(String, Arc<Remote>)> = crate::locking::lock(remotes, "hosts: sessions").iter().filter(|(_, r)| Arc::ptr_eq(&r.link, link)).map(|(k, r)| (k.clone(), r.clone())).collect();
    for (id, r) in all {
        take_remote(remotes, &id);
        if gone {
            r.drop_pending("its agent stopped with the older cue-host it ran on; send it again to start it");
            if r.running.swap(false, Ordering::SeqCst) {
                r.driver.exited(None);
            }
        }
    }
}

/// The older hosts looked at once: each that's gone, or runs nothing any more, let go (and stopped, and its folder
/// cleared). How many are left.
fn drain(old: &Olds, remotes: &Remotes) -> usize {
    // Asked outside the list's lock: each is a question to a host.
    let links: Vec<Arc<HostLink>> = crate::locking::lock(old, "hosts: older hosts").iter().map(|o| o.link.clone()).collect();
    for link in links {
        if link.connected() && old_hosts::busy(&link) {
            continue;
        }
        let gone = !link.connected();
        let_go(remotes, &link, gone);
        let mut all = crate::locking::lock(old, "hosts: older hosts");
        if let Some(i) = all.iter().position(|o| Arc::ptr_eq(&o.link, &link)) {
            let o = all.remove(i);
            drop(all);
            eprintln!("cue: an older cue-host (pid {}) runs nothing any more: stopped", o.pid);
            old_hosts::stop(&o);
        }
    }
    crate::locking::lock(old, "hosts: older hosts").len()
}

impl Hosted for Hosts {
    /// On this Mac through its host, or as Cue's own child while that can't be reached. On a machine through the
    /// host there, never anything else (why not, when it can't be reached); one the host there already runs under
    /// this id (Cue lost sight of it: the machine couldn't be reached when Cue started) is picked up, not started.
    fn start(&self, spec: &Spec, report: Report) -> Result<Started, String> {
        if !spec.machine.is_empty() {
            let link = self.machines.link(&spec.machine, &self.remotes)?;
            if let Some(info) = running_there(&link, spec.start.session_id()) {
                let back = self.pick_up(&link, info, spec, report).ok_or("it runs there, but Cue couldn't pick it up")?;
                (back.go)();
                return back.started.ok_or_else(|| "it ended as Cue picked it up".to_string());
            }
            return self.start_hosted(&link, spec, report);
        }
        match self.link() {
            Some(link) => self.start_hosted(&link, spec, report),
            None => self.direct.start(spec, report),
        }
    }

    fn send(&self, session_id: &str, text: &str) -> Result<(), String> {
        match self.remote(session_id) {
            Some(r) => r.send_message(text),
            None => self.direct.send(session_id, text),
        }
    }

    fn answer(&self, session_id: &str, request_id: &str, answer: &Answer) -> Result<(), String> {
        match self.remote(session_id) {
            Some(r) => r.write_lines(&r.driver.answer(request_id, answer)?),
            None => self.direct.answer(session_id, request_id, answer),
        }
    }

    fn interrupt(&self, session_id: &str) -> Result<(), String> {
        match self.remote(session_id) {
            Some(r) => r.write_lines(&r.driver.interrupt()),
            None => self.direct.interrupt(session_id),
        }
    }

    fn close(&self, session_id: &str) {
        match self.drop_remote(session_id) {
            Some(r) => r.close(),
            None => self.direct.close(session_id),
        }
    }

    /// Whether its agent runs. While the host can't be reached that isn't known: it's taken as running, so
    /// nothing starts a second one beside it (a message to it is refused, saying why, until the host is back).
    fn running(&self, session_id: &str) -> bool {
        match self.remote(session_id) {
            Some(r) => !r.link.connected() || r.running.load(Ordering::SeqCst),
            None => self.direct.running(session_id),
        }
    }

    /// Cue quitting: the agents the hosts run go on (that's what they're for); only those run as Cue's own end.
    fn close_all(&self) {
        let all: Vec<String> = crate::locking::lock(&self.remotes, "hosts: sessions").keys().cloned().collect();
        for id in all {
            self.drop_remote(&id);
        }
        self.machines.shut_all();
        self.direct.close_all();
    }

    /// Only those run as Cue's own children: the hosts' go on without it.
    fn in_cue(&self) -> Vec<String> {
        self.direct.in_cue()
    }

    /// Those processes, of a Cue that's gone, still running on their own: not any host's (they're meant to run on).
    fn end_left(&self, pids: &[i32]) {
        let hosts: Vec<i32> = self.links().iter().filter_map(|l| old_hosts::hello(l).ok()).flat_map(|h| h.sessions.into_iter().filter_map(|s| s.pid)).collect();
        let left: Vec<i32> = pids.iter().copied().filter(|p| !hosts.contains(p)).collect();
        self.direct.end_left(&left);
    }

    /// A session a host ran for an earlier Cue (this build's host or an older one), picked up: its current run's
    /// records up to where that Cue had read, read again with nothing reported. Then (`go`) each ask it still waits
    /// on reported again, and what came after, as it comes. None when no host has anything for it.
    fn reattach(&self, spec: &Spec, report: Report) -> Option<Reattached> {
        let (host_id, _) = Cursor::find(&self.seen, spec.start.session_id())?;
        let links = if spec.machine.is_empty() {
            self.link();
            self.links()
        } else {
            self.links_on(&spec.machine)
        };
        let (link, info) = links.into_iter().find_map(|l| {
            let info = old_hosts::hello(&l).ok()?.sessions.into_iter().find(|s| s.session == host_id)?;
            Some((l, info))
        })?;
        self.pick_up(&link, info, spec, report)
    }

    /// Every session Cue knows has been picked up: what an older host still runs that no session of Cue's is (Cue
    /// forgot it) is closed, so the host can go; older hosts running nothing are stopped now, and the rest watched.
    fn settled(&self) {
        let olds: Vec<Arc<HostLink>> = crate::locking::lock(&self.old, "hosts: older hosts").iter().map(|o| o.link.clone()).collect();
        for link in olds {
            let ours: Vec<String> = on_link(&self.remotes, &link).iter().map(|r| r.host_id.clone()).collect();
            let theirs = old_hosts::hello(&link).map(|h| h.sessions).unwrap_or_default();
            for s in theirs.into_iter().filter(|s| s.running && !ours.contains(&s.session)) {
                eprintln!("cue: closing {} on an older cue-host: no session of Cue's is it", s.session);
                let _ = link.request(json!({ "op": "close", "session": s.session }));
            }
        }
        if drain(&self.old, &self.remotes) > 0 {
            self.watch_old();
        }
    }

    /// Every agent on every host ended (each session shows it exited and comes back paused; your next message
    /// starts it on a fresh host), then the hosts themselves, by signal. For a host that misbehaves.
    fn restart_host(&self) -> Result<String, String> {
        let links = self.links();
        let mut ended = 0;
        for link in &links {
            let hello = old_hosts::hello(link).ok();
            let running: Vec<String> = hello.iter().flat_map(|h| h.sessions.iter()).filter(|s| s.running).map(|s| s.session.clone()).collect();
            ended += running.len();
            // All at once: each takes a few seconds to end. Their exits are reported (not closed on purpose).
            let closing: Vec<_> = running.into_iter().map(|id| { let l = link.clone(); std::thread::spawn(move || l.request(json!({ "op": "close", "session": id }))) }).collect();
            for c in closing {
                let _ = c.join();
            }
            // Their exits handled before the host goes.
            let until = Instant::now() + Duration::from_secs(3);
            while on_link(&self.remotes, link).iter().any(|r| r.running.load(Ordering::SeqCst)) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(50));
            }
            let_go(&self.remotes, link, true);
            link.shut();
            if let Some(h) = hello {
                old_hosts::end_process(h.pid);
            }
        }
        crate::locking::lock(&self.old, "hosts: older hosts").retain(|o| o.machine.is_some());
        *crate::locking::lock(&self.link, "hosts: link") = None;
        *crate::locking::lock(&self.failed_at, "hosts: link tried") = None;
        Ok(match (links.len(), ended) {
            (0, _) => "cue-host wasn't running".into(),
            (_, 0) => "Restarted cue-host (nothing was running)".into(),
            (_, 1) => "Restarted cue-host: 1 session ended, and resumes with your next message".into(),
            (_, n) => format!("Restarted cue-host: {n} sessions ended, and each resumes with your next message"),
        })
    }

    /// The hosts, for Settings: reachable, how many sessions each runs, and how many run inside Cue.
    fn host_status(&self) -> Value {
        let count = |l: &HostLink| old_hosts::hello(l).map(|h| h.sessions.iter().filter(|s| s.running).count()).unwrap_or(0);
        let current = crate::locking::lock(&self.link, "hosts: link").clone();
        let olds: Vec<(i32, Arc<HostLink>)> = crate::locking::lock(&self.old, "hosts: older hosts").iter().map(|o| (o.pid, o.link.clone())).collect();
        let older: Vec<Value> = olds.iter().map(|(pid, l)| json!({ "pid": pid, "running": count(l) })).collect();
        json!({
            "reachable": current.as_ref().is_some_and(|l| l.connected()),
            "build": cue_host::proto::BUILD,
            "running": current.as_deref().map_or(0, count),
            "older": older,
            "inside_cue": self.direct.in_cue().len(),
            "machines": self.machines.status(),
        })
    }
}
