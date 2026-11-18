//! Ask me before: per project, the few kinds of step you want to say yes to first. Asked once, as a project's Desk
//! gets its first lead (the window's Ask me before dialog), and kept in config.json as `project.oneway`: project → its
//! kinds' keys; `project.asked` lists the projects whose answer was given. A project not in `project.oneway` has the
//! default, the rule from before there was a question: a push to main (or master).
//!
//! Not a policy engine: a short fixed list of known kinds. The ones Cue can see in a tool call (`offered`) are checked
//! from the command's own words (a step inside a script isn't seen) on every session of a project with a Desk: the agent's PreToolUse hook (hook.rs `guard`) and Pi's
//! extension stop the call and Cue asks you (hub/guard.rs). The others can't be told from a command's words, so they
//! aren't offered; one carried over from before still reaches the sessions as words. A crew commit on main is still
//! marked one-way on its crew log (sessions/crew_log.rs; its Undo says "Make a revert").

use serde_json::{json, Value};

/// One kind of step: its key in config, its words in the window, and as the sessions' instructions say it.
/// `offered`: Cue checks it before the call runs, so the window offers it; else `why` it can't.
pub struct Kind {
    pub key: &'static str,
    pub label: &'static str,
    pub ask: &'static str,
    pub offered: bool,
    pub why: &'static str,
}

/// The kinds, in the order the window shows them and the sessions are told them.
pub const KINDS: &[Kind] = &[
    Kind { key: "main", label: "Pushing to main", ask: "a push to main or master", offered: true, why: "" },
    Kind { key: "push", label: "Pushing any branch", ask: "a push of any branch", offered: true, why: "" },
    Kind { key: "release", label: "Releasing", ask: "a release (scripts/release.sh, a tag, a package publish)", offered: true, why: "" },
    Kind { key: "deploy", label: "Deploying", ask: "a deploy", offered: false, why: "a deploy can be any command, so Cue can't tell one apart" },
    Kind { key: "data", label: "Deleting files", ask: "deleting files or data (rm, git rm, git clean, a dropped table)", offered: true, why: "" },
    Kind { key: "outside", label: "Files outside the project", ask: "changing files outside this project's folder", offered: false, why: "a command can write anywhere, so Cue can't see which files it changes" },
];

/// A project never set: a push to main.
pub const DEFAULT: &[&str] = &["main"];

/// What the question ticks for a project never set.
pub const ASK_DEFAULT: &[&str] = &["main", "release", "data"];

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

/// Whether project `project` was asked (its answer given, or the question put off) in config `c`.
pub fn asked_in(c: &Value, project: &str) -> bool {
    c.pointer("/project/asked").and_then(Value::as_array).is_some_and(|l| l.iter().any(|v| v.as_str() == Some(project)))
}

/// What the question ticks for `project` in config `c`: what it was set to before (One-way here, carried over), else
/// ASK_DEFAULT.
pub fn question_in(c: &Value, project: &str) -> Vec<&'static str> {
    match c.pointer("/project/oneway").and_then(|m| m.get(project)) {
        Some(_) => kinds_in(c, project),
        None => ASK_DEFAULT.to_vec(),
    }
}

/// Whether a crew commit on `branch` is one-way in a project with `kinds`: a commit on main or master when a push to
/// main is one of them (its Undo then makes a revert).
pub fn commit_one_way(kinds: &[&str], branch: &str) -> bool {
    matches!(branch, "main" | "master") && kinds.contains(&"main")
}

/// Whether a shell command an agent asks to run is one of `kinds`, read from its words alone (the current branch
/// unknown): the tests' short form of `call_hits`.
#[cfg(test)]
pub fn command_one_way(kinds: &[&str], command: &str) -> bool {
    hits(command, &|_| None).iter().any(|k| kinds.contains(k))
}

/// The kinds a tool call is, whatever is set: its command (`command`, a string or an argument list), or a patch that
/// deletes a file (Codex's apply_patch). `branch(dir)`: the branch checked out in the session's folder, or in `dir`
/// under it (`git -C dir`), when the caller can look (the hook, on the session's own machine); None when it can't.
pub fn call_hits(tool_input: &Value, branch: &dyn Fn(Option<&str>) -> Option<String>) -> Vec<&'static str> {
    let text = match tool_input.get("command") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => {
            let args: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
            // A shell given its script (`bash -lc "rm -rf dist"`): the script, read as a command line.
            match args.as_slice() {
                [sh, flag, script] if SHELLS.contains(&sh.rsplit('/').next().unwrap_or(sh)) && flag.starts_with('-') && flag.ends_with('c') => script.to_string(),
                _ => args.join(" "),
            }
        }
        _ => ["input", "patch"].iter().find_map(|k| tool_input.get(*k).and_then(Value::as_str)).unwrap_or("").to_string(),
    };
    let mut out = hits(&text, branch);
    if text.contains("*** Delete File:") && !out.contains(&"data") {
        out.push("data");
    }
    in_order(out)
}

/// The kinds a shell command is: a git push (`push`), one to main or master (`main`), a release (`release`), or one
/// that deletes files or data (`data`). Each part of it (split at `&&`, `||`, `;`, `|` and new lines) is read alone.
pub fn hits(command: &str, branch: &dyn Fn(Option<&str>) -> Option<String>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = vec![];
    for part in command.split(['&', '|', ';', '\n', '(', ')', '`']).map(str::trim).filter(|p| !p.is_empty()) {
        let w: Vec<&str> = part.split_whitespace().map(|x| x.trim_matches(|c| c == '"' || c == '\'' || c == '{' || c == '}')).filter(|x| !x.is_empty()).collect();
        for k in part_hits(part, &w, branch) {
            if !out.contains(&k) {
                out.push(k);
            }
        }
    }
    in_order(out)
}

fn in_order(list: Vec<&'static str>) -> Vec<&'static str> {
    KINDS.iter().map(|k| k.key).filter(|k| list.contains(k)).collect()
}

/// The words that only start a command: `sudo rm`, `env X=1 rm`, `xargs rm`, `find … -exec rm`.
const PREFIX: &[&str] = &["sudo", "env", "nohup", "time", "command", "exec", "xargs", "-exec", "-execdir", "doas"];

/// The shells a script is run by.
const SHELLS: &[&str] = &["sh", "bash", "zsh"];

/// Whether word `i` of a command is where a command starts: first, or after a prefix (sudo, xargs, find's -exec),
/// settings (`FOO=1`) and options.
fn at_start(w: &[&str], i: usize) -> bool {
    i == 0 || PREFIX.contains(&w[i - 1]) || w[..i].iter().all(|y| y.contains('=') || PREFIX.contains(y) || y.starts_with('-'))
}

/// The commands that delete files.
const DELETERS: &[&str] = &["rm", "rmdir", "unlink", "shred", "trash", "srm"];

/// Where a deleted file is only scratch: never asked about.
fn scratch(path: &str) -> bool {
    ["/tmp/", "/private/tmp/", "/var/folders/", "$TMPDIR", "${TMPDIR"].iter().any(|p| path.starts_with(p))
}

fn part_hits(part: &str, w: &[&str], branch: &dyn Fn(Option<&str>) -> Option<String>) -> Vec<&'static str> {
    let mut out = vec![];
    // Deleting: a deleter where a command starts (after sudo, env's settings, xargs, find's -exec), not of scratch.
    for (i, x) in w.iter().enumerate() {
        let name = x.rsplit('/').next().unwrap_or(x);
        if at_start(w, i) && DELETERS.contains(&name) {
            let targets: Vec<&&str> = w[i + 1..].iter().take_while(|y| !PREFIX.contains(y)).filter(|y| !y.starts_with('-')).collect();
            if targets.is_empty() || !targets.iter().all(|t| scratch(t)) {
                out.push("data");
            }
        }
    }
    if w.contains(&"find") && w.contains(&"-delete") {
        out.push("data");
    }
    let lower = part.to_lowercase();
    if ["drop table", "drop database", "drop schema", "truncate table", "delete from"].iter().any(|s| lower.contains(s)) {
        out.push("data");
    }
    // A release: the project's release script, a GitHub release, a package published.
    // The script run, itself or by a shell (`bash scripts/release.sh`), not named (`cat scripts/release.sh`).
    let run = |i: usize| at_start(w, i) || (i > 0 && SHELLS.contains(&w[i - 1]) && at_start(w, i - 1));
    if w.iter().enumerate().any(|(i, x)| x.rsplit('/').next() == Some("release.sh") && run(i)) {
        out.push("release");
    }
    if w.windows(3).any(|p| p[0] == "gh" && p[1] == "release" && matches!(p[2], "create" | "upload" | "edit" | "delete")) {
        out.push("release");
    }
    // A pull request merged on GitHub: into its base branch, main as a rule.
    if w.windows(3).any(|p| p[0] == "gh" && p[1] == "pr" && p[2] == "merge") {
        out.extend(["push", "main"]);
    }
    if w.windows(2).any(|p| matches!(p[0], "npm" | "pnpm" | "yarn" | "cargo" | "twine" | "gem" | "poetry" | "flit") && matches!(p[1], "publish" | "upload")) {
        out.push("release");
    }
    if let Some(g) = w.iter().position(|x| *x == "git" || x.ends_with("/git")) {
        out.extend(git_hits(&w[g + 1..], branch));
    }
    out
}

/// A git command's kinds: its subcommand found past git's own options (`-C <dir>`, `-c <key=value>`).
fn git_hits(git: &[&str], branch: &dyn Fn(Option<&str>) -> Option<String>) -> Vec<&'static str> {
    let (mut i, mut dir) = (0, None);
    while i < git.len() {
        match git[i] {
            "-C" => {
                dir = git.get(i + 1).copied();
                i += 2;
            }
            "-c" | "--git-dir" | "--work-tree" | "--namespace" => i += 2,
            x if x.starts_with('-') => i += 1,
            _ => break,
        }
    }
    let Some(sub) = git.get(i) else { return vec![] };
    let args = &git[i + 1..];
    let opt = |names: &[&str]| args.iter().any(|a| names.contains(a));
    // A short flag, alone or in a bundle (`-fdx` holds f).
    let short = |c: char| args.iter().any(|a| a.starts_with('-') && !a.starts_with("--") && a.contains(c));
    // The options of this subcommand that take the next word as their value (`-u` is a flag on push, a key on tag).
    let valued: &[&str] = match *sub {
        "push" => &["-o", "--push-option", "--repo", "--receive-pack", "--exec"],
        "tag" => &["-m", "-F", "-u", "--message", "--file", "--local-user", "--cleanup", "--format", "--sort", "--color", "--column", "--contains", "--no-contains", "--merged", "--no-merged", "--points-at"],
        _ => &[],
    };
    let positional = || -> Vec<&str> {
        let mut out = vec![];
        let mut j = 0;
        while j < args.len() {
            let a = args[j];
            if valued.contains(&a) {
                j += 2;
                continue;
            }
            if !a.starts_with('-') {
                out.push(a);
            }
            j += 1;
        }
        out
    };
    match *sub {
        "push" => {
            // The branch checked out, when the push names none: unknown (a detached head, git can't say) counts as main.
            let here_main = || branch(dir).is_none_or(|b| matches!(b.as_str(), "main" | "master"));
            let refs: Vec<&str> = positional().into_iter().skip(1).collect();
            push_hits(&refs, opt(&["--all", "--mirror"]), opt(&["--tags", "--follow-tags"]), &here_main)
        }
        // A tag made (not listed, read or deleted).
        "tag" if !opt(&["-l", "--list", "-d", "--delete", "-v", "--verify", "-n", "--contains", "--points-at"]) && !positional().is_empty() => vec!["release"],
        "rm" if !opt(&["--cached", "-n", "--dry-run"]) => vec!["data"],
        // A branch deleted whether merged or not (`-d` only deletes one that's merged, so nothing is lost).
        // Short flags may come bundled (`-Df`, `-fd`).
        "branch" if short('D') || ((opt(&["--delete"]) || short('d')) && (opt(&["--force"]) || short('f'))) => vec!["data"],
        "clean" if !opt(&["-n", "--dry-run"]) && (opt(&["--force"]) || short('f')) => vec!["data"],
        _ => vec![],
    }
}

/// A push's kinds: a push; to main when it names main (or HEAD, or nothing, on main: `here_main`) or pushes `all`;
/// a release when it pushes `tags` or names one.
fn push_hits(refs: &[&str], all: bool, tags: bool, here_main: &dyn Fn() -> bool) -> Vec<&'static str> {
    let main_ref = |r: &str| {
        let r = r.trim_start_matches('+');
        let dst = r.rsplit(':').next().unwrap_or(r).trim_start_matches("refs/heads/");
        if dst == "HEAD" || dst.is_empty() {
            return here_main();
        }
        matches!(dst, "main" | "master")
    };
    let mut out = vec!["push"];
    if all || if refs.is_empty() { here_main() } else { refs.iter().any(|r| main_ref(r)) } {
        out.push("main");
    }
    if tags || refs.iter().any(|r| r.contains("refs/tags/") || version_tag(r)) {
        out.push("release");
    }
    out
}

/// A ref pushed by a bare name that reads as a version tag (`v1.2.3`, `1.4`, `v2`): a release. Other bare tag
/// names can't be told from a branch by the words alone.
fn version_tag(r: &str) -> bool {
    let name = r.trim_start_matches('+');
    let name = name.rsplit(':').next().unwrap_or(name);
    let (v, n) = match name.strip_prefix('v') {
        Some(n) => (true, n),
        None => (false, name),
    };
    // Numbers and dots, then at most one `-rc.1` / `+build` part with no further dashes.
    let (core, extra) = n.split_once(['-', '+']).unwrap_or((n, ""));
    let numbers = !core.is_empty() && core.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    numbers && (v || core.contains('.')) && extra.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
}

/// The line a session's instructions get for a project with `kinds` (team_out.rs, hosted.rs, a Desk's hand-off):
/// what it asks the user first before. "" when nothing's set there.
pub fn ask_first(kinds: &[&str]) -> String {
    match listed(kinds) {
        None => String::new(),
        Some(list) => format!("- Ask me before, as the user set it for this project: ask the user first, and end your turn, before {list}."),
    }
}

/// `kinds` as the sessions are told them: "a push to main or master, a release …, or deleting files …".
fn listed(kinds: &[&str]) -> Option<String> {
    let said: Vec<&str> = KINDS.iter().filter(|k| kinds.contains(&k.key)).map(|k| k.ask).collect();
    Some(match said.as_slice() {
        [] => return None,
        [one] => one.to_string(),
        [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
    })
}

/// What a project's Desk sessions are sent when its setting changes: the new rule, said as replacing whatever an
/// earlier one said (a session's instructions may still hold the old one).
pub fn changed_words(kinds: &[&str]) -> String {
    let checked = KINDS.iter().any(|k| k.offered && kinds.contains(&k.key));
    match listed(kinds) {
        None => "Ask me before, changed by the user just now: nothing is set for this project any more. This replaces any earlier Ask me before or One-way rule you were given. No reply needed.".into(),
        Some(list) => format!(
            "Ask me before, changed by the user just now: ask the user first, and end your turn, before {list}. This replaces any earlier Ask me before or One-way rule you were given.{} Tell your crew if you have one. No reply needed.",
            if checked { " Cue also stops these and asks the user itself." } else { "" }
        ),
    }
}

/// `project.oneway` as it's saved: each project's keys, known ones only, once each, in KINDS's order (an empty list
/// is kept: nothing set there, which isn't the default).
pub fn checked(value: &Value) -> Result<Value, String> {
    let map = value.as_object().ok_or("ask me before must be project: [kinds]")?;
    let mut out = serde_json::Map::new();
    for (p, list) in map {
        let list = list.as_array().ok_or("a project's kinds are a list")?;
        if let Some(bad) = list.iter().find(|v| !v.as_str().is_some_and(|s| KINDS.iter().any(|k| k.key == s))) {
            return Err(format!("{bad} isn't a kind of step"));
        }
        let keys: Vec<&str> = KINDS.iter().map(|k| k.key).filter(|k| list.iter().any(|v| v.as_str() == Some(k))).collect();
        out.insert(p.clone(), json!(keys));
    }
    Ok(Value::Object(out))
}

/// `project.asked` as it's saved: project names, once each.
pub fn checked_asked(value: &Value) -> Result<Value, String> {
    let list = value.as_array().ok_or("asked must be a list of projects")?;
    let mut out: Vec<&str> = vec![];
    for v in list {
        let p = v.as_str().ok_or("a project is a name")?;
        if !p.is_empty() && !out.contains(&p) {
            out.push(p);
        }
    }
    Ok(json!(out))
}

/// What the window gets (config.rs `effective`): each project you set, the projects asked, the kinds with their words
/// (offered or why not), the default and what the question ticks.
pub fn view(c: &Value) -> Value {
    let set = c.pointer("/project/oneway").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let asked = c.pointer("/project/asked").cloned().filter(Value::is_array).unwrap_or_else(|| json!([]));
    let kinds: Vec<Value> = KINDS.iter().map(|k| json!({ "key": k.key, "label": k.label, "offered": k.offered, "why": k.why })).collect();
    json!({ "set": set, "asked": asked, "kinds": kinds, "default": DEFAULT, "ask_default": ASK_DEFAULT })
}

/// What a Desk on `project` shows of it (hub/desk.rs desk_view): its kinds (with their words), whether it was asked
/// yet (not: the window asks, once the Desk has a lead), and what the question ticks.
pub fn desk_view(project: &str) -> Value {
    let c = crate::config::load();
    let kinds: Vec<Value> = kinds_in(&c, project).into_iter().filter_map(|k| KINDS.iter().find(|x| x.key == k)).map(|k| json!({ "key": k.key, "label": k.label })).collect();
    json!({ "project": project, "kinds": kinds, "asked": asked_in(&c, project), "ticks": question_in(&c, project) })
}

/// Project `project`'s answer: its kinds saved (only known ones, those not offered kept as they were) and the project
/// counted as asked, in one save. Returns the kinds as saved.
pub fn answer(project: &str, kinds: &[String]) -> Result<Vec<&'static str>, String> {
    if project.trim().is_empty() {
        return Err("no project".into());
    }
    if let Some(bad) = kinds.iter().find(|k| !KINDS.iter().any(|x| x.key == k.as_str())) {
        return Err(format!("{bad} isn't a kind of step"));
    }
    let mut saved = vec![];
    crate::config::update("project.oneway", |c| {
        let before = kinds_in(c, project);
        // A kind the window doesn't offer can't be ticked there: what it was is kept.
        let keep = |k: &Kind| if k.offered { kinds.iter().any(|x| x == k.key) } else { before.contains(&k.key) };
        saved = KINDS.iter().filter(|k| keep(k)).map(|k| k.key).collect();
        let mut map = c.pointer("/project/oneway").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        map[project] = json!(saved);
        Some(map)
    })?;
    mark_asked(project)?;
    Ok(saved)
}

/// Project `project` counted as asked: the question isn't put again.
pub fn mark_asked(project: &str) -> Result<(), String> {
    crate::config::update("project.asked", |c| {
        if asked_in(c, project) {
            return None;
        }
        let mut list = c.pointer("/project/asked").cloned().filter(Value::is_array).unwrap_or_else(|| json!([]));
        list.as_array_mut()?.push(json!(project));
        Some(list)
    })
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: Option<&str>) -> Option<String> {
        None
    }

    #[test]
    fn a_project_you_never_set_counts_a_push_to_main() {
        let c = json!({ "project": { "oneway": { "api": ["push", "deploy"], "site": [] } } });
        assert_eq!(kinds_in(&c, "web"), ["main"]);
        assert_eq!(kinds_in(&c, "api"), ["push", "deploy"]);
        assert!(kinds_in(&c, "site").is_empty(), "nothing checked is nothing, not the default");
        assert_eq!(kinds_in(&json!({}), "web"), ["main"]);
    }

    #[test]
    fn the_question_carries_over_one_way_here_and_ticks_three_for_a_project_never_set() {
        let c = json!({ "project": { "oneway": { "api": ["push", "deploy"], "site": [] }, "asked": ["site"] } });
        assert_eq!(question_in(&c, "api"), ["push", "deploy"], "the old setting, carried over as it was");
        assert!(question_in(&c, "site").is_empty(), "nothing checked before: nothing ticked");
        assert_eq!(question_in(&c, "web"), ["main", "release", "data"], "never set: the three");
        assert!(asked_in(&c, "site") && !asked_in(&c, "api"));
        assert_eq!(checked_asked(&json!(["a", "a", "", "b"])).unwrap(), json!(["a", "b"]));
        assert!(checked_asked(&json!({ "a": 1 })).is_err());
    }

    #[test]
    fn a_commit_is_one_way_from_the_setting() {
        assert!(commit_one_way(DEFAULT, "main") && commit_one_way(DEFAULT, "master"));
        assert!(!commit_one_way(DEFAULT, "crew/rows"));
        assert!(!commit_one_way(&[], "main"), "a push to main is fine here: its commit isn't one-way");
        assert!(!commit_one_way(&["push", "release"], "main"));
    }

    #[test]
    fn a_push_to_main_is_seen_in_its_refspec_or_the_branch_checked_out() {
        let on = |b: &'static str| move |_: Option<&str>| Some(b.to_string());
        assert_eq!(hits("cd api && git push origin HEAD:main", &none), ["main", "push"]);
        assert_eq!(hits("git -C web push origin master", &none), ["main", "push"]);
        assert_eq!(hits("git push origin +feature:refs/heads/main", &none), ["main", "push"]);
        assert_eq!(hits("git push --all origin", &none), ["main", "push"]);
        assert_eq!(hits("git push origin crew/rows", &none), ["push"], "a crew branch isn't main");
        assert_eq!(hits("git push", &on("main")), ["main", "push"], "a plain push on main");
        assert_eq!(hits("git push -u origin HEAD", &on("master")), ["main", "push"]);
        assert_eq!(hits("git push", &on("feature")), ["push"]);
        assert_eq!(hits("git push", &none), ["main", "push"], "the branch unknown (a detached head, git can't say): main");
        assert_eq!(hits("git push origin HEAD", &none), ["main", "push"]);
        // -u is a flag on push, not an option with a value: the refspec after the remote is still read.
        assert_eq!(hits("git push -u origin main", &on("feature")), ["main", "push"], "from a feature branch, to main");
        assert_eq!(hits("git push --set-upstream origin main", &on("feature")), ["main", "push"]);
        assert_eq!(hits("git push -u origin feature", &on("feature")), ["push"]);
        assert_eq!(hits("git push -o ci.skip origin feature", &on("main")), ["push"], "-o takes a value");
        // A pull request merged on GitHub goes into main.
        assert_eq!(hits("gh pr merge 12 --squash", &on("feature")), ["main", "push"]);
        let asked = std::cell::RefCell::new(vec![]);
        let _ = hits("git -C ../web push", &|d| { asked.borrow_mut().push(d.map(str::to_string)); None });
        assert_eq!(*asked.borrow(), [Some("../web".to_string())], "the branch looked up where -C says");
        // A local merge or commit on main isn't a push (finishing at DONE commits there).
        assert!(hits("git merge crew/hub main", &on("main")).is_empty());
        assert!(hits("git commit -m 'fix main'", &on("main")).is_empty());
        assert!(hits("git log main..crew/rows", &none).is_empty(), "reading main changes nothing");
        assert!(hits("npm test", &none).is_empty());
    }

    #[test]
    fn a_release_is_its_script_a_tag_a_github_release_or_a_publish() {
        for c in ["scripts/release.sh 0.2.0 \"notes\" --publish", "./release.sh", "bash scripts/release.sh 1.0", "git tag v1.2.0", "git tag -a v1 -m 'one'", "gh release create v1", "npm publish --access public", "cargo publish", "twine upload dist/*", "git push origin --tags", "git push origin v1.2.3", "git push origin 0.4.1", "git push origin refs/tags/nightly", "git tag -u key v1"] {
            assert!(hits(c, &none).contains(&"release"), "{c}");
        }
        for c in ["git tag", "git tag -l", "git tag -d v1", "gh release list", "cat scripts/release.sh", "grep release scripts", "git push origin v2-ui-fixes", "git push origin feature"] {
            assert!(!hits(c, &none).contains(&"release"), "{c}");
        }
    }

    #[test]
    fn deleting_files_is_rm_git_rm_git_clean_find_delete_or_a_dropped_table() {
        for c in ["rm -rf build", "sudo rm file", "rm a.txt /tmp/b", "find . -name '*.o' -delete", "find . -exec rm {} ;", "ls | xargs rm", "git rm ui/old.js", "git clean -fdx", "unlink x", "/bin/rm x", "psql -c 'DROP TABLE users'", "sqlite3 db 'delete from rows'", "FOO=1 rm x", "git branch -D old", "git branch --delete --force old", "git branch -Df old", "git branch -fd old", "git branch -df old", "git branch -d -f old"] {
            assert_eq!(hits(c, &none), ["data"], "{c}");
        }
        for c in ["rm -rf /tmp/cue-test", "rm \"$TMPDIR/x\"", "git rm --cached x", "git clean -n", "git branch -d merged", "git branch -f topic main", "grep -r rm .", "echo rmdir", "npm run format", "cargo build --release"] {
            assert!(hits(c, &none).is_empty(), "{c}");
        }
        assert_eq!(call_hits(&json!({ "command": ["bash", "-lc", "rm -rf dist"] }), &none), ["data"], "Codex's argument list");
        assert_eq!(call_hits(&json!({ "input": "*** Begin Patch\n*** Delete File: a.rs\n*** End Patch" }), &none), ["data"], "a patch that deletes");
        assert!(call_hits(&json!({ "file_path": "a.rs", "content": "rm -rf" }), &none).is_empty(), "a file's text isn't a command");
    }

    #[test]
    fn a_command_is_one_way_from_its_words_and_the_setting() {
        assert!(command_one_way(DEFAULT, "cd api && git push origin HEAD:main"));
        assert!(!command_one_way(DEFAULT, "git merge crew/hub main"), "a local merge isn't a push");
        assert!(!command_one_way(DEFAULT, "git push origin crew/rows"));
        assert!(!command_one_way(&[], "git push origin main"), "nothing set here");
        assert!(command_one_way(&["push"], "npm test; git push"));
        assert!(command_one_way(&["release"], "npm publish --access public") && command_one_way(&["release"], "gh release create v1"));
        assert!(command_one_way(&["data"], "rm -rf src") && !command_one_way(&["main"], "rm -rf src"));
    }

    #[test]
    fn the_sessions_are_told_what_to_ask_first_before_and_a_change_replaces_it() {
        assert_eq!(ask_first(DEFAULT), "- Ask me before, as the user set it for this project: ask the user first, and end your turn, before a push to main or master.");
        let two = ask_first(&["deploy", "push"]);
        assert!(two.contains("before a push of any branch, or a deploy."), "in the list's order: {two}");
        assert_eq!(ask_first(&[]), "");
        let said = changed_words(&["main", "data"]);
        assert!(said.contains("before a push to main or master, or deleting files") && said.contains("replaces any earlier") && said.contains("Cue also stops these"), "{said}");
        assert!(!changed_words(&["deploy"]).contains("Cue also stops"), "only words: Cue can't check a deploy");
        assert!(changed_words(&[]).contains("nothing is set") && changed_words(&[]).contains("replaces any earlier"));
    }

    #[test]
    fn only_what_cue_checks_is_offered_and_each_other_says_why() {
        let offered: Vec<&str> = KINDS.iter().filter(|k| k.offered).map(|k| k.key).collect();
        assert_eq!(offered, ["main", "push", "release", "data"]);
        assert!(KINDS.iter().all(|k| k.offered == k.why.is_empty()));
        assert!(ASK_DEFAULT.iter().all(|k| offered.contains(k)));
    }

    #[test]
    fn the_setting_keeps_known_kinds_once_in_order() {
        let saved = checked(&json!({ "api": ["deploy", "main", "deploy"], "site": [] })).unwrap();
        assert_eq!(saved, json!({ "api": ["main", "deploy"], "site": [] }));
        assert!(checked(&json!({ "api": ["rm -rf"] })).is_err());
        assert!(checked(&json!({ "api": "main" })).is_err());
        assert!(checked(&json!(["main"])).is_err());
        let v = view(&json!({ "project": { "oneway": { "api": ["push"] }, "asked": ["api"] } }));
        assert_eq!((v["set"]["api"][0].as_str(), v["kinds"].as_array().map(Vec::len), v["default"][0].as_str()), (Some("push"), Some(KINDS.len()), Some("main")));
        assert_eq!((v["asked"][0].as_str(), v["ask_default"].as_array().map(Vec::len), v["kinds"][3]["offered"].as_bool()), (Some("api"), Some(3), Some(false)));
    }
}
