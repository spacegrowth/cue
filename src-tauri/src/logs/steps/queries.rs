//! The open feeds, one per session log someone reads, and what the rest of Cue asks of them: the steps,
//! the background work running, the model, the last words, a hand-off note, the context; and the window
//! Claude Code's status line says, taken by the feeds that have that session open.

use super::feed::Feed;
use super::helper::{Helper, TaskFailure};
use super::kind::Kind;
use super::model::{Item, Turn};
use super::reading::{Applied, ROUNDS};
use super::windows::{remember_window, InsightState};
use super::FORGET;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// How Cue's own hand-off ask starts (ipc/insights.rs HANDOFF_ASK): a turn it began is the note's turn.
pub const HANDOFF_ASK_START: &str = "Hand off: write a short note";

/// One open feed: the feed under a lock of its own, where its log is read, and when it was last asked for.
struct Open {
    feed: Arc<Mutex<Feed>>,
    used: Instant,
}

/// The open feeds, by log path. Held only to find, file or forget one: never while a log is read, so a
/// log that's slow to read (a big one, a stalled disk) holds up only the queries about its own session.
static FEEDS: Mutex<Option<HashMap<String, Open>>> = Mutex::new(None);

/// The open feed for `path`, if there is one (`touch`: it's been asked for now). Feeds unasked for past
/// FORGET are dropped first.
fn find(path: &str, touch: bool) -> Option<Arc<Mutex<Feed>>> {
    let mut table = crate::locking::lock(&FEEDS, "FEEDS");
    let feeds = table.get_or_insert_with(HashMap::new);
    feeds.retain(|_, o| o.used.elapsed() < FORGET);
    let open = feeds.get_mut(path)?;
    if touch {
        open.used = Instant::now();
    }
    Some(open.feed.clone())
}

/// The feed for `path`, opened if none is (its log's start read with no lock held), and whether it's new.
fn open(path: &str) -> (Arc<Mutex<Feed>>, bool) {
    if let Some(feed) = find(path, true) {
        return (feed, false);
    }
    let new = Arc::new(Mutex::new(Feed::open(path, 1)));
    let mut table = crate::locking::lock(&FEEDS, "FEEDS");
    let open = table.get_or_insert_with(HashMap::new).entry(path.to_string()).or_insert(Open { feed: new, used: Instant::now() });
    (open.feed.clone(), true)
}

/// `f` on the feed for `path` (opened if need be, and whether it's new), caught up with its log (read with
/// its lock let go: `reading`).
fn caught_up<R>(path: &str, f: impl FnOnce(&mut Feed, bool) -> R) -> R {
    let (feed, fresh) = open(path);
    catch_up_shared(&feed);
    let mut feed = crate::locking::lock(&feed, "a session's feed");
    f(&mut feed, fresh)
}

/// Catch the shared feed up, its lock taken only to plan and to put in: never while a file is read. A read
/// dropped because another went in first is tried once more: the one that went in may have read less than
/// this one saw, and the caller would answer a poll late.
pub(super) fn catch_up_shared(feed: &Mutex<Feed>) {
    let mut retried = false;
    for _ in 0..ROUNDS + 1 {
        let plan = crate::locking::lock(feed, "a session's feed").plan();
        let reads = plan.fetch();
        match crate::locking::lock(feed, "a session's feed").apply(reads) {
            Applied::Again => {}
            Applied::Stale if !retried => retried = true,
            Applied::Done | Applied::Stale => return,
        }
    }
}

/// `f` on a feed kept elsewhere (a session's history), caught up with its log first.
pub(super) fn with_caught_up<R>(feed: &Mutex<Feed>, f: impl FnOnce(&Feed) -> R) -> R {
    catch_up_shared(feed);
    let feed = crate::locking::lock(feed, "a session's feed");
    f(&feed)
}

/// The open feeds whose log paths `pick` picks, for what looks across them.
fn all(pick: impl Fn(&str) -> bool) -> Vec<Arc<Mutex<Feed>>> {
    crate::locking::lock(&FEEDS, "FEEDS").get_or_insert_with(HashMap::new).iter().filter(|(p, _)| pick(p)).map(|(_, o)| o.feed.clone()).collect()
}

/// Tests: the feed for `path` forgotten, as one unasked for a while is.
#[cfg(test)]
pub(super) fn forget(path: &str) {
    crate::locking::lock(&FEEDS, "FEEDS").get_or_insert_with(HashMap::new).remove(path);
}

/// Tests: `feed` open for `path`, as if read there.
#[cfg(test)]
pub(super) fn put(path: &str, feed: Feed) {
    crate::locking::lock(&FEEDS, "FEEDS").get_or_insert_with(HashMap::new).insert(path.to_string(), Open { feed: Arc::new(Mutex::new(feed)), used: Instant::now() });
}

/// A session's background agent is still finished (not put back to work since); also when its feed is gone.
pub(super) fn helper_done(path: &str, id: &str) -> bool {
    let Some(feed) = find(path, false) else { return true };
    let feed = crate::locking::lock(&feed, "a session's feed");
    feed.helpers.iter().find(|h| h.id == id).is_none_or(|h| h.finished_ms != 0)
}

/// The session's turns and steps, newest last. `known`: the version the caller already has; when
/// nothing changed, only the version comes back.
pub fn steps(path: &str, known: u64) -> Value {
    // A feed nobody asked for in a while is dropped: a fresh one starts mid-file and counts its version
    // from 1 again. Its count can land exactly on the version the caller already has, and the "nothing
    // changed" answer would leave the caller's model and steps stale forever. A fresh feed answers in
    // full, whatever its count, so the caller redraws from what's there now.
    caught_up(path, |feed, fresh| {
        if feed.version == known && !fresh {
            return json!({ "version": feed.version });
        }
        let turns: Vec<&Turn> = feed.turns.iter().filter(|t| !t.items.is_empty()).collect();
        let meta = (!feed.meta.model.is_empty() || feed.meta.context > 0).then_some(&feed.meta);
        // Each helper with whether it's still at work, and stuck (read from its log and the clock, not fields).
        let now = crate::model::now_ms();
        let helpers: Vec<Value> = feed.helpers.iter().map(|h| helper_json(h, now)).collect();
        json!({ "version": feed.version, "turns": turns, "meta": meta, "helpers": helpers })
    })
}

/// A helper as the window reads it: with whether it's still at work, and stuck (read from its log and the
/// clock, not fields).
pub(super) fn helper_json(h: &Helper, now: u64) -> Value {
    let mut v = serde_json::to_value(h).unwrap_or(Value::Null);
    v["running"] = json!(h.running());
    v["stuck"] = json!(h.stuck(now));
    // Quiet (sessions/quiet.rs): "tool" with the step it has open, or "stuck".
    v["quiet"] = json!(h.quiet(now));
    v["quiet_what"] = json!(h.open_step());
    v
}

/// The background agents a Claude Code session has running right now: their descriptions.
/// (The session is at its prompt while they run; Claude Code wakes it when they report.)
pub fn running_helpers(path: &str) -> Vec<String> {
    caught_up(path, |feed, _| {
        feed.helpers.iter().filter(|h| h.running() && !h.script).map(|h| h.desc.clone()).collect()
    })
}

/// How many background tasks a Claude Code session has running right now: its agents and its shell
/// commands in the background alike (the session's card counts them).
pub fn running_background(path: &str) -> usize {
    caught_up(path, |feed, _| {
        feed.helpers.iter().filter(|h| h.running()).count()
    })
}

/// How many agents a session has: the background agents and teammates (not its scripts) in its feed, the same
/// list the window shows as its agents. `open`: start its feed
/// when there's none (a session at work); else None while nobody reads it, and a feed only this asks for isn't kept
/// alive by it.
pub fn agent_count(path: &str, open: bool) -> Option<usize> {
    let count = |feed: &mut Feed, _| feed.helpers.iter().filter(|h| !h.script).count();
    if open {
        return Some(caught_up(path, count));
    }
    let feed = find(path, false)?;
    catch_up_shared(&feed);
    let n = count(&mut crate::locking::lock(&feed, "a session's feed"), false);
    Some(n)
}

/// What one of a session's agents goes by (a teammate its name, a background agent its task), by its id; "" when its
/// feed doesn't list it (yet).
pub fn agent_name(path: &str, id: &str) -> String {
    caught_up(path, |feed, _| {
        feed.helpers.iter().find(|h| h.id == id).map(|h| if h.teammate && !h.name.is_empty() { h.name.clone() } else { h.desc.clone() }).unwrap_or_default()
    })
}

/// The background tasks that failed after a while since last asked, across every open feed (each is
/// given out once). Nothing is read here: a failure is noted when its session's feed reads the notice.
pub fn take_task_failures() -> Vec<TaskFailure> {
    all(|_| true).iter().flat_map(|f| std::mem::take(&mut crate::locking::lock(f, "a session's feed").failures)).collect()
}

/// The model a session last answered with ("claude-opus-5-5"), from its transcript.
pub fn model_of(path: &str) -> Option<String> {
    caught_up(path, |feed, _| {
        Some(model_for_window(&feed.meta.model, feed.meta.window)).filter(|m| !m.is_empty())
    })
}

/// A model to start Claude Code on with the same window: the transcript names a 1M session's model without
/// "[1m]", and `--model` without it starts one on the standard window.
pub(super) fn model_for_window(model: &str, window: u64) -> String {
    if model.is_empty() || window < 1_000_000 || model.ends_with("[1m]") {
        model.to_string()
    } else {
        format!("{model}[1m]")
    }
}

/// What the session last said (its latest turn's last words), for the session that continues its hand-off.
pub fn last_say(path: &str) -> Option<String> {
    caught_up(path, |feed, _| {
        let turn = feed.turns.last()?;
        turn.items.iter().rev().find_map(|i| match i { Item::Say { text, .. } if !text.trim().is_empty() => Some(text.trim().to_string()), _ => None })
    })
}

/// A hand-off note: a file named HANDOFF.md, or any .md whose name says hand-off ("handoff", "hand-off" or
/// "hand_off", in any case), wherever it is. Only the name counts: another file in a handoff/ folder doesn't.
pub fn is_handoff_note(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
    name.ends_with(".md") && ["handoff", "hand-off", "hand_off"].iter().any(|w| name.contains(w))
}

/// Your latest prompt in a session's log: when, by its clock (0: none read), and whether it was Cue's hand-off ask.
pub fn last_prompt(path: &str) -> (u64, bool) {
    caught_up(path, |feed, _| {
        feed.prompted
    })
}

/// The hand-off note the session wrote itself, if its last work did: its newest Write of a hand-off note
/// (`is_handoff_note`) that went through, with no prompt of yours after it. Its path as the log names it
/// (on the session's machine: a string to pass on, never looked up here), and when, by the log's clock.
pub fn handoff_note(path: &str) -> Option<(String, u64)> {
    caught_up(path, |feed, _| {
        feed.note.clone()
    })
}

/// What Claude Code's status line says a session's window is. A feed that has it open takes it at
/// once (and counts a new version, so the window redraws its context). `for_model`: it also stands for
/// its model's window until a session says otherwise: this Mac's sessions, not a machine's (one there
/// may run the same model with another window, through another provider).
pub fn set_window(session_id: &str, window: u64, for_model: bool) {
    if window == 0 {
        return;
    }
    // This session's feeds only (a log is named after its session): a feed being read elsewhere holds no one else
    // up. None is held while the window is kept on disk.
    let feeds: Vec<_> = all(|p| std::path::Path::new(p).file_stem().and_then(|s| s.to_str()) == Some(session_id)).into_iter().filter(|f| crate::locking::lock(f, "a session's feed").kind == Kind::Claude).collect();
    let model = feeds.first().map(|f| crate::locking::lock(f, "a session's feed").meta.model.clone()).unwrap_or_default();
    remember_window(session_id, window, for_model.then_some(model.as_str()));
    for f in feeds {
        let mut f = crate::locking::lock(&f, "a session's feed");
        if f.meta.window != window {
            f.meta.window = window;
            f.version += 1;
        }
    }
}

pub fn insight_state(path: &str) -> Option<InsightState> {
    caught_up(path, |feed, _| {
        let m = &feed.meta;
        let until = if m.cache_ttl_s > 0 && m.last_call_ms > 0 { m.last_call_ms + m.cache_ttl_s * 1000 } else { 0 };
        (m.context > 0).then_some(InsightState { until_ms: until, tokens: m.context, start: m.start_context, window: m.window, last_ms: m.last_call_ms, cold_x: m.cold_x, cold_cents: m.cold_cents, auto_at: m.auto_at })
    })
}

/// How full the session's context is (the latest reply's tokens, as a share of the model's window), 0–100.
pub fn context_pct(path: &str) -> Option<u8> {
    caught_up(path, |feed, _| {
        let m = &feed.meta;
        (m.window > 0).then(|| (m.context * 100 / m.window).min(100) as u8)
    })
}
