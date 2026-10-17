//! A Claude Code session's MCP servers: which are on for its folder, where each is configured, and
//! what the session called through them. Turning one on or off edits Claude Code's own config
//! (`~/.claude.json`); the session is then restarted, since Claude Code reads that config at start.
//!
//! Where Claude Code keeps them:
//! - `mcpServers` at the top of ~/.claude.json: "user" scope, on for every folder. Claude Code has no
//!   per-folder off switch for these, so Cue shows them but doesn't toggle them.
//! - `projects[<folder>].mcpServers`: "local" scope, this folder only.
//! - `<folder>/.mcp.json`: "project" scope (checked in with the project), approved or refused through
//!   `projects[<folder>].enabledMcpjsonServers` / `disabledMcpjsonServers`.
//!
//! Cue's own key `projects[<folder>].cueMcpOff` holds the definition of a local server turned off here,
//! so it can come back; Claude Code ignores keys it doesn't know.
//!
//! A session can also be pinned to a set of servers of its own, whatever its folder has on: Cue writes
//! `<Cue's folder>/mcp/<session id>.json` (`{ "mcpServers": { name: definition } }`, the selected names
//! being its keys) and starts or resumes Claude Code with `--mcp-config <that file> --strict-mcp-config`.
//! Claude Code's help: "--mcp-config <configs...>  Load MCP servers from JSON files or strings
//! (space-separated)" and "--strict-mcp-config  Only use MCP servers from --mcp-config, ignoring all
//! other MCP configurations". So with a selection, servers Claude Code adds itself (its claude.ai
//! connectors, claude-in-chrome: "session" scope here, in no config Cue can pass) may be left out.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

/// Claude Code's config file.
fn claude_json_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".claude.json")
}

/// Only the last so much of a transcript is read.
const TRANSCRIPT_CAP: u64 = 8 * 1024 * 1024;

const OFF_KEY: &str = "cueMcpOff";

/// What a session called through each server, from its transcript.
#[derive(Default)]
struct Use {
    used: u64,
    failed: u64,
    tools: BTreeMap<String, (u64, u64)>,
}

/// A server's definition, shown as its transport and target.
fn describe(def: &Value) -> (String, String) {
    let s = |k: &str| def.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let command = s("command");
    let url = s("url");
    let transport = match s("type").as_str() {
        "" if !command.is_empty() => "stdio".into(),
        "" if !url.is_empty() => "http".into(),
        t => t.to_string(),
    };
    let target = if !command.is_empty() {
        let args: Vec<String> = def.get("args").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
        std::iter::once(command).chain(args).collect::<Vec<_>>().join(" ")
    } else {
        url
    };
    (transport, target)
}

fn read_json(path: &Path) -> Value {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
}

/// The servers in an object of definitions, by name.
fn servers_in(v: Option<&Value>) -> Vec<(String, Value)> {
    v.and_then(Value::as_object).into_iter().flatten().filter(|(_, d)| d.is_object()).map(|(k, d)| (k.clone(), d.clone())).collect()
}

fn mcp_json(cwd: &str) -> PathBuf {
    Path::new(cwd).join(".mcp.json")
}

/// Tool calls per server in a session's transcript: `mcp__<server>__<tool>` names, split at the last
/// `__`; a call failed when its tool_result says is_error. The last 8 MB of the file.
fn usage(transcript_path: &str) -> (BTreeMap<String, Use>, u64) {
    let mut out: BTreeMap<String, Use> = BTreeMap::new();
    let mut calls = 0;
    let Ok(mut f) = std::fs::File::open(transcript_path) else { return (out, 0) };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut text = String::new();
    if len > TRANSCRIPT_CAP {
        f.seek(std::io::SeekFrom::Start(len - TRANSCRIPT_CAP)).ok();
    }
    let mut raw = vec![];
    if f.read_to_end(&mut raw).is_err() {
        return (out, 0);
    }
    text.push_str(&String::from_utf8_lossy(&raw));
    let body = if len > TRANSCRIPT_CAP { text.split_once('\n').map(|(_, rest)| rest).unwrap_or("") } else { text.as_str() };
    // tool_use id → (server, tool), to pair with its result.
    let mut open: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
    for line in body.lines() {
        if !line.contains("mcp__") && !line.contains("tool_result") {
            continue;
        }
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        for b in e.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
            match b.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let name = b.get("name").and_then(Value::as_str).unwrap_or("");
                    let Some(rest) = name.strip_prefix("mcp__") else { continue };
                    let Some((server, tool)) = rest.rsplit_once("__") else { continue };
                    calls += 1;
                    let u = out.entry(server.to_string()).or_default();
                    u.used += 1;
                    u.tools.entry(tool.to_string()).or_default().0 += 1;
                    if let Some(id) = b.get("id").and_then(Value::as_str) {
                        open.insert(id.to_string(), (server.to_string(), tool.to_string()));
                    }
                }
                Some("tool_result") => {
                    let Some(id) = b.get("tool_use_id").and_then(Value::as_str) else { continue };
                    let Some((server, tool)) = open.remove(id) else { continue };
                    if b.get("is_error").and_then(Value::as_bool) == Some(true) {
                        let u = out.entry(server).or_default();
                        u.failed += 1;
                        u.tools.entry(tool).or_default().1 += 1;
                    }
                }
                _ => {}
            }
        }
    }
    (out, calls)
}

fn row(name: &str, scope: &str, def: &Value, from: &str, enabled: bool, toggle: bool, u: Option<&Use>) -> Value {
    let (transport, target) = describe(def);
    let empty = Use::default();
    let u = u.unwrap_or(&empty);
    json!({
        "name": name, "scope": scope, "transport": transport, "target": target, "from": from,
        "enabled": enabled, "selected": enabled, "toggle": toggle, "used": u.used, "failed": u.failed,
        "tools": u.tools.iter().map(|(t, (calls, failed))| json!({ "name": t, "calls": calls, "failed": failed })).collect::<Vec<_>>(),
    })
}

/// The servers of a session in `cwd`: every scope, with what the session called through each.
pub fn for_session(cwd: &str, transcript_path: &str) -> Value {
    for_session_in(&claude_json_path(), cwd, transcript_path)
}

/// The same for a session Cue knows by id: each server's `selected` is whether it's in the session's
/// selection when it has one (`pinned` true), else whether it's on for the folder. Session-scope servers
/// (Claude Code's own) stay listed but can't be picked (`toggle` false): they're in no config Cue can
/// pass, and `--strict-mcp-config` may drop them.
pub fn for_session_id(cwd: &str, transcript_path: &str, session_id: &str) -> Value {
    with_selection(for_session(cwd, transcript_path), read_selection(&selection_path(session_id)), &selection_path(session_id))
}

/// Marks `selected` from a selection (names → definitions), adding any picked server the configs no
/// longer have, and `pinned`.
pub fn with_selection(mut v: Value, sel: Option<Map<String, Value>>, sel_path: &Path) -> Value {
    let Some(sel) = sel else {
        v["pinned"] = json!(false);
        return v;
    };
    let mut seen = vec![];
    if let Some(rows) = v.get_mut("servers").and_then(Value::as_array_mut) {
        for r in rows.iter_mut() {
            let name = r["name"].as_str().unwrap_or("").to_string();
            if r["scope"] != "session" {
                r["selected"] = json!(sel.contains_key(&name));
            }
            seen.push(name);
        }
        for (name, def) in &sel {
            if !seen.contains(name) {
                let mut r = row(name, "available", def, &sel_path.to_string_lossy(), false, true, None);
                r["selected"] = json!(true);
                rows.push(r);
            }
        }
    }
    v["pinned"] = json!(true);
    v
}

/// What a server's name looks like in its tools' names (`mcp__<it>__<tool>`): "claude.ai Linear" →
/// "claude_ai_Linear", as Claude Code writes it.
fn tool_prefix(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

/// What a running session really has from each server, by tool prefix: "on" (its tools loaded),
/// "sign-in" (only its authenticate tools: it waits for you in /mcp), "connecting", "failed". From
/// Claude Code's own `deferred_tools_delta` records in the transcript, replayed from the start (each
/// adds and removes tools, and carries the servers still connecting or failed at that point). None
/// when it can't be told: nothing written since `since_ms` (it restarted and hasn't had a turn yet),
/// or no such record at all (Claude Code loads every tool up front then, and says nothing).
/// With it, each server's tools by name (its sign-in ones left out), for the picker to list.
fn live_state(transcript_path: &str, since_ms: u64) -> Option<(BTreeMap<String, &'static str>, BTreeMap<String, Vec<String>>)> {
    // SHORTCUT: the whole transcript is read each time the picker asks (about 0.1 s for a 36 MB one); fine
    // while it's only read on opening ⋯ → MCP servers. Keep the replayed state per (path, length) and read
    // only what was added if it's ever read more often.
    let text = std::fs::read_to_string(transcript_path).ok()?;
    let mut tools: std::collections::BTreeSet<String> = Default::default();
    let (mut pending, mut failed, mut seen, mut fresh) = (vec![], vec![], false, false);
    let names = |a: &Value, k: &str| -> Vec<String> { a.get(k).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect() };
    for line in text.lines() {
        if !fresh {
            let at = line.find("\"timestamp\":\"").and_then(|i| line.get(i + 13..i + 37)).and_then(super::transcript::iso_ms);
            fresh = at.is_some_and(|t| t >= since_ms);
        }
        if !line.contains("\"deferred_tools_delta\"") {
            continue;
        }
        let Ok(e) = serde_json::from_str::<Value>(line) else { continue };
        let Some(a) = e.get("attachment").filter(|a| a["type"] == "deferred_tools_delta") else { continue };
        seen = true;
        for t in names(a, "removedNames") {
            tools.remove(&t);
        }
        tools.extend(names(a, "addedNames").into_iter().filter(|t| t.starts_with("mcp__")));
        pending = names(a, "pendingMcpServers");
        failed = names(a, "failedMcpServers");
    }
    if !seen || !fresh {
        return None;
    }
    let mut out: BTreeMap<String, &'static str> = BTreeMap::new();
    let mut by_server: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for t in &tools {
        let Some((server, tool)) = t.strip_prefix("mcp__").and_then(|r| r.rsplit_once("__")) else { continue };
        let auth = tool == "authenticate" || tool == "complete_authentication";
        let s = out.entry(server.to_string()).or_insert("sign-in");
        if !auth {
            *s = "on";
            by_server.entry(server.to_string()).or_default().push(tool.to_string());
        }
    }
    for n in pending {
        out.entry(tool_prefix(&n)).or_insert("connecting");
    }
    for n in failed {
        out.insert(tool_prefix(&n), "failed");
    }
    Some((out, by_server))
}

/// Marks each server with what the running session really has from it (`live`: see live_state; "off"
/// when it has nothing from it) and the names of the tools it loaded (`loaded`), and `live_known`. Not running (`started_ms` None): nothing to tell.
pub fn with_live(mut v: Value, transcript_path: &str, started_ms: Option<u64>) -> Value {
    let state = started_ms.and_then(|t| live_state(transcript_path, t));
    v["running"] = json!(started_ms.is_some());
    v["live_known"] = json!(state.is_some());
    if let (Some((state, tools)), Some(rows)) = (state, v.get_mut("servers").and_then(Value::as_array_mut)) {
        for r in rows.iter_mut() {
            let key = tool_prefix(r["name"].as_str().unwrap_or(""));
            r["live"] = json!(state.get(&key).copied().unwrap_or("off"));
            r["loaded"] = json!(tools.get(&key).cloned().unwrap_or_default());
        }
    }
    v
}

/// Every server definition Cue can find for `cwd`, by name, the first found winning: user scope, this
/// folder's local ones, its `.mcp.json`, the ones Cue saved when they were turned off here, then other
/// folders' (local, then `.mcp.json`).
pub fn definitions(cwd: &str) -> Map<String, Value> {
    definitions_in(&claude_json_path(), cwd)
}

pub fn definitions_in(cfg_path: &Path, cwd: &str) -> Map<String, Value> {
    let cfg = read_json(cfg_path);
    let proj = cfg.pointer(&format!("/projects/{}", escape(cwd)));
    let mut out = Map::new();
    let mut add = |list: Vec<(String, Value)>| {
        for (name, def) in list {
            out.entry(name).or_insert(def);
        }
    };
    add(servers_in(cfg.get("mcpServers")));
    add(servers_in(proj.and_then(|p| p.get("mcpServers"))));
    add(servers_in(read_json(&mcp_json(cwd)).get("mcpServers")));
    add(servers_in(proj.and_then(|p| p.get(OFF_KEY))));
    add(elsewhere(&cfg, cwd).into_iter().map(|(n, d, _)| (n, d)).collect());
    out
}

/// Where a session's own selection lives: `<Cue's folder>/mcp/<session id>.json`.
pub fn selection_path(session_id: &str) -> PathBuf {
    selection_path_in(&crate::server::cue_dir().join("mcp"), session_id)
}

fn selection_path_in(dir: &Path, session_id: &str) -> PathBuf {
    let safe: String = session_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    dir.join(format!("{safe}.json"))
}

/// A selection file's servers (names → definitions); None when there's none.
pub fn read_selection(path: &Path) -> Option<Map<String, Value>> {
    if !path.exists() {
        return None;
    }
    Some(read_json(path).get("mcpServers").and_then(Value::as_object).cloned().unwrap_or_default())
}

/// Write a selection: the named servers' definitions from `defs`. Names Cue has no definition of are
/// refused, naming them. Written to a temp file and renamed over.
pub fn write_selection(path: &Path, defs: &Map<String, Value>, names: &[String]) -> Result<usize, String> {
    let unknown: Vec<&str> = names.iter().filter(|n| !defs.contains_key(n.as_str())).map(String::as_str).collect();
    if !unknown.is_empty() {
        return Err(format!("Cue has no definition of {}", unknown.join(", ")));
    }
    let picked: Map<String, Value> = names.iter().map(|n| (n.clone(), defs[n.as_str()].clone())).collect();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't make {}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&json!({ "mcpServers": picked })).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("Couldn't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;
    Ok(picked.len())
}

/// The flags that start or resume a session with its selection, when it has one:
/// `--mcp-config <file> --strict-mcp-config`. Empty without.
pub fn selection_flags(session_id: &str) -> Vec<String> {
    flags_for(&selection_path(session_id))
}

pub fn flags_for(path: &Path) -> Vec<String> {
    if path.exists() {
        vec!["--mcp-config".into(), path.to_string_lossy().to_string(), "--strict-mcp-config".into()]
    } else {
        vec![]
    }
}

pub fn for_session_in(cfg_path: &Path, cwd: &str, transcript_path: &str) -> Value {
    let cfg = read_json(cfg_path);
    let cfg_s = cfg_path.to_string_lossy().to_string();
    let (uses, calls) = usage(transcript_path);
    let mut rows: Vec<Value> = vec![];
    let mut seen: Vec<String> = vec![];
    let proj = cfg.pointer(&format!("/projects/{}", escape(cwd)));
    for (name, def) in servers_in(cfg.get("mcpServers")) {
        rows.push(row(&name, "user", &def, &cfg_s, true, false, uses.get(&name)));
        seen.push(name);
    }
    for (name, def) in servers_in(proj.and_then(|p| p.get("mcpServers"))) {
        if !seen.contains(&name) {
            rows.push(row(&name, "local", &def, &cfg_s, true, true, uses.get(&name)));
            seen.push(name);
        }
    }
    let mcp_path = mcp_json(cwd);
    let disabled: Vec<String> = proj.and_then(|p| p.get("disabledMcpjsonServers")).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
    for (name, def) in servers_in(read_json(&mcp_path).get("mcpServers")) {
        if !seen.contains(&name) {
            rows.push(row(&name, "project", &def, &mcp_path.to_string_lossy(), !disabled.contains(&name), true, uses.get(&name)));
            seen.push(name);
        }
    }
    // Off here but defined elsewhere: Cue's saved ones first, then other folders'.
    for (name, def) in servers_in(proj.and_then(|p| p.get(OFF_KEY))) {
        if !seen.contains(&name) {
            rows.push(row(&name, "available", &def, &cfg_s, false, true, uses.get(&name)));
            seen.push(name);
        }
    }
    for (name, def, from) in elsewhere(&cfg, cwd) {
        if !seen.contains(&name) {
            rows.push(row(&name, "available", &def, &from, false, true, uses.get(&name)));
            seen.push(name);
        }
    }
    for (name, u) in &uses {
        if !seen.contains(name) {
            rows.push(row(name, "session", &Value::Null, "", true, false, Some(u)));
        }
    }
    json!({ "servers": rows, "calls": calls, "pinned": false })
}

/// Servers defined for other folders: their `mcpServers`, and their `.mcp.json` where the folder exists.
fn elsewhere(cfg: &Value, cwd: &str) -> Vec<(String, Value, String)> {
    let mut out = vec![];
    for (folder, p) in cfg.get("projects").and_then(Value::as_object).into_iter().flatten() {
        if folder == cwd {
            continue;
        }
        for (name, def) in servers_in(p.get("mcpServers")) {
            out.push((name, def, format!("{folder} (local)")));
        }
        if Path::new(folder).is_dir() {
            let f = mcp_json(folder);
            for (name, def) in servers_in(read_json(&f).get("mcpServers")) {
                out.push((name, def, f.to_string_lossy().to_string()));
            }
        }
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

/// Every configured server, for a Settings list: `{ name, scope, transport, target, project, from }`,
/// `project` "" for user scope.
pub fn all() -> Vec<Value> {
    all_in(&claude_json_path())
}

pub fn all_in(cfg_path: &Path) -> Vec<Value> {
    let cfg = read_json(cfg_path);
    let cfg_s = cfg_path.to_string_lossy().to_string();
    let entry = |name: &str, scope: &str, def: &Value, project: &str, from: &str| {
        let (transport, target) = describe(def);
        json!({ "name": name, "scope": scope, "transport": transport, "target": target, "project": project, "from": from })
    };
    let mut out = vec![];
    for (name, def) in servers_in(cfg.get("mcpServers")) {
        out.push(entry(&name, "user", &def, "", &cfg_s));
    }
    for (folder, p) in cfg.get("projects").and_then(Value::as_object).into_iter().flatten() {
        for (name, def) in servers_in(p.get("mcpServers")) {
            out.push(entry(&name, "local", &def, folder, &cfg_s));
        }
        if Path::new(folder).is_dir() {
            let f = mcp_json(folder);
            for (name, def) in servers_in(read_json(&f).get("mcpServers")) {
                out.push(entry(&name, "project", &def, folder, &f.to_string_lossy()));
            }
        }
    }
    out
}

/// Turn a server on or off for `cwd` (see the module doc for where each scope lives). The session
/// has to be restarted for Claude Code to notice.
pub fn set_enabled(cwd: &str, name: &str, on: bool) -> Result<(), String> {
    set_enabled_in(&claude_json_path(), cwd, name, on)
}

pub fn set_enabled_in(cfg_path: &Path, cwd: &str, name: &str, on: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(cfg_path).map_err(|e| format!("Couldn't read {}: {e}", cfg_path.display()))?;
    let mut cfg: Value = serde_json::from_str(&text).map_err(|e| format!("{} isn't valid JSON ({e})", cfg_path.display()))?;
    if !cfg.is_object() {
        return Err(format!("{} isn't an object", cfg_path.display()));
    }
    if cfg.pointer(&format!("/mcpServers/{}", escape(name))).is_some() {
        return Err("that server is on for every folder (user scope)".into());
    }
    let in_project = read_json(&mcp_json(cwd)).pointer(&format!("/mcpServers/{}", escape(name))).is_some();
    let from_elsewhere = elsewhere(&cfg, cwd).into_iter().find(|(n, _, _)| n == name).map(|(_, d, _)| d);
    let projects = cfg.as_object_mut().unwrap().entry("projects").or_insert_with(|| json!({}));
    if !projects.is_object() {
        return Err("\"projects\" isn't an object".into());
    }
    let proj = projects.as_object_mut().unwrap().entry(cwd.to_string()).or_insert_with(|| json!({}));
    let Some(proj) = proj.as_object_mut() else { return Err("that folder's entry isn't an object".into()) };
    if in_project {
        let (into, out_of) = if on { ("enabledMcpjsonServers", "disabledMcpjsonServers") } else { ("disabledMcpjsonServers", "enabledMcpjsonServers") };
        list_remove(proj, out_of, name);
        list_add(proj, into, name);
    } else if on {
        let present = proj.get("mcpServers").and_then(|m| m.get(name)).is_some();
        if !present {
            let saved = proj.get_mut(OFF_KEY).and_then(Value::as_object_mut).and_then(|m| m.remove(name));
            let def = saved.or(from_elsewhere).ok_or(format!("Cue has no definition of {name} to turn on"))?;
            proj.entry("mcpServers").or_insert_with(|| json!({})).as_object_mut().ok_or("\"mcpServers\" isn't an object")?.insert(name.to_string(), def);
        }
        if proj.get(OFF_KEY).and_then(Value::as_object).is_some_and(Map::is_empty) {
            proj.remove(OFF_KEY);
        }
    } else {
        let def = proj.get_mut("mcpServers").and_then(Value::as_object_mut).and_then(|m| m.remove(name)).ok_or(format!("{name} isn't on for this folder"))?;
        proj.entry(OFF_KEY).or_insert_with(|| json!({})).as_object_mut().ok_or("cueMcpOff isn't an object")?.insert(name.to_string(), def);
    }
    write_config(cfg_path, &cfg)
}

fn list_remove(proj: &mut Map<String, Value>, key: &str, name: &str) {
    if let Some(a) = proj.get_mut(key).and_then(Value::as_array_mut) {
        a.retain(|v| v.as_str() != Some(name));
    }
}

fn list_add(proj: &mut Map<String, Value>, key: &str, name: &str) {
    let a = proj.entry(key).or_insert_with(|| json!([]));
    if let Some(a) = a.as_array_mut() {
        if !a.iter().any(|v| v.as_str() == Some(name)) {
            a.push(json!(name));
        }
    }
}

/// Write the config: backed up to `<file>.cue-bak` once a day, written to a temp file beside it and
/// renamed over, so a crash mid-write leaves the old file whole.
fn write_config(path: &Path, cfg: &Value) -> Result<(), String> {
    let bak = PathBuf::from(format!("{}.cue-bak", path.display()));
    let fresh = std::fs::metadata(&bak).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() < 24 * 3600);
    if !fresh {
        std::fs::copy(path, &bak).map_err(|e| format!("Couldn't back up {}: {e}", path.display()))?;
    }
    let tmp = PathBuf::from(format!("{}.cue-tmp", path.display()));
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| format!("Couldn't write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Couldn't replace {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("cue-mcp-{}-{}", std::process::id(), crate::model::now_ms() + rand_ish()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn rand_ish() -> u64 {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) * 7919
    }

    fn setup() -> (PathBuf, PathBuf, String, String) {
        let d = dir();
        let cwd = d.join("proj");
        let other = d.join("other");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let cwd_s = cwd.to_string_lossy().to_string();
        let other_s = other.to_string_lossy().to_string();
        let cfg = d.join("claude.json");
        std::fs::write(&cfg, serde_json::to_string_pretty(&json!({
            "numStartups": 5,
            "mcpServers": { "linear": { "type": "http", "url": "https://mcp.linear.app/mcp" } },
            "projects": {
                &cwd_s: {
                    "allowedTools": ["Bash"],
                    "mcpServers": { "local-one": { "command": "npx", "args": ["-y", "local-one"] } },
                    "disabledMcpjsonServers": ["proj-off"],
                },
                &other_s: {
                    "mcpServers": { "chrome-devtools": { "command": "npx", "args": ["chrome-devtools-mcp"] } },
                }
            },
            "tipsHistory": { "a": 1 }
        })).unwrap()).unwrap();
        std::fs::write(cwd.join(".mcp.json"), json!({ "mcpServers": { "proj-on": { "command": "p1" }, "proj-off": { "url": "http://x", "type": "sse" } } }).to_string()).unwrap();
        let t = d.join("t.jsonl");
        std::fs::write(&t, concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__linear__list_issues","input":{}}]}}"#, "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#, "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"mcp__linear__list_issues","input":{}}]}}"#, "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","is_error":true,"content":"boom"}]}}"#, "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t3","name":"mcp__claude_ai_Claude_Docs__read","input":{}}]}}"#, "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t3","content":"ok"}]}}"#, "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t4","name":"Bash","input":{}}]}}"#, "\n",
        )).unwrap();
        (cfg, t, cwd_s, other_s)
    }

    fn find<'a>(v: &'a Value, name: &str) -> &'a Value {
        v["servers"].as_array().unwrap().iter().find(|s| s["name"] == name).unwrap_or_else(|| panic!("no {name} in {v}"))
    }

    #[test]
    fn the_five_scopes() {
        let (cfg, t, cwd, _) = setup();
        let v = for_session_in(&cfg, &cwd, &t.to_string_lossy());
        assert_eq!(v["calls"], 3);
        let linear = find(&v, "linear");
        assert_eq!(linear["scope"], "user");
        assert_eq!(linear["toggle"], false);
        assert_eq!(linear["enabled"], true);
        assert_eq!(linear["transport"], "http");
        assert_eq!(linear["target"], "https://mcp.linear.app/mcp");
        assert_eq!(linear["used"], 2);
        assert_eq!(linear["failed"], 1);
        assert_eq!(linear["tools"], json!([{ "name": "list_issues", "calls": 2, "failed": 1 }]));
        let local = find(&v, "local-one");
        assert_eq!(local["scope"], "local");
        assert_eq!(local["toggle"], true);
        assert_eq!(local["transport"], "stdio");
        assert_eq!(local["target"], "npx -y local-one");
        assert_eq!(find(&v, "proj-on")["scope"], "project");
        assert_eq!(find(&v, "proj-on")["enabled"], true);
        assert_eq!(find(&v, "proj-off")["enabled"], false);
        assert_eq!(find(&v, "proj-off")["transport"], "sse");
        let avail = find(&v, "chrome-devtools");
        assert_eq!(avail["scope"], "available");
        assert_eq!(avail["enabled"], false);
        assert_eq!(avail["toggle"], true);
        let sess = find(&v, "claude_ai_Claude_Docs");
        assert_eq!(sess["scope"], "session");
        assert_eq!(sess["toggle"], false);
        assert_eq!(sess["used"], 1);
    }

    #[test]
    fn project_scope_moves_between_the_lists() {
        let (cfg, t, cwd, _) = setup();
        set_enabled_in(&cfg, &cwd, "proj-off", true).unwrap();
        let c = read_json(&cfg);
        let p = &c["projects"][&cwd];
        assert_eq!(p["disabledMcpjsonServers"], json!([]));
        assert_eq!(p["enabledMcpjsonServers"], json!(["proj-off"]));
        assert_eq!(find(&for_session_in(&cfg, &cwd, &t.to_string_lossy()), "proj-off")["enabled"], true);
        set_enabled_in(&cfg, &cwd, "proj-on", false).unwrap();
        let c = read_json(&cfg);
        assert_eq!(c["projects"][&cwd]["disabledMcpjsonServers"], json!(["proj-on"]));
        // Unrelated keys stay.
        assert_eq!(c["numStartups"], 5);
        assert_eq!(c["tipsHistory"]["a"], 1);
        assert_eq!(c["projects"][&cwd]["allowedTools"], json!(["Bash"]));
        assert!(cfg.with_extension("json.cue-bak").exists());
    }

    #[test]
    fn local_scope_round_trips_through_cue_mcp_off_and_copies_from_elsewhere() {
        let (cfg, t, cwd, _) = setup();
        let ts = t.to_string_lossy().to_string();
        set_enabled_in(&cfg, &cwd, "local-one", false).unwrap();
        let c = read_json(&cfg);
        assert!(c["projects"][&cwd]["mcpServers"].get("local-one").is_none());
        assert_eq!(c["projects"][&cwd][OFF_KEY]["local-one"]["command"], "npx");
        let v = for_session_in(&cfg, &cwd, &ts);
        let off = find(&v, "local-one");
        assert_eq!(off["scope"], "available");
        assert_eq!(off["enabled"], false);
        assert_eq!(off["target"], "npx -y local-one");
        set_enabled_in(&cfg, &cwd, "local-one", true).unwrap();
        let c = read_json(&cfg);
        assert_eq!(c["projects"][&cwd]["mcpServers"]["local-one"]["args"], json!(["-y", "local-one"]));
        assert!(c["projects"][&cwd].get(OFF_KEY).is_none());
        // From another folder.
        set_enabled_in(&cfg, &cwd, "chrome-devtools", true).unwrap();
        let v = for_session_in(&cfg, &cwd, &ts);
        assert_eq!(find(&v, "chrome-devtools")["scope"], "local");
        assert_eq!(find(&v, "chrome-devtools")["enabled"], true);
        assert!(set_enabled_in(&cfg, &cwd, "nobody", true).is_err());
        assert!(set_enabled_in(&cfg, &cwd, "nobody", false).is_err());
    }

    #[test]
    fn user_scope_cannot_be_toggled() {
        let (cfg, _, cwd, _) = setup();
        let before = std::fs::read_to_string(&cfg).unwrap();
        assert_eq!(set_enabled_in(&cfg, &cwd, "linear", false), Err("that server is on for every folder (user scope)".to_string()));
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), before);
    }

    #[test]
    fn all_lists_every_configured_server_with_its_folder() {
        let (cfg, _, cwd, other) = setup();
        let v = all_in(&cfg);
        let get = |n: &str| v.iter().find(|s| s["name"] == n).unwrap().clone();
        assert_eq!(get("linear")["project"], "");
        assert_eq!(get("local-one")["project"], cwd);
        assert_eq!(get("chrome-devtools")["project"], other);
        assert_eq!(get("proj-on")["scope"], "project");
        assert_eq!(get("proj-on")["project"], cwd);
    }

    #[test]
    fn definitions_first_found_wins() {
        let (cfg, _, cwd, other) = setup();
        let c = read_json(&cfg);
        let mut c2 = c.clone();
        // The same name in several places: user beats local beats .mcp.json beats elsewhere.
        c2["projects"][&cwd]["mcpServers"]["linear"] = json!({ "command": "local-linear" });
        c2["projects"][&cwd][OFF_KEY] = json!({ "proj-on": { "command": "saved" }, "saved-only": { "command": "s" } });
        c2["projects"][&other]["mcpServers"]["local-one"] = json!({ "command": "other-local" });
        std::fs::write(&cfg, c2.to_string()).unwrap();
        let d = definitions_in(&cfg, &cwd);
        assert_eq!(d["linear"]["url"], "https://mcp.linear.app/mcp");
        assert_eq!(d["local-one"]["command"], "npx");
        assert_eq!(d["proj-on"]["command"], "p1");
        assert_eq!(d["proj-off"]["type"], "sse");
        assert_eq!(d["saved-only"]["command"], "s");
        assert_eq!(d["chrome-devtools"]["args"], json!(["chrome-devtools-mcp"]));
        assert!(!d.contains_key("claude_ai_Claude_Docs"));
    }

    #[test]
    fn a_selection_is_written_and_marks_what_is_selected() {
        let (cfg, t, cwd, _) = setup();
        let d = definitions_in(&cfg, &cwd);
        let sel_dir = cfg.parent().unwrap().join("mcp");
        let path = selection_path_in(&sel_dir, "ab-12");
        assert_eq!(path, sel_dir.join("ab-12.json"));
        assert_eq!(selection_path_in(&sel_dir, "../x"), sel_dir.join("x.json"));
        assert_eq!(write_selection(&path, &d, &["nope".into(), "proj-on".into(), "zz".into()]), Err("Cue has no definition of nope, zz".to_string()));
        assert!(!path.exists());
        assert!(flags_for(&path).is_empty());
        assert_eq!(write_selection(&path, &d, &["chrome-devtools".into(), "proj-on".into()]), Ok(2));
        let f = read_json(&path);
        assert_eq!(f, json!({ "mcpServers": { "chrome-devtools": { "command": "npx", "args": ["chrome-devtools-mcp"] }, "proj-on": { "command": "p1" } } }));
        assert_eq!(flags_for(&path), vec!["--mcp-config".to_string(), path.to_string_lossy().to_string(), "--strict-mcp-config".to_string()]);
        // Not pinned: selected follows enabled.
        let v = with_selection(for_session_in(&cfg, &cwd, &t.to_string_lossy()), None, &path);
        assert_eq!(v["pinned"], false);
        assert_eq!(find(&v, "linear")["selected"], true);
        assert_eq!(find(&v, "chrome-devtools")["selected"], false);
        // Pinned: selected is the file's names; session-scope ones keep theirs and can't be toggled.
        let mut sel = read_selection(&path).unwrap();
        sel.insert("gone-now".into(), json!({ "url": "http://g" }));
        let v = with_selection(for_session_in(&cfg, &cwd, &t.to_string_lossy()), Some(sel), &path);
        assert_eq!(v["pinned"], true);
        assert_eq!(find(&v, "linear")["selected"], false);
        assert_eq!(find(&v, "local-one")["selected"], false);
        assert_eq!(find(&v, "proj-on")["selected"], true);
        assert_eq!(find(&v, "chrome-devtools")["selected"], true);
        assert_eq!(find(&v, "gone-now")["selected"], true);
        assert_eq!(find(&v, "claude_ai_Claude_Docs")["toggle"], false);
        // An empty selection: no servers at all, still pinned.
        write_selection(&path, &d, &[]).unwrap();
        assert_eq!(read_selection(&path).unwrap().len(), 0);
    }

    #[test]
    fn live_state_from_claude_codes_records() {
        let d = dir();
        let t = d.join("t.jsonl");
        let delta = |at: &str, added: &[&str], removed: &[&str], pending: &[&str], failed: &[&str]| {
            json!({ "type": "attachment", "timestamp": at, "attachment": { "type": "deferred_tools_delta", "addedNames": added, "removedNames": removed, "pendingMcpServers": pending, "failedMcpServers": failed } }).to_string()
        };
        let lines = [
            delta("2026-10-17T15:00:00.000Z", &["mcp__linear__list_issues", "mcp__claude_ai_Gmail__authenticate", "mcp__old__x", "Bash"], &[], &[], &[]),
            json!({ "type": "user", "timestamp": "2026-10-17T15:10:00.000Z", "message": { "content": "hi" } }).to_string(),
            delta("2026-10-17T15:10:00.100Z", &["mcp__robinhood__authenticate", "mcp__robinhood__complete_authentication"], &["mcp__old__x"], &["claude-in-chrome"], &["broken one"]),
        ];
        std::fs::write(&t, lines.join("\n")).unwrap();
        let path = t.to_string_lossy();
        let since = |iso: &str| crate::transcript::iso_ms(iso).unwrap();
        let (s, tools) = live_state(&path, since("2026-10-17T15:05:00.000Z")).unwrap();
        assert_eq!(s.get("linear"), Some(&"on"));
        assert_eq!(s.get("robinhood"), Some(&"sign-in"), "only its authenticate tools: waiting for you");
        assert_eq!(s.get("claude_ai_Gmail"), Some(&"sign-in"));
        assert_eq!(s.get("claude-in-chrome"), Some(&"connecting"));
        assert_eq!(s.get("broken_one"), Some(&"failed"), "named as its tools would be");
        assert_eq!(s.get("old"), None, "its tools were removed");
        assert_eq!(tools.get("linear"), Some(&vec!["list_issues".to_string()]));
        assert_eq!(tools.get("robinhood"), None, "sign-in tools aren't listed");
        assert!(live_state(&path, since("2026-10-17T16:00:00.000Z")).is_none(), "restarted since: nothing to tell yet");

        let v = with_live(json!({ "servers": [{ "name": "linear" }, { "name": "robinhood" }, { "name": "absent" }, { "name": "claude.ai Gmail" }] }), &path, Some(since("2026-10-17T15:05:00.000Z")));
        let live: Vec<&str> = v["servers"].as_array().unwrap().iter().map(|r| r["live"].as_str().unwrap()).collect();
        assert_eq!(live, ["on", "sign-in", "off", "sign-in"]);
        assert_eq!(v["servers"][0]["loaded"], json!(["list_issues"]));
        assert_eq!(v["live_known"], true);
        let v = with_live(json!({ "servers": [{ "name": "linear" }] }), &path, None);
        assert_eq!((v["running"].clone(), v["live_known"].clone(), v["servers"][0].get("live").is_none()), (json!(false), json!(false), true));

        // No records at all (tools loaded up front): can't be told.
        std::fs::write(&t, json!({ "type": "user", "timestamp": "2026-10-17T15:10:00.000Z" }).to_string()).unwrap();
        assert!(live_state(&path, 0).is_none());
    }
}
