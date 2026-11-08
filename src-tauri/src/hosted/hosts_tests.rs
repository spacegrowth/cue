//! Sessions through cue-host, as Cue runs them: a real host (in this process, on a socket of the test's own) and
//! the stand-in Claude (testing::FakeClaude). Above all, a Cue that quits mid-turn and starts again: the turn goes
//! on, nothing it had already seen comes again, an ask still waiting is asked again, and the answer gets there.

use super::running::{Hosted, Report};
use super::{Agent, Answer, AskKind, HostedEvent, Hosts, Program, Spec, Start};
use crate::model::Decision;
use crate::testing::{until, FakeClaude};
use serde_json::json;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

/// A host serving a folder of the test's own (short: a socket's path must fit in 104 bytes), and where Cue keeps
/// what it has read.
struct Place {
    home: PathBuf,
    seen: PathBuf,
}

fn host(name: &str) -> Place {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = PathBuf::from(format!("/tmp/ch-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("h");
    std::fs::create_dir_all(&home).unwrap();
    let sock = home.join("host.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let h = cue_host::server::Host::new(&home, cue_host::log::LIMIT);
    std::thread::spawn(move || h.serve(listener, &sock, Duration::from_secs(600)));
    Place { home, seen: root.join("seen") }
}

/// A Cue's hosted sessions through that host (the host is running: nothing to start it with).
fn cue(p: &Place) -> Hosts {
    Hosts::new(p.home.clone(), vec![], p.seen.clone())
}

fn spec(fake: &FakeClaude, start: Start) -> Spec {
    Spec { agent: Agent::Claude, cwd: std::env::temp_dir(), start, permissions: String::new(), model: None, args: vec![], program: Program::At(fake.program()) }
}

fn report() -> (Report, Receiver<HostedEvent>) {
    let (tx, rx) = channel();
    let r: Report = Box::new(move |_, e| {
        let _ = tx.send(e);
    });
    (r, rx)
}

fn until_event(rx: &Receiver<HostedEvent>, stop: impl Fn(&HostedEvent) -> bool) -> Vec<HostedEvent> {
    let end = Instant::now() + Duration::from_secs(10);
    let mut seen = vec![];
    loop {
        let e = rx.recv_timeout(end.saturating_duration_since(Instant::now())).unwrap_or_else(|_| panic!("no such event in 10 s; saw {seen:?}"));
        let done = stop(&e);
        seen.push(e);
        if done {
            return seen;
        }
    }
}

fn init(sid: &str) -> serde_json::Value {
    json!({ "type": "system", "subtype": "init", "session_id": sid, "model": "claude-haiku-5-5" })
}

fn echo(text: &str) -> serde_json::Value {
    json!({ "type": "user", "isReplay": true, "message": { "content": text } })
}

fn said(text: &str) -> serde_json::Value {
    json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": text }] } })
}

fn bash_ask(id: &str) -> serde_json::Value {
    json!({ "type": "control_request", "request_id": id, "request": { "subtype": "can_use_tool", "tool_name": "Bash", "input": { "command": "ls" }, "tool_use_id": "t1" } })
}

fn result(text: &str) -> serde_json::Value {
    json!({ "type": "result", "subtype": "success", "result": text, "total_cost_usd": 0.01 })
}

fn allow() -> Answer {
    Answer::Decision(Decision { behavior: "allow".into(), ..Default::default() })
}

#[test]
fn a_session_through_the_host_reports_takes_messages_and_answers_as_one_run_by_cue_does() {
    let p = host("plain");
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).say(bash_ask("r1")).wait_for("control_response").say(result("done"));
    let hosts = cue(&p);
    let (r, rx) = report();
    let started = hosts.start(&spec(&fake, start), r).unwrap();
    assert_eq!(started.session_id, sid);
    assert!(started.pid > 0 && hosts.running(&sid));
    hosts.send(&sid, "go").unwrap();
    let seen = until_event(&rx, |e| matches!(e, HostedEvent::Ask(_)));
    assert!(seen.contains(&HostedEvent::TurnStarted) && seen.contains(&HostedEvent::Taken { text: "go".into() }), "{seen:?}");
    hosts.answer(&sid, "r1", &allow()).unwrap();
    let end = until_event(&rx, |e| matches!(e, HostedEvent::TurnEnded { .. })).pop().unwrap();
    assert!(matches!(end, HostedEvent::TurnEnded { ref text, failed: false, .. } if text == "done"), "{end:?}");
    hosts.close(&sid);
    until("it's ended", || !hosts.running(&sid));
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "closed on purpose: no exit reported");
}

#[test]
fn a_cue_that_quits_mid_turn_and_starts_again_carries_the_turn_on_and_asks_again_what_still_waits() {
    let p = host("again");
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    // Asks, waits for its answer; once answered, says more and ends its turn.
    let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).say(said("about to list")).say(bash_ask("r1")).wait_for("control_response").say(said("listed")).say(result("all done"));
    let s = spec(&fake, start);
    let first = cue(&p);
    let (r, rx) = report();
    first.start(&s, r).unwrap();
    first.send(&sid, "go").unwrap();
    until_event(&rx, |e| matches!(e, HostedEvent::Ask(_)));
    // Cue quits: the agent goes on, under the host, waiting for its answer.
    first.close_all();
    drop(first);
    let again = cue(&p);
    let (r2, rx2) = report();
    let back = again.reattach(&Spec { start: Start::Resume(sid.clone()), ..s.clone() }, r2).expect("still running under the host");
    let started = back.started.as_ref().expect("its process");
    assert_eq!(started.session_id, sid);
    assert!(started.in_turn, "mid-turn");
    assert!(rx2.recv_timeout(Duration::from_millis(300)).is_err(), "nothing reported before it's told to go");
    (back.go)();
    // Only the ask comes again; nothing it had seen (the turn starting, its message, what it said) does.
    let Some(HostedEvent::Ask(a)) = rx2.recv_timeout(Duration::from_secs(5)).ok() else { panic!("the ask, again") };
    assert_eq!((a.request_id.as_str(), a.kind), ("r1", AskKind::Permission));
    assert!(rx2.recv_timeout(Duration::from_millis(500)).is_err(), "nothing else from before");
    again.answer(&sid, "r1", &allow()).unwrap();
    let rest = until_event(&rx2, |e| matches!(e, HostedEvent::TurnEnded { .. }));
    assert!(rest.contains(&HostedEvent::Said { text: "listed".into(), agent: None }), "{rest:?}");
    assert!(matches!(rest.last(), Some(HostedEvent::TurnEnded { text, .. }) if text == "all done"));
    again.close(&sid);
}

#[test]
fn what_it_said_while_no_cue_was_there_comes_in_once_cue_is_back_and_an_answered_ask_isnt_asked_again() {
    let p = host("away");
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).say(bash_ask("r1")).wait_for("control_response").pause_ms(1500).say(said("finished while you were away")).say(result("done"));
    let s = spec(&fake, start);
    let first = cue(&p);
    let (r, rx) = report();
    first.start(&s, r).unwrap();
    first.send(&sid, "go").unwrap();
    until_event(&rx, |e| matches!(e, HostedEvent::Ask(_)));
    first.answer(&sid, "r1", &allow()).unwrap();
    first.close_all();
    drop(first);
    // Its turn ends while no Cue is there.
    std::thread::sleep(Duration::from_millis(2500));
    let again = cue(&p);
    let (r2, rx2) = report();
    let back = again.reattach(&Spec { start: Start::Resume(sid.clone()), ..s }, r2).expect("its agent still runs");
    assert!(back.started.as_ref().unwrap().in_turn, "picked up as Cue left it, mid-turn; the turn's end, which came while it was away, follows");
    (back.go)();
    let seen = until_event(&rx2, |e| matches!(e, HostedEvent::TurnEnded { .. }));
    assert!(seen.iter().all(|e| !matches!(e, HostedEvent::Ask(_))), "answered before Cue quit: not asked again: {seen:?}");
    assert!(seen.contains(&HostedEvent::Said { text: "finished while you were away".into(), agent: None }), "{seen:?}");
    again.close(&sid);
}

#[test]
fn a_session_that_ended_while_cue_was_away_says_so_and_isnt_running() {
    let p = host("ended");
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).pause_ms(1000).complain("fatal: gone").exit(4);
    let s = spec(&fake, start);
    let first = cue(&p);
    let (r, rx) = report();
    first.start(&s, r).unwrap();
    first.send(&sid, "go").unwrap();
    until_event(&rx, |e| matches!(e, HostedEvent::Taken { .. }));
    first.close_all();
    drop(first);
    std::thread::sleep(Duration::from_millis(2000));
    let again = cue(&p);
    let (r2, rx2) = report();
    let back = again.reattach(&Spec { start: Start::Resume(sid.clone()), ..s.clone() }, r2).expect("it has records Cue hasn't read");
    assert!(back.started.is_none(), "nothing runs for it");
    (back.go)();
    let HostedEvent::Exited { code, during_turn, last_words } = until_event(&rx2, |e| matches!(e, HostedEvent::Exited { .. })).pop().unwrap() else { unreachable!() };
    assert_eq!((code, during_turn), (Some(4), true));
    assert!(last_words.contains("fatal: gone"), "{last_words}");
    assert!(!again.running(&sid));
    // Read to its end: the next Cue has nothing to pick up.
    let (r3, _rx3) = report();
    assert!(cue(&p).reattach(&Spec { start: Start::Resume(sid), ..s }, r3).is_none());
}

#[test]
fn a_session_running_under_the_host_isnt_ended_as_a_leftover() {
    let p = host("left");
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let fake = FakeClaude::new();
    let hosts = cue(&p);
    let (r, _rx) = report();
    let pid = hosts.start(&spec(&fake, start), r).unwrap().pid;
    hosts.end_left(&[pid]);
    std::thread::sleep(Duration::from_millis(300));
    assert!(hosts.running(&sid), "the host's: left alone");
    hosts.close(&sid);
}
