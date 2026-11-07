//! Tests: catch-ups of one feed at once (its log read with no lock held, put in under it): no line is
//! taken twice or lost, whichever read goes in first, and a slow read holds up no one else.

use super::super::reading::{slow_disk, Applied};
use super::*;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A log of `n` turns, each a prompt and a reply that says "reply <i>".
fn turn(i: usize) -> String {
    format!(
        "{}\n{}\n",
        json!({"type": "user", "timestamp": "2026-10-04T10:00:00.000Z", "message": {"content": format!("prompt {i}")}}),
        json!({"type": "assistant", "timestamp": "2026-10-04T10:00:01.000Z", "message": {"content": [{"type": "text", "text": format!("reply {i}")}]}})
    )
}

fn log_at(name: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("cue-steps-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.jsonl");
    std::fs::write(&path, "").unwrap();
    let p = path.to_string_lossy().into_owned();
    (dir, p)
}

fn append(path: &str, text: &str) {
    std::fs::OpenOptions::new().append(true).open(path).unwrap().write_all(text.as_bytes()).unwrap();
}

/// What a feed says, turn by turn ("reply 0", "reply 1", …).
fn replies(f: &Feed) -> Vec<String> {
    f.turns.iter().flat_map(|t| t.items.iter()).filter_map(|i| match i { Item::Say { text, .. } => Some(text.clone()), _ => None }).collect()
}

#[test]
fn two_reads_planned_at_once_put_their_lines_in_once() {
    let (dir, p) = log_at("twice");
    append(&p, &turn(0));
    let mut f = Feed::open(&p, 1);
    let (a, b) = (f.plan(), f.plan());
    let (a, b) = (a.fetch(), b.fetch());
    assert_eq!(f.apply(a), Applied::Done);
    assert_eq!(f.apply(b), Applied::Stale, "the other read already took these lines");
    assert_eq!(replies(&f), ["reply 0"]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_dropped_read_loses_nothing_the_next_catch_up_reads_on() {
    let (dir, p) = log_at("lose");
    append(&p, &turn(0));
    let mut f = Feed::open(&p, 1);
    let (early, late) = (f.plan(), f.plan());
    let early = early.fetch();
    // The log grows between the two reads: the later one saw more, but the earlier one goes in first.
    append(&p, &turn(1));
    let late = late.fetch();
    assert_eq!(f.apply(early), Applied::Done);
    assert_eq!(f.apply(late), Applied::Stale);
    assert_eq!(replies(&f), ["reply 0"]);
    f.catch_up();
    assert_eq!(replies(&f), ["reply 0", "reply 1"], "read on from where the read that went in left off");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_read_planned_before_the_log_was_replaced_is_dropped() {
    let (dir, p) = log_at("replaced");
    append(&p, &(turn(0) + &turn(1)));
    let mut f = Feed::open(&p, 1);
    f.catch_up();
    let stale = f.plan();
    std::fs::write(&p, turn(2)).unwrap();
    f.catch_up();
    assert_eq!(replies(&f), ["reply 2"], "started over on the new file");
    assert_eq!(f.apply(stale.fetch()), Applied::Stale);
    assert_eq!(replies(&f), ["reply 2"]);
    std::fs::remove_dir_all(&dir).ok();
}

/// Many askers at once while the log grows: every line is in exactly once, in order.
#[test]
fn askers_at_once_while_the_log_grows_take_each_line_once() {
    let (dir, p) = log_at("many");
    let feed = Arc::new(Mutex::new(Feed::open(&p, 1)));
    let writer = {
        let p = p.clone();
        std::thread::spawn(move || {
            for i in 0..200 {
                append(&p, &turn(i));
            }
        })
    };
    let askers: Vec<_> = (0..8).map(|_| {
        let feed = feed.clone();
        std::thread::spawn(move || {
            for _ in 0..100 {
                super::super::queries::catch_up_shared(&feed);
            }
        })
    }).collect();
    writer.join().unwrap();
    askers.into_iter().for_each(|a| a.join().unwrap());
    super::super::queries::catch_up_shared(&feed);
    let f = feed.lock().unwrap();
    let want: Vec<String> = (0..200).map(|i| format!("reply {i}")).collect::<Vec<_>>().split_off(200 - 30);
    assert_eq!(replies(&f), want, "the last 30 turns, each once");
    std::fs::remove_dir_all(&dir).ok();
}

/// The read waiting on a slow disk doesn't hold the feed: another asker of the same session is answered
/// (from its own read) meanwhile.
#[test]
fn a_slow_read_holds_up_no_one_else_asking_about_the_session() {
    let (dir, p) = log_at("slow");
    append(&p, &turn(0));
    assert_eq!(last_say(&p).as_deref(), Some("reply 0"));
    append(&p, &turn(1));
    let (release, started) = slow_disk::hold_next(&p);
    let slow = { let p = p.clone(); std::thread::spawn(move || last_say(&p)) };
    started.recv_timeout(Duration::from_secs(5)).expect("the slow read reached the disk");
    let (tx, rx) = std::sync::mpsc::channel();
    { let p = p.clone(); std::thread::spawn(move || { let _ = tx.send(context_pct(&p).is_none() && last_say(&p).is_some()); }); }
    let answered = rx.recv_timeout(Duration::from_secs(2));
    release.send(()).unwrap();
    assert_eq!(answered, Ok(true), "the other asker waited on the slow read");
    assert_eq!(slow.join().unwrap().as_deref(), Some("reply 1"));
    std::fs::remove_dir_all(&dir).ok();
}

/// One session's history slow to read (Show earlier) holds up no other session's.
#[test]
fn a_slow_history_read_holds_up_no_other_sessions_history() {
    let (dir_a, a) = log_at("history-a");
    let (dir_b, b) = log_at("history-b");
    append(&a, &turn(0));
    append(&b, &turn(0));
    let (release, started) = slow_disk::hold_next(&a);
    let slow = { let a = a.clone(); std::thread::spawn(move || history(&a)) };
    started.recv_timeout(Duration::from_secs(5)).expect("the slow read reached the disk");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(history(&b)); });
    let answered = rx.recv_timeout(Duration::from_secs(2));
    release.send(()).unwrap();
    assert_eq!(answered.ok(), Some(json!([])), "the other session's history waited on the slow read");
    slow.join().unwrap();
    std::fs::remove_dir_all(&dir_a).ok();
    std::fs::remove_dir_all(&dir_b).ok();
}

/// A read dropped because another went in first is tried again: the one that went in read before the log
/// grew, and the dropped one saw more, so its asker answers with the newest lines, not a poll late.
#[test]
fn a_dropped_read_is_tried_again_so_its_asker_isnt_a_poll_late() {
    let (dir, p) = log_at("retry");
    append(&p, &turn(0));
    assert_eq!(last_say(&p).as_deref(), Some("reply 0"));
    // This asker's read is planned, then waits on the disk while another asker's read goes in first.
    let (release, started) = slow_disk::hold_next(&p);
    let late = { let p = p.clone(); std::thread::spawn(move || last_say(&p)) };
    started.recv_timeout(Duration::from_secs(5)).expect("the held read reached the disk");
    assert_eq!(last_say(&p).as_deref(), Some("reply 0"), "the other asker's read goes in first");
    // The log grows, then the held read goes on: it sees reply 1, but its read is dropped.
    append(&p, &turn(1));
    release.send(()).unwrap();
    assert_eq!(late.join().unwrap().as_deref(), Some("reply 1"));
    std::fs::remove_dir_all(&dir).ok();
}
