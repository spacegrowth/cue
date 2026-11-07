//! `relay verify`'s verdict on an executor's report: run by Cue in the background, or as relay already
//! recorded it.

use super::commands::relay_bin;
use super::files::s;
use super::Member;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// `relay verify` on each reported relay executor: does its report's list of changes match what it
/// staged? Its verdict (COUNTS-MATCH / MISMATCH / MALFORMED) and, when off, a few words why. Run in
/// the background, once per report (it's keyed by the report file's modification time).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Verdict {
    pub verdict: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}
static VERIFY: Mutex<Option<HashMap<String, (String, Option<Verdict>)>>> = Mutex::new(None);
/// A verdict came in since the last refresh: redraw.
pub(super) static VERIFIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The verdict and why, from `relay verify`'s output.
pub(super) fn parse_verify(out: &str) -> Option<Verdict> {
    let verdict = out.lines().find_map(|l| l.trim().strip_prefix("VERDICT:"))?.trim().to_string();
    let count = |label: &str| out.lines().find_map(|l| l.trim().strip_prefix(label)).and_then(|r| r.trim().parse::<u64>().ok()).unwrap_or(0);
    let missing = count("claimed, NOT staged:");
    let note = if missing > 0 {
        format!("{missing} change{} it reports {} not staged", if missing == 1 { "" } else { "s" }, if missing == 1 { "is" } else { "are" })
    } else if verdict == "MALFORMED" {
        "its report's summary block is missing or broken".into()
    } else {
        String::new()
    };
    Some(Verdict { verdict, note })
}

/// Start `relay verify` for each reported executor whose report is new (or changed) since its last check.
pub(super) fn verify_reported(index: &HashMap<String, Member>, root: &Path) {
    let Some(bin) = relay_bin() else { return };
    let mut cache = crate::locking::lock(&VERIFY, "VERIFY");
    let cache = cache.get_or_insert_with(HashMap::new);
    for m in index.values().filter(|m| m.plugin == "relay" && m.role == "executor" && m.status == "reported") {
        let Some(n) = m.packet else { continue };
        let report = root.join(&m.id).join(format!("packets/{n:03}-report.md"));
        let Ok(mtime) = std::fs::metadata(&report).and_then(|x| x.modified()) else { continue };
        let key = format!("{n}:{:?}", mtime);
        if cache.get(&m.id).is_some_and(|(k, _)| *k == key) {
            continue;
        }
        cache.insert(m.id.clone(), (key.clone(), None));
        let (bin, id) = (bin.clone(), m.id.clone());
        std::thread::spawn(move || {
            let out = std::process::Command::new(&bin).args(["verify", &id]).output();
            let v = out.ok().and_then(|o| parse_verify(&String::from_utf8_lossy(&o.stdout)));
            if let Some(c) = crate::locking::lock(&VERIFY, "VERIFY").as_mut() {
                if c.get(&id).is_some_and(|(k, _)| *k == key) {
                    c.insert(id, (key, v));
                    VERIFIED.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });
    }
}

/// The verdict Cue's own `relay verify` run came up with, else the one relay already had on record.
pub(super) fn verdict_of(m: &Member) -> Option<Verdict> {
    crate::locking::lock(&VERIFY, "VERIFY").as_ref().and_then(|c| c.get(&m.id)).and_then(|(_, v)| v.clone()).or_else(|| m.verify.clone())
}

/// A verdict from relay's `list --json` (`verify`: {verdict, mismatches, …}), if it gave one.
pub(super) fn verdict_from_json(v: &Value) -> Option<Verdict> {
    let verdict = s(v.get("verify")?, "verdict");
    if verdict.is_empty() {
        return None;
    }
    let n = v["verify"].get("mismatches").and_then(Value::as_u64).unwrap_or(0);
    let note = Some(s(&v["verify"], "note")).filter(|t| !t.is_empty()).unwrap_or_else(|| if verdict == "MISMATCH" && n > 0 { format!("{n} mismatch{}", if n == 1 { "" } else { "es" }) } else { String::new() });
    Some(Verdict { verdict, note })
}
