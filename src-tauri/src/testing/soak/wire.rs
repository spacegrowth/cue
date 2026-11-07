//! The soak's side of Cue's socket, as the agents' hooks and cue-ctl use it: an event told and hung up on,
//! a control message with its one line back, and an ask held open until its decision comes.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// How long a line back may take before the soak counts the socket as not answering.
const ANSWER_CAP: Duration = Duration::from_secs(30);

fn line(msg: &Value) -> String {
    format!("{msg}\n")
}

/// `msg` told, as a hook reports an event: whether it reached Cue.
pub fn tell(sock: &Path, msg: &Value) -> bool {
    UnixStream::connect(sock).and_then(|mut s| s.write_all(line(msg).as_bytes())).is_ok()
}

/// `msg` sent and Cue's one line back, with its size in bytes.
pub fn call(sock: &Path, msg: &Value) -> Option<(Value, usize)> {
    let mut s = UnixStream::connect(sock).ok()?;
    s.set_read_timeout(Some(ANSWER_CAP)).ok()?;
    s.write_all(line(msg).as_bytes()).ok()?;
    let mut back = String::new();
    BufReader::new(s).read_line(&mut back).ok()?;
    Some((serde_json::from_str(&back).ok()?, back.len()))
}

/// An ask Cue has queued (its card's id), its connection open for the decision.
pub struct OpenAsk {
    pub id: String,
    reader: BufReader<UnixStream>,
}

/// `msg` (an ask) sent, as a permission hook sends it: open once Cue says it's queued.
pub fn ask(sock: &Path, msg: &Value) -> Option<OpenAsk> {
    let mut s = UnixStream::connect(sock).ok()?;
    s.write_all(line(msg).as_bytes()).ok()?;
    s.set_read_timeout(Some(ANSWER_CAP)).ok()?;
    let mut reader = BufReader::new(s);
    let queued = next(&mut reader)?;
    Some(OpenAsk { id: queued["id"].as_str()?.to_string(), reader })
}

impl OpenAsk {
    /// The decision, once it comes; None when the connection ended without one or `wait` passed.
    pub fn decision(mut self, wait: Duration) -> Option<Value> {
        self.reader.get_ref().set_read_timeout(Some(wait)).ok()?;
        next(&mut self.reader).filter(|v| v["type"] == "decision")
    }
}

fn next(r: &mut BufReader<UnixStream>) -> Option<Value> {
    let mut l = String::new();
    r.read_line(&mut l).ok().filter(|n| *n > 0)?;
    serde_json::from_str(&l).ok()
}
