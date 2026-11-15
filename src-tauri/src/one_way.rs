//! One-way here (Settings → General → Projects): per project, the few kinds of step you count as one-way, so a crew
//! lead asks you first before any of them, and a crew commit that is one of them is marked one-way on its crew log
//! (sessions/crew_log.rs; its Undo says "Make a revert"). Kept in config.json as `project.oneway`: project → its
//! kinds' keys. A project not in it has the default, today's rule: a merge or push to main (or master).
//!
//! Not a policy engine: a short fixed list of known kinds, checked or not. Only one of them is something Cue sees
//! itself (a crew commit on main or master: `main`); the others reach the lead as words in its instructions.

use serde_json::{json, Value};

/// One kind of one-way step: its key in config, its words in Settings, and as the lead's instructions say it.
pub struct Kind {
    pub key: &'static str,
    pub label: &'static str,
    pub ask: &'static str,
}

/// The kinds, in the order Settings shows them and the lead is told them.
pub const KINDS: &[Kind] = &[
    Kind { key: "main", label: "Merge or push to main", ask: "a merge or push to main or master" },
    Kind { key: "push", label: "Push any branch", ask: "a push of any branch" },
    Kind { key: "release", label: "A release", ask: "a release (a version tagged or published)" },
    Kind { key: "deploy", label: "Deploy", ask: "a deploy" },
    Kind { key: "data", label: "Delete data", ask: "deleting data (a database's rows, files git can't bring back)" },
    Kind { key: "outside", label: "Files outside the project", ask: "changing files outside this project's folder" },
];

/// A project you never set: a merge or push to main.
pub const DEFAULT: &[&str] = &["main"];

/// Project `project`'s kinds in config `c`: what you checked, else the default.
pub fn kinds_in(c: &Value, project: &str) -> Vec<&'static str> {
    match c.pointer("/project/oneway").and_then(|m| m.get(project)).and_then(Value::as_array) {
        Some(list) => KINDS.iter().map(|k| k.key).filter(|k| list.iter().any(|v| v.as_str() == Some(k))).collect(),
        None => DEFAULT.to_vec(),
    }
}

/// Project `project`'s kinds, as config.json says now.
pub fn kinds(project: &str) -> Vec<&'static str> {
    kinds_in(&crate::config::load(), project)
}

/// Whether a crew commit on `branch` is one-way in a project with `kinds`: a commit on main or master when a merge
/// or push to main is one of them.
pub fn commit_one_way(kinds: &[&str], branch: &str) -> bool {
    matches!(branch, "main" | "master") && kinds.contains(&"main")
}

/// Whether a shell command an agent asks to run is one-way in a project with `kinds`: any of its parts (split at
/// `&&`, `||`, `;` and `|`) is a git push, merge or commit that names main or master (`main`), any git push (`push`),
/// a release (`git tag`, `gh release`, a package publish: `release`), or a deploy (`deploy` as a word of it). Read
/// from its words only: a plain `git push` on a branch that happens to be main isn't seen (Cue doesn't stat the
/// session's folder for it). The crew page's Needs you bar answers a one-way ask only with a click.
pub fn command_one_way(kinds: &[&str], command: &str) -> bool {
    let parts = command.split(['&', '|', ';', '\n']).map(str::trim).filter(|p| !p.is_empty());
    parts.into_iter().any(|p| {
        let w: Vec<&str> = p.split_whitespace().collect();
        let git = w.iter().position(|x| *x == "git").map(|i| &w[i + 1..]).unwrap_or(&[]);
        // Its subcommand: the first word that isn't an option (nor `-C <dir>` / `-c <key=value>`'s value).
        let mut sub = "";
        let mut i = 0;
        while i < git.len() {
            match git[i] {
                "-C" | "-c" => i += 2,
                x if x.starts_with('-') => i += 1,
                x => { sub = x; break; }
            }
        }
        let main = git.iter().any(|x| x.split([':', '/']).any(|r| r == "main" || r == "master"));
        let publish = w.windows(2).any(|p| matches!(p[0], "npm" | "pnpm" | "yarn" | "cargo" | "twine" | "gem") && p[1] == "publish");
        (kinds.contains(&"main") && matches!(sub, "push" | "merge" | "commit") && main)
            || (kinds.contains(&"push") && sub == "push")
            || (kinds.contains(&"release") && (sub == "tag" || publish || w.windows(2).any(|p| p[0] == "gh" && p[1] == "release")))
            || (kinds.contains(&"deploy") && w.iter().any(|x| *x == "deploy"))
    })
}

/// The line the crew lead's instructions get for a project with `kinds` (team_out.rs): what it asks the user first
/// before. "" when you count nothing one-way there.
pub fn ask_first(kinds: &[&str]) -> String {
    let said: Vec<&str> = KINDS.iter().filter(|k| kinds.contains(&k.key)).map(|k| k.ask).collect();
    let list = match said.as_slice() {
        [] => return String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
    };
    format!("- One-way in this project, as the user set it: ask the user first, and end your turn, before {list}.")
}

/// `project.oneway` as it's saved: each project's keys, known ones only, once each, in KINDS's order (an empty list
/// is kept: nothing one-way there, which isn't the default).
pub fn checked(value: &Value) -> Result<Value, String> {
    let map = value.as_object().ok_or("one-way must be project: [kinds]")?;
    let mut out = serde_json::Map::new();
    for (p, list) in map {
        let list = list.as_array().ok_or("a project's one-way kinds are a list")?;
        if let Some(bad) = list.iter().find(|v| !v.as_str().is_some_and(|s| KINDS.iter().any(|k| k.key == s))) {
            return Err(format!("{bad} isn't a one-way kind"));
        }
        let keys: Vec<&str> = KINDS.iter().map(|k| k.key).filter(|k| list.iter().any(|v| v.as_str() == Some(k))).collect();
        out.insert(p.clone(), json!(keys));
    }
    Ok(Value::Object(out))
}

/// What the window gets (config.rs `effective`): each project you set, the kinds with their words, the default.
pub fn view(c: &Value) -> Value {
    let set = c.pointer("/project/oneway").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let kinds: Vec<Value> = KINDS.iter().map(|k| json!({ "key": k.key, "label": k.label })).collect();
    json!({ "set": set, "kinds": kinds, "default": DEFAULT })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_you_never_set_counts_a_merge_or_push_to_main() {
        let c = json!({ "project": { "oneway": { "api": ["push", "deploy"], "site": [] } } });
        assert_eq!(kinds_in(&c, "web"), ["main"]);
        assert_eq!(kinds_in(&c, "api"), ["push", "deploy"]);
        assert!(kinds_in(&c, "site").is_empty(), "nothing checked is nothing, not the default");
        assert_eq!(kinds_in(&json!({}), "web"), ["main"]);
    }

    #[test]
    fn a_commit_is_one_way_from_the_setting() {
        assert!(commit_one_way(DEFAULT, "main") && commit_one_way(DEFAULT, "master"));
        assert!(!commit_one_way(DEFAULT, "crew/rows"));
        assert!(!commit_one_way(&[], "main"), "a push to main is fine here: its commit isn't one-way");
        assert!(!commit_one_way(&["push", "release"], "main"));
    }

    #[test]
    fn a_command_is_one_way_from_its_words_and_the_setting() {
        assert!(command_one_way(DEFAULT, "git merge crew/hub main"));
        assert!(command_one_way(DEFAULT, "cd api && git push origin HEAD:main"));
        assert!(command_one_way(DEFAULT, "git -C web push origin master"));
        assert!(!command_one_way(DEFAULT, "git push origin crew/rows"), "a crew branch isn't main");
        assert!(!command_one_way(DEFAULT, "git log main..crew/rows"), "reading main changes nothing");
        assert!(!command_one_way(DEFAULT, "npm test"));
        assert!(!command_one_way(&[], "git push origin main"), "nothing counted one-way here");
        assert!(command_one_way(&["push"], "npm test; git push"));
        assert!(command_one_way(&["release"], "npm publish --access public") && command_one_way(&["release"], "gh release create v1"));
        assert!(command_one_way(&["deploy"], "fly deploy") && !command_one_way(&["deploy"], "npm run deploy-docs"));
    }

    #[test]
    fn the_lead_is_told_what_to_ask_first_before() {
        assert_eq!(ask_first(DEFAULT), "- One-way in this project, as the user set it: ask the user first, and end your turn, before a merge or push to main or master.");
        let two = ask_first(&["deploy", "push"]);
        assert!(two.contains("before a push of any branch, or a deploy."), "in the list's order: {two}");
        assert!(ask_first(&["main", "push", "release"]).contains("main or master, a push of any branch, or a release"));
        assert_eq!(ask_first(&[]), "");
    }

    #[test]
    fn the_setting_keeps_known_kinds_once_in_order() {
        let saved = checked(&json!({ "api": ["deploy", "main", "deploy"], "site": [] })).unwrap();
        assert_eq!(saved, json!({ "api": ["main", "deploy"], "site": [] }));
        assert!(checked(&json!({ "api": ["rm -rf"] })).is_err());
        assert!(checked(&json!({ "api": "main" })).is_err());
        assert!(checked(&json!(["main"])).is_err());
        let v = view(&json!({ "project": { "oneway": { "api": ["push"] } } }));
        assert_eq!((v["set"]["api"][0].as_str(), v["kinds"].as_array().map(Vec::len), v["default"][0].as_str()), (Some("push"), Some(KINDS.len()), Some("main")));
    }
}
