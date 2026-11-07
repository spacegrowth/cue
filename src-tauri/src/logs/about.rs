//! Your latest request to each Claude session, from its transcript, for the window's state. The state takes
//! what was last read (`about`); the watcher's tick asks for the transcripts to be read again (`read_about`),
//! each on a thread of its own, at most one read in flight a transcript, so a transcript that stops answering
//! holds up only its own session's: never the tick, the window's state, or another session's.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

/// A transcript is looked at again this long after it last was (a busy one changes every second).
const AGAIN_MS: u64 = 5_000;

/// What was last read of a transcript: when it was looked at, its modified time then, and the prompt.
struct Kept {
    at: u64,
    mtime: Option<SystemTime>,
    prompt: String,
}

static ABOUT: Mutex<Option<HashMap<String, Kept>>> = Mutex::new(None);
/// Transcripts being read now.
static READING: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// A read since the tick last asked found another prompt.
static NEWS: AtomicBool = AtomicBool::new(false);

/// Your latest request to a Claude session, as last read ("" before its transcript was, or once it's gone):
/// nothing is read here.
pub fn about(transcript: &str) -> String {
    crate::locking::lock(&ABOUT, "ABOUT").as_ref().and_then(|a| a.get(transcript)).map(|k| k.prompt.clone()).unwrap_or_default()
}

/// The watcher's tick (`now`, its clock): each of these transcripts not looked at in AGAIN_MS, and not
/// being read already, read again on a thread of its own. Returns at once.
pub fn read_about(transcripts: Vec<String>, now: u64) {
    let due: Vec<String> = {
        let kept = crate::locking::lock(&ABOUT, "ABOUT");
        transcripts.into_iter().filter(|t| !t.is_empty() && kept.as_ref().and_then(|k| k.get(t)).is_none_or(|k| now.saturating_sub(k.at) >= AGAIN_MS)).collect()
    };
    for t in due {
        if !crate::locking::lock(&READING, "about READING").get_or_insert_with(HashSet::new).insert(t.clone()) {
            continue;
        }
        let read = std::thread::Builder::new().name("cue-about".into()).spawn({
            let t = t.clone();
            move || {
                read_one(&t, now);
                crate::locking::lock(&READING, "about READING").get_or_insert_with(HashSet::new).remove(&t);
            }
        });
        if read.is_err() {
            crate::locking::lock(&READING, "about READING").get_or_insert_with(HashSet::new).remove(&t);
        }
    }
}

/// Whether a read since the last ask found another prompt (the tick then redraws).
pub fn about_news() -> bool {
    NEWS.swap(false, Ordering::SeqCst)
}

/// Look at one transcript again: read when it changed. Gone (not found): no prompt. Any other failure (a
/// file that can't be read just now) keeps what was last read.
fn read_one(transcript: &str, now: u64) {
    let mtime = match std::fs::metadata(transcript) {
        Ok(m) => m.modified().ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return keep(transcript, now, None, Some(String::new())),
        Err(_) => return,
    };
    let unchanged = crate::locking::lock(&ABOUT, "ABOUT").as_ref().and_then(|a| a.get(transcript)).is_some_and(|k| k.mtime.is_some() && k.mtime == mtime);
    let prompt = if unchanged { None } else { crate::logs::transcript::about(transcript) };
    if unchanged || prompt.is_some() {
        keep(transcript, now, mtime, prompt);
    }
}

/// Note a look at `transcript`: when, its modified time, and its prompt when one was read (None: the same).
fn keep(transcript: &str, now: u64, mtime: Option<SystemTime>, prompt: Option<String>) {
    let mut about = crate::locking::lock(&ABOUT, "ABOUT");
    let kept = about.get_or_insert_with(HashMap::new).entry(transcript.to_string()).or_insert(Kept { at: now, mtime, prompt: String::new() });
    kept.at = now;
    kept.mtime = mtime;
    if let Some(p) = prompt.filter(|p| *p != kept.prompt) {
        kept.prompt = p;
        NEWS.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cue-about-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn asked(prompt: &str) -> String {
        format!("{{\"type\":\"last-prompt\",\"lastPrompt\":\"{prompt}\"}}\n")
    }

    fn until(what: &str, done: impl Fn() -> bool) {
        let t0 = Instant::now();
        while !done() {
            assert!(t0.elapsed() < Duration::from_secs(3), "waited for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A transcript that doesn't answer (a stalled disk: `Stall`) holds up only its own session's read: the
    /// ask returns at once, and another session's transcript is read meanwhile.
    #[test]
    fn a_transcript_that_doesnt_answer_holds_up_only_its_own_read() {
        let d = dir("stall");
        let stalled = crate::testing::Stall::at(&d.join("stalled.jsonl"));
        let fine = d.join("fine.jsonl");
        std::fs::write(&fine, asked("fix the login page")).unwrap();
        let (s, f) = (stalled.path().to_string_lossy().into_owned(), fine.to_string_lossy().into_owned());
        let t0 = Instant::now();
        read_about(vec![s.clone(), f.clone()], 1);
        assert!(t0.elapsed() < Duration::from_secs(1), "the ask waited on a read");
        until("the other transcript's read", || about(&f) == "fix the login page");
        let done = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|sc| {
            sc.spawn(|| stalled.answer_until(|| done.load(Ordering::SeqCst)));
            until("the stalled read to end once answered", || !crate::locking::lock(&READING, "about READING").as_ref().is_some_and(|r| r.contains(&s)));
            done.store(true, Ordering::SeqCst);
        });
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A transcript that's gone says no prompt; one that can't be read just now keeps the last one read.
    #[test]
    fn a_gone_transcript_says_nothing_and_an_unreadable_one_keeps_its_prompt() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir("gone");
        let (gone, locked) = (d.join("gone.jsonl"), d.join("locked.jsonl"));
        std::fs::write(&gone, asked("tidy up")).unwrap();
        std::fs::write(&locked, asked("ship it")).unwrap();
        let (g, l) = (gone.to_string_lossy().into_owned(), locked.to_string_lossy().into_owned());
        read_one(&g, 1);
        read_one(&l, 1);
        assert_eq!((about(&g), about(&l)), ("tidy up".to_string(), "ship it".to_string()));
        std::fs::remove_file(&gone).unwrap();
        std::fs::write(&locked, asked("ship it now")).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        read_one(&g, 10_000);
        read_one(&l, 10_000);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(about(&g), "", "its transcript is gone");
        assert_eq!(about(&l), "ship it", "couldn't be read just now: the last one read");
        let _ = std::fs::remove_dir_all(&d);
    }
}
