//! Every live agent session on this Mac, not just the ones that have talked to Cue — for the
//! Sessions view ("where is that session?"). Cue learns about a session from its hooks: one that's
//! quiet (busy on a long turn, or idle since before Cue started) is invisible to the Board.
//!
//! Claude Code keeps its own registry, `~/.claude/sessions/<pid>.json` (name, folder, busy/idle),
//! so every live Claude session can be listed. Pi sessions running Cue's extension subscribe to
//! Cue the moment they start (the hub remembers them). Codex has no registry: Cue knows a Codex
//! session from your first message to it.
//!
//! Each folder's repository and branch (for the Sessions view's headings) are in repos.rs.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A live session Cue may not have heard from yet.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Quiet {
    pub session_id: String,
    pub harness: String,
    /// Its name ("login-fix", set with /rename or by Claude), else "".
    pub name: String,
    /// The name is one Claude made up for the process (`nameSource: "derived"`, "cue-68"), not one anybody gave it.
    #[serde(skip)]
    pub generated: bool,
    pub cwd: String,
    pub pid: i32,
    /// "busy", "idle", or "waiting" (a prompt is open in its terminal), as Claude Code reports it.
    pub status: String,
    /// While "waiting": what for, in Claude Code's words ("approve Bash"), else "".
    #[serde(skip)]
    pub waiting_for: String,
    /// Since when it's been busy / idle (ms).
    pub since_ms: u64,
}

static CLAUDE: Mutex<Vec<Quiet>> = Mutex::new(Vec::new());

fn claude_sessions_dir() -> PathBuf {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home.join(".claude")).join("sessions")
}

/// The live interactive Claude Code sessions in `dir` (Claude's registry), oldest first.
fn scan_claude(dir: &Path) -> Vec<Quiet> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<Quiet> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| serde_json::from_str::<Value>(&std::fs::read_to_string(p).ok()?).ok())
        .filter(|r| r.get("kind").and_then(Value::as_str).map_or(true, |k| k == "interactive"))
        .filter_map(|r| {
            let pid = r.get("pid").and_then(Value::as_i64).filter(|p| *p > 0)? as i32;
            if !crate::server::alive(pid) {
                return None;
            }
            let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            let ms = |k: &str| r.get(k).and_then(Value::as_u64).unwrap_or(0);
            if throwaway(pid, ms("startedAt")) {
                return None;
            }
            Some(Quiet {
                session_id: Some(s("sessionId")).filter(|x| !x.is_empty())?,
                harness: "claude".into(),
                name: s("name"),
                generated: s("nameSource") == "derived",
                cwd: s("cwd"),
                pid,
                status: s("status"),
                waiting_for: s("waitingFor"),
                since_ms: [ms("statusUpdatedAt"), ms("startedAt")].into_iter().find(|t| *t > 0).unwrap_or(0),
            })
        })
        .collect();
    out.sort_by_key(|q| (q.since_ms, q.pid));
    out
}

/// Registry pids known to be (true) or not to be a throwaway run, by pid and when it started.
static THROWAWAY: Mutex<Option<HashMap<i32, (u64, bool)>>> = Mutex::new(None);

/// A Claude run that keeps no session (`--no-session-persistence`): Cue's own helpers (the slash-command
/// list, btw, catch-up, the status list) and anyone else's. Claude lists it like a session, under its
/// folder or, for a fork, the session it forked; it isn't one, so it never becomes a card. Its command
/// line is read once per process.
fn throwaway(pid: i32, started: u64) -> bool {
    let known = crate::locking::lock(&THROWAWAY, "throwaway runs").as_ref().and_then(|m| m.get(&pid).copied());
    if let Some((_, is)) = known.filter(|(at, _)| *at == started) {
        return is;
    }
    let args = std::process::Command::new("/bin/ps").args(["-ww", "-o", "args=", "-p", &pid.to_string()]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let is = args.split_whitespace().any(|a| a == "--no-session-persistence");
    let mut cache = crate::locking::lock(&THROWAWAY, "throwaway runs");
    let map = cache.get_or_insert_with(HashMap::new);
    // Pids come and go: forget them all once there are many (each is read again, once).
    if map.len() > 512 {
        map.clear();
    }
    map.insert(pid, (started, is));
    is
}

/// Re-read Claude's registry. True when the list changed (so the window should redraw).
pub fn refresh() -> bool {
    let fresh = scan_claude(&claude_sessions_dir());
    let mut cur = crate::locking::lock(&CLAUDE, "CLAUDE");
    if *cur == fresh {
        return false;
    }
    *cur = fresh;
    true
}

/// When a live Claude session's process started (ms, its registry entry's `startedAt`); None when it
/// isn't running here.
pub fn started_ms(session_id: &str) -> Option<u64> {
    let pid = claude().into_iter().find(|q| q.session_id == session_id)?.pid;
    let r: Value = serde_json::from_str(&std::fs::read_to_string(claude_sessions_dir().join(format!("{pid}.json"))).ok()?).ok()?;
    r.get("startedAt").and_then(Value::as_u64)
}

/// Every live Claude session in the registry.
pub fn claude() -> Vec<Quiet> {
    crate::locking::lock(&CLAUDE, "CLAUDE").clone()
}

/// Tests: stand in for Claude Code's registry.
#[cfg(test)]
pub fn set_claude(list: Vec<Quiet>) {
    *crate::locking::lock(&CLAUDE, "CLAUDE") = list;
}

/// The terminal a process runs in ("/dev/ttys012"), for "Go to tab" on a session that never sent
/// Cue its own (its hooks report the tty; the registry doesn't).
pub fn tty_of(pid: i32) -> String {
    std::process::Command::new("/bin/ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|t| !t.is_empty() && t != "??")
        .map(|t| if t.starts_with("/dev/") { t } else { format!("/dev/{t}") })
        .unwrap_or_default()
}

/// Where a quiet Claude session runs, as its hooks would have said: its process's terminal (tty) and
/// the terminal's variables from its environment (`ps -E`, your own processes only), so Cue can type
/// into it before it has sent Cue anything.
pub fn origin_of(q: &Quiet) -> crate::model::Origin {
    let out = std::process::Command::new("/bin/ps").args(["-E", "-ww", "-o", "command=", "-p", &q.pid.to_string()]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let env = parse_env(&out);
    let get = |k: &str| env.get(k).cloned().unwrap_or_default();
    crate::model::Origin {
        harness: q.harness.clone(),
        session_id: q.session_id.clone(),
        cwd: q.cwd.clone(),
        transcript_path: claude_transcript(&q.cwd, &q.session_id).unwrap_or_default(),
        tty: tty_of(q.pid),
        agent_pid: Some(q.pid),
        term_program: get("TERM_PROGRAM"),
        iterm_session_id: get("ITERM_SESSION_ID"),
        tmux_pane: if get("TMUX").is_empty() { String::new() } else { get("TMUX_PANE") },
        wezterm_pane: get("WEZTERM_PANE"),
        kitty_window_id: get("KITTY_WINDOW_ID"),
        kitty_listen_on: get("KITTY_LISTEN_ON"),
        machine: String::new(), // a session Claude Code lists on this Mac
        hosted: false,          // that list is of terminal sessions only
    }
}

/// The terminal's variables in `ps -E` output (the command, then its environment, space-separated).
fn parse_env(ps: &str) -> HashMap<String, String> {
    const KEYS: [&str; 7] = ["TERM_PROGRAM", "ITERM_SESSION_ID", "TMUX", "TMUX_PANE", "WEZTERM_PANE", "KITTY_WINDOW_ID", "KITTY_LISTEN_ON"];
    ps.split_whitespace().filter_map(|w| w.split_once('=')).filter(|(k, _)| KEYS.contains(k)).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// A Claude session's transcript when Cue never got its path from a hook (a quiet session):
/// `~/.claude/projects/<its folder, every character but letters and digits as "-">/<session id>.jsonl`.
/// A session resumed from another folder keeps writing where it began, so any project folder holding
/// its id is looked in next.
pub fn claude_transcript(cwd: &str, session_id: &str) -> Option<String> {
    let p = claude_transcript_path(cwd, session_id);
    if p.is_file() {
        return Some(p.to_string_lossy().into_owned());
    }
    let projects = p.parent()?.parent()?;
    super::transcript::find_claude_in(projects, session_id)
}

/// Where Claude Code writes a session's transcript, whether or not it's there (nothing is looked up).
pub fn claude_transcript_path(cwd: &str, session_id: &str) -> PathBuf {
    let dir: String = cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    claude_sessions_dir().with_file_name("projects").join(dir).join(format!("{session_id}.jsonl"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_terminal_from_ps() {
        let env = parse_env("/usr/local/bin/claude --resume TERM=xterm TERM_PROGRAM=Apple_Terminal TMUX_PANE=%3 SHELL=/bin/zsh");
        assert_eq!(env.get("TERM_PROGRAM").map(String::as_str), Some("Apple_Terminal"));
        assert_eq!(env.get("TMUX_PANE").map(String::as_str), Some("%3"));
        assert!(!env.contains_key("TERM") && !env.contains_key("SHELL"));
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cue-live-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lists_live_interactive_claude_sessions() {
        let d = tmp("reg");
        let me = std::process::id();
        std::fs::write(d.join(format!("{me}.json")), format!(r#"{{"pid":{me},"sessionId":"S1","name":"login-fix","cwd":"/x/cue","status":"busy","kind":"interactive","startedAt":10,"statusUpdatedAt":20}}"#)).unwrap();
        std::fs::write(d.join("999999.json"), r#"{"pid":999999,"sessionId":"GONE","cwd":"/x","status":"idle","kind":"interactive"}"#).unwrap();
        std::fs::write(d.join("1.json"), format!(r#"{{"pid":{me},"sessionId":"BG","cwd":"/x","status":"busy","kind":"background"}}"#)).unwrap();
        std::fs::write(d.join("1.key"), "not json").unwrap();
        let got = scan_claude(&d);
        assert_eq!(got.len(), 1, "a dead process and a non-interactive session are left out: {got:?}");
        assert_eq!((got[0].session_id.as_str(), got[0].name.as_str(), got[0].status.as_str(), got[0].since_ms), ("S1", "login-fix", "busy", 20));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Claude Code writes `<pid>.json` in place: read mid-write, it's skipped this time and read whole the next.
    #[test]
    fn a_registry_file_read_half_written_is_skipped_until_it_is_whole() {
        let d = tmp("half");
        let me = std::process::id();
        let whole = format!(r#"{{"pid":{me},"sessionId":"S1","cwd":"/x","status":"idle","kind":"interactive"}}"#);
        std::fs::write(d.join(format!("{me}.json")), &whole[..whole.len() / 2]).unwrap();
        std::fs::write(d.join(format!("{}.json", std::os::unix::process::parent_id())), r#"{"pid":"#).unwrap();
        assert!(scan_claude(&d).is_empty(), "half a record names no session");
        std::fs::write(d.join(format!("{me}.json")), &whole).unwrap();
        assert_eq!(scan_claude(&d).iter().map(|q| q.session_id.as_str()).collect::<Vec<_>>(), ["S1"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A run that keeps no session (Cue's btw, its slash-command reader) is listed by Claude, but isn't one.
    #[test]
    fn a_run_that_keeps_no_session_is_not_listed() {
        let d = tmp("throwaway");
        let mut helper = std::process::Command::new("/bin/sh").args(["-c", "sleep 30; :", "sh", "--no-session-persistence"]).spawn().unwrap();
        let (me, other) = (std::process::id(), helper.id());
        std::fs::write(d.join(format!("{me}.json")), format!(r#"{{"pid":{me},"sessionId":"S1","cwd":"/x","status":"idle","kind":"interactive","startedAt":1}}"#)).unwrap();
        std::fs::write(d.join(format!("{other}.json")), format!(r#"{{"pid":{other},"sessionId":"S1","cwd":"/tmp","status":"busy","kind":"interactive","startedAt":2}}"#)).unwrap();
        let got = scan_claude(&d);
        let _ = helper.kill();
        let _ = helper.wait();
        assert_eq!(got.iter().map(|q| q.pid).collect::<Vec<_>>(), [me as i32], "only the real session");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn finds_a_quiet_sessions_transcript_from_its_folder() {
        let d = tmp("proj");
        std::fs::create_dir_all(d.join("sessions")).unwrap();
        std::fs::create_dir_all(d.join("projects/-Users-me-code--agent-workspaces-data-provider")).unwrap();
        std::fs::write(d.join("projects/-Users-me-code--agent-workspaces-data-provider/S1.jsonl"), "").unwrap();
        std::env::set_var("CLAUDE_CONFIG_DIR", &d);
        let found = claude_transcript("/Users/me/code/.agent-workspaces/data_provider", "S1");
        let missing = claude_transcript("/Users/me/code/.agent-workspaces/data_provider", "S2");
        // Resumed from another folder, it still writes where it began.
        let moved = claude_transcript("/Users/me/elsewhere", "S1");
        std::env::remove_var("CLAUDE_CONFIG_DIR");
        assert!(found.is_some_and(|p| p.ends_with("-Users-me-code--agent-workspaces-data-provider/S1.jsonl")));
        assert_eq!(missing, None);
        assert!(moved.is_some_and(|p| p.ends_with("-Users-me-code--agent-workspaces-data-provider/S1.jsonl")));
        let _ = std::fs::remove_dir_all(&d);
    }
}
