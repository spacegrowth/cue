//! Who leads whom. claude-relay (Claude Code) and pi-lead (Pi) run a lead session that hands work
//! packets to executor sessions; both keep that in their own state folders, keyed by the same
//! session ids Cue gets from its hooks. Cue reads those folders every few seconds so it can mark a
//! lead and its executors, and tell you an executor's finished turn is its lead's, not yours.
//!
//! relay (`~/.relay-tasks`): `lead/<claude session>/marker.json` while a lead is armed, and
//!   `<executor>/session.json` per executor (`claude_session`, `owner_lead`, …). A marker can
//!   outlive its lead (a crashed tab: relay's board calls it a ghost), so a lead counts as listening
//!   only while Claude Code's own registry, `~/.claude/sessions/<pid>.json`, has it open in a live process.
//! pi-lead (`$PI_LEAD_HOME`, default `~/.pi-lead`): `leads/<pi session>.json` per lead, and
//!   `sessions/<pi session>/meta.json` + `status` per executor (pi runs as `--session-id <sid>`).
//! Your machines (+ New → Machine): relay's own listing there (`relay list --all --json`), read
//!   over Cue's shared SSH connection while Cue has sessions there; relay's commands (close, auto,
//!   diff) run there too.
//!
//! `index` holds the merged index and answers what the rest of Cue asks; it's read from this Mac's folders
//! (`relay`, `pilead`) and your machines (`remote`), with `ledger`, `verify`, `git` and `wake` filling in
//! each executor; `commands` and `close` are what the window's buttons run.

mod close;
mod commands;
mod files;
mod git;
mod index;
mod ledger;
mod pilead;
mod relay;
mod remote;
mod verify;
mod wake;

pub use self::commands::{Act, Cmd};
pub use self::index::{
    act, close_left_behind, close_plan, exec_tmux, hand_left_behind, handoffs, is_done, is_executor, is_relay_lead, is_reviewed, lead_close, missed, name_of, needing_you, packet_of, refresh,
    resolve_driver, view,
};
#[cfg_attr(not(feature = "ext"), allow(unused_imports))]
pub use self::index::worktree;
#[cfg(test)]
pub use self::index::{install_index, reported_exec, test_turn, TEST_LOCK};
pub use self::remote::{refresh_remote, remote_known, remote_screens_known, remote_setup, retain_remote};

use self::files::s;
use self::verify::Verdict;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Member {
    /// "lead" or "executor".
    pub role: String,
    /// "relay" or "pilead".
    pub plugin: String,
    /// A lead's project name; an executor's topic.
    pub name: String,
    /// The lead's tab color, [r, g, b] (relay only; the window picks one for pi-lead).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Executor: busy / reported / idle / stalled / closed / superseded / dead …
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// Executor: the packet it's on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packet: Option<u64>,
    /// Executor: its lead's session id and project.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lead: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub lead_name: String,
    /// Executor: whether its lead is still armed (relay: its marker exists). A report from an
    /// executor whose lead isn't armed reaches nobody.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead_armed: Option<bool>,
    /// Lead (relay): the lead it took over from in a handoff (relay's `predecessor`), and when it
    /// started (ms). The one before steps down but stays open until you close it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub predecessor: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub started_ms: u64,
    /// Lead: relay's autonomous posture (`/relay:auto`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
    /// Lead (relay): its plan queue (`plan.json`): what's next and how many wait to be sent.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub plan_next: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub plan_queued: u64,
    /// Executor: its current packet's goal (the packet's "GOAL: …" line) and, once it reported, the
    /// report's first line (the outcome). What it's for and what it did, without asking a model.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub goal: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub outcome: String,
    /// Executor: relay's last `relay verify` verdict on this packet's report, as read from relay's
    /// ledger here or from `list --json` on a machine (the window gets it as `verify`, see `view`).
    #[serde(skip)]
    pub verify: Option<Verdict>,
    /// Executor: its id in relay / pi-lead (what their commands take), not Cue's session id.
    #[serde(skip)]
    pub id: String,
    /// The machine it runs on (one of yours, by name); "" for this Mac.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub machine: String,
    /// Executor on a machine: the name of its own detached tmux session there (relay's and pi-lead's
    /// `tmux_session`), which its terminal in Cue attaches to. "" when the listing doesn't say (an older
    /// relay / pi-lead, or not in tmux) or names one Cue won't pass to tmux (see `tmux_name`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tmux_session: String,
    /// Executor on a machine: its terminal is on a screen before Claude Code starts ("trust": asking
    /// whether you trust the folder; "enter": waiting for Enter), so it isn't working, whatever relay says.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub setup: String,
    /// Executor: relay has woken its lead for this packet's report, or the lead opened its diff
    /// (relay's `report_seen_packet`). Only that the lead was told: with auto off it announces and waits for you.
    #[serde(skip)]
    pub woken: bool,
    /// Executor: a review of this packet ran (relay ledgered `report_reviewed`, or saved
    /// `packets/<NNN>-review.md`).
    #[serde(skip)]
    pub reviewed: bool,
    /// Executor with a review: the saved review's text, "" when relay saved none (an inline review).
    #[serde(skip)]
    pub review: String,
    /// Executor: the staged diff's files, lines added and lines removed, when relay's ledger has them.
    #[serde(skip)]
    pub diff_size: Option<(u64, u64, u64)>,
    /// Executor: relay parked it after a review (`auto_closed: reviewed`) and what it staged is still
    /// staged in its worktree: nobody has committed or discarded it yet.
    #[serde(skip)]
    pub parked_staged: bool,
    /// Executor: a commit made after its report touched the files its packet staged (its lead committed it,
    /// by hand or through relay). It no longer waits on you, whatever relay's status still says.
    #[serde(skip)]
    pub landed: bool,
    /// Executor: its report has been ready (done: reported or idle) for MISSED_WAKE_SECS or more, going
    /// by relay's `updated`. Kept as a flag, not a time, so the window redraws when it turns true.
    #[serde(skip)]
    pub ready_late: bool,
    /// Sort key for duplicates (an executor id reused by a resume): the newest record wins.
    #[serde(skip)]
    pub(crate) updated: String,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Statuses that mean the executor is done with (closed, replaced): left out of a lead's list. (A
/// "dead" or "stalled" one isn't: its agent is gone but relay still lists it, so it stays under its
/// lead until you Close it.)
const GONE: &[&str] = &["closed", "superseded", "retired", "done"];

/// relay's leads by session id: each one's project and tab color, as its executors show them.
type RelayLeads = HashMap<String, (String, Option<[u8; 3]>)>;

/// A relay session id Cue may build a path from (`<relay>/<id>/...`): one folder name, never a path.
/// Relay only ever writes such ids; anything else came from a file Cue shouldn't trust.
fn safe_id(id: &str) -> bool {
    !id.is_empty() && !id.contains('/') && !id.contains('\\') && !id.contains("..")
}

/// The key an executor is filed under: its Claude session, or (a stalled one that never got as far
/// as having a session) relay's own id, so the lead's team still lists it and Close can reach it.
fn exec_key(e: &Value) -> String {
    let c = s(e, "claude_session");
    if c.is_empty() && matches!(s(e, "status").as_str(), "stalled" | "dead") && !s(e, "session_id").is_empty() { format!("relay:{}", s(e, "session_id")) } else { c }
}

fn put(index: &mut HashMap<String, Member>, sid: String, m: Member) {
    if sid.is_empty() {
        return;
    }
    if index.get(&sid).is_some_and(|old| old.updated > m.updated) {
        return;
    }
    index.insert(sid, m);
}

#[cfg(test)]
mod tests;
