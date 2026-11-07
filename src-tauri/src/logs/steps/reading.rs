//! A feed's catch-up in three steps, so no lock is held while a file is read: a slow disk then holds up
//! only the reader, never everyone else asking about the same session. What to read is taken under the
//! feed's lock (`Feed::plan`), the files are read with no lock held (`Plan::fetch`), and what came back is
//! put in under the lock again (`Feed::apply`).
//!
//! Each put-in counts the feed's generation, and a read carries the generation it was planned at: one
//! planned before another was put in is dropped whole (that other one already took those lines, and the
//! next catch-up reads on from where it left off), so two catch-ups at once never take a line twice or
//! skip one.

use super::feed::{Feed, Head};
use super::helper::{helper_meta, mtime_ms, Helper};
use super::kind::Kind;
use super::windows::auto_compact_at;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Rounds of a catch-up: a background task started in the lines just read has its own file read in the
/// next round (it wasn't known when the round was planned).
pub(super) const ROUNDS: usize = 3;

/// A background script's output shown is read from this far before its end.
pub(super) const SCRIPT_TAIL_BYTES: u64 = 256 * 1024;

/// What one feed's catch-up reads: taken under its lock, with no file touched.
pub(super) struct Plan {
    generation: u64,
    path: String,
    offset: u64,
    /// The model whose auto-compaction point is asked (Claude Code only; the db is asked with no lock held).
    model: Option<String>,
    helpers: Vec<(String, HelperPlan)>,
}

/// What one background task's catch-up reads.
enum HelperPlan {
    /// An agent: its own log (none when its path couldn't be made), and its meta file when it's wanted.
    Agent { log: Option<Plan>, meta: MetaWanted },
    /// A shell command: its output file, and how long it was when last read.
    Script { out: Option<PathBuf>, len: u64 },
}

/// When an agent's meta file is read: always on its first read (what its launch didn't say), and later
/// only when its log grew while something is still missing.
#[derive(Clone, Copy, PartialEq)]
enum MetaWanted {
    Always,
    IfGrown,
    Never,
}

/// What a plan's reading found, to put in under the lock.
pub(super) struct Reads {
    generation: u64,
    offset: u64,
    pub(super) log: LogRead,
    /// Claude Code: the model the plan saw, and where sessions on it lately compacted on their own.
    auto_at: Option<(String, u64)>,
    helpers: Vec<(String, HelperRead)>,
}

/// What a log had from the offset the read was planned at.
pub(super) struct LogRead {
    /// The file could be opened.
    pub(super) found: bool,
    /// Where the bytes start.
    pub(super) at: u64,
    /// The file was replaced (it's shorter than the offset): its feed starts over from this head.
    pub(super) reopened: Option<Head>,
    pub(super) bytes: Vec<u8>,
}

pub(super) enum HelperRead {
    Agent { log: Option<Reads>, meta: Option<Value> },
    Script(ScriptRead),
}

/// A script's output file: its length and tail when it changed length since last read, and when it was
/// last written.
pub(super) struct ScriptRead {
    pub(super) planned_len: u64,
    pub(super) tail: Option<Tail>,
    pub(super) mtime: Option<u64>,
}

pub(super) struct Tail {
    pub(super) len: u64,
    pub(super) from: u64,
    pub(super) bytes: Vec<u8>,
}

/// How a read went in.
#[derive(Debug, PartialEq)]
pub(super) enum Applied {
    Done,
    /// Put in, and a background task it started still has its own file to read.
    Again,
    /// Another catch-up was put in since this one was planned: dropped.
    Stale,
}

impl Feed {
    /// What a catch-up reads, from where the feed is now. No file is touched.
    pub(super) fn plan(&self) -> Plan {
        let helpers = self.helpers.iter().map(|h| (h.id.clone(), h.plan())).collect();
        let model = (self.kind == Kind::Claude).then(|| self.meta.model.clone());
        Plan { generation: self.generation, path: self.path.clone(), offset: self.offset, model, helpers }
    }

    /// Put in what `reads` found, unless another catch-up was put in since it was planned.
    pub(super) fn apply(&mut self, reads: Reads) -> Applied {
        if reads.generation != self.generation || reads.offset != self.offset {
            return Applied::Stale;
        }
        let model_moved = self.put(reads);
        if model_moved || self.unread() { Applied::Again } else { Applied::Done }
    }

    /// Read what was added since last time, for a feed no one else reads (no lock to let go of).
    pub(super) fn catch_up(&mut self) {
        for _ in 0..ROUNDS {
            let reads = self.plan().fetch();
            if self.apply(reads) != Applied::Again {
                return;
            }
        }
    }

    /// A background task (or one of an agent's own) whose file hasn't been read yet.
    fn unread(&self) -> bool {
        self.helpers.iter().any(|h| !h.seen || h.feed.as_ref().is_some_and(|f| f.unread()))
    }

    /// Put a read in (`apply` checked it's current): the log's new lines, then its background tasks'
    /// files, then what follows from both. True when the lines put in name another model than the plan saw:
    /// its auto-compaction point is asked for in another round.
    fn put(&mut self, reads: Reads) -> bool {
        let generation = self.generation;
        let mut changed = self.put_log(reads.log);
        for (id, read) in reads.helpers {
            if let Some(h) = self.helpers.iter_mut().find(|h| h.id == id) {
                changed |= h.put(read);
            }
        }
        // Taken up on a message, and its log isn't there: not one of its agents after all.
        self.helpers.retain(|h| !(h.unconfirmed && h.seen));
        self.close_scripts();
        let (auto_moved, model_moved) = self.take_auto_at(reads.auto_at);
        if changed || auto_moved {
            self.changed();
        }
        self.generation = generation + 1;
        model_moved
    }

    /// Where sessions on its model lately compacted on their own, when the read asked for the model it's on now
    /// (else the model just changed: false, true, and the next round asks for the new one). Whether the point
    /// moved, and whether the model did.
    fn take_auto_at(&mut self, auto_at: Option<(String, u64)>) -> (bool, bool) {
        let Some((model, at)) = auto_at.filter(|_| self.kind == Kind::Claude) else { return (false, false) };
        if model != self.meta.model {
            return (false, true);
        }
        let moved = self.meta.auto_at != at;
        self.meta.auto_at = at;
        (moved, false)
    }
}

impl Helper {
    fn plan(&self) -> HelperPlan {
        if self.script {
            return HelperPlan::Script { out: self.out_path.clone(), len: self.out_len };
        }
        let missing = self.kind.is_empty() || self.desc.is_empty() || !self.meta_read;
        let meta = if !self.seen { MetaWanted::Always } else if missing { MetaWanted::IfGrown } else { MetaWanted::Never };
        HelperPlan::Agent { log: self.feed.as_ref().map(|f| f.plan()), meta }
    }
}

impl Plan {
    /// Read what the plan says, with no lock held: the log first, then its background tasks' files (a
    /// script's output read after the log that says it ended has its last lines).
    pub(super) fn fetch(self) -> Reads {
        crate::chaos::pause(crate::chaos::Knob::FsDelayMs);
        #[cfg(test)]
        slow_disk::wait(&self.path);
        let log = read_log(&self.path, self.offset);
        let auto_at = self.model.map(|m| { let at = auto_compact_at(&m); (m, at) });
        let helpers = self.helpers.into_iter().map(|(id, h)| (id, h.fetch())).collect();
        Reads { generation: self.generation, offset: self.offset, log, auto_at, helpers }
    }
}

impl HelperPlan {
    fn fetch(self) -> HelperRead {
        match self {
            HelperPlan::Script { out, len } => HelperRead::Script(read_script(out.as_deref(), len)),
            HelperPlan::Agent { log, meta } => {
                let path = log.as_ref().map(|p| PathBuf::from(&p.path));
                let log = log.map(Plan::fetch);
                let grown = log.as_ref().is_some_and(|r| !r.log.bytes.is_empty() || r.log.reopened.is_some());
                let wanted = meta == MetaWanted::Always || (meta == MetaWanted::IfGrown && grown);
                HelperRead::Agent { log, meta: path.filter(|_| wanted).map(|p| helper_meta(&p)) }
            }
        }
    }
}

/// A log's bytes from `offset` to its end (from a new head when it was replaced).
fn read_log(path: &str, offset: u64) -> LogRead {
    let mut read = LogRead { found: false, at: offset, reopened: None, bytes: Vec::new() };
    let Ok(mut file) = std::fs::File::open(path) else { return read };
    read.found = true;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len < offset {
        let head = Head::read(path);
        read.at = head.offset;
        read.reopened = Some(head);
    }
    if len > read.at && file.seek(SeekFrom::Start(read.at)).is_ok() {
        let _ = file.take(len - read.at).read_to_end(&mut read.bytes);
    }
    read
}

/// A script's output file: its tail when its length isn't `len` any more, and when it was last written.
fn read_script(out: Option<&std::path::Path>, len: u64) -> ScriptRead {
    let mut read = ScriptRead { planned_len: len, tail: None, mtime: None };
    let Some(path) = out else { return read };
    read.mtime = mtime_ms(path);
    let Ok(mut file) = std::fs::File::open(path) else { return read };
    let now = file.metadata().map(|m| m.len()).unwrap_or(0);
    if now == len {
        return read;
    }
    let from = now.saturating_sub(SCRIPT_TAIL_BYTES);
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(from)).is_ok() && file.read_to_end(&mut bytes).is_ok() {
        read.tail = Some(Tail { len: now, from, bytes });
    }
    read
}

/// Tests: a disk that holds a log's next read until the test lets it go.
#[cfg(test)]
pub(crate) mod slow_disk {
    use std::collections::HashMap;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::Mutex;
    
    /// Per log path: the read that's held tells `started`, then waits on `release`.
    static HELD: Mutex<Option<HashMap<String, (Sender<()>, Receiver<()>)>>> = Mutex::new(None);

    /// The next read of `path` waits until the returned sender sends (or is dropped); the receiver hears
    /// when that read has started.
    pub(crate) fn hold_next(path: &str) -> (Sender<()>, Receiver<()>) {
        let (release_tx, release_rx) = channel();
        let (started_tx, started_rx) = channel();
        HELD.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_string(), (started_tx, release_rx));
        (release_tx, started_rx)
    }

    pub(super) fn wait(path: &str) {
        let held = HELD.lock().unwrap().as_mut().and_then(|h| h.remove(path));
        if let Some((started, release)) = held {
            let _ = started.send(());
            let _ = release.recv();
        }
    }
}
