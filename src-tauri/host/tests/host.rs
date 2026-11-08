//! The host as Cue uses it: a real host on a socket in a folder of the test's own, real children (sh, cat), and a
//! client speaking its protocol.

use cue_host::proto::{FromHost, Hello, Record, What};
use cue_host::server::Host;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

/// A folder of the test's own, removed when the test ends. Short: a socket's path must fit in 104 bytes.
struct Home(PathBuf);

impl std::ops::Deref for Home {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn home(name: &str) -> Home {
    let d = PathBuf::from(format!("/tmp/ch-{}-{}-{}", name, std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Home(d)
}

/// A host serving `home` on a thread, idle after `idle`.
fn serve(home: &Path, idle: Duration) -> std::thread::JoinHandle<()> {
    let sock = home.join("host.sock");
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();
    let host = Host::new(home, cue_host::log::LIMIT);
    std::thread::spawn(move || host.serve(listener, &sock, idle))
}

struct Client {
    out: UnixStream,
    lines: Receiver<FromHost>,
    next: u64,
    /// Events that came while waiting for a response, in order.
    events: Vec<FromHost>,
}

/// Gone as a client really goes: its connection closed (the reader thread's handle to it too).
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.out.shutdown(std::net::Shutdown::Both);
    }
}

impl Client {
    fn new(home: &Path) -> Client {
        let out = UnixStream::connect(home.join("host.sock")).unwrap();
        let input = out.try_clone().unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for l in BufReader::new(input).lines().map_while(Result::ok) {
                if tx.send(serde_json::from_str::<FromHost>(&l).unwrap()).is_err() {
                    break;
                }
            }
        });
        Client { out, lines, next: 1, events: vec![] }
    }

    /// A request and its response (events meanwhile are kept for `records`).
    fn ask(&mut self, op: Value) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        let mut req = op;
        req["id"] = json!(id);
        writeln!(self.out, "{req}").unwrap();
        loop {
            match self.lines.recv_timeout(Duration::from_secs(10)).expect("an answer") {
                FromHost::Response { id: got, ok, result, error } if got == id => return if ok { Ok(result) } else { Err(error.unwrap_or_default()) },
                other => self.events.push(other),
            }
        }
    }

    /// Records for `session` until `done` says so (or 10 s go by).
    fn records_until(&mut self, session: &str, done: impl Fn(&[Record]) -> bool) -> Vec<Record> {
        let mut got: Vec<Record> = vec![];
        let take = |e: FromHost, got: &mut Vec<Record>| {
            if let FromHost::Event { session: s, event: cue_host::proto::Event::Record { record } } = e {
                if s == session {
                    got.push(record);
                }
            }
        };
        for e in std::mem::take(&mut self.events) {
            take(e, &mut got);
        }
        let until = Instant::now() + Duration::from_secs(10);
        while !done(&got) {
            match self.lines.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(e) => take(e, &mut got),
                Err(RecvTimeoutError::Timeout) => panic!("timed out; got {got:?}"),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        got
    }

    fn hello(&mut self) -> Hello {
        serde_json::from_value(self.ask(json!({ "op": "hello", "version": 1 })).unwrap()).unwrap()
    }
}

fn start(c: &mut Client, session: &str, script: &str) -> Result<Value, String> {
    let env: Vec<(String, String)> = vec![("PATH".into(), "/usr/bin:/bin".into())];
    c.ask(json!({ "op": "start", "session": session, "argv": ["/bin/sh", "-c", script], "env": env, "cwd": "/tmp", "meta": { "agent": "test" } }))
}

fn exited(rs: &[Record]) -> bool {
    rs.iter().any(|r| matches!(r.what, What::Exited(_)))
}

fn outs(rs: &[Record]) -> Vec<String> {
    rs.iter().filter_map(|r| if let What::Out(l) = &r.what { Some(l.clone()) } else { None }).collect()
}

#[test]
fn a_childs_lines_come_numbered_then_its_exit_and_read_back_from_any_point() {
    let h = home("lines");
    serve(&h, Duration::from_secs(600));
    let mut c = Client::new(&h);
    c.ask(json!({ "op": "attach", "session": "s1", "after": 0 })).expect_err("nothing under that id yet");
    start(&mut c, "s1", "echo one; echo two >&2; echo three; exit 3").unwrap();
    c.ask(json!({ "op": "attach", "session": "s1", "after": 0 })).unwrap();
    let rs = c.records_until("s1", exited);
    assert_eq!(outs(&rs), ["one", "three"]);
    assert!(rs.iter().any(|r| r.what == What::Err("two".into())));
    assert_eq!(rs.last().unwrap().what, What::Exited(Some(3)));
    assert_eq!(rs.iter().map(|r| r.seq).collect::<Vec<_>>(), (1..=rs.len() as u64).collect::<Vec<_>>(), "numbered from 1, no holes");
    // A client that comes later, having read up to the first: the rest, in order.
    let mut later = Client::new(&h);
    later.ask(json!({ "op": "attach", "session": "s1", "after": 1 })).unwrap();
    let again = later.records_until("s1", exited);
    assert_eq!(again, rs[1..].to_vec());
    let info = &later.hello().sessions[0];
    assert_eq!((info.running, info.code, info.first_seq, info.last_seq, info.meta["agent"].as_str()), (false, Some(3), 1, rs.len() as u64, Some("test")));
}

#[test]
fn a_running_session_isnt_started_twice_and_one_that_ended_starts_again_numbering_on() {
    let h = home("twice");
    serve(&h, Duration::from_secs(600));
    let mut c = Client::new(&h);
    start(&mut c, "s1", "read x; echo got $x").unwrap();
    let refused = start(&mut c, "s1", "echo second").unwrap_err();
    assert!(refused.contains("already running"), "{refused}");
    c.ask(json!({ "op": "attach", "session": "s1", "after": 0 })).unwrap();
    c.ask(json!({ "op": "write", "session": "s1", "line": "hi" })).unwrap();
    let first = c.records_until("s1", exited);
    assert_eq!(outs(&first), ["got hi"]);
    start(&mut c, "s1", "echo again").unwrap();
    let second = c.records_until("s1", exited);
    assert_eq!(outs(&second), ["again"]);
    assert_eq!(second[0].seq, first.last().unwrap().seq + 1, "one numbering across both runs: {first:?} then {second:?}");
    // Where the latest run began, and its records read back up to a point (what a Cue that started again re-reads).
    let info = c.hello().sessions.remove(0);
    assert_eq!(info.run_first, second[0].seq);
    let read: Vec<Record> = serde_json::from_value(c.ask(json!({ "op": "read", "session": "s1", "after": info.run_first - 1, "upto": second[0].seq })).unwrap()).unwrap();
    assert_eq!(read, second[..1].to_vec());
}

#[test]
fn a_write_reaches_the_child_is_kept_and_isnt_written_twice_under_one_message_id() {
    let h = home("write");
    serve(&h, Duration::from_secs(600));
    let mut c = Client::new(&h);
    start(&mut c, "s1", "cat").unwrap();
    c.ask(json!({ "op": "attach", "session": "s1", "after": 0 })).unwrap();
    assert_eq!(c.ask(json!({ "op": "write", "session": "s1", "line": "hello", "msg": "m1" })).unwrap()["written"], true);
    assert_eq!(c.ask(json!({ "op": "write", "session": "s1", "line": "hello", "msg": "m1" })).unwrap()["written"], false, "the same message again");
    c.ask(json!({ "op": "write", "session": "s1", "line": "bye", "msg": "m2" })).unwrap();
    let rs = c.records_until("s1", |rs| outs(rs).len() == 2);
    assert_eq!(outs(&rs), ["hello", "bye"]);
    let ins: Vec<&What> = rs.iter().map(|r| &r.what).filter(|w| matches!(w, What::In(_))).collect();
    assert_eq!(ins, [&What::In("hello".into()), &What::In("bye".into())], "what was written is in the log too");
    c.ask(json!({ "op": "close", "session": "s1" })).unwrap();
    assert!(!c.hello().sessions[0].running);
}

#[test]
fn close_ends_a_child_that_ignores_its_input_closing_and_everything_it_started() {
    let h = home("close");
    serve(&h, Duration::from_secs(600));
    let mut c = Client::new(&h);
    // Ignores its input closing, and starts a background job holding its output.
    let pid = start(&mut c, "s1", "sleep 300 & echo up; while :; do sleep 1; done").unwrap()["pid"].as_i64().unwrap() as i32;
    c.ask(json!({ "op": "attach", "session": "s1", "after": 0 })).unwrap();
    c.records_until("s1", |rs| !outs(rs).is_empty());
    let t = Instant::now();
    c.ask(json!({ "op": "close", "session": "s1" })).unwrap();
    assert!(t.elapsed() < Duration::from_secs(8), "within its graces");
    c.records_until("s1", exited);
    // SAFETY: kill with signal 0 only asks whether the group exists.
    assert!(unsafe { libc::killpg(pid, 0) } != 0, "its whole group is gone");
}

#[test]
fn every_client_attached_gets_every_record_and_a_client_leaving_takes_nothing_with_it() {
    let h = home("two");
    serve(&h, Duration::from_secs(600));
    let (mut a, mut b) = (Client::new(&h), Client::new(&h));
    start(&mut a, "s1", "cat").unwrap();
    a.ask(json!({ "op": "attach", "session": "s1", "after": 0 })).unwrap();
    b.ask(json!({ "op": "attach", "session": "s1", "after": 0 })).unwrap();
    a.ask(json!({ "op": "write", "session": "s1", "line": "from a" })).unwrap();
    assert_eq!(outs(&b.records_until("s1", |rs| !outs(rs).is_empty())), ["from a"]);
    drop(a);
    b.ask(json!({ "op": "write", "session": "s1", "line": "from b" })).unwrap();
    assert_eq!(outs(&b.records_until("s1", |rs| !outs(rs).is_empty())), ["from b"]);
    assert!(b.hello().sessions[0].running, "its child runs on without the client that started it");
    b.ask(json!({ "op": "forget", "session": "s1" })).unwrap();
    assert!(b.hello().sessions.is_empty());
    assert!(!h.join("sessions/s1").exists(), "its log is gone with it");
}

#[test]
fn a_new_host_finds_the_last_ones_sessions_and_says_a_child_it_lost_ended() {
    let h = home("again");
    // As an earlier host left it: a session that ran, its log ending mid-run (the host went with it running).
    let dir = h.join("sessions/s1");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("meta.json"), r#"{"agent":"claude"}"#).unwrap();
    std::fs::write(dir.join("log.jsonl"), "{\"seq\":1,\"out\":\"working\"}\n{\"seq\":2,\"in\":\"more\"}\n").unwrap();
    serve(&h, Duration::from_secs(600));
    let mut c = Client::new(&h);
    let s = c.hello().sessions.remove(0);
    assert_eq!((s.session.as_str(), s.running, s.last_seq, s.meta["agent"].as_str()), ("s1", false, 3, Some("claude")));
    c.ask(json!({ "op": "attach", "session": "s1", "after": 2 })).unwrap();
    assert_eq!(c.records_until("s1", exited), [Record { seq: 3, what: What::Exited(None) }]);
}

#[test]
fn a_session_quiet_for_a_month_is_dropped_when_a_host_starts() {
    let h = home("old");
    for (id, days) in [("old", 40u64), ("recent", 2)] {
        let dir = h.join("sessions").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("log.jsonl"), "{\"seq\":1,\"exited\":0}\n").unwrap();
        let when = std::time::SystemTime::now() - Duration::from_secs(days * 24 * 3600);
        std::fs::File::options().write(true).open(dir.join("log.jsonl")).unwrap().set_modified(when).unwrap();
    }
    serve(&h, Duration::from_secs(600));
    let ids: Vec<String> = Client::new(&h).hello().sessions.into_iter().map(|s| s.session).collect();
    assert_eq!(ids, ["recent"]);
    assert!(!h.join("sessions/old").exists());
}

#[test]
fn with_nothing_to_do_it_takes_its_socket_away_and_stops() {
    let h = home("idle");
    let served = serve(&h, Duration::from_millis(300));
    let mut c = Client::new(&h);
    start(&mut c, "s1", "sleep 1").unwrap();
    drop(c);
    std::thread::sleep(Duration::from_millis(600));
    assert!(h.join("host.sock").exists(), "a child still runs: it stays");
    let t = Instant::now();
    while !served.is_finished() {
        assert!(t.elapsed() < Duration::from_secs(10), "it never went");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!h.join("host.sock").exists());
}

#[test]
fn connect_starts_a_host_when_none_runs_and_finds_the_running_one_after() {
    let h = home("daemon");
    let serve_cmd = vec![env!("CARGO_BIN_EXE_cue-host").to_string(), "serve".into(), "--home".into(), h.to_string_lossy().into_owned()];
    let s = cue_host::daemon::connect(&h, &serve_cmd).unwrap();
    drop(s);
    let pid: i32 = std::fs::read_to_string(h.join("host.pid")).unwrap().trim().parse().unwrap();
    let mut c = Client::new(&h);
    assert_eq!(c.hello().pid, pid);
    let s2 = cue_host::daemon::connect(&h, &serve_cmd).unwrap();
    drop(s2);
    assert_eq!(Client::new(&h).hello().pid, pid, "the same host");
    // SAFETY: ends the host this test started.
    unsafe { libc::kill(pid, libc::SIGTERM) };
}

#[test]
fn a_child_started_with_inherit_gets_the_hosts_environment_with_cues_over_it_and_without_only_cues() {
    let h = home("inherit");
    serve(&h, Duration::from_secs(600));
    let mut c = Client::new(&h);
    let script = "echo \"home=${HOME:-none} cue=${CUE_X:-none}\"";
    let env = vec![("CUE_X".to_string(), "given".to_string()), ("PATH".to_string(), "/usr/bin:/bin".to_string())];
    c.ask(json!({ "op": "start", "session": "inh", "argv": ["/bin/sh", "-c", script], "env": env, "cwd": "/tmp", "meta": {}, "inherit": true })).unwrap();
    c.ask(json!({ "op": "start", "session": "own", "argv": ["/bin/sh", "-c", script], "env": env, "cwd": "/tmp", "meta": {} })).unwrap();
    for s in ["inh", "own"] {
        c.ask(json!({ "op": "attach", "session": s, "after": 0 })).unwrap();
    }
    let home_here = std::env::var("HOME").unwrap();
    assert_eq!(outs(&c.records_until("inh", exited)), [format!("home={home_here} cue=given")], "the host's HOME, Cue's variable");
    assert_eq!(outs(&c.records_until("own", exited)), ["home=none cue=given"], "exactly what Cue gave");
}

impl Client {
    /// The text of `tail`'s events since the last call, as Cue would put it together (an event at 0 replaces what came
    /// before it), until `done` says so (or 10 s go by).
    fn tailed_until(&mut self, tail: &str, done: impl Fn(&str) -> bool) -> String {
        let mut got = String::new();
        let take = |e: FromHost, got: &mut String| {
            if let FromHost::Event { session, event: cue_host::proto::Event::Tail { at, data } } = e {
                if session == tail {
                    if at == 0 {
                        got.clear();
                    }
                    got.push_str(&data);
                }
            }
        };
        for e in std::mem::take(&mut self.events) {
            take(e, &mut got);
        }
        let until = Instant::now() + Duration::from_secs(10);
        while !done(&got) {
            match self.lines.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(e) => take(e, &mut got),
                Err(_) => panic!("timed out; got {got:?}"),
            }
        }
        got
    }
}

#[test]
fn a_tailed_file_comes_over_as_it_is_written_whole_lines_only_and_again_when_written_over() {
    let h = home("tail");
    serve(&h, Duration::from_secs(600));
    let mut c = Client::new(&h);
    let file = h.join("transcript.jsonl");
    // Not there yet: waited for.
    c.ask(json!({ "op": "tail", "tail": "t1", "path": file, "from": 0 })).unwrap();
    std::fs::write(&file, "one\ntwo\nhal").unwrap();
    assert_eq!(c.tailed_until("t1", |t| t == "one\ntwo\n"), "one\ntwo\n", "a half-written line waits");
    let mut f = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
    std::io::Write::write_all(&mut f, b"f done\nthree\n").unwrap();
    assert_eq!(c.tailed_until("t1", |t| t.ends_with("three\n")), "half done\nthree\n", "only what's new, the half line whole now");
    // Written over, shorter: from its start again.
    std::fs::write(&file, "new\n").unwrap();
    assert_eq!(c.tailed_until("t1", |t| t == "new\n"), "new\n");
    // From a point: only what's after it. (Added to, as a transcript is: writing the file anew empties it first,
    // which the host rightly takes for written over.)
    c.ask(json!({ "op": "tail", "tail": "t2", "path": file, "from": 4 })).unwrap();
    std::io::Write::write_all(&mut std::fs::OpenOptions::new().append(true).open(&file).unwrap(), b"more\n").unwrap();
    assert_eq!(c.tailed_until("t2", |t| t.contains("more")), "more\n");
    // Stopped: nothing more comes.
    c.ask(json!({ "op": "untail", "tail": "t1" })).unwrap();
    c.ask(json!({ "op": "untail", "tail": "t2" })).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    c.events.clear();
    let mut f = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
    std::io::Write::write_all(&mut f, b"unseen\n").unwrap();
    std::thread::sleep(Duration::from_millis(600));
    assert!(c.lines.try_iter().all(|e| !matches!(e, FromHost::Event { event: cue_host::proto::Event::Tail { .. }, .. })), "untailed: nothing comes");
}
