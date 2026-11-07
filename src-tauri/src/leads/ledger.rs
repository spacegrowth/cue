//! relay's ledger (`sessions.jsonl`), read a little more each tick: verdicts, hand-offs, reviews and diff sizes.

use super::files::s;
use super::verify::Verdict;
use super::wake::epoch_secs;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// relay's ledger (`sessions.jsonl`): the last `report_verify` event per (executor id, packet), the
/// same rule relay's own `list --json` uses. The ledger only grows (megabytes of it), so Cue keeps
/// how far it has read and the verdicts found so far, and each tick reads just the new bytes. A last
/// line that isn't finished yet is left for the next tick; a ledger that shrank or is a different
/// file (its inode changed) is read again from the start.
pub(super) type Verdicts = std::sync::Arc<HashMap<(String, u64), Verdict>>;
/// Every `lead_handoff` in the ledger: the lead that handed off → (the lead it opened, its project,
/// when). relay's own record of a lead (its marker, which names its predecessor) goes once that lead
/// steps down or ends; the ledger keeps the handoff for good.
pub(super) type Handoffs = std::sync::Arc<HashMap<String, (String, String, u64)>>;
/// Every packet relay ledgered a review for (`report_reviewed`): (executor id, packet).
pub(super) type Reviews = std::sync::Arc<HashSet<(String, u64)>>;
/// The staged diff's size relay recorded for a packet (its auto-commit check): (files, added, removed).
pub(super) type DiffStats = std::sync::Arc<HashMap<(String, u64), (u64, u64, u64)>>;
#[derive(Clone, Default)]
pub(super) struct Ledger {
    inode: u64,
    /// Bytes read so far: the end of the last complete line seen.
    pub(super) offset: u64,
    pub(super) verdicts: Verdicts,
    handoffs: Handoffs,
    pub(super) reviews: Reviews,
    pub(super) stats: DiffStats,
}
/// By path (Cue reads one ledger; tests read several).
pub(super) static LEDGER: Mutex<Option<HashMap<PathBuf, Ledger>>> = Mutex::new(None);

pub(super) fn ledger_handoffs(path: &Path) -> Handoffs {
    read_ledger(path).handoffs
}

pub(super) fn read_ledger(path: &Path) -> Ledger {
    use std::io::{Read, Seek, SeekFrom};
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = std::fs::metadata(path) else { return Default::default() };
    let mut cache = crate::locking::lock(&LEDGER, "LEDGER");
    let cache = cache.get_or_insert_with(HashMap::new);
    let same = cache.get(path).is_some_and(|l| l.inode == meta.ino() && l.offset <= meta.len());
    if !same {
        cache.insert(path.to_path_buf(), Ledger { inode: meta.ino(), ..Default::default() });
    }
    let ledger = cache.get_mut(path).unwrap();
    let seen = |l: &Ledger| l.clone();
    if meta.len() == ledger.offset {
        return seen(ledger);
    }
    let mut bytes = Vec::new();
    let read = std::fs::File::open(path).and_then(|mut f| {
        f.seek(SeekFrom::Start(ledger.offset))?;
        f.read_to_end(&mut bytes)
    });
    if read.is_err() {
        return seen(ledger);
    }
    // Only whole lines: what follows the last newline may still be being written.
    let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { return seen(ledger) };
    ledger.offset += end as u64 + 1;
    let text = String::from_utf8_lossy(&bytes[..end]);
    add_handoffs(ledger, &text);
    add_reviews(ledger, &text);
    add_verdicts(ledger, &text);
    seen(ledger)
}

/// The `lead_handoff` events in newly read lines.
fn add_handoffs(ledger: &mut Ledger, text: &str) {
    let handoffs = std::sync::Arc::make_mut(&mut ledger.handoffs);
    for line in text.lines().filter(|l| l.contains("\"lead_handoff\"")) {
        let Ok(d) = serde_json::from_str::<Value>(line) else { continue };
        let (from, to) = (s(&d, "from_lead"), s(&d, "to_lead"));
        if s(&d, "event") == "lead_handoff" && !from.is_empty() && !to.is_empty() {
            handoffs.insert(from, (to, s(&d, "project"), epoch_secs(&s(&d, "ts"), None).map_or(0, |t| t * 1000)));
        }
    }
}

/// The `report_reviewed` and auto-commit events in newly read lines: which packets were reviewed, and the size
/// of each one's staged diff.
fn add_reviews(ledger: &mut Ledger, text: &str) {
    let reviews = std::sync::Arc::make_mut(&mut ledger.reviews);
    let stats = std::sync::Arc::make_mut(&mut ledger.stats);
    for line in text.lines().filter(|l| l.contains("\"report_reviewed\"") || l.contains("\"auto_commit")) {
        let Ok(d) = serde_json::from_str::<Value>(line) else { continue };
        let (sid, n) = (s(&d, "session_id"), d.get("packet").and_then(Value::as_u64));
        let (true, Some(n)) = (!sid.is_empty(), n) else { continue };
        match s(&d, "event").as_str() {
            "report_reviewed" => {
                reviews.insert((sid, n));
            }
            "auto_commit" | "auto_commit_blocked" => {
                let k = |f: &str| d.get(f).and_then(Value::as_u64).unwrap_or(0);
                stats.insert((sid, n), (k("files"), k("insertions"), k("deletions")));
            }
            _ => {}
        }
    }
}

/// The `report_verify` events in newly read lines: the last verdict per (executor id, packet) wins.
fn add_verdicts(ledger: &mut Ledger, text: &str) {
    let map = std::sync::Arc::make_mut(&mut ledger.verdicts);
    for line in text.lines().filter(|l| l.contains("\"report_verify\"")) {
        let Ok(d) = serde_json::from_str::<Value>(line) else { continue };
        let (sid, n) = (s(&d, "session_id"), d.get("packet").and_then(Value::as_u64));
        if s(&d, "event") != "report_verify" || sid.is_empty() {
            continue;
        }
        let (Some(n), Some(verdict)) = (n, Some(s(&d, "verdict")).filter(|v| !v.is_empty())) else { continue };
        let k = d.get("mismatches").and_then(Value::as_u64).unwrap_or(0);
        let note = if verdict == "MISMATCH" && k > 0 { format!("{k} mismatch{}", if k == 1 { "" } else { "es" }) } else { String::new() };
        map.insert((sid, n), Verdict { verdict, note });
    }
}
