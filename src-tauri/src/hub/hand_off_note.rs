//! Hand-off notes: Cue's own hand-off ask (a short note a fresh session can start from), a note a session wrote
//! itself, and going on from one in a new session ("Continue in a new session", or fresh from its last reply).
//! What's recorded about them as insights. The window's commands are in ipc/insights.rs; the timers that ask
//! ahead of a cache running out are hub/hand_off.rs and hub/cache_tick.rs.

use super::Hub;
use crate::db;
use crate::logs::steps;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// A session's log, if it can be read from this Mac: its own, or Cue's copy of another machine's. Another
/// machine's own path is never looked up here: "/home/…" is macOS's automounter, which can stall a stat
/// for good. (Checked by its name only: is_copy would stat it.)
pub(crate) fn local_log(h: &Hub, session_id: &str) -> Option<String> {
    let remote = h.session_origin(session_id).is_some_and(|o| !o.machine.is_empty());
    let copies = crate::server::cue_dir().join("machines");
    super::steps_path(h, session_id).filter(|p| !remote || std::path::Path::new(p).starts_with(&copies))
}

/// A session wrote its hand-off note itself (steps::handoff_note): kept like Cue's own hand-off, with the
/// note's path, so its card offers "Continue in a new session". Called as its turn ends. True when Cue
/// took it (the caller redraws).
pub(crate) fn notice_handoff(h: &Hub, session_id: &str) -> bool {
    local_log(h, session_id).is_some_and(|log| notice_handoff_in(session_id, &log))
}

/// notice_handoff, from the session's log as read here (its own, or Cue's copy of a machine's). The
/// note's path is only passed on, never looked up: on another machine it's that machine's.
pub(crate) fn notice_handoff_in(session_id: &str, log: &str) -> bool {
    let kept = db::handoff(session_id);
    let written = steps::handoff_note(log);
    // A note asked for ahead, and you've written to its session since (a prompt of yours after the note, or
    // after the ask when no note was seen: it wrote none, or wrote it where Cue couldn't see), and not the ask
    // itself: you went on here, so it's dropped. A note still being written is never dropped.
    if let Some(k) = kept.as_ref().filter(|k| k.ahead > 0 && k.next.is_empty()) {
        let (prompted, asked) = steps::last_prompt(log);
        if !asked && prompted > 0 && prompted > (if k.note_ms > 0 { k.note_ms } else { k.sent_ms }) {
            db::handoff_drop(session_id);
            record_insight(session_id, "ahead", "acted", "continue", 0);
            return true;
        }
    }
    let Some((note, at_ms)) = written else { return false };
    let Some(fresh) = note_is_new(kept.as_ref(), at_ms) else { return false };
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

/// Cue's own hand-off, for a host with no hand-off skill (or when you choose it): what it's kept as.
pub(crate) const CUE_HANDOFF: &str = "cue:handoff";

/// What Cue's own hand-off sends: write a short note a fresh session can start from, in `{file}`. Starts
/// with steps::HANDOFF_ASK_START, which is how a log's turn is known as the note's.
const HANDOFF_ASK: &str = "Hand off: write a short note so a fresh session can carry on from here.

Put it in {file} at the project root (the git top level, else this folder), replacing any older one. Keep it under about 60 lines, plain and specific:
- Goal: what we're trying to get done, in a sentence or two.
- Where it stands: what works and how it was checked; uncommitted or unpushed work, and where it is.
- Done in this session: the changes that matter, by file.
- Next: the very next step, concrete enough to start without asking, then the rest.
- Open decisions: anything waiting on me, with the options and your recommendation.
- Key files, and gotchas: what bit us, rules I set, commands that must or mustn't be run.

Facts only: no pasted logs or long code, and never secrets or credentials. Don't commit, stage or change code: writing {file} is the only change, and it stays a local note. Then reply in a line or two with where the note is.";

/// The note Cue's hand-off asks a session for: one per session (by the start of its id), so two sessions
/// in one folder don't write over each other's.
fn note_file(session_id: &str) -> String {
    let short: String = session_id.chars().filter(char::is_ascii_alphanumeric).take(8).collect();
    if short.is_empty() { "HANDOFF.md".into() } else { format!("HANDOFF-{short}.md") }
}

fn handoff_ask(session_id: &str) -> String {
    HANDOFF_ASK.replace("{file}", &note_file(session_id))
}

/// The message that starts the next session, when its hand-off skill doesn't take "resume" (or it was
/// Cue's own): pick up the note, check it, carry on. `last`: what the old session last said. `noted`: it
/// wrote a hand-off note; otherwise (a new session in place of a closed one) its last reply is all there is.
/// `note`: where Cue saw it written ("" when it didn't: then `file`, the name it was asked for).
fn resume_ask(last: &str, noted: bool, note: &str, file: &str) -> String {
    // SHORTCUT: the reply's first 1500 characters; a longer one loses its end. Upgrade path: its last
    // paragraphs, where a hand-off says where the note is.
    let last: String = last.chars().take(1500).collect();
    let said = if last.is_empty() { String::new() } else { format!("\n\nIts last reply:\n\n{}", last.lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n")) };
    if !noted {
        return format!("Carry on from an earlier session in this folder.{said}\n\nIt left no hand-off note. Check git status, the branch and the files that reply names, say briefly where things stand, then ask me what to pick up.");
    }
    let at = if note.is_empty() { format!(" ({file} at the project root, unless that reply says it's elsewhere)") } else { format!(" at {note}") };
    format!("Carry on from the previous session's hand-off.{said}\n\nRead its hand-off note{at}. Check it against git status, the branch and the files it names, and say briefly what still matches and what doesn't. Then carry on with its next step, unless an open decision needs me first: then ask only that. Leave the note in place until its work is done.")
}

pub(crate) fn handoff_cue_now(h: &Hub, session_id: &str) -> Result<String, String> {
    handoff_cue_ahead(h, session_id, 0)
}

/// Who asked for a note before the cache ran out (the handoffs table's `ahead`): you, a session's "Hand off if
/// I don't come back", Settings, the Desk's rule for a project with a lead (hub/cache_tick.rs desk_due); or, not
/// for the cache, the Desk's hand-off of a session over its context mark or whose compacting failed (full_due),
/// which goes on in a fresh session from the note as soon as it's written (desk/hub/lead/handoff.rs).
pub(crate) const AHEAD_YOU: u8 = 1;

pub(crate) const AHEAD_SESSION: u8 = 2;

pub(crate) const AHEAD_ALL: u8 = 3;

pub(crate) const AHEAD_DESK: u8 = 4;

pub(crate) const AHEAD_FULL: u8 = 5;

const AHEAD_BY: [&str; 6] = ["", "you", "session", "all", "desk", "full"];

/// handoff_cue, with who asked ahead of its cache running out (0: not ahead).
pub(crate) fn handoff_cue_ahead(h: &Hub, session_id: &str, ahead: u8) -> Result<String, String> {
    let r = h.tell(session_id, &handoff_ask(session_id), crate::sessions::Kind::HandOff)?;
    // Asked unasked (its tick, or Settings) and it began working in the meantime: Cue kept the ask to send when
    // that turn ends, when you're there. Taken back: held, not sent.
    if ahead > AHEAD_YOU && r.starts_with("kept in Cue") {
        h.unhold(session_id);
        return Err("busy".into());
    }
    db::handoff_sent(session_id, CUE_HANDOFF, false, ahead);
    if ahead > 0 {
        let tokens = local_log(h, session_id).and_then(|l| steps::insight_state(&l)).map_or(0, |st| st.tokens);
        record_insight(session_id, "ahead", "shown", AHEAD_BY[ahead as usize], tokens);
    } else {
        record_insight(session_id, "handoff", "acted", "cue", 0);
    }
    h.redraw();
    Ok(r)
}

pub(crate) fn handoff_keep_now(h: &Hub, session_id: &str) {
    if db::handoff(session_id).is_some_and(|k| k.ahead > 0 && k.next.is_empty()) {
        db::handoff_drop(session_id);
        record_insight(session_id, "ahead", "acted", "continue", 0);
        h.redraw();
    }
}

/// A new session in place of `session_id`: the same agent, folder and machine (Claude Code: the same
/// model), started with `first`, given what the old one last said, and named after it, numbered on
/// (next_name). Returns the new session's id.
fn start_after(h: &Arc<Hub>, session_id: &str, first: impl FnOnce(&str) -> String) -> Result<String, String> {
    let o = h.session_origin(session_id).ok_or("Cue has no record of that session")?;
    let log = local_log(h, session_id);
    let model = (o.harness == "claude").then(|| log.as_deref().and_then(steps::model_of)).flatten();
    // A hosted lead carries on hosted (its team with it: `carry_team`).
    let opts = super::StartOpts { extra: model.map(|m| format!("--model {m}")).unwrap_or_default(), machine: o.machine.clone(), hosted: o.hosted, ..Default::default() };
    let first = first(&log.as_deref().and_then(steps::last_say).unwrap_or_default());
    let name = next_name(&h.session_name(session_id), &crate::model::folder_of(&o.cwd), &h.desk_item_names(session_id), &h.names_taken());
    h.cue_says(&first, crate::sessions::Kind::HandOff);
    let started = super::start_session_with(h, &o.harness, &o.cwd, &first, &name, &opts)?;
    let new_id = started.get("session_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    crate::config::carry_move(session_id, &new_id);
    h.carry_team(session_id, &new_id);
    // Its place on its project's Desk too: the lead, the items it holds, the Desk's list.
    if !new_id.is_empty() {
        h.desk_carry(session_id, &new_id, &name);
    }
    Ok(new_id)
}

/// The name of the session that carries on from one named `name` (else from its folder): "cue-ios" →
/// "cue-ios-2", "cue-ios-2" → "cue-ios-3". A bracketed tag in front ("[WIP] …") isn't carried over. A session named
/// after an item it holds (`items`: "cue-744") goes on as "cue-744-2": its number is the item's, not a count (#752).
/// The number goes on past any name an open or parked session already has (`taken`), so no two share one.
fn next_name(name: &str, folder: &str, items: &[String], taken: &[String]) -> String {
    let name = name.trim();
    let name = name.strip_prefix('[').and_then(|r| r.split_once(']')).map(|(_, rest)| rest.trim()).unwrap_or(name);
    let base = if name.is_empty() { folder.trim() } else { name };
    if base.is_empty() {
        return String::new();
    }
    let (stem, from) = match base.rsplit_once('-') {
        _ if items.iter().any(|i| i.eq_ignore_ascii_case(base)) => (base, 2),
        Some((stem, n)) if !stem.is_empty() && !n.is_empty() && n.len() <= 3 && n.chars().all(|c| c.is_ascii_digit()) => (stem, n.parse::<u32>().unwrap_or(1) + 1),
        _ => (base, 2),
    };
    (from..).map(|n| format!("{stem}-{n}")).find(|n| !taken.iter().any(|t| t.eq_ignore_ascii_case(n))).unwrap_or_default()
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
    let ahead = db::handoff(session_id).map_or(0, |k| k.ahead);
    let started = start_after(h, session_id, |last| if takes_resume { format!("/{skill} resume") } else { resume_ask(last, true, &note, &note_file(session_id)) });
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
    if ahead > 0 {
        record_insight(session_id, "ahead", "acted", "fresh", 0);
    }
    // Every way to a fresh session closes the old one, a note written ahead's too: left open, its cache renewed by
    // the note, it would be handed off again an hour later.
    let (closed, detail) = close_replaced(h, session_id, "Continued in a new session");
    Ok(serde_json::json!({ "session_id": new_id, "closed": closed, "detail": detail }))
}

/// The session a fresh one now carries on from is closed (Closed keeps it), unless its background agents are
/// still at work or it can't be closed now; then its card says where it continued and offers Close. Only an
/// open one: a parked or closed session stays where it is. Returns whether it closed, and what to say (`done`
/// first).
fn close_replaced(h: &Arc<Hub>, session_id: &str, done: &str) -> (bool, String) {
    // Open in Cue (a parked or closed one isn't among its sessions).
    let open = h.session_origin(session_id).is_some();
    if !open {
        h.redraw();
        return (false, done.to_string());
    }
    let log = local_log(h, session_id);
    let busy = log.as_deref().is_some_and(|p| steps::running_background(p) > 0);
    let close = if busy { Err("its background agents are still at work".to_string()) } else { super::end_session(h, session_id) };
    h.redraw();
    match close {
        Ok(_) => (true, format!("{done}; the old one is closed (Closed keeps it)")),
        Err(e) => (false, format!("{done}. The old one is still open ({e}): close it from its item in Waiting")),
    }
}

pub(crate) fn handoff_fresh_now(h: &Arc<Hub>, session_id: &str) -> Result<serde_json::Value, String> {
    let new_id = start_after(h, session_id, |last| resume_ask(last, false, "", ""))?;
    let (closed, detail) = close_replaced(h, session_id, "Started fresh from its last reply");
    Ok(serde_json::json!({ "session_id": new_id, "closed": closed, "detail": detail }))
}

/// Record one insight event: known rules and actions only, the detail kept short.
pub(crate) fn record_insight(session_id: &str, rule: &str, action: &str, detail: &str, tokens: u64) {
    // "ahead": a note asked before a cache ran out (shown, with who asked), what you did when you came back
    // (acted: "continue" here or "fresh" from the note), and held (a timer that didn't send, and why).
    const RULES: [&str; 6] = ["handoff", "cache", "queue", "autocompact", "delegate", "ahead"];
    const ACTIONS: [&str; 5] = ["shown", "acted", "later", "dismissed", "held"];
    if RULES.contains(&rule) && ACTIONS.contains(&action) {
        db::insight_event(session_id, rule, action, &detail.chars().take(80).collect::<String>(), tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_ask_quotes_the_last_reply() {
        let m = resume_ask("Note is in HANDOFF.md.\nNext: run the tests.", true, "", "HANDOFF.md");
        assert!(m.contains("> Note is in HANDOFF.md.\n> Next: run the tests."));
        assert!(m.starts_with("Carry on from the previous session's hand-off."));
        assert!(!resume_ask("", true, "", "HANDOFF.md").contains("Its last reply"));
        assert!(resume_ask("Done.", false, "", "").contains("It left no hand-off note"));
    }

    #[test]
    fn resume_ask_names_the_note_where_it_was_written() {
        let m = resume_ask("Done.", true, "/home/me/proj/docs/mac-security-handoff.md", "HANDOFF-ab12cd34.md");
        assert!(m.contains("Read its hand-off note at /home/me/proj/docs/mac-security-handoff.md. Check it"), "{m}");
        assert!(!m.contains("at the project root"));
        let old = resume_ask("Done.", true, "", "HANDOFF-ab12cd34.md");
        assert!(old.contains("Read its hand-off note (HANDOFF-ab12cd34.md at the project root, unless that reply says it's elsewhere). Check it"), "{old}");
    }

    #[test]
    fn each_session_is_asked_for_a_note_of_its_own() {
        assert_eq!(note_file("3081f22f-1c2d-4e5f"), "HANDOFF-3081f22f.md");
        assert_eq!(note_file(""), "HANDOFF.md");
        let ask = handoff_ask("3081f22f-1c2d-4e5f");
        assert!(ask.contains("Put it in HANDOFF-3081f22f.md at the project root") && ask.contains("writing HANDOFF-3081f22f.md is the only change"), "{ask}");
        assert!(!ask.contains("{file}"));
        assert!(ask.starts_with(steps::HANDOFF_ASK_START), "the log reader knows the note's turn by its start");
        assert!(steps::is_handoff_note(&format!("/r/{}", note_file("3081f22f"))), "Cue sees the note it asked for written");
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
    fn a_note_written_ahead_goes_once_you_reply_in_its_session() {
        let _g = db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-ahead-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        db::reset();
        let log = dir.join("s1.jsonl");
        let prompt = |at: &str, text: &str| format!("{}\n", serde_json::json!({"type":"user","timestamp":at,"message":{"content":text}}));
        // The note is written after Cue asked for it (now), so its log's times are later.
        let call = serde_json::json!({"type":"assistant","timestamp":"2098-01-01T00:00:05.000Z","message":{"content":[{"type":"tool_use","id":"w1","name":"Write","input":{"file_path":"/r/HANDOFF-s1.md"}}]}});
        let result = serde_json::json!({"type":"user","timestamp":"2098-01-01T00:00:05.000Z","message":{"content":[{"type":"tool_result","tool_use_id":"w1","content":"ok"}]}});
        std::fs::write(&log, format!("{}{call}\n{result}\n", prompt("2098-01-01T00:00:00.000Z", &handoff_ask("s1")))).unwrap();
        let p = log.to_str().unwrap();
        // Asked ahead (sent before the note, as the timer does), then its note seen: kept, still ahead.
        db::handoff_sent("s1", CUE_HANDOFF, false, AHEAD_SESSION);
        assert!(notice_handoff_in("s1", p));
        let h = db::handoff("s1").unwrap();
        assert_eq!((h.ahead, h.note.as_str()), (AHEAD_SESSION, "/r/HANDOFF-s1.md"));
        assert!(!notice_handoff_in("s1", p), "nothing new while you're away");
        // You came back and replied there: continued here, so the note written ahead is dropped.
        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        std::io::Write::write_all(&mut f, prompt("2099-01-01T00:00:00.000Z", "are you done?").as_bytes()).unwrap();
        assert!(notice_handoff_in("s1", p));
        assert!(db::handoff("s1").is_none());
        // A hand-off you sent yourself (not ahead) isn't dropped by a reply.
        db::handoff_sent("s1", CUE_HANDOFF, false, 0);
        assert!(!notice_handoff_in("s1", p));
        assert!(db::handoff("s1").is_some());
        std::env::remove_var("CUE_HOME");
        db::reset();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_note_asked_ahead_is_kept_while_written_and_goes_only_when_you_write_to_it() {
        let _g = db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-ahead2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        db::reset();
        let line = |v: serde_json::Value| format!("{v}\n");
        let prompt = |at: &str, text: &str| line(serde_json::json!({"type":"user","timestamp":at,"message":{"content":text}}));
        let tool = |at: &str, id: &str, name: &str, file: &str| line(serde_json::json!({"type":"assistant","timestamp":at,"message":{"content":[{"type":"tool_use","id":id,"name":name,"input":{"file_path":file}}]}}))
            + &line(serde_json::json!({"type":"user","timestamp":at,"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}}));
        let append = |log: &std::path::Path, text: &str| std::io::Write::write_all(&mut std::fs::OpenOptions::new().append(true).open(log).unwrap(), text.as_bytes()).unwrap();

        // It handed off once before (a note seen then), and is asked again ahead: the old note's time is gone
        // with the new ask, so a look before the new note is written keeps it.
        let log = dir.join("s1.jsonl");
        std::fs::write(&log, prompt("2026-10-04T10:00:00.000Z", "hand off") + &tool("2026-10-04T10:00:05.000Z", "w0", "Write", "/r/HANDOFF-s1.md")).unwrap();
        let p = log.to_str().unwrap();
        assert!(notice_handoff_in("s1", p));
        db::handoff_continued("s1", "old-next");
        db::handoff_sent("s1", CUE_HANDOFF, false, AHEAD_SESSION);
        assert_eq!(db::handoff("s1").unwrap().note_ms, 0, "a new ask forgets the old note's time");
        append(&log, &prompt("2098-01-01T00:00:00.000Z", &handoff_ask("s1")));
        assert!(!notice_handoff_in("s1", p), "still being written: nothing to do");
        assert_eq!(db::handoff("s1").map(|k| k.ahead), Some(AHEAD_SESSION), "kept while it writes");
        // It edits its older note into the new one (in the ask's turn): that's its note.
        append(&log, &tool("2098-01-01T00:00:09.000Z", "e1", "Edit", "/r/HANDOFF-s1.md"));
        assert!(notice_handoff_in("s1", p));
        let k = db::handoff("s1").unwrap();
        assert_eq!((k.ahead, k.note.as_str()), (AHEAD_SESSION, "/r/HANDOFF-s1.md"), "an Edit in the ask's turn is the note, still ahead");

        // Asked ahead, no note seen (it wrote none), and you wrote to it after: dropped, not stuck.
        let log2 = dir.join("s2.jsonl");
        db::handoff_sent("s2", CUE_HANDOFF, false, AHEAD_SESSION);
        std::fs::write(&log2, prompt("2098-01-01T00:00:00.000Z", &handoff_ask("s2"))).unwrap();
        let p2 = log2.to_str().unwrap();
        assert!(!notice_handoff_in("s2", p2));
        append(&log2, &prompt("2098-01-01T01:30:00.000Z", "where are we?"));
        assert!(notice_handoff_in("s2", p2));
        assert!(db::handoff("s2").is_none(), "you went on here");
        // An Edit of a note file outside the ask's turn is still not a hand-off of its own.
        let log3 = dir.join("s3.jsonl");
        std::fs::write(&log3, prompt("2098-01-01T00:00:00.000Z", "tidy the notes") + &tool("2098-01-01T00:00:05.000Z", "e3", "Edit", "/r/HANDOFF.md")).unwrap();
        assert!(!notice_handoff_in("s3", log3.to_str().unwrap()));
        assert!(db::handoff("s3").is_none());
        std::env::remove_var("CUE_HOME");
        db::reset();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_next_session_is_numbered_on_from_the_old_ones_name() {
        assert_eq!(next_name("cue-ios", "cue", &[], &[]), "cue-ios-2");
        assert_eq!(next_name("cue-ios-2", "cue", &[], &[]), "cue-ios-3");
        assert_eq!(next_name("", "cue", &[], &[]), "cue-2");
        assert_eq!(next_name("[Lead] cue-9", "cue", &[], &[]), "cue-10");
        assert_eq!(next_name("", "", &[], &[]), "");
    }

    /// #752: cue-744 handed #744 off while cue-745 (another item's session) was open, and the new one was named
    /// "cue-745" too. It goes on under its item's name, at the next number no session has.
    #[test]
    fn a_hand_off_takes_its_items_name_and_never_an_open_sessions() {
        let items = ["cue-744".to_string()];
        let taken: Vec<String> = ["cue-744", "cue-745", "cue-lead"].iter().map(|s| s.to_string()).collect();
        assert_eq!(next_name("cue-744", "cue", &items, &taken), "cue-744-2");
        let mut more = taken.clone();
        more.push("cue-744-2".into());
        assert_eq!(next_name("cue-744", "cue", &items, &more), "cue-744-3");
        // Handed off again, it counts on from its own number.
        assert_eq!(next_name("cue-744-2", "cue", &items, &more), "cue-744-3");
        // Not an item's name: still numbered on, but past one already open.
        assert_eq!(next_name("cue-ios", "cue", &[], &["cue-ios-2".into()]), "cue-ios-3");
        assert_eq!(next_name("CUE-9", "cue", &[], &["cue-10".into()]), "CUE-11");
    }
}
