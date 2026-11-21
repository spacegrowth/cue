//! Which model a Desk's new sessions start on (#620). The lead can name one on a route to a new session
//! (`ROUTE: #id -> new | model: fable | why: …`); without one, the Desk's default from its settings; without that,
//! Claude Code's own pick. The name is matched against Claude Code's list of models on this Mac when that's in, so
//! "Fable" or "opus" become the value Claude Code takes; one it doesn't list is refused, with the list. The chosen
//! model is kept on the item, and the session starts with `--model <it>`. Kept in Cue's settings
//! (`project.model`: Desk → model), never in the project.

use serde_json::{json, Value};

/// "Claude Code picks": no `--model` flag.
pub(crate) const DEFAULT: &str = "default";
/// The longest model name taken.
const MAX: usize = 80;

/// A model name as it can go on a start line: no spaces, quotes or shell marks, at most MAX characters.
fn plain(m: &str) -> bool {
    !m.is_empty() && m.len() <= MAX && m.chars().all(|c| c.is_ascii_alphanumeric() || "-_.[]:/@".contains(c))
}

/// `project.model` as saved: Desk → model, each a plain name ("default" dropped: that's no setting).
pub(crate) fn checked(value: &Value) -> Result<Value, String> {
    let map = value.as_object().ok_or("a Desk's model must be desk: model")?;
    let mut out = serde_json::Map::new();
    for (desk, m) in map {
        let m = m.as_str().ok_or("a model is text")?.trim();
        if m.is_empty() || m.eq_ignore_ascii_case(DEFAULT) {
            continue;
        }
        if !plain(m) {
            return Err(format!("{m} isn't a model name"));
        }
        out.insert(desk.clone(), json!(m));
    }
    Ok(Value::Object(out))
}

/// Desk `desk`'s default model in config `c` ("" for none: Claude Code picks).
pub(crate) fn default_in(c: &Value, desk: &str) -> String {
    c.pointer("/project/model").and_then(|m| m.get(desk)).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Desk `desk`'s default model ("" for none).
pub(crate) fn default_of(desk: &str) -> String {
    default_in(&crate::config::load(), desk)
}

/// Desk `desk`'s default model set to `model` ("" or "default": Claude Code picks).
pub(crate) fn set_default(desk: &str, model: &str) -> Result<String, String> {
    let model = model.trim();
    if desk.trim().is_empty() {
        return Err("no Desk".into());
    }
    let model = if model.eq_ignore_ascii_case(DEFAULT) { "" } else { model };
    if !model.is_empty() && !plain(model) {
        return Err(format!("{model} isn't a model name"));
    }
    crate::config::update_yours("project.model", |c| {
        let mut map = c.pointer("/project/model").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        if model.is_empty() {
            map.as_object_mut().map(|m| m.remove(desk));
        } else {
            map[desk] = json!(model);
        }
        Ok((Some(map), ()))
    })?;
    Ok(model.to_string())
}

/// The lead's `model:` word as the value Claude Code takes, matched against `list` (Claude Code's models: value, label;
/// None while it isn't read yet): its value or its label, any case ("Opus" or "opus"), or a label that starts with it
/// ("Fable" for "Fable 5.2"). "default" is Claude Code's own pick. Err: why it's refused, as the lead is told.
pub(crate) fn named(word: &str, list: Option<&[(String, String)]>) -> Result<String, String> {
    let w = word.trim().trim_matches(['"', '`', '*', '.']).trim();
    if w.eq_ignore_ascii_case(DEFAULT) {
        return Ok(DEFAULT.into());
    }
    if !plain(w) {
        return Err(format!("\"{w}\" isn't a model name"));
    }
    let Some(list) = list.filter(|l| !l.is_empty()) else { return Ok(w.to_ascii_lowercase()) };
    let bare = |s: &str| s.split('[').next().unwrap_or(s).to_ascii_lowercase();
    let lw = w.to_ascii_lowercase();
    let found = list.iter().find(|(v, l)| v.eq_ignore_ascii_case(w) || l.eq_ignore_ascii_case(w))
        .or_else(|| list.iter().find(|(v, l)| bare(v) == lw || l.to_ascii_lowercase().split_whitespace().next() == Some(lw.as_str())))
        .or_else(|| list.iter().find(|(v, _)| bare(v).starts_with(&format!("claude-{lw}-"))));
    match found {
        Some((v, _)) => Ok(v.clone()),
        // A full id or one with a bracket ([1m]) goes as typed: Claude Code's list names aliases, not every id.
        None if w.contains('-') || w.contains('[') => Ok(w.to_string()),
        None => {
            let names: Vec<&str> = list.iter().filter(|(v, _)| !v.eq_ignore_ascii_case(DEFAULT)).map(|(v, l)| if l.is_empty() { v.as_str() } else { l.as_str() }).collect();
            Err(format!("Claude Code here has no model called {w}. Its models: {}.", names.join(", ")))
        }
    }
}

/// Claude Code's models on this Mac, as last read (value, label); None while it's still read (never waits).
#[cfg(not(test))]
pub(crate) fn listed() -> Option<Vec<(String, String)>> {
    let home = crate::commands::home_folder("");
    crate::commands::models_for(&home, "", false).map(|l| l.into_iter().map(|a| (a.value, a.label)).collect())
}

/// Tests never start Claude Code to read its list: a fixed one, as Claude Code names its models.
#[cfg(test)]
pub(crate) fn listed() -> Option<Vec<(String, String)>> {
    let l = [("default", "Default (recommended)"), ("fable", "Fable"), ("opus", "Opus"), ("sonnet", "Sonnet"), ("haiku", "Haiku")];
    Some(l.iter().map(|(v, n)| (v.to_string(), n.to_string())).collect())
}

/// The `--model` flag a new session starts with for `model` ("" for none: Claude Code picks).
pub(crate) fn flag(model: &str) -> String {
    if model.is_empty() || model.eq_ignore_ascii_case(DEFAULT) || !plain(model) { String::new() } else { format!("--model {model}") }
}

/// What the lead reads of the Desk's default in its <desk> block, with how to pick another.
pub(crate) fn lead_line(desk: &str) -> String {
    let d = default_of(desk);
    let now = if d.is_empty() { "Claude Code's own pick".to_string() } else { d };
    format!("New sessions start on: {now} (the Desk's default; name another on a route to new with | model: <name>).")
}
