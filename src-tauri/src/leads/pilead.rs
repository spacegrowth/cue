//! pi-lead's crews: from its state folder on this Mac, and from its listing on one of your machines.

use super::files::{first_line, read_json, s, subdirs};
use super::remote::{clip, tmux_name};
use super::verify::verdict_from_json;
use super::{put, Member, GONE};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

pub(super) fn scan_pilead(root: &Path, index: &mut HashMap<String, Member>) {
    let mut leads: HashMap<String, String> = HashMap::new();
    if let Ok(dir) = std::fs::read_dir(root.join("leads")) {
        for f in dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")) {
            let Some(m) = read_json(&f) else { continue };
            let sid = Some(s(&m, "sid")).filter(|x| !x.is_empty()).unwrap_or_else(|| f.file_stem().unwrap_or_default().to_string_lossy().into());
            leads.insert(sid.clone(), s(&m, "project"));
            put(index, sid, Member { role: "lead".into(), plugin: "pilead".into(), name: s(&m, "project"), updated: s(&m, "started"), ..Default::default() });
        }
    }
    for d in subdirs(&root.join("sessions")) {
        let Some(m) = read_json(&d.join("meta.json")) else { continue };
        // The directory's name is the session id (pi-lead never builds a path from `sid`).
        let sid = d.file_name().unwrap_or_default().to_string_lossy().to_string();
        let status = std::fs::read_to_string(d.join("status")).map(|t| t.trim().to_string()).ok().filter(|t| !t.is_empty()).unwrap_or_else(|| s(&m, "status"));
        let lead = s(&m, "lead");
        let packets = m.get("packets").and_then(Value::as_u64);
        let (goal, outcome) = match packets.filter(|_| !GONE.contains(&status.as_str())) {
            Some(n) => (first_line(&d.join(format!("packet-{n:04}.md"))), first_line(&d.join(format!("report-{n:04}.md")))),
            None => (String::new(), String::new()),
        };
        let e = Member {
            role: "executor".into(),
            plugin: "pilead".into(),
            name: Some(s(&m, "topic")).filter(|t| !t.is_empty()).unwrap_or_else(|| sid.clone()),
            model: s(&m, "model"),
            status,
            packet: m.get("packets").and_then(Value::as_u64),
            lead_name: leads.get(&lead).cloned().unwrap_or_default(),
            lead_armed: Some(leads.contains_key(&lead)),
            lead,
            goal,
            outcome,
            id: sid.clone(),
            updated: s(&m, "created"),
            ..Default::default()
        };
        put(index, sid, e);
    }
}

/// A machine's pi-lead crews (`pilead list --json`: {"leads": […], "executors": […]}) into the same
/// tree as relay's. A machine without pilead (or an answer that isn't JSON) adds nothing.
pub(super) fn parse_pilead(section: &str, machine: &str, index: &mut HashMap<String, Member>) {
    let json = section.lines().skip_while(|l| !l.starts_with("PBIN:")).skip(1).collect::<Vec<_>>().join("\n");
    let Ok(v) = serde_json::from_str::<Value>(json.trim()) else { return };
    let mut leads: HashMap<String, (String, bool)> = HashMap::new();
    for l in v.get("leads").and_then(Value::as_array).into_iter().flatten() {
        let sid = s(l, "sid");
        // "ghost": its record outlived its session. Every other state is a lead that's there.
        leads.insert(sid.clone(), (s(l, "project"), s(l, "live") != "ghost"));
        let lead = Member { role: "lead".into(), plugin: "pilead".into(), name: s(l, "project"), model: s(l, "model"), updated: s(l, "started"), machine: machine.into(), ..Default::default() };
        put(index, sid, lead);
    }
    for e in v.get("executors").and_then(Value::as_array).into_iter().flatten() {
        let sid = s(e, "sid");
        let status = s(e, "status");
        let lead = s(e, "lead");
        let known = leads.get(&lead);
        let open = !GONE.contains(&status.as_str());
        let packet = e.get("packet").or_else(|| e.get("packets")).and_then(|p| p.as_u64().or_else(|| p.as_str()?.parse().ok()));
        let m = Member {
            role: "executor".into(),
            plugin: "pilead".into(),
            name: Some(s(e, "topic")).filter(|t| !t.is_empty()).unwrap_or_else(|| sid.clone()),
            model: s(e, "model"),
            status,
            packet,
            lead_name: known.map(|l| l.0.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| s(e, "project")),
            lead_armed: Some(known.is_some_and(|l| l.1)),
            lead,
            goal: if open { clip(&s(e, "goal")) } else { String::new() },
            outcome: if open { clip(&s(e, "outcome")) } else { String::new() },
            verify: if open { verdict_from_json(e) } else { None },
            id: sid.clone(),
            updated: s(e, "created"),
            machine: machine.into(),
            tmux_session: tmux_name(&s(e, "tmux_session")),
            ..Default::default()
        };
        put(index, sid, m);
    }
}
