//! Insights: what the window reads to show them, and what it records about them. The rules themselves
//! live in the window (ui/insights.js): they work from each session's own log (its turns' tokens, its
//! context, when its cache runs out). What's sent to a session is only what you click: Cue's hand-off
//! ask, and the first message of the session a hand-off continues in.

use crate::db;
use crate::hub::Hub;
use crate::logs::steps;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tauri::State;

/// A session's log, if it can be read from this Mac: its own, or Cue's copy of another machine's. Another
/// machine's own path is never looked up here: "/home/…" is macOS's automounter, which can stall a stat
/// for good. (Checked by its name only: is_copy would stat it.)
fn local_log(h: &Hub, session_id: &str) -> Option<String> {
    let remote = h.session_origin(session_id).is_some_and(|o| !o.machine.is_empty());
    let copies = crate::server::cue_dir().join("machines");
    super::steps_path(h, session_id).filter(|p| !remote || std::path::Path::new(p).starts_with(&copies))
}

/// For each session asked about: when its cached context runs out (0: its log doesn't say), its context
/// now (what the next reply re-writes once the cache is gone), the context it started with, and when its
/// latest reply came. Waiting
/// cards count down from it and say how far a session has grown.
#[tauri::command]
pub(crate) async fn insight_state(hub: State<'_, Arc<Hub>>, session_ids: Vec<String>) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || insight_states(&h, &session_ids)).await.map_err(|e| e.to_string())
}

/// The numbers insight_state answers with (it reads the sessions' logs: call it off the main thread).
/// A hand-off note one of them wrote is taken here too (notice_handoff_in): a machine's log copy can
/// trail its turn's end, when Cue looks first.
pub(crate) fn insight_states(h: &Hub, session_ids: &[String]) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    let mut noticed = false;
    for sid in session_ids.iter().take(50) {
        let Some(log) = local_log(h, sid) else { continue };
        noticed |= notice_handoff_in(sid, &log);
        if let Some(st) = steps::insight_state(&log) {
            out.insert(sid.clone(), serde_json::json!(st));
        }
    }
    if noticed {
        h.redraw();
    }
    serde_json::Value::Object(out)
}

/// A session wrote its hand-off note itself (steps::handoff_note): kept like Cue's own hand-off, with the
/// note's path, so its card offers "Continue in a new session". Called as its turn ends. True when Cue
/// took it (the caller redraws).
pub(crate) fn notice_handoff(h: &Hub, session_id: &str) -> bool {
    local_log(h, session_id).is_some_and(|log| notice_handoff_in(session_id, &log))
}

/// notice_handoff, from the session's log as read here (its own, or Cue's copy of a machine's). The
/// note's path is only passed on, never looked up: on another machine it's that machine's.
fn notice_handoff_in(session_id: &str, log: &str) -> bool {
    let Some((note, at_ms)) = steps::handoff_note(log) else { return false };
    let Some(fresh) = note_is_new(db::handoff(session_id).as_ref(), at_ms) else { return false };
    db::handoff_noted(session_id, CUE_HANDOFF, &note, at_ms, fresh);
    true
}

/// Whether a hand-off note written at `at_ms` is news, given the hand-off Cue has for its session: None
/// when it's no newer than the note already kept, the hand-off sent, or its continuing (an old write
/// doesn't offer Continue again); else Some(true) for a new hand-off (none kept, or that one continued),
/// Some(false) when it's the note of the hand-off waiting to continue.
fn note_is_new(kept: Option<&db::Handoff>, at_ms: u64) -> Option<bool> {
    match kept {
        None => Some(true),
        Some(k) if at_ms <= k.note_ms || at_ms <= k.sent_ms.max(k.continued_ms) => None,
        Some(k) => Some(!k.next.is_empty()),
    }
}

/// Each host's hand-off skills for Claude Code (`hosts`: "" is this Mac, then your machines by name): a
/// list, or null while it's read for the first time; `reading` is true while a newer list is on its way
/// for any of them; `commands`: every command it has (name, kind, desc), for choosing one in Settings. `fresh`: every folder's command list is read again (after you add a skill somewhere),
/// so sessions' "/" menus and insights see it too, not only this list.
#[tauri::command]
pub(crate) async fn insight_host_skills(fresh: Option<bool>) -> Result<serde_json::Value, String> {
    let fresh = fresh.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        if fresh {
            crate::commands::reread_all();
        }
        let mut out = serde_json::Map::new();
        let mut all = serde_json::Map::new();
        let mut reading = false;
        for host in std::iter::once(String::new()).chain(crate::machines::list().into_iter().map(|m| m.name)) {
            out.insert(host.clone(), serde_json::json!(crate::commands::handoff_skills(&host)));
            // Everything it has, for choosing one in Settings (null while it's read).
            let cmds = crate::commands::for_folder(&crate::commands::home_folder(&host), &host)
                .map(|l| l.into_iter().map(|c| serde_json::json!({ "name": c.name, "kind": c.kind, "desc": c.desc })).collect::<Vec<_>>());
            all.insert(host.clone(), serde_json::json!(cmds));
            reading |= crate::commands::reading(&crate::commands::home_folder(&host), &host);
        }
        serde_json::json!({ "hosts": out, "commands": all, "reading": reading })
    })
    .await
    .map_err(|e| e.to_string())
}

/// Cue's own hand-off, for a host with no hand-off skill (or when you choose it): what it's kept as.
pub(crate) const CUE_HANDOFF: &str = "cue:handoff";

/// What Cue's own hand-off sends: write a short note a fresh session can start from.
const HANDOFF_ASK: &str = "Hand off: write a short note so a fresh session can carry on from here.

Put it in HANDOFF.md at the project root (the git top level, else this folder), replacing any older one. Keep it under about 60 lines, plain and specific:
- Goal: what we're trying to get done, in a sentence or two.
- Where it stands: what works and how it was checked; uncommitted or unpushed work, and where it is.
- Done in this session: the changes that matter, by file.
- Next: the very next step, concrete enough to start without asking, then the rest.
- Open decisions: anything waiting on me, with the options and your recommendation.
- Key files, and gotchas: what bit us, rules I set, commands that must or mustn't be run.

Facts only: no pasted logs or long code, and never secrets or credentials. Don't commit, stage or change code: writing HANDOFF.md is the only change, and it stays a local note. Then reply in a line or two with where the note is.";

/// The message that starts the next session, when its hand-off skill doesn't take "resume" (or it was
/// Cue's own): pick up the note, check it, carry on. `last`: what the old session last said. `noted`: it
/// wrote a hand-off note; otherwise (a new session in place of a closed one) its last reply is all there is.
/// `note`: where Cue saw it written ("" when it didn't).
fn resume_ask(last: &str, noted: bool, note: &str) -> String {
    // SHORTCUT: the reply's first 1500 characters; a longer one loses its end. Upgrade path: its last
    // paragraphs, where a hand-off says where the note is.
    let last: String = last.chars().take(1500).collect();
    let said = if last.is_empty() { String::new() } else { format!("\n\nIts last reply:\n\n{}", last.lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n")) };
    if !noted {
        return format!("Carry on from an earlier session in this folder.{said}\n\nIt left no hand-off note. Check git status, the branch and the files that reply names, say briefly where things stand, then ask me what to pick up.");
    }
    let at = if note.is_empty() { " (HANDOFF.md at the project root, unless that reply says it's elsewhere)".to_string() } else { format!(" at {note}") };
    format!("Carry on from the previous session's hand-off.{said}\n\nRead its hand-off note{at}. Check it against git status, the branch and the files it names, and say briefly what still matches and what doesn't. Then carry on with its next step, unless an open decision needs me first: then ask only that. Leave the note in place until its work is done.")
}

/// You sent a session its hand-off skill (`skill`, without the slash): remembered until it continues in a
/// new session, so "Continue in a new session" is still offered after a restart. `resume`: the skill takes
/// "resume" (its description says so); otherwise Cue's own message starts the next session.
#[tauri::command]
pub(crate) fn handoff_sent(hub: State<'_, Arc<Hub>>, session_id: String, skill: String, resume: Option<bool>) {
    let skill: String = skill.trim_start_matches('/').chars().take(120).collect();
    if !skill.is_empty() && !skill.contains(char::is_whitespace) {
        db::handoff_sent(&session_id, &skill, resume.unwrap_or(true) && skill != CUE_HANDOFF);
        hub.redraw();
    }
}

/// Cue's own hand-off: sends the session HANDOFF_ASK (your click is the message), queued if it's working.
#[tauri::command]
pub(crate) async fn handoff_cue(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<String, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || handoff_cue_now(&h, &session_id)).await.map_err(|e| e.to_string())?
}

pub(crate) fn handoff_cue_now(h: &Hub, session_id: &str) -> Result<String, String> {
    let r = h.send_to_session(session_id, HANDOFF_ASK, &[], false)?;
    db::handoff_sent(session_id, CUE_HANDOFF, false);
    record_insight(session_id, "handoff", "acted", "cue", 0);
    h.redraw();
    Ok(r)
}

/// "Continue in a new session": a new session of the same agent, in the same folder and on the same
/// machine (Claude Code: the same model), started with `/<skill> resume`, or Cue's own resume message when
/// the skill doesn't take one (your click is the message); then the old one closes (Closed keeps it),
/// unless background agents are still at work there. Linked both ways.
#[tauri::command]
pub(crate) async fn handoff_continue(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || handoff_continue_now(&h, &session_id)).await.map_err(|e| e.to_string())?
}

/// A new session in place of `session_id`: the same agent, folder and machine (Claude Code: the same
/// model), started with `first`, given what the old one last said, and named after it, numbered on
/// (next_name). Returns the new session's id.
fn start_after(h: &Arc<Hub>, session_id: &str, first: impl FnOnce(&str) -> String) -> Result<String, String> {
    let o = h.session_origin(session_id).ok_or("Cue has no record of that session")?;
    let log = local_log(h, session_id);
    let model = (o.harness == "claude").then(|| log.as_deref().and_then(steps::model_of)).flatten();
    let opts = super::StartOpts { extra: model.map(|m| format!("--model {m}")).unwrap_or_default(), machine: o.machine.clone(), ..Default::default() };
    let first = first(&log.as_deref().and_then(steps::last_say).unwrap_or_default());
    let name = next_name(&h.session_name(session_id), &crate::model::folder_of(&o.cwd));
    let started = super::start_session_with(h, &o.harness, &o.cwd, &first, &name, &opts)?;
    Ok(started.get("session_id").and_then(|v| v.as_str()).unwrap_or_default().to_string())
}

/// The name of the session that carries on from one named `name` (else from its folder): "cue-ios" →
/// "cue-ios-2", "cue-ios-2" → "cue-ios-3". A relay tag in front ("[Lead] …") isn't carried over.
fn next_name(name: &str, folder: &str) -> String {
    let name = name.trim();
    let name = name.strip_prefix('[').and_then(|r| r.split_once(']')).map(|(_, rest)| rest.trim()).unwrap_or(name);
    let base = if name.is_empty() { folder.trim() } else { name };
    match base.rsplit_once('-') {
        Some((stem, n)) if !stem.is_empty() && !n.is_empty() && n.len() <= 3 && n.chars().all(|c| c.is_ascii_digit()) => format!("{stem}-{}", n.parse::<u32>().unwrap_or(1) + 1),
        _ if base.is_empty() => String::new(),
        _ => format!("{base}-2"),
    }
}

/// Hand-offs continuing right now: a second click while the new session starts doesn't start another.
static CONTINUING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

pub(crate) fn handoff_continue_now(h: &Arc<Hub>, session_id: &str) -> Result<serde_json::Value, String> {
    let db::Handoff { skill, next, resume: takes_resume, note, .. } = db::handoff(session_id).ok_or("it hasn't handed off")?;
    if !next.is_empty() {
        return Err("it already continued in a new session".into());
    }
    let claimed = {
        let mut guard = crate::locking::lock(&CONTINUING, "CONTINUING");
        guard.get_or_insert_with(HashSet::new).insert(session_id.to_string())
    };
    if !claimed {
        return Err("it's already starting its new session".into());
    }
    let started = start_after(h, session_id, |last| if takes_resume { format!("/{skill} resume") } else { resume_ask(last, true, &note) });
    if let Some(new_id) = started.as_ref().ok().filter(|id| !id.is_empty()) {
        db::handoff_continued(session_id, new_id);
    }
    {
        let mut guard = crate::locking::lock(&CONTINUING, "CONTINUING");
        if let Some(set) = guard.as_mut() {
            set.remove(session_id);
        }
    }
    let new_id = started?;
    let log = local_log(h, session_id);
    let busy = log.as_deref().is_some_and(|p| steps::running_background(p) > 0);
    // Not closed (it's working, say): its hand-off card then says where it continued and offers Close.
    let close = if busy { Err("its background agents are still at work".to_string()) } else { super::end_session(h, session_id) };
    h.redraw();
    let detail = match &close {
        Ok(_) => "Continued in a new session; the old one is closed (Closed keeps it)".to_string(),
        Err(e) => format!("Continued in a new session. The old one is still open ({e}): close it from its hand-off card"),
    };
    Ok(serde_json::json!({ "session_id": new_id, "closed": close.is_ok(), "detail": detail }))
}

/// A parked or closed session whose cache is gone: a new session of the same agent, folder and machine
/// (Claude Code: the same model), started from its last reply instead of resuming the whole context.
/// The old one stays where it is (Parked or Closed).
#[tauri::command]
pub(crate) async fn handoff_fresh(hub: State<'_, Arc<Hub>>, session_id: String) -> Result<serde_json::Value, String> {
    let h = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let new_id = start_after(&h, &session_id, |last| resume_ask(last, false, ""))?;
        h.redraw();
        Ok(serde_json::json!({ "session_id": new_id, "detail": "Started fresh from its last reply" }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One thing an insight did, or you did with it: shown, acted (with what you picked), later, dismissed.
/// `tokens`: what acting on it saved, where that's known when it happens.
#[tauri::command]
pub(crate) fn insight_log(session_id: String, rule: String, action: String, detail: Option<String>, tokens: Option<u64>) {
    record_insight(&session_id, &rule, &action, &detail.unwrap_or_default(), tokens.unwrap_or(0));
}

/// Record one insight event: known rules and actions only, the detail kept short.
pub(crate) fn record_insight(session_id: &str, rule: &str, action: &str, detail: &str, tokens: u64) {
    const RULES: [&str; 4] = ["handoff", "cache", "queue", "autocompact"];
    const ACTIONS: [&str; 4] = ["shown", "acted", "later", "dismissed"];
    if RULES.contains(&rule) && ACTIONS.contains(&action) {
        db::insight_event(session_id, rule, action, &detail.chars().take(80).collect::<String>(), tokens);
    }
}

/// The last 30 days, per insight: shown, acted on, and the tokens acting on them saved.
#[tauri::command]
pub(crate) fn insight_counts() -> serde_json::Value {
    let since = crate::model::now_ms().saturating_sub(30 * 24 * 3600 * 1000);
    serde_json::Value::Array(db::insight_counts(since).into_iter().map(|(rule, shown, acted, tokens)| serde_json::json!({ "rule": rule, "shown": shown, "acted": acted, "tokens": tokens })).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_ask_quotes_the_last_reply() {
        let m = resume_ask("Note is in HANDOFF.md.\nNext: run the tests.", true, "");
        assert!(m.contains("> Note is in HANDOFF.md.\n> Next: run the tests."));
        assert!(m.starts_with("Carry on from the previous session's hand-off."));
        assert!(!resume_ask("", true, "").contains("Its last reply"));
        assert!(resume_ask("Done.", false, "").contains("It left no hand-off note"));
    }

    #[test]
    fn resume_ask_names_the_note_where_it_was_written() {
        let m = resume_ask("Done.", true, "/home/me/proj/docs/mac-security-handoff.md");
        assert!(m.contains("Read its hand-off note at /home/me/proj/docs/mac-security-handoff.md. Check it"), "{m}");
        assert!(!m.contains("HANDOFF.md at the project root"));
        let old = resume_ask("Done.", true, "");
        assert!(old.contains("Read its hand-off note (HANDOFF.md at the project root, unless that reply says it's elsewhere). Check it"), "{old}");
    }

    #[test]
    fn a_note_counts_only_when_newer_than_the_hand_off_kept() {
        let kept = |sent_ms, next: &str, continued_ms, note_ms| db::Handoff { skill: CUE_HANDOFF.into(), sent_ms, next: next.into(), continued_ms, note_ms, ..Default::default() };
        // None kept: a new hand-off.
        assert_eq!(note_is_new(None, 100), Some(true));
        // Waiting to continue: a note written after it was sent is its note; one from before isn't.
        assert_eq!(note_is_new(Some(&kept(100, "", 0, 0)), 150), Some(false));
        assert_eq!(note_is_new(Some(&kept(100, "", 0, 0)), 90), None);
        // Already continued: the old write doesn't offer Continue again; a newer one is a new hand-off.
        assert_eq!(note_is_new(Some(&kept(100, "new-1", 200, 150)), 150), None);
        assert_eq!(note_is_new(Some(&kept(100, "new-1", 200, 0)), 180), None);
        assert_eq!(note_is_new(Some(&kept(100, "new-1", 200, 150)), 250), Some(true));
        // The note already kept, read again (by a log clock ahead of the Mac's): not news.
        assert_eq!(note_is_new(Some(&kept(100, "", 0, 500)), 500), None);
    }

    #[test]
    fn a_note_the_session_wrote_offers_continue_once() {
        let _g = db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-noted-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        db::reset();
        let log = dir.join("s1.jsonl");
        // A Write of `file` and its result, as Claude Code logs them.
        let write = |at: &str, id: &str, file: &str| {
            let call = serde_json::json!({"type":"assistant","timestamp":at,"message":{"content":[{"type":"tool_use","id":id,"name":"Write","input":{"file_path":file}}]}});
            let result = serde_json::json!({"type":"user","timestamp":at,"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}});
            format!("{call}\n{result}\n")
        };
        let prompt = serde_json::json!({"type":"user","timestamp":"2026-10-04T10:00:00.000Z","message":{"content":"hand off"}});
        std::fs::write(&log, format!("{prompt}\n{}", write("2026-10-04T10:00:05.000Z", "w1", "/r/HANDOFF.md"))).unwrap();
        let p = log.to_str().unwrap();
        assert!(notice_handoff_in("s1", p));
        let h = db::handoff("s1").unwrap();
        assert_eq!((h.skill.as_str(), h.resume, h.note.as_str(), h.next.as_str()), (CUE_HANDOFF, false, "/r/HANDOFF.md", ""));
        assert!(!notice_handoff_in("s1", p), "the same note again is no news");
        // It continued; the old write doesn't bring Continue back, a new note does.
        db::handoff_continued("s1", "s2");
        assert!(!notice_handoff_in("s1", p));
        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        std::io::Write::write_all(&mut f, write("2099-01-01T00:00:00.000Z", "w2", "/r/notes/next-handoff.md").as_bytes()).unwrap();
        assert!(notice_handoff_in("s1", p));
        let h = db::handoff("s1").unwrap();
        assert_eq!((h.note.as_str(), h.next.as_str()), ("/r/notes/next-handoff.md", ""));
        std::env::remove_var("CUE_HOME");
        db::reset();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_next_session_is_numbered_on_from_the_old_ones_name() {
        assert_eq!(next_name("cue-ios", "cue"), "cue-ios-2");
        assert_eq!(next_name("cue-ios-2", "cue"), "cue-ios-3");
        assert_eq!(next_name("", "cue"), "cue-2");
        assert_eq!(next_name("[Lead] cue-9", "cue"), "cue-10");
        assert_eq!(next_name("", ""), "");
    }
}
