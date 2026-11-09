//! Tests for a machine's hooks: the scripts run under /bin/sh, the watcher's stream, and events reaching Cue.

use super::events::to_cue;
use super::scripts::{watch_script, wrap_status_line, SCRIPT, STATUS, STATUS_CMD, WATCH};
use super::watch::{capped_line, parse};
use super::*;
use crate::machines;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

/// A watcher ends every other watcher of this user (as on a machine, where there is one ~/.cue):
/// the tests that run one take turns.
static WATCHERS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Never with the CUE_HOSTED of whatever runs the tests (a hosted session): the script then skips most events.
fn run_sh(script: &str, home: &std::path::Path, args: &[&str], input: &str) -> String {
    let mut c = Command::new("/bin/sh").arg("-c").arg(script).arg("cue-hook").args(args).env("HOME", home).env_remove("CUE_HOSTED").env("TMUX", "/tmp/x,1,0").env("TMUX_PANE", "%7").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    String::from_utf8_lossy(&c.wait_with_output().unwrap().stdout).to_string()
}

#[test]
fn the_status_line_tells_cue_the_window_when_it_changes_or_every_10_minutes_and_shows_yours_unchanged() {
    let home = std::env::temp_dir().join(format!("cue-status-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/in")).unwrap();
    std::fs::write(home.join(".cue/cue-hook"), SCRIPT).unwrap();
    std::fs::set_permissions(home.join(".cue/cue-hook"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    // Yours: prints the model it was given (so it got the same input), with a quote in it.
    std::fs::write(home.join(".cue/statusline.orig"), r#"sed -n 's/.*"display_name":"\([^"]*\)".*/[\1] it'"'"'s mine/p'"#).unwrap();
    let input = |win: u64| format!(r#"{{"session_id":"abc-123","model":{{"display_name":"Opus"}},"context_window":{{"context_window_size":{win},"used_percentage":12}},"cost":{{"total_cost_usd":1.5}}}}"#);
    let events = || std::fs::read_dir(home.join(".cue/in")).unwrap().count();
    assert_eq!(run_sh(STATUS, &home, &[], &input(200_000)).trim(), "[Opus] it's mine", "your status line, as it was");
    assert_eq!(events(), 1, "the window, told once");
    let ev: Value = serde_json::from_str(&std::fs::read_to_string(std::fs::read_dir(home.join(".cue/in")).unwrap().next().unwrap().unwrap().path()).unwrap()).unwrap();
    assert_eq!(to_cue("box", &ev), Some(json!({ "type": "usage", "session_id": "abc-123", "window": 200_000, "machine": "box" })), "only the window, from that machine");
    run_sh(STATUS, &home, &[], &input(200_000));
    assert_eq!(events(), 1, "the same again: nothing new (it refreshes often)");
    run_sh(STATUS, &home, &[], &input(1_000_000));
    assert_eq!(events(), 2, "a new window: told");
    // The same window 11 minutes on: told again (one lost while Cue couldn't hear the machine comes back).
    let stamp = home.join(".cue/win/abc-123");
    std::fs::File::options().write(true).open(&stamp).unwrap().set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(11 * 60)).unwrap();
    run_sh(STATUS, &home, &[], &input(1_000_000));
    assert_eq!(events(), 3, "the same window, 10 minutes on: told again");
    run_sh(STATUS, &home, &[], &input(1_000_000));
    assert_eq!(events(), 3, "and then not again at once");
    // No status line of yours: it shows nothing, and still tells Cue.
    std::fs::remove_file(home.join(".cue/statusline.orig")).unwrap();
    assert_eq!(run_sh(STATUS, &home, &[], &input(200_000)), "");
    assert_eq!(events(), 4);
    std::fs::remove_dir_all(&home).ok();
}

/// On a real machine, by hand: CUE_HOME=<a Cue folder listing it> CUE_MACHINE=name cargo test -q
/// setup_on_a_machine -- --ignored --nocapture. Sets Cue up there for real (backing up its settings).
#[test]
#[ignore]
fn setup_on_a_machine() {
    let Ok(name) = std::env::var("CUE_MACHINE") else { return };
    let m = machines::get(&name).unwrap();
    let first = setup(&m, &json!({ "claude": true })).unwrap();
    println!("first: {first:?}");
    let again = setup(&m, &json!({ "claude": true })).unwrap();
    assert!(again.is_empty(), "set up already: nothing done the second time ({again:?})");
    let cfg: Value = serde_json::from_str(&machines::run(&m.host, r#"cat "$HOME/.claude/settings.json""#).unwrap()).unwrap();
    assert_eq!(cfg["statusLine"]["command"], STATUS_CMD);
    assert!(machines::run(&m.host, r#"test -x "$HOME/.cue/cue-statusline" && echo ok"#).unwrap().contains("ok"));
}

#[test]
fn cues_status_line_goes_in_once_keeping_yours_to_run() {
    let cfg = json!({ "statusLine": { "type": "command", "command": "bash ~/.claude/line.sh", "padding": 0 }, "model": "opus" });
    let (wrapped, was) = wrap_status_line(&cfg).unwrap();
    assert_eq!(was, "bash ~/.claude/line.sh");
    assert_eq!(wrapped["statusLine"], json!({ "type": "command", "command": "\"$HOME/.cue/cue-statusline\"", "padding": 0 }), "yours replaced, its other settings kept");
    assert_eq!(wrapped["model"], "opus", "everything else as it was");
    assert!(wrap_status_line(&wrapped).is_none(), "in already: nothing to do, and the saved one stays");
    let (none, was) = wrap_status_line(&json!({})).unwrap();
    assert_eq!((none["statusLine"]["command"].as_str(), was.as_str()), (Some("\"$HOME/.cue/cue-statusline\""), ""), "none of yours: Cue's, showing nothing");
}

#[test]
fn the_watcher_hands_an_event_over_within_a_fraction_of_a_second() {
    let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
    let home = std::env::temp_dir().join(format!("cue-watch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/in")).unwrap();
    let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let out = c.stdout.take().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send((l, std::time::Instant::now()));
        }
    });
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap().0, "@@hb", "it says it's alive first");
    std::thread::sleep(Duration::from_millis(250));
    std::fs::write(home.join(".cue/in/123-4.json"), "{\"event\":\"stop\"}\n").unwrap();
    let dropped = std::time::Instant::now();
    let (line, at) = loop {
        let (l, at) = rx.recv_timeout(Duration::from_secs(3)).expect("the event comes through");
        if l != "@@hb" {
            break (l, at);
        }
    };
    let _ = c.kill();
    let _ = c.wait();
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(line, r#"@@123-4 {"event":"stop"}"#);
    assert!(at.duration_since(dropped) < Duration::from_millis(400), "within a few tenths of a second: {:?}", at.duration_since(dropped));
}

#[test]
fn a_new_watcher_ends_the_one_before_and_gets_every_event() {
    let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
    let home = std::env::temp_dir().join(format!("cue-watch2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/in")).unwrap();
    // A watcher left behind by an earlier Cue (its output going nowhere, as on a server that never ends it).
    let mut old = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::null()).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(400));
    let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let out = c.stdout.take().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), "@@hb");
    let gone = (0..30).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        matches!(old.try_wait(), Ok(Some(_)))
    });
    for i in 0..10 {
        std::fs::write(home.join(format!(".cue/in/9-{i}.json")), format!("{{\"n\":{i}}}\n")).unwrap();
    }
    let mut got = vec![];
    while let Ok(l) = rx.recv_timeout(Duration::from_secs(2)) {
        if l != "@@hb" {
            got.push(l);
        }
        if got.len() == 10 {
            break;
        }
    }
    let _ = old.kill();
    let _ = old.wait();
    let _ = c.kill();
    let _ = c.wait();
    let _ = std::fs::remove_dir_all(&home);
    assert!(gone, "the earlier watcher left");
    got.sort();
    assert_eq!(got, (0..10).map(|i| format!("@@9-{i} {{\"n\":{i}}}")).collect::<Vec<_>>(), "every event, whole, to the current watcher");
}

#[test]
fn another_macs_watcher_is_left_alone_and_an_event_a_dead_watcher_claimed_is_delivered() {
    let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
    let home = std::env::temp_dir().join(format!("cue-watch3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/in")).unwrap();
    let mut other = Command::new("/bin/sh").arg("-c").arg(WATCH.replace("@MAC@", "another-mac")).env("HOME", &home).stdout(Stdio::null()).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(400));
    // Claimed by a watcher that died before handing it over (no process has that pid).
    std::fs::write(home.join(".cue/in/7-1.json.999999"), "{\"n\":7}\n").unwrap();
    let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
    let out = c.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    // The other Mac's watcher may take it first: then it's handed over there, which is fine too.
    let mut got = None;
    let end = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < end {
        if let Ok(l) = rx.recv_timeout(Duration::from_millis(200)) {
            if l != "@@hb" {
                got = Some(l);
                break;
            }
        }
    }
    // Its file goes right after its line is printed.
    let count = || std::fs::read_dir(home.join(".cue/in")).unwrap().count();
    let left = (0..20).map(|_| { std::thread::sleep(Duration::from_millis(50)); count() }).find(|n| *n == 0).unwrap_or_else(count);
    let other_alive = matches!(other.try_wait(), Ok(None));
    for p in [&mut other, &mut c] {
        let _ = p.kill();
        let _ = p.wait();
    }
    let _ = std::fs::remove_dir_all(&home);
    assert!(other_alive, "another Mac's watcher keeps running");
    assert_eq!(left, 0, "the claimed event was put back and handed over");
    if let Some(l) = got {
        assert_eq!(l, r#"@@7-1 {"n":7}"#);
    }
}

#[test]
fn a_line_from_a_machine_is_capped_and_the_stream_goes_on_after_one_too_long() {
    let long = "x".repeat(50);
    let data = format!("@@hb\r\n{long}\nshort\nlast");
    let mut r = BufReader::with_capacity(8, data.as_bytes());
    assert_eq!(capped_line(&mut r, 20).unwrap(), Some(Some("@@hb".into())));
    assert_eq!(capped_line(&mut r, 20).unwrap(), Some(None), "over the cap: skipped whole");
    assert_eq!(capped_line(&mut r, 20).unwrap(), Some(Some("short".into())), "and the next line is intact");
    assert_eq!(capped_line(&mut r, 20).unwrap(), Some(Some("last".into())), "a last line without its newline");
    assert_eq!(capped_line(&mut r, 20).unwrap(), None, "the end");
    let mut exact = BufReader::new("12345\n".as_bytes());
    assert_eq!(capped_line(&mut exact, 5).unwrap(), Some(Some("12345".into())), "the cap counts the line, not its newline");
}

#[test]
fn the_watcher_drops_answers_nobody_collected_after_a_day() {
    let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
    let home = std::env::temp_dir().join(format!("cue-watch-old-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/out")).unwrap();
    let (old, fresh) = (home.join(".cue/out/1-1.json"), home.join(".cue/out/2-2.json"));
    std::fs::write(&old, "{}").unwrap();
    std::fs::write(&fresh, "{}").unwrap();
    let two_days_ago = std::time::SystemTime::now() - Duration::from_secs(2 * 86400);
    std::fs::File::options().write(true).open(&old).unwrap().set_modified(two_days_ago).unwrap();
    let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
    let out = c.stdout.take().unwrap();
    let mut r = BufReader::new(out);
    let _ = capped_line(&mut r, 1024); // the first heartbeat: the sweep ran just before it
    std::thread::sleep(Duration::from_millis(300));
    let _ = c.kill();
    let _ = c.wait();
    let (old_there, fresh_there) = (old.exists(), fresh.exists());
    let _ = std::fs::remove_dir_all(&home);
    assert!(!old_there, "a day-old answer is gone");
    assert!(fresh_there, "a fresh one stays");
}

#[test]
fn events_waiting_on_the_machine_come_before_its_first_beat() {
    let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
    // An agent ended while Cue wasn't watching: its event waits in ~/.cue/in. The stream hands it
    // over before saying the machine is reached, so a rebuild on reaching it knows it ended.
    let home = std::env::temp_dir().join(format!("cue-watch-q-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/in")).unwrap();
    std::fs::write(home.join(".cue/in/100-1.json"), "{\"event\":\"end\"}\n").unwrap();
    let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
    let mut lines = BufReader::new(c.stdout.take().unwrap()).lines();
    let first = lines.next().unwrap().unwrap();
    let second = lines.next().unwrap().unwrap();
    let _ = c.kill();
    let _ = c.wait();
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!((first.as_str(), second.as_str()), (r#"@@100-1 {"event":"end"}"#, "@@hb"));
}

#[test]
fn the_hook_drops_each_event_for_cue_and_never_waits_when_cue_isnt_watching() {
    let home = std::env::temp_dir().join(format!("cue-mh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    run_sh(SCRIPT, &home, &["stop", "claude"], r#"{"session_id":"s1","last_assistant_message":"Done."}"#);
    let files: Vec<_> = std::fs::read_dir(home.join(".cue/in")).unwrap().flatten().collect();
    assert_eq!(files.len(), 1);
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(files[0].metadata().unwrap().permissions().mode() & 0o777, 0o600, "the event is readable by you only");
    }
    let ev: Value = serde_json::from_str(&std::fs::read_to_string(files[0].path()).unwrap()).unwrap();
    assert_eq!(ev["event"], "stop");
    assert_eq!(ev["pane"], "%7");
    assert_eq!(ev["input"]["last_assistant_message"], "Done.");
    // A permission with nobody watching: it leaves at once, with no answer (the terminal asks).
    let t = std::time::Instant::now();
    let out = run_sh(SCRIPT, &home, &["permission", "claude"], r#"{"session_id":"s1","tool_name":"Bash"}"#);
    assert!(out.is_empty() && t.elapsed() < Duration::from_secs(3));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_message_or_reply_waits_for_its_status_list_only_in_a_session_its_on_for_while_cue_is_watching() {
    let home = std::env::temp_dir().join(format!("cue-mh-status-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/out")).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::write(home.join(".cue/watching"), format!("{now}\n")).unwrap();
    std::fs::write(home.join(".cue/status-on"), "s-on\nother").unwrap();
    // Cue's side: answer whatever shows up, as handle() does.
    let answer_next = |home: std::path::PathBuf| std::thread::spawn(move || {
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(100));
            let ev = std::fs::read_dir(home.join(".cue/in")).ok().into_iter().flatten().flatten().find(|e| !home.join(format!(".cue/out/{}.json", e.path().file_stem().unwrap().to_string_lossy())).exists() && std::fs::read_to_string(e.path()).is_ok_and(|t| t.contains("s-on")));
            if let Some(f) = ev {
                let id = f.path().file_stem().unwrap().to_string_lossy().to_string();
                std::fs::write(home.join(format!(".cue/out/{id}.json")), r#"{"hookSpecificOutput":{"additionalContext":"list"}}"#).unwrap();
                return;
            }
        }
    });
    // A session it isn't on for: no wait, nothing back (its event still goes to Cue).
    let t = std::time::Instant::now();
    assert_eq!(run_sh(SCRIPT, &home, &["prompt", "claude"], r#"{"session_id":"s-off","prompt":"hi"}"#), "");
    assert!(t.elapsed() < Duration::from_secs(1), "never waits for a session it's off for");
    assert_eq!(std::fs::read_dir(home.join(".cue/in")).unwrap().count(), 1);
    // One it's on for: Cue's answer, handed to the agent.
    let a = answer_next(home.clone());
    assert_eq!(run_sh(SCRIPT, &home, &["prompt", "claude"], r#"{"session_id":"s-on","prompt":"hi"}"#), r#"{"hookSpecificOutput":{"additionalContext":"list"}}"#);
    a.join().unwrap();
    // Cue not watching: no wait at all.
    std::fs::write(home.join(".cue/watching"), "0\n").unwrap();
    let t = std::time::Instant::now();
    assert_eq!(run_sh(SCRIPT, &home, &["stop", "claude"], r#"{"session_id":"s-on","last_assistant_message":"Done."}"#), "");
    assert!(t.elapsed() < Duration::from_secs(1), "Cue away: never waits");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_permission_waits_for_cues_answer_while_cue_is_watching() {
    let home = std::env::temp_dir().join(format!("cue-mh2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/out")).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::write(home.join(".cue/watching"), format!("{now}\n")).unwrap();
    let h2 = home.clone();
    // Cue's side: answer whatever question shows up.
    let answerer = std::thread::spawn(move || {
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            if let Some(f) = std::fs::read_dir(h2.join(".cue/in")).ok().and_then(|mut d| d.next()).and_then(|e| e.ok()) {
                let id = f.path().file_stem().unwrap().to_string_lossy().to_string();
                std::fs::write(h2.join(format!(".cue/out/{id}.json")), r#"{"decision":"allow"}"#).unwrap();
                return;
            }
        }
    });
    let out = run_sh(SCRIPT, &home, &["permission", "claude"], r#"{"session_id":"s1","tool_name":"Bash"}"#);
    answerer.join().unwrap();
    assert_eq!(out, r#"{"decision":"allow"}"#);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_permission_keeps_waiting_through_an_empty_read_of_the_stamp() {
    let home = std::env::temp_dir().join(format!("cue-mh4-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/out")).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::write(home.join(".cue/watching"), format!("{now}\n")).unwrap();
    let h2 = home.clone();
    // The stamp is empty for a while (as it is for an instant each time the watcher rewrote it by
    // truncating), then back; then Cue answers.
    let answerer = std::thread::spawn(move || {
        std::fs::write(h2.join(".cue/watching"), "").unwrap();
        std::thread::sleep(Duration::from_millis(1200));
        std::fs::write(h2.join(".cue/watching"), format!("{now}\n")).unwrap();
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            if let Some(f) = std::fs::read_dir(h2.join(".cue/in")).ok().and_then(|mut d| d.next()).and_then(|e| e.ok()) {
                let id = f.path().file_stem().unwrap().to_string_lossy().to_string();
                std::fs::write(h2.join(format!(".cue/out/{id}.json")), r#"{"decision":"allow"}"#).unwrap();
                return;
            }
        }
    });
    let out = run_sh(SCRIPT, &home, &["permission", "claude"], r#"{"session_id":"s1","tool_name":"Bash"}"#);
    answerer.join().unwrap();
    assert_eq!(out, r#"{"decision":"allow"}"#, "an empty read of the stamp isn't Cue gone");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_question_cue_has_taken_still_gets_its_answer_after_the_line_goes_quiet() {
    let home = std::env::temp_dir().join(format!("cue-mh3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/out")).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::write(home.join(".cue/watching"), format!("{now}\n")).unwrap();
    let h2 = home.clone();
    // Cue takes the question, then goes quiet for a minute (asleep, Wi-Fi), then answers.
    let answerer = std::thread::spawn(move || {
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            if let Some(f) = std::fs::read_dir(h2.join(".cue/in")).ok().and_then(|mut d| d.next()).and_then(|e| e.ok()) {
                let id = f.path().file_stem().unwrap().to_string_lossy().to_string();
                std::fs::rename(f.path(), h2.join(format!(".cue/in/{id}.json.1"))).unwrap();
                std::fs::write(h2.join(".cue/watching"), format!("{}\n", now - 60)).unwrap();
                std::thread::sleep(Duration::from_millis(1500));
                std::fs::write(h2.join(format!(".cue/out/{id}.json")), r#"{"decision":"allow"}"#).unwrap();
                return;
            }
        }
    });
    let out = run_sh(SCRIPT, &home, &["permission", "claude"], r#"{"session_id":"s1","tool_name":"AskUserQuestion"}"#);
    answerer.join().unwrap();
    assert_eq!(out, r#"{"decision":"allow"}"#, "a quiet minute after Cue took it isn't the end");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_event_from_a_machine_reaches_cue_as_the_macs_hook_would_send_it_placed_on_that_machine() {
    let ev = json!({ "event": "prompt", "harness": "claude", "pane": "%3", "tmux": "1", "input": { "session_id": "s9", "cwd": "/home/me/app", "prompt": "pwd", "transcript_path": "/home/me/.claude/x.jsonl" } });
    let m = to_cue("build-box", &ev).unwrap();
    assert_eq!(m["type"], "event");
    assert_eq!(m["event"], "active");
    assert_eq!(m["message"], "pwd");
    assert_eq!(m["machine"], "build-box");
    assert_eq!(m["tmux_pane"], "%3");
    let ask = to_cue("build-box", &json!({ "event": "permission", "harness": "claude", "input": { "session_id": "s9", "tool_name": "Bash", "tool_input": { "command": "ls" } } })).unwrap();
    assert_eq!(ask["type"], "ask");
    assert_eq!(ask["kind"], "permission");
    assert_eq!(ask["tmux_pane"], "", "not in tmux: no pane");
    assert!(to_cue("b", &json!({ "event": "whatever", "input": {} })).is_none());
    assert_eq!(parse(r#"@@1700000000-42 {"event":"stop","input":{}}"#).unwrap().0, "1700000000-42");
    assert!(parse("@@../../x {}").is_none(), "an id is digits and a dash");
}

#[test]
fn a_hosted_agents_events_stay_on_the_machine_except_the_status_lists() {
    let home = std::env::temp_dir().join(format!("cue-hook-hosted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cue/in")).unwrap();
    std::fs::write(home.join(".cue/status-on"), "s-on\n").unwrap();
    let hosted = |args: &[&str], input: &str| {
        let mut c = Command::new("/bin/sh").arg("-c").arg(SCRIPT).arg("cue-hook").args(args).env("HOME", &home).env("CUE_HOSTED", "1").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let t = std::time::Instant::now();
        let out = String::from_utf8_lossy(&c.wait_with_output().unwrap().stdout).to_string();
        (out, t.elapsed())
    };
    let written = || std::fs::read_dir(home.join(".cue/in")).unwrap().flatten().map(|f| std::fs::read_to_string(f.path()).unwrap()).collect::<Vec<_>>();
    // A permission ask, a turn's end, a prompt in a session with no list: nothing for Cue, no wait (Cue asks it
    // itself, over cue-host).
    for (args, input) in [(&["permission", "claude"][..], r#"{"session_id":"s-on","tool_name":"Bash"}"#), (&["stop", "claude"][..], r#"{"session_id":"s-off"}"#), (&["prompt", "claude"][..], r#"{"session_id":"s-off","prompt":"hi"}"#)] {
        let (out, took) = hosted(args, input);
        assert_eq!(out, "");
        assert!(took < Duration::from_secs(1), "{args:?} never waits");
    }
    assert!(written().is_empty(), "nothing handed to Cue: {:?}", written());
    // A prompt in a session with the list on still goes, marked hosted (Cue answers only its list; not watching here,
    // so no wait).
    hosted(&["prompt", "claude"], r#"{"session_id":"s-on","prompt":"hi"}"#);
    let w = written();
    assert_eq!(w.len(), 1);
    assert!(w[0].contains(r#""hosted":"1""#), "{}", w[0]);
    let _ = std::fs::remove_dir_all(&home);
}
