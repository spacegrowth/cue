//! One session cue-host runs, as Cue sees it (hosts.rs): its driver, which host it's on, the last record Cue handled
//! of it (`Cursor`, on disk), and its records handled in order.

use super::driver::{Driver, Logged, Sink};
use super::host_link::HostLink;
use super::running::Report;
use cue_host::proto::{Event, What};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// How long a session's place is kept with nothing read: as long as the host keeps its log.
const FORGOTTEN: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);

/// `report` as a driver's sink.
pub(super) fn sink(report: Report) -> Sink {
    let report: Arc<Report> = Arc::new(report);
    Arc::new(move |id, e| report(id, e))
}

/// A kept record as the driver reads it back; an exit isn't one (a run that ended is never caught up on).
pub(super) fn logged(w: What) -> Option<Logged> {
    match w {
        What::Out(l) => Some(Logged::Out(l)),
        What::Err(l) => Some(Logged::Err(l)),
        What::In(l) => Some(Logged::In(l)),
        What::Exited(_) => None,
    }
}

/// One session the host runs.
pub(super) struct Remote {
    /// What the host knows it by: the id it was started under.
    pub(super) host_id: String,
    pub(super) driver: Driver,
    pub(super) link: Arc<HostLink>,
    cursor: Mutex<Cursor>,
    pub(super) running: AtomicBool,
    pub(super) pid: AtomicI32,
    /// Closed on purpose: its exit isn't reported (whoever closed it knows).
    pub(super) closing: AtomicBool,
    /// Its records, to its own thread.
    pub(super) queue: Mutex<Option<Sender<Event>>>,
}

impl Remote {
    pub(super) fn new(host_id: &str, driver: Driver, link: Arc<HostLink>, seen: &Path) -> Remote {
        let cursor = Cursor { path: seen.join(format!("{host_id}.json")), session: host_id.to_string(), seq: 0 };
        Remote { host_id: host_id.to_string(), driver, link, cursor: Mutex::new(cursor), running: AtomicBool::new(false), pid: AtomicI32::new(0), closing: AtomicBool::new(false), queue: Mutex::new(None) }
    }

    pub(super) fn seq(&self) -> u64 {
        crate::locking::lock(&self.cursor, "hosts: a session's cursor").seq
    }

    pub(super) fn set_cursor(&self, seq: u64) {
        crate::locking::lock(&self.cursor, "hosts: a session's cursor").set(seq);
    }

    /// Known by the conversation's id now (Codex picks its own once it's open).
    pub(super) fn named(&self, session_id: &str) {
        let mut c = crate::locking::lock(&self.cursor, "hosts: a session's cursor");
        c.session = session_id.to_string();
        c.save();
    }

    /// Its records after what's been read, from the host, as they come.
    pub(super) fn attach(&self) -> Result<(), String> {
        self.link.request(json!({ "op": "attach", "session": self.host_id, "after": self.seq() })).map(|_| ())
    }

    pub(super) fn write_lines(&self, lines: &[String]) -> Result<(), String> {
        for line in lines {
            self.link.request(json!({ "op": "write", "session": self.host_id, "line": line }))?;
        }
        Ok(())
    }

    pub(super) fn close(&self) {
        self.closing.store(true, Ordering::SeqCst);
        if let Err(e) = self.link.request(json!({ "op": "close", "session": self.host_id })) {
            eprintln!("cue: couldn't close {} in cue-host: {e}", self.host_id);
        }
    }

    /// One record, once: its line made sense of (and any reply written), Cue's own line read back, its exit said.
    pub(super) fn handle(&self, e: Event) {
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
pub(super) struct Cursor {
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
    pub(super) fn prune(dir: &Path) {
        let Ok(files) = std::fs::read_dir(dir) else { return };
        for f in files.flatten() {
            let old = f.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > FORGOTTEN);
            if old {
                let _ = std::fs::remove_file(f.path());
            }
        }
    }

    /// The host's id for conversation `session` and the last record read, from what's kept in `dir`.
    pub(super) fn find(dir: &Path, session: &str) -> Option<(String, u64)> {
        std::fs::read_dir(dir).ok()?.flatten().find_map(|f| {
            let v: Value = serde_json::from_slice(&std::fs::read(f.path()).ok()?).ok()?;
            let host_id = f.path().file_stem()?.to_string_lossy().into_owned();
            (v["session"].as_str() == Some(session)).then(|| (host_id, v["seq"].as_u64().unwrap_or(0)))
        })
    }
}
