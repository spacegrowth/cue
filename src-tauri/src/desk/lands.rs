//! How work lands: per project, the six parts of the setting in docs/desk-landing-design.md (where an item works, how
//! it lands, the check, after landing, push, items at once). Kept in config.json as `project.lands`: project → the
//! parts you saved, next to Ask me before (desk/one_way.rs). Never in the project's files. A project not there has the
//! defaults (part 7): its own copy per item, three at once, one commit per item, no check, nothing after, ask before
//! a push. Cue's own project has Cue's setup (its check, then installed or served in place), and a folder that isn't a
//! git repository can only work in place, its work committed by you.
//!
//! Some choices are shown before Cue can do them (a branch, merge): they can't be picked, and say so. A check or an
//! after landing of your own is its steps, kept here too (`check_steps`, `after_steps`): run in order, the first that
//! fails stops the rest (`run_steps`). Cue's own check and install stay Cue's (its project's defaults); any other
//! project starts with no check and nothing after landing. A pull request and the push choices are built (desk/hub/landing/pr.rs). The Desk reads the rest as it works (desk/hub/landing/trees.rs,
//! desk/hub/landing/finish.rs, desk/hub/landing/commit.rs).

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
            c("command", "Your own steps", "Your steps run in order in a clean copy of main plus the item. The first that fails stops the rest, and nothing reaches main. They run as in your terminal. Each step gets up to 15 minutes and all of them 30 minutes together; anything a step leaves running in the background counts toward that time. Steps can't push: a push always asks you first.", true),
        ],
    },
    Part {
        key: "after",
        label: "After landing",
        choices: &[
            c("nothing", "Nothing", "Landed is the last step: an item is done when it's on main.", true),
            c("install", "Install or serve Cue", "Window-only changes are served in place, after the check. Others join the install every half hour.", true),
            c("command", "Your own steps", "Once the item is on main, your steps run in order in the project's main folder. The first that fails stops the rest, and the item waits for you. They run as in your terminal. Each step gets up to 15 minutes and all of them 30 minutes together; anything a step leaves running in the background counts toward that time. Steps can't push: a push always asks you first.", true),
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

/// The most steps a check or after landing of your own has.
pub const STEPS_MAX: usize = 10;

/// What goes between two steps in the lead's `LANDS: … | steps: <one> ;; <two>` (desk/parse.rs): never a step's own
/// `;` or `|`, so `npm test | tail -5` and `cd web; make` stay one step each.
pub const STEP_SEP: &str = " ;; ";

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
    /// The check's steps, when it's your own ("command").
    pub check_steps: Vec<String>,
    /// After landing's steps, when it's your own ("command").
    pub after_steps: Vec<String>,
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
        json!({ "where": self.at, "how": self.how, "check": self.check, "after": self.after, "push": self.push, "at_once": self.at_once,
            "check_steps": self.check_steps, "after_steps": self.after_steps })
    }
}

/// The defaults for a project in `place` (part 7).
pub fn defaults(place: Place) -> Lands {
    match place {
        Place::Cue => Lands { at: "copy", how: "squash", check: "cue", after: "install", push: "ask", at_once: AT_ONCE_DEFAULT, check_steps: vec![], after_steps: vec![] },
        Place::Repo => Lands { at: "copy", how: "squash", check: "none", after: "nothing", push: "ask", at_once: AT_ONCE_DEFAULT, check_steps: vec![], after_steps: vec![] },
        Place::NoRepo => Lands { at: "shared", how: "owner", check: "none", after: "nothing", push: "ask", at_once: AT_ONCE_DEFAULT, check_steps: vec![], after_steps: vec![] },
    }
}

/// The steps in `v` (a list of commands, or one per line), blank ones left out. None: not a list, or too many.
pub fn steps_in(v: &Value) -> Option<Vec<String>> {
    let all: Vec<String> = match v {
        Value::Array(a) => a.iter().map(|s| s.as_str().map(str::to_string)).collect::<Option<_>>()?,
        Value::String(s) => s.lines().map(str::to_string).collect(),
        _ => return None,
    };
    let out: Vec<String> = all.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    (out.len() <= STEPS_MAX).then_some(out)
}

/// How long one of your steps may run before it's stopped.
pub const STEP_CAP: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// How long all of a check's or an after landing's steps may run together, so one item can't hold the landing line
/// for hours.
pub const TOTAL_CAP: std::time::Duration = std::time::Duration::from_secs(30 * 60);

#[cfg(test)]
thread_local! {
    /// Tests: a shorter cap for the steps run on this thread.
    pub static TEST_STEP_CAP: std::cell::Cell<Option<std::time::Duration>> = const { std::cell::Cell::new(None) };
    /// Tests: a shorter cap for all the steps of one run on this thread.
    pub static TEST_TOTAL_CAP: std::cell::Cell<Option<std::time::Duration>> = const { std::cell::Cell::new(None) };
}

fn step_cap() -> std::time::Duration {
    #[cfg(test)]
    if let Some(cap) = TEST_STEP_CAP.with(|c| c.get()) {
        return cap;
    }
    STEP_CAP
}

fn total_cap() -> std::time::Duration {
    #[cfg(test)]
    if let Some(cap) = TEST_TOTAL_CAP.with(|c| c.get()) {
        return cap;
    }
    TOTAL_CAP
}

/// "15 minutes", "2s": how long a cap is, in words.
fn cap_words(cap: std::time::Duration) -> String {
    match cap.as_secs() {
        s if s >= 60 && s % 60 == 0 => format!("{} minute{}", s / 60, if s == 60 { "" } else { "s" }),
        s if s > 0 && cap.subsec_millis() == 0 => format!("{s}s"),
        _ => format!("{}ms", cap.as_millis()),
    }
}

/// The step of yours that stopped the rest: `why` it did ("failed", "took over 15 minutes and was stopped") and the
/// end of what it said.
#[derive(Debug)]
pub struct StepFailed {
    pub step: String,
    pub why: String,
    pub said: String,
}

/// Why `steps` can't be saved as your own steps: one pushes. A push always asks you first (desk/lands.rs `push`), so it never
/// goes in through steps.
pub fn refused_steps(steps: &[String]) -> Result<(), String> {
    if steps.iter().any(|s| pushes(s)) {
        return Err("Steps can't push; a push always asks you first".into());
    }
    Ok(())
}

/// Whether command line `step` runs a git push (`git push`, `git -C web push origin main`, after a `;` or `&&`).
fn pushes(step: &str) -> bool {
    let lower = step.to_ascii_lowercase();
    lower.split(|c: char| matches!(c, ';' | '&' | '|' | '\n' | '(' | ')' | '`')).any(|cmd| {
        let words: Vec<&str> = cmd.split_whitespace().map(|w| w.trim_matches(|c| c == '"' || c == '\'')).collect();
        words.iter().position(|w| *w == "git" || w.ends_with("/git")).is_some_and(|g| words[g + 1..].contains(&"push"))
    })
}

/// The shell your steps run with: your login shell, as in your terminal. Started from the Finder, Cue's own PATH is
/// short (no npm, no node); the login shell's is yours.
pub(crate) fn login_shell() -> String {
    std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into())
}

/// `steps` run in order in `dir`, each with your login shell (`$SHELL -lc`) in a process group of its own and at most
/// STEP_CAP, all of them together at most TOTAL_CAP (past either the group is killed), stopping at the first that
/// fails. A step done only when all it started that still hold its output are, so one left running in the background
/// counts toward the time. Takes as long as the steps do: never with a lock held.
pub fn run_steps(dir: &str, steps: &[String]) -> Result<(), StepFailed> {
    let (cap, total) = (step_cap(), total_cap());
    let until = std::time::Instant::now() + total;
    for step in steps {
        let fail = |why: String, said: String| StepFailed { step: step.clone(), why, said };
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err(fail(format!("didn't start: your steps took over {} in all", cap_words(total)), String::new()));
        }
        let this = cap.min(left);
        let mut cmd = std::process::Command::new(login_shell());
        cmd.arg("-lc").arg(step).current_dir(dir);
        match crate::terminal::focus::capped_group(&mut cmd, this) {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                let said = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
                let lines: Vec<&str> = said.lines().collect();
                return Err(fail("failed".into(), lines[lines.len().saturating_sub(40)..].join("\n")));
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                let why = if this < cap { format!("was stopped: your steps took over {} in all", cap_words(total)) } else { format!("took over {} and was stopped", cap_words(cap)) };
                return Err(fail(why, String::new()));
            }
            Err(e) => return Err(fail("failed".into(), format!("couldn't run it: {e}"))),
        }
    }
    Ok(())
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
        l.check_steps = s.get("check_steps").and_then(steps_in).unwrap_or_default();
        l.after_steps = s.get("after_steps").and_then(steps_in).unwrap_or_default();
    }
    // Your own steps with none saved: as if not picked.
    if l.check == "command" && l.check_steps.is_empty() {
        l.check = defaults(place).check;
    }
    if l.after == "command" && l.after_steps.is_empty() {
        l.after = defaults(place).after;
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
/// The first time `folder` is asked about it waits a moment for its lookup: call with no lock held (under the store's
/// lock, `lands_known`).
pub fn lands_of(project: &str, folder: &str) -> Lands {
    crate::logs::repos::first_look(folder);
    lands_known(project, folder)
}

/// `lands_of` from what's known of `folder` now, never waiting: for code that holds the store's lock.
pub fn lands_known(project: &str, folder: &str) -> Lands {
    of_in(&crate::config::load(), project, place_of(folder))
}

/// What runs before an item of a project lands.
#[derive(Debug, PartialEq)]
pub enum Check {
    /// Your own steps, in order.
    Steps(Vec<String>),
    /// The repository's own check, where it has one (Cue's); any other lands "not checked".
    Repo,
    /// Nothing: you picked None, and it lands "not checked".
    Off,
}

/// The check a landing of `project` in `repo` runs.
pub fn check_of(project: &str, repo: &str) -> Check {
    crate::logs::repos::first_look(repo);
    check_in(&crate::config::load(), project, place_of(repo))
}

/// `check_of`, read from config `c`. None turns the check off only when you picked it: not picked, the repository's own
/// runs where it has one, and any other lands "not checked" all the same, as the default says.
fn check_in(c: &Value, project: &str, place: Place) -> Check {
    let l = of_in(c, project, place);
    let picked_none = stored(c, project).and_then(|s| s.get("check")).and_then(Value::as_str) == Some("none");
    match l.check {
        "command" => Check::Steps(l.check_steps),
        "none" if picked_none => Check::Off,
        _ => Check::Repo,
    }
}

/// What `folder` (on this Mac) is. Read from what Cue already knows of the folder (logs/repos.rs, never a wait, so
/// never the disk under the store's lock): a folder not looked at yet counts as a repository until it is. Call
/// `repos::first_look` first to wait for it.
pub fn place_of(folder: &str) -> Place {
    if folder.is_empty() {
        return Place::Repo;
    }
    match crate::logs::repos::facts(folder) {
        Some(f) if f.cue => Place::Cue,
        Some(f) if f.top.is_none() => Place::NoRepo,
        _ => Place::Repo,
    }
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

/// What a Desk on `project` (its folder `folder`) shows of it (desk/hub/core/desk.rs desk_view): the setting now, its words for
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
    for (part, key, label) in [("check", "check_steps", "Check before landing"), ("after", "after_steps", "After landing")] {
        let steps = match value.get(key) {
            Some(v) => steps_in(v).ok_or(format!("{label}: at most {STEPS_MAX} steps"))?,
            None => vec![],
        };
        refused_steps(&steps).map_err(|why| format!("{label}: {why}"))?;
        if out.get(part).and_then(Value::as_str) == Some("command") {
            if steps.is_empty() {
                return Err(format!("{label}: give at least one step"));
            }
            out.insert(key.into(), json!(steps));
        }
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

/// The setting as the Desk's lead reads it in its <desk> block, with how to change it.
pub fn lead_line(l: &Lands) -> String {
    let label = |part: &str, key: &str| PARTS.iter().find(|p| p.key == part).and_then(|p| p.choices.iter().find(|c| c.key == key)).map_or(key.to_string(), |c| c.label.to_lowercase());
    let steps = |key: &str, steps: &[String]| if key == "command" { format!(" ({})", steps.join(STEP_SEP)) } else { String::new() };
    format!(
        "How work lands now (LANDS: to propose a change): where: {} · how: {} · check: {}{} · after: {}{} · push: {} · at once: {}\n",
        label("where", l.at), label("how", l.how), label("check", l.check), steps(l.check, &l.check_steps), label("after", l.after), steps(l.after, &l.after_steps), label("push", l.push), l.at_once
    )
}

/// The words a part goes by when the lead proposes a change to it, and in its question to you.
fn part_words(part: &str) -> &'static str {
    match part {
        "where" => "where an item works",
        "how" => "how it lands",
        "check" => "check",
        "after" => "after landing",
        "push" => "push to main",
        _ => "items at once",
    }
}

/// The lead's proposed change to `project`'s setting (`LANDS: <part> -> <choice>`, desk/parse.rs), its folder in
/// `place`: what would be saved (what you saved, with the change) and the change in plain words ("check → none").
/// Nothing is saved here: only your yes saves it (desk/hub/lead/lands.rs). A push after each landing is never the lead's to
/// pick: a push to main keeps asking unless you pick that yourself on the settings screen.
pub fn propose(c: &Value, project: &str, place: Place, part: &str, to: &str, steps: &[String]) -> Result<(Value, String), String> {
    let part = part.trim().to_ascii_lowercase().replace(['_', '-'], " ");
    let to = to.trim().to_ascii_lowercase();
    let mut set = stored(c, project).cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let words = if part == "at once" || part == "items at once" {
        let n: u64 = to.parse().map_err(|_| format!("items at once is a number, {} to {}", AT_ONCE.0, AT_ONCE.1))?;
        set["at_once"] = json!(n);
        format!("items at once → {n}")
    } else {
        let p = PARTS.iter().find(|p| p.key == part).ok_or(format!("{part} isn't a part: where, how, check, after, push or at once"))?;
        let ch = p.choices.iter().find(|c| c.key == to || c.label.eq_ignore_ascii_case(&to)).ok_or(format!("{to} isn't a choice for {part}"))?;
        if p.key == "push" && ch.key == "after" {
            return Err("a push after each landing is only picked by the user on the settings screen; a push to main keeps asking".into());
        }
        allowed(p.key, ch.key, place).map_err(|why| format!("{part}: {to} can't be picked ({why})"))?;
        set[p.key] = json!(ch.key);
        let mut w = format!("{} → {}", part_words(p.key), ch.label.to_lowercase().replace("cue's", "Cue's"));
        if ch.key == "command" {
            if steps.is_empty() {
                return Err(format!("{part}: give its steps (steps: <one>{STEP_SEP}<two>)"));
            }
            if steps.len() > STEPS_MAX {
                return Err(format!("{part}: at most {STEPS_MAX} steps"));
            }
            refused_steps(steps)?;
            set[format!("{}_steps", p.key)] = json!(steps);
            w = format!("{} → {}", part_words(p.key), steps.join(", then "));
        }
        // You commit and a pull request decide where an item works, as on the screen.
        match (p.key, ch.key) {
            ("how", "owner") => set["where"] = json!("shared"),
            ("how", "pr") => set["where"] = json!("copy"),
            ("where", at) if at != "shared" && set.get("how").and_then(Value::as_str) == Some("owner") => set["how"] = json!("squash"),
            ("where", "shared") if set.get("how").and_then(Value::as_str) == Some("pr") => set["how"] = json!("squash"),
            _ => {}
        }
        w
    };
    Ok((set, words))
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
                "check_steps" | "after_steps" => {
                    let steps = steps_in(v).ok_or(format!("{k}: at most {STEPS_MAX} steps"))?;
                    refused_steps(&steps)?;
                    true
                }
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
        assert_eq!(of_in(&none, "p", Place::Repo), Lands { at: "copy", how: "squash", check: "none", after: "nothing", push: "ask", at_once: 3, check_steps: vec![], after_steps: vec![] });
        assert_eq!(of_in(&none, "cue", Place::Cue), Lands { at: "copy", how: "squash", check: "cue", after: "install", push: "ask", at_once: 3, check_steps: vec![], after_steps: vec![] });
        let c = json!({ "project": { "lands": { "p": { "where": "copy", "how": "squash" } } } });
        assert_eq!(of_in(&c, "p", Place::NoRepo), Lands { at: "shared", how: "owner", check: "none", after: "nothing", push: "ask", at_once: 3, check_steps: vec![], after_steps: vec![] }, "a copy needs a repository");
    }

    #[test]
    fn what_cant_be_picked_says_why_in_plain_words() {
        assert_eq!(allowed("where", "branch", Place::Repo), Err(NOT_YET));
        assert_eq!(allowed("how", "merge", Place::Cue), Err(NOT_YET));
        assert_eq!(allowed("how", "pr", Place::Repo), Ok("pr"));
        assert_eq!(allowed("how", "pr", Place::NoRepo), Err("needs a git repository"));
        assert_eq!(allowed("push", "after", Place::Cue), Ok("after"));
        assert_eq!(allowed("check", "command", Place::Cue), Ok("command"));
        assert_eq!(allowed("after", "command", Place::Repo), Ok("command"));
        assert_eq!(allowed("check", "cue", Place::Repo), Err("only for Cue's own project"));
        assert_eq!(allowed("where", "copy", Place::NoRepo), Err("needs a git repository"));
        assert_eq!(allowed("how", "owner", Place::Repo), Ok("owner"));
    }

    #[test]
    fn a_saved_setting_holds_and_you_commit_means_the_shared_folder() {
        let c = json!({ "project": { "lands": { "p": { "where": "branch", "how": "owner", "check": "command", "at_once": 5, "push": "after" } } } });
        let l = of_in(&c, "p", Place::Repo);
        assert_eq!((l.at, l.how, l.check, l.at_once, l.push), ("shared", "owner", "none", 5, "after"), "a choice not built yet, or your own steps with none, is never in effect");
        let c = json!({ "project": { "lands": { "p": { "where": "shared", "how": "pr" } } } });
        assert_eq!(of_in(&c, "p", Place::Repo).how, "squash", "a pull request needs a copy");
        assert!(checked(&json!({ "p": { "where": "nowhere" } })).is_err());
        assert!(checked(&json!({ "p": { "at_once": 0 } })).is_err());
        assert!(checked(&json!({ "p": { "where": "shared", "at_once": 2 } })).is_ok());
    }

    #[test]
    fn cues_own_project_keeps_its_check_and_install_and_a_new_project_gets_neither() {
        let cue = of_in(&json!({}), "cue", Place::Cue);
        assert_eq!((cue.check, cue.after, cue.push), ("cue", "install", "ask"), "Cue's own setup, nothing saved");
        let new = of_in(&json!({}), "web", Place::Repo);
        assert_eq!((new.check, new.after, new.push), ("none", "nothing", "ask"), "no check, nothing after landing");
        assert!(new.check_steps.is_empty() && new.after_steps.is_empty());
        assert_eq!(allowed("after", "install", Place::Repo), Err("only for Cue's own project"));
    }

    #[test]
    fn your_own_steps_are_kept_in_cue_and_need_at_least_one() {
        let c = json!({ "project": { "lands": { "web": { "check": "command", "check_steps": ["npm test", " ", "npm run lint"], "after": "command" } } } });
        let l = of_in(&c, "web", Place::Repo);
        assert_eq!((l.check, l.check_steps.clone()), ("command", vec!["npm test".to_string(), "npm run lint".to_string()]));
        assert_eq!(l.after, "nothing", "after landing of your own with no steps is as if not picked");
        assert_eq!(steps_in(&json!("a\n\n b ")), Some(vec!["a".to_string(), "b".to_string()]));
        assert!(checked(&json!({ "web": { "check_steps": ["a"], "after_steps": "b" } })).is_ok());
        assert!(checked(&json!({ "web": { "check_steps": 3 } })).is_err());
    }

    #[test]
    fn steps_run_in_order_and_stop_at_the_first_that_fails() {
        let dir = std::env::temp_dir().join(format!("cue-lands-steps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().to_string();
        let steps: Vec<String> = ["echo one >> ran", "echo two >> ran; echo it broke; exit 3", "echo three >> ran"].map(String::from).to_vec();
        let e = run_steps(&d, &steps).unwrap_err();
        assert_eq!((e.step.as_str(), e.why.as_str()), (steps[1].as_str(), "failed"));
        assert!(e.said.contains("it broke"), "{}", e.said);
        assert_eq!(std::fs::read_to_string(dir.join("ran")).unwrap(), "one\ntwo\n", "the third never ran");
        assert!(run_steps(&d, &steps[..1]).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_step_that_hangs_is_stopped_with_all_it_started() {
        let dir = std::env::temp_dir().join(format!("cue-lands-hang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().to_string();
        TEST_STEP_CAP.with(|c| c.set(Some(std::time::Duration::from_secs(1))));
        let t = std::time::Instant::now();
        // A child of the step's shell that holds its output: stopped with it, and the next step never runs.
        let steps: Vec<String> = ["(sleep 2; echo late >> ran) & sleep 30", "echo next >> ran"].map(String::from).to_vec();
        let e = run_steps(&d, &steps).unwrap_err();
        TEST_STEP_CAP.with(|c| c.set(None));
        assert!(t.elapsed() < std::time::Duration::from_secs(5), "{:?}", t.elapsed());
        assert_eq!((e.step.as_str(), e.why.as_str()), (steps[0].as_str(), "took over 1s and was stopped"));
        std::thread::sleep(std::time::Duration::from_millis(2500));
        assert!(!dir.join("ran").exists(), "nothing it started is left running, and the next step didn't run");
        assert_eq!(cap_words(STEP_CAP), "15 minutes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn all_the_steps_together_are_stopped_past_the_total_time() {
        let dir = std::env::temp_dir().join(format!("cue-lands-total-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().to_string();
        TEST_TOTAL_CAP.with(|c| c.set(Some(std::time::Duration::from_millis(1500))));
        // Each step is well under its own cap; a step left running in the background counts too.
        let steps: Vec<String> = ["sleep 1", "(sleep 5) & echo started", "echo never >> ran"].map(String::from).to_vec();
        let t = std::time::Instant::now();
        let e = run_steps(&d, &steps).unwrap_err();
        TEST_TOTAL_CAP.with(|c| c.set(None));
        assert!(t.elapsed() < std::time::Duration::from_secs(4), "{:?}", t.elapsed());
        assert_eq!((e.step.as_str(), e.why.as_str()), (steps[1].as_str(), "was stopped: your steps took over 1500ms in all"));
        assert!(!dir.join("ran").exists());
        assert_eq!(cap_words(TOTAL_CAP), "30 minutes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn steps_run_with_your_login_shell() {
        let dir = std::env::temp_dir().join(format!("cue-lands-shell-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().to_string();
        // A pipe and a `;` in one step, as a shell reads them.
        assert!(run_steps(&d, &["echo one; echo two | tr a-z A-Z > said".to_string()]).is_ok());
        assert_eq!(std::fs::read_to_string(dir.join("said")).unwrap(), "TWO\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn steps_that_push_are_refused_everywhere_they_could_go_in() {
        for s in ["git push", "npm test && git push origin main", "git -C web push", "make; /usr/bin/git  push --force"] {
            assert_eq!(refused_steps(&[s.to_string()]).unwrap_err(), "Steps can't push; a push always asks you first", "{s}");
        }
        for s in ["git pull", "npm test", "echo push", "git status; echo git"] {
            assert!(refused_steps(&[s.to_string()]).is_ok(), "{s}");
        }
        let c = json!({});
        assert_eq!(propose(&c, "web", Place::Repo, "after", "command", &["make".into(), "git push".into()]).unwrap_err(), "Steps can't push; a push always asks you first");
        assert!(checked(&json!({ "web": { "after": "command", "after_steps": ["git push"] } })).is_err(), "nor through config");
    }

    #[test]
    fn one_way_decides_the_check() {
        let c = json!({ "project": { "lands": { "web": { "check": "command", "check_steps": ["npm test"] }, "off": { "check": "none" } } } });
        assert_eq!(check_in(&c, "web", Place::Repo), Check::Steps(vec!["npm test".into()]));
        assert_eq!(check_in(&c, "off", Place::Cue), Check::Off, "None picked: off, Cue's own project too");
        assert_eq!(check_in(&c, "new", Place::Cue), Check::Repo, "not picked: the repository's own");
        assert_eq!(check_in(&json!({ "project": { "lands": { "p": { "check": "command" } } } }), "p", Place::Repo), Check::Repo, "no steps: as if not picked");
    }

    #[test]
    fn the_leads_proposal_says_the_change_and_never_turns_off_the_push_ask() {
        let c = json!({ "project": { "lands": { "cue": { "at_once": 4 } } } });
        let (set, words) = propose(&c, "cue", Place::Cue, "check", "none", &[]).unwrap();
        assert_eq!(words, "check → none");
        assert_eq!(set, json!({ "at_once": 4, "check": "none" }), "what you saved stays, with the change");
        let (set, words) = propose(&c, "web", Place::Repo, "after", "command", &["make deploy".into()]).unwrap();
        assert_eq!(words, "after landing → make deploy");
        assert_eq!(set["after_steps"], json!(["make deploy"]));
        assert!(propose(&c, "cue", Place::Cue, "push", "after", &[]).unwrap_err().contains("keeps asking"));
        assert!(propose(&c, "cue", Place::Cue, "check", "command", &[]).is_err(), "your own steps need steps");
        assert!(propose(&c, "web", Place::Repo, "after", "install", &[]).is_err(), "only for Cue's own project");
        assert_eq!(propose(&c, "cue", Place::Cue, "at once", "2", &[]).unwrap().1, "items at once → 2");
    }
}
