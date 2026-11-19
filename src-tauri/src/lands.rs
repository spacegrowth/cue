//! How work lands: per project, the six parts of the setting in docs/desk-landing-design.md (where an item works, how
//! it lands, the check, after landing, push, items at once). Kept in config.json as `project.lands`: project → the
//! parts you saved, next to Ask me before (one_way.rs). Never in the project's files. A project not there has the
//! defaults (part 7): its own copy per item, three at once, one commit per item, no check, nothing after, ask before
//! a push. Cue's own project has Cue's setup (its check, then installed or served in place), and a folder that isn't a
//! git repository can only work in place, its work committed by you.
//!
//! Some choices are shown before Cue can do them (a branch, merge, a command of your own): they can't be picked, and
//! say so. A pull request and the push choices are built (hub/desk_pr.rs). The Desk reads the rest as it works (hub/desk_trees.rs,
//! hub/desk_finish.rs, hub/desk_commit.rs).

use serde_json::{json, Map, Value};

/// One choice of a part: its key in config, its words in the window, what it means there, and whether Cue can do it.
pub struct Choice {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub ready: bool,
}

/// One part of the setting: its key, its words, its choices in the order the window shows them.
pub struct Part {
    pub key: &'static str,
    pub label: &'static str,
    pub choices: &'static [Choice],
}

const fn c(key: &'static str, label: &'static str, hint: &'static str, ready: bool) -> Choice {
    Choice { key, label, hint, ready }
}

/// The parts with choices (items at once is a number: `AT_ONCE`).
pub const PARTS: &[Part] = &[
    Part {
        key: "where",
        label: "Where an item works",
        choices: &[
            c("shared", "Shared folder", "Every session works in the project's own folder. The Desk commits only the item's own files.", true),
            c("copy", "Own copy", "Each item gets its own copy of the project, cut from main when it starts. Cue makes it and removes it once the item is on main.", true),
            c("branch", "A branch", "", false),
        ],
    },
    Part {
        key: "how",
        label: "How it lands",
        choices: &[
            c("squash", "One commit per item", "The Desk puts each item on main as one commit, named after the item. One at a time.", true),
            c("merge", "Merge", "", false),
            c("pr", "Pull request", "The Desk checks each item, pushes its branch and opens a pull request. It lands once the pull request is merged.", true),
            c("owner", "You commit", "The Desk commits nothing. A finished item shows its changed files and waits for you.", true),
        ],
    },
    Part {
        key: "check",
        label: "Check before landing",
        choices: &[
            c("none", "None", "Nothing is run. An item lands and says \"landed, not checked\".", true),
            c("cue", "Cue's own check", "Run in a clean copy of main plus the item, in order. If one fails, nothing reaches main.", true),
            c("command", "A command", "", false),
        ],
    },
    Part {
        key: "after",
        label: "After landing",
        choices: &[
            c("nothing", "Nothing", "Landed is the last step: an item is done when it's on main.", true),
            c("install", "Install or serve Cue", "Window-only changes are served in place, after the check. Others join the install every half hour.", true),
            c("command", "A command", "", false),
        ],
    },
    Part {
        key: "push",
        label: "Push to main",
        choices: &[
            c("never", "Never", "The Desk pushes nothing and asks nothing. You push by hand.", true),
            c("ask", "Ask each time", "A push to main asks you first, naming its commits.", true),
            c("after", "After each landing", "After each landing the Desk pushes main as it is, everything on it, without asking. Never forced. Sessions still never push main.", true),
        ],
    },
];

/// Items at once: the most items of a project at work in copies of their own.
pub const AT_ONCE: (u32, u32) = (1, 9);
/// Items at once when not set.
pub const AT_ONCE_DEFAULT: u32 = 3;

/// What a choice that isn't built yet says.
pub const NOT_YET: &str = "not yet";
/// Cue's own check, as its steps (docs/desk-landing-design.md part 7).
pub const CUE_STEPS: &[(&str, &str)] = &[
    ("Build", "the app built from a clean copy"),
    ("Rust tests", "the hub's own tests"),
    ("Window loads", "from the clean copy, failing on a name missing or declared twice"),
];

/// What the project's folder is, which decides what can be picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Cue's own source.
    Cue,
    /// A git repository.
    Repo,
    /// A folder that isn't a git repository.
    NoRepo,
}

impl Place {
    fn key(self) -> &'static str {
        match self {
            Place::Cue => "cue",
            Place::Repo => "repo",
            Place::NoRepo => "folder",
        }
    }
}

/// Why a folder that isn't a git repository lands as it does.
pub const NO_REPO_WHY: &str = "This folder isn't a git repository, so items work in it directly, nothing is checked, and you commit their work yourself.";

/// A project's six parts, as they hold now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lands {
    pub at: &'static str,
    pub how: &'static str,
    pub check: &'static str,
    pub after: &'static str,
    pub push: &'static str,
    pub at_once: u32,
}

impl Lands {
    fn get(&self, part: &str) -> &'static str {
        match part {
            "where" => self.at,
            "how" => self.how,
            "check" => self.check,
            "after" => self.after,
            _ => self.push,
        }
    }

    fn set(&mut self, part: &str, key: &'static str) {
        match part {
            "where" => self.at = key,
            "how" => self.how = key,
            "check" => self.check = key,
            "after" => self.after = key,
            _ => self.push = key,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({ "where": self.at, "how": self.how, "check": self.check, "after": self.after, "push": self.push, "at_once": self.at_once })
    }
}

/// The defaults for a project in `place` (part 7).
pub fn defaults(place: Place) -> Lands {
    match place {
        Place::Cue => Lands { at: "copy", how: "squash", check: "cue", after: "install", push: "ask", at_once: AT_ONCE_DEFAULT },
        Place::Repo => Lands { at: "copy", how: "squash", check: "none", after: "nothing", push: "ask", at_once: AT_ONCE_DEFAULT },
        Place::NoRepo => Lands { at: "shared", how: "owner", check: "none", after: "nothing", push: "ask", at_once: AT_ONCE_DEFAULT },
    }
}

/// Whether choice `key` of part `part` can be picked in `place`: Err says why not, in plain words.
pub fn allowed(part: &str, key: &str, place: Place) -> Result<&'static str, &'static str> {
    let ch = PARTS.iter().find(|p| p.key == part).and_then(|p| p.choices.iter().find(|c| c.key == key)).ok_or("not a choice")?;
    if !ch.ready {
        return Err(NOT_YET);
    }
    match (part, key, place) {
        ("check", "cue", p) | ("after", "install", p) if p != Place::Cue => Err("only for Cue's own project"),
        ("where", "copy", Place::NoRepo) | ("how", "squash" | "pr", Place::NoRepo) => Err("needs a git repository"),
        _ => Ok(ch.key),
    }
}

/// Project `project`'s setting in config `c`, for its folder in `place`: what you saved where it can still be
/// picked, the defaults for the rest.
pub fn of_in(c: &Value, project: &str, place: Place) -> Lands {
    let mut l = defaults(place);
    if let Some(s) = stored(c, project) {
        for p in PARTS {
            if let Some(k) = s.get(p.key).and_then(Value::as_str).and_then(|k| allowed(p.key, k, place).ok()) {
                l.set(p.key, k);
            }
        }
        if let Some(n) = s.get("at_once").and_then(Value::as_u64).filter(|n| (AT_ONCE.0 as u64..=AT_ONCE.1 as u64).contains(n)) {
            l.at_once = n as u32;
        }
    }
    // You commit: there's no copy for it to land from.
    if l.how == "owner" {
        l.at = "shared";
    }
    // A pull request: its branch is the item's own copy's, so there's none in the shared folder.
    if l.how == "pr" && l.at == "shared" {
        l.how = defaults(place).how;
    }
    l
}

/// What you saved for `project` in config `c` (None: never set).
fn stored<'c>(c: &'c Value, project: &str) -> Option<&'c Value> {
    c.pointer("/project/lands").and_then(|m| m.get(project))
}

/// How work lands for `project`, its items working in `folder` (a session's on this Mac, or a repository): as the
/// Desk shows it, so the engine and the window never disagree. "" when the folder isn't known here (another machine).
pub fn lands_of(project: &str, folder: &str) -> Lands {
    of_in(&crate::config::load(), project, place_of(folder))
}

/// Whether a landing of `project` in `repo` skips the check. Only when you picked None: not picked, the repository's
/// own check runs where it has one (Cue's), and any other lands "not checked" all the same, as the default says.
pub fn check_off(project: &str, repo: &str) -> bool {
    let c = crate::config::load();
    let picked = stored(&c, project).is_some_and(|s| s.get("check").is_some());
    picked && of_in(&c, project, place_of(repo)).check == "none"
}

/// What `folder` (on this Mac) is. Read from what Cue already knows of the folder (logs/repos.rs, never a wait): a
/// folder not looked at yet counts as a repository until it is. Call `repos::first_look` first to wait for it.
pub fn place_of(folder: &str) -> Place {
    if folder.is_empty() {
        return Place::Repo;
    }
    if is_cue(folder) {
        return Place::Cue;
    }
    match crate::logs::repos::facts(folder) {
        Some(f) if f.top.is_none() => Place::NoRepo,
        Some(f) if f.root.as_deref().is_some_and(|r| is_cue(&r.to_string_lossy())) => Place::Cue,
        _ => Place::Repo,
    }
}

/// Whether `repo` is Cue's own source, remembered per folder (read under the store's lock, on each redraw).
fn is_cue(repo: &str) -> bool {
    static SEEN: std::sync::Mutex<Option<std::collections::HashMap<String, bool>>> = std::sync::Mutex::new(None);
    let mut seen = crate::locking::lock(&SEEN, "lands cue folders");
    let map = seen.get_or_insert_with(Default::default);
    if let Some(v) = map.get(repo) {
        return *v;
    }
    let v = crate::hub::builds_this_cue(repo);
    map.insert(repo.to_string(), v);
    v
}

/// The short words for each choice in a Desk's header ("Lands: own copy, checked, one commit per item").
fn short(part: &str, key: &str) -> &'static str {
    match (part, key) {
        ("where", "shared") => "shared folder",
        ("where", "copy") => "own copy",
        ("how", "squash") => "one commit per item",
        ("how", "owner") => "you commit",
        ("how", "pr") => "pull request",
        ("check", "none") => "not checked",
        _ => "checked",
    }
}

/// What a Desk on `project` (its folder `folder`) shows of it (hub/desk.rs desk_view): the setting now, its words for
/// the header, the parts with what can be picked (and why not), and why when the folder decides it.
pub fn desk_view(project: &str, folder: &str) -> Value {
    let place = place_of(folder);
    let l = of_in(&crate::config::load(), project, place);
    let summary = format!("{}, {}, {}", short("where", l.at), short("check", l.check), short("how", l.how));
    let parts: Vec<Value> = PARTS
        .iter()
        .map(|p| {
            let choices: Vec<Value> = p
                .choices
                .iter()
                .map(|c| {
                    let why = allowed(p.key, c.key, place).err().unwrap_or("");
                    json!({ "key": c.key, "label": c.label, "hint": c.hint, "ok": why.is_empty(), "why": why })
                })
                .collect();
            json!({ "key": p.key, "label": p.label, "choices": choices, "now": l.get(p.key) })
        })
        .collect();
    let steps: Vec<Value> = CUE_STEPS.iter().map(|(w, h)| json!({ "label": w, "hint": h })).collect();
    json!({
        "project": project, "place": place.key(), "set": l.to_json(), "summary": summary, "parts": parts,
        "at_once": { "min": AT_ONCE.0, "max": AT_ONCE.1 }, "steps": steps,
        "why": if place == Place::NoRepo { NO_REPO_WHY } else { "" },
    })
}

/// Project `project`'s setting saved from `value` (each part's key, and `at_once`), for its folder in `place`. A choice
/// that can't be picked there is refused, saying why. Returns the setting as it holds now.
pub fn save(project: &str, place: Place, value: &Value) -> Result<Lands, String> {
    if project.trim().is_empty() {
        return Err("no project".into());
    }
    let mut out = Map::new();
    for p in PARTS {
        let Some(k) = value.get(p.key).and_then(Value::as_str) else { continue };
        let k = allowed(p.key, k, place).map_err(|why| format!("{}: {k} can't be picked ({why})", p.label))?;
        out.insert(p.key.into(), json!(k));
    }
    if let Some(n) = value.get("at_once") {
        let n = n.as_u64().filter(|n| (AT_ONCE.0 as u64..=AT_ONCE.1 as u64).contains(n)).ok_or(format!("Items at once is {} to {}", AT_ONCE.0, AT_ONCE.1))?;
        out.insert("at_once".into(), json!(n));
    }
    if out.get("how").and_then(Value::as_str) == Some("owner") && out.get("where").and_then(Value::as_str).is_some_and(|w| w != "shared") {
        return Err("You commit works only in the shared folder".into());
    }
    if out.get("how").and_then(Value::as_str) == Some("pr") && out.get("where").and_then(Value::as_str) == Some("shared") {
        return Err("A pull request needs each item in its own copy".into());
    }
    crate::config::update("project.lands", |c| {
        let mut map = c.pointer("/project/lands").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        map[project] = Value::Object(out);
        Some(map)
    })?;
    Ok(of_in(&crate::config::load(), project, place))
}

/// `project.lands` as it's saved: project → its parts (known keys, known choices, a number in range).
pub fn checked(value: &Value) -> Result<Value, String> {
    let map = value.as_object().ok_or("how work lands must be project: {parts}")?;
    let mut out = Map::new();
    for (project, set) in map {
        let set = set.as_object().ok_or("a project's setting is its parts")?;
        let mut keep = Map::new();
        for (k, v) in set {
            let ok = match k.as_str() {
                "at_once" => v.as_u64().is_some_and(|n| (AT_ONCE.0 as u64..=AT_ONCE.1 as u64).contains(&n)),
                part => PARTS.iter().find(|p| p.key == part).is_some_and(|p| p.choices.iter().any(|c| Some(c.key) == v.as_str())),
            };
            if !ok {
                return Err(format!("{k}: {v} isn't a choice"));
            }
            keep.insert(k.clone(), v.clone());
        }
        out.insert(project.clone(), Value::Object(keep));
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_project_gets_the_defaults_and_a_folder_with_no_repository_works_in_place() {
        let none = json!({});
        assert_eq!(of_in(&none, "p", Place::Repo), Lands { at: "copy", how: "squash", check: "none", after: "nothing", push: "ask", at_once: 3 });
        assert_eq!(of_in(&none, "cue", Place::Cue), Lands { at: "copy", how: "squash", check: "cue", after: "install", push: "ask", at_once: 3 });
        let c = json!({ "project": { "lands": { "p": { "where": "copy", "how": "squash" } } } });
        assert_eq!(of_in(&c, "p", Place::NoRepo), Lands { at: "shared", how: "owner", check: "none", after: "nothing", push: "ask", at_once: 3 }, "a copy needs a repository");
    }

    #[test]
    fn what_cant_be_picked_says_why_in_plain_words() {
        assert_eq!(allowed("where", "branch", Place::Repo), Err(NOT_YET));
        assert_eq!(allowed("how", "merge", Place::Cue), Err(NOT_YET));
        assert_eq!(allowed("how", "pr", Place::Repo), Ok("pr"));
        assert_eq!(allowed("how", "pr", Place::NoRepo), Err("needs a git repository"));
        assert_eq!(allowed("push", "after", Place::Cue), Ok("after"));
        assert_eq!(allowed("check", "command", Place::Cue), Err(NOT_YET));
        assert_eq!(allowed("check", "cue", Place::Repo), Err("only for Cue's own project"));
        assert_eq!(allowed("where", "copy", Place::NoRepo), Err("needs a git repository"));
        assert_eq!(allowed("how", "owner", Place::Repo), Ok("owner"));
    }

    #[test]
    fn a_saved_setting_holds_and_you_commit_means_the_shared_folder() {
        let c = json!({ "project": { "lands": { "p": { "where": "branch", "how": "owner", "check": "command", "at_once": 5, "push": "after" } } } });
        let l = of_in(&c, "p", Place::Repo);
        assert_eq!((l.at, l.how, l.check, l.at_once, l.push), ("shared", "owner", "none", 5, "after"), "a choice not built yet is never in effect");
        let c = json!({ "project": { "lands": { "p": { "where": "shared", "how": "pr" } } } });
        assert_eq!(of_in(&c, "p", Place::Repo).how, "squash", "a pull request needs a copy");
        assert!(checked(&json!({ "p": { "where": "nowhere" } })).is_err());
        assert!(checked(&json!({ "p": { "at_once": 0 } })).is_err());
        assert!(checked(&json!({ "p": { "where": "shared", "at_once": 2 } })).is_ok());
    }
}
