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

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
    /// Executor: its lead has seen this packet's report (relay's `report_seen_packet`): reviewed,
    /// by your Review or by relay waking the lead itself.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reviewed: bool,
    /// Executor: its report has been ready (done: reported or idle) for MISSED_WAKE_SECS or more, going
    /// by relay's `updated`. Kept as a flag, not a time, so the window redraws when it turns true.
    #[serde(skip)]
    pub ready_late: bool,
    /// Sort key for duplicates (an executor id reused by a resume): the newest record wins.
    #[serde(skip)]
    updated: String,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Statuses that mean the executor is done with (closed, replaced): left out of a lead's list. (A
/// "dead" or "stalled" one isn't: its agent is gone but relay still lists it, so it stays under its
/// lead until you Close it.)
const GONE: &[&str] = &["closed", "superseded", "retired", "done"];

static INDEX: Mutex<Option<HashMap<String, Member>>> = Mutex::new(None);
/// This Mac's part of INDEX, as last scanned.
static LOCAL: Mutex<Option<HashMap<String, Member>>> = Mutex::new(None);
/// Each machine's part: its relay command there, and its leads and executors.
static REMOTE: Mutex<Option<HashMap<String, (String, HashMap<String, Member>)>>> = Mutex::new(None);
/// Each machine's pilead command ("" when it has none).
static REMOTE_PILEAD: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Each file's last parse, by path, with the modification time it was parsed at. relay never deletes
/// executor records (hundreds of closed ones pile up), and a closed record never changes again: a
/// rescan stats every file but parses only the ones that changed. Each entry also keeps the scan
/// that last saw it, so a record that's gone (pruned) is dropped at the end of the next scan.
#[derive(Default)]
struct Parsed {
    scan: u64,
    files: HashMap<PathBuf, (std::time::SystemTime, Option<Value>, u64)>,
}
static PARSED: Mutex<Option<Parsed>> = Mutex::new(None);

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}
fn relay_dir() -> PathBuf {
    std::env::var_os("RELAY_STATE_ROOT").map(PathBuf::from).unwrap_or_else(|| home().join(".relay-tasks"))
}
fn claude_sessions_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".claude")).join("sessions")
}

/// Claude sessions open in a live process right now (Claude Code's `sessions/<pid>.json`).
fn live_claude_sessions(dir: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return HashSet::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| read_json(&p))
        .filter(|r| r.get("pid").and_then(Value::as_i64).is_some_and(|pid| pid > 0 && crate::server::alive(pid as i32)))
        .map(|r| s(&r, "sessionId"))
        .filter(|sid| !sid.is_empty())
        .collect()
}

fn pilead_dir() -> PathBuf {
    std::env::var_os("PI_LEAD_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".pi-lead"))
}

fn read_json(p: &Path) -> Option<Value> {
    let mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok()?;
    let mut parsed = crate::locking::lock(&PARSED, "PARSED");
    let parsed = parsed.get_or_insert_with(Parsed::default);
    let scan = parsed.scan;
    if let Some((at, v, seen)) = parsed.files.get_mut(p) {
        if *at == mtime {
            *seen = scan;
            return v.clone();
        }
    }
    let v = std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str(&t).ok());
    parsed.files.insert(p.to_path_buf(), (mtime, v.clone(), scan));
    v
}
fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}
fn color(v: &Value) -> Option<[u8; 3]> {
    let a = v.get("color")?.as_array()?;
    let c = |i: usize| a.get(i)?.as_u64().filter(|n| *n <= 255).map(|n| n as u8);
    Some([c(0)?, c(1)?, c(2)?])
}
fn subdirs(p: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(p).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

/// The first non-empty line of a packet or report (≤ 300 chars), "GOAL: " dropped. Read again
/// only when the file changes (they're written once per packet).
static FIRST_LINES: Mutex<Option<HashMap<PathBuf, (std::time::SystemTime, String)>>> = Mutex::new(None);
fn first_line(p: &Path) -> String {
    let Ok(mtime) = std::fs::metadata(p).and_then(|m| m.modified()) else { return String::new() };
    let mut cache = crate::locking::lock(&FIRST_LINES, "FIRST_LINES");
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((at, line)) = cache.get(p) {
        if *at == mtime {
            return line.clone();
        }
    }
    let text = std::fs::read_to_string(p).unwrap_or_default();
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let line = line.strip_prefix("GOAL:").unwrap_or(line).trim();
    let line: String = if line.chars().count() > 300 { format!("{}…", line.chars().take(300).collect::<String>()) } else { line.to_string() };
    cache.insert(p.to_path_buf(), (mtime, line.clone()));
    line
}

/// Whether a relay executor's lead has seen its current packet's report (relay marks it when the
/// lead reviews or opens its diff).
fn seen_report(e: &Value) -> bool {
    let n = |k: &str| e.get(k).and_then(Value::as_u64);
    matches!((n("current_packet"), n("report_seen_packet")), (Some(cur), Some(seen)) if seen >= cur)
}

/// A relay session id Cue may build a path from (`<relay>/<id>/...`): one folder name, never a path.
/// Relay only ever writes such ids; anything else came from a file Cue shouldn't trust.
fn safe_id(id: &str) -> bool {
    !id.is_empty() && !id.contains('/') && !id.contains('\\') && !id.contains("..")
}

/// How long a report may sit before its lead is said to have missed it (relay's own wake-up is
/// normally within seconds).
const MISSED_WAKE_SECS: u64 = 90;

/// relay's timestamp ("2026-10-04T09:50:00", the local time of the machine it ran on, maybe with a
/// fraction or a zone suffix, which is ignored) as calendar fields.
fn naive_tm(ts: &str) -> Option<libc::tm> {
    let (d, t) = ts.trim().split_once(['T', ' '])?;
    let mut dp = d.split('-').map(|n| n.parse::<i32>().ok());
    let (y, mo, da) = (dp.next()??, dp.next()??, dp.next()??);
    let mut tp = t.split(|c: char| !c.is_ascii_digit() && c != ':').next()?.split(':').map(|n| n.parse::<i32>().ok());
    let (h, mi) = (tp.next()??, tp.next()??);
    let sec = tp.next().flatten().unwrap_or(0);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    (tm.tm_year, tm.tm_mon, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec, tm.tm_isdst) = (y - 1900, mo - 1, da, h, mi, sec, -1);
    Some(tm)
}

/// Seconds since the epoch for relay's timestamp. `zone`: the UTC offset (seconds) of the machine that
/// wrote it; None is this Mac, in its own zone (daylight saving included).
fn epoch_secs(ts: &str, zone: Option<i64>) -> Option<u64> {
    let mut tm = naive_tm(ts)?;
    let t = match zone {
        None => unsafe { libc::mktime(&mut tm) as i64 },
        Some(off) => (unsafe { libc::timegm(&mut tm) } as i64) - off,
    };
    (t > 0).then_some(t as u64)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Whether an executor is done (reported or idle) and has been for MISSED_WAKE_SECS (relay's
/// `updated`, in the zone of the machine it ran on: the last thing relay wrote to the record is the
/// report landing).
fn ready_late(e: &Value, now: u64, zone: Option<i64>) -> bool {
    matches!(s(e, "status").as_str(), "reported" | "idle") && epoch_secs(&s(e, "updated"), zone).is_some_and(|at| now >= at + MISSED_WAKE_SECS)
}

/// The missed wake-up: the report is ready (a while) and its lead hasn't seen it, though the lead is
/// listening and free (`lead_free`: not working, compacting or asking you something). relay would
/// normally have woken it by now.
fn missed_wake(m: &Member, lead_free: bool) -> bool {
    m.role == "executor" && matches!(m.status.as_str(), "reported" | "idle") && !m.reviewed && m.ready_late && m.lead_armed == Some(true) && lead_free
}

/// Done executors whose lead missed the wake-up, as Cue session id; `lead_free` says whether a lead
/// (its Cue session id) is free to take a review.
pub fn missed(lead_free: impl Fn(&str) -> bool) -> Vec<String> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    cur.as_ref().map(|i| i.iter().filter(|(_, m)| missed_wake(m, lead_free(&m.lead))).map(|(sid, _)| sid.clone()).collect()).unwrap_or_default()
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

fn scan_relay(root: &Path, live: &HashSet<String>, index: &mut HashMap<String, Member>) {
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
            plan_next: plan.0,
            plan_queued: plan.1,
            updated: s(&m, "last_active"),
            ..Default::default()
        };
        put(index, sid, lead);
    }
    let verdicts = ledger_verdicts(&root.join("sessions.jsonl"));
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
        // Goal and outcome only for executors still at work: closed ones pile up by the hundred.
        let open = packet.filter(|_| !GONE.contains(&s(&e, "status").as_str()));
        let (goal, outcome) = match open {
            Some(n) => (first_line(&d.join(format!("packets/{n:03}-packet.md"))), first_line(&d.join(format!("packets/{n:03}-report.md")))),
            None => (String::new(), String::new()),
        };
        let verify = open.and_then(|n| verdicts.get(&(s(&e, "session_id"), n)).cloned());
        let m = Member {
            role: "executor".into(),
            plugin: "relay".into(),
            name: Some(s(&e, "topic")).filter(|t| !t.is_empty()).unwrap_or_else(|| s(&e, "session_id")),
            color: lead.and_then(|l| l.1),
            model: s(&e, "model"),
            status: s(&e, "status"),
            packet: e.get("current_packet").and_then(Value::as_u64),
            lead_name: lead.map(|l| l.0.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| s(&e, "owner_project")),
            lead_armed: Some(lead.is_some() && live.contains(&owner)),
            lead: owner,
            goal,
            outcome,
            verify,
            reviewed: seen_report(&e),
            ready_late: ready_late(&e, now_secs(), None),
            id: s(&e, "session_id"),
            updated: s(&e, "updated"),
            ..Default::default()
        };
        put(index, exec_key(&e), m);
    }
}

fn scan_pilead(root: &Path, index: &mut HashMap<String, Member>) {
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

/// A relay plan's queue: the first item not sent yet (its note, else its packet's name) and how many
/// aren't sent yet (not done and not bound to an executor).
fn plan_queue(plan: &Value) -> (String, u64) {
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
static VERIFIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The verdict and why, from `relay verify`'s output.
fn parse_verify(out: &str) -> Option<Verdict> {
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
fn verify_reported(index: &HashMap<String, Member>, root: &Path) {
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
fn verdict_of(m: &Member) -> Option<Verdict> {
    crate::locking::lock(&VERIFY, "VERIFY").as_ref().and_then(|c| c.get(&m.id)).and_then(|(_, v)| v.clone()).or_else(|| m.verify.clone())
}

/// A verdict from relay's `list --json` (`verify`: {verdict, mismatches, …}), if it gave one.
fn verdict_from_json(v: &Value) -> Option<Verdict> {
    let verdict = s(v.get("verify")?, "verdict");
    if verdict.is_empty() {
        return None;
    }
    let n = v["verify"].get("mismatches").and_then(Value::as_u64).unwrap_or(0);
    let note = Some(s(&v["verify"], "note")).filter(|t| !t.is_empty()).unwrap_or_else(|| if verdict == "MISMATCH" && n > 0 { format!("{n} mismatch{}", if n == 1 { "" } else { "es" }) } else { String::new() });
    Some(Verdict { verdict, note })
}

/// relay's ledger (`sessions.jsonl`): the last `report_verify` event per (executor id, packet), the
/// same rule relay's own `list --json` uses. The ledger only grows (megabytes of it), so Cue keeps
/// how far it has read and the verdicts found so far, and each tick reads just the new bytes. A last
/// line that isn't finished yet is left for the next tick; a ledger that shrank or is a different
/// file (its inode changed) is read again from the start.
type Verdicts = std::sync::Arc<HashMap<(String, u64), Verdict>>;
struct Ledger {
    inode: u64,
    /// Bytes read so far: the end of the last complete line seen.
    offset: u64,
    verdicts: Verdicts,
}
/// By path (Cue reads one ledger; tests read several).
static LEDGER: Mutex<Option<HashMap<PathBuf, Ledger>>> = Mutex::new(None);

fn ledger_verdicts(path: &Path) -> Verdicts {
    use std::io::{Read, Seek, SeekFrom};
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = std::fs::metadata(path) else { return Default::default() };
    let mut cache = crate::locking::lock(&LEDGER, "LEDGER");
    let cache = cache.get_or_insert_with(HashMap::new);
    let same = cache.get(path).is_some_and(|l| l.inode == meta.ino() && l.offset <= meta.len());
    if !same {
        cache.insert(path.to_path_buf(), Ledger { inode: meta.ino(), offset: 0, verdicts: Default::default() });
    }
    let ledger = cache.get_mut(path).unwrap();
    if meta.len() == ledger.offset {
        return ledger.verdicts.clone();
    }
    let mut bytes = Vec::new();
    let read = std::fs::File::open(path).and_then(|mut f| {
        f.seek(SeekFrom::Start(ledger.offset))?;
        f.read_to_end(&mut bytes)
    });
    if read.is_err() {
        return ledger.verdicts.clone();
    }
    // Only whole lines: what follows the last newline may still be being written.
    let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { return ledger.verdicts.clone() };
    ledger.offset += end as u64 + 1;
    let map = std::sync::Arc::make_mut(&mut ledger.verdicts);
    for line in String::from_utf8_lossy(&bytes[..end]).lines().filter(|l| l.contains("\"report_verify\"")) {
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
    ledger.verdicts.clone()
}

fn scan(relay: &Path, pilead: &Path, claude_sessions: &Path) -> HashMap<String, Member> {
    let mut index = HashMap::new();
    scan_relay(relay, &live_claude_sessions(claude_sessions), &mut index);
    scan_pilead(pilead, &mut index);
    if let Some(parsed) = crate::locking::lock(&PARSED, "PARSED").as_mut() {
        let scan = parsed.scan;
        parsed.files.retain(|_, (_, _, seen)| *seen == scan);
        parsed.scan += 1;
    }
    index
}

/// Re-read both plugins' folders. True when anything changed (so the window should redraw).
pub fn refresh() -> bool {
    let fresh = scan(&relay_dir(), &pilead_dir(), &claude_sessions_dir());
    verify_reported(&fresh, &relay_dir());
    let verified = VERIFIED.swap(false, std::sync::atomic::Ordering::Relaxed);
    *crate::locking::lock(&LOCAL, "LOCAL") = Some(fresh);
    merge() || verified
}

/// INDEX again from this Mac's part and every machine's (this Mac's wins a clash). True when it changed.
fn merge() -> bool {
    let mut all = crate::locking::lock(&LOCAL, "LOCAL").clone().unwrap_or_default();
    for (_, members) in crate::locking::lock(&REMOTE, "REMOTE").as_ref().into_iter().flat_map(|r| r.values()) {
        for (sid, m) in members {
            all.entry(sid.clone()).or_insert_with(|| m.clone());
        }
    }
    let mut cur = crate::locking::lock(&INDEX, "INDEX");
    if cur.as_ref() == Some(&all) {
        return false;
    }
    *cur = Some(all);
    true
}

/// What runs on a machine to list its crews: relay's command there (from Claude Code's installed
/// plugins, else on PATH), its JSON listing, pi-lead's (when `pilead` is there), then the Claude sessions open in a live process there
/// (a lead counts as listening only while one is), then its UTC offset (relay writes its timestamps
/// in the machine's local time).
const REMOTE_LIST: &str = r#"c="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
r=$(sed -n 's/.*"installPath": *"\([^"]*\)".*/\1/p' "$c/plugins/installed_plugins.json" 2>/dev/null | grep -i relay | head -1)
if [ -n "$r" ] && [ -x "$r/bin/relay" ]; then r="$r/bin/relay"; else r=$(command -v relay 2>/dev/null || echo "$HOME/.local/bin/relay"); fi
if [ -x "$r" ]; then echo "BIN:$r"; "$r" list --all --json 2>/dev/null; else echo NORELAY; fi
# pi-lead's crews, when pilead is on this machine (its own marker, then PBIN: and its JSON).
echo "__CUE_PILEAD__"
p=$(command -v pilead 2>/dev/null || echo "$HOME/.local/bin/pilead")
if [ -x "$p" ]; then echo "PBIN:$p"; "$p" list --json 2>/dev/null; else echo NOPILEAD; fi
echo "__CUE_LIVE__"
for f in "$c"/sessions/*.json; do
  # Claude Code writes these without a final newline, and sed then prints none: one id per line regardless.
  [ -f "$f" ] && kill -0 "$(basename "$f" .json)" 2>/dev/null && { sed -n 's/.*"sessionId": *"\([^"]*\)".*/\1/p' "$f"; echo; }
done
# The zone relay's naive timestamps are in: this machine's UTC offset.
echo "__CUE_TZ__:$(date +%z)"
exit 0"#;

/// Re-read one machine's crews (blocking: an SSH round trip): whether INDEX changed, and the Claude
/// sessions running there (None: it couldn't be reached; it keeps what was last read from it).
pub fn refresh_remote(machine: &str, host: &str) -> (bool, Option<HashSet<String>>) {
    let Ok(out) = crate::machines::run(host, REMOTE_LIST) else { return (false, None) };
    let parsed = parse_remote(&out, machine);
    crate::locking::lock(&REMOTE_PILEAD, "REMOTE_PILEAD").get_or_insert_with(HashMap::new).insert(machine.to_string(), pilead_bin_of(&out));
    crate::locking::lock(&REMOTE, "REMOTE").get_or_insert_with(HashMap::new).insert(machine.to_string(), parsed);
    (merge(), Some(live_of(&out)))
}

/// The Claude sessions REMOTE_LIST found running.
fn live_of(out: &str) -> HashSet<String> {
    out.split_once("__CUE_LIVE__").map(|(_, l)| l.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(TZ_MARK)).map(String::from).collect()).unwrap_or_default()
}

const TZ_MARK: &str = "__CUE_TZ__:";

/// The machine's UTC offset in seconds, from REMOTE_LIST's `date +%z` ("+0530", "-0800"); None when it
/// didn't say (an older answer, or a garbled one): its timestamps are then read in this Mac's zone.
fn tz_of(out: &str) -> Option<i64> {
    let z = out.lines().find_map(|l| l.trim().strip_prefix(TZ_MARK))?.trim();
    let sign = match z.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let d = &z[1..];
    if d.len() != 4 || !d.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(sign * (d[..2].parse::<i64>().ok()? * 3600 + d[2..].parse::<i64>().ok()? * 60))
}

/// Forget the crews of machines that aren't yours any more (removed in + New → Machine): whether
/// INDEX changed. Every machine still listed keeps what was last read from it.
pub fn retain_remote(machines: &[String]) -> bool {
    if let Some(p) = crate::locking::lock(&REMOTE_PILEAD, "REMOTE_PILEAD").as_mut() {
        p.retain(|m, _| machines.contains(m));
    }
    let gone = crate::locking::lock(&REMOTE, "REMOTE").as_mut().is_some_and(|r| {
        let n = r.len();
        r.retain(|m, _| machines.contains(m));
        r.len() != n
    });
    gone && merge()
}

/// Whether Cue knows this session as a relay or pi-lead executor (here or on a machine): it belongs
/// to its lead, which resumes, replaces or closes it; Cue never brings it back by itself.
pub fn is_executor(session_id: &str) -> bool {
    crate::locking::lock(&INDEX, "INDEX").as_ref().is_some_and(|i| executor_in(i, session_id))
}

fn executor_in(index: &HashMap<String, Member>, session_id: &str) -> bool {
    index.get(session_id).is_some_and(|m| m.role == "executor")
}

/// Whether this machine's crews have been read since Cue started (or since it was last removed).
pub fn remote_known(machine: &str) -> bool {
    crate::locking::lock(&REMOTE, "REMOTE").as_ref().is_some_and(|r| r.contains_key(machine))
}

/// REMOTE_LIST's answer: relay's command there, and its leads and executors keyed by Cue session id.
fn parse_remote(out: &str, machine: &str) -> (String, HashMap<String, Member>) {
    let bin = out.lines().find_map(|l| l.strip_prefix("BIN:")).unwrap_or("").trim().to_string();
    let (relay_part, pilead) = remote_sections(out);
    let json = relay_part.split_once('\n').filter(|_| !bin.is_empty()).map(|(_, rest)| rest).unwrap_or("");
    let live = live_of(out);
    let zone = tz_of(out);
    let v: Value = serde_json::from_str(json.trim()).unwrap_or(Value::Null);
    let mut index = HashMap::new();
    let mut leads: HashMap<String, (String, Option<[u8; 3]>)> = HashMap::new();
    for l in v.get("leads").and_then(Value::as_array).into_iter().flatten() {
        let sid = s(l, "session_id");
        leads.insert(sid.clone(), (s(l, "project"), color(l)));
        let lead = Member {
            role: "lead".into(),
            plugin: "relay".into(),
            name: s(l, "project"),
            color: color(l),
            model: s(l, "model"),
            auto: l.get("autonomous").and_then(Value::as_bool),
            updated: s(l, "last_active"),
            machine: machine.into(),
            ..Default::default()
        };
        put(&mut index, sid, lead);
    }
    for e in v.get("executors").and_then(Value::as_array).into_iter().flatten() {
        if !s(e, "agent").starts_with("relay-executor") {
            continue;
        }
        if !safe_id(&s(e, "session_id")) {
            eprintln!("cue: relay on {machine} lists a session whose id isn't a folder name ({:?}): skipped", s(e, "session_id"));
            continue;
        }
        let owner = s(e, "owner_lead");
        let lead = leads.get(&owner);
        // goal / outcome / verify are newer than the rest of relay's listing: a machine on an older
        // relay doesn't send them, and that's no error, just nothing to show.
        let open = !GONE.contains(&s(e, "status").as_str());
        let m = Member {
            role: "executor".into(),
            plugin: "relay".into(),
            name: Some(s(e, "topic")).filter(|t| !t.is_empty()).unwrap_or_else(|| s(e, "session_id")),
            color: lead.and_then(|l| l.1),
            model: s(e, "model"),
            status: s(e, "status"),
            goal: if open { clip(&s(e, "goal")) } else { String::new() },
            outcome: if open { clip(&s(e, "outcome")) } else { String::new() },
            verify: if open { verdict_from_json(e) } else { None },
            packet: e.get("current_packet").and_then(Value::as_u64),
            lead_name: lead.map(|l| l.0.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| s(e, "owner_project")),
            lead_armed: Some(lead.is_some() && live.contains(&owner)),
            lead: owner,
            reviewed: seen_report(e),
            ready_late: ready_late(e, now_secs(), zone),
            id: s(e, "session_id"),
            updated: s(e, "updated"),
            machine: machine.into(),
            tmux_session: tmux_name(&s(e, "tmux_session")),
            ..Default::default()
        };
        put(&mut index, exec_key(e), m);
    }
    parse_pilead(pilead, machine, &mut index);
    (bin, index)
}

/// REMOTE_LIST's answer up to the live-sessions marker, split into relay's part (its `BIN:` line and
/// JSON) and pilead's (what follows `__CUE_PILEAD__`; "" when the machine's answer has none, an older one).
fn remote_sections(out: &str) -> (&str, &str) {
    let head = out.split_once("__CUE_LIVE__").map_or(out, |(j, _)| j);
    head.split_once(PILEAD_MARK).unwrap_or((head, ""))
}

const PILEAD_MARK: &str = "__CUE_PILEAD__";

/// A machine's pilead command, from its section's `PBIN:` line ("" when it has none).
fn pilead_bin_of(out: &str) -> String {
    out.split_once(PILEAD_MARK).and_then(|(_, p)| p.lines().find_map(|l| l.strip_prefix("PBIN:"))).unwrap_or("").trim().to_string()
}

/// A tmux session name from a machine's listing, as Cue will pass it to tmux there: only letters, digits,
/// `_` and `-` (nothing the remote shell or tmux's own target syntax could read as more than a name); ""
/// for anything else, which leaves that executor's terminal as it was before the listing named one.
fn tmux_name(t: &str) -> String {
    let t = t.trim();
    if !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') { t.to_string() } else { String::new() }
}

/// An executor on a machine that runs in a tmux session of its own: (machine, that session's name).
/// None for anything else (a lead, a session on this Mac, or a listing that didn't name one).
pub fn exec_tmux(session_id: &str) -> Option<(String, String)> {
    exec_tmux_in(crate::locking::lock(&INDEX, "INDEX").as_ref()?, session_id)
}

fn exec_tmux_in(index: &HashMap<String, Member>, session_id: &str) -> Option<(String, String)> {
    let m = index.get(session_id)?;
    (m.role == "executor" && !m.machine.is_empty() && !m.tmux_session.is_empty()).then(|| (m.machine.clone(), m.tmux_session.clone()))
}

/// At most 300 characters, as relay clips the lines it lists.
fn clip(t: &str) -> String {
    let t = t.trim();
    if t.chars().count() > 300 { format!("{}…", t.chars().take(299).collect::<String>()) } else { t.to_string() }
}

/// A machine's pi-lead crews (`pilead list --json`: {"leads": […], "executors": […]}) into the same
/// tree as relay's. A machine without pilead (or an answer that isn't JSON) adds nothing.
fn parse_pilead(section: &str, machine: &str, index: &mut HashMap<String, Member>) {
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

/// A relay / pi-lead command, on this Mac or on the machine the session runs on.
#[derive(Debug, PartialEq)]
pub struct Cmd {
    pub bin: String,
    pub args: Vec<String>,
    /// The machine to run it on ("" for this Mac).
    pub machine: String,
}

impl Cmd {
    fn here(bin: PathBuf, args: Vec<String>) -> Cmd {
        Cmd { bin: bin.to_string_lossy().into_owned(), args, machine: String::new() }
    }

    /// Run it: Ok(what it printed) or Err(its last line of complaint). Blocking.
    pub fn run(&self) -> Result<String, String> {
        let last = |t: &str, or: &str| t.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(or).trim().to_string();
        if self.machine.is_empty() {
            let out = std::process::Command::new(&self.bin).args(&self.args).output().map_err(|e| format!("couldn't run {}: {e}", self.bin))?;
            return if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).into_owned()) } else { Err(last(&String::from_utf8_lossy(&out.stderr), "it failed")) };
        }
        let m = crate::machines::get(&self.machine).ok_or(format!("{} isn't one of your machines any more", self.machine))?;
        let line = std::iter::once(&self.bin).chain(&self.args).map(|a| crate::terminal::focus::shq(a)).collect::<Vec<_>>().join(" ");
        // Its own words either way (Cue's SSH hides a script's stderr), and its exit code last.
        let out = crate::machines::run(&m.host, &format!("{line} 2>&1; printf '\\n__CUE_EXIT__:%d\\n' \"$?\""))?;
        match out.rsplit_once("\n__CUE_EXIT__:") {
            Some((said, code)) if code.trim() == "0" => Ok(said.to_string()),
            Some((said, _)) => Err(last(said, "it failed")),
            None => Err("it didn't run there".into()),
        }
    }
}

/// relay's command where this member runs: its machine's (as last listed there), or this Mac's.
fn relay_for(m: &Member) -> Result<Cmd, String> {
    if m.machine.is_empty() {
        return relay_bin().map(|b| Cmd::here(b, vec![])).ok_or_else(|| "can't find relay's command (is the claude-relay plugin installed?)".to_string());
    }
    let bin = crate::locking::lock(&REMOTE, "REMOTE").as_ref().and_then(|r| r.get(&m.machine)).map(|(b, _)| b.clone()).filter(|b| !b.is_empty());
    bin.map(|bin| Cmd { bin, args: vec![], machine: m.machine.clone() }).ok_or_else(|| format!("can't find relay's command on {}", m.machine))
}

/// pilead's command where this executor runs: where it was launched (this Mac), or its machine's
/// (as last listed there).
fn pilead_for(m: &Member) -> Result<Cmd, String> {
    if m.machine.is_empty() {
        return pilead_bin(&m.id).map(|b| Cmd::here(b, vec![])).ok_or_else(|| "can't find pilead's command".to_string());
    }
    let bin = crate::locking::lock(&REMOTE_PILEAD, "REMOTE_PILEAD").as_ref().and_then(|r| r.get(&m.machine)).cloned().filter(|b| !b.is_empty());
    bin.map(|bin| Cmd { bin, args: vec![], machine: m.machine.clone() }).ok_or_else(|| format!("can't find pilead's command on {}", m.machine))
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

/// What a relay / pi-lead button in the window does.
#[derive(Debug, PartialEq)]
pub enum Act {
    /// Type this into a session (Cue's session id): the lead runs it as a command.
    Send { to: String, text: String },
    /// Run relay's own CLI; on success, say this.
    Run(Cmd, String),
    /// relay's diff page for an executor on a machine: written there (its path is what relay
    /// prints), brought here, then opened.
    RemoteDiff(Cmd),
    /// Open a page that's already written (pi-lead's executor writes its diff page itself).
    Open(PathBuf),
}

/// relay's own CLI, from the installed plugin (`~/.claude/plugins/installed_plugins.json`), else
/// `~/.local/bin/relay`. A window app's PATH has neither.
fn relay_bin() -> Option<PathBuf> {
    let claude = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".claude"));
    let installed = std::fs::read_to_string(claude.join("plugins/installed_plugins.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let from_plugin = installed.as_ref().and_then(|v| v.get("plugins")).and_then(Value::as_object).and_then(|plugins| {
        plugins.iter().filter(|(k, _)| k.starts_with("relay@")).find_map(|(_, v)| v.as_array()?.first()?.get("installPath")?.as_str().map(|p| PathBuf::from(p).join("bin/relay")))
    });
    from_plugin.into_iter().chain([home().join(".local/bin/relay")]).find(|p| p.is_file())
}

/// The newest diff page an executor wrote for itself (`sessions/<sid>/diff-NNNN.html`).
fn pilead_diff_page(root: &Path, id: &str) -> Option<PathBuf> {
    let dir = root.join("sessions").join(id);
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("diff-") && n.ends_with(".html"))).max()
}

/// pi-lead's own CLI, from where its executor was launched (`sessions/<id>/bootstrap.sh` runs
/// `pi … -e <pi-lead>/extensions/pi-lead.ts`), else ~/.local/bin/pilead.
fn pilead_bin(id: &str) -> Option<PathBuf> {
    let boot = std::fs::read_to_string(pilead_dir().join("sessions").join(id).join("bootstrap.sh")).unwrap_or_default();
    let from_boot = boot.split_whitespace().find(|w| w.ends_with("/extensions/pi-lead.ts")).and_then(|ext| Path::new(ext).parent()?.parent().map(|root| root.join("bin/pilead")));
    from_boot.into_iter().chain([home().join(".local/bin/pilead")]).find(|p| p.is_file())
}

/// How to close this session through its own tool, if it has one: a relay / pi-lead executor is closed
/// by relay / pilead (it marks the executor closed and closes its tab). A lead that still has
/// executors isn't closed from Cue: that's relay's handoff / close. None: not in a crew, or an
/// executor relay is already done with (closed, superseded) whose agent still runs (resumed in a
/// pane relay's record doesn't know): ending it is Cue's, as for any session.
pub fn close_plan(session_id: &str) -> Option<Result<Cmd, String>> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    let index = cur.as_ref()?;
    let m = index.get(session_id)?;
    if m.role == "executor" && GONE.contains(&m.status.as_str()) {
        return None;
    }
    match (m.plugin.as_str(), m.role.as_str()) {
        ("relay", "executor") => Some(relay_for(m).map(|c| Cmd { args: vec!["close".into(), m.id.clone()], ..c })),
        ("pilead", "executor") => Some(pilead_for(m).map(|c| Cmd { args: vec!["close".into(), m.id.clone()], ..c })),
        (_, "lead") => {
            let n = index.values().filter(|e| e.role == "executor" && e.lead == session_id && !GONE.contains(&e.status.as_str())).count();
            (n > 0).then(|| Err(format!("it leads {n} executor{} still open: close them, or hand the lead off, first", if n == 1 { "" } else { "s" })))
        }
        _ => None,
    }
}

/// Closing a relay lead that has no executors left: relay's own step-down (`relay close --self <lead
/// session id>`, run where the lead runs) comes first, so relay stops thinking it leads; then the lead
/// is ended as any session. None: not such a lead (an executor, a pi-lead lead, a lead with
/// executors still open, a session outside a crew).
pub fn lead_stop(session_id: &str) -> Option<Result<Cmd, String>> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    let index = cur.as_ref()?;
    let m = index.get(session_id).filter(|m| m.plugin == "relay" && m.role == "lead")?;
    let open = index.values().any(|e| e.role == "executor" && e.lead == session_id && !GONE.contains(&e.status.as_str()));
    (!open).then(|| relay_for(m).map(|c| Cmd { args: vec!["close".into(), "--self".into(), session_id.to_string()], ..c }))
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

/// Done executors whose report their lead has seen: Cue session id → its lead's session id.
pub fn reviewed() -> Vec<(String, String)> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    cur.as_ref().map(|i| i.iter().filter(|(_, m)| m.role == "executor" && m.reviewed && matches!(m.status.as_str(), "reported" | "idle")).map(|(sid, m)| (sid.clone(), m.lead.clone())).collect()).unwrap_or_default()
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
    let relay = |args: &[&str]| relay_for(m).map(|c| Cmd { args: args.iter().map(|a| a.to_string()).collect(), ..c });
    match (m.plugin.as_str(), m.role.as_str(), action) {
        (plugin, "executor", "review") => {
            if m.lead_armed != Some(true) {
                return Err(format!("its lead{} isn't listening", if m.lead_name.is_empty() { String::new() } else { format!(" ({})", m.lead_name) }));
            }
            let cmd = if plugin == "pilead" { "/pilead:review" } else { "/relay:review" };
            Ok(Act::Send { to: m.lead.clone(), text: format!("{cmd} {}", m.id) })
        }
        ("relay", "executor", "diff") if !m.machine.is_empty() => Ok(Act::RemoteDiff(relay(&["diff", &m.id])?)),
        ("relay", "executor", "diff") => Ok(Act::Run(relay(&["diff", &m.id, "--open"])?, "Opened in your browser".into())),
        // A lead's auto mode on or off (it goes ahead on routine steps without asking you).
        ("relay", "lead", "auto-on" | "auto-off") => {
            let on = action == "auto-on";
            Ok(Act::Run(relay(&["auto", if on { "on" } else { "off" }, "--session", session_id])?, format!("Auto mode {}", if on { "on" } else { "off" })))
        }
        // An executor whose agent exited before it reported: relay closes its record where it runs.
        ("relay", "executor", "close") if matches!(m.status.as_str(), "stalled" | "dead") => Ok(Act::Run(relay(&["close", &m.id])?, "Closed".into())),
        ("relay", "executor", "close") => Err("only an executor whose agent exited can be closed from here".into()),
        // pilead writes its diff page on the machine it runs on and Cue has no way to bring it here yet.
        ("pilead", "executor", "diff") if !m.machine.is_empty() => Err(format!("pilead's diff page is written on {}; Cue can't bring it here yet", m.machine)),
        ("pilead", "executor", "close") if matches!(m.status.as_str(), "stalled" | "dead") => Ok(Act::Run(pilead_for(m).map(|c| Cmd { args: vec!["close".into(), m.id.clone()], ..c })?, "Closed".into())),
        ("pilead", "executor", "diff") => pilead_diff_page(&pilead_dir(), &m.id).map(Act::Open).ok_or_else(|| "it hasn't written a diff page yet".into()),
        _ => Err(format!("no {action} for this session")),
    }
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
            let mut execs: Vec<(&String, &Member)> = index.iter().filter(|(_, e)| e.role == "executor" && e.lead == *sid && !GONE.contains(&e.status.as_str())).collect();
            execs.sort_by(|a, b| a.1.updated.cmp(&b.1.updated));
            v["executors"] = execs
                .into_iter()
                .map(|(esid, e)| serde_json::json!({ "session_id": esid, "name": e.name, "model": e.model, "status": e.status, "packet": e.packet, "goal": e.goal, "outcome": e.outcome, "verify": verdict_of(e), "plugin": e.plugin, "machine": e.machine, "tmux_session": e.tmux_session }))
                .collect();
        }
        out.insert(sid.clone(), v);
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_machines_crews_come_from_relays_listing_there() {
        let out = r#"BIN:/home/me/.claude/plugins/cache/claude-relay/relay/0.5.11/bin/relay
{"leads": [{"session_id": "L9", "project": "smoke", "color": [200, 140, 135], "model": "claude-opus-5-5", "autonomous": true, "last_active": "2026-10-04T09:57:37"},
           {"session_id": "L8", "project": "old", "last_active": "2026-10-03T18:30:38", "ended": true}],
 "executors": [{"agent": "relay-executor", "session_id": "hello-md", "topic": "hello-md", "status": "reported", "current_packet": 2, "owner_lead": "L9", "claude_session": "C9", "model": "claude-sonnet-5-5", "updated": "2026-10-04T09:50:00"},
               {"agent": "relay-executor", "session_id": "ls-root", "topic": "", "status": "busy", "owner_lead": "L8", "owner_project": "old", "claude_session": "C8"},
               {"agent": "something-else", "session_id": "x", "claude_session": "CX"}]}
__CUE_LIVE__
L9
C9
"#;
        let (bin, index) = parse_remote(out, "build-box");
        assert_eq!(bin, "/home/me/.claude/plugins/cache/claude-relay/relay/0.5.11/bin/relay");
        let l9 = &index["L9"];
        assert_eq!((l9.role.as_str(), l9.name.as_str(), l9.machine.as_str(), l9.auto), ("lead", "smoke", "build-box", Some(true)));
        let c9 = &index["C9"];
        assert_eq!((c9.role.as_str(), c9.id.as_str(), c9.lead.as_str(), c9.lead_name.as_str(), c9.lead_armed, c9.packet), ("executor", "hello-md", "L9", "smoke", Some(true), Some(2)));
        assert_eq!(c9.color, Some([200, 140, 135]), "its lead's color");
        assert_eq!(index["C8"].lead_armed, Some(false), "its lead's session isn't running there");
        assert_eq!(index["C8"].name, "ls-root", "no topic: its relay id");
        assert!(!index.contains_key("CX"), "not a relay executor");
        assert!(parse_remote("NORELAY\n", "build-box").1.is_empty(), "no relay there: no crews");
    }

    #[test]
    fn a_relay_id_that_would_leave_relays_folder_is_skipped() {
        assert!(super::safe_id("finder-1"));
        for bad in ["", "../x", "a/b", "a\\b", "..", "x/../../etc"] {
            assert!(!super::safe_id(bad), "{bad:?}");
        }
        // From a machine's listing: the bad one is dropped, the good one kept.
        let out = "BIN:/r\n{\"leads\": [], \"executors\": [{\"agent\": \"relay-executor\", \"session_id\": \"../../.ssh\", \"claude_session\": \"CBAD\", \"status\": \"reported\"}, {\"agent\": \"relay-executor\", \"session_id\": \"ok-1\", \"claude_session\": \"COK\", \"status\": \"busy\"}]}\n";
        let (_, index) = parse_remote(out, "build-box");
        assert!(!index.contains_key("CBAD"));
        assert_eq!(index["COK"].id, "ok-1");
        // From relay's folder on this Mac: the same.
        let root = std::env::temp_dir().join(format!("cue-leads-badid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |p: std::path::PathBuf, t: &str| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, t).unwrap();
        };
        write(root.join("evil/session.json"), r#"{"session_id":"../../x","agent":"relay-executor","claude_session":"C1","status":"reported"}"#);
        write(root.join("fine/session.json"), r#"{"session_id":"fine","agent":"relay-executor","claude_session":"C2","status":"reported"}"#);
        let mut index = std::collections::HashMap::new();
        super::scan_relay(&root, &std::collections::HashSet::new(), &mut index);
        let _ = std::fs::remove_dir_all(&root);
        assert!(!index.contains_key("C1"));
        assert_eq!(index["C2"].id, "fine");
    }

    #[test]
    fn the_listing_script_gives_one_live_session_per_line() {
        // A machine as Claude Code and relay leave it: two live sessions whose files end without a
        // newline (as Claude Code writes them), one dead, and relay's command in the installed plugin.
        let home = std::env::temp_dir().join(format!("cue-remote-list-{}", std::process::id()));
        let plugin = home.join("plugins/cache/claude-relay/relay/9.9.9");
        std::fs::create_dir_all(plugin.join("bin")).unwrap();
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        std::fs::write(home.join("plugins/installed_plugins.json"), format!("{{\n  \"plugins\": {{\n    \"relay@claude-relay\": [{{\n      \"installPath\": \"{}\"\n    }}]\n  }}\n}}\n", plugin.display())).unwrap();
        let relay = plugin.join("bin/relay");
        std::fs::write(&relay, "#!/bin/sh\necho '{\"leads\": [{\"session_id\": \"L1\", \"project\": \"p\"}], \"executors\": [{\"agent\": \"relay-executor\", \"session_id\": \"e\", \"owner_lead\": \"L1\", \"claude_session\": \"C1\", \"status\": \"reported\"}]}'\n").unwrap();
        std::os::unix::fs::PermissionsExt::set_mode(&mut std::fs::metadata(&relay).unwrap().permissions(), 0o755);
        std::fs::set_permissions(&relay, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let me = std::process::id();
        std::fs::write(home.join(format!("sessions/{me}.json")), r#"{"pid":1,"sessionId":"L1","cwd":"/tmp"}"#).unwrap();
        std::fs::write(home.join(format!("sessions/{}.json", std::os::unix::process::parent_id())), r#"{"pid":2,"sessionId":"C1","cwd":"/tmp"}"#).unwrap();
        std::fs::write(home.join("sessions/999999.json"), r#"{"pid":3,"sessionId":"DEAD"}"#).unwrap();
        let out = std::process::Command::new("/bin/sh").env("CLAUDE_CONFIG_DIR", &home).env("HOME", &home).args(["-c", REMOTE_LIST]).output().unwrap();
        let out = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(out.contains("NOPILEAD") && pilead_bin_of(&out).is_empty(), "no pilead there: {out:?}");
        let (bin, index) = parse_remote(&out, "box");
        assert_eq!(bin, relay.display().to_string());
        assert_eq!(index["C1"].lead_armed, Some(true), "its lead is live there: {out:?}");
        let live: Vec<&str> = out.split("__CUE_LIVE__").nth(1).unwrap().lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(TZ_MARK)).collect();
        assert_eq!(live.len(), 2, "one id per line, the dead one left out: {live:?}");
        assert!(tz_of(&out).is_some_and(|z| z.abs() <= 14 * 3600), "it says its UTC offset: {out:?}");
        assert_eq!(live_of(&out).len(), 2, "the offset line isn't a session");

        // The same machine with pilead on its PATH: its crews come in the same answer.
        let pilead = home.join("bin/pilead");
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(&pilead, format!("#!/bin/sh\ncat <<'EOF'\n{PI_LISTING}\nEOF\n")).unwrap();
        std::fs::set_permissions(&pilead, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let path = format!("{}:/usr/bin:/bin", home.join("bin").display());
        let out = std::process::Command::new("/bin/sh").env("CLAUDE_CONFIG_DIR", &home).env("HOME", &home).env("PATH", path).args(["-c", REMOTE_LIST]).output().unwrap();
        let out = String::from_utf8_lossy(&out.stdout).to_string();
        assert_eq!(pilead_bin_of(&out), pilead.display().to_string(), "{out:?}");
        let (bin, index) = parse_remote(&out, "box");
        assert_eq!(bin, relay.display().to_string(), "relay is still found");
        assert!(index.contains_key("C1") && index["pe-1"].plugin == "pilead" && index["PL1"].role == "lead", "both tools' crews: {:?}", index.keys());
        assert_eq!(live_of(&out).len(), 2);
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn an_executor_counts_as_reviewed_once_its_lead_saw_this_packets_report() {
        let e = |v: &str| serde_json::from_str::<Value>(v).unwrap();
        assert!(seen_report(&e(r#"{"current_packet": 2, "report_seen_packet": 2}"#)));
        assert!(!seen_report(&e(r#"{"current_packet": 2, "report_seen_packet": 1}"#)), "seen: the last packet's, not this one's");
        assert!(!seen_report(&e(r#"{"current_packet": 1, "report_seen_packet": null}"#)));
        assert!(!seen_report(&e(r#"{"current_packet": 1}"#)));
        let out = "BIN:/r\n{\"leads\": [], \"executors\": [{\"agent\": \"relay-executor\", \"session_id\": \"a\", \"claude_session\": \"CA\", \"status\": \"reported\", \"current_packet\": 1, \"report_seen_packet\": 1}]}\n__CUE_LIVE__\n";
        assert!(parse_remote(out, "box").1["CA"].reviewed, "on a machine too");
    }

    #[test]
    fn a_command_here_says_what_it_printed_or_its_last_complaint() {
        let ok = Cmd { bin: "/bin/sh".into(), args: vec!["-c".into(), "echo done".into()], machine: String::new() };
        assert_eq!(ok.run().unwrap().trim(), "done");
        let bad = Cmd { bin: "/bin/sh".into(), args: vec!["-c".into(), "echo first >&2; echo no such session: x >&2; exit 1".into()], machine: String::new() };
        assert_eq!(bad.run().unwrap_err(), "no such session: x");
    }

    use super::*;

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// The tests that fill INDEX / REMOTE take turns.
    static GLOBALS: Mutex<()> = Mutex::new(());

    #[test]
    fn relay_and_pilead_leads_own_their_executors() {
        let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("cue-leads-{}", std::process::id()));
        let (relay, pi, cs) = (root.join("relay"), root.join("pi"), root.join("claude-sessions"));
        // L1 is open in a live process (this test's own pid); GHOST has a marker but no process.
        write(&cs.join("1.json"), &format!(r#"{{"pid":{},"sessionId":"L1"}}"#, std::process::id()));
        write(&cs.join("2.json"), r#"{"pid":999999,"sessionId":"GHOST"}"#);
        write(&relay.join("lead/GHOST/marker.json"), r#"{"session_id":"GHOST","project":"crashed"}"#);
        write(&relay.join("ghosted-1/session.json"), r#"{"session_id":"ghosted-1","agent":"relay-executor","claude_session":"C4","owner_lead":"GHOST","topic":"ghosted","status":"reported"}"#);
        write(&relay.join("lead/L1/marker.json"), r#"{"session_id":"L1","project":"chart-lines","color":[132,180,180],"autonomous":true}"#);
        write(&relay.join("finder-1/packets/002-packet.md"), "GOAL: tune the finder from this week's corrections\n\n## Context\n…");
        write(&relay.join("finder-1/packets/002-report.md"), "\nGates rise on 3 of 4 sets; staged.\n\nStatus: clean\n");
        write(&relay.join("finder-1/session.json"), r#"{"session_id":"finder-1","agent":"relay-executor","claude_session":"C1","owner_lead":"L1","owner_project":"old-name","topic":"finder","model":"claude-sonnet-5-5","status":"reported","current_packet":2}"#);
        write(&relay.join("old-1/session.json"), r#"{"session_id":"old-1","agent":"relay-executor","claude_session":"C2","owner_lead":"L1","topic":"old","status":"closed"}"#);
        write(&relay.join("orphan-1/session.json"), r#"{"session_id":"orphan-1","agent":"relay-executor","claude_session":"C3","owner_lead":"GONE","owner_project":"claude-relay","topic":"orphan","status":"reported"}"#);
        write(&pi.join("leads/P1.json"), r#"{"sid":"P1","project":"pi-lead"}"#);
        write(&pi.join("leads/P1.inbox.md"), "");
        write(&pi.join("sessions/b12-1/meta.json"), r#"{"sid":"b12-1","lead":"P1","topic":"b12","model":"deepseek/deepseek-flash","status":"busy","packets":1}"#);
        write(&pi.join("sessions/b12-1/status"), "reported\n");

        let index = scan(&relay, &pi, &cs);
        let c1 = &index["C1"];
        assert_eq!((c1.role.as_str(), c1.name.as_str(), c1.lead_name.as_str(), c1.packet), ("executor", "finder", "chart-lines", Some(2)));
        assert_eq!(c1.color, Some([132, 180, 180]));
        assert_eq!((c1.goal.as_str(), c1.outcome.as_str()), ("tune the finder from this week's corrections", "Gates rise on 3 of 4 sets; staged."));
        assert_eq!(index["C2"].goal, "", "a closed executor's packet isn't read");
        assert_eq!(index["C3"].lead_armed, Some(false));
        assert_eq!(index["C4"].lead_armed, Some(false), "a marker with no live process is a ghost");
        assert_eq!(index["C4"].lead_name, "crashed");
        assert_eq!(index["C3"].lead_name, "claude-relay");
        assert_eq!(index["b12-1"].status, "reported", "the status file wins over meta.json");
        assert_eq!(index["b12-1"].lead_name, "pi-lead");
        assert_eq!(index["P1"].role, "lead");

        *crate::locking::lock(&INDEX, "INDEX") = Some(index);
        assert_eq!(resolve_driver("C1", ""), "its relay lead (chart-lines)");
        assert_eq!(resolve_driver("b12-1", ""), "its pi-lead lead (pi-lead)");
        assert_eq!(resolve_driver("C3", "its relay lead (claude-relay)"), "", "no armed lead: its report is yours");
        assert_eq!(resolve_driver("L1", ""), "");
        assert_eq!(resolve_driver("unknown", "its relay lead (x)"), "its relay lead (x)", "not in the index: the hook's word stands");

        assert_eq!(act("C1", "review"), Ok(Act::Send { to: "L1".into(), text: "/relay:review finder-1".into() }));
        assert_eq!(act("b12-1", "review"), Ok(Act::Send { to: "P1".into(), text: "/pilead:review b12-1".into() }));
        assert!(act("P1", "board").is_err(), "no board from Cue");
        assert!(act("C3", "review").unwrap_err().contains("isn't listening"), "no review sent to a lead nobody's running");
        assert!(act("L1", "review").is_err());
        assert!(act("nobody", "diff").is_err());

        assert!(close_plan("L1").unwrap().unwrap_err().contains("leads 1 executor still open"), "a lead with open executors isn't closed from Cue");
        assert_eq!(close_plan("nobody"), None);
        assert_eq!(close_plan("C2"), None, "an executor relay already closed (its agent resumed elsewhere) is ended as any session");

        let known: HashSet<String> = ["L1", "P1", "nobody"].iter().map(|s| s.to_string()).collect();
        let v = view(&known);
        let names: Vec<&str> = v["L1"]["executors"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["finder"], "closed executors and other leads' are left out");
        assert_eq!(v["P1"]["executors"][0]["session_id"], "b12-1");
        assert!(v.get("nobody").is_none());

        // A record that changes is read again; one that's removed drops out of the cache.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&relay.join("finder-1/session.json"), r#"{"session_id":"finder-1","agent":"relay-executor","claude_session":"C1","owner_lead":"L1","topic":"finder","status":"closed","current_packet":2}"#);
        std::fs::remove_dir_all(relay.join("old-1")).unwrap();
        let again = scan(&relay, &pi, &cs);
        assert_eq!(again["C1"].status, "closed");
        assert!(!again.contains_key("C2"));
        assert!(!crate::locking::lock(&PARSED, "PARSED").as_ref().unwrap().files.keys().any(|p| p.starts_with(relay.join("old-1"))));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_a_plans_queue() {
        let plan: Value = serde_json::from_str(r#"{"items":[
            {"n":1,"packet":"_staging/a-packet.md","executor":"a-1","done":"2026-09-13T14:57:51"},
            {"n":2,"packet":"_staging/b-packet.md","executor":"b-1","done":null},
            {"n":3,"packet":"_staging/docs-refresh-packet.md","note":"","executor":null},
            {"n":4,"packet":"_staging/d-packet.md","note":"row 90"}]}"#).unwrap();
        assert_eq!(plan_queue(&plan), ("docs-refresh".to_string(), 2), "done and bound items aren't queued");
        assert_eq!(plan_queue(&serde_json::json!({})), (String::new(), 0));
    }

    #[test]
    fn reads_relay_verifys_verdict() {
        let ok = "  VERDICT: COUNTS-MATCH\n  STAGED REALITY\n    claimed, NOT staged:  0\n";
        assert_eq!(parse_verify(ok), Some(Verdict { verdict: "COUNTS-MATCH".into(), note: String::new() }));
        let off = "  VERDICT: MISMATCH\n    claimed and staged:   3\n    claimed, NOT staged:  2\n";
        assert_eq!(parse_verify(off).unwrap().note, "2 changes it reports are not staged");
        assert_eq!(parse_verify("no such session: x"), None);
    }

    fn done_exec() -> Member {
        Member { role: "executor".into(), plugin: "relay".into(), status: "reported".into(), lead: "L".into(), lead_armed: Some(true), ready_late: true, ..Default::default() }
    }

    #[test]
    fn a_report_is_late_once_it_has_waited_ninety_seconds() {
        let at = epoch_secs("2026-10-04T09:50:00", None).unwrap();
        assert_eq!(epoch_secs("2026-10-04T09:50:00.123456", None), Some(at), "a fraction is ignored");
        assert_eq!(epoch_secs("2026-10-04T09:51:00", None), Some(at + 60));
        assert_eq!(epoch_secs("not a time", None), None);
        assert_eq!(epoch_secs("", None), None);
        let e = |status: &str| serde_json::json!({ "status": status, "updated": "2026-10-04T09:50:00" });
        assert!(!ready_late(&e("reported"), at + 89, None), "not yet");
        assert!(ready_late(&e("reported"), at + 90, None));
        assert!(ready_late(&e("idle"), at + 600, None));
        assert!(!ready_late(&e("busy"), at + 600, None), "still at work: no report to miss");
        assert!(!ready_late(&serde_json::json!({ "status": "reported" }), at + 600, None), "no timestamp: no claim");
    }

    #[test]
    fn a_machines_timestamps_are_read_in_its_own_zone() {
        // 09:50 at +0900 is 00:50 UTC; at -0800 it's 17:50 UTC.
        assert_eq!(epoch_secs("2026-10-04T09:50:00", Some(9 * 3600)), Some(1_791_075_000));
        assert_eq!(epoch_secs("2026-10-04T09:50:00", Some(0)), Some(1_791_075_000 + 9 * 3600));
        assert_eq!(epoch_secs("2026-10-04T09:50:00", Some(-8 * 3600)), Some(1_791_075_000 + 17 * 3600));
        assert_eq!(tz_of("x\n__CUE_LIVE__\nL\n__CUE_TZ__:+0530\n"), Some(19_800));
        assert_eq!(tz_of("__CUE_TZ__:-0800\n"), Some(-28_800));
        assert_eq!(tz_of("__CUE_TZ__:+0000"), Some(0));
        assert_eq!(tz_of("__CUE_TZ__:"), None);
        assert_eq!(tz_of("__CUE_TZ__:oops"), None);
        assert_eq!(tz_of("no offset said"), None);
        let ready = |zone: Option<i64>, ts: &str| ready_late(&serde_json::json!({ "status": "reported", "updated": ts }), 1_791_075_000 + 120, zone);
        assert!(ready(Some(9 * 3600), "2026-10-04T09:50:00"), "ready 120 s ago there");
        assert!(!ready(Some(9 * 3600), "2026-10-04T09:51:00"), "ready 60 s ago there");
        assert!(!ready(Some(-8 * 3600), "2026-10-04T09:50:00"), "in a zone 17 h behind, 09:50 hasn't happened yet");
    }

    /// A machine's answer to REMOTE_LIST: its UTC offset, and one reported executor whose record was
    /// last written `secs_ago` seconds ago by that machine's clock (its local time, not ours).
    fn listing_in_zone(off: i64, secs_ago: i64) -> String {
        let t = (now_secs() as i64 + off - secs_ago) as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::gmtime_r(&t, &mut tm) };
        let updated = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec);
        let z = format!("{}{:02}{:02}", if off < 0 { '-' } else { '+' }, off.abs() / 3600, off.abs() % 3600 / 60);
        format!("BIN:/r/relay\n{{\"leads\": [{{\"session_id\": \"ZL\", \"project\": \"p\"}}], \"executors\": [{{\"agent\": \"relay-executor\", \"session_id\": \"z-1\", \"status\": \"reported\", \"current_packet\": 1, \"owner_lead\": \"ZL\", \"claude_session\": \"ZC\", \"updated\": \"{updated}\"}}]}}\n__CUE_LIVE__\nZL\n__CUE_TZ__:{z}\n")
    }

    #[test]
    fn a_machine_in_another_zone_is_judged_by_its_own_clock() {
        for off in [9 * 3600, -8 * 3600, 5 * 3600 + 1800, 0] {
            assert!(parse_remote(&listing_in_zone(off, 300), "far").1["ZC"].ready_late, "ready 5 min ago at {off}");
            assert!(!parse_remote(&listing_in_zone(off, 30), "far").1["ZC"].ready_late, "ready 30 s ago at {off}");
        }
        // Without its offset the timestamp can only be read here: a machine 9 h ahead looks 9 h in the future.
        let blind = listing_in_zone(9 * 3600, 300).replace("__CUE_TZ__:+0900", "");
        assert!(!parse_remote(&blind, "far").1["ZC"].ready_late || local_offset_secs() >= 9 * 3600, "no offset: read as this Mac's local time");
    }

    /// This Mac's UTC offset now, in seconds.
    fn local_offset_secs() -> i64 {
        let t = now_secs() as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&t, &mut tm) };
        tm.tm_gmtoff as i64
    }

    #[test]
    fn a_missed_wake_needs_every_condition() {
        assert!(missed_wake(&done_exec(), true));
        assert!(missed_wake(&Member { status: "idle".into(), ..done_exec() }, true), "idle is done too");
        assert!(!missed_wake(&Member { status: "busy".into(), ..done_exec() }, true), "not done");
        assert!(!missed_wake(&Member { status: "stalled".into(), ..done_exec() }, true), "stalled never reported");
        assert!(!missed_wake(&Member { reviewed: true, ..done_exec() }, true), "its lead has seen it");
        assert!(!missed_wake(&Member { ready_late: false, ..done_exec() }, true), "ready under 90 seconds ago");
        assert!(!missed_wake(&Member { lead_armed: Some(false), ..done_exec() }, true), "its lead isn't listening");
        assert!(!missed_wake(&Member { lead_armed: None, ..done_exec() }, true));
        assert!(!missed_wake(&done_exec(), false), "its lead is working, compacting or asking");
        assert!(!missed_wake(&Member { role: "lead".into(), ..done_exec() }, true));
    }

    #[test]
    fn the_scan_marks_a_late_report_and_lists_it_when_its_lead_is_free() {
        let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        // Reported long ago, never seen; a listening lead (a machine's live list).
        let out = r#"BIN:/r/relay
{"leads": [{"session_id": "ML", "project": "p"}],
 "executors": [{"agent": "relay-executor", "session_id": "old-1", "status": "reported", "current_packet": 1, "owner_lead": "ML", "claude_session": "MC1", "updated": "2020-01-01T00:00:00"},
               {"agent": "relay-executor", "session_id": "seen-1", "status": "reported", "current_packet": 1, "report_seen_packet": 1, "owner_lead": "ML", "claude_session": "MC2", "updated": "2020-01-01T00:00:00"},
               {"agent": "relay-executor", "session_id": "fresh-1", "status": "reported", "current_packet": 1, "owner_lead": "ML", "claude_session": "MC3", "updated": "2999-01-01T00:00:00"}]}
__CUE_LIVE__
ML
"#;
        let index = parse_remote(out, "box").1;
        assert!(index["MC1"].ready_late && !index["MC3"].ready_late);
        *crate::locking::lock(&INDEX, "INDEX") = Some(index);
        assert_eq!(missed(|_| true), ["MC1"], "seen and fresh reports aren't missed");
        assert!(missed(|_| false).is_empty(), "its lead is busy");
    }

    #[test]
    fn closing_a_stalled_executor_runs_relays_close_where_it_runs() {
        let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        // A stalled executor that never got a Claude session (it sat at the trust prompt) is still listed.
        let out = r#"BIN:/r/relay
{"leads": [{"session_id": "ML", "project": "p"}],
 "executors": [{"agent": "relay-executor", "session_id": "stuck-1", "topic": "stuck", "status": "stalled", "owner_lead": "ML"},
               {"agent": "relay-executor", "session_id": "busy-1", "status": "busy", "owner_lead": "ML"},
               {"agent": "relay-executor", "session_id": "gone-1", "topic": "gone", "status": "dead", "owner_lead": "ML", "claude_session": "MC9"}]}
__CUE_LIVE__
ML
"#;
        let (bin, index) = parse_remote(out, "box");
        assert!(index.contains_key("relay:stuck-1"), "no Claude session: filed under relay's id");
        assert!(!index.contains_key("relay:busy-1"), "only a stalled or dead one gets that");
        *crate::locking::lock(&REMOTE, "REMOTE") = Some(HashMap::from([("box".to_string(), (bin, index.clone()))]));
        *crate::locking::lock(&INDEX, "INDEX") = Some(index);
        let close = Cmd { bin: "/r/relay".into(), args: vec!["close".into(), "stuck-1".into()], machine: "box".into() };
        assert_eq!(act("relay:stuck-1", "close"), Ok(Act::Run(close, "Closed".into())));
        assert_eq!(act("MC9", "close"), Ok(Act::Run(Cmd { bin: "/r/relay".into(), args: vec!["close".into(), "gone-1".into()], machine: "box".into() }, "Closed".into())));
        assert!(act("ML", "close").is_err(), "a lead isn't closed that way");
        let v = view(&HashSet::from(["ML".to_string()]));
        let rows: Vec<(&str, &str)> = v["ML"]["executors"].as_array().unwrap().iter().map(|e| (e["session_id"].as_str().unwrap(), e["status"].as_str().unwrap())).collect();
        assert!(rows.contains(&("relay:stuck-1", "stalled")), "its row stays under the lead: {rows:?}");
        assert!(rows.contains(&("MC9", "dead")), "a dead one too, with its Claude session: {rows:?}");
        assert!(!rows.iter().any(|r| r.0 == "relay:busy-1"));
        // Closed, relay's record says so on the next listing, and the row goes.
        let after = parse_remote(&out.replace("\"stalled\"", "\"closed\""), "box").1;
        assert!(!view_rows(&after, "ML").iter().any(|s| s == "relay:stuck-1"));
    }

    fn view_rows(index: &HashMap<String, Member>, lead: &str) -> Vec<String> {
        index.iter().filter(|(_, e)| e.role == "executor" && e.lead == lead && !GONE.contains(&e.status.as_str())).map(|(sid, _)| sid.clone()).collect()
    }

    #[test]
    fn a_lead_with_no_executors_left_is_stepped_down_by_relay_before_it_is_ended() {
        let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let out = r#"BIN:/r/relay
{"leads": [{"session_id": "EMPTY", "project": "a"}, {"session_id": "BUSY", "project": "b"}],
 "executors": [{"agent": "relay-executor", "session_id": "done-1", "status": "closed", "owner_lead": "EMPTY", "claude_session": "X1"},
               {"agent": "relay-executor", "session_id": "work-1", "status": "busy", "owner_lead": "BUSY", "claude_session": "X2"}]}
__CUE_LIVE__
EMPTY
BUSY
"#;
        let (bin, mut index) = parse_remote(out, "box");
        index.insert("PL".into(), Member { role: "lead".into(), plugin: "pilead".into(), ..Default::default() });
        *crate::locking::lock(&REMOTE, "REMOTE") = Some(HashMap::from([("box".to_string(), (bin, index.clone()))]));
        *crate::locking::lock(&INDEX, "INDEX") = Some(index);
        assert_eq!(lead_stop("EMPTY"), Some(Ok(Cmd { bin: "/r/relay".into(), args: vec!["close".into(), "--self".into(), "EMPTY".into()], machine: "box".into() })), "run on its machine");
        assert_eq!(lead_stop("BUSY"), None, "it still has an executor open: close_plan refuses it");
        assert_eq!(lead_stop("PL"), None, "pi-lead leads are left as they are");
        assert_eq!(lead_stop("X1"), None, "an executor is closed by close_plan");
        assert_eq!(lead_stop("nobody"), None);
        assert_eq!(close_plan("EMPTY"), None, "close_plan itself is unchanged for it");
    }

    #[test]
    fn a_machines_relay_may_or_may_not_list_goal_outcome_and_verify() {
        let out = r#"BIN:/r/relay
{"leads": [{"session_id": "L1", "project": "p"}],
 "executors": [{"agent": "relay-executor", "session_id": "new-1", "topic": "new", "status": "reported", "current_packet": 1, "owner_lead": "L1", "claude_session": "C1",
                "goal": "add CSV export", "outcome": "Export writes trades.csv", "verify": {"verdict": "MISMATCH", "ts": "2026-10-10T10:00:00", "rerun": false, "mismatches": 2}},
               {"agent": "relay-executor", "session_id": "new-2", "topic": "pending", "status": "busy", "current_packet": 1, "owner_lead": "L1", "claude_session": "C2", "goal": "tune it", "outcome": "", "verify": null},
               {"agent": "relay-executor", "session_id": "old-1", "topic": "older", "status": "reported", "current_packet": 1, "owner_lead": "L1", "claude_session": "C3"},
               {"agent": "relay-executor", "session_id": "bad-1", "topic": "odd", "status": "busy", "owner_lead": "L1", "claude_session": "C4", "goal": 7, "verify": "COUNTS-MATCH"},
               {"agent": "relay-executor", "session_id": "gone-1", "topic": "gone", "status": "closed", "owner_lead": "L1", "claude_session": "C5", "goal": "stale"}]}
__CUE_LIVE__
"#;
        let index = parse_remote(out, "box").1;
        let c1 = &index["C1"];
        assert_eq!((c1.goal.as_str(), c1.outcome.as_str()), ("add CSV export", "Export writes trades.csv"));
        assert_eq!(c1.verify, Some(Verdict { verdict: "MISMATCH".into(), note: "2 mismatches".into() }));
        assert_eq!((index["C2"].goal.as_str(), index["C2"].outcome.as_str(), index["C2"].verify.clone()), ("tune it", "", None), "no report yet, verify null");
        let o = &index["C3"];
        assert_eq!((o.goal.as_str(), o.outcome.as_str(), o.verify.clone(), o.status.as_str()), ("", "", None, "reported"), "an older relay sends none of them: nothing to show, no error");
        assert_eq!((index["C4"].goal.as_str(), index["C4"].verify.clone()), ("", None), "a field of the wrong type is ignored");
        assert_eq!(index["C5"].goal, "", "a closed executor's isn't kept");
        let long = format!(r#"BIN:/r/relay
{{"leads": [], "executors": [{{"agent": "relay-executor", "session_id": "l", "claude_session": "CL", "status": "busy", "goal": "{}"}}]}}
"#, "x".repeat(400));
        assert_eq!(parse_remote(&long, "box").1["CL"].goal.chars().count(), 300, "clipped, however long a machine sends it");
    }

    #[test]
    fn a_machines_relay_and_pilead_executors_are_known_as_executors_and_their_leads_arent() {
        let relay = r#"{"leads": [{"session_id": "L9", "project": "smoke"}],
 "executors": [{"agent": "relay-executor", "session_id": "hello-md", "topic": "hello-md", "status": "busy", "owner_lead": "L9", "claude_session": "C9"},
               {"agent": "relay-executor", "session_id": "old", "topic": "old", "status": "closed", "owner_lead": "L9", "claude_session": "C7"}]}"#;
        let out = format!("BIN:/x/relay\n{relay}\n__CUE_PILEAD__\nPBIN:/x/pilead\n{PI_LISTING}\n__CUE_LIVE__\nL9\n");
        let (_, index) = parse_remote(&out, "box");
        for sid in ["C9", "C7", "pe-1", "pe-2"] {
            assert!(executor_in(&index, sid), "{sid}: an executor, its lead's to bring back");
        }
        for sid in ["L9", "PL1", "someone-else"] {
            assert!(!executor_in(&index, sid), "{sid}: not an executor");
        }
    }

    #[test]
    fn a_machines_executors_name_their_own_tmux_session_when_their_listing_does() {
        let relay = r#"{"leads": [{"session_id": "L9", "project": "smoke", "tmux_session": "relay-lead-smoke"}],
 "executors": [{"agent": "relay-executor", "session_id": "hello-md", "topic": "hello-md", "status": "busy", "owner_lead": "L9", "claude_session": "C9", "tmux_session": "relay-hello-md_2"},
               {"agent": "relay-executor", "session_id": "no-key", "topic": "no-key", "status": "busy", "owner_lead": "L9", "claude_session": "C8"},
               {"agent": "relay-executor", "session_id": "odd", "topic": "odd", "status": "busy", "owner_lead": "L9", "claude_session": "C7", "tmux_session": "x; rm -rf ~"},
               {"agent": "relay-executor", "session_id": "colon", "topic": "colon", "status": "busy", "owner_lead": "L9", "claude_session": "C6", "tmux_session": "lead:0.1"},
               {"agent": "relay-executor", "session_id": "blank", "topic": "blank", "status": "busy", "owner_lead": "L9", "claude_session": "C5", "tmux_session": "  "}]}"#;
        let pi = r#"{"leads": [{"sid": "PL1", "project": "pi-proj", "live": "live"}],
 "executors": [{"sid": "pe-1", "lead": "PL1", "status": "busy", "topic": "parser", "tmux_session": "pilead-pe-1"},
               {"sid": "pe-2", "lead": "PL1", "status": "busy", "topic": "wire"},
               {"sid": "pe-3", "lead": "PL1", "status": "busy", "topic": "quoted", "tmux_session": "pe 3"}]}"#;
        let out = format!("BIN:/x/relay\n{relay}\n__CUE_PILEAD__\nPBIN:/x/pilead\n{pi}\n__CUE_LIVE__\nL9\nPL1\n");
        let (_, index) = parse_remote(&out, "box");
        // Present: that executor's own session, on its machine (relay's and pi-lead's alike).
        assert_eq!(exec_tmux_in(&index, "C9"), Some(("box".into(), "relay-hello-md_2".into())));
        assert_eq!(exec_tmux_in(&index, "pe-1"), Some(("box".into(), "pilead-pe-1".into())));
        // Missing (an older relay / pi-lead): nothing, so its terminal opens as it did before.
        for sid in ["C8", "pe-2"] {
            assert_eq!((index[sid].tmux_session.as_str(), exec_tmux_in(&index, sid)), ("", None), "{sid}");
        }
        // Anything but letters, digits, _ and -: refused, never handed to the shell or to tmux.
        for sid in ["C7", "C6", "C5", "pe-3"] {
            assert_eq!((index[sid].tmux_session.as_str(), exec_tmux_in(&index, sid)), ("", None), "{sid}");
        }
        // A lead is never what an executor's terminal attaches to, and a lead has none of its own here.
        assert_eq!(exec_tmux_in(&index, "L9"), None);
        assert_eq!(index["L9"].tmux_session, "");
        // On this Mac (no machine): not this path either.
        let mut local = index["C9"].clone();
        local.machine.clear();
        assert_eq!(exec_tmux_in(&HashMap::from([("C9".to_string(), local)]), "C9"), None);
        // The window gets it on the lead's executor list, for the rows it draws.
        let v = serde_json::to_value(&index["C9"]).unwrap();
        assert_eq!(v["tmux_session"], "relay-hello-md_2");
        assert!(serde_json::to_value(&index["C8"]).unwrap().get("tmux_session").is_none());
    }

    const PI_LISTING: &str = r#"{"leads": [{"sid": "PL1", "project": "pi-proj", "live": "live", "model": "deepseek/flash", "started": "2026-10-10T09:00:00Z", "waiting": 0, "wake": "on"},
                                       {"sid": "PL2", "project": "dead-proj", "live": "ghost", "started": "2026-10-01T09:00:00Z"}],
 "executors": [{"sid": "pe-1", "lead": "PL1", "project": "pi-proj", "status": "reported", "topic": "parser", "label": "[Exec] pe-1", "packet": 2, "goal": "write the parser", "outcome": "Parser handles nested lists", "reported": true, "worktree": "/w/1", "model": "deepseek/flash", "created": "2026-10-10T09:05:00Z"},
               {"sid": "pe-2", "lead": "PL1", "project": "pi-proj", "status": "busy", "topic": "", "packet": "1", "goal": "wire it up", "outcome": "", "reported": false},
               {"sid": "pe-3", "lead": "PL2", "project": "dead-proj", "status": "busy", "topic": "orphan", "packet": 1},
               {"sid": "pe-4", "lead": "PL1", "project": "pi-proj", "status": "closed", "topic": "done", "goal": "old goal"}]}"#;

    #[test]
    fn a_machines_pilead_crews_sit_beside_relays_in_the_same_tree() {
        let out = format!("BIN:/r/relay\n{{\"leads\": [{{\"session_id\": \"L1\", \"project\": \"rl\"}}], \"executors\": []}}\n__CUE_PILEAD__\nPBIN:/home/me/pi-lead/bin/pilead\n{PI_LISTING}\n__CUE_LIVE__\nL1\n__CUE_TZ__:+0000\n");
        let (bin, index) = parse_remote(&out, "build-box");
        assert_eq!(bin, "/r/relay", "relay's command is relay's");
        assert_eq!(pilead_bin_of(&out), "/home/me/pi-lead/bin/pilead");
        assert_eq!(index["L1"].plugin, "relay");
        let l = &index["PL1"];
        assert_eq!((l.role.as_str(), l.plugin.as_str(), l.name.as_str(), l.machine.as_str()), ("lead", "pilead", "pi-proj", "build-box"));
        let e = &index["pe-1"];
        assert_eq!((e.role.as_str(), e.plugin.as_str(), e.name.as_str(), e.lead.as_str(), e.lead_name.as_str(), e.lead_armed, e.packet, e.machine.as_str(), e.id.as_str()),
            ("executor", "pilead", "parser", "PL1", "pi-proj", Some(true), Some(2), "build-box", "pe-1"));
        assert_eq!((e.status.as_str(), e.goal.as_str(), e.outcome.as_str()), ("reported", "write the parser", "Parser handles nested lists"));
        assert_eq!((index["pe-2"].name.as_str(), index["pe-2"].packet, index["pe-2"].outcome.as_str()), ("pe-2", Some(1), ""), "no topic: its id; packet as a string reads too");
        assert_eq!(index["pe-3"].lead_armed, Some(false), "its lead is a ghost record");
        assert_eq!(index["pe-4"].goal, "", "closed: no goal kept");
        assert_eq!(live_of(&out), HashSet::from(["L1".to_string()]), "the live list is still what follows its own marker");

        // pilead alone (no relay there).
        let only = format!("NORELAY\n__CUE_PILEAD__\nPBIN:/p/pilead\n{PI_LISTING}\n__CUE_LIVE__\n");
        let (bin, index) = parse_remote(&only, "box");
        assert_eq!(bin, "");
        assert!(index.contains_key("pe-1") && index.contains_key("PL1") && !index.contains_key("L1"));
    }

    #[test]
    fn a_machine_without_pilead_lists_exactly_as_before() {
        let relay = r#"BIN:/r/relay
{"leads": [{"session_id": "L1", "project": "p"}], "executors": [{"agent": "relay-executor", "session_id": "e1", "claude_session": "C1", "owner_lead": "L1", "status": "busy"}]}
"#;
        let before = parse_remote(&format!("{relay}__CUE_LIVE__\nL1\n"), "box");
        let without = parse_remote(&format!("{relay}__CUE_PILEAD__\nNOPILEAD\n__CUE_LIVE__\nL1\n"), "box");
        assert_eq!(before, without, "the new marker adds nothing");
        assert_eq!(without.1.keys().cloned().collect::<HashSet<_>>(), HashSet::from(["L1".to_string(), "C1".to_string()]));
        assert_eq!(pilead_bin_of(&format!("{relay}__CUE_PILEAD__\nNOPILEAD\n")), "");
        assert_eq!(pilead_bin_of(relay), "");
        let neither = parse_remote("NORELAY\n__CUE_PILEAD__\nNOPILEAD\n__CUE_LIVE__\n__CUE_TZ__:+0000\n", "box");
        assert!(neither.1.is_empty() && neither.0.is_empty());
        // pilead that printed something that isn't JSON (a traceback, an older CLI): no crews, no error.
        assert!(parse_remote("NORELAY\n__CUE_PILEAD__\nPBIN:/p/pilead\nTraceback (most recent call last):\n__CUE_LIVE__\n", "box").1.is_empty());
    }

    #[test]
    fn only_the_buttons_pilead_has_a_command_for_are_wired_for_its_remote_executors() {
        let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let out = format!("NORELAY\n__CUE_PILEAD__\nPBIN:/p/pilead\n{PI_LISTING}\n__CUE_LIVE__\n");
        let index = parse_remote(&out, "box").1;
        *crate::locking::lock(&INDEX, "INDEX") = Some(index);
        *crate::locking::lock(&REMOTE_PILEAD, "REMOTE_PILEAD") = Some(HashMap::from([("box".to_string(), "/p/pilead".to_string())]));
        assert_eq!(act("pe-1", "review"), Ok(Act::Send { to: "PL1".into(), text: "/pilead:review pe-1".into() }));
        assert!(act("pe-1", "diff").unwrap_err().contains("can't bring it here"), "no remote diff page for pilead");
        assert_eq!(close_plan("pe-1"), Some(Ok(Cmd { bin: "/p/pilead".into(), args: vec!["close".into(), "pe-1".into()], machine: "box".into() })));
        *crate::locking::lock(&REMOTE_PILEAD, "REMOTE_PILEAD") = None;
        assert!(close_plan("pe-1").unwrap().unwrap_err().contains("can't find pilead's command on box"));
        *crate::locking::lock(&INDEX, "INDEX") = None;
    }

    #[test]
    fn the_ledger_is_read_incrementally() {
        let dir = std::env::temp_dir().join(format!("cue-ledger-inc-{}", std::process::id()));
        let path = dir.join("sessions.jsonl");
        let ev = |sid: &str, n: u64, v: &str| format!("{{\"event\":\"report_verify\",\"session_id\":\"{sid}\",\"packet\":{n},\"verdict\":\"{v}\",\"mismatches\":0}}\n");
        let get = |sid: &str, n: u64| ledger_verdicts(&path).get(&(sid.to_string(), n)).map(|v| v.verdict.clone());
        let offset = || crate::locking::lock(&LEDGER, "LEDGER").as_ref().and_then(|c| c.get(&path)).map(|l| l.offset).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(get("a", 1), None, "no ledger yet: no verdicts");

        // First read: everything there.
        let first = format!("{}{}", ev("a", 1, "MISMATCH"), ev("b", 1, "COUNTS-MATCH"));
        write(&path, &first);
        assert_eq!((get("a", 1).as_deref(), get("b", 1).as_deref()), (Some("MISMATCH"), Some("COUNTS-MATCH")));
        assert_eq!(offset(), first.len() as u64);

        // Appended lines: only they are read, and a later verdict for the same packet wins.
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut f, format!("{}{}", ev("a", 1, "COUNTS-MATCH"), ev("c", 2, "MALFORMED")).as_bytes()).unwrap();
        assert_eq!((get("a", 1).as_deref(), get("c", 2).as_deref()), (Some("COUNTS-MATCH"), Some("MALFORMED")));
        // Proof it didn't start over: corrupt the already-read part in place (same length); the old verdict stands.
        let len = std::fs::metadata(&path).unwrap().len();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[..first.len()].fill(b' ');
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), len);
        assert_eq!(get("b", 1).as_deref(), Some("COUNTS-MATCH"), "the bytes already read aren't read again");
        let after_appends = offset();

        // A partial last line waits, then counts once it's finished.
        let line = ev("d", 3, "MISMATCH");
        let (head, tail) = line.split_at(line.len() / 2);
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut f, head.as_bytes()).unwrap();
        assert_eq!(get("d", 3), None, "half a line isn't a verdict");
        assert_eq!(offset(), after_appends, "the partial line isn't consumed");
        std::io::Write::write_all(&mut f, tail.as_bytes()).unwrap();
        assert_eq!(get("d", 3).as_deref(), Some("MISMATCH"));
        assert_eq!(get("a", 1).as_deref(), Some("COUNTS-MATCH"));

        // The ledger shrinks (replaced by a shorter one, same file): start over from its beginning.
        write(&path, &ev("z", 9, "COUNTS-MATCH"));
        assert_eq!((get("z", 9).as_deref(), get("a", 1)), (Some("COUNTS-MATCH"), None), "only what's in the new ledger");

        // A different file at the same path (a new inode, even a longer one) is read from the start too.
        std::fs::remove_file(&path).unwrap();
        write(&path, &format!("{}{}{}", ev("y", 1, "MALFORMED"), ev("y", 2, "MALFORMED"), ev("y", 3, "MALFORMED")));
        assert_eq!((get("y", 3).as_deref(), get("z", 9)), (Some("MALFORMED"), None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_a_local_executors_verdict_from_relays_ledger() {
        let root = std::env::temp_dir().join(format!("cue-ledger-{}", std::process::id()));
        let (relay, pi, cs) = (root.join("relay"), root.join("pi"), root.join("cs"));
        write(&relay.join("sessions.jsonl"), concat!(
            r#"{"ts":"2026-10-10T10:00:00","event":"spawn","session_id":"ex-1"}"#, "\n",
            r#"{"ts":"2026-10-10T10:05:00","event":"report_verify","session_id":"ex-1","packet":2,"verdict":"MISMATCH","rerun":false,"mismatches":1}"#, "\n",
            "not json but mentions \"report_verify\"\n",
            r#"{"ts":"2026-10-10T10:09:00","event":"report_verify","session_id":"ex-1","packet":2,"verdict":"COUNTS-MATCH","rerun":false,"mismatches":0}"#, "\n",
            r#"{"ts":"2026-10-10T10:09:00","event":"report_verify","session_id":"ex-2","packet":1,"verdict":"MALFORMED","rerun":false,"mismatches":0}"#, "\n"));
        write(&relay.join("ex-1/session.json"), r#"{"session_id":"ex-1","agent":"relay-executor","claude_session":"C1","owner_lead":"L","topic":"one","status":"reported","current_packet":2}"#);
        write(&relay.join("ex-1/packets/002-packet.md"), "GOAL: first\n");
        write(&relay.join("ex-2/session.json"), r#"{"session_id":"ex-2","agent":"relay-executor","claude_session":"C2","owner_lead":"L","topic":"two","status":"reported","current_packet":1}"#);
        write(&relay.join("ex-3/session.json"), r#"{"session_id":"ex-3","agent":"relay-executor","claude_session":"C3","owner_lead":"L","topic":"three","status":"busy","current_packet":4}"#);
        write(&relay.join("ex-4/session.json"), r#"{"session_id":"ex-4","agent":"relay-executor","claude_session":"C4","owner_lead":"L","topic":"four","status":"closed","current_packet":2}"#);
        let index = scan(&relay, &pi, &cs);
        assert_eq!(index["C1"].verify, Some(Verdict { verdict: "COUNTS-MATCH".into(), note: String::new() }), "the last verdict for the packet wins");
        assert_eq!(index["C2"].verify.as_ref().map(|v| v.verdict.as_str()), Some("MALFORMED"));
        assert_eq!(index["C3"].verify, None, "no verdict for packet 4 yet");
        assert_eq!(index["C4"].verify, None, "closed: not read");
        assert_eq!((index["C1"].goal.as_str(), index["C1"].outcome.as_str()), ("first", ""), "goal from the packet; no report yet");
        // The ledger grows: the next scan sees the new line.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut text = std::fs::read_to_string(relay.join("sessions.jsonl")).unwrap();
        text.push_str("{\"event\":\"report_verify\",\"session_id\":\"ex-3\",\"packet\":4,\"verdict\":\"MISMATCH\",\"mismatches\":3}\n");
        write(&relay.join("sessions.jsonl"), &text);
        assert_eq!(scan(&relay, &pi, &cs)["C3"].verify, Some(Verdict { verdict: "MISMATCH".into(), note: "3 mismatches".into() }));
        // The window gets it as `verify`, on the executor and in its lead's list.
        assert_eq!(verdict_of(&index["C2"]).map(|v| v.verdict), Some("MALFORMED".to_string()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
