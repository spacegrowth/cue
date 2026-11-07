//! About (stability plan §4, Tasks / About; tasks.rs): following a session reads its chat once, each turn's
//! end reads only what was said since, stopping forgets it, and a small model that's slow, wrong or failing
//! holds nothing else up and loses nothing kept. The small model is a stand-in (tasks::small), never Claude Code.

use crate::btw::{Answer, Spent};
use crate::tasks::small;
use crate::testing::World;
use serde_json::{json, Value};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A World with About on, its tracks read from its own folder, and a small model that answers each ask with
/// the next of `answers` (the prompts it was given kept in the returned list).
fn about_on(answers: Vec<Result<Value, String>>) -> (World, Arc<Mutex<Vec<String>>>) {
    let w = World::new();
    std::fs::write(w.home().join("config.json"), r#"{"tasks": {"on": true}}"#).unwrap();
    crate::config::refresh();
    small::reset();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let answers = Mutex::new(answers.into_iter());
    let log = asked.clone();
    small::stand_in(move |prompt, _| {
        log.lock().unwrap().push(prompt.to_string());
        let next = answers.lock().unwrap().next().unwrap_or_else(|| Err("asked once too often".into()));
        next.map(|v| Answer { text: v.to_string(), spent: Spent { tokens: 100, usd: 0.01 } })
    });
    (w, asked)
}

impl Drop for Reset {
    fn drop(&mut self) {
        small::reset();
    }
}
struct Reset;

/// Session `sid`'s About as the window gets it.
fn track(sid: &str) -> Value {
    crate::tasks::view()[sid].clone()
}

/// Waits (two seconds at most) until session `sid`'s About isn't busy reading.
fn read_done(sid: &str) -> Value {
    let until = Instant::now() + Duration::from_secs(2);
    while track(sid)["busy"] != "" {
        assert!(Instant::now() < until, "still reading: {}", track(sid));
        std::thread::yield_now();
    }
    track(sid)
}

#[test]
fn following_a_session_reads_its_chat_once_and_keeps_what_it_said() {
    let _r = Reset;
    let (w, asked) = about_on(vec![Ok(json!({"about": "Login bug", "now": "Fixed, tests pass", "summary": "Fixed the login."}))]);
    w.session("a").prompt("fix the login bug").stop("Fixed it; tests pass.");
    crate::tasks::start(w.hub().clone(), "a").unwrap();
    let t = read_done("a");
    assert_eq!((t["about"].as_str(), t["now"].as_str(), t["summary"].as_str()), (Some("Login bug"), Some("Fixed, tests pass"), Some("Fixed the login.")));
    assert_eq!(t["spent"]["tokens"], 100);
    assert_eq!(crate::tasks::spent()["calls"], 1);
    let prompts = asked.lock().unwrap().clone();
    assert!(prompts.len() == 1 && prompts[0].contains("fix the login bug") && prompts[0].contains("Fixed it; tests pass."), "{prompts:?}");
    assert!(w.home().join("tasks/a.json").exists(), "kept in Cue's folder");
}

/// After a turn, only what was said since the last read goes to the small model; its line for the turn is
/// kept at the reply's time.
#[test]
fn a_turns_end_reads_only_what_was_said_since() {
    let _r = Reset;
    let (w, asked) = about_on(vec![Ok(json!({"about": "Login", "now": "Looking"})), Ok(json!({"turn": "Wrote the test", "now": "Testing"}))]);
    let a = w.session("a");
    a.prompt("fix the login bug").stop("Looking into it.");
    crate::tasks::start(w.hub().clone(), "a").unwrap();
    read_done("a");
    a.prompt("write a test first").stop("Wrote a failing test.");
    crate::tasks::turn_ended(w.hub().clone(), "a");
    let t = read_done("a");
    let second = asked.lock().unwrap()[1].clone();
    assert!(second.contains("write a test first") && !second.contains("fix the login bug"), "only since: {second}");
    assert_eq!((t["about"].as_str(), t["now"].as_str()), (Some("Login"), Some("Testing")), "what it left out stays");
    assert_eq!(t["turns"].as_array().map(|l| l.iter().map(|x| x["line"].clone()).collect::<Vec<_>>()), Some(vec![json!("Wrote the test")]));
}

/// A turn that ends while the last one is still being read asks nothing; the turn after reads from where the
/// last read left off, so nothing said meanwhile is missed.
#[test]
fn a_turn_ending_mid_read_is_read_with_the_next() {
    let _r = Reset;
    let (w, asked) = about_on(vec![Ok(json!({"about": "Login"})), Ok(json!({"turn": "Both"}))]);
    let a = w.session("a");
    a.prompt("one").stop("First.");
    let (gate_tx, gate) = mpsc::channel::<()>();
    let gate = Mutex::new(gate);
    let inner = asked.clone();
    small::stand_in(move |prompt, _| {
        inner.lock().unwrap().push(prompt.to_string());
        let n = inner.lock().unwrap().len();
        if n == 1 {
            let _ = gate.lock().unwrap().recv();
        }
        Ok(Answer { text: json!({"about": "Login", "turn": "Both"}).to_string(), spent: Spent::default() })
    });
    crate::tasks::start(w.hub().clone(), "a").unwrap();
    a.prompt("two").stop("Second.");
    crate::tasks::turn_ended(w.hub().clone(), "a");
    assert_eq!(track("a")["busy"], "listing", "the first read still out, no update started beside it");
    gate_tx.send(()).unwrap();
    read_done("a");
    a.prompt("three").stop("Third.");
    crate::tasks::turn_ended(w.hub().clone(), "a");
    read_done("a");
    let last = asked.lock().unwrap().last().cloned().unwrap();
    assert!(last.contains("Second.") && last.contains("Third."), "{last}");
}

/// A small model that takes its time holds nothing up: following returns at once, and the window builds
/// meanwhile, the About saying it's reading.
#[test]
fn a_slow_small_model_holds_nothing_up() {
    let _r = Reset;
    let (w, _) = about_on(vec![]);
    w.session("a").prompt("fix it").stop("Fixed.");
    let (go_tx, go) = mpsc::channel::<()>();
    let go = Mutex::new(go);
    small::stand_in(move |_, _| {
        let _ = go.lock().unwrap().recv();
        Ok(Answer { text: json!({"about": "Fix"}).to_string(), spent: Spent::default() })
    });
    let t0 = Instant::now();
    crate::tasks::start(w.hub().clone(), "a").unwrap();
    let s = w.snapshot();
    assert!(t0.elapsed() < Duration::from_millis(500), "waited {:?}", t0.elapsed());
    assert_eq!(s.0["tasks"]["a"]["busy"], "listing");
    assert_eq!(crate::tasks::start(w.hub().clone(), "a"), Err("it's already reading the chat".into()), "one read at a time");
    go_tx.send(()).unwrap();
    assert_eq!(read_done("a")["about"], "Fix");
}

/// An answer that isn't what was asked, or no answer at all: said on the About, and what it had kept stays.
#[test]
fn a_wrong_or_failed_answer_is_said_and_keeps_what_was_there() {
    let _r = Reset;
    let (w, _) = about_on(vec![Ok(json!({"about": "Login", "now": "Fixed"})), Ok(json!("sorry, I can't")), Err("claude: not logged in".into())]);
    let a = w.session("a");
    a.prompt("fix").stop("Fixed.");
    crate::tasks::start(w.hub().clone(), "a").unwrap();
    read_done("a");
    for _ in 0..2 {
        a.prompt("more").stop("More.");
        crate::tasks::turn_ended(w.hub().clone(), "a");
        let t = read_done("a");
        assert!(!t["err"].as_str().unwrap_or("").is_empty(), "the failure is said: {t}");
        assert_eq!((t["about"].as_str(), t["now"].as_str()), (Some("Login"), Some("Fixed")));
    }
}

/// Stopping forgets the session (its file too), and its turns' ends ask nothing after.
#[test]
fn stopping_forgets_a_session_and_its_turns_ask_nothing() {
    let _r = Reset;
    let (w, asked) = about_on(vec![Ok(json!({"about": "Login"}))]);
    let a = w.session("a");
    a.prompt("fix").stop("Fixed.");
    crate::tasks::start(w.hub().clone(), "a").unwrap();
    read_done("a");
    crate::tasks::stop("a");
    a.prompt("more").stop("More.");
    crate::tasks::turn_ended(w.hub().clone(), "a");
    assert!(track("a").is_null() && !crate::tasks::is_tracked("a"));
    assert!(!w.home().join("tasks/a.json").exists());
    assert_eq!(asked.lock().unwrap().len(), 1);
}

/// With About off, or for what isn't a session Cue can name a file after, or one that's gone, nothing starts.
#[test]
fn nothing_is_followed_with_about_off_or_for_what_isnt_a_session() {
    let _r = Reset;
    let (w, asked) = about_on(vec![]);
    w.session("a").prompt("fix").stop("Fixed.");
    assert_eq!(crate::tasks::start(w.hub().clone(), "../a"), Err("not a session Cue can follow".into()));
    assert_eq!(crate::tasks::start(w.hub().clone(), "gone"), Err("that session is gone".into()));
    std::fs::write(w.home().join("config.json"), r#"{"tasks": {"on": false}}"#).unwrap();
    crate::config::refresh();
    assert!(crate::tasks::start(w.hub().clone(), "a").unwrap_err().starts_with("About is off"));
    assert!(asked.lock().unwrap().is_empty());
}

/// A restart finds what each followed session's About kept, and none of it busy.
#[test]
fn a_restart_finds_each_about_as_it_was_kept_and_idle() {
    let _r = Reset;
    let (w, _) = about_on(vec![Ok(json!({"about": "Login", "summary": "Fixed it."}))]);
    w.session("a").prompt("fix").stop("Fixed.");
    crate::tasks::start(w.hub().clone(), "a").unwrap();
    read_done("a");
    small::reset();
    let t = track("a");
    assert_eq!((t["about"].as_str(), t["summary"].as_str(), t["busy"].as_str()), (Some("Login"), Some("Fixed it."), Some("")));
}
