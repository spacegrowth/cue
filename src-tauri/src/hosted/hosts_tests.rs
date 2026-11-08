//! Sessions through cue-host, as Cue runs them: a real host (in this process, on a socket of the test's own) and
//! the stand-in Claude (testing::FakeClaude). Above all, a Cue that quits mid-turn and starts again: the turn goes
//! on, nothing it had already seen comes again, an ask still waiting is asked again, and the answer gets there.

use super::running::{Hosted, Report};
use super::{Agent, Answer, AskKind, HostedEvent, Hosts, Program, Spec, Start};
use crate::model::Decision;
use crate::testing::{until, FakeClaude};
use serde_json::json;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

/// Hosts under a folder of the test's own (short: a socket's path must fit in 104 bytes): the one of Cue's own build
/// in `home` (`b-now`), serving; and where Cue keeps what it has read. All of it removed when the test ends.
struct Place {
    root: PathBuf,
    home: PathBuf,
    seen: PathBuf,
}

impl Drop for Place {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn host(name: &str) -> Place {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = PathBuf::from(format!("/tmp/ch-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("b-now");
    serve_at(&home);
    Place { seen: root.join("seen"), root, home }
}

/// A host serving `dir` (in this process).
fn serve_at(dir: &Path) {
    serve_on(dir, &dir.join("host.sock"));
}

/// A host keeping its sessions in `dir`, listening on `sock`.
fn serve_on(dir: &Path, sock: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let listener = UnixListener::bind(sock).unwrap();
    let h = cue_host::server::Host::new(dir, cue_host::log::LIMIT);
    let sock = sock.to_path_buf();
    std::thread::spawn(move || h.serve(listener, &sock, Duration::from_secs(600)));
}

/// A Cue's hosted sessions through that host (the host is running: nothing to start it with).
fn cue(p: &Place) -> Hosts {
    cue_at(p, &p.home)
}

/// A Cue whose connection to its host is made again when it drops, as the app's is (the host is running, so the
/// program that would start one is never run).
fn cue_coming_back(p: &Place) -> Hosts {
    Hosts::new(p.root.clone(), p.home.clone(), vec!["/usr/bin/false".into()], p.seen.clone())
}

/// A Cue whose own build's host lives in `home` (under the same root).
fn cue_at(p: &Place, home: &Path) -> Hosts {
    Hosts::new(p.root.clone(), home.to_path_buf(), vec![], p.seen.clone())
}

/// Until the host's log for session `sid` has `needle` (something the agent did while no Cue was there).
fn until_logged(p: &Place, sid: &str, needle: &str) {
    let log = p.home.join("sessions").join(sid).join("log.jsonl");
    until(&format!("{needle} in {}", log.display()), || std::fs::read_to_string(&log).is_ok_and(|l| l.contains(needle)));
}

fn spec(fake: &FakeClaude, start: Start) -> Spec {
    Spec { agent: Agent::Claude, cwd: std::env::temp_dir(), start, permissions: String::new(), model: None, args: vec![], program: Program::At(fake.program()), machine: String::new() }
}

pub(super) fn report() -> (Report, Receiver<HostedEvent>) {
    let (tx, rx) = channel();
    let r: Report = Box::new(move |_, e| {
        let _ = tx.send(e);
    });
    (r, rx)
}

pub(super) fn until_event(rx: &Receiver<HostedEvent>, stop: impl Fn(&HostedEvent) -> bool) -> Vec<HostedEvent> {
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

pub(super) fn init(sid: &str) -> serde_json::Value {
    json!({ "type": "system", "subtype": "init", "session_id": sid, "model": "claude-haiku-5-5" })
}

pub(super) fn echo(text: &str) -> serde_json::Value {
    json!({ "type": "user", "isReplay": true, "message": { "content": text } })
}

pub(super) fn said(text: &str) -> serde_json::Value {
    json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": text }] } })
}

pub(super) fn bash_ask(id: &str) -> serde_json::Value {
    json!({ "type": "control_request", "request_id": id, "request": { "subtype": "can_use_tool", "tool_name": "Bash", "input": { "command": "ls" }, "tool_use_id": "t1" } })
}

pub(super) fn result(text: &str) -> serde_json::Value {
    json!({ "type": "result", "subtype": "success", "result": text, "total_cost_usd": 0.01 })
}

pub(super) fn allow() -> Answer {
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
    // Its turn ends while no Cue is there (the host has kept the end of it).
    until_logged(&p, &sid, r#"\"type\":\"result\""#);
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
    until_logged(&p, &sid, r#""exited":4"#);
    let again = cue(&p);
    let (r2, rx2) = report();
    let back = again.reattach(&Spec { start: Start::Resume(sid.clone()), ..s.clone() }, r2).expect("it has records Cue hasn't read");
    assert!(back.started.is_none(), "nothing runs for it");
    (back.go)();
    let HostedEvent::Exited { code, during_turn, last_words } = until_event(&rx2, |e| matches!(e, HostedEvent::Exited { .. })).pop().unwrap() else { unreachable!() };
    assert_eq!((code, during_turn), (Some(4), true));
    assert!(last_words.contains("fatal: gone"), "{last_words}");
    assert!(!again.running(&sid));
    // Read to its end (where Cue has read to is kept just after each record is handled): the next Cue has nothing
    // to pick up.
    let last = std::fs::read_to_string(p.home.join("sessions").join(&sid).join("log.jsonl")).unwrap().lines().count() as u64;
    let read_to = || std::fs::read_to_string(p.seen.join(format!("{sid}.json"))).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v["seq"].as_u64());
    until("Cue's place kept at the end", || read_to() == Some(last));
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

/// A session `fake` runs, started through `hosts` with a message taken: its id, and where it reports.
fn running_session(hosts: &Hosts, fake: &FakeClaude) -> (String, Spec, Receiver<HostedEvent>) {
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let s = spec(fake, start);
    let (r, rx) = report();
    hosts.start(&s, r).unwrap();
    (sid, s, rx)
}

fn sessions_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir.join("sessions")).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    v.sort();
    v
}

#[test]
fn after_an_update_a_session_on_the_older_host_is_picked_up_new_ones_go_to_the_new_host_and_the_old_one_goes_once_empty() {
    let p = host("upd");
    let old_home = p.root.join("b-old");
    serve_at(&old_home);
    // The Cue before the update, whose host was the old build's: a session on it, then Cue quits.
    let fake = FakeClaude::new();
    let before = cue_at(&p, &old_home);
    let (old_sid, old_spec, _rx) = running_session(&before, &fake);
    before.close_all();
    drop(before);
    // The updated Cue: its own build's host is `b-now`; the old one is found and kept for what it runs.
    let after = cue(&p);
    let (r, _rx2) = report();
    let back = after.reattach(&Spec { start: Start::Resume(old_sid.clone()), ..old_spec }, r).expect("picked up from the older host");
    assert!(back.started.is_some());
    (back.go)();
    after.settled();
    assert!(old_home.exists(), "it still runs a session: kept");
    assert_eq!(after.host_status()["older"].as_array().map(Vec::len), Some(1));
    // A new session goes to the new build's host, never the old one.
    let fake2 = FakeClaude::new();
    let (new_sid, _, _rx3) = running_session(&after, &fake2);
    assert_eq!(sessions_in(&p.home), [new_sid.clone()]);
    assert_eq!(sessions_in(&old_home), [old_sid.clone()]);
    // A message still reaches the session on the old host.
    after.send(&old_sid, "still there?").unwrap();
    until("the old host's session hears it", || fake.heard().iter().any(|v| v["message"]["content"] == "still there?"));
    // Its agent ends: the old host runs nothing, and goes (its folder with it).
    after.close(&old_sid);
    after.settled();
    assert!(!old_home.exists(), "the older host's folder is cleared once it runs nothing");
    assert_eq!(after.host_status()["older"].as_array().map(Vec::len), Some(0));
    assert!(after.running(&new_sid), "the new host's session is untouched");
    after.close(&new_sid);
}

#[test]
fn the_first_host_from_before_builds_is_let_go_too_its_files_cleared_and_the_newer_folders_left() {
    let p = host("legacy");
    // The first host lived in the root itself.
    serve_at(&p.root);
    let fake = FakeClaude::new();
    let before = Hosts::new(p.root.clone(), p.root.clone(), vec![], p.seen.clone());
    let (sid, _, _rx) = running_session(&before, &fake);
    before.close_all();
    drop(before);
    let after = cue(&p);
    assert_eq!(after.host_status()["older"].as_array().map(Vec::len), Some(1), "found in the root");
    // Cue doesn't know its session any more (never picked up): settling closes it, and the host goes.
    after.settled();
    until("its agent closed", || !p.root.join("host.sock").exists());
    assert!(!p.root.join("sessions").join(&sid).exists(), "its logs are gone");
    assert!(p.home.join("host.sock").exists(), "the new build's host is untouched");
    assert_eq!(after.host_status()["older"].as_array().map(Vec::len), Some(0));
}

#[test]
fn restarting_the_host_ends_every_agent_saying_so_and_the_next_session_starts_on_a_fresh_host() {
    let p = host("restart");
    let fake = FakeClaude::new();
    let hosts = cue(&p);
    let (sid, _, rx) = running_session(&hosts, &fake);
    let said = hosts.restart_host().unwrap();
    assert!(said.contains("1 session ended"), "{said}");
    assert!(matches!(until_event(&rx, |e| matches!(e, HostedEvent::Exited { .. })).pop(), Some(HostedEvent::Exited { .. })), "its exit is reported: it shows paused");
    assert!(!hosts.running(&sid));
    // The in-process host isn't ended by a signal (it's this test): a fresh one would be started; here the same
    // one is reached again for the next session.
    let fake2 = FakeClaude::new();
    let (next, _, _rx2) = running_session(&hosts, &fake2);
    assert!(hosts.running(&next));
    hosts.close(&next);
}

/// How many times `fake` was sent `text` as a message.
fn heard_times(fake: &FakeClaude, text: &str) -> usize {
    fake.heard().iter().filter(|v| v["message"]["content"] == text).count()
}

#[test]
fn a_message_sent_while_the_connection_to_the_host_is_down_is_held_and_gets_there_once_when_its_back() {
    let p = host("held");
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let fake = FakeClaude::new().wait_for("go").say(init(&sid)).say(echo("go")).say(result("done"));
    let hosts = cue_coming_back(&p);
    let (r, rx) = report();
    hosts.start(&spec(&fake, start), r).unwrap();
    hosts.drop_connection();
    until("the connection is down", || !hosts.connected());
    assert!(hosts.running(&sid), "not known while it's down: taken as running, so nothing starts a second one");
    hosts.send(&sid, "go").expect("held, not refused");
    assert_eq!(heard_times(&fake, "go"), 0, "not there yet");
    let seen = until_event(&rx, |e| matches!(e, HostedEvent::TurnEnded { .. }));
    assert!(seen.contains(&HostedEvent::Taken { text: "go".into() }), "{seen:?}");
    assert!(hosts.connected(), "made again");
    assert_eq!(heard_times(&fake, "go"), 1, "once");
    // And after it, as before.
    hosts.send(&sid, "more").unwrap();
    until("the next one gets there too", || heard_times(&fake, "more") == 1);
    hosts.close(&sid);
}

#[test]
fn a_host_that_comes_back_without_the_session_pauses_it_and_says_what_was_held_wasnt_taken() {
    let p = host("lost");
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let fake = FakeClaude::new().wait_for("go");
    let hosts = cue_coming_back(&p);
    let (r, rx) = report();
    hosts.start(&spec(&fake, start), r).unwrap();
    // The host dies with the connection, and the next one Cue reaches (in the same place) never ran it.
    hosts.drop_connection();
    until("the connection is down", || !hosts.connected());
    let sock = p.home.join("host.sock");
    std::fs::remove_file(&sock).unwrap();
    serve_on(&p.root.join("b-fresh"), &sock);
    hosts.send(&sid, "go").expect("held, not refused");
    let seen = until_event(&rx, |e| matches!(e, HostedEvent::Exited { .. }));
    assert!(seen.iter().any(|e| matches!(e, HostedEvent::NotTaken { text, .. } if text == "go")), "{seen:?}");
    assert!(!hosts.running(&sid), "paused: a message starts it again");
    assert_eq!(heard_times(&fake, "go"), 0);
}
