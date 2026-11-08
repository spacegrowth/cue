//! The session record's tests: its state changes; the rest are by area, beside this file.

use super::*;

mod delivery;
mod names;
mod thread;

fn named(sessions: &mut Sessions, sid: &str, harness: &str, machine: &str) {
    sessions.mark(&Origin { session_id: sid.into(), harness: harness.into(), cwd: "/x/cue".into(), machine: machine.into(), ..Default::default() }, "waiting", None);
}

fn listed(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

fn get<'a>(sessions: &'a Sessions, sid: &str) -> &'a Session {
    sessions.0.get(sid).unwrap()
}

fn long_text() -> String {
    (0..1600).map(|n| format!("word{n} ")).collect::<String>() + "\nthe end"   // ~10k characters
}

fn origin(sid: &str) -> Origin {
    Origin { session_id: sid.into(), cwd: "/x/proj".into(), harness: "claude".into(), ..Default::default() }
}

#[test]
fn state_changes_open_and_close_segments() {
    let mut s = Sessions::default();
    let o = origin("a");
    s.mark(&o, "working", Some("fix the bug"));
    s.mark(&o, "working", None); // same kind: no new segment
    s.mark(&o, "waiting", None);
    s.mark(&o, "deciding", None); // waiting and deciding share the striped kind
    let snap = s.snapshot();
    assert_eq!(snap.len(), 1);
    let a = &snap[0];
    assert_eq!(a.state, "deciding");
    assert_eq!(a.prompt, "fix the bug");
    assert_eq!(a.project, "proj");
    let kinds: Vec<&str> = a.segments.iter().map(|g| g.kind.as_str()).collect();
    assert_eq!(kinds, vec!["working", "waiting"]);
    assert!(a.segments[0].end_ms.is_some() && a.segments[1].end_ms.is_none());
}

#[test]
fn sessions_without_an_id_are_ignored_and_remove_works() {
    let mut s = Sessions::default();
    s.mark(&origin(""), "working", None);
    assert!(s.snapshot().is_empty());
    s.mark(&origin("b"), "waiting", None);
    assert!(s.remove("b"));
    assert!(s.snapshot().is_empty());
}

#[test]
fn a_hook_from_the_session_means_its_past_the_trust_question() {
    let mut s = Sessions::default();
    let started = Origin { session_id: "a".into(), cwd: "/tmp".into(), ..Default::default() };
    s.mark(&started, "waiting", None);
    s.set_trust("a", 5);
    // Cue's own bookkeeping (no transcript) leaves it asking.
    s.mark(&started, "waiting", None);
    assert_eq!(s.asking_trust().len(), 1);
    let hook = Origin { session_id: "a".into(), cwd: "/private/tmp".into(), transcript_path: "/t/a.jsonl".into(), ..Default::default() };
    s.mark(&hook, "working", Some("hi"));
    assert!(s.asking_trust().is_empty());
}

#[test]
fn a_failed_background_task_is_told_once_even_when_its_transcript_is_read_again() {
    let mut sessions = Sessions::default();
    sessions.mark(&origin("s1"), "working", None);
    assert!(sessions.set_task_failed("s1", 100, "tests", "exit 1"));
    assert!(!sessions.set_task_failed("s1", 100, "tests", "exit 1"));
    // Its state changes (the line goes), then Cue reads the transcript again from the top (a restart): the same
    // failure, and an older one before it, are no news.
    sessions.mark(&origin("s1"), "waiting", None);
    assert_eq!(get(&sessions, "s1").task_failed_ms, 0);
    assert!(!sessions.set_task_failed("s1", 50, "build", "exit 2"));
    assert!(!sessions.set_task_failed("s1", 100, "tests", "exit 1"));
    // A new one is.
    assert!(sessions.set_task_failed("s1", 200, "tests", "exit 1"));
}
