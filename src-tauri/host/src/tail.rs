//! A file copied to a client as it's written (`Request::Tail`): a session's transcript, read by Cue on another
//! machine. Looked at a few times a second; whole lines sent (a line half written waits for its end). A file not
//! there yet is waited for, and one that shrinks or is replaced (written over) is sent again from its start.

use crate::proto::{Event, FromHost};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

/// How often a tailed file is looked at.
const EVERY: Duration = Duration::from_millis(200);
/// The most sent in one event (a big file's start comes over in pieces).
const PIECE: usize = 256 * 1024;

/// `path` from byte `from`, as `tail` events to `to`, until `stop` is set or the client is gone. On a thread of its own.
pub fn follow(tail: String, path: String, from: u64, to: Sender<String>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut at = from;
        let mut file: Option<(std::fs::File, u64)> = None;
        let mut partial: Vec<u8> = Vec::new();
        while !stop.load(Ordering::SeqCst) {
            if file.is_none() {
                file = std::fs::File::open(&path).ok().and_then(|f| f.metadata().ok().map(|m| (f, m.ino())));
            }
            if let Some((f, ino)) = file.as_mut() {
                match std::fs::metadata(&path) {
                    // Written over: a new file under the name, or this one shorter than what's been read.
                    Ok(m) if m.ino() != *ino || m.len() < at => {
                        file = None;
                        at = 0;
                        partial.clear();
                        continue;
                    }
                    Ok(_) => {}
                    Err(_) => {
                        file = None;
                        std::thread::sleep(EVERY);
                        continue;
                    }
                }
                match more(f, at, &mut partial) {
                    Ok(Some((start, data))) => {
                        at = start + data.len() as u64;
                        let line = FromHost::Event { session: tail.clone(), event: Event::Tail { at: start, data } }.line();
                        if to.send(line).is_err() {
                            return;
                        }
                        continue;
                    }
                    Ok(None) => {}
                    Err(_) => file = None,
                }
            }
            std::thread::sleep(EVERY);
        }
    });
}

/// What `f` has past `at` (and past what's waiting in `partial`), up to its last whole line: where it starts, and the
/// lines. None when there's no whole line yet.
fn more(f: &mut std::fs::File, at: u64, partial: &mut Vec<u8>) -> std::io::Result<Option<(u64, String)>> {
    let read_from = at + partial.len() as u64;
    f.seek(SeekFrom::Start(read_from))?;
    let mut buf = vec![0u8; PIECE];
    let n = f.read(&mut buf)?;
    partial.extend_from_slice(&buf[..n]);
    let Some(end) = partial.iter().rposition(|b| *b == b'\n') else { return Ok(None) };
    let whole: Vec<u8> = partial.drain(..=end).collect();
    Ok(Some((at, String::from_utf8_lossy(&whole).into_owned())))
}
