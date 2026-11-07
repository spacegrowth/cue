//! The index of leads and executors (this Mac's and every machine's, merged): how it's rebuilt, and every
//! question the rest of Cue asks of it. INDEX is taken only here; the other files get the index handed in.

use super::close::{close_left_behind_in, close_plan_in, hand_left_behind_in, lead_close_in, open_executors, LeadClose};
use super::commands::{act_on, Act, Cmd};
use super::files::{claude_sessions_dir, end_scan, live_claude_sessions, pilead_dir, relay_dir};
use super::ledger::ledger_handoffs;
use super::pilead::scan_pilead;
use super::relay::scan_relay;
use super::remote::add_machines;
use super::verify::{verdict_of, verify_reported, VERIFIED};
use super::wake::missed_wake;
use super::Member;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

pub(super) static INDEX: Mutex<Option<HashMap<String, Member>>> = Mutex::new(None);

/// This Mac's part of INDEX, as last scanned.
static LOCAL: Mutex<Option<HashMap<String, Member>>> = Mutex::new(None);

/// Done executors whose lead missed the wake-up, as Cue session id; `lead_free` says whether a lead
/// (its Cue session id) is free to take a review.
/// `lead_free` runs after the index is let go: it may read the index itself (a lead's name, say), and a
/// second lock on INDEX from the same thread would wait forever.
pub fn missed(lead_free: impl Fn(&str) -> bool) -> Vec<String> {
    let waiting: Vec<(String, String)> = {
        let cur = crate::locking::lock(&INDEX, "INDEX");
        cur.as_ref().map(|i| i.iter().filter(|(_, m)| missed_wake(m, true)).map(|(sid, m)| (sid.clone(), m.lead.clone())).collect()).unwrap_or_default()
    };
    waiting.into_iter().filter(|(_, lead)| lead_free(lead)).map(|(sid, _)| sid).collect()
}

pub(super) fn scan(relay: &Path, pilead: &Path, claude_sessions: &Path) -> HashMap<String, Member> {
    let mut index = HashMap::new();
    scan_relay(relay, &live_claude_sessions(claude_sessions), &mut index);
    scan_pilead(pilead, &mut index);
    end_scan();
    index
}

/// Re-read both plugins' folders. True when anything changed (so the window should redraw).
pub fn refresh() -> bool {
    let fresh = scan(&relay_dir(), &pilead_dir(), &claude_sessions_dir());
    verify_reported(&fresh, &relay_dir());
    let verified = VERIFIED.swap(false, std::sync::atomic::Ordering::Relaxed);
    *crate::locking::lock(&LOCAL, "LOCAL") = Some(fresh);
    let changed = merge();
    carry_status_over();
    changed || verified
}

/// INDEX again from this Mac's part and every machine's (this Mac's wins a clash). True when it changed.
pub(super) fn merge() -> bool {
    let mut all = crate::locking::lock(&LOCAL, "LOCAL").clone().unwrap_or_default();
    add_machines(&mut all);
    let mut cur = crate::locking::lock(&INDEX, "INDEX");
    if cur.as_ref() == Some(&all) {
        return false;
    }
    *cur = Some(all);
    true
}

/// The hand-offs whose status-list switch was already looked at (old lead, new lead): each link once per run.
static CARRIED: Mutex<Option<HashSet<(String, String)>>> = Mutex::new(None);

/// A hand-off older than this isn't carried over when Cue first sees it (Cue starting up finds every old one).
pub(super) const FRESH_HANDOFF_MS: u64 = 3_600_000;

/// The leads in `index` that took over from another and weren't in `seen` yet, as (old, new), and only the ones that
/// started within `FRESH_HANDOFF_MS` of `now_ms` (a start time relay didn't give counts as old).
pub(super) fn new_successors(index: &HashMap<String, Member>, seen: &mut HashSet<(String, String)>, now_ms: u64) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = index
        .iter()
        .filter(|(_, m)| m.role == "lead" && !m.predecessor.is_empty())
        .filter(|(sid, m)| seen.insert((m.predecessor.clone(), (*sid).clone())) && m.started_ms > 0 && now_ms.saturating_sub(m.started_ms) < FRESH_HANDOFF_MS)
        .map(|(sid, m)| (m.predecessor.clone(), sid.clone()))
        .collect();
    out.sort();
    out
}

/// A relay hand-off Cue just learned of: the status list is on for the new lead if it was for the one before.
pub(super) fn carry_status_over() {
    let found = {
        let index = crate::locking::lock(&INDEX, "INDEX").clone().unwrap_or_default();
        let mut seen = crate::locking::lock(&CARRIED, "CARRIED");
        new_successors(&index, seen.get_or_insert_with(HashSet::new), crate::model::now_ms())
    };
    for (old, new) in found {
        crate::statuslist::carry_over(&old, &new);
    }
}

/// Whether Cue knows this session as a relay or pi-lead executor (here or on a machine): it belongs
/// to its lead, which resumes, replaces or closes it; Cue never brings it back by itself.
pub fn is_executor(session_id: &str) -> bool {
    crate::locking::lock(&INDEX, "INDEX").as_ref().is_some_and(|i| executor_in(i, session_id))
}

/// A relay lead (not an executor, not pi-lead's): its steps go to executors through relay itself.
pub fn is_relay_lead(session_id: &str) -> bool {
    crate::locking::lock(&INDEX, "INDEX").as_ref().is_some_and(|i| relay_lead_in(i, session_id))
}

pub(super) fn relay_lead_in(index: &HashMap<String, Member>, session_id: &str) -> bool {
    index.get(session_id).is_some_and(|m| m.role == "lead" && m.plugin == "relay")
}

pub(super) fn executor_in(index: &HashMap<String, Member>, session_id: &str) -> bool {
    index.get(session_id).is_some_and(|m| m.role == "executor")
}

/// An executor on a machine that runs in a tmux session of its own: (machine, that session's name).
/// None for anything else (a lead, a session on this Mac, or a listing that didn't name one).
pub fn exec_tmux(session_id: &str) -> Option<(String, String)> {
    exec_tmux_in(crate::locking::lock(&INDEX, "INDEX").as_ref()?, session_id)
}

pub(super) fn exec_tmux_in(index: &HashMap<String, Member>, session_id: &str) -> Option<(String, String)> {
    let m = index.get(session_id)?;
    (m.role == "executor" && !m.machine.is_empty() && !m.tmux_session.is_empty()).then(|| (m.machine.clone(), m.tmux_session.clone()))
}

/// Who handles this session's finished turn, given what its hook said (`hooked`). An executor
/// with an armed lead: "its relay lead (chart-lines)". An executor whose lead isn't armed: "",
/// because its report reaches nobody unless you see it. Anything else keeps the hook's answer.
/// (The Claude hook works out relay executors itself; Pi's extension doesn't.)
pub fn resolve_driver(session_id: &str, hooked: &str) -> String {
    let known = crate::locking::lock(&INDEX, "INDEX").as_ref().is_some_and(|i| i.get(session_id).is_some_and(|m| m.role == "executor"));
    if known { driven_by(session_id) } else { hooked.to_string() }
}

fn driven_by(session_id: &str) -> String {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    match cur.as_ref().and_then(|i| i.get(session_id)) {
        Some(m) if m.role == "executor" && m.lead_armed == Some(true) => {
            let who = if m.plugin == "pilead" { "pi-lead" } else { "relay" };
            format!("its {who} lead ({})", if m.lead_name.is_empty() { "lead" } else { &m.lead_name })
        }
        _ => String::new(),
    }
}

/// How to close this session through its own tool, if it has one: a relay / pi-lead executor is closed
/// by relay / pilead (it marks the executor closed and closes its tab). A lead that still has
/// executors can't be parked or moved (Close closes a relay lead's with it: see lead_close). None: not in a crew, or an
/// executor relay is already done with (closed, superseded) whose agent still runs (resumed in a
/// pane relay's record doesn't know): ending it is Cue's, as for any session.
pub fn close_plan(session_id: &str) -> Option<Result<Cmd, String>> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    close_plan_in(cur.as_ref()?, session_id)
}

/// How to close this relay lead and its team. None: not a relay lead (an executor, a pi-lead lead, a
/// session outside a crew).
pub fn lead_close(session_id: &str) -> Option<Result<LeadClose, String>> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    lead_close_in(cur.as_ref()?, session_id)
}

/// Close every executor left behind with this one (see left_behind): each one's Cue session id, name
/// and relay close.
pub fn close_left_behind(session_id: &str) -> Result<Vec<(String, String, Cmd)>, String> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    close_left_behind_in(cur.as_ref().ok_or("Cue hasn't read relay's sessions yet")?, session_id)
}

/// Hand every executor left behind with this one to a relay lead that's running (`to`, its Cue session
/// id) on the same machine: relay's `adopt` for each, run as that lead (relay takes its caller from
/// CLAUDE_CODE_SESSION_ID). Its reports wake that lead from then on.
pub fn hand_left_behind(session_id: &str, to: &str) -> Result<Vec<Cmd>, String> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    hand_left_behind_in(cur.as_ref().ok_or("Cue hasn't read relay's sessions yet")?, session_id, to)
}

/// Leads that handed off and are still open, among `known`: each one's session id → the lead that has
/// the work now (`to`: the one it handed off to, or the last of a chain of handoffs; its name, its
/// machine, how many executors it has now), when it handed off (`at_ms`), and how many handoffs on
/// that is (`hops`). Whether that lead still runs is the window's to see. Not a session that leads again.
pub fn handoffs(known: &HashSet<String>) -> Value {
    let logged = ledger_handoffs(&relay_dir().join("sessions.jsonl"));
    let cur = crate::locking::lock(&INDEX, "INDEX");
    let Some(index) = cur.as_ref() else { return Value::Object(Default::default()) };
    // Each lead that handed off → the lead it opened (its id, name, when, machine): relay's ledger on
    // this Mac, and the leads relay runs now (their markers name their predecessor; on another machine too).
    let mut next: HashMap<String, (String, String, u64, String)> = logged.iter().map(|(from, (to, project, at))| (from.clone(), (to.clone(), project.clone(), *at, String::new()))).collect();
    for (sid, m) in index.iter().filter(|(_, m)| m.role == "lead" && !m.predecessor.is_empty()) {
        next.insert(m.predecessor.clone(), (sid.clone(), m.name.clone(), m.started_ms, m.machine.clone()));
    }
    let mut out = serde_json::Map::new();
    for (old, first) in next.iter().filter(|(old, _)| known.contains(*old) && !index.get(*old).is_some_and(|o| o.role == "lead")) {
        // That lead may have handed off in turn: the work is with the last one.
        let (mut to, mut hops) = (first, 0);
        while let Some(n) = next.get(&to.0).filter(|n| n.0 != *old && hops < 20) {
            to = n;
            hops += 1;
        }
        let (sid, name, _, machine) = to;
        out.insert(old.clone(), serde_json::json!({ "to": sid, "name": name, "at_ms": first.2, "machine": machine, "executors": open_executors(index, sid).len(), "hops": hops + 1 }));
    }
    Value::Object(out)
}

/// A relay executor's worktree (where its staged changes are), from relay's session file.
#[cfg_attr(not(feature = "ext"), allow(dead_code))]
pub fn worktree(session_id: &str) -> Option<String> {
    let id = crate::locking::lock(&INDEX, "INDEX").as_ref()?.get(session_id).filter(|m| m.role == "executor" && m.plugin == "relay")?.id.clone();
    let v: Value = serde_json::from_str(&std::fs::read_to_string(relay_dir().join(&id).join("session.json")).ok()?).ok()?;
    v.get("worktree")?.as_str().filter(|w| !w.is_empty()).map(String::from)
}

/// Whether an executor is done and waiting for its lead's review (relay: reported or idle).
pub fn is_done(session_id: &str) -> bool {
    crate::locking::lock(&INDEX, "INDEX").as_ref().and_then(|i| i.get(session_id)).is_some_and(|m| m.role == "executor" && matches!(m.status.as_str(), "reported" | "idle"))
}

/// Whether a done executor's report has been reviewed already (see Member::reviewed).
pub fn is_reviewed(session_id: &str) -> bool {
    crate::locking::lock(&INDEX, "INDEX").as_ref().and_then(|i| i.get(session_id)).is_some_and(|m| m.role == "executor" && m.reviewed)
}

/// Every executor whose packet waits on you, with whether its lead is on auto mode: it reported and
/// nobody has committed its work yet, or relay parked it after a review and what it staged is still staged. Cue session id first.
pub fn needing_you() -> Vec<(String, Member, bool)> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    let Some(index) = cur.as_ref() else { return vec![] };
    index
        .iter()
        .filter(|(_, m)| m.role == "executor" && ((matches!(m.status.as_str(), "reported" | "idle") && !m.landed) || m.parked_staged))
        .map(|(sid, m)| (sid.clone(), m.clone(), index.get(&m.lead).and_then(|l| l.auto) == Some(true)))
        .collect()
}

/// The relay id and current packet of an executor, which is what a set-aside packet is remembered by.
pub fn packet_of(session_id: &str) -> Option<(String, u64)> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    let m = cur.as_ref()?.get(session_id).filter(|m| m.role == "executor")?;
    Some((m.id.clone(), m.packet?))
}

/// The tests that fill INDEX / REMOTE take turns (here and in the hub's).
#[cfg(test)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

/// This test's turn at TEST_LOCK, held until the guard goes.
#[cfg(test)]
pub fn test_turn() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// For the hub's tests: the crew as relay's files would give it.
#[cfg(test)]
pub fn install_index(index: HashMap<String, Member>) {
    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
}

/// For the hub's tests: a relay executor that reported to its lead L.
#[cfg(test)]
pub fn reported_exec() -> Member {
    Member { role: "executor".into(), plugin: "relay".into(), status: "reported".into(), lead: "L".into(), lead_armed: Some(true), id: "ex".into(), packet: Some(1), ..Default::default() }
}

/// A lead's or executor's name (relay's topic), else its session id.
pub fn name_of(session_id: &str) -> String {
    crate::locking::lock(&INDEX, "INDEX").as_ref().and_then(|i| i.get(session_id)).map(|m| m.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| session_id.to_string())
}

/// What `action` ("review", "diff", "close", "auto-on", "auto-off") means for this session: an executor's
/// review goes to its lead, its diff opens as a page; a relay lead's auto mode is switched by relay.
pub fn act(session_id: &str, action: &str) -> Result<Act, String> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    let m = cur.as_ref().and_then(|i| i.get(session_id)).ok_or("Cue doesn't know this session as a lead or executor")?;
    act_on(m, session_id, action)
}

/// What the window needs for the sessions it knows: each one's role, and for a lead the
/// executors still working for it (whether or not Cue has seen them).
pub fn view(known: &HashSet<String>) -> Value {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    let Some(index) = cur.as_ref() else { return Value::Object(Default::default()) };
    let mut out = serde_json::Map::new();
    for sid in known {
        let Some(m) = index.get(sid) else { continue };
        let mut v = serde_json::to_value(m).unwrap_or_default();
        if let Some(vf) = (m.role == "executor").then(|| verdict_of(m)).flatten() {
            v["verify"] = serde_json::to_value(vf).unwrap_or_default();
        }
        if m.role == "lead" {
            let mut execs = open_executors(index, sid);
            execs.sort_by(|a, b| a.1.updated.cmp(&b.1.updated));
            v["executors"] = execs
                .into_iter()
                .map(|(esid, e)| serde_json::json!({ "session_id": esid, "name": e.name, "model": e.model, "status": e.status, "packet": e.packet, "goal": e.goal, "outcome": e.outcome, "verify": verdict_of(e), "plugin": e.plugin, "machine": e.machine, "tmux_session": e.tmux_session, "setup": e.setup }))
                .collect();
        }
        out.insert(sid.clone(), v);
    }
    Value::Object(out)
}
