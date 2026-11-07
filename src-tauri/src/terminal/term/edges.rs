//! The built-in terminal's edges (stability plan §4, Terminal): sizes past the bounds, a terminal that isn't
//! open, an opening that fails with an open waiting on it, a client that ends at once (as ssh does with 255),
//! keystrokes from many terminals at once, a writer that blocks until Resume, and the pane lookup through a
//! stand-in tmux (terminal::fake_tmux) that hangs, isn't installed, or answers oddly.

use super::support::*;
use super::*;
use crate::terminal::fake_tmux::FakeTmux;
use crate::terminal::input::tests::Stalled;
use std::sync::{Arc, Mutex as M};

fn sid(name: &str) -> String {
    format!("{name}-{}", std::process::id())
}

/// Its pty's size as (cols, rows).
fn size(sid: &str) -> (u16, u16) {
    let master = crate::locking::lock(open_terms(), "open_terms").open.get(sid).map(|o| o.master.clone()).expect("open");
    let s = crate::locking::lock(&master, "pty master").get_size().unwrap();
    (s.cols, s.rows)
}

/// A collector for a terminal's events: what it printed, and whether it ended (with which gen).
#[derive(Clone, Default)]
struct Seen(Arc<M<(String, Option<u64>)>>);

impl Seen {
    fn out(&self) -> impl Fn(serde_json::Value) + Send + 'static {
        let me = self.clone();
        move |v| {
            let mut s = crate::locking::lock(&me.0, "seen");
            if let Some(d) = v["data"].as_str() {
                s.0.push_str(&String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(d).unwrap()));
            }
            if v["end"] == true {
                s.1 = v["gen"].as_u64();
            }
        }
    }
    fn text(&self) -> String {
        crate::locking::lock(&self.0, "seen").0.clone()
    }
    fn ended(&self) -> Option<u64> {
        crate::locking::lock(&self.0, "seen").1
    }
}

fn sh(script: &str) -> CommandBuilder {
    let mut c = CommandBuilder::new("/bin/sh");
    c.args(["-c", script]);
    c
}

/// A terminal whose input lands in `got` (a client that reads everything at once).
fn reading(sid: &str, got: &Arc<M<Vec<u8>>>) -> u32 {
    let (go, release) = std::sync::mpsc::channel();
    drop(go);
    record(sid, sleeper(), Some(Box::new(Stalled { release, got: got.clone() })))
}

#[test]
fn a_size_below_the_smallest_is_kept_at_ten_by_two_and_the_largest_is_taken() {
    let t = sid("sizes");
    record(&t, sleeper(), None);
    resize(&t, 0, 0).unwrap();
    assert_eq!(size(&t), (10, 2));
    resize(&t, u16::MAX, u16::MAX).unwrap();
    assert_eq!(size(&t), (u16::MAX, u16::MAX));
    close(&t);
}

#[test]
fn a_terminal_that_isnt_open_refuses_typing_resize_and_resume_saying_so() {
    let t = sid("never");
    for r in [type_in(&t, "x"), resize(&t, 80, 24), resume(&t)] {
        assert_eq!(r, Err("its terminal isn't open".into()));
    }
}

#[test]
fn once_closed_nothing_reaches_it_and_closing_again_is_harmless() {
    let t = sid("closed");
    let pid = record(&t, sleeper(), None);
    close(&t);
    assert!(until(|| !alive(pid)), "its client ended");
    assert!(type_in(&t, "x").is_err() && resize(&t, 80, 24).is_err());
    close(&t);
    close_if(&t, 12345);
}

#[test]
fn a_close_naming_an_opening_that_never_was_leaves_the_open_one() {
    let t = sid("othergen");
    let starts = Arc::new(M::new(vec![]));
    let g = once(&t, 20, 4, || fake(&starts, 0)).unwrap();
    close_if(&t, g + 1_000);
    assert_eq!(foreground(&t).len(), 1, "still open");
    close(&t);
}

#[test]
fn an_open_that_joined_one_in_flight_gets_its_own_size() {
    let t = sid("joinsize");
    let starts = Arc::new(M::new(vec![]));
    let (a, s1) = (t.clone(), starts.clone());
    let (started_tx, started) = std::sync::mpsc::channel();
    let (go, wait) = std::sync::mpsc::channel::<()>();
    let first = std::thread::spawn(move || {
        once(&a, 20, 4, || {
            started_tx.send(()).unwrap();
            let _ = wait.recv();
            fake(&s1, 0)
        })
    });
    started.recv().unwrap();
    let b = t.clone();
    let joiner = std::thread::spawn(move || once(&b, 50, 7, || panic!("joined: never launches its own")));
    assert!(until(|| crate::locking::lock(open_terms(), "open_terms").opening.get(&t).is_some_and(|p| !crate::locking::lock(&p.0, "term opening").waiting.is_empty())));
    go.send(()).unwrap();
    assert_eq!(first.join().unwrap(), joiner.join().unwrap());
    assert_eq!(size(&t), (50, 7), "the later open's size, not the first's");
    assert_eq!(crate::locking::lock(&starts, "starts").len(), 1);
    close(&t);
}

#[test]
fn an_opening_that_fails_tells_the_open_waiting_on_it_and_the_next_open_starts_afresh() {
    let t = sid("fails");
    let (started_tx, started) = std::sync::mpsc::channel();
    let (go, wait) = std::sync::mpsc::channel::<()>();
    let a = t.clone();
    let first = std::thread::spawn(move || {
        once(&a, 20, 4, || {
            started_tx.send(()).unwrap();
            let _ = wait.recv();
            Err("tmux pane %5 is gone".into())
        })
    });
    started.recv().unwrap();
    let b = t.clone();
    let joiner = std::thread::spawn(move || once(&b, 20, 4, || panic!("joined: never launches its own")));
    assert!(until(|| crate::locking::lock(open_terms(), "open_terms").opening.get(&t).is_some_and(|p| !crate::locking::lock(&p.0, "term opening").waiting.is_empty())));
    go.send(()).unwrap();
    assert_eq!(first.join().unwrap(), Err("tmux pane %5 is gone".into()));
    assert_eq!(joiner.join().unwrap(), Err("tmux pane %5 is gone".into()), "the same answer");
    let starts = Arc::new(M::new(vec![]));
    assert!(once(&t, 20, 4, || fake(&starts, 0)).is_ok(), "nothing left in flight");
    close(&t);
}

#[test]
fn a_client_that_ends_at_once_as_ssh_does_with_255_ends_the_terminal_and_the_window_hears_it() {
    let t = sid("exit255");
    let seen = Seen::default();
    let gen = spawn(&t, sh("printf 'ssh: connect to host box port 22: Connection refused'; exit 255"), 80, 12, seen.out()).unwrap();
    assert!(until(|| seen.ended().is_some()), "the window hears it ended");
    assert_eq!(seen.ended(), Some(gen), "under its own opening's gen");
    assert!(seen.text().contains("Connection refused"), "what it said is shown: {:?}", seen.text());
    let t2 = t.clone();
    assert!(within(crate::terminal::input::BOUND.as_millis() as u64 + 1_000, move || type_in(&t2, "x")).is_some(), "typing into it never hangs");
    close(&t);
}

#[test]
fn what_a_client_prints_reaches_the_window_in_order() {
    let t = sid("prints");
    let seen = Seen::default();
    spawn(&t, sh("for i in 1 2 3 4 5 6 7 8 9; do printf \"<$i>\"; done"), 80, 12, seen.out()).unwrap();
    assert!(until(|| seen.ended().is_some()));
    assert_eq!(seen.text(), "<1><2><3><4><5><6><7><8><9>");
    close(&t);
}

#[test]
fn keystrokes_into_many_terminals_at_once_each_land_whole_and_in_order() {
    let names: Vec<String> = (0..6).map(|i| sid(&format!("load{i}"))).collect();
    let gots: Vec<_> = names.iter().map(|n| {
        let got = Arc::new(M::new(vec![]));
        reading(n, &got);
        got
    }).collect();
    let typing: Vec<_> = names.iter().cloned().map(|n| std::thread::spawn(move || {
        for k in 0..200 {
            type_in(&n, &format!("<{k}>")).unwrap();
        }
    })).collect();
    for t in typing {
        t.join().unwrap();
    }
    let want: String = (0..200).map(|k| format!("<{k}>")).collect();
    for (n, got) in names.iter().zip(&gots) {
        assert_eq!(String::from_utf8(crate::locking::lock(got, "got").clone()).unwrap(), want, "{n}");
        close(n);
    }
}

#[test]
fn two_writers_into_one_terminal_never_split_each_others_writes() {
    let t = sid("interleave");
    let got = Arc::new(M::new(vec![]));
    reading(&t, &got);
    let writers: Vec<_> = ["A", "B"].into_iter().map(|c| {
        let t = t.clone();
        std::thread::spawn(move || {
            for _ in 0..50 {
                type_in(&t, &c.repeat(100)).unwrap();
            }
        })
    }).collect();
    for w in writers {
        w.join().unwrap();
    }
    let all = String::from_utf8(crate::locking::lock(&got, "got").clone()).unwrap();
    assert_eq!(all.len(), 100 * 100);
    assert!(all.as_bytes().chunks(100).all(|c| c.iter().all(|b| *b == c[0])), "each write whole");
    close(&t);
}

#[test]
fn a_terminal_whose_writer_blocks_is_latched_until_resume_then_takes_typing_again() {
    let t = sid("resume");
    let (go, release) = std::sync::mpsc::channel();
    let got = Arc::new(M::new(vec![]));
    record(&t, sleeper(), Some(Box::new(Stalled { release, got: got.clone() })));
    let err = type_in(&t, "stuck").unwrap_err();
    assert!(err.contains("isn't responding"), "{err}");
    assert!(type_in(&t, "more").is_err(), "latched: refused while it's stuck");
    drop(go); // the client reads again
    assert!(type_in(&t, "still latched").is_err(), "not until you resume it");
    resume(&t).unwrap();
    type_in(&t, "ok").unwrap();
    assert_eq!(String::from_utf8(crate::locking::lock(&got, "got").clone()).unwrap(), "stuckok", "nothing typed while latched goes in");
    close(&t);
}

#[test]
fn the_pane_is_found_through_its_session_group_and_its_window_follows_the_latest_client() {
    let tmux = FakeTmux::answering("display-message) echo 'work|stuff|@3' ;;");
    let (chain, view) = pane_chain("%5", None, 7).unwrap();
    let at = |a: &str, b: &str| chain.windows(2).any(|w| w[0] == a && w[1] == b);
    assert!(at("-t", "work|stuff"), "a session name may hold a |: split at the last: {chain:?}");
    assert!(at("-t", &format!("{view}:@3")) && at("-t", "%5") && at("-s", &view), "{chain:?}");
    assert!(view.ends_with("-7"), "{view}");
    assert_eq!(tmux.calls_of("set-option"), ["set-option -w -t %5 window-size latest"]);
}

#[test]
fn a_pane_tmux_says_nothing_about_is_gone() {
    let _tmux = FakeTmux::answering("display-message) exit 1 ;;");
    assert_eq!(pane_chain("%5", None, 1).unwrap_err(), "tmux pane %5 is gone");
    let _odd = FakeTmux::answering("display-message) echo 'no-window-here' ;;");
    assert_eq!(pane_chain("%5", None, 1).unwrap_err(), "tmux pane %5 is gone");
}

#[test]
fn with_no_tmux_installed_opening_fails_and_leaves_nothing_in_flight() {
    let t = sid("notmux");
    let _none = FakeTmux::missing();
    assert!(open_with(&t, "%5", None, 80, 12, |_| {}).is_err());
    assert!(attach_local(&t, "cue-shell-1", 80, 12, |_| {}).is_err());
    let left = crate::locking::lock(open_terms(), "open_terms").opening.contains_key(&t);
    assert!(!left, "no opening left behind to hold the next one");
}

#[test]
fn a_tmux_that_never_answers_the_pane_lookup_fails_the_opening_in_time() {
    let t = sid("hungtmux");
    let t2 = t.clone();
    let r = within(15_000, move || {
        let _tmux = FakeTmux::answering("display-message) exec sleep 60 ;;");
        open_with(&t2, "%5", None, 80, 12, |_| {})
    });
    assert!(matches!(r, Some(Err(_))), "an error within its cap, not an opening that never answers: {r:?}");
    assert!(!crate::locking::lock(open_terms(), "open_terms").opening.contains_key(&t));
}

#[test]
fn a_pane_whose_client_exits_at_once_opens_and_then_ends() {
    let t = sid("client255");
    let _tmux = FakeTmux::answering("display-message) echo 'work|@3' ;;\n  new-session) echo 'lost connection'; exit 255 ;;");
    let seen = Seen::default();
    let gen = open_with(&t, "%5", None, 80, 12, seen.out()).unwrap();
    assert!(until(|| seen.ended() == Some(gen)), "the window hears it ended: {:?}", seen.text());
    close(&t);
}
