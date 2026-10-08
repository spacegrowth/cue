//! config.json in Cue's data folder: every setting Cue has, shared with the Pi extension (which reads `pi.gate`).
//! Missing keys mean defaults; writes merge, so a setting never clobbers the others.

use serde_json::{json, Value};

pub mod login;

pub const HISTORY_DEFAULT: u64 = 300;

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
    // (flag() falls back to true, so defaults that are off must be listed here.)
    json!({
        "history": { "keep": history_keep() },
        "notify": { "finished": b("/notify/finished", true), "decisions": b("/notify/decisions", true) },
        // Read from macOS, so it matches System Settings → Login Items.
        "login": { "open": login::enabled() },
        "tray": { "show": tray_shown() },
        // + New session runs it in Cue (tmux underneath; off: a terminal tab). Offered only when tmux is installed.
        "sessions": { "tmux": tmux_sessions(), "tmux_installed": crate::focus::tmux_installed(), "terminal": crate::focus::terminal_name() },
        "agents": { "show_driven": b("/agents/show_driven", false) },
        // The first-launch "Connect your agents" screen, closed with Done.
        "setup": { "done": b("/setup/done", false) },
        "usage": { "file": usage_file(), "claude": b("/usage/claude", true) },
        "pi": { "gate": c.pointer("/pi/gate").and_then(Value::as_str).unwrap_or("dangerous") },
        "appearance": { "mode": c.pointer("/appearance/mode").and_then(Value::as_str).unwrap_or("system") },
        "context": { "keep": context_keep() },
        "turn": { "mode": turn_mode() },
        "quick": { "phrases": quick_phrases() },
        "steps": { "mode": c.pointer("/steps/mode").and_then(Value::as_str).unwrap_or("line") },
    })
}

/// Quick phrases: chips above a session's text box (in the window) that send
/// their text, after anything typed in the box. Yours, from Settings, up to `QUICK_MAX`.
pub const QUICK_MAX: usize = 4;
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

/// New sessions run in Cue (in tmux, shown in Cue's own terminal) unless Settings says a terminal tab;
/// only when tmux is installed.
pub fn tmux_sessions() -> bool {
    load().pointer("/sessions/tmux").and_then(Value::as_bool).unwrap_or(true) && crate::focus::tmux_installed()
}

/// What a finished card shows when a Stop hook made the agent continue: "answer" | "last".
pub fn turn_mode() -> String {
    load().pointer("/turn/mode").and_then(Value::as_str).filter(|m| *m == "last").unwrap_or("answer").to_string()
}

/// How many earlier exchanges each session keeps for context (Settings → Context; default 5).
pub fn context_keep() -> usize {
    load().pointer("/context/keep").and_then(Value::as_u64).unwrap_or(5).clamp(1, 50) as usize
}

pub fn history_keep() -> usize {
    load().pointer("/history/keep").and_then(Value::as_u64).unwrap_or(HISTORY_DEFAULT).max(10) as usize
}

/// Set one dotted key ("history.keep", "notify.finished", "pi.gate"), keeping everything else.
pub fn set(key: &str, value: Value) -> Result<(), String> {
    let allowed = ["history.keep", "notify.finished", "notify.decisions", "pi.gate", "appearance.mode", "context.keep", "turn.mode", "agents.show_driven", "login.open", "tray.show", "quick.phrases", "steps.mode", "setup.done", "usage.file", "usage.claude", "sessions.tmux"];
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
            add_hooks(std::path::Path::new(&format!("{home}/.claude/settings.json")), &hook.to_string_lossy(), "claude")?;
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
    }
}

/// The menu bar icon. On unless you hide it: Cue stays in the Dock either way.
pub fn tray_shown() -> bool {
    load().pointer("/tray/show").and_then(Value::as_bool).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

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
