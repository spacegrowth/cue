//! Your machines' crews: the listing script run there over SSH, and what Cue reads from its answer
//! (relay's crews, the live sessions, the machine's zone, and panes on a screen before Claude Code starts).

use super::files::{color, predecessor, s};
use super::index::{carry_status_over, merge};
use super::pilead::parse_pilead;
use super::verify::verdict_from_json;
use super::wake::{epoch_secs, now_secs, ready_late, report_woken};
use super::{exec_key, put, safe_id, Member, RelayLeads, GONE};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Each machine's part: its relay command there, and its leads and executors.
pub(super) static REMOTE: Mutex<Option<HashMap<String, (String, HashMap<String, Member>)>>> = Mutex::new(None);
/// Each machine's pilead command ("" when it has none).
pub(super) static REMOTE_PILEAD: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
/// Per machine: its tmux panes on a screen before Claude Code starts (pane id -> which), as last listed.
static REMOTE_SCREENS: Mutex<Option<HashMap<String, HashMap<String, crate::terminal::dialog::Setup>>>> = Mutex::new(None);

/// What runs on a machine to list its crews: relay's command there (from Claude Code's installed
/// plugins, else on PATH), its JSON listing, pi-lead's (when `pilead` is there), then the Claude sessions open in a live process there
/// (a lead counts as listening only while one is), then its UTC offset (relay writes its timestamps
/// in the machine's local time).
pub(super) const REMOTE_LIST: &str = r#"c="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
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
# The bottom of every tmux pane: Cue reads which are on a screen before Claude Code starts (it runs no
# hook until it's past them, so nothing else would tell).
echo "__CUE_SCREENS__"
t=$(command -v tmux 2>/dev/null)
for x in /opt/homebrew/bin/tmux /usr/local/bin/tmux /usr/bin/tmux; do [ -n "$t" ] || { [ -x "$x" ] && t="$x"; }; done
[ -n "$t" ] && "$t" list-panes -a -F '#{pane_id} #{session_name}' 2>/dev/null | while read -r p n; do
  echo "@@PANE $p $n"; "$t" capture-pane -p -t "$p" 2>/dev/null | grep -v '^[[:space:]]*$' | tail -n 6
done
exit 0"#;

/// Re-read one machine's crews (blocking: an SSH round trip): whether INDEX changed, and the Claude
/// sessions running there (None: it couldn't be reached; it keeps what was last read from it).
pub fn refresh_remote(machine: &str, host: &str) -> (bool, Option<HashSet<String>>) {
    let Ok(out) = crate::machines::run(host, REMOTE_LIST) else { return (false, None) };
    let mut parsed = parse_remote(&out, machine);
    let screens = screens_of(&out);
    for m in parsed.1.values_mut().filter(|m| m.role == "executor" && !m.tmux_session.is_empty()) {
        m.setup = screens.values().find(|(n, _)| *n == m.tmux_session).map(|(_, s)| setup_word(*s).to_string()).unwrap_or_default();
    }
    crate::locking::lock(&REMOTE_SCREENS, "REMOTE_SCREENS").get_or_insert_with(HashMap::new).insert(machine.to_string(), screens.into_iter().map(|(p, (_, s))| (p, s)).collect());
    crate::locking::lock(&REMOTE_PILEAD, "REMOTE_PILEAD").get_or_insert_with(HashMap::new).insert(machine.to_string(), pilead_bin_of(&out));
    crate::locking::lock(&REMOTE, "REMOTE").get_or_insert_with(HashMap::new).insert(machine.to_string(), parsed);
    let changed = merge();
    carry_status_over();
    (changed, Some(live_of(&out)))
}

/// What a machine's tmux pane (`pane`, its id there) showed at the last listing, if it's a screen before
/// Claude Code starts. None as well when that machine hasn't been listed yet.
pub fn remote_setup(machine: &str, pane: &str) -> Option<crate::terminal::dialog::Setup> {
    crate::locking::lock(&REMOTE_SCREENS, "REMOTE_SCREENS").as_ref()?.get(machine)?.get(pane).copied()
}

/// Whether a machine's panes have been read at all (an older listing, or none yet, says nothing).
pub fn remote_screens_known(machine: &str) -> bool {
    crate::locking::lock(&REMOTE_SCREENS, "REMOTE_SCREENS").as_ref().is_some_and(|r| r.contains_key(machine))
}

fn setup_word(s: crate::terminal::dialog::Setup) -> &'static str {
    match s {
        crate::terminal::dialog::Setup::Trust => "trust",
        crate::terminal::dialog::Setup::Enter => "enter",
    }
}

/// REMOTE_LIST's panes on a screen before Claude Code starts: pane id -> (its tmux session, which screen).
pub(super) fn screens_of(out: &str) -> HashMap<String, (String, crate::terminal::dialog::Setup)> {
    let Some((_, part)) = out.split_once("__CUE_SCREENS__") else { return HashMap::new() };
    part.split("@@PANE ")
        .skip(1)
        .filter_map(|chunk| {
            let (head, screen) = chunk.split_once('\n').unwrap_or((chunk, ""));
            let (pane, name) = head.trim().split_once(' ')?;
            let setup = crate::terminal::dialog::setup_on(screen)?;
            Some((pane.to_string(), (tmux_name(name.trim()), setup)))
        })
        .collect()
}

/// The Claude sessions REMOTE_LIST found running.
pub(super) fn live_of(out: &str) -> HashSet<String> {
    let out = out.split_once("__CUE_SCREENS__").map_or(out, |(o, _)| o);
    out.split_once("__CUE_LIVE__").map(|(_, l)| l.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(TZ_MARK)).map(String::from).collect()).unwrap_or_default()
}

pub(super) const TZ_MARK: &str = "__CUE_TZ__:";

/// The machine's UTC offset in seconds, from REMOTE_LIST's `date +%z` ("+0530", "-0800"); None when it
/// didn't say (an older answer, or a garbled one): its timestamps are then read in this Mac's zone.
pub(super) fn tz_of(out: &str) -> Option<i64> {
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

/// Whether this machine's crews have been read since Cue started (or since it was last removed).
pub fn remote_known(machine: &str) -> bool {
    crate::locking::lock(&REMOTE, "REMOTE").as_ref().is_some_and(|r| r.contains_key(machine))
}

/// REMOTE_LIST's answer: relay's command there, and its leads and executors keyed by Cue session id.
pub(super) fn parse_remote(out: &str, machine: &str) -> (String, HashMap<String, Member>) {
    let bin = out.lines().find_map(|l| l.strip_prefix("BIN:")).unwrap_or("").trim().to_string();
    let (relay_part, pilead) = remote_sections(out);
    let json = relay_part.split_once('\n').filter(|_| !bin.is_empty()).map(|(_, rest)| rest).unwrap_or("");
    let live = live_of(out);
    let zone = tz_of(out);
    let v: Value = serde_json::from_str(json.trim()).unwrap_or(Value::Null);
    let mut index = HashMap::new();
    let leads = listed_leads(&v, machine, zone, &mut index);
    for e in v.get("executors").and_then(Value::as_array).into_iter().flatten() {
        if !s(e, "agent").starts_with("relay-executor") {
            continue;
        }
        if !safe_id(&s(e, "session_id")) {
            eprintln!("cue: relay on {machine} lists a session whose id isn't a folder name ({:?}): skipped", s(e, "session_id"));
            continue;
        }
        let m = listed_executor(e, &leads, &live, zone, machine);
        put(&mut index, exec_key(e), m);
    }
    parse_pilead(pilead, machine, &mut index);
    (bin, index)
}

/// The leads in relay's listing on a machine (`v`) into the index; what their executors show of each, by its
/// session id. `zone`: the machine's UTC offset.
fn listed_leads(v: &Value, machine: &str, zone: Option<i64>, index: &mut HashMap<String, Member>) -> RelayLeads {
    let mut leads: RelayLeads = HashMap::new();
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
            predecessor: predecessor(l),
            started_ms: epoch_secs(&s(l, "started"), zone).map_or(0, |t| t * 1000),
            updated: s(l, "last_active"),
            machine: machine.into(),
            ..Default::default()
        };
        put(index, sid, lead);
    }
    leads
}

/// An executor in relay's listing on a machine (`e`), under its lead as `leads` has it.
fn listed_executor(e: &Value, leads: &RelayLeads, live: &HashSet<String>, zone: Option<i64>, machine: &str) -> Member {
    let owner = s(e, "owner_lead");
    let lead = leads.get(&owner);
    // goal / outcome / verify are newer than the rest of relay's listing: a machine on an older
    // relay doesn't send them, and that's no error, just nothing to show.
    let open = !GONE.contains(&s(e, "status").as_str());
    Member {
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
        woken: report_woken(e),
        ready_late: ready_late(e, now_secs(), zone),
        id: s(e, "session_id"),
        updated: s(e, "updated"),
        machine: machine.into(),
        tmux_session: tmux_name(&s(e, "tmux_session")),
        ..Default::default()
    }
}

/// REMOTE_LIST's answer up to the live-sessions marker, split into relay's part (its `BIN:` line and
/// JSON) and pilead's (what follows `__CUE_PILEAD__`; "" when the machine's answer has none, an older one).
fn remote_sections(out: &str) -> (&str, &str) {
    let head = out.split_once("__CUE_LIVE__").map_or(out, |(j, _)| j);
    head.split_once(PILEAD_MARK).unwrap_or((head, ""))
}

const PILEAD_MARK: &str = "__CUE_PILEAD__";

/// A machine's pilead command, from its section's `PBIN:` line ("" when it has none).
pub(super) fn pilead_bin_of(out: &str) -> String {
    out.split_once(PILEAD_MARK).and_then(|(_, p)| p.lines().find_map(|l| l.strip_prefix("PBIN:"))).unwrap_or("").trim().to_string()
}

/// A tmux session name from a machine's listing, as Cue will pass it to tmux there: only letters, digits,
/// `_` and `-` (nothing the remote shell or tmux's own target syntax could read as more than a name); ""
/// for anything else, which leaves that executor's terminal as it was before the listing named one.
pub(super) fn tmux_name(t: &str) -> String {
    let t = t.trim();
    if !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') { t.to_string() } else { String::new() }
}

/// At most 300 characters, as relay clips the lines it lists.
pub(super) fn clip(t: &str) -> String {
    let t = t.trim();
    if t.chars().count() > 300 { format!("{}…", t.chars().take(299).collect::<String>()) } else { t.to_string() }
}

/// Each machine's leads and executors added to `all`, under the session ids it doesn't have yet (this Mac's
/// part wins a clash).
pub(super) fn add_machines(all: &mut HashMap<String, Member>) {
    for (_, members) in crate::locking::lock(&REMOTE, "REMOTE").as_ref().into_iter().flat_map(|r| r.values()) {
        for (sid, m) in members {
            all.entry(sid.clone()).or_insert_with(|| m.clone());
        }
    }
}

/// relay's command on a machine, as last listed there; None when it has none.
pub(super) fn relay_bin_on(machine: &str) -> Option<String> {
    crate::locking::lock(&REMOTE, "REMOTE").as_ref().and_then(|r| r.get(machine)).map(|(b, _)| b.clone()).filter(|b| !b.is_empty())
}

/// pilead's command on a machine, as last listed there; None when it has none.
pub(super) fn pilead_bin_on(machine: &str) -> Option<String> {
    crate::locking::lock(&REMOTE_PILEAD, "REMOTE_PILEAD").as_ref().and_then(|r| r.get(machine)).cloned().filter(|b| !b.is_empty())
}
