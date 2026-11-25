//! The session record's tests: its state changes; the rest are by area, beside this file.

use super::*;

mod delivery;
mod names;
mod thread;

fn named(sessions: &mut Sessions, sid: &str, harness: &str, machine: &str) {
    sessions.mark(&Origin { session_id: sid.into(), harness: harness.into(), cwd: "/x/cue".into(), machine: machine.into(), ..Default::default() }, "waiting", None);
}

fn listed(pairs: &[(&str, &str)]) -> HashMap<String, crate::sessions::names::Listed> {
    in_process(1, pairs)
}

/// Claude Code's list with every session in process `pid`, under names given to it.
fn in_process(pid: i32, pairs: &[(&str, &str)]) -> HashMap<String, crate::sessions::names::Listed> {
    pairs.iter().map(|(a, b)| (a.to_string(), crate::sessions::names::Listed { name: b.to_string(), pid, made_up: false })).collect()
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
fn a_failed_background_task_is_told_once_even_when_its_transcript_is_read_again() {
    let mut sessions = Sessions::default();
    sessions.mark(&origin("s1"), "working", None);
    assert!(sessions.set_task_failed("s1", 100, "tests", "exit 1"));
    assert!(!sessions.set_task_failed("s1", 100, "tests", "exit 1"));
    // Kept while it waits; its next turn starts (the line goes), then Cue reads the transcript again from the top (a
    // restart): the same failure, and an older one before it, are no news.
    sessions.mark(&origin("s1"), "waiting", None);
    assert_eq!(get(&sessions, "s1").task_failed_ms, 100);
    sessions.mark(&origin("s1"), "working", None);
    assert_eq!(get(&sessions, "s1").task_failed_ms, 0);
    assert!(!sessions.set_task_failed("s1", 50, "build", "exit 2"));
    assert!(!sessions.set_task_failed("s1", 100, "tests", "exit 1"));
    // A new one is.
    assert!(sessions.set_task_failed("s1", 200, "tests", "exit 1"));
}
