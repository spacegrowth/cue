//! Watching: one SSH stream per machine, read a line at a time, and what runs when a machine is reached
//! or a new session shows up on it.

use super::events::{handle, log_raw};
use super::scripts::watch_script;
use super::setup::{refresh_hook, status_on_to};
use crate::machines::{self, Machine};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

fn watchers() -> &'static Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>> {
    static W: OnceLock<Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Watch every machine (at launch, and when one's added). One already watched is left alone.
pub fn watch_all() {
    // A watcher an earlier Cue left behind would take events away from this one: end them first.
    // Only ssh processes whose command carries the watcher's mark, nothing else that mentions it.
    machines::end_streams("# cue-watch");
    crate::machines::logs::stop_strays();
    for m in machines::list() {
        watch(&m);
    }
}

pub fn watch(m: &Machine) {
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut w = crate::locking::lock(watchers(), "watchers");
        if w.contains_key(&m.name) {
            return;
        }
        w.insert(m.name.clone(), stop.clone());
    }
    let m = m.clone();
    std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = stream(&m, &stop);
            // Down (asleep, offline, needs a login): try again in a while, quietly.
            for _ in 0..15 {
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

/// Stop watching a machine (it was removed).
pub fn unwatch(name: &str) {
    if let Some(stop) = crate::locking::lock(watchers(), "watchers").remove(name) {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// What runs each time a machine is reached again (Cue started, or it came back from a reboot or a
/// dropped line): given the machine's name, on the watcher's own thread.
static ON_REACHED: OnceLock<Box<dyn Fn(&str) + Send + Sync>> = OnceLock::new();

pub fn on_reached(f: impl Fn(&str) + Send + Sync + 'static) {
    let _ = ON_REACHED.set(Box::new(f));
}

/// What runs when a session Cue hasn't heard from in this run sends its first event from a machine (one
/// just started there, say): given the machine's name, on a thread of its own, now and once
/// more a few seconds later (its pane may not be on its first screen yet). At most once per machine every
/// few seconds, so a machine reached with many sessions asks once.
static ON_NEW_SESSION: OnceLock<Box<dyn Fn(&str) + Send + Sync>> = OnceLock::new();

pub fn on_new_session(f: impl Fn(&str) + Send + Sync + 'static) {
    let _ = ON_NEW_SESSION.set(Box::new(f));
}

pub(super) fn new_session_seen(machine: &str, sid: &str) {
    static SEEN: Mutex<Option<(HashSet<String>, HashMap<String, Instant>)>> = Mutex::new(None);
    let due = {
        let mut g = crate::locking::lock(&SEEN, "seen sessions");
        let (seen, last) = g.get_or_insert_with(Default::default);
        if !seen.insert(format!("{machine}|{sid}")) || last.get(machine).is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
            false
        } else {
            last.insert(machine.to_string(), Instant::now());
            true
        }
    };
    if due && ON_NEW_SESSION.get().is_some() {
        let name = machine.to_string();
        std::thread::spawn(move || {
            for wait in [0, 4] {
                std::thread::sleep(Duration::from_secs(wait));
                if let Some(f) = ON_NEW_SESSION.get() {
                    f(&name);
                }
            }
        });
    }
}

/// One SSH stream: runs until the connection drops or the machine is removed.
pub(crate) fn stream(m: &Machine, stop: &std::sync::atomic::AtomicBool) -> Result<(), String> {
    refresh_hook(m);
    status_on_to(m);
    let mut child = machines::stream_command(&m.host, &watch_script()).stdout(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let out = child.stdout.take().ok_or("no output")?;
    let mut reached = false;
    let mut reader = BufReader::new(out);
    loop {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let line = match capped_line(&mut reader, MAX_LINE) {
            Ok(Some(Some(l))) => l,
            // Too long to be an event: dropped, and said so in the log.
            Ok(Some(None)) => {
                log_raw(&m.name, "(a line over the size limit)");
                continue;
            }
            // The stream ended, or the line dropped.
            Ok(None) | Err(_) => break,
        };
        if line == "@@hb" {
            // It's alive. The first beat of a stream: the machine is reached (again), and the events
            // queued there while Cue wasn't watching (an agent that ended meanwhile) are handled already:
            // what's rebuilt on reaching it goes by what they left.
            if !reached {
                reached = true;
                machines::forget(&m.host);
                if let Some(f) = ON_REACHED.get() {
                    let name = m.name.clone();
                    std::thread::spawn(move || f(&name));
                }
            }
            continue;
        }
        match parse(&line) {
            Some((id, ev)) => handle(m, &id, ev),
            // An event Cue couldn't read: in the log, so it isn't lost without a trace.
            None if line.starts_with("@@") => log_raw(&m.name, &line),
            None => {}
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

/// The most a line from a machine may hold (an event is a few KB; a transcript's largest tool result is
/// well under this). Past it, the rest of the line is read and thrown away as it comes, so a machine
/// that never sends a newline can't make Cue keep growing.
const MAX_LINE: usize = 1024 * 1024;

/// The next line from `r`, without its newline: Ok(Some(Some(line))), or Ok(Some(None)) for a line over
/// `max` bytes (skipped whole), or Ok(None) at the end of the stream. Memory never goes past `max`.
pub(crate) fn capped_line(r: &mut impl BufRead, max: usize) -> std::io::Result<Option<Option<String>>> {
    let mut buf = Vec::new();
    let mut over = false;
    loop {
        let chunk = r.fill_buf()?;
        if chunk.is_empty() {
            // The end: a last line without its newline still counts, nothing at all doesn't.
            return Ok(if buf.is_empty() && !over { None } else if over { Some(None) } else { Some(Some(String::from_utf8_lossy(&buf).into_owned())) });
        }
        let (take, done) = match chunk.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (chunk.len(), false),
        };
        if !over {
            let keep = take - usize::from(done);
            if buf.len() + keep > max {
                over = true;
                buf = Vec::new();
            } else {
                buf.extend_from_slice(&chunk[..keep]);
            }
        }
        r.consume(take);
        if done {
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            return Ok(Some(if over { None } else { Some(String::from_utf8_lossy(&buf).into_owned()) }));
        }
    }
}

/// "@@<id> <json>" -> (id, event).
pub(super) fn parse(line: &str) -> Option<(String, Value)> {
    let (id, json) = line.strip_prefix("@@")?.split_once(' ')?;
    if !id.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return None;
    }
    Some((id.to_string(), serde_json::from_str(json).ok()?))
}
