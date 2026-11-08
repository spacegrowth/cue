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

use super::driver::{Driver, Logged, Sink};
use super::host_link::HostLink;
use super::running::{Hosted, Reattached, Report, Running, Started};
use super::session::wait_open;
use super::{launch, Answer, Spec};
use cue_host::proto::{Event, Hello, Record, What};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

/// How long a session's place is kept with nothing read: as long as the host keeps its log.
const FORGOTTEN: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);

/// How often a host that couldn't be reached is tried again (before a session starts).
const RETRY_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

pub struct Hosts {
    home: PathBuf,
    serve: Vec<String>,
    /// None while the host can't be reached: sessions started meanwhile run as Cue's own children (`in_cue`).
    link: Mutex<Option<Arc<HostLink>>>,
    /// When reaching it was last tried and failed.
    failed_at: Mutex<Option<std::time::Instant>>,
    direct: Running,
    /// The sessions run by the host, by the conversation's id.
    remotes: Arc<Mutex<HashMap<String, Arc<Remote>>>>,
    /// Where each session's last handled record is kept.
    seen: PathBuf,
}

impl Hosts {
    /// Through the host in `home` (started with `serve` if it isn't running), keeping what's been read in `seen`.
    /// While it can't be reached, sessions run as Cue's own children, as before the host, and it's tried again.
    pub fn new(home: PathBuf, serve: Vec<String>, seen: PathBuf) -> Hosts {
        Cursor::prune(&seen);
        let hosts = Hosts { home, serve, link: Mutex::new(None), failed_at: Mutex::new(None), direct: Running::default(), remotes: Arc::default(), seen };
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
                // Back after a dropped connection: each session attaches again after what it has read.
                let all = Arc::downgrade(&self.remotes);
                l.on_back(move || {
                    let Some(all) = all.upgrade() else { return };
                    let list: Vec<Arc<Remote>> = crate::locking::lock(&all, "hosts: sessions").values().cloned().collect();
                    for r in list {
                        if let Err(e) = r.attach() {
                            eprintln!("cue: couldn't attach {} again: {e}", r.host_id);
                        }
                    }
                });
                *crate::locking::lock(&self.link, "hosts: link") = Some(l.clone());
                Some(l)
            }
            Err(e) => {
                eprintln!("cue: {e}; hosted sessions run inside Cue until it can be reached");
                *crate::locking::lock(&self.failed_at, "hosts: link tried") = Some(std::time::Instant::now());
                None
            }
        }
    }

    fn remote(&self, session_id: &str) -> Option<Arc<Remote>> {
        crate::locking::lock(&self.remotes, "hosts: sessions").get(session_id).cloned()
    }

    fn hello(link: &HostLink) -> Result<Hello, String> {
        serde_json::from_value(link.request(json!({ "op": "hello", "version": cue_host::proto::VERSION }))?).map_err(|e| e.to_string())
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
        let r = crate::locking::lock(&self.remotes, "hosts: sessions").remove(session_id)?;
        r.link.unroute(&r.host_id);
        crate::locking::lock(&r.queue, "hosts: a session's queue").take();
        Some(r)
    }

    fn start_hosted(&self, link: &Arc<HostLink>, spec: &Spec, report: Report) -> Result<Started, String> {
        let host_id = spec.start.session_id().to_string();
        if let Some(old) = self.drop_remote(&host_id) {
            old.close();
        }
        let launch::HostCommand { argv, env, cwd } = launch::for_host(spec)?;
        let meta = json!({ "agent": spec.agent.program(), "cwd": cwd, "session": host_id });
        let r = Arc::new(Remote::new(&host_id, Driver::new(spec, sink(report)), link.clone(), &self.seen));
        let started = link.request(json!({ "op": "start", "session": host_id, "argv": argv, "env": env, "cwd": cwd, "meta": meta }))?;
        let pid = started["pid"].as_i64().unwrap_or(0) as i32;
        r.pid.store(pid, Ordering::SeqCst);
        r.running.store(true, Ordering::SeqCst);
        r.set_cursor(started["after"].as_u64().unwrap_or(0));
        self.listen(&r, link);
        let opened = r.attach().and_then(|()| r.write_lines(&r.driver.opening())).and_then(|()| wait_open(&r.driver, || !r.running.load(Ordering::SeqCst)));
        if let Err(e) = opened {
            r.close();
            r.link.unroute(&host_id);
            return Err(e);
        }
        let session_id = r.driver.session_id();
        r.named(&session_id);
        crate::locking::lock(&self.remotes, "hosts: sessions").insert(session_id.clone(), r.clone());
        Ok(Started { pid, session_id, transcript: r.driver.transcript(), in_turn: false })
    }
}

/// `report` as a driver's sink.
fn sink(report: Report) -> Sink {
    let report: Arc<Report> = Arc::new(report);
    Arc::new(move |id, e| report(id, e))
}

impl Hosted for Hosts {
    fn start(&self, spec: &Spec, report: Report) -> Result<Started, String> {
        match self.link() {
            Some(link) => self.start_hosted(&link, spec, report),
            None => self.direct.start(spec, report),
        }
    }

    fn send(&self, session_id: &str, text: &str) -> Result<(), String> {
        match self.remote(session_id) {
            Some(r) => r.write_lines(&r.driver.message(text)),
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

    /// Cue quitting: the agents the host runs go on (that's what it's for); only those run as Cue's own end.
    fn close_all(&self) {
        let all: Vec<String> = crate::locking::lock(&self.remotes, "hosts: sessions").keys().cloned().collect();
        for id in all {
            self.drop_remote(&id);
        }
        self.direct.close_all();
    }

    /// Only those run as Cue's own children: the host's go on without it.
    fn in_cue(&self) -> Vec<String> {
        self.direct.in_cue()
    }

    /// Those processes, of a Cue that's gone, still running on their own: not the host's (they're meant to run on).
    fn end_left(&self, pids: &[i32]) {
        let hosts: Vec<i32> = self.link().as_deref().and_then(|l| Hosts::hello(l).ok()).map(|h| h.sessions.iter().filter_map(|s| s.pid).collect()).unwrap_or_default();
        let left: Vec<i32> = pids.iter().copied().filter(|p| !hosts.contains(p)).collect();
        self.direct.end_left(&left);
    }

    /// A session the host ran for an earlier Cue, picked up: its current run's records up to where that Cue had
    /// read, read again with nothing reported. Then (`go`) each ask it still waits on reported again, and what came
    /// after, as it comes. None when the host has nothing for it (never run there, or it ended and every record was
    /// read).
    fn reattach(&self, spec: &Spec, report: Report) -> Option<Reattached> {
        let link = self.link()?;
        let sid = spec.start.session_id().to_string();
        let (host_id, cursor) = Cursor::find(&self.seen, &sid)?;
        let hello = Hosts::hello(&link).inspect_err(|e| eprintln!("cue: cue-host didn't say what it runs: {e}")).ok()?;
        let info = hello.sessions.into_iter().find(|s| s.session == host_id)?;
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
        self.listen(&r, &link);
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
}

/// A kept record as the driver reads it back; an exit isn't one (a run that ended is never caught up on).
fn logged(w: What) -> Option<Logged> {
    match w {
        What::Out(l) => Some(Logged::Out(l)),
        What::Err(l) => Some(Logged::Err(l)),
        What::In(l) => Some(Logged::In(l)),
        What::Exited(_) => None,
    }
}

/// One session the host runs.
struct Remote {
    /// What the host knows it by: the id it was started under.
    host_id: String,
    driver: Driver,
    link: Arc<HostLink>,
    cursor: Mutex<Cursor>,
    running: AtomicBool,
    pid: AtomicI32,
    /// Closed on purpose: its exit isn't reported (whoever closed it knows).
    closing: AtomicBool,
    /// Its records, to its own thread.
    queue: Mutex<Option<Sender<Event>>>,
}

impl Remote {
    fn new(host_id: &str, driver: Driver, link: Arc<HostLink>, seen: &Path) -> Remote {
        let cursor = Cursor { path: seen.join(format!("{host_id}.json")), session: host_id.to_string(), seq: 0 };
        Remote { host_id: host_id.to_string(), driver, link, cursor: Mutex::new(cursor), running: AtomicBool::new(false), pid: AtomicI32::new(0), closing: AtomicBool::new(false), queue: Mutex::new(None) }
    }

    fn seq(&self) -> u64 {
        crate::locking::lock(&self.cursor, "hosts: a session's cursor").seq
    }

    fn set_cursor(&self, seq: u64) {
        crate::locking::lock(&self.cursor, "hosts: a session's cursor").set(seq);
    }

    /// Known by the conversation's id now (Codex picks its own once it's open).
    fn named(&self, session_id: &str) {
        let mut c = crate::locking::lock(&self.cursor, "hosts: a session's cursor");
        c.session = session_id.to_string();
        c.save();
    }

    /// Its records after what's been read, from the host, as they come.
    fn attach(&self) -> Result<(), String> {
        self.link.request(json!({ "op": "attach", "session": self.host_id, "after": self.seq() })).map(|_| ())
    }

    fn write_lines(&self, lines: &[String]) -> Result<(), String> {
        for line in lines {
            self.link.request(json!({ "op": "write", "session": self.host_id, "line": line }))?;
        }
        Ok(())
    }

    fn close(&self) {
        self.closing.store(true, Ordering::SeqCst);
        if let Err(e) = self.link.request(json!({ "op": "close", "session": self.host_id })) {
            eprintln!("cue: couldn't close {} in cue-host: {e}", self.host_id);
        }
    }

    /// One record, once: its line made sense of (and any reply written), Cue's own line read back, its exit said.
    fn handle(&self, e: Event) {
        let record = match e {
            Event::Gap { from } => {
                if self.seq() + 1 < from {
                    eprintln!("cue: cue-host no longer has {}'s records before {from}", self.host_id);
                    self.set_cursor(from - 1);
                }
                return;
            }
            Event::Record { record } => record,
        };
        if record.seq <= self.seq() {
            return;
        }
        match record.what {
            What::Out(line) => {
                let replies = self.driver.read(&line);
                if let Err(e) = self.write_lines(&replies) {
                    eprintln!("cue: couldn't reply to {}: {e}", self.host_id);
                }
            }
            What::Err(line) => self.driver.error_line(line),
            What::In(line) => self.driver.speak(|p| p.wrote(&line)),
            What::Exited(code) => {
                self.running.store(false, Ordering::SeqCst);
                if !self.closing.load(Ordering::SeqCst) {
                    self.driver.exited(code);
                }
            }
        }
        self.set_cursor(record.seq);
    }
}

/// The last record Cue handled of a session the host runs, kept on disk with the conversation's id, so a Cue that
/// starts again knows where it was. Written as each record is handled (a small file, rewritten whole).
struct Cursor {
    path: PathBuf,
    /// The conversation's id (what Cue knows it by).
    session: String,
    seq: u64,
}

impl Cursor {
    fn set(&mut self, seq: u64) {
        self.seq = seq;
        self.save();
    }

    fn save(&self) {
        let body = json!({ "session": self.session, "seq": self.seq }).to_string();
        let tmp = self.path.with_extension("tmp");
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&tmp, body).and_then(|()| std::fs::rename(&tmp, &self.path)).is_err() {
            eprintln!("cue: couldn't keep where {} was read to", self.session);
        }
    }

    /// Those in `dir` not written for a month (as the host drops a session's log: host/server.rs FORGOTTEN).
    fn prune(dir: &Path) {
        let Ok(files) = std::fs::read_dir(dir) else { return };
        for f in files.flatten() {
            let old = f.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > FORGOTTEN);
            if old {
                let _ = std::fs::remove_file(f.path());
            }
        }
    }

    /// The host's id for conversation `session` and the last record read, from what's kept in `dir`.
    fn find(dir: &Path, session: &str) -> Option<(String, u64)> {
        std::fs::read_dir(dir).ok()?.flatten().find_map(|f| {
            let v: Value = serde_json::from_slice(&std::fs::read(f.path()).ok()?).ok()?;
            let host_id = f.path().file_stem()?.to_string_lossy().into_owned();
            (v["session"].as_str() == Some(session)).then(|| (host_id, v["seq"].as_u64().unwrap_or(0)))
        })
    }
}
