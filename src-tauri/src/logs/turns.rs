//! The Insights tab's history: every session's turns, read from its own log into the `turns` table
//! (db.rs). Claude Code's, Codex's and Pi's logs on this Mac, and the copies Cue keeps of machines' logs
//! (never a machine's own folders).
//!
//! On a background thread at low priority: at launch, then every 10 minutes. Each log is read from where
//! the last read left off, a few MB at a time, so the first pass over years of logs doesn't hold anything
//! up; while some are still behind, the next pass comes sooner.

use crate::db;
use crate::logs::steps;
use std::path::{Path, PathBuf};

/// The most read from one log in one pass.
const PER_FILE: u64 = 4 * 1024 * 1024;
/// The most read in one pass, over every log.
const PER_PASS: u64 = 64 * 1024 * 1024;
/// A single turn bigger than this is skipped rather than read whole.
const LONGEST_TURN: u64 = 64 * 1024 * 1024;

/// While catching up: how much of every log has been read, and how much there is (bytes). None once it
/// has caught up. For the Insights tab's "Reading your session history" bar.
static CATCHING_UP: std::sync::Mutex<Option<(u64, u64)>> = std::sync::Mutex::new(None);

pub fn catching_up() -> Option<(u64, u64)> {
    let c = crate::locking::lock(&CATCHING_UP, "turns CATCHING_UP");
    *c
}

/// How much of these logs has been read so far, and their sizes, in bytes.
fn read_so_far(logs: &[(String, PathBuf)]) -> (u64, u64) {
    let (mut done, mut total) = (0u64, 0u64);
    for (_, path) in logs {
        let (Some(p), Ok(meta)) = (path.to_str(), std::fs::metadata(path)) else { continue };
        let size = meta.len();
        let (from, seen, _) = db::turns_read(p).unwrap_or((0, 0, 0));
        total += size;
        done += if seen == size { size } else { from.min(size) };
    }
    (done, total)
}

/// Keep the turns current: now, then every 10 minutes (every 20 seconds while catching up).
pub fn start() {
    std::thread::Builder::new()
        .name("cue-turns".into())
        .spawn(|| {
            low_priority();
            loop {
                let all = logs();
                let behind = pass(&all);
                let now = if behind { Some(read_so_far(&all)) } else { None };
                {
                    let mut c = crate::locking::lock(&CATCHING_UP, "turns CATCHING_UP");
                    *c = now;
                }
                std::thread::sleep(std::time::Duration::from_secs(if behind { 20 } else { 600 }));
            }
        })
        .ok();
}

/// This thread runs in the background class: it never competes with the window or the sessions.
fn low_priority() {
    #[cfg(target_os = "macos")]
    unsafe {
        extern "C" {
            fn pthread_set_qos_class_self_np(class: u32, relative: i32) -> i32;
        }
        const QOS_CLASS_BACKGROUND: u32 = 0x09;
        pthread_set_qos_class_self_np(QOS_CLASS_BACKGROUND, 0);
    }
}

/// Every session log to read, with its machine ('' = this Mac).
fn logs() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let jsonl = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir).into_iter().flatten().flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e == "jsonl")).collect()
    };
    let subdirs = |dir: &Path| -> Vec<PathBuf> { std::fs::read_dir(dir).into_iter().flatten().flatten().map(|f| f.path()).filter(|p| p.is_dir()).collect() };
    let home = std::env::var("HOME").unwrap_or_default();
    // Claude Code: ~/.claude/projects/<folder>/<session>.jsonl (top level only: subagents/ holds a session's helpers).
    for d in subdirs(&Path::new(&home).join(".claude/projects")) {
        out.extend(jsonl(&d).into_iter().map(|p| (String::new(), p)));
    }
    // Codex: <sessions>/<year>/<month>/<day>/rollout-….jsonl.
    for y in subdirs(&crate::ipc::codex_sessions_dir()) {
        for m in subdirs(&y) {
            for d in subdirs(&m) {
                out.extend(jsonl(&d).into_iter().map(|p| (String::new(), p)));
            }
        }
    }
    // Pi: <sessions>/<folder>/<time>_<session>.jsonl.
    for d in subdirs(&crate::ipc::pi_sessions_dir()) {
        out.extend(jsonl(&d).into_iter().map(|p| (String::new(), p)));
    }
    out.extend(crate::machines::logs::copied_logs());
    out
}

/// Read what's new in each log. True when some log still has more than this pass read.
fn pass(logs: &[(String, PathBuf)]) -> bool {
    let mut budget = PER_PASS;
    let mut behind = false;
    for (machine, path) in logs {
        if budget == 0 {
            return true;
        }
        let (read, more) = read_log(machine, path, budget.min(PER_FILE));
        budget = budget.saturating_sub(read);
        behind |= more;
    }
    behind
}

/// A log's session id, from its file name: Claude Code's is the name; Codex's ends `-<id>`; Pi's `_<id>`.
fn session_of(path: &Path, harness: &str) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    match harness {
        "codex" if stem.len() >= 36 && stem.is_char_boundary(stem.len() - 36) => stem[stem.len() - 36..].to_string(),
        "pi" => stem.rsplit('_').next().unwrap_or(&stem).to_string(),
        _ => stem,
    }
}

/// Read one log's new turns, up to `max` bytes, into the database. Returns the bytes read and whether
/// there's more after them.
fn read_log(machine: &str, path: &Path, max: u64) -> (u64, bool) {
    let Some(p) = path.to_str() else { return (0, false) };
    let Ok(meta) = std::fs::metadata(path) else { return (0, false) };
    let size = meta.len();
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64);
    let (mut from, was_size, was_mtime) = db::turns_read(p).unwrap_or((0, 0, 0));
    if size == was_size && mtime == was_mtime {
        return (0, false);
    }
    // Smaller than where it was read to: it was written over (a machine's copy starts over each time it
    // reconnects). Read it again from the top; turns already kept are only brought up to date.
    if size < from {
        from = 0;
    }
    let mut want = max;
    let (rows, next) = loop {
        let (rows, next) = steps::turn_rows(p, from, want);
        // One turn longer than what was read: read more of it at once, up to a point, then skip it.
        // SHORTCUT: a turn over LONGEST_TURN is lost from the history. Upgrade path: a turn_rows that
        // carries a turn's running totals across reads instead of starting it over.
        if next == from && size > from + want {
            if want < LONGEST_TURN {
                want = (want * 4).min(LONGEST_TURN);
                continue;
            }
            break (rows, from + want);
        }
        break (rows, next);
    };
    let harness = steps::harness_of(p);
    let sid = session_of(path, harness);
    // Read to its end: kept with its size and time, so it isn't opened again until it changes (its open
    // turn, from `next`, is read again then). Not to its end: read on next pass whatever it does.
    let more = size > from + want;
    let seen = if more { (0, 0) } else { (size, mtime) };
    db::save_turns(p, &db::TurnsOf { session_id: &sid, machine, harness }, &rows, (next, seen.0, seen.1));
    (size.min(from + want) - from, more)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn line(f: &mut std::fs::File, v: serde_json::Value) {
        writeln!(f, "{v}").unwrap();
    }
    fn prompt(f: &mut std::fs::File, at: &str, text: &str) {
        line(f, json!({"type":"user","timestamp":at,"cwd":"/x/proj","message":{"role":"user","content":text}}));
    }
    fn reply(f: &mut std::fs::File, at: &str, out: u64) {
        line(f, json!({"type":"assistant","timestamp":at,"cwd":"/x/proj","message":{"role":"assistant","model":"claude-test","content":[{"type":"text","text":"ok"}],
            "usage":{"input_tokens":10,"cache_read_input_tokens":1000,"cache_creation_input_tokens":0,"output_tokens":out}}}));
    }
    /// A session's kept turns, oldest first: (tokens, corrected).
    fn stored(sid: &str) -> Vec<(u64, bool)> {
        db::turns_since(0).into_iter().filter(|t| t.session_id == sid).map(|t| (t.tokens, t.corrected)).collect()
    }

    #[test]
    fn reads_on_from_where_it_left_off_and_brings_the_open_turn_up_to_date() {
        let _g = db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-turns-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        db::reset();
        let path = dir.join("sess-1.jsonl");
        let p = path.to_str().unwrap();
        let mut f = std::fs::File::create(&path).unwrap();
        prompt(&mut f, "2026-10-01T10:00:00Z", "add the thing");
        reply(&mut f, "2026-10-01T10:00:05Z", 50);
        prompt(&mut f, "2026-10-01T10:01:00Z", "now test it");
        reply(&mut f, "2026-10-01T10:01:05Z", 20);
        f.sync_all().unwrap();
        let (read, more) = read_log("", &path, PER_FILE);
        assert!(read > 0 && !more);
        let first = stored("sess-1");
        assert_eq!(first.len(), 2, "both turns, the open one too: {first:?}");
        let (offset, size, _) = db::turns_read(p).unwrap();
        let len = std::fs::metadata(&path).unwrap().len();
        assert!(offset > 0 && offset < len, "the next read starts at the open turn: {offset} of {len}");
        assert_eq!(size, len);
        assert_eq!(read_log("", &path, PER_FILE), (0, false), "unchanged: not read again");

        // The open turn goes on, then a correction follows it.
        reply(&mut f, "2026-10-01T10:01:30Z", 30);
        prompt(&mut f, "2026-10-01T10:02:00Z", "no, the other file");
        reply(&mut f, "2026-10-01T10:02:05Z", 5);
        f.sync_all().unwrap();
        read_log("", &path, PER_FILE);
        let after = stored("sess-1");
        assert_eq!(after.len(), 3, "the second turn brought up to date, not added again: {after:?}");
        assert!(after[1].0 > first[1].0, "its tokens grew with the reply added: {first:?} → {after:?}");
        assert!(after[1].1 && !after[0].1, "the prompt after it read as a correction");

        // Written over from the top (a machine's copy, reconnected): read again, nothing counted twice.
        std::fs::write(&path, "").unwrap();
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        prompt(&mut f, "2026-10-01T10:00:00Z", "add the thing");
        reply(&mut f, "2026-10-01T10:00:05Z", 50);
        f.sync_all().unwrap();
        read_log("", &path, PER_FILE);
        assert_eq!(stored("sess-1").len(), 3);
        std::env::remove_var("CUE_HOME");
        db::reset();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_small_cap_reads_a_long_log_over_several_passes() {
        let _g = db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-turns-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        db::reset();
        let path = dir.join("sess-2.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        for n in 0..20 {
            prompt(&mut f, &format!("2026-10-02T10:{n:02}:00Z"), &format!("step {n}"));
            reply(&mut f, &format!("2026-10-02T10:{n:02}:05Z"), 10);
        }
        f.sync_all().unwrap();
        let mut passes = 0;
        while read_log("", &path, 1500).1 {
            passes += 1;
            assert!(passes < 100);
        }
        assert!(passes > 1, "more than one pass");
        let n = db::turns_since(0).into_iter().filter(|t| t.session_id == "sess-2").count();
        assert_eq!(n, 20);
        std::env::remove_var("CUE_HOME");
        db::reset();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_session_id_from_each_agents_file_name() {
        assert_eq!(session_of(Path::new("/a/abc-1.jsonl"), "claude"), "abc-1");
        assert_eq!(session_of(Path::new("/a/rollout-2026-10-01T10-00-00-0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b.jsonl"), "codex"), "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b");
        assert_eq!(session_of(Path::new("/a/2026-10-01T10-00-00-000Z_0199a1b2-c3d4.jsonl"), "pi"), "0199a1b2-c3d4");
    }
}
