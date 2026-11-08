//! Transcript copies over a machine's cue-host connection (host/src/tail.rs), for machines/logs.rs: a machine Cue is
//! connected to (it hosts a session there) copies each log over that one connection instead of a `tail -F` SSH
//! session each. A copy goes on where the local file ends after the connection is made again (`resume`).

use super::host_link::HostLink;
use cue_host::proto::Event;
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

/// The connections to machines' hosts, by machine name (on_machines.rs keeps them; this only looks).
fn links() -> &'static Mutex<HashMap<String, Weak<HostLink>>> {
    static L: OnceLock<Mutex<HashMap<String, Weak<HostLink>>>> = OnceLock::new();
    L.get_or_init(Mutex::default)
}

struct Tail {
    link: Weak<HostLink>,
    remote: String,
    local: PathBuf,
}

/// The copies running over a connection, by Cue's name for each.
fn tails() -> &'static Mutex<HashMap<String, Tail>> {
    static T: OnceLock<Mutex<HashMap<String, Tail>>> = OnceLock::new();
    T.get_or_init(Mutex::default)
}

/// Machine `name`'s host is reached over `link` from now on.
pub(super) fn known(name: &str, link: &Arc<HostLink>) {
    crate::locking::lock(links(), "machine tails: links").insert(name.to_string(), Arc::downgrade(link));
}

/// `remote` on machine `machine` copied into `local` over the machine's cue-host connection, from where `local` ends
/// (`local` emptied first unless `keep`). Cue's name for the copy; None when there's no connection to that machine's
/// host (the caller copies another way). Never waits on the machine: the request goes on a thread of its own (callers
/// hold their own locks), and one the host refuses ends the copy (`tail_running` then says so).
pub fn tail_start(machine: &str, remote: &str, local: &Path, keep: bool) -> Option<String> {
    let link = crate::locking::lock(links(), "machine tails: links").get(machine).and_then(Weak::upgrade).filter(|l| l.connected())?;
    if !keep {
        std::fs::File::create(local).ok()?;
    }
    static N: AtomicU64 = AtomicU64::new(1);
    let id = format!("tail-{}", N.fetch_add(1, Ordering::SeqCst));
    crate::locking::lock(tails(), "machine tails").insert(id.clone(), Tail { link: Arc::downgrade(&link), remote: remote.to_string(), local: local.to_path_buf() });
    let (tail, remote, to) = (id.clone(), remote.to_string(), local.to_path_buf());
    std::thread::spawn(move || {
        let into = to.clone();
        link.route(&tail, Arc::new(move |e| {
            if let Event::Tail { at, data } = e {
                write_at(&into, at, &data);
            }
        }));
        if let Err(e) = request(&link, &tail, &remote, &to) {
            eprintln!("cue: couldn't copy {remote} over cue-host: {e}");
            link.unroute(&tail);
            crate::locking::lock(tails(), "machine tails").remove(&tail);
        }
    });
    Some(id)
}

/// Whether copy `id` goes on: its connection is there, or is being made again.
pub fn tail_running(id: &str) -> bool {
    crate::locking::lock(tails(), "machine tails").get(id).and_then(|t| t.link.upgrade()).is_some_and(|l| l.comes_back())
}

/// Copy `id` stopped.
pub fn tail_stop(id: &str) {
    let Some(t) = crate::locking::lock(tails(), "machine tails").remove(id) else { return };
    let Some(l) = t.link.upgrade() else { return };
    let id = id.to_string();
    // On a thread of its own, as a start's request is: whoever stops it may hold locks of its own.
    std::thread::spawn(move || {
        l.unroute(&id);
        let _ = l.request(json!({ "op": "untail", "tail": id }));
    });
}

/// `link`'s connection is back: each copy over it asked for again from where its file ends.
pub(super) fn resume(link: &Arc<HostLink>) {
    let mine: Vec<(String, String, PathBuf)> = crate::locking::lock(tails(), "machine tails").iter().filter(|(_, t)| t.link.upgrade().is_some_and(|l| Arc::ptr_eq(&l, link))).map(|(id, t)| (id.clone(), t.remote.clone(), t.local.clone())).collect();
    for (id, remote, local) in mine {
        if let Err(e) = request(link, &id, &remote, &local) {
            eprintln!("cue: couldn't copy {remote} again: {e}");
        }
    }
}

fn request(link: &HostLink, id: &str, remote: &str, local: &Path) -> Result<(), String> {
    let from = std::fs::metadata(local).map_or(0, |m| m.len());
    link.request(json!({ "op": "tail", "tail": id, "path": remote, "from": from })).map(|_| ())
}

/// `data`, from byte `at` of the machine's file, into `to`. Pieces come in order, and after a reconnect from where `to`
/// ends; one starting before that (the file written over: from 0) cuts what `to` has past it first, so nothing is
/// doubled.
fn write_at(to: &Path, at: u64, data: &str) {
    let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(to) else { return };
    if f.metadata().is_ok_and(|m| at < m.len()) {
        let _ = f.set_len(at);
    }
    let _ = f.write_all(data.as_bytes());
}
