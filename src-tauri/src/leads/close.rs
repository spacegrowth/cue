//! Closing through relay or pi-lead: an executor, a relay lead with its team, and executors whose lead is gone.
//! Each plan is worked out from the index the caller holds (see `index`).

use super::commands::{pilead_for, relay_for, Cmd};
use super::{safe_id, Member, GONE};
use std::collections::HashMap;

/// `close_plan` for a session in `index`.
pub(super) fn close_plan_in(index: &HashMap<String, Member>, session_id: &str) -> Option<Result<Cmd, String>> {
    let m = index.get(session_id)?;
    if m.role == "executor" && GONE.contains(&m.status.as_str()) {
        return None;
    }
    match (m.plugin.as_str(), m.role.as_str()) {
        ("relay", "executor") => Some(relay_close(m)),
        ("pilead", "executor") => Some(pilead_for(m).map(|c| Cmd { args: vec!["close".into(), m.id.clone()], ..c })),
        (_, "lead") => {
            let n = open_executors(index, session_id).len();
            (n > 0).then(|| Err(format!("it leads {n} executor{} still open: close them, or hand the lead off, first", if n == 1 { "" } else { "s" })))
        }
        _ => None,
    }
}

/// The executors still open under this lead, as Cue session id and record.
pub(super) fn open_executors<'a>(index: &'a HashMap<String, Member>, lead: &str) -> Vec<(&'a String, &'a Member)> {
    index.iter().filter(|(_, e)| e.role == "executor" && e.lead == lead && !GONE.contains(&e.status.as_str())).collect()
}

/// An executor's relay close, where it runs: relay stops it and closes its tab (what it staged stays
/// in its worktree).
fn relay_close(e: &Member) -> Result<Cmd, String> {
    relay_for(e).map(|c| Cmd { args: vec!["close".into(), e.id.clone()], ..c })
}

/// Closing a relay lead: relay closes each executor it still has, then steps the lead down (`relay
/// close --self <lead session id>`, where the lead runs) so relay stops thinking it leads; then the
/// lead is ended as any session.
#[derive(Debug, PartialEq)]
pub struct LeadClose {
    /// Each open executor: its Cue session id, its name, and its close.
    pub executors: Vec<(String, String, Cmd)>,
    pub stop: Cmd,
}

/// `lead_close` for a session in `index`.
pub(super) fn lead_close_in(index: &HashMap<String, Member>, session_id: &str) -> Option<Result<LeadClose, String>> {
    let m = index.get(session_id).filter(|m| m.plugin == "relay" && m.role == "lead")?;
    let plan = || -> Result<LeadClose, String> {
        let mut executors = Vec::new();
        for (sid, e) in open_executors(index, session_id) {
            executors.push((sid.clone(), e.name.clone(), relay_close(e)?));
        }
        let stop = relay_for(m).map(|c| Cmd { args: vec!["close".into(), "--self".into(), session_id.to_string()], ..c })?;
        Ok(LeadClose { executors, stop })
    };
    Some(plan())
}

/// The relay executors left behind with this one: the same lead, which isn't running any more, and
/// still open (this one included). Cue session id and record.
fn left_behind<'a>(index: &'a HashMap<String, Member>, session_id: &str) -> Vec<(&'a String, &'a Member)> {
    let Some(m) = index.get(session_id).filter(|m| m.role == "executor" && m.plugin == "relay" && m.lead_armed == Some(false)) else { return vec![] };
    index.iter().filter(|(_, e)| e.role == "executor" && e.plugin == "relay" && e.lead == m.lead && e.machine == m.machine && e.lead_armed == Some(false) && !GONE.contains(&e.status.as_str())).collect()
}

/// `close_left_behind` for a session in `index`.
pub(super) fn close_left_behind_in(index: &HashMap<String, Member>, session_id: &str) -> Result<Vec<(String, String, Cmd)>, String> {
    let left = left_behind(index, session_id);
    if left.is_empty() {
        return Err("its lead is still running, or it's already closed".into());
    }
    left.into_iter().map(|(sid, e)| Ok((sid.clone(), e.name.clone(), relay_close(e)?))).collect()
}

/// `hand_left_behind` for a session in `index`.
pub(super) fn hand_left_behind_in(index: &HashMap<String, Member>, session_id: &str, to: &str) -> Result<Vec<Cmd>, String> {
    let lead = index.get(to).filter(|l| l.role == "lead" && l.plugin == "relay" && safe_id(to)).ok_or("that isn't a relay lead")?;
    let left = left_behind(index, session_id);
    if left.is_empty() {
        return Err("its lead is still running, or it's already closed".into());
    }
    if left.iter().any(|(_, e)| e.machine != lead.machine) {
        return Err("that lead runs on another machine".into());
    }
    left.into_iter()
        .map(|(_, e)| relay_for(e).map(|c| Cmd { bin: "/usr/bin/env".into(), args: vec![format!("CLAUDE_CODE_SESSION_ID={to}"), c.bin, "adopt".into(), e.id.clone()], machine: c.machine }))
        .collect()
}
