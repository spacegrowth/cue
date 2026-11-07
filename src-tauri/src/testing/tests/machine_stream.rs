//! A machine's watcher stream (machines::hooks::stream) into the World's socket: events reach Cue as the
//! Mac's hook sends them; a stream that drops, garbage, a line too long, a machine that can't be reached, a
//! watcher started again, a permission answered there, and a session log read from its copy, never from the
//! machine's own path (on this Mac a stat of /home can stall: `Stall`).

use crate::machines::{self, hooks, Machine};
use crate::testing::{until, FakeSsh, Served, Snap, Stall, World, RUNS_IT};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// The machine's watcher prints `$FAKE_HOME/stream` and ends (the line drops there); a log copy prints
/// `$FAKE_HOME/log` and ends; everything else runs for real in the machine's home.
const PLAYS_STREAM: &str = r#"case "$1" in
  *cue-watch*) exec cat "$FAKE_HOME/stream" ;;
  *cue-copy*) cat "$FAKE_HOME/log" 2>/dev/null; exit 0 ;;
esac
"#;

struct Remote {
    w: World,
    ssh: FakeSsh,
    m: Machine,
    _served: Served,
}

/// A World served on its socket, with machine `host` whose watcher prints `lines` (each ends in a newline
/// unless `cut`: the line dropped mid-line) and whose sessions' logs hold `log`.
fn machine(host: &str, lines: &[String], cut: bool, log: &str) -> Remote {
    let w = World::new();
    let served = w.serve();
    let ssh = FakeSsh::new();
    ssh.host(host, &format!("{PLAYS_STREAM}{RUNS_IT}"));
    let home = ssh.home(host);
    std::fs::create_dir_all(&home).unwrap();
    let mut stream = lines.join("\n");
    if !cut {
        stream.push('\n');
    }
    std::fs::write(home.join("stream"), stream).unwrap();
    std::fs::write(home.join("log"), log).unwrap();
    let m = machines::add(host, "").unwrap();
    Remote { w, ssh, m, _served: served }
}

impl Remote {
    /// The watcher's stream, to its end.
    fn stream(&self) {
        hooks::stream(&self.m, &AtomicBool::new(false)).unwrap();
        self.w.settle();
    }

    fn machines_log(&self) -> String {
        std::fs::read_to_string(self.w.home().join("machines.log")).unwrap_or_default()
    }
}

/// One event line as the machine's hook leaves it: `@@<written>-<pid> {event, harness, pane, tmux, input}`.
fn event(n: u32, event: &str, input: Value) -> String {
    format!("@@1700000000-{n} {}", json!({"event": event, "harness": "claude", "pane": "", "tmux": "", "input": input}))
}

const REMOTE_LOG: &str = "/home/v/.claude/projects/app/s1.jsonl";

fn input(sid: &str, more: Value) -> Value {
    let mut i = json!({"session_id": sid, "cwd": "/home/v/app", "transcript_path": REMOTE_LOG});
    i.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
    i
}

#[test]
fn events_from_a_machine_reach_cue_placed_on_that_machine_reading_a_copy_of_its_log() {
    let line = r#"{"type":"user","message":{"content":"fix the login"}}"#;
    let b = machine("ev-box", &[event(1, "prompt", input("s1", json!({"prompt": "fix the login"}))), event(2, "stop", input("s1", json!({"last_assistant_message": "Fixed."})))], false, &format!("{line}\n"));
    b.stream();
    let s = b.w.snapshot();
    assert_eq!(s.state("s1"), "waiting", "its turn ended: yours");
    let rec = b.w.hub().session_record("s1").unwrap();
    assert_eq!(rec.origin.machine, "ev-box");
    let copy = b.w.home().join("machines/ev-box/s1.jsonl");
    assert_eq!(rec.origin.transcript_path, copy.to_string_lossy(), "its copy on this Mac, never the machine's path");
    until("the copy to hold the machine's log", || std::fs::read_to_string(&copy).is_ok_and(|c| c.contains(line)));
    assert_eq!(std::fs::read_to_string(b.w.home().join("machines/ev-box/s1.jsonl.from")).unwrap(), REMOTE_LOG, "and where it came from");
}

#[test]
fn a_stream_that_drops_mid_line_hands_over_what_came_before_and_logs_the_rest() {
    let b = machine("cut-box", &[event(1, "prompt", input("s1", json!({"prompt": "go"}))), r#"@@1700000000-2 {"event":"stop","inp"#.into()], true, "");
    b.stream();
    assert_eq!(b.w.snapshot().state("s1"), "working", "the stop never came whole");
    assert!(b.machines_log().contains(r#"UNREADABLE @@1700000000-2 {"event":"stop","inp"#), "{}", b.machines_log());
}

#[test]
fn garbage_from_a_machine_is_logged_when_it_looks_like_an_event_and_the_events_after_it_still_come() {
    let lines = vec!["Welcome to Ubuntu".to_string(), "@@abc {}".into(), "@@1-1 not json".into(), "\u{1}\u{7f}binary".into(), event(3, "prompt", input("s1", json!({"prompt": "after"})))];
    let b = machine("junk-box", &lines, false, "");
    b.stream();
    assert_eq!(b.w.snapshot().state("s1"), "working", "the event after the junk");
    let log = b.machines_log();
    assert_eq!(log.matches("UNREADABLE").count(), 2, "the two lines that looked like events: {log}");
    assert!(!log.contains("Welcome"), "a login banner is no event: {log}");
}

#[test]
fn a_line_over_a_megabyte_is_skipped_and_the_stream_goes_on() {
    let huge = format!("@@1700000000-1 {}", "x".repeat(1_100_000));
    let b = machine("big-box", &[huge, event(2, "prompt", input("s1", json!({"prompt": "small"})))], false, "");
    b.stream();
    assert_eq!(b.w.snapshot().state("s1"), "working");
    assert!(b.machines_log().contains("(a line over the size limit)"));
}

#[test]
fn a_machine_that_cant_be_reached_ends_its_stream_at_once_with_nothing_handed_over() {
    let b = machine("gone-box", &[event(1, "prompt", input("s1", json!({"prompt": "never"})))], false, "");
    b.ssh.unplug("gone-box");
    let start = Instant::now();
    b.stream();
    assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
    assert!(b.w.snapshot().find("s1").is_none());
    assert_eq!(b.ssh.calls_with("gone-box", "cue-watch"), 1, "the watcher tried, once: the loop around it waits before the next");
}

#[test]
fn a_watcher_started_again_asks_the_machine_what_it_kept_about_it_once_more() {
    let b = machine("again-box", &["@@hb".to_string()], false, "");
    b.ssh.host("again-box", &format!("{PLAYS_STREAM}case \"$1\" in *'command -v tmux'*) echo /usr/bin/tmux ;; esac"));
    machines::tmux_path(&b.m.host).unwrap();
    machines::tmux_path(&b.m.host).unwrap();
    assert_eq!(b.ssh.calls_with("again-box", "command -v tmux"), 1);
    b.stream();
    machines::tmux_path(&b.m.host).unwrap();
    assert_eq!(b.ssh.calls_with("again-box", "command -v tmux"), 2, "reached again: it may have rebooted");
}

#[test]
fn events_left_on_the_machine_while_no_watcher_ran_come_once_each_across_restarts() {
    let b = machine("real-box", &[], false, "");
    // The machine runs Cue's real watcher this time.
    b.ssh.host("real-box", &format!("case \"$1\" in *cue-copy*) exit 0 ;; esac\n{RUNS_IT}"));
    let inbox = b.ssh.home("real-box").join(".cue/in");
    std::fs::create_dir_all(&inbox).unwrap();
    let drop_event = |n: u32, sid: &str| {
        let ev = json!({"event": "prompt", "harness": "claude", "pane": "", "tmux": "", "input": input(sid, json!({"prompt": "hi"}))});
        std::fs::write(inbox.join(format!("1700000000-{n}.json")), ev.to_string()).unwrap();
    };
    let watch = |until_sid: &'static str| {
        let (m, stop) = (b.m.clone(), std::sync::Arc::new(AtomicBool::new(false)));
        let s = stop.clone();
        let t = std::thread::spawn(move || hooks::stream(&m, &s));
        until(&format!("{until_sid} to reach Cue"), || b.w.hub().session_record(until_sid).is_some());
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        t.join().unwrap().unwrap();
    };
    drop_event(1, "r1");
    watch("r1");
    drop_event(2, "r2");
    watch("r2");
    b.w.settle();
    let log = b.machines_log();
    assert_eq!(log.matches(" r1 ").count(), 1, "r1 handed over once, not again by the second watcher: {log}");
    assert_eq!(log.matches(" r2 ").count(), 1, "{log}");
    assert_eq!(std::fs::read_dir(&inbox).unwrap().count(), 0, "nothing left waiting there");
}

/// A permission asked on the machine, its card answered `allow` in Cue: the answer file Cue writes there, and
/// whether the machine's hook takes it (`taken`: the test removes it, as the hook does).
fn permission_answered(host: &str, taken: bool) -> (Remote, Option<Value>) {
    let ask = event(7, "permission", input("p1", json!({"tool_name": "Bash", "tool_input": {"command": "make"}})));
    let b = machine(host, &[ask], false, "");
    // Where the machine's hook waits for answers: its watcher made it.
    std::fs::create_dir_all(b.ssh.home(host).join(".cue/out")).unwrap();
    b.stream();
    until("the machine's question to show as a card", || {
        b.w.settle();
        !b.w.snapshot().cards_of("p1").is_empty()
    });
    let id = b.w.snapshot().card("p1")["id"].as_str().unwrap().to_string();
    b.w.answer(&id, json!({"behavior": "allow"}));
    let out = b.ssh.home(host).join(".cue/out/1700000000-7.json");
    if !taken {
        return (b, None);
    }
    until("Cue's answer to land on the machine", || std::fs::read_to_string(&out).is_ok_and(|a| !a.is_empty()));
    let answer: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    std::fs::remove_file(&out).unwrap();
    (b, Some(answer))
}

#[test]
fn a_permission_asked_on_a_machine_is_answered_there_as_the_agents_hook_reads_it() {
    let (b, answer) = permission_answered("ask-box", true);
    assert_eq!(answer.unwrap(), json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}}));
    assert_eq!(b.w.hub().session_record("p1").unwrap().origin.machine, "ask-box");
}

#[test]
fn an_answer_the_machines_hook_never_took_is_taken_back_and_said_lost() {
    let (b, _) = permission_answered("lost-box", false);
    let out = b.ssh.home("lost-box").join(".cue/out/1700000000-7.json");
    until("Cue to say the answer didn't reach it", || {
        b.w.settle();
        b.w.snapshot().cards_of("p1").iter().any(|c| c["kind"] == "terminal")
    });
    assert!(!out.exists(), "the answer nobody took is taken back off the machine");
}

#[test]
fn a_sessions_log_is_never_read_at_the_machines_path_on_this_mac() {
    let w_dir = std::env::temp_dir().join(format!("cue-automount-{}", std::process::id()));
    std::fs::create_dir_all(&w_dir).unwrap();
    // The machine's log path, as it would sit under an automounted /home on this Mac that doesn't answer.
    let stalled = Stall::at(&w_dir.join("s1.jsonl"));
    let remote = stalled.path().to_string_lossy().into_owned();
    let lines = [event(1, "prompt", json!({"session_id": "s1", "cwd": "/home/v/app", "transcript_path": remote, "prompt": "go"})), event(2, "stop", json!({"session_id": "s1", "cwd": "/home/v/app", "transcript_path": remote, "last_assistant_message": "Done."}))];
    let b = machine("mount-box", &lines, false, "{}\n");
    let m = b.m.clone();
    let streamed = std::thread::spawn(move || hooks::stream(&m, &AtomicBool::new(false)));
    until("the stream to hand both events over", || streamed.is_finished());
    b.w.settle();
    // Built on a thread of its own: had it read the machine's path, it would wait for good.
    let c = b.w.client();
    let built = std::thread::spawn(move || c.ask(json!({"type": "list"})));
    until("the window's state, without reading the machine's path", || built.is_finished());
    assert_eq!(Snap(built.join().unwrap()).state("s1"), "waiting");
    assert_ne!(b.w.hub().session_record("s1").unwrap().origin.transcript_path, remote);
    // Nothing ever opened the machine's path: the pipe was never read, so it's left unanswered.
    let _ = std::fs::remove_dir_all(&w_dir);
}
