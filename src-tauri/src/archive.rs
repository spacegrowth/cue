//! Claude Code's older sessions, for search by name: each transcript's name (your /rename), last
//! prompt, folder and when it was last used.
//!
//! Only the last 64 KB of each transcript is read (that's where Claude Code keeps re-writing those
//! entries), only when the file changed, on a background thread: search reads this small index and
//! stays instant, and the transcripts' contents are never searched.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;

const TAIL: u64 = 64 * 1024;

#[derive(Clone, Default)]
struct Entry {
    session_id: String,
    title: String,
    last_prompt: String,
    cwd: String,
    updated_ms: u64,
}

/// transcript path → (its modified time when read, what it says)
static INDEX: Mutex<Option<HashMap<String, (u64, Entry)>>> = Mutex::new(None);

/// Keep the index current: now, then about once a minute (only changed files are read again).
pub fn start() {
    std::thread::spawn(|| loop {
        refresh();
        std::thread::sleep(std::time::Duration::from_secs(60));
    });
}

fn refresh() {
    let Ok(home) = std::env::var("HOME") else { return };
    let mut seen: HashMap<String, (u64, Entry)> = HashMap::new();
    let old = INDEX.lock().unwrap().clone().unwrap_or_default();
    let Ok(dirs) = std::fs::read_dir(format!("{home}/.claude/projects")) else { return };
    for dir in dirs.flatten() {
        let Ok(files) = std::fs::read_dir(dir.path()) else { continue };
        // Top-level transcripts only: subagents/ holds a session's helpers, not sessions of its own.
        for f in files.flatten().filter(|f| f.path().extension().is_some_and(|e| e == "jsonl")) {
            let path = f.path().to_string_lossy().to_string();
            let Some(mtime) = f.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64) else { continue };
            let entry = match old.get(&path) {
                Some((m, e)) if *m == mtime => e.clone(),
                _ => match read_entry(&path, mtime) {
                    Some(e) => e,
                    None => continue,
                },
            };
            seen.insert(path, (mtime, entry));
        }
    }
    *INDEX.lock().unwrap() = Some(seen);
}

/// Title, last prompt and folder from the end of one transcript.
fn read_entry(path: &str, mtime: u64) -> Option<Entry> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(TAIL))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let (mut custom, mut prompt, mut cwd) = (String::new(), String::new(), String::new());
    // Starting mid-file, the first line is a fragment: skip it.
    for line in text.lines().skip(usize::from(len > TAIL)) {
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        let s = |k: &str| e.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        match e.get("type").and_then(Value::as_str) {
            Some("custom-title") => custom = s("customTitle"),
            Some("last-prompt") => prompt = s("lastPrompt"),
            _ => {}
        }
        if let Some(c) = e.get("cwd").and_then(Value::as_str) {
            cwd = c.to_string();
        }
    }
    let session_id = std::path::Path::new(path).file_stem()?.to_string_lossy().to_string();
    // Its name is the one you gave it (/rename): Claude's own AI title is never used as a name.
    let title = custom;
    (!title.is_empty() || !prompt.is_empty()).then_some(Entry { session_id, title, last_prompt: prompt, cwd, updated_ms: mtime })
}

/// Older sessions whose title, last prompt or folder has `q` (case-insensitive), newest first,
/// leaving out any session in `skip` (ones Cue already knows, shown as live or closed).
pub fn find(q: &str, skip: &std::collections::HashSet<String>, limit: usize) -> Vec<Value> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return vec![];
    }
    let guard = INDEX.lock().unwrap();
    let Some(index) = guard.as_ref() else { return vec![] };
    let mut hits: Vec<&Entry> = index
        .values()
        .map(|(_, e)| e)
        .filter(|e| !skip.contains(&e.session_id))
        .filter(|e| [&e.title, &e.last_prompt, &e.cwd].iter().any(|t| t.to_lowercase().contains(&q)))
        .collect();
    hits.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms));
    hits.dedup_by(|a, b| a.session_id == b.session_id);
    hits.into_iter()
        .take(limit)
        .map(|e| {
            let project = e.cwd.rsplit('/').next().unwrap_or("").to_string();
            json!({ "kind": "older", "session_id": e.session_id, "name": e.title, "project": project, "cwd": e.cwd,
                    "harness": "claude", "at_ms": e.updated_ms, "last": e.last_prompt, "last_role": "you", "ended": true })
        })
        .collect()
}

/// An older session's transcript, folder and title, so Resume can pick it up there and show it in Cue.
pub fn lookup(session_id: &str) -> Option<(String, String, String)> {
    let guard = INDEX.lock().unwrap();
    let (path, (_, e)) = guard.as_ref()?.iter().find(|(_, (_, e))| e.session_id == session_id)?;
    (!e.cwd.is_empty()).then(|| (path.clone(), e.cwd.clone(), e.title.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_the_title_prompt_and_folder_from_the_end() {
        let dir = std::env::temp_dir().join(format!("cue-archive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("abc-123.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        for l in [
            json!({"type":"user","cwd":"/Users/v/dev/cue","message":{"content":"hi"}}),
            json!({"type":"ai-title","aiTitle":"Waiting status line clarity"}),
            json!({"type":"custom-title","customTitle":"cue-bug-fixes"}),
            json!({"type":"last-prompt","lastPrompt":"can we search old sessions by name"}),
        ] {
            writeln!(f, "{l}").unwrap();
        }
        let e = read_entry(path.to_str().unwrap(), 7).unwrap();
        assert_eq!((e.session_id.as_str(), e.title.as_str(), e.cwd.as_str()), ("abc-123", "cue-bug-fixes", "/Users/v/dev/cue"));
        assert_eq!(e.last_prompt, "can we search old sessions by name");
        // Only Claude's own AI title: no name.
        let p2 = dir.join("def-456.jsonl");
        let mut f2 = std::fs::File::create(&p2).unwrap();
        for l in [json!({"type":"ai-title","aiTitle":"Waiting status line clarity"}), json!({"type":"last-prompt","lastPrompt":"hi"})] {
            writeln!(f2, "{l}").unwrap();
        }
        assert_eq!(read_entry(p2.to_str().unwrap(), 7).unwrap().title, "");
        std::fs::remove_dir_all(&dir).ok();
    }
}
