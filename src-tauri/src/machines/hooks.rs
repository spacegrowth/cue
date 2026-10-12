//! Cue's hooks on a machine (+ New → Machine), for the agents installed there. Cue's own hook is a
//! Mac program, so a machine gets a small POSIX script instead, ~/.cue/cue-hook: it drops each hook
//! event into ~/.cue/in/ and, for a permission question, waits for Cue's answer in ~/.cue/out/ while
//! Cue is watching. On the Mac, one SSH stream per machine picks the events up and hands each to Cue
//! as a local hook would, so everything after that is the same as on this Mac. Nothing listens on the
//! network. When Cue isn't watching, the script leaves at once and the agent asks in its terminal.
use crate::machines::{self, Machine};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The hook on the machine. Its name has "cue-hook" in it, so Connect's merge replaces older entries.
pub const SCRIPT: &str = r#"#!/bin/sh
# Cue's hook (installed by Cue on your Mac when you added this machine): hands the agent's hook event to
# Cue over your SSH connection. Never in the agent's way: without Cue watching, it leaves at once.
d="$HOME/.cue"
mkdir -p "$d/in" "$d/out" 2>/dev/null || exit 0
id="$(date +%s)-$$"
# A line per run (kept short), for when an event seems to go missing.
[ "$(wc -c 2>/dev/null <"$d/hook.log" || echo 0)" -lt 65536 ] || mv -f "$d/hook.log" "$d/hook.log.old" 2>/dev/null
echo "$id $1 ${TMUX_PANE:-}" >>"$d/hook.log" 2>/dev/null
# The event (a prompt, a command about to run) is yours alone: the file is readable by you only.
( umask 077; {
  printf '{"event":"%s","harness":"%s","pane":"%s","tmux":"%s","input":' "$1" "$2" "${TMUX_PANE:-}" "${TMUX:+1}"
  cat
  printf '}\n'
} >"$d/in/$id.tmp" ) 2>/dev/null && mv "$d/in/$id.tmp" "$d/in/$id.json"
[ "$1" = permission ] || exit 0
# A permission question: wait for the answer while Cue is watching (it stamps the time every second).
while :; do
  if [ -e "$d/out/$id.json" ]; then cat "$d/out/$id.json"; rm -f "$d/out/$id.json"; exit 0; fi
  seen=$(cat "$d/watching" 2>/dev/null || echo 0)
  [ $(( $(date +%s) - ${seen:-0} )) -lt 20 ] || { rm -f "$d/in/$id.json"; exit 0; }
  sleep 0.2 2>/dev/null || sleep 1
done
"#;

/// The status line on the machine, as on this Mac (install.sh's cue-statusline): Claude Code's transcript
/// doesn't say how big a session's context window is, its status line does. This passes the size on to
/// Cue through the hook (as a "usage" event, only when it changes, not on every refresh), then runs your
/// own status line command unchanged (saved in ~/.cue/statusline.orig; none there: it shows nothing).
pub const STATUS: &str = r#"#!/bin/sh
# Cue's pass-through status line (installed by Cue on your Mac when you added this machine): tells Cue
# the session's context window when it changes, then runs your own status line command unchanged.
input=$(cat)
d="$HOME/.cue"
one=$(printf '%s' "$input" | tr -d '\n')
sid=$(printf '%s' "$one" | sed -n 's/.*"session_id" *: *"\([A-Za-z0-9-]*\)".*/\1/p')
win=$(printf '%s' "$one" | sed -n 's/.*"context_window_size" *: *\([0-9][0-9]*\).*/\1/p')
if [ -n "$sid" ] && [ -n "$win" ] && [ "$(cat "$d/win/$sid" 2>/dev/null)" != "$win" ]; then
  mkdir -p "$d/win" 2>/dev/null && printf '%s' "$win" >"$d/win/$sid" 2>/dev/null
  find "$d/win" -type f -mtime +7 -delete 2>/dev/null
  [ -x "$d/cue-hook" ] && printf '%s' "$input" | "$d/cue-hook" usage claude >/dev/null 2>&1
fi
cmd=$(cat "$d/statusline.orig" 2>/dev/null)
[ -n "$cmd" ] && printf '%s' "$input" | sh -c "$cmd"
exit 0
"#;

/// The status line command Cue puts in the machine's Claude Code settings.
const STATUS_CMD: &str = "\"$HOME/.cue/cue-statusline\"";

/// Claude Code's settings with Cue's status line in, and the command it replaces (to run it from there):
/// None when Cue's is in already (the one saved then is kept).
fn wrap_status_line(cfg: &Value) -> Option<(Value, String)> {
    let line = cfg.get("statusLine").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let was = line.get("command").and_then(Value::as_str).unwrap_or("").to_string();
    if was.contains("cue-statusline") {
        return None;
    }
    let mut line = line;
    line["type"] = json!("command");
    line["command"] = json!(STATUS_CMD);
    let mut cfg = cfg.clone();
    cfg["statusLine"] = line;
    Some((cfg, was))
}

/// What the watcher runs on the machine: print each waiting event as one line, looking ten times a second
/// (an event reaches Cue in about a tenth of a second, as a hook on this Mac does), and once a second stamp
/// the time and say it's alive. The heartbeat matters: if the Cue that started this quits, the next one
/// fails to reach it, so ssh ends instead of living on and taking events nobody reads. A `sleep` that
/// can't do fractions (not GNU's or BusyBox's) falls back to a second. ("cue-watch" marks these processes.)
/// Once a minute it also drops answers in out/ a day old or more: one the hook never collected (its agent
/// was gone by the time you answered) would otherwise stay there for good.
///
/// One watcher per Mac: some SSH servers never end a command whose connection is gone, so an
/// earlier connection's watcher can live on, its output going nowhere, and take events the current one
/// never sees (prompts and Stops lost, some read by both as an empty line). A watcher records itself in
/// ~/.cue/watcher-<this Mac>, and an older one of the same Mac leaves as soon as that names another
/// (another Mac's watcher is left alone). Once a second it ends watchers of the script before this one,
/// which don't know that file: shells only, known by their script read from /proc, never one carrying
/// this script's marker (itself, and whatever started it with this text in its command line). Each
/// event is claimed (renamed) before it's read, so two never take one, and at start a claimed event
/// whose watcher died is put back. On a machine without /proc or pgrep the sweep does nothing.
const WATCH: &str = r#"# cue-watch
d="$HOME/.cue"; mkdir -p "$d/in" "$d/out"
v2=cue-watch-v2; w="$d/watcher-@MAC@"
echo $$ >"$w"
for g in "$d"/in/*.json.*; do
  [ -e "$g" ] || continue
  kill -0 "${g##*.}" 2>/dev/null || mv "$g" "${g%.*}" 2>/dev/null
done
n=0; p=0
while :; do
  [ "$(cat "$w" 2>/dev/null)" = $$ ] || exit 0
  if [ "$n" -eq 0 ]; then
    date +%s >"$d/watching"
    for x in $(pgrep -f '# cue-watch' 2>/dev/null); do
      case "$(cat "/proc/$x/comm" 2>/dev/null)" in sh|bash|dash|ash|zsh|ksh|busybox) ;; *) continue ;; esac
      grep -q "$v2" "/proc/$x/cmdline" 2>/dev/null && continue
      grep -q 'n=0; p=0' "/proc/$x/cmdline" 2>/dev/null && kill "$x" 2>/dev/null
    done
  fi
  for f in "$d"/in/*.json; do
    [ -e "$f" ] || continue
    g="$f.$$"; mv "$f" "$g" 2>/dev/null || continue
    printf '@@%s ' "$(basename "$f" .json)"; tr -d '\n' <"$g"; echo; rm -f "$g"
  done
  # The beat comes after the events waiting there: the first one means all of them were handed over.
  if [ "$n" -eq 0 ]; then
    [ "$p" -eq 0 ] && find "$d/out" "$d/in" -name '*.json*' -mmin +1440 -delete 2>/dev/null
    p=$(( (p + 1) % 60 ))
    echo "@@hb" || exit 0
  fi
  n=$(( (n + 1) % 10 ))
  sleep 0.1 2>/dev/null || { sleep 1; n=0; }
done"#;

/// The watcher for this Mac: WATCH with this Mac's name (letters, digits and dashes) in its file's name.
fn watch_script() -> String {
    WATCH.replace("@MAC@", &mac_key())
}

fn mac_key() -> String {
    let key: String = hostname().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).take(64).collect();
    if key.is_empty() { "mac".into() } else { key }
}

/// This Mac's host name, as the system has it ("" if it can't say).
pub(crate) fn hostname() -> String {
    let mut buf = [0u8; 256];
    let n = if unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } == 0 { buf.iter().position(|b| *b == 0).unwrap_or(0) } else { 0 };
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

/// Set Cue's hooks up on the machine for the agents found there (`tools`, from its check): the script,
/// and Cue's entries in each agent's settings (backed up first, everything else kept). What it did.
pub fn setup(m: &Machine, tools: &Value) -> Result<Vec<String>, String> {
    let has = |t: &str| tools.get(t).and_then(Value::as_bool).unwrap_or(false);
    let agents: Vec<(&str, &str, &str)> = [("claude", "Claude Code", ".claude/settings.json"), ("codex", "Codex", ".codex/hooks.json")].into_iter().filter(|(t, _, _)| has(t)).collect();
    if agents.is_empty() {
        return Ok(vec![]);
    }
    let put = |name: &str, body: &str| machines::run_with_input(&m.host, &format!(r#"mkdir -p "$HOME/.cue" && cat >"$HOME/.cue/{name}.tmp" && chmod 755 "$HOME/.cue/{name}.tmp" && mv "$HOME/.cue/{name}.tmp" "$HOME/.cue/{name}""#), body);
    put("cue-hook", SCRIPT)?;
    if has("claude") {
        put("cue-statusline", STATUS)?;
    }
    let mut did = vec![];
    for (harness, label, file) in agents {
        let path = format!("$HOME/{file}");
        let current = machines::run(&m.host, &format!(r#"cat "{path}" 2>/dev/null; true"#))?;
        let cfg: Value = if current.trim().is_empty() { json!({}) } else { serde_json::from_str(&current).map_err(|e| format!("~/{file} on {} isn't valid JSON ({e}): fix it there, then Check again", m.name))? };
        let mut merged = crate::config::merge_hooks(cfg.clone(), "$HOME/.cue/cue-hook", harness)?;
        // Claude Code's status line too: your command is saved first, so it keeps running from Cue's.
        let mut what = "hooks";
        if harness == "claude" {
            if let Some((wrapped, was)) = wrap_status_line(&merged) {
                machines::run_with_input(&m.host, r#"umask 077; cat >"$HOME/.cue/statusline.orig.tmp" && mv "$HOME/.cue/statusline.orig.tmp" "$HOME/.cue/statusline.orig""#, &was)?;
                merged = wrapped;
                what = "hooks and status line";
            }
        }
        if merged == cfg {
            continue; // set up already
        }
        let write = format!(
            r#"f="{path}"; mkdir -p "$(dirname "$f")"; [ -e "$f" ] && cp "$f" "$f.cue-backup-$(date +%s)"; cat >"$f.cue-tmp" && mv "$f.cue-tmp" "$f""#
        );
        machines::run_with_input(&m.host, &write, &(serde_json::to_string_pretty(&merged).unwrap_or_default() + "\n"))?;
        did.push(format!("Set up Cue's {what} for {label} (~/{file}, backed up first)"));
    }
    Ok(did)
}

// ---------- watching: one SSH stream per machine ----------

fn watchers() -> &'static Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>> {
    static W: OnceLock<Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Watch every machine (at launch, and when one's added). One already watched is left alone.
pub fn watch_all() {
    // A watcher an earlier Cue left behind would take events away from this one: end them first.
    // Only ssh processes whose command carries the watcher's mark, nothing else that mentions it.
    machines::end_streams("# cue-watch");
    crate::machines::logs::stop_strays();
    for m in machines::list() {
        watch(&m);
    }
}

pub fn watch(m: &Machine) {
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut w = crate::locking::lock(watchers(), "watchers");
        if w.contains_key(&m.name) {
            return;
        }
        w.insert(m.name.clone(), stop.clone());
    }
    let m = m.clone();
    std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = stream(&m, &stop);
            // Down (asleep, offline, needs a login): try again in a while, quietly.
            for _ in 0..15 {
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

/// Stop watching a machine (it was removed).
pub fn unwatch(name: &str) {
    if let Some(stop) = crate::locking::lock(watchers(), "watchers").remove(name) {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// What runs each time a machine is reached again (Cue started, or it came back from a reboot or a
/// dropped line): given the machine's name, on the watcher's own thread.
static ON_REACHED: OnceLock<Box<dyn Fn(&str) + Send + Sync>> = OnceLock::new();

pub fn on_reached(f: impl Fn(&str) + Send + Sync + 'static) {
    let _ = ON_REACHED.set(Box::new(f));
}

/// What runs when a session Cue hasn't heard from in this run sends its first event from a machine (a
/// relay executor just started, say): given the machine's name, on a thread of its own, now and once
/// more a few seconds later (its lead may not have written it down yet). At most once per machine every
/// few seconds, so a machine reached with many sessions asks once.
static ON_NEW_SESSION: OnceLock<Box<dyn Fn(&str) + Send + Sync>> = OnceLock::new();

pub fn on_new_session(f: impl Fn(&str) + Send + Sync + 'static) {
    let _ = ON_NEW_SESSION.set(Box::new(f));
}

fn new_session_seen(machine: &str, sid: &str) {
    static SEEN: Mutex<Option<(HashSet<String>, HashMap<String, Instant>)>> = Mutex::new(None);
    let due = {
        let mut g = crate::locking::lock(&SEEN, "seen sessions");
        let (seen, last) = g.get_or_insert_with(Default::default);
        if !seen.insert(format!("{machine}|{sid}")) || last.get(machine).is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
            false
        } else {
            last.insert(machine.to_string(), Instant::now());
            true
        }
    };
    if due && ON_NEW_SESSION.get().is_some() {
        let name = machine.to_string();
        std::thread::spawn(move || {
            for wait in [0, 4] {
                std::thread::sleep(Duration::from_secs(wait));
                if let Some(f) = ON_NEW_SESSION.get() {
                    f(&name);
                }
            }
        });
    }
}

/// One SSH stream: runs until the connection drops or the machine is removed.
fn stream(m: &Machine, stop: &std::sync::atomic::AtomicBool) -> Result<(), String> {
    let mut child = machines::stream_command(&m.host, &watch_script()).stdout(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let out = child.stdout.take().ok_or("no output")?;
    let mut reached = false;
    let mut reader = BufReader::new(out);
    loop {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let line = match capped_line(&mut reader, MAX_LINE) {
            Ok(Some(Some(l))) => l,
            // Too long to be an event: dropped, and said so in the log.
            Ok(Some(None)) => {
                log_raw(&m.name, "(a line over the size limit)");
                continue;
            }
            // The stream ended, or the line dropped.
            Ok(None) | Err(_) => break,
        };
        if line == "@@hb" {
            // It's alive. The first beat of a stream: the machine is reached (again), and the events
            // queued there while Cue wasn't watching (an agent that ended meanwhile) are handled already:
            // what's rebuilt on reaching it goes by what they left.
            if !reached {
                reached = true;
                machines::forget(&m.host);
                if let Some(f) = ON_REACHED.get() {
                    let name = m.name.clone();
                    std::thread::spawn(move || f(&name));
                }
            }
            continue;
        }
        match parse(&line) {
            Some((id, ev)) => handle(m, &id, ev),
            // An event Cue couldn't read: in the log, so it isn't lost without a trace.
            None if line.starts_with("@@") => log_raw(&m.name, &line),
            None => {}
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

/// The most a line from a machine may hold (an event is a few KB; a transcript's largest tool result is
/// well under this). Past it, the rest of the line is read and thrown away as it comes, so a machine
/// that never sends a newline can't make Cue keep growing.
const MAX_LINE: usize = 1024 * 1024;

/// The next line from `r`, without its newline: Ok(Some(Some(line))), or Ok(Some(None)) for a line over
/// `max` bytes (skipped whole), or Ok(None) at the end of the stream. Memory never goes past `max`.
pub(crate) fn capped_line(r: &mut impl BufRead, max: usize) -> std::io::Result<Option<Option<String>>> {
    let mut buf = Vec::new();
    let mut over = false;
    loop {
        let chunk = r.fill_buf()?;
        if chunk.is_empty() {
            // The end: a last line without its newline still counts, nothing at all doesn't.
            return Ok(if buf.is_empty() && !over { None } else if over { Some(None) } else { Some(Some(String::from_utf8_lossy(&buf).into_owned())) });
        }
        let (take, done) = match chunk.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (chunk.len(), false),
        };
        if !over {
            let keep = take - usize::from(done);
            if buf.len() + keep > max {
                over = true;
                buf = Vec::new();
            } else {
                buf.extend_from_slice(&chunk[..keep]);
            }
        }
        r.consume(take);
        if done {
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            return Ok(Some(if over { None } else { Some(String::from_utf8_lossy(&buf).into_owned()) }));
        }
    }
}

/// "@@<id> <json>" -> (id, event).
fn parse(line: &str) -> Option<(String, Value)> {
    let (id, json) = line.strip_prefix("@@")?.split_once(' ')?;
    if !id.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return None;
    }
    Some((id.to_string(), serde_json::from_str(json).ok()?))
}

/// What Cue's socket gets for an event from a machine: the same message the Mac's hook sends, with the
/// session placed on that machine. None for events Cue doesn't take.
pub fn to_cue(machine: &str, ev: &Value) -> Option<Value> {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let p = ev.get("input")?;
    let event = s(ev, "event");
    // From its status line: only the session's context window (the plan's limits and cost are this
    // Mac's to show, and a machine's window is its own, not its model's everywhere).
    if event == "usage" {
        let window = p.pointer("/context_window/context_window_size").and_then(Value::as_u64)?;
        return Some(json!({ "type": "usage", "session_id": s(p, "session_id"), "window": window, "machine": machine }));
    }
    let harness = Some(s(ev, "harness")).filter(|h| !h.is_empty()).unwrap_or_else(|| "claude".into());
    let mut msg = Map::new();
    msg.insert("harness".into(), json!(harness));
    msg.insert("session_id".into(), json!(s(p, "session_id")));
    msg.insert("cwd".into(), json!(s(p, "cwd")));
    msg.insert("transcript_path".into(), json!(s(p, "transcript_path")));
    let in_tmux = !s(ev, "tmux").is_empty();
    msg.insert("tmux_pane".into(), json!(if in_tmux { s(ev, "pane") } else { String::new() }));
    msg.insert("term_program".into(), json!(if in_tmux { "tmux" } else { "" }));
    msg.insert("machine".into(), json!(machine));
    let mut event_msg = |name: &str, message: String| {
        msg.insert("type".into(), json!("event"));
        msg.insert("event".into(), json!(name));
        msg.insert("message".into(), json!(message));
        if let Some(id) = p.get("prompt_id").filter(|v| !v.is_null()) {
            msg.insert("prompt_id".into(), id.clone());
        }
    };
    if let Some((name, message)) = crate::hook::event_for(&event, p) {
        event_msg(name, message);
        return Some(Value::Object(msg));
    }
    match event.as_str() {
        "failure" => {
            event_msg("failed", Some(s(p, "error_details")).filter(|d| !d.is_empty()).unwrap_or_else(|| s(p, "last_assistant_message")));
            msg.insert("error_type".into(), json!(Some(s(p, "error_type")).filter(|d| !d.is_empty()).unwrap_or_else(|| "error".into())));
        }
        "permission" => {
            let tool = s(p, "tool_name");
            msg.insert("type".into(), json!("ask"));
            msg.insert("kind".into(), json!(if tool == "AskUserQuestion" { "question" } else { "permission" }));
            msg.insert("tool_name".into(), json!(tool));
            msg.insert("tool_input".into(), p.get("tool_input").cloned().unwrap_or(json!({})));
            msg.insert("suggestions".into(), p.get("permission_suggestions").cloned().unwrap_or(json!([])));
        }
        _ => return None,
    }
    Some(Value::Object(msg))
}

/// One line per event from a machine, in machines.log in Cue's data folder (cut back past 256 KB): when
/// the machine wrote it (from its id), when Cue got it, and for which session. What to look at when a
/// machine's session seems slow to report.
fn log(machine: &str, id: &str, ev: &Value) {
    let path = crate::server::cue_dir().join("machines.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    let written: u64 = id.split('-').next().and_then(|t| t.parse().ok()).unwrap_or(0);
    let now = crate::model::now_ms();
    let line = format!(
        "{now} {machine} {} {} written {written}s, {}s ago\n",
        ev["event"].as_str().unwrap_or("?"),
        ev["input"]["session_id"].as_str().unwrap_or("?"),
        (now / 1000).saturating_sub(written)
    );
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

pub(crate) fn log_raw(machine: &str, line: &str) {
    let path = crate::server::cue_dir().join("machines.log");
    let cut: String = line.chars().take(600).collect();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(format!("{} {machine} UNREADABLE {cut}\n", crate::model::now_ms()).as_bytes());
    }
}

/// Hand an event to Cue. A permission question waits for your answer (on its own thread) and sends it
/// back to the machine; answered elsewhere, an empty answer lets the agent's own prompt stand.
fn handle(m: &Machine, id: &str, ev: Value) {
    log(&m.name, id, &ev);
    let Some(mut msg) = to_cue(&m.name, &ev) else { return };
    if let Some(sid) = msg["session_id"].as_str().filter(|s| !s.is_empty()) {
        new_session_seen(&m.name, sid);
    }
    // The session reads its log from a live copy on this Mac (machine_logs); the hooks carry on as well.
    if let Some(remote) = msg["transcript_path"].as_str().filter(|p| !p.is_empty()).map(str::to_string) {
        if ev["event"] == "end" {
            crate::machines::logs::stop(m, &remote);
        } else {
            let local = crate::machines::logs::local_copy(m, &remote);
            if !local.is_empty() {
                msg["transcript_path"] = json!(local);
            }
        }
    }
    if msg["type"] != "ask" {
        if let Some(mut s) = crate::hook::connect(Duration::from_millis(800)) {
            if s.write_all(format!("{msg}\n").as_bytes()).is_ok() {
                // Cue hangs up once it has taken the event in: wait for that (up to the timeout), so
                // the next line, and the machine counting as reached, come after it.
                let _ = s.shutdown(std::net::Shutdown::Write);
                let _ = std::io::Read::read(&mut s, &mut [0u8; 64]);
            }
        }
        return;
    }
    let (m, id, p) = (m.clone(), id.to_string(), ev["input"].clone());
    std::thread::spawn(move || {
        let answer = ask(&msg).map(|d| crate::hook::to_claude(&d, &p).to_string()).unwrap_or_default();
        let _ = machines::run_with_input(&m.host, &format!(r#"umask 077; cat >"$HOME/.cue/out/{id}.tmp" && mv "$HOME/.cue/out/{id}.tmp" "$HOME/.cue/out/{id}.json""#), &answer);
    });
}

/// Ask Cue's socket and wait for the decision (None: answered elsewhere, or Cue went away).
fn ask(msg: &Value) -> Option<Value> {
    let mut s = crate::hook::connect(Duration::from_secs(1))?;
    s.write_all(format!("{msg}\n").as_bytes()).ok()?;
    let _ = s.set_read_timeout(None);
    for l in BufReader::new(s).lines() {
        let m: Value = serde_json::from_str(&l.ok()?).ok()?;
        match m.get("type").and_then(Value::as_str) {
            Some("decision") => return Some(m),
            Some("cancel") | Some("error") => return None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// A watcher ends every other watcher of this user (as on a machine, where there is one ~/.cue):
    /// the tests that run one take turns.
    static WATCHERS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn run_sh(script: &str, home: &std::path::Path, args: &[&str], input: &str) -> String {
        let mut c = Command::new("/bin/sh").arg("-c").arg(script).arg("cue-hook").args(args).env("HOME", home).env("TMUX", "/tmp/x,1,0").env("TMUX_PANE", "%7").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        String::from_utf8_lossy(&c.wait_with_output().unwrap().stdout).to_string()
    }

    #[test]
    fn the_status_line_tells_cue_the_window_when_it_changes_and_shows_yours_unchanged() {
        let home = std::env::temp_dir().join(format!("cue-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue/in")).unwrap();
        std::fs::write(home.join(".cue/cue-hook"), SCRIPT).unwrap();
        std::fs::set_permissions(home.join(".cue/cue-hook"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        // Yours: prints the model it was given (so it got the same input), with a quote in it.
        std::fs::write(home.join(".cue/statusline.orig"), r#"sed -n 's/.*"display_name":"\([^"]*\)".*/[\1] it'"'"'s mine/p'"#).unwrap();
        let input = |win: u64| format!(r#"{{"session_id":"abc-123","model":{{"display_name":"Opus"}},"context_window":{{"context_window_size":{win},"used_percentage":12}},"cost":{{"total_cost_usd":1.5}}}}"#);
        let events = || std::fs::read_dir(home.join(".cue/in")).unwrap().count();
        assert_eq!(run_sh(STATUS, &home, &[], &input(200_000)).trim(), "[Opus] it's mine", "your status line, as it was");
        assert_eq!(events(), 1, "the window, told once");
        let ev: Value = serde_json::from_str(&std::fs::read_to_string(std::fs::read_dir(home.join(".cue/in")).unwrap().next().unwrap().unwrap().path()).unwrap()).unwrap();
        assert_eq!(to_cue("box", &ev), Some(json!({ "type": "usage", "session_id": "abc-123", "window": 200_000, "machine": "box" })), "only the window, from that machine");
        run_sh(STATUS, &home, &[], &input(200_000));
        assert_eq!(events(), 1, "the same again: nothing new (it refreshes often)");
        run_sh(STATUS, &home, &[], &input(1_000_000));
        assert_eq!(events(), 2, "a new window: told");
        // No status line of yours: it shows nothing, and still tells Cue.
        std::fs::remove_file(home.join(".cue/statusline.orig")).unwrap();
        assert_eq!(run_sh(STATUS, &home, &[], &input(200_000)), "");
        assert_eq!(events(), 3);
        std::fs::remove_dir_all(&home).ok();
    }

    /// On a real machine, by hand: CUE_HOME=<a Cue folder listing it> CUE_MACHINE=name cargo test -q
    /// setup_on_a_machine -- --ignored --nocapture. Sets Cue up there for real (backing up its settings).
    #[test]
    #[ignore]
    fn setup_on_a_machine() {
        let Ok(name) = std::env::var("CUE_MACHINE") else { return };
        let m = machines::get(&name).unwrap();
        let first = setup(&m, &json!({ "claude": true })).unwrap();
        println!("first: {first:?}");
        let again = setup(&m, &json!({ "claude": true })).unwrap();
        assert!(again.is_empty(), "set up already: nothing done the second time ({again:?})");
        let cfg: Value = serde_json::from_str(&machines::run(&m.host, r#"cat "$HOME/.claude/settings.json""#).unwrap()).unwrap();
        assert_eq!(cfg["statusLine"]["command"], STATUS_CMD);
        assert!(machines::run(&m.host, r#"test -x "$HOME/.cue/cue-statusline" && echo ok"#).unwrap().contains("ok"));
    }

    #[test]
    fn cues_status_line_goes_in_once_keeping_yours_to_run() {
        let cfg = json!({ "statusLine": { "type": "command", "command": "bash ~/.claude/line.sh", "padding": 0 }, "model": "opus" });
        let (wrapped, was) = wrap_status_line(&cfg).unwrap();
        assert_eq!(was, "bash ~/.claude/line.sh");
        assert_eq!(wrapped["statusLine"], json!({ "type": "command", "command": "\"$HOME/.cue/cue-statusline\"", "padding": 0 }), "yours replaced, its other settings kept");
        assert_eq!(wrapped["model"], "opus", "everything else as it was");
        assert!(wrap_status_line(&wrapped).is_none(), "in already: nothing to do, and the saved one stays");
        let (none, was) = wrap_status_line(&json!({})).unwrap();
        assert_eq!((none["statusLine"]["command"].as_str(), was.as_str()), (Some("\"$HOME/.cue/cue-statusline\""), ""), "none of yours: Cue's, showing nothing");
    }

    #[test]
    fn the_watcher_hands_an_event_over_within_a_fraction_of_a_second() {
        let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join(format!("cue-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue/in")).unwrap();
        let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let out = c.stdout.take().unwrap();
        std::thread::spawn(move || {
            for l in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send((l, std::time::Instant::now()));
            }
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap().0, "@@hb", "it says it's alive first");
        std::thread::sleep(Duration::from_millis(250));
        std::fs::write(home.join(".cue/in/123-4.json"), "{\"event\":\"stop\"}\n").unwrap();
        let dropped = std::time::Instant::now();
        let (line, at) = loop {
            let (l, at) = rx.recv_timeout(Duration::from_secs(3)).expect("the event comes through");
            if l != "@@hb" {
                break (l, at);
            }
        };
        let _ = c.kill();
        let _ = c.wait();
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(line, r#"@@123-4 {"event":"stop"}"#);
        assert!(at.duration_since(dropped) < Duration::from_millis(400), "within a few tenths of a second: {:?}", at.duration_since(dropped));
    }

    #[test]
    fn a_new_watcher_ends_the_one_before_and_gets_every_event() {
        let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join(format!("cue-watch2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue/in")).unwrap();
        // A watcher left behind by an earlier Cue (its output going nowhere, as on a server that never ends it).
        let mut old = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::null()).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(400));
        let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let out = c.stdout.take().unwrap();
        std::thread::spawn(move || {
            for l in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send(l);
            }
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), "@@hb");
        let gone = (0..30).any(|_| {
            std::thread::sleep(Duration::from_millis(100));
            matches!(old.try_wait(), Ok(Some(_)))
        });
        for i in 0..10 {
            std::fs::write(home.join(format!(".cue/in/9-{i}.json")), format!("{{\"n\":{i}}}\n")).unwrap();
        }
        let mut got = vec![];
        while let Ok(l) = rx.recv_timeout(Duration::from_secs(2)) {
            if l != "@@hb" {
                got.push(l);
            }
            if got.len() == 10 {
                break;
            }
        }
        let _ = old.kill();
        let _ = old.wait();
        let _ = c.kill();
        let _ = c.wait();
        let _ = std::fs::remove_dir_all(&home);
        assert!(gone, "the earlier watcher left");
        got.sort();
        assert_eq!(got, (0..10).map(|i| format!("@@9-{i} {{\"n\":{i}}}")).collect::<Vec<_>>(), "every event, whole, to the current watcher");
    }

    #[test]
    fn another_macs_watcher_is_left_alone_and_an_event_a_dead_watcher_claimed_is_delivered() {
        let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join(format!("cue-watch3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue/in")).unwrap();
        let mut other = Command::new("/bin/sh").arg("-c").arg(WATCH.replace("@MAC@", "another-mac")).env("HOME", &home).stdout(Stdio::null()).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(400));
        // Claimed by a watcher that died before handing it over (no process has that pid).
        std::fs::write(home.join(".cue/in/7-1.json.999999"), "{\"n\":7}\n").unwrap();
        let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
        let out = c.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for l in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send(l);
            }
        });
        // The other Mac's watcher may take it first: then it's handed over there, which is fine too.
        let mut got = None;
        let end = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < end {
            if let Ok(l) = rx.recv_timeout(Duration::from_millis(200)) {
                if l != "@@hb" {
                    got = Some(l);
                    break;
                }
            }
        }
        // Its file goes right after its line is printed.
        let count = || std::fs::read_dir(home.join(".cue/in")).unwrap().count();
        let left = (0..20).map(|_| { std::thread::sleep(Duration::from_millis(50)); count() }).find(|n| *n == 0).unwrap_or_else(count);
        let other_alive = matches!(other.try_wait(), Ok(None));
        for p in [&mut other, &mut c] {
            let _ = p.kill();
            let _ = p.wait();
        }
        let _ = std::fs::remove_dir_all(&home);
        assert!(other_alive, "another Mac's watcher keeps running");
        assert_eq!(left, 0, "the claimed event was put back and handed over");
        if let Some(l) = got {
            assert_eq!(l, r#"@@7-1 {"n":7}"#);
        }
    }

    #[test]
    fn a_line_from_a_machine_is_capped_and_the_stream_goes_on_after_one_too_long() {
        let long = "x".repeat(50);
        let data = format!("@@hb\r\n{long}\nshort\nlast");
        let mut r = BufReader::with_capacity(8, data.as_bytes());
        assert_eq!(capped_line(&mut r, 20).unwrap(), Some(Some("@@hb".into())));
        assert_eq!(capped_line(&mut r, 20).unwrap(), Some(None), "over the cap: skipped whole");
        assert_eq!(capped_line(&mut r, 20).unwrap(), Some(Some("short".into())), "and the next line is intact");
        assert_eq!(capped_line(&mut r, 20).unwrap(), Some(Some("last".into())), "a last line without its newline");
        assert_eq!(capped_line(&mut r, 20).unwrap(), None, "the end");
        let mut exact = BufReader::new("12345\n".as_bytes());
        assert_eq!(capped_line(&mut exact, 5).unwrap(), Some(Some("12345".into())), "the cap counts the line, not its newline");
    }

    #[test]
    fn the_watcher_drops_answers_nobody_collected_after_a_day() {
        let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::temp_dir().join(format!("cue-watch-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue/out")).unwrap();
        let (old, fresh) = (home.join(".cue/out/1-1.json"), home.join(".cue/out/2-2.json"));
        std::fs::write(&old, "{}").unwrap();
        std::fs::write(&fresh, "{}").unwrap();
        let two_days_ago = std::time::SystemTime::now() - Duration::from_secs(2 * 86400);
        std::fs::File::options().write(true).open(&old).unwrap().set_modified(two_days_ago).unwrap();
        let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
        let out = c.stdout.take().unwrap();
        let mut r = BufReader::new(out);
        let _ = capped_line(&mut r, 1024); // the first heartbeat: the sweep ran just before it
        std::thread::sleep(Duration::from_millis(300));
        let _ = c.kill();
        let _ = c.wait();
        let (old_there, fresh_there) = (old.exists(), fresh.exists());
        let _ = std::fs::remove_dir_all(&home);
        assert!(!old_there, "a day-old answer is gone");
        assert!(fresh_there, "a fresh one stays");
    }

    #[test]
    fn events_waiting_on_the_machine_come_before_its_first_beat() {
        let _turn = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
        // An agent ended while Cue wasn't watching: its event waits in ~/.cue/in. The stream hands it
        // over before saying the machine is reached, so a rebuild on reaching it knows it ended.
        let home = std::env::temp_dir().join(format!("cue-watch-q-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue/in")).unwrap();
        std::fs::write(home.join(".cue/in/100-1.json"), "{\"event\":\"end\"}\n").unwrap();
        let mut c = Command::new("/bin/sh").arg("-c").arg(watch_script()).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
        let mut lines = BufReader::new(c.stdout.take().unwrap()).lines();
        let first = lines.next().unwrap().unwrap();
        let second = lines.next().unwrap().unwrap();
        let _ = c.kill();
        let _ = c.wait();
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!((first.as_str(), second.as_str()), (r#"@@100-1 {"event":"end"}"#, "@@hb"));
    }

    #[test]
    fn the_hook_drops_each_event_for_cue_and_never_waits_when_cue_isnt_watching() {
        let home = std::env::temp_dir().join(format!("cue-mh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        run_sh(SCRIPT, &home, &["stop", "claude"], r#"{"session_id":"s1","last_assistant_message":"Done."}"#);
        let files: Vec<_> = std::fs::read_dir(home.join(".cue/in")).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(files[0].metadata().unwrap().permissions().mode() & 0o777, 0o600, "the event is readable by you only");
        }
        let ev: Value = serde_json::from_str(&std::fs::read_to_string(files[0].path()).unwrap()).unwrap();
        assert_eq!(ev["event"], "stop");
        assert_eq!(ev["pane"], "%7");
        assert_eq!(ev["input"]["last_assistant_message"], "Done.");
        // A permission with nobody watching: it leaves at once, with no answer (the terminal asks).
        let t = std::time::Instant::now();
        let out = run_sh(SCRIPT, &home, &["permission", "claude"], r#"{"session_id":"s1","tool_name":"Bash"}"#);
        assert!(out.is_empty() && t.elapsed() < Duration::from_secs(3));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_permission_waits_for_cues_answer_while_cue_is_watching() {
        let home = std::env::temp_dir().join(format!("cue-mh2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue/out")).unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        std::fs::write(home.join(".cue/watching"), format!("{now}\n")).unwrap();
        let h2 = home.clone();
        // Cue's side: answer whatever question shows up.
        let answerer = std::thread::spawn(move || {
            for _ in 0..50 {
                std::thread::sleep(Duration::from_millis(100));
                if let Some(f) = std::fs::read_dir(h2.join(".cue/in")).ok().and_then(|mut d| d.next()).and_then(|e| e.ok()) {
                    let id = f.path().file_stem().unwrap().to_string_lossy().to_string();
                    std::fs::write(h2.join(format!(".cue/out/{id}.json")), r#"{"decision":"allow"}"#).unwrap();
                    return;
                }
            }
        });
        let out = run_sh(SCRIPT, &home, &["permission", "claude"], r#"{"session_id":"s1","tool_name":"Bash"}"#);
        answerer.join().unwrap();
        assert_eq!(out, r#"{"decision":"allow"}"#);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_event_from_a_machine_reaches_cue_as_the_macs_hook_would_send_it_placed_on_that_machine() {
        let ev = json!({ "event": "prompt", "harness": "claude", "pane": "%3", "tmux": "1", "input": { "session_id": "s9", "cwd": "/home/me/app", "prompt": "pwd", "transcript_path": "/home/me/.claude/x.jsonl" } });
        let m = to_cue("build-box", &ev).unwrap();
        assert_eq!(m["type"], "event");
        assert_eq!(m["event"], "active");
        assert_eq!(m["message"], "pwd");
        assert_eq!(m["machine"], "build-box");
        assert_eq!(m["tmux_pane"], "%3");
        let ask = to_cue("build-box", &json!({ "event": "permission", "harness": "claude", "input": { "session_id": "s9", "tool_name": "Bash", "tool_input": { "command": "ls" } } })).unwrap();
        assert_eq!(ask["type"], "ask");
        assert_eq!(ask["kind"], "permission");
        assert_eq!(ask["tmux_pane"], "", "not in tmux: no pane");
        assert!(to_cue("b", &json!({ "event": "whatever", "input": {} })).is_none());
        assert_eq!(parse(r#"@@1700000000-42 {"event":"stop","input":{}}"#).unwrap().0, "1700000000-42");
        assert!(parse("@@../../x {}").is_none(), "an id is digits and a dash");
    }
}
