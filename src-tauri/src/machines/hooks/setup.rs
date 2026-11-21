//! Setting Cue's hooks up on a machine, and keeping its hook up to date.

use super::scripts::{wrap_status_line, SCRIPT, STATUS};
use crate::machines::{self, Machine};
use serde_json::{json, Value};

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
        let mut merged = settings_with_hooks(cfg.clone(), harness)?;
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

/// An agent's settings on a machine with Cue's hook entries in (any older ones of Cue's replaced, yours kept): the
/// machine's set (config::merge_hooks), and Ask me before's check of each tool call before it runs (PreToolUse,
/// `cue-hook guard`), as on this Mac. Claude Code runs its commands through Bash (an edit is only checked on this
/// Mac, where a Desk item's copies are); Codex names its shell tools in more ways, and deletes with a patch.
pub(super) fn settings_with_hooks(cfg: Value, harness: &str) -> Result<Value, String> {
    let mut merged = crate::config::merge_hooks(cfg, "$HOME/.cue/cue-hook", harness, false)?;
    // Claude Code waits as long as you take; Codex caps hooks (600 s by default).
    let wait = if harness == "claude" { 86400 } else { 3600 };
    let matcher = if harness == "claude" { "Bash" } else { "*" };
    let entry = json!({ "matcher": matcher, "hooks": [{ "type": "command", "command": format!("\"$HOME/.cue/cue-hook\" guard {harness}"), "timeout": wait }] });
    match merged["hooks"].get_mut("PreToolUse").and_then(Value::as_array_mut) {
        Some(list) => list.push(entry),
        None => merged["hooks"]["PreToolUse"] = json!([entry]),
    }
    Ok(merged)
}

/// The hook on the machine brought up to this Cue's, when it was set up there and has changed since
/// (a fix in the script reaches the machine with the next connection, no setup to run again). Not set
/// up: nothing is put there. Can't be checked (the line dropped): left as it is.
pub(super) fn refresh_hook(m: &Machine) {
    let script = r#"f="$HOME/.cue/cue-hook"; t="$f.new"; cat >"$t" || exit 0; if [ -x "$f" ] && ! cmp -s "$t" "$f"; then chmod 755 "$t" && mv -f "$t" "$f" && echo updated; else rm -f "$t"; fi"#;
    if let Ok(out) = machines::run_with_input(&m.host, script, SCRIPT) {
        if out.trim() == "updated" {
            eprintln!("cue: {}: its hook brought up to date", m.name);
        }
    }
}
