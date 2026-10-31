//! config.json in Cue's data folder: every setting Cue has, shared with the Pi extension (which reads `pi.gate`).
//! Missing keys mean defaults; writes merge, so a setting never clobbers the others.

use serde_json::{json, Value};

pub mod login;

pub const HISTORY_DEFAULT: u64 = 300;

/// Your own Color (project.hue): { h: 0–360, k: 0–1 }, the hue and strength the window's colors are made from.
fn own_hue(v: &Value) -> bool {
    let n = |k: &str, max: f64| v[k].as_f64().is_some_and(|x| (0.0..=max).contains(&x));
    v.is_object() && n("h", 360.0) && n("k", 1.0)
}

fn path() -> std::path::PathBuf {
    crate::server::cue_dir().join("config.json")
}

pub fn load() -> Value {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

/// Every setting with its default filled in — what the Settings screen shows.
pub fn effective() -> Value {
    let c = load();
    let b = |p: &str, d: bool| c.pointer(p).and_then(Value::as_bool).unwrap_or(d);
    let (ins, snooze) = (b("/insights/on", false), c.pointer("/insights/snooze").and_then(Value::as_u64).unwrap_or(0));
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    // (flag() falls back to true, so defaults that are off must be listed here.)
    json!({
        "history": { "keep": history_keep() },
        "notify": { "finished": b("/notify/finished", true), "decisions": b("/notify/decisions", true), "cold": b("/notify/cold", true), "sound": crate::sounds::choice() },
        // Read from macOS, so it matches System Settings → Login Items.
        "login": { "open": login::enabled() },
        "tray": { "show": tray_shown() },
        // + New session runs it in Cue (tmux underneath; off: a terminal tab). Offered only when tmux is installed.
        "sessions": { "tmux": tmux_sessions(), "tmux_installed": crate::terminal::focus::tmux_installed(), "terminal": crate::terminal::focus::terminal_name() },
        "agents": { "show_driven": b("/agents/show_driven", false) },
        // The voice Cue reads out in: "" the Mac's own, else a natural one ("af_heart"), downloaded in Settings → Voice.
        "voice": { "pick": c.pointer("/voice/pick").and_then(Value::as_str).unwrap_or("") },
        // Insights (ui/insights.js): one switch for all of them, off until you turn it on. `snooze`: hidden until
        // then (ms), from a note's "Hide for a day / a week", which also holds off the hand-off before the cache
        // runs out. The queue note's "Got it" turns that one off alone.
        "insights": { "on": ins, "handoff": ins, "cache": ins, "queue": ins && b("/insights/queue", true), "ahead": ins && snooze <= now_ms, "snooze": snooze, "skills": c.pointer("/insights/skills").cloned().unwrap_or_else(|| json!({})) },
        // The first-launch "Connect your agents" screen, closed with Done.
        "setup": { "done": b("/setup/done", false) },
        "usage": { "file": usage_file(), "claude": b("/usage/claude", true) },
        "pi": { "gate": c.pointer("/pi/gate").and_then(Value::as_str).unwrap_or("dangerous") },
        "appearance": { "mode": c.pointer("/appearance/mode").and_then(Value::as_str).unwrap_or("system"), "zoom": zoom(), "project": c.pointer("/appearance/project").and_then(Value::as_bool).unwrap_or(false) },
        "context": { "keep": context_keep() },
        "turn": { "mode": turn_mode() },
        "quick": { "phrases": quick_phrases() },
        // Settings → Actions: what "Ask the agent to set up actions" puts in the session's box.
        "actions": { "prompt": crate::actions::prompt(), "default": crate::actions::PROMPT },
        "steps": { "mode": c.pointer("/steps/mode").and_then(Value::as_str).unwrap_or("line") },
        // A project's dot color, by folder name: the swatch you picked (0–7); unlisted folders hash to one.
        "project": { "colors": c.pointer("/project/colors").cloned().filter(Value::is_object).unwrap_or_else(|| json!({})), "all": c.pointer("/project/all").and_then(Value::as_u64).filter(|n| *n < 10), "hue": c.pointer("/project/hue").cloned().filter(own_hue), "pins": c.pointer("/project/pins").cloned().filter(Value::is_array).unwrap_or_else(|| json!([])), "names": c.pointer("/project/names").cloned().filter(Value::is_object).unwrap_or_else(|| json!({})), "moves": c.pointer("/project/moves").cloned().filter(Value::is_object).unwrap_or_else(|| json!({})), "folders": c.pointer("/project/folders").cloned().filter(Value::is_object).unwrap_or_else(|| json!({})), "made": c.pointer("/project/made").cloned().filter(Value::is_array).unwrap_or_else(|| json!([])) },
    })
}

/// Projects pinned beside All projects in the header, up to this many.
pub const PINS_MAX: usize = 3;
/// Quick phrases: chips above a session's text box (in the window) that send
/// their text, after anything typed in the box. Yours, from Settings, up to `QUICK_MAX`.
pub const QUICK_MAX: usize = 4;
/// The projects pinned beside All projects: names, each once, at most `PINS_MAX`.
fn pins(value: &Value) -> Result<Value, String> {
    let mut pins: Vec<&str> = Vec::new();
    for p in value.as_array().ok_or("pins must be a list")?.iter().filter_map(Value::as_str).filter(|p| !p.is_empty()) {
        if !pins.contains(&p) {
            pins.push(p);
        }
    }
    if pins.len() > PINS_MAX {
        return Err(format!("at most {PINS_MAX} pinned projects"));
    }
    Ok(json!(pins))
}
/// Projects you made in the projects list (+ New project…), kept while empty: names, trimmed, each once.
pub const MADE_MAX: usize = 50;
fn made(value: &Value) -> Result<Value, String> {
    let mut made: Vec<&str> = Vec::new();
    for p in value.as_array().ok_or("made must be a list")?.iter().filter_map(Value::as_str).map(str::trim).filter(|p| !p.is_empty()) {
        if p.chars().count() > 40 {
            return Err("a project's name is 40 characters at most".into());
        }
        if !made.contains(&p) {
            made.push(p);
        }
    }
    if made.len() > MADE_MAX {
        return Err(format!("at most {MADE_MAX} projects made here"));
    }
    Ok(json!(made))
}
/// The names you gave projects in Cue (project: name), shown in place of the folder's or repository's own.
/// Each one trimmed, its spaces single, at most `NAME_MAX` characters; an empty one is dropped (the folder's
/// name again).
const NAME_MAX: usize = 40;
fn names(value: &Value) -> Result<Value, String> {
    let mut out = serde_json::Map::new();
    for (p, n) in value.as_object().ok_or("names must be project: name")? {
        let n = n.as_str().ok_or("a name must be text")?.split_whitespace().collect::<Vec<_>>().join(" ");
        if !n.is_empty() && n != *p {
            out.insert(p.clone(), json!(n.chars().take(NAME_MAX).collect::<String>()));
        }
    }
    Ok(Value::Object(out))
}
pub fn quick_phrases() -> Vec<String> {
    match load().pointer("/quick/phrases").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(Value::as_str).map(String::from).collect(),
        None => ["Commit", "Go ahead", "Think hard"].map(String::from).to_vec(),
    }
}

/// Where your own usage meters are read from (Settings → Usage file).
pub const USAGE_FILE_DEFAULT: &str = "~/.cue/usage.csv";
pub fn usage_file() -> String {
    load().pointer("/usage/file").and_then(Value::as_str).filter(|p| !p.trim().is_empty()).unwrap_or(USAGE_FILE_DEFAULT).to_string()
}

pub fn flag(pointer: &str) -> bool {
    effective().pointer(pointer).and_then(Value::as_bool).unwrap_or(true)
}

/// Whether Claude Code itself compacts a session of `tokens` context left idle, just before its hour's cache
/// runs out (idle compaction, since 2.1.299: on unless its settings say `"idleCompaction": false`, from
/// CLAUDE_CODE_IDLE_COMPACT_MIN_TOKENS, 200k by default and never under 100k). Such a session needs no hand-off
/// note from Cue ahead of its cache: its next message sends Claude Code's summary.
/// SHORTCUT: doesn't check Claude Code's version (2.1.290 and up); an older one gets no note ahead on big
/// sessions. Upgrade path: read the version its logs name, or `claude --version` once a day.
pub fn idle_compacts(tokens: u64) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let settings = std::fs::read_to_string(format!("{home}/.claude/settings.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    idle_compacts_in(settings.as_ref(), std::env::var("CLAUDE_CODE_IDLE_COMPACT_MIN_TOKENS").ok().as_deref(), tokens)
}
/// idle_compacts, given Claude Code's settings and its CLAUDE_CODE_IDLE_COMPACT_MIN_TOKENS.
fn idle_compacts_in(settings: Option<&Value>, min_env: Option<&str>, tokens: u64) -> bool {
    if settings.and_then(|v| v.pointer("/idleCompaction")).and_then(Value::as_bool) == Some(false) {
        return false;
    }
    let min = min_env.and_then(|v| v.parse::<u64>().ok()).map_or(200_000, |n| n.max(100_000));
    tokens >= min
}

/// New sessions run in Cue (in tmux, shown in Cue's own terminal) unless Settings says a terminal tab;
/// only when tmux is installed.
pub fn tmux_sessions() -> bool {
    load().pointer("/sessions/tmux").and_then(Value::as_bool).unwrap_or(true) && crate::terminal::focus::tmux_installed()
}

/// What a finished card shows when a Stop hook made the agent continue: "answer" | "last".
pub fn turn_mode() -> String {
    load().pointer("/turn/mode").and_then(Value::as_str).filter(|m| *m == "last").unwrap_or("answer").to_string()
}

/// Text size: the window's zoom, one of these steps (⌘+ / ⌘− / ⌘0, or Settings → Text size).
pub const ZOOM_STEPS: [f64; 4] = [0.9, 1.0, 1.15, 1.3];

/// The text size you picked (1.0 until you pick one; anything not a step reads as 1.0).
pub fn zoom() -> f64 {
    load().pointer("/appearance/zoom").and_then(Value::as_f64).and_then(zoom_step).unwrap_or(1.0)
}
fn zoom_step(z: f64) -> Option<f64> {
    ZOOM_STEPS.into_iter().find(|s| (s - z).abs() < 0.001)
}

/// How many earlier exchanges each session keeps for context (Settings → Context; default 5).
pub fn context_keep() -> usize {
    load().pointer("/context/keep").and_then(Value::as_u64).unwrap_or(5).clamp(1, 50) as usize
}

/// Sessions moved to another project in Cue (session id: project), shown under it in place of their folder's.
/// The agent stays in its folder. Each project trimmed; an empty one is dropped (its folder's again).
const MOVES_MAX: usize = 500;
fn moves(value: &Value) -> Result<Value, String> {
    let mut out = serde_json::Map::new();
    for (sid, p) in value.as_object().ok_or("moves must be session: project")? {
        let p = p.as_str().ok_or("a project must be text")?.trim();
        if !sid.is_empty() && !p.is_empty() {
            out.insert(sid.clone(), json!(p));
        }
    }
    if out.len() > MOVES_MAX {
        return Err(format!("at most {MOVES_MAX} moved sessions"));
    }
    Ok(Value::Object(out))
}
/// Folders put in a project in Cue (+ New's Project, or Move to project): "<folder>" on this Mac,
/// "<machine>:<folder>" on another, to the project. Every session in that folder or under it, now and
/// later, shows under it (model.rs project_at). The agent stays in its folder.
const FOLDERS_MAX: usize = 500;
fn folders(value: &Value) -> Result<Value, String> {
    let mut out = serde_json::Map::new();
    for (dir, p) in value.as_object().ok_or("folders must be folder: project")? {
        let p = p.as_str().ok_or("a project must be text")?.trim();
        let dir = dir.trim_end_matches('/');
        let path = dir.split_once(':').map_or(dir, |(_, f)| f);
        if !path.starts_with('/') && !path.starts_with('~') {
            return Err(format!("“{dir}” isn't a folder"));
        }
        if !p.is_empty() {
            out.insert(dir.to_string(), json!(p));
        }
    }
    if out.len() > FOLDERS_MAX {
        return Err(format!("at most {FOLDERS_MAX} folders"));
    }
    Ok(Value::Object(out))
}
/// The folders put in a project: folder (or "<machine>:<folder>") → the project.
pub fn folders_put() -> std::collections::HashMap<String, String> {
    load().pointer("/project/folders").and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|p| (k.clone(), p.to_string()))).collect())
        .unwrap_or_default()
}
/// The moved sessions: session id → the project it shows under.
pub fn moved() -> std::collections::HashMap<String, String> {
    load().pointer("/project/moves").and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|p| (k.clone(), p.to_string()))).collect())
        .unwrap_or_default()
}

pub fn history_keep() -> usize {
    load().pointer("/history/keep").and_then(Value::as_u64).unwrap_or(HISTORY_DEFAULT).max(10) as usize
}

/// Set one dotted key ("history.keep", "notify.finished", "pi.gate"), keeping everything else.
pub fn set(key: &str, value: Value) -> Result<(), String> {
    let allowed = ["history.keep", "notify.finished", "notify.decisions", "notify.cold", "notify.sound", "pi.gate", "appearance.mode", "appearance.zoom", "appearance.project", "context.keep", "turn.mode", "agents.show_driven", "login.open", "tray.show", "quick.phrases", "steps.mode", "setup.done", "usage.file", "usage.claude", "sessions.tmux", "project.colors", "project.all", "project.hue", "project.pins", "project.names", "project.moves", "project.folders", "project.made", "insights.on", "insights.queue", "insights.snooze", "insights.skills", "actions.prompt", "voice.pick"];
    if !allowed.contains(&key) {
        return Err(format!("unknown setting {key}"));
    }
    let value = match key {
        "history.keep" => json!(value.as_u64().ok_or("keep must be a number")?.max(10)),
        "sessions.tmux" => json!(value.as_bool().ok_or("tmux must be on or off")?),
        "context.keep" => json!(value.as_u64().ok_or("keep must be a number")?.clamp(1, 50)),
        "pi.gate" => match value.as_str() {
            Some(g @ ("dangerous" | "all" | "off")) => json!(g),
            _ => return Err("gate must be dangerous, all or off".into()),
        },
        "appearance.mode" => match value.as_str() {
            Some(m @ ("system" | "light" | "dark")) => json!(m),
            _ => return Err("appearance must be system, light or dark".into()),
        },
        "notify.sound" => match value.as_str() {
            Some(c) if crate::sounds::CHOICES.contains(&c) => json!(c),
            _ => return Err(format!("the sound must be one of {}", crate::sounds::CHOICES.join(", "))),
        },
        "appearance.zoom" => json!(value.as_f64().and_then(zoom_step).ok_or("text size must be 0.9, 1, 1.15 or 1.3")?),
        "turn.mode" => match value.as_str() {
            Some(m @ ("answer" | "last")) => json!(m),
            _ => return Err("turn mode must be answer or last".into()),
        },
        // Your usage meters' CSV file (a path; ~ is your home folder). Empty: back to the default.
        "usage.file" => {
            let p = value.as_str().ok_or("the usage file must be a path")?.trim();
            json!(if p.is_empty() { USAGE_FILE_DEFAULT } else { p })
        }
        // Steps in the chat: one line each (open one with a tap), everything open, or hidden.
        "steps.mode" => match value.as_str() {
            Some(m @ ("line" | "all" | "hidden")) => json!(m),
            _ => return Err("steps must be line, all or hidden".into()),
        },
        "project.colors" => {
            let map = value.as_object().ok_or("colors must be folder: swatch")?;
            if !map.values().all(|v| v.as_u64().is_some_and(|n| n < 10)) {
                return Err("a swatch is 0 to 9".into());
            }
            value
        }
        // The skill that hands a session off, per machine and agent ("build-box|claude", "|codex" for this
        // Mac): one command name each, as the session's "/" menu lists it.
        "insights.skills" => {
            let map = value.as_object().ok_or("skills must be machine|agent: skill")?;
            let one_name = |v: &Value| v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 120 && !s.chars().any(char::is_whitespace));
            if map.len() > 200 || !map.iter().all(|(k, v)| k.len() <= 200 && k.contains('|') && one_name(v)) {
                return Err("each hand-off skill is one command name".into());
            }
            value
        }
        // Insights hidden until this time (ms since 1970); 0 shows them again.
        "insights.snooze" => json!(value.as_u64().ok_or("snooze must be a time")?),
        "voice.pick" => match value.as_str() {
            Some(v) if v.len() <= 24 && v.chars().all(|c| c.is_ascii_lowercase() || c == '_') => json!(v),
            _ => return Err("the voice must be one of Kokoro's names, or empty for the Mac's own".into()),
        },
        // Your actions prompt ("" is Cue's own again).
        "actions.prompt" => return crate::actions::set_prompt(value.as_str().ok_or("the prompt must be text")?).map(|_| ()),
        "project.all" => json!(value.as_u64().filter(|n| *n < 10).ok_or("a swatch is 0 to 9")?),
        // Your own Color, past the ten: a hue and a strength (null: one of the ten, project.all).
        "project.hue" if value.is_null() || own_hue(&value) => value,
        "project.hue" => return Err("your own color is a hue (0 to 360) and a strength (0 to 1)".into()),
        "project.pins" => pins(&value)?,
        "project.made" => made(&value)?,
        "project.names" => names(&value)?,
        "project.moves" => moves(&value)?,
        "project.folders" => folders(&value)?,
        "quick.phrases" => {
            let list = value.as_array().ok_or("phrases must be a list")?;
            let phrases: Vec<String> = list.iter().filter_map(Value::as_str).map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|p| !p.is_empty()).collect();
            if phrases.len() > QUICK_MAX {
                return Err(format!("at most {QUICK_MAX} phrases"));
            }
            if phrases.iter().any(|p| p.chars().count() > 40) {
                return Err("keep each phrase to 40 characters".into());
            }
            json!(phrases)
        }
        "login.open" => {
            let on = value.as_bool().ok_or("must be true or false")?;
            login::set(on)?;
            json!(on)
        }
        _ => json!(value.as_bool().ok_or("must be true or false")?),
    };
    remember(key, value)
}

/// Write one dotted key as-is (no checks): for values Cue sets itself.
pub fn remember(key: &str, value: Value) -> Result<(), String> {
    let mut c = load();
    let (section, name) = key.split_once('.').unwrap();
    if !c[section].is_object() {
        c[section] = json!({});
    }
    c[section][name] = value;
    std::fs::create_dir_all(crate::server::cue_dir()).map_err(|e| e.to_string())?;
    std::fs::write(path(), serde_json::to_string_pretty(&c).unwrap() + "\n").map_err(|e| e.to_string())
}

/// Is Cue actually connected? Checked live so the Settings screen can say why nothing shows up.
pub fn connections() -> Value {
    let home = std::env::var("HOME").unwrap_or_default();
    let hooked = |file: &str| -> Vec<&'static str> {
        let cfg = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
        ["PermissionRequest", "Stop", "UserPromptSubmit", "SessionEnd"]
            .into_iter()
            .filter(|e| {
                cfg.as_ref()
                    .and_then(|v| v.pointer(&format!("/hooks/{e}")))
                    .is_some_and(|g| g.to_string().contains("cue-hook") || g.to_string().contains("cue-claude-hook"))
            })
            .collect()
    };
    let claude = hooked(&format!("{home}/.claude/settings.json"));
    let claude_present = std::path::Path::new(&format!("{home}/.claude")).exists();
    let codex = hooked(&format!("{home}/.codex/hooks.json"));
    let codex_present = std::path::Path::new(&format!("{home}/.codex")).exists();
    let pi_ext = std::path::Path::new(&format!("{home}/.pi/agent/extensions/cue.ts")).exists();
    let pi_present = std::path::Path::new(&format!("{home}/.pi")).exists();
    json!({
        "claude": { "events": claude, "ok": claude.len() == 4, "present": claude_present },
        "codex": { "events": codex, "ok": codex.len() == 4, "present": codex_present },
        "pi": { "ok": pi_ext, "present": pi_present },
    })
}

/// Settings → Connect (what install.sh does, for an app downloaded on its own): Claude Code and Codex
/// get Cue's hooks in their settings file (backed up first; their other hooks and settings stay as
/// they are), Pi gets Cue's extension (`pi_ext`: the copy bundled in the app).
pub fn connect(harness: &str, pi_ext: &std::path::Path) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|_| "no home folder".to_string())?;
    crate::hook::write_shim();
    let hook = crate::server::cue_dir().join("bin/cue-hook");
    match harness {
        "claude" => {
            let settings = format!("{home}/.claude/settings.json");
            add_hooks(std::path::Path::new(&settings), &hook.to_string_lossy(), "claude")?;
            add_status_line(std::path::Path::new(&settings))?;
            Ok("Connected Claude Code. New sessions show up in Cue; in one already open, type /hooks once (or restart it with claude --resume).".into())
        }
        "codex" => {
            add_hooks(std::path::Path::new(&format!("{home}/.codex/hooks.json")), &hook.to_string_lossy(), "codex")?;
            Ok("Connected Codex. In Codex, run /hooks once to trust them.".into())
        }
        "pi" => {
            let dir = std::path::PathBuf::from(format!("{home}/.pi/agent/extensions"));
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::copy(pi_ext, dir.join("cue.ts")).map_err(|e| format!("Couldn't add Cue's Pi extension: {e}"))?;
            Ok("Connected Pi. New Pi sessions load Cue's extension.".into())
        }
        _ => Err(format!("Cue doesn't know how to connect {harness}")),
    }
}

/// Cue's hook entries in an agent's settings file: earlier Cue entries replaced, everything else kept.
fn add_hooks(path: &std::path::Path, hook: &str, harness: &str) -> Result<(), String> {
    let cfg: Value = match std::fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str(&text).map_err(|e| format!("{} isn't valid JSON ({e}). Fix it, then connect again.", path.display()))?,
        _ => json!({}),
    };
    if path.exists() {
        let backup = format!("{}.cue-backup-{}", path.display(), crate::model::now_ms());
        std::fs::copy(path, &backup).map_err(|e| format!("Couldn't back up {}: {e}", path.display()))?;
    }
    let cfg = merge_hooks(cfg, hook, harness).map_err(|e| format!("{e} in {}", path.display()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&cfg).unwrap_or_default() + "\n").map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

/// An agent's settings with Cue's hook entries in: earlier Cue entries replaced, everything else kept.
/// (No files: Connect writes the result here, + New → Machine on a machine.)
pub fn merge_hooks(mut cfg: Value, hook: &str, harness: &str) -> Result<Value, String> {
    if !cfg.is_object() {
        return Err("the settings aren't an object".into());
    }
    let hooks = cfg.as_object_mut().unwrap().entry("hooks").or_insert_with(|| json!({}));
    let Some(events) = hooks.as_object_mut() else { return Err("\"hooks\" isn't an object".into()) };
    let ours = |cmd: &str| cmd.contains("cue-hook") || cmd.contains("cue-claude-hook");
    for groups in events.values_mut() {
        if let Some(list) = groups.as_array_mut() {
            for g in list.iter_mut() {
                if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                    hs.retain(|h| !h.get("command").and_then(Value::as_str).is_some_and(ours));
                }
            }
            list.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|hs| !hs.is_empty()));
        }
    }
    events.retain(|_, g| g.as_array().is_none_or(|l| !l.is_empty()));
    for (event, group) in wanted(hook, harness) {
        let list = events.entry(event).or_insert_with(|| json!([]));
        if let Some(l) = list.as_array_mut() {
            l.push(group);
        }
    }
    Ok(cfg)
}

/// Cue's hook entries for one agent: event name, and the group that goes under it.
fn wanted(hook: &str, harness: &str) -> Vec<(&'static str, Value)> {
    // Claude Code waits as long as you take; Codex caps hooks (600 s by default).
    let wait = if harness == "claude" { 86400 } else { 3600 };
    // Quoted: the path has a space in it ("Application Support").
    let cmd = |what: &str| format!("\"{hook}\" {what} {harness}");
    let mut want = vec![
        ("PermissionRequest", json!({ "matcher": "*", "hooks": [{ "type": "command", "command": cmd("permission"), "timeout": wait }] })),
        ("Stop", json!({ "hooks": [{ "type": "command", "command": cmd("stop"), "timeout": 5 }] })),
        ("UserPromptSubmit", json!({ "hooks": [{ "type": "command", "command": cmd("prompt"), "timeout": 5 }] })),
        ("SessionEnd", json!({ "hooks": [{ "type": "command", "command": cmd("end"), "timeout": 3 }] })),
    ];
    if harness == "claude" {
        // A turn that ends on an API error (a usage limit) fires this, not Stop.
        want.push(("StopFailure", json!({ "hooks": [{ "type": "command", "command": cmd("failure"), "timeout": 5 }] })));
        // Compacting (typed /compact, or its context filled up): not waiting on you until it's done.
        want.push(("PreCompact", json!({ "hooks": [{ "type": "command", "command": cmd("compact"), "timeout": 5 }] })));
        // A permission prompt waiting in its terminal that Cue can't answer (a sandboxed command's network access).
        want.push(("Notification", json!({ "matcher": "permission_prompt", "hooks": [{ "type": "command", "command": cmd("notice"), "timeout": 5 }] })));
    }
    want
}

/// Claude Code's settings with Cue's status line (`cmd`) in, and the command it replaces (Cue's runs it
/// from there, unchanged): None when Cue's is in already (the one saved then is kept). Here and on a machine.
pub fn wrap_status_line(cfg: &Value, cmd: &str) -> Option<(Value, String)> {
    let line = cfg.get("statusLine").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let was = line.get("command").and_then(Value::as_str).unwrap_or("").to_string();
    if was.contains("cue-statusline") {
        return None;
    }
    let mut line = line;
    line["type"] = json!("command");
    line["command"] = json!(cmd);
    let mut cfg = cfg.clone();
    cfg["statusLine"] = line;
    Some((cfg, was))
}

/// Cue's status line on this Mac (bin/cue-statusline, written at launch): Claude Code tells only its status
/// line a session's context window and your plan's usage, so Cue's passes them on and runs yours. Your
/// command is saved to statusline.orig first, the settings file backed up as on Connect.
fn add_status_line(path: &std::path::Path) -> Result<(), String> {
    let Some(cfg) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).filter(Value::is_object) else { return Ok(()) };
    let cmd = format!("\"{}\"", crate::server::cue_dir().join("bin/cue-statusline").display());
    let Some((cfg, was)) = wrap_status_line(&cfg, &cmd) else { return Ok(()) };
    std::fs::write(crate::server::cue_dir().join("statusline.orig"), &was).map_err(|e| format!("Couldn't save your status line: {e}"))?;
    let backup = format!("{}.cue-backup-{}", path.display(), crate::model::now_ms());
    std::fs::copy(path, &backup).map_err(|e| format!("Couldn't back up {}: {e}", path.display()))?;
    std::fs::write(path, serde_json::to_string_pretty(&cfg).unwrap_or_default() + "\n").map_err(|e| format!("Couldn't write {}: {e}", path.display()))
}

/// At launch: an agent you connected with an older Cue gets the hooks this one added (once; its
/// settings file is backed up as on Connect). One you never connected is left alone.
pub fn refresh_hooks() {
    let home = std::env::var("HOME").unwrap_or_default();
    let hook = crate::server::cue_dir().join("bin/cue-hook").to_string_lossy().to_string();
    for (harness, file) in [("claude", format!("{home}/.claude/settings.json")), ("codex", format!("{home}/.codex/hooks.json"))] {
        let Some(cfg) = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
        let ours = |e: &str| cfg.pointer(&format!("/hooks/{e}")).is_some_and(|g| g.to_string().contains("cue-hook"));
        let want = wanted(&hook, harness);
        if want.iter().any(|(e, _)| ours(e)) && !want.iter().all(|(e, _)| ours(e)) {
            if let Err(e) = add_hooks(std::path::Path::new(&file), &hook, harness) {
                eprintln!("cue: couldn't add new hooks for {harness}: {e}");
            }
        }
        // Connected Claude Code: Cue's status line too (once; yours keeps running from it).
        if harness == "claude" && want.iter().any(|(e, _)| ours(e)) {
            if let Err(e) = add_status_line(std::path::Path::new(&file)) {
                eprintln!("cue: couldn't add Cue's status line: {e}");
            }
        }
    }
}

/// The menu bar icon. On unless you hide it: Cue stays in the Dock either way.
pub fn tray_shown() -> bool {
    load().pointer("/tray/show").and_then(Value::as_bool).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    #[test]
    fn idle_compaction_covers_big_idle_sessions_unless_turned_off() {
        use super::idle_compacts_in;
        let off = serde_json::json!({ "idleCompaction": false });
        let on = serde_json::json!({});
        assert!(idle_compacts_in(Some(&on), None, 200_000), "200k and up by default");
        assert!(!idle_compacts_in(Some(&on), None, 199_000));
        assert!(!idle_compacts_in(None, None, 150_000), "no settings file: Claude Code's default");
        assert!(idle_compacts_in(None, Some("120000"), 120_000), "the variable lowers it");
        assert!(!idle_compacts_in(None, Some("50000"), 90_000), "never under 100k");
        assert!(!idle_compacts_in(Some(&off), None, 500_000), "turned off: Cue's note ahead is back");
    }

    use super::*;

    #[test]
    fn text_size_takes_only_its_steps() {
        for z in ZOOM_STEPS {
            assert_eq!(zoom_step(z), Some(z));
        }
        // A number that's nearly a step (as JSON may round it) is that step; anything else isn't one.
        assert_eq!(zoom_step(1.1500000001), Some(1.15));
        for z in [0.0, 0.5, 1.2, 2.0, -1.0, f64::NAN] {
            assert_eq!(zoom_step(z), None, "{z}");
        }
    }

    #[test]
    fn projects_made_are_trimmed_names_each_once() {
        assert_eq!(made(&json!([" checkout ", "", "login", "checkout", 7])), Ok(json!(["checkout", "login"])));
        assert!(made(&json!(["x".repeat(41)])).is_err());
        assert!(made(&json!((0..=MADE_MAX).map(|n| n.to_string()).collect::<Vec<_>>())).is_err());
        assert!(made(&json!("checkout")).is_err());
    }

    #[test]
    fn pinned_projects_are_names_each_once_and_three_at_most() {
        assert_eq!(pins(&json!(["cue", "", "web", "cue", 7])), Ok(json!(["cue", "web"])));
        assert_eq!(pins(&json!(["a", "b", "c"])), Ok(json!(["a", "b", "c"])));
        assert!(pins(&json!(["a", "b", "c", "d"])).is_err());
        assert!(pins(&json!("cue")).is_err());
    }

    #[test]
    fn a_projects_name_is_tidied_and_an_empty_one_means_its_own_again() {
        assert_eq!(names(&json!({ "cue": "  My   app ", "web": "", "api": "api" })), Ok(json!({ "cue": "My app" })));
        assert_eq!(names(&json!({ "x": "a".repeat(50) })), Ok(json!({ "x": "a".repeat(40) })));
        assert!(names(&json!({ "x": 3 })).is_err());
        assert!(names(&json!(["x"])).is_err());
    }

    #[test]
    fn a_moved_session_names_its_project_and_an_empty_one_goes_back() {
        assert_eq!(moves(&json!({ "s1": "  web ", "s2": "", "": "x" })), Ok(json!({ "s1": "web" })));
        assert!(moves(&json!({ "s1": 3 })).is_err());
        assert!(moves(&json!(["s1"])).is_err());
    }

    #[test]
    fn connect_keeps_your_settings_and_hooks_and_replaces_only_cues() {
        let dir = std::env::temp_dir().join(format!("cue-connect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{
  "model": "opus",
  "hooks": {
    "Stop": [{ "hooks": [{ "type": "command", "command": "my-own-check" }] }, { "hooks": [{ "type": "command", "command": "\"/old/bin/cue-hook\" stop claude" }] }],
    "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "guard" }] }]
  },
  "theme": "dark"
}"#).unwrap();
        add_hooks(&path, "/new/bin/cue-hook", "claude").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        // Your keys stay in their order; your own hooks stay; the old Cue entry is replaced.
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "hooks", "theme"]);
        let stop = v["hooks"]["Stop"].to_string();
        assert!(stop.contains("my-own-check") && stop.contains("/new/bin/cue-hook") && !stop.contains("/old/bin"));
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "guard");
        for e in ["PermissionRequest", "UserPromptSubmit", "SessionEnd", "StopFailure"] {
            assert!(v["hooks"][e].to_string().contains("/new/bin/cue-hook"), "{e}");
        }
        // Connecting again doesn't add a second copy.
        add_hooks(&path, "/new/bin/cue-hook", "claude").unwrap();
        let again: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(again["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert!(std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).any(|e| e.file_name().to_string_lossy().contains(".cue-backup-")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
