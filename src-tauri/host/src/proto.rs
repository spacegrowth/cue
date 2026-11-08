//! What Cue and the host say to each other: JSON lines over the host's socket (or `connect`'s input and output
//! through SSH). Cue sends requests, each with an id of its own, and gets one response for each; the host also
//! sends events, unasked, for the sessions a client has attached to. The host never reads what's in a session's
//! lines: they're the agent's, and what they mean is Cue's to know.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// This protocol's version: `hello` carries it both ways.
pub const VERSION: u32 = 1;

/// A request, with the id its response carries back.
#[derive(Debug, Deserialize, Serialize)]
pub struct Envelope {
    pub id: u64,
    #[serde(flatten)]
    pub req: Request,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Who's there: the host's version and pid, and every session it has (running or not).
    Hello { version: u32 },
    /// Start a child under `session`: `argv` run in `cwd` with exactly `env`. `meta` is Cue's, kept and given back
    /// by `hello` (which agent it is, how it was started), so a Cue that comes back knows what it's reading. An id
    /// already running is refused, never restarted: a turn is never cut off by a second start.
    Start { session: String, argv: Vec<String>, env: Vec<(String, String)>, cwd: String, meta: Value },
    /// A line to the child's input. `msg`: Cue's own id for it, so a line sent again after a reconnect isn't
    /// written twice (the last few are remembered).
    Write { session: String, line: String, msg: Option<String> },
    /// Everything the session's log has after `after`, then each new record as it comes, until `detach` or this
    /// connection ends.
    Attach { session: String, after: u64 },
    /// The records after `after` up to `upto`, in the response (what a Cue that started again had already read: it
    /// reads them again to know where it was, then attaches after them).
    Read { session: String, after: u64, upto: u64 },
    Detach { session: String },
    /// Close the child: its input closed, a grace, then ended. Its log stays until `forget`.
    Close { session: String },
    /// Its log dropped (Cue closed or deleted the session for good). Closes it first if it runs.
    Forget { session: String },
}

/// A session, as `hello` lists it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SessionInfo {
    pub session: String,
    pub running: bool,
    /// The child's pid while it runs.
    pub pid: Option<i32>,
    /// How it exited, once it has (None also for a child that ended with its host).
    pub code: Option<i32>,
    /// The first record the log still has (older ones were dropped for size) and the last.
    pub first_seq: u64,
    pub last_seq: u64,
    /// Where its latest run began: its first record after it was last started.
    pub run_first: u64,
    pub meta: Value,
}

/// `hello`'s answer.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Hello {
    pub version: u32,
    pub pid: i32,
    pub sessions: Vec<SessionInfo>,
}

/// One record of a session's log, as it's kept and as an attached client gets it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Record {
    pub seq: u64,
    #[serde(flatten)]
    pub what: What,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum What {
    /// A line the child wrote to its output.
    Out(String),
    /// A line it wrote to its error output.
    Err(String),
    /// A line written to its input (by any client): kept so a Cue reading the log back knows what was answered.
    In(String),
    /// It exited: its code, None for a signal.
    Exited(Option<i32>),
}

/// What the host sends: a response to a request, or an event for an attached session.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum FromHost {
    Response { id: u64, ok: bool, #[serde(default, skip_serializing_if = "Value::is_null")] result: Value, #[serde(default, skip_serializing_if = "Option::is_none")] error: Option<String> },
    Event { session: String, #[serde(flatten)] event: Event },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum Event {
    Record { #[serde(flatten)] record: Record },
    /// The log no longer has what comes after `after`: it starts at `from` (dropped for size).
    Gap { from: u64 },
}

impl FromHost {
    pub fn ok(id: u64, result: Value) -> Self {
        FromHost::Response { id, ok: true, result, error: None }
    }

    pub fn err(id: u64, error: impl Into<String>) -> Self {
        FromHost::Response { id, ok: false, result: Value::Null, error: Some(error.into()) }
    }

    pub fn line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_request_reads_as_its_op_with_its_id() {
        let e: Envelope = serde_json::from_value(json!({ "id": 7, "op": "write", "session": "s", "line": "{}", "msg": "m1" })).unwrap();
        assert_eq!((e.id, e.req), (7, Request::Write { session: "s".into(), line: "{}".into(), msg: Some("m1".into()) }));
    }

    #[test]
    fn a_record_event_and_a_response_read_back_as_themselves() {
        let ev = FromHost::Event { session: "s".into(), event: Event::Record { record: Record { seq: 3, what: What::Out("hi".into()) } } };
        assert_eq!(serde_json::from_str::<FromHost>(&ev.line()).unwrap(), ev);
        assert_eq!(serde_json::to_value(&ev).unwrap(), json!({ "session": "s", "ev": "record", "seq": 3, "out": "hi" }));
        let gone = FromHost::Event { session: "s".into(), event: Event::Record { record: Record { seq: 4, what: What::Exited(None) } } };
        assert_eq!(serde_json::from_str::<FromHost>(&gone.line()).unwrap(), gone);
        let r = FromHost::err(2, "no such session");
        assert_eq!(serde_json::from_str::<FromHost>(&r.line()).unwrap(), r);
    }
}
