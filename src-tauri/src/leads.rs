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
    /// Executor: its id in relay / pi-lead (what their commands take), not Cue's session id.
    #[serde(skip)]
    pub id: String,
    /// The machine it runs on (one of yours, by name); "" for this Mac.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub machine: String,
    /// Executor: its lead has seen this packet's report (relay's `report_seen_packet`): reviewed,
    /// by your Review or by relay waking the lead itself.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reviewed: bool,
    /// Sort key for duplicates (an executor id reused by a resume): the newest record wins.
    #[serde(skip)]
    updated: String,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Statuses that mean the executor is done with (closed, replaced, gone): left out of a lead's list.
const GONE: &[&str] = &["closed", "superseded", "dead", "retired", "done"];

static INDEX: Mutex<Option<HashMap<String, Member>>> = Mutex::new(None);
/// This Mac's part of INDEX, as last scanned.
static LOCAL: Mutex<Option<HashMap<String, Member>>> = Mutex::new(None);
/// Each machine's part: its relay command there, and its leads and executors.
static REMOTE: Mutex<Option<HashMap<String, (String, HashMap<String, Member>)>>> = Mutex::new(None);

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
    let mut parsed = PARSED.lock().unwrap();
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
    let mut cache = FIRST_LINES.lock().unwrap();
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
    for d in subdirs(root) {
        let Some(e) = read_json(&d.join("session.json")) else { continue };
        if !s(&e, "agent").starts_with("relay-executor") {
            continue;
        }
        let owner = s(&e, "owner_lead");
        let lead = leads.get(&owner);
        let packet = e.get("current_packet").and_then(Value::as_u64);
        // Goal and outcome only for executors still at work: closed ones pile up by the hundred.
        let (goal, outcome) = match packet.filter(|_| !GONE.contains(&s(&e, "status").as_str())) {
            Some(n) => (first_line(&d.join(format!("packets/{n:03}-packet.md"))), first_line(&d.join(format!("packets/{n:03}-report.md")))),
            None => (String::new(), String::new()),
        };
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
            reviewed: seen_report(&e),
            id: s(&e, "session_id"),
            updated: s(&e, "updated"),
            ..Default::default()
        };
        put(index, s(&e, "claude_session"), m);
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
    let mut cache = VERIFY.lock().unwrap();
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
            if let Some(c) = VERIFY.lock().unwrap().as_mut() {
                if c.get(&id).is_some_and(|(k, _)| *k == key) {
                    c.insert(id, (key, v));
                    VERIFIED.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });
    }
}

fn verdict_of(id: &str) -> Option<Verdict> {
    VERIFY.lock().unwrap().as_ref()?.get(id)?.1.clone()
}

fn scan(relay: &Path, pilead: &Path, claude_sessions: &Path) -> HashMap<String, Member> {
    let mut index = HashMap::new();
    scan_relay(relay, &live_claude_sessions(claude_sessions), &mut index);
    scan_pilead(pilead, &mut index);
    if let Some(parsed) = PARSED.lock().unwrap().as_mut() {
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
    *LOCAL.lock().unwrap() = Some(fresh);
    merge() || verified
}

/// INDEX again from this Mac's part and every machine's (this Mac's wins a clash). True when it changed.
fn merge() -> bool {
    let mut all = LOCAL.lock().unwrap().clone().unwrap_or_default();
    for (_, members) in REMOTE.lock().unwrap().as_ref().into_iter().flat_map(|r| r.values()) {
        for (sid, m) in members {
            all.entry(sid.clone()).or_insert_with(|| m.clone());
        }
    }
    let mut cur = INDEX.lock().unwrap();
    if cur.as_ref() == Some(&all) {
        return false;
    }
    *cur = Some(all);
    true
}

/// What runs on a machine to list its crews: relay's command there (from Claude Code's installed
/// plugins, else on PATH), its JSON listing, then the Claude sessions open in a live process there
/// (a lead counts as listening only while one is).
const REMOTE_LIST: &str = r#"c="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
r=$(sed -n 's/.*"installPath": *"\([^"]*\)".*/\1/p' "$c/plugins/installed_plugins.json" 2>/dev/null | grep -i relay | head -1)
if [ -n "$r" ] && [ -x "$r/bin/relay" ]; then r="$r/bin/relay"; else r=$(command -v relay 2>/dev/null || echo "$HOME/.local/bin/relay"); fi
if [ -x "$r" ]; then echo "BIN:$r"; "$r" list --all --json 2>/dev/null; else echo NORELAY; fi
echo "__CUE_LIVE__"
for f in "$c"/sessions/*.json; do
  # Claude Code writes these without a final newline, and sed then prints none: one id per line regardless.
  [ -f "$f" ] && kill -0 "$(basename "$f" .json)" 2>/dev/null && { sed -n 's/.*"sessionId": *"\([^"]*\)".*/\1/p' "$f"; echo; }
done
exit 0"#;

/// Re-read one machine's crews (blocking: an SSH round trip): whether INDEX changed, and the Claude
/// sessions running there (None: it couldn't be reached; it keeps what was last read from it).
pub fn refresh_remote(machine: &str, host: &str) -> (bool, Option<HashSet<String>>) {
    let Ok(out) = crate::machines::run(host, REMOTE_LIST) else { return (false, None) };
    let parsed = parse_remote(&out, machine);
    REMOTE.lock().unwrap().get_or_insert_with(HashMap::new).insert(machine.to_string(), parsed);
    (merge(), Some(live_of(&out)))
}

/// The Claude sessions REMOTE_LIST found running.
fn live_of(out: &str) -> HashSet<String> {
    out.split_once("__CUE_LIVE__").map(|(_, l)| l.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()).unwrap_or_default()
}

/// Forget a machine's crews (it has no sessions in Cue any more, or was removed).
pub fn forget_remote(machine: &str) -> bool {
    let gone = REMOTE.lock().unwrap().as_mut().and_then(|r| r.remove(machine)).is_some();
    gone && merge()
}

/// REMOTE_LIST's answer: relay's command there, and its leads and executors keyed by Cue session id.
fn parse_remote(out: &str, machine: &str) -> (String, HashMap<String, Member>) {
    let bin = out.lines().find_map(|l| l.strip_prefix("BIN:")).unwrap_or("").trim().to_string();
    let after_bin = out.split_once('\n').filter(|_| !bin.is_empty()).map(|(_, rest)| rest).unwrap_or("");
    let json = after_bin.split_once("__CUE_LIVE__").map_or(after_bin, |(j, _)| j);
    let live = live_of(out);
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
        let owner = s(e, "owner_lead");
        let lead = leads.get(&owner);
        let m = Member {
            role: "executor".into(),
            plugin: "relay".into(),
            name: Some(s(e, "topic")).filter(|t| !t.is_empty()).unwrap_or_else(|| s(e, "session_id")),
            color: lead.and_then(|l| l.1),
            model: s(e, "model"),
            status: s(e, "status"),
            packet: e.get("current_packet").and_then(Value::as_u64),
            lead_name: lead.map(|l| l.0.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| s(e, "owner_project")),
            lead_armed: Some(lead.is_some() && live.contains(&owner)),
            lead: owner,
            reviewed: seen_report(e),
            id: s(e, "session_id"),
            updated: s(e, "updated"),
            machine: machine.into(),
            ..Default::default()
        };
        put(&mut index, s(e, "claude_session"), m);
    }
    (bin, index)
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
        let line = std::iter::once(&self.bin).chain(&self.args).map(|a| crate::focus::shq(a)).collect::<Vec<_>>().join(" ");
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
    let bin = REMOTE.lock().unwrap().as_ref().and_then(|r| r.get(&m.machine)).map(|(b, _)| b.clone()).filter(|b| !b.is_empty());
    bin.map(|bin| Cmd { bin, args: vec![], machine: m.machine.clone() }).ok_or_else(|| format!("can't find relay's command on {}", m.machine))
}

/// Who handles this session's finished turn, given what its hook said (`hooked`). An executor
/// with an armed lead: "its relay lead (chart-lines)". An executor whose lead isn't armed: "",
/// because its report reaches nobody unless you see it. Anything else keeps the hook's answer.
/// (The Claude hook works out relay executors itself; Pi's extension doesn't.)
pub fn resolve_driver(session_id: &str, hooked: &str) -> String {
    let known = INDEX.lock().unwrap().as_ref().is_some_and(|i| i.get(session_id).is_some_and(|m| m.role == "executor"));
    if known { driven_by(session_id) } else { hooked.to_string() }
}

fn driven_by(session_id: &str) -> String {
    let cur = INDEX.lock().unwrap();
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
/// executors isn't closed from Cue: that's relay's handoff / close. None: not in a crew.
pub fn close_plan(session_id: &str) -> Option<Result<Cmd, String>> {
    let cur = INDEX.lock().unwrap();
    let index = cur.as_ref()?;
    let m = index.get(session_id)?;
    match (m.plugin.as_str(), m.role.as_str()) {
        ("relay", "executor") => Some(relay_for(m).map(|c| Cmd { args: vec!["close".into(), m.id.clone()], ..c })),
        ("pilead", "executor") => Some(pilead_bin(&m.id).map(|b| Cmd::here(b, vec!["close".into(), m.id.clone()])).ok_or_else(|| "can't find pilead's command".to_string())),
        (_, "lead") => {
            let n = index.values().filter(|e| e.role == "executor" && e.lead == session_id && !GONE.contains(&e.status.as_str())).count();
            (n > 0).then(|| Err(format!("it leads {n} executor{} still open: close them, or hand the lead off, first", if n == 1 { "" } else { "s" })))
        }
        _ => None,
    }
}

/// A relay executor's worktree (where its staged changes are), from relay's session file.
#[cfg_attr(not(feature = "ext"), allow(dead_code))]
pub fn worktree(session_id: &str) -> Option<String> {
    let id = INDEX.lock().unwrap().as_ref()?.get(session_id).filter(|m| m.role == "executor" && m.plugin == "relay")?.id.clone();
    let v: Value = serde_json::from_str(&std::fs::read_to_string(relay_dir().join(&id).join("session.json")).ok()?).ok()?;
    v.get("worktree")?.as_str().filter(|w| !w.is_empty()).map(String::from)
}

/// Whether an executor is done and waiting for its lead's review (relay: reported or idle).
pub fn is_done(session_id: &str) -> bool {
    INDEX.lock().unwrap().as_ref().and_then(|i| i.get(session_id)).is_some_and(|m| m.role == "executor" && matches!(m.status.as_str(), "reported" | "idle"))
}

/// Whether a done executor's report has been reviewed already (see Member::reviewed).
pub fn is_reviewed(session_id: &str) -> bool {
    INDEX.lock().unwrap().as_ref().and_then(|i| i.get(session_id)).is_some_and(|m| m.role == "executor" && m.reviewed)
}

/// Done executors whose report their lead has seen: Cue session id → its lead's session id.
pub fn reviewed() -> Vec<(String, String)> {
    let cur = INDEX.lock().unwrap();
    cur.as_ref().map(|i| i.iter().filter(|(_, m)| m.role == "executor" && m.reviewed && matches!(m.status.as_str(), "reported" | "idle")).map(|(sid, m)| (sid.clone(), m.lead.clone())).collect()).unwrap_or_default()
}

/// A lead's or executor's name (relay's topic), else its session id.
pub fn name_of(session_id: &str) -> String {
    INDEX.lock().unwrap().as_ref().and_then(|i| i.get(session_id)).map(|m| m.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| session_id.to_string())
}

/// What `action` ("review", "diff", "auto-on", "auto-off") means for this session: an executor's
/// review goes to its lead, its diff opens as a page; a relay lead's auto mode is switched by relay.
pub fn act(session_id: &str, action: &str) -> Result<Act, String> {
    let cur = INDEX.lock().unwrap();
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
        ("pilead", "executor", "diff") => pilead_diff_page(&pilead_dir(), &m.id).map(Act::Open).ok_or_else(|| "it hasn't written a diff page yet".into()),
        _ => Err(format!("no {action} for this session")),
    }
}

/// What the window needs for the sessions it knows: each one's role, and for a lead the
/// executors still working for it (whether or not Cue has seen them).
pub fn view(known: &HashSet<String>) -> Value {
    let cur = INDEX.lock().unwrap();
    let Some(index) = cur.as_ref() else { return Value::Object(Default::default()) };
    let mut out = serde_json::Map::new();
    for sid in known {
        let Some(m) = index.get(sid) else { continue };
        let mut v = serde_json::to_value(m).unwrap_or_default();
        if let Some(vf) = (m.role == "executor").then(|| verdict_of(&m.id)).flatten() {
            v["verify"] = serde_json::to_value(vf).unwrap_or_default();
        }
        if m.role == "lead" {
            let mut execs: Vec<(&String, &Member)> = index.iter().filter(|(_, e)| e.role == "executor" && e.lead == *sid && !GONE.contains(&e.status.as_str())).collect();
            execs.sort_by(|a, b| a.1.updated.cmp(&b.1.updated));
            v["executors"] = execs
                .into_iter()
                .map(|(esid, e)| serde_json::json!({ "session_id": esid, "name": e.name, "model": e.model, "status": e.status, "packet": e.packet, "goal": e.goal, "outcome": e.outcome, "verify": verdict_of(&e.id) }))
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
        let out = std::process::Command::new("/bin/sh").env("CLAUDE_CONFIG_DIR", &home).args(["-c", REMOTE_LIST]).output().unwrap();
        let out = String::from_utf8_lossy(&out.stdout).to_string();
        let (bin, index) = parse_remote(&out, "box");
        assert_eq!(bin, relay.display().to_string());
        assert_eq!(index["C1"].lead_armed, Some(true), "its lead is live there: {out:?}");
        let live: Vec<&str> = out.split("__CUE_LIVE__").nth(1).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        assert_eq!(live.len(), 2, "one id per line, the dead one left out: {live:?}");
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

    #[test]
    fn relay_and_pilead_leads_own_their_executors() {
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

        *INDEX.lock().unwrap() = Some(index);
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
        assert!(!PARSED.lock().unwrap().as_ref().unwrap().files.keys().any(|p| p.starts_with(relay.join("old-1"))));
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
}
