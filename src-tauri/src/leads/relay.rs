//! relay's crews on this Mac, from its state folder: each lead's marker and plan, and each executor's
//! record with its packet, report, review and verdict.

use super::files::{color, first_line, predecessor, read_json, s, subdirs};
use super::git::{committed_since, has_staged, packet_files, written_at};
use super::ledger::read_ledger;
use super::wake::{epoch_secs, now_secs, ready_late, report_woken};
use super::{exec_key, put, safe_id, Member, GONE};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(super) fn scan_relay(root: &Path, live: &HashSet<String>, index: &mut HashMap<String, Member>) {
    let mut leads: HashMap<String, (String, Option<[u8; 3]>)> = HashMap::new();
    for d in subdirs(&root.join("lead")) {
        let Some(m) = read_json(&d.join("marker.json")) else { continue };
        let sid = s(&m, "session_id");
        leads.insert(sid.clone(), (s(&m, "project"), color(&m)));
        let plan = read_json(&d.join("plan.json")).map(|p| plan_queue(&p)).unwrap_or_default();
        let lead = Member {
            role: "lead".into(),
            plugin: "relay".into(),
            name: s(&m, "project"),
            color: color(&m),
            model: s(&m, "model"),
            auto: m.get("autonomous").and_then(Value::as_bool),
            predecessor: predecessor(&m),
            started_ms: epoch_secs(&s(&m, "started"), None).map_or(0, |t| t * 1000),
            plan_next: plan.0,
            plan_queued: plan.1,
            updated: s(&m, "last_active"),
            ..Default::default()
        };
        put(index, sid, lead);
    }
    let ledger = read_ledger(&root.join("sessions.jsonl"));
    for d in subdirs(root) {
        let Some(e) = read_json(&d.join("session.json")) else { continue };
        if !s(&e, "agent").starts_with("relay-executor") {
            continue;
        }
        if !safe_id(&s(&e, "session_id")) {
            eprintln!("cue: relay session {} has an id that isn't a folder name ({:?}): skipped", d.display(), s(&e, "session_id"));
            continue;
        }
        let owner = s(&e, "owner_lead");
        let lead = leads.get(&owner);
        let packet = e.get("current_packet").and_then(Value::as_u64);
        // Goal and outcome only for executors still at work (closed ones pile up by the hundred), and for
        // one relay parked after a review that nobody has committed yet: its Review row is still up.
        let status = s(&e, "status");
        let id = s(&e, "session_id");
        let parked = status == "closed" && s(&e, "auto_closed") == "reviewed" && live.contains(&owner);
        let open = packet.filter(|_| !GONE.contains(&status.as_str()) || parked);
        let (goal, outcome) = match open {
            Some(n) => (first_line(&d.join(format!("packets/{n:03}-packet.md"))), first_line(&d.join(format!("packets/{n:03}-report.md")))),
            None => (String::new(), String::new()),
        };
        let verify = open.and_then(|n| ledger.verdicts.get(&(id.clone(), n)).cloned());
        let review_file = open.map(|n| d.join(format!("packets/{n:03}-review.md")));
        let reviewed = open.is_some_and(|n| ledger.reviews.contains(&(id.clone(), n))) || review_file.as_ref().is_some_and(|f| f.is_file());
        let review = if reviewed { review_file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default() } else { String::new() };
        let diff_size = open.and_then(|n| ledger.stats.get(&(id.clone(), n)).copied());
        // Waiting on you ends once the packet's work is committed (or, parked, no longer staged).
        let worktree = s(&e, "worktree");
        let waits = parked || matches!(status.as_str(), "reported" | "idle");
        let files = open.filter(|_| waits).map(|n| packet_files(&d, n)).unwrap_or_default();
        let landed = open.is_some_and(|n| waits && committed_since(&worktree, written_at(&d.join(format!("packets/{n:03}-report.md"))), &files));
        let parked_staged = parked && lead.is_some() && !landed && has_staged(&worktree, &files);
        let m = Member {
            role: "executor".into(),
            plugin: "relay".into(),
            name: Some(s(&e, "topic")).filter(|t| !t.is_empty()).unwrap_or_else(|| s(&e, "session_id")),
            color: lead.and_then(|l| l.1),
            model: s(&e, "model"),
            status,
            packet: e.get("current_packet").and_then(Value::as_u64),
            lead_name: lead.map(|l| l.0.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| s(&e, "owner_project")),
            lead_armed: Some(lead.is_some() && live.contains(&owner)),
            lead: owner,
            goal,
            outcome,
            verify,
            woken: report_woken(&e),
            reviewed,
            review,
            diff_size,
            parked_staged,
            landed,
            ready_late: ready_late(&e, now_secs(), None),
            id,
            updated: s(&e, "updated"),
            ..Default::default()
        };
        put(index, exec_key(&e), m);
    }
}

/// A relay plan's queue: the first item not sent yet (its note, else its packet's name) and how many
/// aren't sent yet (not done and not bound to an executor).
pub(super) fn plan_queue(plan: &Value) -> (String, u64) {
    let items = plan.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
    let queued: Vec<&Value> = items.iter().filter(|i| i.get("done").is_none_or(Value::is_null) && s(i, "executor").is_empty()).collect();
    let name = |i: &Value| {
        let note = s(i, "note");
        if !note.is_empty() {
            return note;
        }
        let file = s(i, "packet");
        let stem = Path::new(&file).file_stem().and_then(|f| f.to_str()).unwrap_or("").to_string();
        stem.strip_suffix("-packet").map(str::to_string).unwrap_or(stem)
    };
    (queued.first().map(|i| name(i)).unwrap_or_default(), queued.len() as u64)
}
