//! A project's actions as they're written down: one file of yours per project, in Cue's own folder (never
//! in the project, so nothing of it is shared with the repo). Reading and writing only: which file is
//! a project's, what's allowed or running lives elsewhere (mod.rs, run.rs).

use serde_json::{json, Map, Value};

/// The most of a file Cue reads.
pub const FILE_MAX: usize = 256 * 1024;

/// The notes at the top of the file, Cue's own: for the person and for an agent asked to set them up.
/// A file written without them gets them; a file's own are kept as they are, unless they're an older
/// Cue's (`OLD_ABOUT`), which give way to these.
pub const ABOUT: [&str; 7] = [
    "Actions are buttons in Cue for the project in \"folder\": install, test, start a server, clean a cache. Cue shows them under the agent's latest reply.",
    "Each needs a \"name\" and a \"run\" (a one-line shell command, run by the person's shell in that folder). Any other key is ignored.",
    "Like this: {\"name\": \"Install\", \"run\": \"npm ci\", \"ask\": true}",
    "\"keep\": true for a server or watcher that keeps running; Cue shows its port, Restart and Stop.",
    "\"ask\": true if running it while an agent is working could trip the agent (install, clean): Cue asks.",
    "Keep it to the two or three the person runs most, from what the project really uses (package.json, justfile, Makefile, README, docker-compose).",
    "Cue asks the person before running anything new or changed.",
];

/// Cue's notes as earlier Cues wrote them (they didn't name the keys, so an agent guessed "command").
const OLD_ABOUT: [&[&str]; 1] = [&[
    "Actions are buttons in Cue for the project in \"folder\": install, test, start a server, clean a cache. Cue shows them under the agent's latest reply.",
    "Each needs a name and a one-line shell command, run by the person's shell in that folder.",
    "\"keep\": true for a server or watcher that keeps running; Cue shows its port, Restart and Stop.",
    "\"ask\": true if running it while an agent is working could trip the agent (install, clean): Cue asks.",
    "Keep it to the two or three the person runs most, from what the project really uses (package.json, justfile, Makefile, README, docker-compose).",
    "Cue asks the person before running anything new or changed.",
]];

/// One action.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Action {
    /// "local" (yours; the one kind there is now, kept in its id so what you allowed stays allowed).
    pub source: String,
    pub name: String,
    /// The command line Cue runs.
    pub run: String,
    pub keep: bool,
    pub ask: bool,
}

impl Action {
    /// Stable across reads: its source and its name.
    pub fn id(&self) -> String {
        format!("{}:{}", self.source, self.name)
    }
    /// What you allow when you allow it: its command.
    pub fn trusted(&self) -> String {
        self.run.clone()
    }
}

/// The actions in a file (`source` as each one's). An entry without a name or a
/// command is left out; a file that isn't JSON is an error (Cue says so and never writes over it).
pub fn parse(text: &str, source: &str) -> Result<Vec<Action>, String> {
    if text.trim().is_empty() {
        return Ok(vec![]);
    }
    let v: Value = serde_json::from_str(text).map_err(|e| format!("isn't valid JSON: {e}"))?;
    let list = match v.get("actions") {
        None | Some(Value::Null) => return Ok(vec![]),
        Some(Value::Array(a)) => a,
        Some(_) => return Err("\"actions\" isn't a list".into()),
    };
    let mut out: Vec<Action> = vec![];
    for a in list {
        let (Some(name), Some(run)) = (a["name"].as_str().map(str::trim), a["run"].as_str().map(str::trim)) else { continue };
        if name.is_empty() || run.is_empty() || out.iter().any(|o| o.name == name) {
            continue;
        }
        out.push(Action { source: source.into(), name: name.into(), run: run.into(), keep: a["keep"].as_bool().unwrap_or(false), ask: a["ask"].as_bool().unwrap_or(false) });
    }
    Ok(out)
}

/// What a file's entries left out say, one line each: an entry with no "name" or no "run" isn't an
/// action, and nothing else would tell the person why it isn't there.
pub fn left_out(text: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return vec![] };
    let Some(list) = v.get("actions").and_then(Value::as_array) else { return vec![] };
    let has = |a: &Value, k: &str| a[k].as_str().is_some_and(|t| !t.trim().is_empty());
    list.iter()
        .filter(|a| !has(a, "name") || !has(a, "run"))
        .map(|a| {
            let who = a["name"].as_str().map(str::trim).filter(|n| !n.is_empty()).map(|n| format!("\"{n}\"")).unwrap_or_else(|| "One".into());
            let missing = if has(a, "name") { "\"run\"" } else if has(a, "run") { "\"name\"" } else { "\"name\" and \"run\"" };
            let other = a.as_object().and_then(|o| o.keys().find(|k| matches!(k.as_str(), "command" | "cmd" | "script" | "shell")).cloned());
            match other.filter(|_| !has(a, "run")) {
                Some(k) => format!("{who} has \"{k}\" where Cue reads \"run\""),
                None => format!("{who} has no {missing}"),
            }
        })
        .collect()
}

/// The file with Cue's notes brought up to date when they're an older Cue's; None when there's nothing to change.
pub fn notes_refreshed(text: &str) -> Option<String> {
    let mut m = match serde_json::from_str::<Value>(text).ok()? {
        Value::Object(m) => m,
        _ => return None,
    };
    if !older_notes(&m) {
        return None;
    }
    m.insert("about".into(), json!(ABOUT));
    Some(written(m))
}

/// The file's notes are an older Cue's, word for word (ones you've changed are yours and stay).
fn older_notes(m: &Map<String, Value>) -> bool {
    let notes = m.get("about").and_then(Value::as_array).and_then(|a| a.iter().map(Value::as_str).collect::<Option<Vec<&str>>>());
    notes.is_some_and(|n| OLD_ABOUT.iter().any(|o| *o == n.as_slice()))
}

/// The file's JSON to change: its own (keys and order kept), or a new one, with Cue's notes first when it
/// has none and `folder` (the project's) after them. Refuses a file that isn't JSON rather than lose what's in it.
fn editable(text: Option<&str>, folder: &str) -> Result<Map<String, Value>, String> {
    let mut m = match text.filter(|t| !t.trim().is_empty()) {
        Some(t) => match serde_json::from_str::<Value>(t).map_err(|e| format!("Fix the file first: it isn't valid JSON ({e})"))? {
            Value::Object(m) => m,
            _ => return Err("Fix the file first: it isn't a JSON object".into()),
        },
        None => Map::new(),
    };
    if older_notes(&m) {
        m.insert("about".into(), json!(ABOUT));
    }
    if !m.contains_key("about") {
        let mut with = Map::new();
        with.insert("about".into(), json!(ABOUT));
        if !m.contains_key("folder") {
            with.insert("folder".into(), json!(folder));
        }
        with.extend(std::mem::take(&mut m));
        m = with;
    }
    if !m.get("actions").is_some_and(Value::is_array) {
        m.insert("actions".into(), json!([]));
    }
    Ok(m)
}

fn written(m: Map<String, Value>) -> String {
    serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default() + "\n"
}

/// A new file for the project in `folder`: Cue's notes, the folder, no actions yet.
pub fn fresh(folder: &str) -> String {
    written(editable(None, folder).unwrap_or_default())
}

/// The file with `a` saved in it: the entry named `old` (else `a.name`) changed in place, its other keys
/// kept, or a new entry at the end. `keep`/`ask` are written only when on.
pub fn with_action(text: Option<&str>, folder: &str, old: Option<&str>, a: &Action) -> Result<String, String> {
    let mut m = editable(text, folder)?;
    let list = m.get_mut("actions").and_then(Value::as_array_mut).ok_or("no actions list")?;
    let is = |e: &Value, n: &str| e["name"].as_str().map(str::trim) == Some(n);
    let at = old.and_then(|o| list.iter().position(|e| is(e, o))).or_else(|| list.iter().position(|e| is(e, &a.name)));
    if list.iter().enumerate().any(|(i, e)| Some(i) != at && e["name"].as_str().is_some_and(|n| n.trim().eq_ignore_ascii_case(&a.name))) {
        return Err(format!("There's already an action called {}", a.name));
    }
    let mut e = match at {
        Some(i) => list[i].as_object().cloned().unwrap_or_default(),
        None => Map::new(),
    };
    e.insert("name".into(), json!(a.name));
    e.insert("run".into(), json!(a.run));
    for (k, on) in [("keep", a.keep), ("ask", a.ask)] {
        if on {
            e.insert(k.into(), json!(true));
        } else {
            e.remove(k);
        }
    }
    match at {
        Some(i) => list[i] = Value::Object(e),
        None => list.push(Value::Object(e)),
    }
    Ok(written(m))
}

/// The file without the action named `name` (everything else as it was).
pub fn without_action(text: Option<&str>, folder: &str, name: &str) -> Result<String, String> {
    let mut m = editable(text, folder)?;
    if let Some(list) = m.get_mut("actions").and_then(Value::as_array_mut) {
        list.retain(|e| e["name"].as_str().map(str::trim) != Some(name));
    }
    Ok(written(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_a_file_keeps_its_notes_its_other_keys_and_each_entrys_own() {
        let text = r#"{"about": ["ours"], "version": 2, "actions": [{"name": "API", "run": "npm run dev:api", "keep": true, "color": "lime"}, {"name": "Lint", "run": "npm run lint"}]}"#;
        let out = with_action(Some(text), "/p", Some("API"), &Action { name: "Server".into(), run: "npm run dev".into(), keep: false, ask: true, ..Default::default() }).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["about"], json!(["ours"]), "its own notes stay");
        assert_eq!(v["version"], 2);
        assert_eq!(v["actions"][0], json!({"name": "Server", "run": "npm run dev", "color": "lime", "ask": true}));
        assert_eq!(v["actions"][1]["name"], "Lint");
        assert!(out.starts_with("{\n  \"about\""), "2-space JSON: {out}");
        assert!(out.ends_with("}\n"));
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["about", "version", "actions"], "order kept");
        // Parsed back as written.
        let a = parse(&out, "local").unwrap();
        assert_eq!((a[0].name.as_str(), a[0].ask, a[0].keep), ("Server", true, false));
        assert_eq!(a[0].id(), "local:Server");
    }

    #[test]
    fn a_new_file_gets_cues_notes_first_and_a_broken_one_is_never_written_over() {
        let out = with_action(None, "/p", None, &Action { name: "Install".into(), run: "npm ci".into(), ..Default::default() }).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["about"], json!(ABOUT));
        assert_eq!(v["folder"], "/p", "which project's it is");
        assert_eq!(v["actions"], json!([{"name": "Install", "run": "npm ci"}]));
        let no_notes = with_action(Some(r#"{"actions": [], "x": 1}"#), "/p", None, &Action { name: "T".into(), run: "t".into(), ..Default::default() }).unwrap();
        let keys: Vec<String> = serde_json::from_str::<Value>(&no_notes).unwrap().as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["about", "folder", "actions", "x"]);
        assert!(with_action(Some("{ not json"), "/p", None, &Action { name: "T".into(), run: "t".into(), ..Default::default() }).is_err());
        assert!(parse("{ not json", "local").is_err());
        let two = with_action(Some(&out), "/p", None, &Action { name: "install".into(), run: "pnpm i".into(), ..Default::default() });
        assert!(two.unwrap_err().contains("already"), "two of one name");
        let gone = without_action(Some(&out), "/p", "Install").unwrap();
        assert_eq!(serde_json::from_str::<Value>(&gone).unwrap()["actions"], json!([]));
    }

    #[test]
    fn a_files_entries_need_a_name_and_a_command() {
        let a = parse(r#"{"actions": [{"name": "A", "run": "a"}, {"name": "", "run": "b"}, {"run": "c"}, {"name": "D"}, {"name": "A", "run": "again"}, {"name": "E", "run": "e", "suggest": "x/*"}]}"#, "local").unwrap();
        assert_eq!(a.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["A", "E"]);
        assert!(parse("", "local").unwrap().is_empty());
    }

    #[test]
    fn what_a_file_left_out_is_said_and_a_guessed_key_is_named() {
        let t = r#"{"actions": [{"name": "A", "run": "a"}, {"name": "Install", "command": "npm ci"}, {"name": "B"}, {"run": "c"}, {"x": 1}]}"#;
        assert_eq!(left_out(t), [r#""Install" has "command" where Cue reads "run""#, r#""B" has no "run""#, r#"One has no "name""#, r#"One has no "name" and "run""#]);
        assert!(left_out(r#"{"actions": [{"name": "A", "run": "a"}]}"#).is_empty());
        assert!(left_out("{ not json").is_empty());
    }

    #[test]
    fn an_older_cues_notes_give_way_and_yours_stay() {
        let old = json!({"about": OLD_ABOUT[0], "folder": "/p", "actions": [{"name": "T", "run": "t"}]}).to_string();
        let v: Value = serde_json::from_str(&notes_refreshed(&old).unwrap()).unwrap();
        assert_eq!(v["about"], json!(ABOUT));
        assert_eq!(v["actions"], json!([{"name": "T", "run": "t"}]), "the actions stay");
        assert!(ABOUT[1].contains(r#""run""#), "the notes name the key");
        assert!(notes_refreshed(r#"{"about": ["ours"], "actions": []}"#).is_none());
        assert!(notes_refreshed(&fresh("/p")).is_none(), "today's are left alone");
        let saved: Value = serde_json::from_str(&with_action(Some(&old), "/p", None, &Action { name: "U".into(), run: "u".into(), ..Default::default() }).unwrap()).unwrap();
        assert_eq!(saved["about"], json!(ABOUT), "a save brings them up to date too");
    }
}
