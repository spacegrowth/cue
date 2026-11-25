//! How work lands: per project, the six parts of the setting in docs/desk-landing-design.md (where an item works, how
//! it lands, the check, after landing, push, items at once). Kept in config.json as `project.lands`: project → the
//! parts you saved, next to Ask me before (desk/one_way.rs). Never in the project's files. A project not there has the
//! defaults (part 7): its own copy per item, three at once, one commit per item, no check, nothing after, ask before
//! a push. Cue's own project has Cue's setup (its check, then the app's build, install and restart as named steps after
//! landing, `APP_STEPS`), and a folder that isn't a git repository can only work in place, its work committed by you.
//!
//! A choice shown before Cue can do it can't be picked, and says so (none now). A check or an
//! after landing of your own is its steps, kept here too (`check_steps`, `after_steps`): run in order, the first that
//! fails stops the rest (`run_steps`); a step is named by "[Name]" at its line's start. Cue's own check stays Cue's; any other
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
            c("branch", "A branch", "Each item works on its own branch in the project's own folder, one item at a time. Cue makes the branch when the item starts and removes it once the item is on main.", true),
        ],
    },
    Part {
        key: "how",
        label: "How it lands",
        choices: &[
            c("squash", "One commit per item", "The Desk puts each item on main as one commit, named after the item. One at a time.", true),
            c("merge", "Merge", "The Desk puts each item on main as a merge commit, named after the item, with the item's own commits kept under it. One at a time.", true),
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
            c("command", "Your own steps", "Once the item is on main, your steps run in order in the project's main folder. Start a line with [Name] to name its step. The first that fails stops the rest, and the item waits for you. They run as in your terminal. Each step gets up to 15 minutes and all of them 30 minutes together; anything a step leaves running in the background counts toward that time. Steps can't push: a push always asks you first.", true),
        ],
    },
    Part {
        key: "push",
        label: "Send finished work to GitHub",
        choices: &[
            c("after", "Right away", "Each finished item goes to GitHub as soon as it lands, without asking, so GitHub and your other computers stay current. Nothing there is ever overwritten.", true),
            c("ask", "Ask me first", "You're asked first. Sending keeps GitHub and your other computers current.", true),
            c("never", "Never", "Nothing is sent. GitHub and your other computers get the work only when you push it yourself.", true),
        ],
    },
    // What the Desk's sessions are told of testing in every item's brief (#780; desk/mod.rs `tests_rule`). No words
    // under it: the labels say it, and the screen stays short enough not to scroll (#447).
    Part {
        key: "tests",
        label: "Tests",
        choices: &[
            c("all", "All tests", "", true),
            c("changed", "Only what changed", "", true),
            c("changed_full", "Changed, full run at landing", "", true),
        ],
    },
    // When an item queued after another (after:) starts (#808; desk/hub/turn/route.rs `release`): once the one before
    // has landed (on main, its steps after landing done) or once you've said Looks right on it. No words under it, as
    // Tests (#447).
    Part {
        key: "waits",
        label: "Items waiting on another start",
        choices: &[
            c("landed", "When it lands", "", true),
            c("checked", "When you've checked it", "", true),
        ],
    },
];

/// After landing's old choice, before it was steps: read as the app's built-in steps (`app_steps`).
const OLD_INSTALL: &str = "install";

/// "Working at once", as a build saved it while it was a setting of its own (#798): read once as Items at once.
const OLD_WORKING: &str = "working";

/// Items at once: the most of a project's items at work on this Mac together (#798: in copies of their own, on a
/// branch, or in the shared folder alike), and in copy mode the most copies held.
pub const AT_ONCE: (u32, u32) = (1, 9);
/// Items at once when not set.
pub const AT_ONCE_DEFAULT: u32 = 3;

/// The most steps a check or after landing of your own has.
pub const STEPS_MAX: usize = 10;

/// What goes between two steps in the lead's `LANDS: … | steps: <one> ;; <two>` (desk/parse.rs): never a step's own
/// `;` or `|`, so `npm test | tail -5` and `cd web; make` stay one step each.
pub const STEP_SEP: &str = " ;; ";

/// A step's name, when its line starts with one in brackets ("[Build] npm run build"); else the step is named by its
/// command. A shell's `[ -f x ]` (a space after the bracket) is a command, not a name.
pub fn step_name(step: &str) -> &str {
    named(step).map_or(step.trim(), |(n, _)| n)
}

/// What a step runs: its line without its name.
pub fn step_run(step: &str) -> &str {
    named(step).map_or(step.trim(), |(_, r)| r)
}

fn named(step: &str) -> Option<(&str, &str)> {
    let s = step.trim().strip_prefix('[')?;
    if s.starts_with(char::is_whitespace) {
        return None;
    }
    let (name, run) = s.split_once(']')?;
    let (name, run) = (name.trim(), run.trim());
    (!name.is_empty() && !run.is_empty()).then_some((name, run))
}

/// The steps after landing that update the app you're running from its own source, built in: what each runs, its
/// default name, and the updater's word for it while it runs (desk/installer.rs). They run together, in this order,
/// as the last steps: gathered for the next update (every half hour, or Update now), the app built from a clean copy
/// at main (with the add-on when it's there), put in place and started, the one before put back if the new one
/// doesn't start. Only a project whose folder is the running app's own source can have them (`Place::Cue`).
pub const APP_STEPS: [(&str, &str, &str); 3] = [("app:build", "Build", "building"), ("app:install", "Install", "installing"), ("app:restart", "Restart", "starting")];

/// Whether step `s` is one of the app's built-in steps.
pub fn app_step(s: &str) -> bool {
    APP_STEPS.iter().any(|(run, _, _)| step_run(s) == *run)
}

/// The app's built-in steps as a Desk's after landing has them by default.
pub fn app_steps() -> Vec<String> {
    APP_STEPS.iter().map(|(run, name, _)| format!("[{name}] {run}")).collect()
}

/// Why `steps` can't be after landing's steps in `place`: the app's built-in steps outside its own source, or not all
/// three, in order, at the end.
fn refused_app_steps(steps: &[String], place: Place) -> Result<(), String> {
    let at: Vec<usize> = (0..steps.len()).filter(|k| app_step(&steps[*k])).collect();
    if at.is_empty() {
        return Ok(());
    }
    if place != Place::Cue {
        return Err("the app's own build, install and restart are only for the app's own source".into());
    }
    let runs: Vec<&str> = at.iter().map(|k| step_run(&steps[*k])).collect();
    let want: Vec<&str> = APP_STEPS.iter().map(|(r, _, _)| *r).collect();
    if runs != want || at.first() != Some(&(steps.len() - 3)) {
        return Err(format!("{} go together, in that order, as the last steps", want.join(", ")));
    }
    Ok(())
}

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
    /// What its sessions test (#780): "all", "changed" or "changed_full".
    pub tests: &'static str,
    /// When an item waiting on one of its items starts (#808): "landed" or "checked".
    pub waits: &'static str,
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
            "tests" => self.tests,
            "waits" => self.waits,
            _ => self.push,
        }
    }

    fn set(&mut self, part: &str, key: &'static str) {
        match part {
            "where" => self.at = key,
            "how" => self.how = key,
            "check" => self.check = key,
            "after" => self.after = key,
            "tests" => self.tests = key,
            "waits" => self.waits = key,
            _ => self.push = key,
        }
    }

    pub fn to_json(&self) -> Value {
        let names: Map<String, Value> = APP_STEPS.iter().filter_map(|(_, _, word)| self.app_step_name(word).map(|n| (word.to_string(), json!(n)))).collect();
        json!({ "where": self.at, "how": self.how, "check": self.check, "after": self.after, "push": self.push, "tests": self.tests, "waits": self.waits, "at_once": self.at_once,
            "check_steps": self.check_steps, "after_steps": self.after_steps, "after_names": names })
    }

    /// Whether after landing updates the app you're running (its built-in steps are among the steps).
    pub fn updates_app(&self) -> bool {
        self.after == "command" && self.after_steps.iter().any(|s| app_step(s))
    }

    /// After landing's own commands, run as each item lands (the app's built-in steps left out: they run together
    /// later).
    pub fn plain_after(&self) -> Vec<String> {
        if self.after != "command" {
            return vec![];
        }
        self.after_steps.iter().filter(|s| !app_step(s)).cloned().collect()
    }

    /// The name of the built-in step the updater calls `word` ("building"), as this Desk names it.
    pub fn app_step_name(&self, word: &str) -> Option<String> {
        let run = APP_STEPS.iter().find(|(_, _, w)| *w == word)?.0;
        self.updates_app().then(|| self.after_steps.iter().find(|s| step_run(s) == run).map(|s| step_name(s).to_string())).flatten()
    }
}

/// The defaults for a project in `place` (part 7). Tests: every test for a new Desk; Cue's own source only what changed,
/// its check running the whole suite at landing (#780). An item waiting on another starts once that one has landed,
/// on every Desk (#808).
pub fn defaults(place: Place) -> Lands {
    match place {
        Place::Cue => Lands { at: "copy", how: "squash", check: "cue", after: "command", push: "ask", tests: "changed", waits: "landed", at_once: AT_ONCE_DEFAULT, check_steps: vec![], after_steps: app_steps() },
        Place::Repo => Lands { at: "copy", how: "squash", check: "none", after: "nothing", push: "ask", tests: "all", waits: "landed", at_once: AT_ONCE_DEFAULT, check_steps: vec![], after_steps: vec![] },
        Place::NoRepo => Lands { at: "shared", how: "owner", check: "none", after: "nothing", push: "ask", tests: "all", waits: "landed", at_once: AT_ONCE_DEFAULT, check_steps: vec![], after_steps: vec![] },
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
    run_steps_seen(dir, steps, &|_| {})
}

/// `run_steps`, with `seen` told each step as it begins (the item's row says which runs, now_line.rs).
pub fn run_steps_seen(dir: &str, steps: &[String], seen: &dyn Fn(&str)) -> Result<(), StepFailed> {
    let (cap, total) = (step_cap(), total_cap());
    let until = std::time::Instant::now() + total;
    for step in steps {
        seen(step);
        let fail = |why: String, said: String| StepFailed { step: step_name(step).to_string(), why, said };
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err(fail(format!("didn't start: your steps took over {} in all", cap_words(total)), String::new()));
        }
        let this = cap.min(left);
        let mut cmd = std::process::Command::new(login_shell());
        cmd.arg("-lc").arg(step_run(step)).current_dir(dir);
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
        ("check", "cue", p) if p != Place::Cue => Err("only for Cue's own project"),
        ("where", "copy" | "branch", Place::NoRepo) | ("how", "squash" | "merge" | "pr", Place::NoRepo) => Err("needs a git repository"),
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
        // A "Working at once" saved by a build that had it as a setting of its own (#798): read as this Mac's number,
        // unless one is saved; the next save keeps only that.
        let old = s.get(OLD_WORKING).and_then(Value::as_str).and_then(|w| w.parse::<u64>().ok());
        if let Some(n) = s.get("at_once").and_then(Value::as_u64).or(old).filter(|n| (AT_ONCE.0 as u64..=AT_ONCE.1 as u64).contains(n)) {
            l.at_once = n as u32;
        }
        l.check_steps = s.get("check_steps").and_then(steps_in).unwrap_or_default();
        // After landing's steps as saved; none saved, the defaults' (the app's built-in steps on its own source).
        if let Some(steps) = s.get("after_steps").and_then(steps_in) {
            l.after_steps = steps;
        }
        // Saved before after landing was steps: "install" is the app's built-in steps.
        if s.get("after").and_then(Value::as_str) == Some(OLD_INSTALL) && place == Place::Cue {
            (l.after, l.after_steps) = ("command", app_steps());
        }
        // The app's built-in steps where they can't be: as if not there.
        if refused_app_steps(&l.after_steps, place).is_err() {
            l.after_steps.retain(|s| !app_step(s));
        }
    }
    // Your own steps with none saved: as if not picked.
    if l.check == "command" && l.check_steps.is_empty() {
        l.check = defaults(place).check;
    }
    if l.after == "command" && l.after_steps.is_empty() {
        (l.after, l.after_steps) = (defaults(place).after, defaults(place).after_steps);
    }
    // Nothing after landing: no steps kept with it.
    if l.after != "command" {
        l.after_steps.clear();
    }
    // You commit: there's no copy for it to land from.
    if l.how == "owner" {
        l.at = "shared";
    }
    // Merge: its commits are on a branch of its own (a copy or a branch); in the shared folder there are none.
    if l.how == "merge" && l.at == "shared" {
        l.how = defaults(place).how;
    }
    // A pull request: its branch is the item's own copy's, so there's none in the shared folder, nor on a branch
    // there (the folder would wait on its branch till the pull request is merged).
    if l.how == "pr" && l.at != "copy" {
        l.how = defaults(place).how;
    }
    l
}

/// What Desk `project`'s sessions test (#780), its items working in `folder` (`cue`: a copy of Cue's own source, as
/// noted when it was made, so its default is Cue's whether or not the folder was looked at yet): from the store's
/// lock too (config is held in memory).
pub fn tests_of(project: &str, folder: &str, cue: bool) -> &'static str {
    of_in(&crate::config::load(), project, if cue { Place::Cue } else { place_of(folder) }).tests
}

/// What you saved for `project` in config `c` (None: never set).
/// The key, among a project's settings, of the machine its full run goes on (Cue's own project only).
const FULL_RUN_ON: &str = "full_run_on";

/// The machine (as Cue names it) every test of `project` runs on after landing; "": this machine
/// (desk/hub/landing/full_run.rs).
pub(crate) fn full_run_on(project: &str) -> String {
    stored(&crate::config::load(), project).and_then(|s| s.get(FULL_RUN_ON)).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

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

/// How work lands for `project` whose folder was just found on disk to be the running app's own source (`Place::Cue`),
/// whatever the folder's facts, looked at earlier, say. Reads no folder: safe with any lock held.
pub fn app_source_lands(project: &str) -> Lands {
    of_in(&crate::config::load(), project, Place::Cue)
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
        ("how", "merge") => "merge",
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
        if part == "after" {
            refused_app_steps(&steps, place).map_err(|why| format!("{label}: {why}"))?;
        } else if steps.iter().any(|s| app_step(s)) {
            return Err(format!("{label}: the app's own build, install and restart go after landing"));
        }
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
    if out.get("how").and_then(Value::as_str) == Some("merge") && out.get("where").and_then(Value::as_str) == Some("shared") {
        return Err("Merge needs each item in its own copy or on a branch".into());
    }
    crate::config::update("project.lands", |c| {
        let mut map = c.pointer("/project/lands").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        // Where its full run goes isn't on the screen: kept as it is unless given.
        if let Some(on) = value.get(FULL_RUN_ON).or_else(|| map.get(project).and_then(|s| s.get(FULL_RUN_ON))).and_then(Value::as_str).map(str::trim).filter(|o| !o.is_empty()) {
            out.insert(FULL_RUN_ON.into(), json!(on));
        }
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
        "How work lands now (LANDS: to propose a change): where: {} · how: {} · check: {}{} · after: {}{} · push: {} · tests: {} · waiting items start: {} · at once: {}\n",
        label("where", l.at), label("how", l.how), label("check", l.check), steps(l.check, &l.check_steps), label("after", l.after), steps(l.after, &l.after_steps), label("push", l.push), label("tests", l.tests), label("waits", l.waits), l.at_once
    )
}

/// The words a part goes by when the lead proposes a change to it, and in its question to you.
fn part_words(part: &str) -> &'static str {
    match part {
        "where" => "where an item works",
        "how" => "how it lands",
        "check" => "check",
        "after" => "after landing",
        "push" => "send finished work to GitHub",
        "tests" => "tests",
        "waits" => "items waiting on another start",
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
        let p = PARTS.iter().find(|p| p.key == part).ok_or(format!("{part} isn't a part: where, how, check, after, push, tests, waits or at once"))?;
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
            if p.key == "after" {
                refused_app_steps(steps, place)?;
            } else if steps.iter().any(|s| app_step(s)) {
                return Err("the app's own build, install and restart go after landing".into());
            }
            set[format!("{}_steps", p.key)] = json!(steps);
            w = format!("{} → {}", part_words(p.key), steps.iter().map(|s| step_name(s)).collect::<Vec<_>>().join(", then "));
        }
        // You commit, a pull request and merge decide where an item works, as on the screen.
        match (p.key, ch.key) {
            ("how", "owner") => set["where"] = json!("shared"),
            ("how", "pr") => set["where"] = json!("copy"),
            ("how", "merge") if set.get("where").and_then(Value::as_str).is_none_or(|w| w == "shared") => set["where"] = json!("copy"),
            ("where", at) if at != "shared" && set.get("how").and_then(Value::as_str) == Some("owner") => set["how"] = json!("squash"),
            ("where", "shared") if matches!(set.get("how").and_then(Value::as_str), Some("pr" | "merge")) => set["how"] = json!("squash"),
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
                "full_run_on" => v.is_string(),
                OLD_WORKING => v.is_string(),
                "at_once" => v.as_u64().is_some_and(|n| (AT_ONCE.0 as u64..=AT_ONCE.1 as u64).contains(&n)),
                // Saved before after landing was steps: kept, and read as the app's built-in steps.
                "after" if v.as_str() == Some(OLD_INSTALL) => true,
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
#[path = "lands_tests.rs"]
mod tests;
