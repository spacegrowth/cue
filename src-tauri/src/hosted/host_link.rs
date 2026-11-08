//! Cue's connection to cue-host (crates host/: the program that keeps hosted agents running across Cue restarts):
//! requests answered in order of their ids, and each attached session's records handed to its route as they come.
//! A connection that drops is made again (the host started again if it's gone), with a pause that grows from 1 s
//! to 30 s; `on_back` is told, so the sessions attach again after what they've read. A link to an older host
//! (`existing`: one Cue lets finish what it runs, never starts) isn't made again.

use cue_host::proto::{Event, FromHost};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

/// How long a request waits for its answer (a close waits for its child: a few seconds at most).
const ANSWER_WITHIN: Duration = Duration::from_secs(20);
/// Pauses between tries to reach a host that's gone.
const RETRY_FIRST: Duration = Duration::from_secs(1);
const RETRY_MOST: Duration = Duration::from_secs(30);

/// Where a session's records go: called on the connection's own thread, in order.
pub type Route = Arc<dyn Fn(Event) + Send + Sync>;

pub struct HostLink {
    home: PathBuf,
    /// The program and arguments that run `serve` for `home`, to start a host when none runs.
    serve: Vec<String>,
    /// The connection's writing half, while there is one.
    out: Mutex<Option<UnixStream>>,
    waiting: Mutex<HashMap<u64, Sender<Result<Value, String>>>>,
    routes: Mutex<HashMap<String, Route>>,
    next: AtomicU64,
    connected: AtomicBool,
    /// Making the connection again (one at a time).
    retrying: AtomicBool,
    /// Done with: a connection that drops isn't made again (Restart cue-host, or a host being let go).
    shut: AtomicBool,
    /// Told once a dropped connection is made again.
    on_back: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl HostLink {
    /// A link to the host in `home`, connected now (starting the host if it isn't running).
    pub fn connect(home: PathBuf, serve: Vec<String>) -> Result<Arc<HostLink>, String> {
        let link = Arc::new(HostLink { home, serve, out: Mutex::new(None), waiting: Mutex::default(), routes: Mutex::default(), next: AtomicU64::new(1), connected: AtomicBool::new(false), retrying: AtomicBool::new(false), shut: AtomicBool::new(false), on_back: Mutex::new(None) });
        link.open().map_err(|e| format!("couldn't reach cue-host: {e}"))?;
        Ok(link)
    }

    /// A link to a host already running in `home`, never starting one, and not made again once it drops.
    pub fn existing(home: PathBuf) -> Result<Arc<HostLink>, String> {
        HostLink::connect(home, vec![])
    }

    /// Where the host it reaches lives.
    pub fn home(&self) -> &std::path::Path {
        &self.home
    }

    /// Done with it: the connection closed, and not made again.
    pub fn shut(&self) {
        self.shut.store(true, Ordering::SeqCst);
        if let Some(s) = crate::locking::lock(&self.out, "cue-host link: out").as_ref() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }

    /// Who to tell when a dropped connection is back.
    pub fn on_back(&self, f: impl Fn() + Send + Sync + 'static) {
        *crate::locking::lock(&self.on_back, "cue-host link: on back") = Some(Box::new(f));
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn open(self: &Arc<Self>) -> std::io::Result<()> {
        // A host Cue only lets finish (`existing`) is reached where it is, never started.
        let stream = if self.serve.is_empty() { UnixStream::connect(cue_host::daemon::socket(&self.home))? } else { cue_host::daemon::connect(&self.home, &self.serve)? };
        let input = stream.try_clone()?;
        *crate::locking::lock(&self.out, "cue-host link: out") = Some(stream);
        self.connected.store(true, Ordering::SeqCst);
        let me = Arc::downgrade(self);
        std::thread::spawn(move || read(me, input));
        Ok(())
    }

    /// `op`, answered: its result, or why not (the host's refusal, or no connection).
    pub fn request(&self, op: Value) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = channel();
        crate::locking::lock(&self.waiting, "cue-host link: waiting").insert(id, tx);
        let mut req = op;
        req["id"] = json!(id);
        let sent = {
            let mut out = crate::locking::lock(&self.out, "cue-host link: out");
            match out.as_mut() {
                Some(s) => writeln!(s, "{req}").map_err(|e| e.to_string()),
                None => Err("Cue is reconnecting to cue-host".to_string()),
            }
        };
        if let Err(e) = sent {
            crate::locking::lock(&self.waiting, "cue-host link: waiting").remove(&id);
            return Err(e);
        }
        rx.recv_timeout(ANSWER_WITHIN).unwrap_or_else(|_| {
            crate::locking::lock(&self.waiting, "cue-host link: waiting").remove(&id);
            Err("cue-host didn't answer".to_string())
        })
    }

    /// Records for host session `session` go to `route` from now on.
    pub fn route(&self, session: &str, route: Route) {
        crate::locking::lock(&self.routes, "cue-host link: routes").insert(session.to_string(), route);
    }

    pub fn unroute(&self, session: &str) {
        crate::locking::lock(&self.routes, "cue-host link: routes").remove(session);
    }

    /// The connection dropped: every request waiting is told, and it's made again until it's back.
    fn lost(self: &Arc<Self>) {
        self.connected.store(false, Ordering::SeqCst);
        crate::locking::lock(&self.out, "cue-host link: out").take();
        let waiting: Vec<_> = crate::locking::lock(&self.waiting, "cue-host link: waiting").drain().map(|(_, tx)| tx).collect();
        for tx in waiting {
            let _ = tx.send(Err("Cue lost its connection to cue-host".into()));
        }
        // Done with, or an older host Cue only lets finish: not made again (its sessions are let go: hosts.rs drain).
        if self.shut.load(Ordering::SeqCst) || self.serve.is_empty() {
            return;
        }
        if self.retrying.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = Arc::downgrade(self);
        std::thread::spawn(move || {
            let mut pause = RETRY_FIRST;
            loop {
                std::thread::sleep(pause);
                let Some(link) = me.upgrade() else { return };
                match link.open() {
                    Ok(()) => {
                        link.retrying.store(false, Ordering::SeqCst);
                        eprintln!("cue: reconnected to cue-host");
                        if let Some(f) = crate::locking::lock(&link.on_back, "cue-host link: on back").as_ref() {
                            f();
                        }
                        return;
                    }
                    Err(e) => eprintln!("cue: cue-host still unreachable ({e}); trying again in {}s", pause.as_secs()),
                }
                pause = (pause * 2).min(RETRY_MOST);
            }
        });
    }
}

/// The connection's lines until it ends: answers to their requests, records to their sessions' routes.
fn read(link: Weak<HostLink>, input: UnixStream) {
    let mut reader = BufReader::new(input);
    let mut buf = Vec::new();
    while reader.read_until(b'\n', &mut buf).is_ok_and(|n| n > 0) {
        let Some(l) = link.upgrade() else { return };
        match serde_json::from_slice::<FromHost>(&buf) {
            Ok(FromHost::Response { id, ok, result, error }) => {
                if let Some(tx) = crate::locking::lock(&l.waiting, "cue-host link: waiting").remove(&id) {
                    let _ = tx.send(if ok { Ok(result) } else { Err(error.unwrap_or_else(|| "cue-host refused".into())) });
                }
            }
            Ok(FromHost::Event { session, event }) => {
                let route = crate::locking::lock(&l.routes, "cue-host link: routes").get(&session).cloned();
                if let Some(r) = route {
                    r(event);
                }
            }
            Err(e) => eprintln!("cue: a line from cue-host Cue can't read ({e})"),
        }
        buf.clear();
    }
    if let Some(l) = link.upgrade() {
        eprintln!("cue: lost cue-host");
        l.lost();
    }
}
