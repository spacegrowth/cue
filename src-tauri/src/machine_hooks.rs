//! Cue's hooks on a machine (+ New → Machine), for the agents installed there. Cue's own hook is a
//! Mac program, so a machine gets a small POSIX script instead, ~/.cue/cue-hook: it drops each hook
//! event into ~/.cue/in/ and, for a permission question, waits for Cue's answer in ~/.cue/out/ while
//! Cue is watching. On the Mac, one SSH stream per machine picks the events up and hands each to Cue
//! as a local hook would, so everything after that is the same as on this Mac. Nothing listens on the
//! network. When Cue isn't watching, the script leaves at once and the agent asks in its terminal.
use crate::machines::{self, Machine};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

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
{
  printf '{"event":"%s","harness":"%s","pane":"%s","tmux":"%s","input":' "$1" "$2" "${TMUX_PANE:-}" "${TMUX:+1}"
  cat
  printf '}\n'
} >"$d/in/$id.tmp" 2>/dev/null && mv "$d/in/$id.tmp" "$d/in/$id.json"
[ "$1" = permission ] || exit 0
# A permission question: wait for the answer while Cue is watching (it stamps the time every second).
while :; do
  if [ -e "$d/out/$id.json" ]; then cat "$d/out/$id.json"; rm -f "$d/out/$id.json"; exit 0; fi
  seen=$(cat "$d/watching" 2>/dev/null || echo 0)
  [ $(( $(date +%s) - ${seen:-0} )) -lt 20 ] || { rm -f "$d/in/$id.json"; exit 0; }
  sleep 0.2 2>/dev/null || sleep 1
done
"#;

/// What the watcher runs on the machine: print each waiting event as one line, looking ten times a second
/// (an event reaches Cue in about a tenth of a second, as a hook on this Mac does), and once a second stamp
/// the time and say it's alive. The heartbeat matters: if the Cue that started this quits, the next one
/// fails to reach it, so ssh ends instead of living on and taking events nobody reads. A `sleep` that
/// can't do fractions (not GNU's or BusyBox's) falls back to a second. ("cue-watch" marks these processes.)
const WATCH: &str = r#"# cue-watch
d="$HOME/.cue"; mkdir -p "$d/in" "$d/out"
n=0
while :; do
  if [ "$n" -eq 0 ]; then
    date +%s >"$d/watching"
    echo "@@hb" || exit 0
  fi
  n=$(( (n + 1) % 10 ))
  for f in "$d"/in/*.json; do
    [ -e "$f" ] || continue
    printf '@@%s ' "$(basename "$f" .json)"; tr -d '\n' <"$f"; echo; rm -f "$f"
  done
  sleep 0.1 2>/dev/null || { sleep 1; n=0; }
done"#;

/// Set Cue's hooks up on the machine for the agents found there (`tools`, from its check): the script,
/// and Cue's entries in each agent's settings (backed up first, everything else kept). What it did.
pub fn setup(m: &Machine, tools: &Value) -> Result<Vec<String>, String> {
    let has = |t: &str| tools.get(t).and_then(Value::as_bool).unwrap_or(false);
    let agents: Vec<(&str, &str, &str)> = [("claude", "Claude Code", ".claude/settings.json"), ("codex", "Codex", ".codex/hooks.json")].into_iter().filter(|(t, _, _)| has(t)).collect();
    if agents.is_empty() {
        return Ok(vec![]);
    }
    machines::run_with_input(&m.host, r#"mkdir -p "$HOME/.cue" && cat >"$HOME/.cue/cue-hook.tmp" && chmod 755 "$HOME/.cue/cue-hook.tmp" && mv "$HOME/.cue/cue-hook.tmp" "$HOME/.cue/cue-hook""#, SCRIPT)?;
    let mut did = vec![];
    for (harness, label, file) in agents {
        let path = format!("$HOME/{file}");
        let current = machines::run(&m.host, &format!(r#"cat "{path}" 2>/dev/null; true"#))?;
        let cfg: Value = if current.trim().is_empty() { json!({}) } else { serde_json::from_str(&current).map_err(|e| format!("~/{file} on {} isn't valid JSON ({e}): fix it there, then Check again", m.name))? };
        let merged = crate::config::merge_hooks(cfg.clone(), "$HOME/.cue/cue-hook", harness)?;
        if merged == cfg {
            continue; // set up already
        }
        let write = format!(
            r#"f="{path}"; mkdir -p "$(dirname "$f")"; [ -e "$f" ] && cp "$f" "$f.cue-backup-$(date +%s)"; cat >"$f.cue-tmp" && mv "$f.cue-tmp" "$f""#
        );
        machines::run_with_input(&m.host, &write, &(serde_json::to_string_pretty(&merged).unwrap_or_default() + "\n"))?;
        did.push(format!("Set up Cue's hooks for {label} (~/{file}, backed up first)"));
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
    crate::machine_logs::stop_strays();
    for m in machines::list() {
        watch(&m);
    }
}

pub fn watch(m: &Machine) {
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut w = watchers().lock().unwrap();
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
    if let Some(stop) = watchers().lock().unwrap().remove(name) {
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// One SSH stream: runs until the connection drops or the machine is removed.
fn stream(m: &Machine, stop: &std::sync::atomic::AtomicBool) -> Result<(), String> {
    let mut child = machines::stream_command(&m.host, WATCH).stdout(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let out = child.stdout.take().ok_or("no output")?;
    for line in BufReader::new(out).lines() {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        let Ok(line) = line else { break };
        if line == "@@hb" {
            continue; // it's alive
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
    // The session reads its log from a live copy on this Mac (machine_logs); the hooks carry on as well.
    if let Some(remote) = msg["transcript_path"].as_str().filter(|p| !p.is_empty()).map(str::to_string) {
        if ev["event"] == "end" {
            crate::machine_logs::stop(m, &remote);
        } else {
            let local = crate::machine_logs::local_copy(m, &remote);
            if !local.is_empty() {
                msg["transcript_path"] = json!(local);
            }
        }
    }
    if msg["type"] != "ask" {
        if let Some(mut s) = crate::hook::connect(Duration::from_millis(800)) {
            let _ = s.write_all(format!("{msg}\n").as_bytes());
        }
        return;
    }
    let (m, id, p) = (m.clone(), id.to_string(), ev["input"].clone());
    std::thread::spawn(move || {
        let answer = ask(&msg).map(|d| crate::hook::to_claude(&d, &p).to_string()).unwrap_or_default();
        let _ = machines::run_with_input(&m.host, &format!(r#"cat >"$HOME/.cue/out/{id}.tmp" && mv "$HOME/.cue/out/{id}.tmp" "$HOME/.cue/out/{id}.json""#), &answer);
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

    fn run_sh(script: &str, home: &std::path::Path, args: &[&str], input: &str) -> String {
        let mut c = Command::new("/bin/sh").arg("-c").arg(script).arg("cue-hook").args(args).env("HOME", home).env("TMUX", "/tmp/x,1,0").env("TMUX_PANE", "%7").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        String::from_utf8_lossy(&c.wait_with_output().unwrap().stdout).to_string()
    }

    #[test]
    fn the_watcher_hands_an_event_over_within_a_fraction_of_a_second() {
        let home = std::env::temp_dir().join(format!("cue-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cue/in")).unwrap();
        let mut c = Command::new("/bin/sh").arg("-c").arg(WATCH).env("HOME", &home).stdout(Stdio::piped()).spawn().unwrap();
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
    fn the_hook_drops_each_event_for_cue_and_never_waits_when_cue_isnt_watching() {
        let home = std::env::temp_dir().join(format!("cue-mh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        run_sh(SCRIPT, &home, &["stop", "claude"], r#"{"session_id":"s1","last_assistant_message":"Done."}"#);
        let files: Vec<_> = std::fs::read_dir(home.join(".cue/in")).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
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
