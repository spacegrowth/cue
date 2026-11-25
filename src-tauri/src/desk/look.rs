//! New Desk's look at a folder (#927): how a project builds, checks and lands, worked out by Cue itself from its
//! files, GitHub, its last 30 changes on main and its past sessions (desk/look_sessions.rs), then turned into the
//! rows the New Desk sheet shows and a full setting for desk/lands.rs `save`. It only reads: nothing is written
//! anywhere, and nothing that changes a repository or GitHub is run.
//!
//! `look` answers one part at a time (the window asks all four at once and ticks each as it comes); `rows` takes the
//! four answers back and decides. Files win over past sessions when the two disagree.

use crate::desk::lands::{self, Place, STEPS_MAX};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The most a call to `gh` may take before GitHub counts as out of reach.
const GH_CAP: Duration = Duration::from_secs(6);
/// How many changes on main are read.
const HISTORY_TAKE: usize = 30;

/// `folder` with a leading `~` made the home folder, no trailing slash.
pub fn expand(folder: &str) -> String {
    let f = folder.trim();
    let f = if f.len() > 1 { f.trim_end_matches('/') } else { f };
    match (f.strip_prefix('~'), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}{rest}"),
        _ => f.to_string(),
    }
}

/// `folder` as you'd say it: the home folder as `~`.
fn said(folder: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && folder.starts_with(&home) => format!("~{}", &folder[home.len()..]),
        _ => folder.to_string(),
    }
}

/// One line of the ticked list, naming `file` when there is one.
fn line(text: impl Into<String>, file: Option<&str>) -> Value {
    match file {
        Some(f) => json!({ "text": text.into(), "file": f }),
        None => json!({ "text": text.into() }),
    }
}

/// Part `part` of the look at `folder`: { part, lines, found }. `names` gives Cue's name for a past session by its
/// id (sessions only). Err when there's no such folder or no such part.
pub fn look(folder: &str, part: &str, names: &dyn Fn(&str) -> Option<String>) -> Result<Value, String> {
    let dir = expand(folder);
    if dir.is_empty() || !Path::new(&dir).is_dir() {
        return Err(format!("No folder at {}", said(&dir)));
    }
    let (lines, found) = match part {
        "files" => files(&dir),
        "github" => github(&dir),
        "history" => history(&dir),
        "sessions" => crate::desk::look_sessions::sessions(&crate::desk::look_sessions::projects_root(), &dir, names),
        _ => return Err(format!("not a part of the look: {part}")),
    };
    Ok(json!({ "part": part, "lines": lines, "found": found }))
}

// ---------------------------------------------------------------- what a command is for

/// What a command line is for: "test", "lint", "typecheck", "check", "build" or "deploy"; None for anything else
/// (installing what a project needs, moving about, printing).
pub fn kind_of(cmd: &str) -> Option<&'static str> {
    let lower = cmd.trim().to_ascii_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let first = *words.first()?;
    if !runs_something(first) {
        return None;
    }
    let second = words.get(1).copied().unwrap_or("");
    if matches!(first, "cd" | "echo" | "export" | "ls" | "cat" | "mkdir" | "rm" | "cp" | "mv" | "source" | "set" | "sudo" | "apt" | "apt-get" | "brew" | "curl" | "wget" | "rustup" | "git" | "gh")
        || (matches!(first, "npm" | "pnpm" | "yarn" | "bun") && matches!(second, "ci" | "install" | "i" | "add" | "audit"))
        || (matches!(first, "pip" | "pip3" | "uv" | "poetry") && matches!(second, "install" | "sync" | "add"))
        || (first == "cargo" && second == "install")
        || (matches!(first, "bash" | "sh" | "zsh") && second == "-n")
        || (first == "go" && matches!(second, "get" | "mod"))
    {
        return None;
    }
    // A flag (`--release`, `--check`) never says what a command is for: only its names do.
    let has = |pred: &dyn Fn(&str) -> bool| words.iter().filter(|w| !w.starts_with('-')).map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != ':' && c != '.' && c != '/')).any(|w| pred(w));
    // Release, deploy and publish only as the program or script run: the first word, the one after it (`fly deploy`,
    // `make release`, `sh deploy.sh`), or the one after `run` (`npm run release`), never an argument of another
    // command (`cargo test deploy_tests`).
    let named: Vec<&str> = {
        let plain: Vec<&str> = words.iter().copied().filter(|w| !w.starts_with('-')).collect();
        let mut n: Vec<&str> = plain.iter().take(2).copied().collect();
        if let Some(i) = plain.iter().position(|w| *w == "run") {
            n.extend(plain.get(i + 1).copied());
        }
        n
    };
    let deploys = |w: &str| {
        let w = w.rsplit('/').next().unwrap_or(w);
        w.contains("deploy") || w.contains("release") || w.contains("publish") || w == "install.sh"
    };
    if named.iter().any(|w| deploys(w)) {
        return Some("deploy");
    }
    if has(&|w| w.contains("test") || w == "jest" || w == "vitest") {
        return Some("test");
    }
    if has(&|w| w.contains("lint") || w == "clippy" || w == "ruff" || w == "flake8" || w == "fmt" || w == "prettier" || w == "eslint") {
        return Some("lint");
    }
    if has(&|w| w == "tsc" || w.contains("typecheck") || w.contains("type-check") || w == "mypy" || w == "pyright") {
        return Some("typecheck");
    }
    if has(&|w| w == "check" || w == "vet") {
        return Some("check");
    }
    if has(&|w| w == "build" || w.starts_with("build:")) {
        return Some("build");
    }
    None
}

/// Whether a command starting with `first` runs a project's tool or script (not a search, an edit or a print).
fn runs_something(first: &str) -> bool {
    const TOOLS: &[&str] = &[
        "npm", "npx", "pnpm", "pnpx", "yarn", "bun", "bunx", "deno", "node", "cargo", "go", "make", "just", "gradle", "mvn", "pytest", "python", "python3", "uv", "poetry", "tox", "nox", "tsc", "eslint",
        "prettier", "ruff", "flake8", "mypy", "pyright", "vitest", "jest", "playwright", "swift", "xcodebuild", "flutter", "dart", "dotnet", "mix", "rake", "bundle", "composer", "cmake", "ctest", "ninja",
        "bash", "sh", "zsh", "docker", "fly", "flyctl", "vercel", "netlify", "wrangler", "firebase", "terraform", "kubectl", "helm",
    ];
    TOOLS.contains(&first) || first.starts_with("./") || first.starts_with("../") || first.ends_with(".sh") || first.ends_with("/gradlew") || first == "gradlew"
}

/// The word in a step's label: "[Lint] npm run lint".
fn label_of(kind: &str) -> &'static str {
    match kind {
        "test" => "Test",
        "lint" => "Lint",
        "typecheck" => "Types",
        "check" => "Check",
        "build" => "Build",
        _ => "Run",
    }
}

/// `cmd` as a row shows it: "[Lint] npm run lint".
fn labelled(kind: &str, cmd: &str) -> String {
    format!("[{}] {cmd}", label_of(kind))
}

// ---------------------------------------------------------------- files

/// One command a file names: what it's for, the line to run, the file, what in it said so, and the script's own
/// words when it's a script (`body`).
fn cmd(kind: &str, run: &str, file: &str, what: &str, body: Option<&str>) -> Value {
    let mut v = json!({ "kind": kind, "cmd": run, "file": file, "what": what });
    if let Some(b) = body {
        v["body"] = json!(b);
    }
    v
}

fn read(dir: &str, rel: &str) -> Option<String> {
    let p = Path::new(dir).join(rel);
    let meta = std::fs::metadata(&p).ok()?;
    if !meta.is_file() || meta.len() > 2_000_000 {
        return None;
    }
    std::fs::read_to_string(p).ok()
}

/// How `dir`'s package.json scripts are run: npm, or pnpm, yarn, bun when its lock file says so.
fn runner(dir: &str) -> &'static str {
    let has = |f: &str| Path::new(dir).join(f).is_file();
    if has("pnpm-lock.yaml") {
        "pnpm"
    } else if has("yarn.lock") {
        "yarn"
    } else if has("bun.lockb") || has("bun.lock") {
        "bun"
    } else {
        "npm"
    }
}

/// What the project's files say: { commands: [cmd], ci: [{ file, steps: [cmd], on_main }], scripts: [file] }.
pub fn files(dir: &str) -> (Vec<Value>, Value) {
    let mut lines = vec![];
    let mut commands: Vec<Value> = vec![];
    let mut ci: Vec<Value> = vec![];
    let mut scripts: Vec<String> = vec![];
    let mut any_build_file = false;

    if let Some(text) = read(dir, "package.json") {
        any_build_file = true;
        let pkg: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let run = runner(dir);
        let mut named = vec![];
        if let Some(s) = pkg.get("scripts").and_then(Value::as_object) {
            for name in ["build", "test", "lint", "check", "typecheck"] {
                let Some(body) = s.get(name).and_then(Value::as_str) else { continue };
                if name == "test" && body.contains("no test specified") {
                    continue;
                }
                let kind = match name {
                    "check" => kind_of(body).filter(|k| *k != "deploy").unwrap_or("check"),
                    n => n,
                };
                let line = if name == "test" && run != "bun" { format!("{run} test") } else if run == "yarn" { format!("yarn {name}") } else { format!("{run} run {name}") };
                commands.push(cmd(kind, &line, "package.json", &format!("its {name} script"), Some(body)));
                named.push(name);
            }
        }
        lines.push(line(if named.is_empty() { "Read package.json · no build or test scripts".into() } else { format!("Read package.json · {} scripts", list_words(&named)) }, Some("package.json")));
    }

    let make = ["Makefile", "makefile", "GNUmakefile"].into_iter().find_map(|f| read(dir, f).map(|t| (f, t)));
    match make {
        Some((f, text)) => {
            any_build_file = true;
            let targets = targets_in(&text, true);
            let mut named = vec![];
            for t in ["build", "test", "lint", "check"] {
                if targets.iter().any(|x| x == t) {
                    commands.push(cmd(kind_of(&format!("make {t}")).unwrap_or(t), &format!("make {t}"), f, &format!("its {t} target"), None));
                    named.push(t);
                }
            }
            lines.push(line(if named.is_empty() { format!("Read {f} · no build or test targets") } else { format!("Read {f} · {} targets", list_words(&named)) }, Some(f)));
        }
        None => lines.push(line("No Makefile", Some("Makefile"))),
    }

    if let Some((f, text)) = ["justfile", "Justfile", ".justfile"].into_iter().find_map(|f| read(dir, f).map(|t| (f, t))) {
        any_build_file = true;
        let targets = targets_in(&text, false);
        let mut named = vec![];
        for t in ["build", "test", "lint", "check"] {
            if targets.iter().any(|x| x == t) {
                commands.push(cmd(kind_of(&format!("just {t}")).unwrap_or(t), &format!("just {t}"), f, &format!("its {t} recipe"), None));
                named.push(t);
            }
        }
        lines.push(line(format!("Read {f} · {}", if named.is_empty() { "no build or test recipes".into() } else { format!("{} recipes", list_words(&named)) }), Some(f)));
    }

    // Cargo.toml at the top, or one folder down (an app's Rust beside its window).
    let cargo = std::iter::once(String::new()).chain(subfolders(dir)).find(|sub| Path::new(dir).join(sub).join("Cargo.toml").is_file());
    if let Some(sub) = cargo {
        any_build_file = true;
        let file = if sub.is_empty() { "Cargo.toml".to_string() } else { format!("{sub}/Cargo.toml") };
        let at = if sub.is_empty() { String::new() } else { format!("cd {sub} && ") };
        commands.push(cmd("build", &format!("{at}cargo build"), &file, "it's a Rust crate", None));
        commands.push(cmd("test", &format!("{at}cargo test"), &file, "it's a Rust crate", None));
        lines.push(line(format!("Read {file} · cargo build, cargo test"), Some(&file)));
    }
    if read(dir, "go.mod").is_some() {
        any_build_file = true;
        commands.push(cmd("build", "go build ./...", "go.mod", "it's a Go module", None));
        commands.push(cmd("test", "go test ./...", "go.mod", "it's a Go module", None));
        lines.push(line("Read go.mod · go build, go test", Some("go.mod")));
    }
    if let Some(text) = read(dir, "pyproject.toml") {
        any_build_file = true;
        let mut said = vec![];
        if text.contains("pytest") {
            commands.push(cmd("test", "pytest", "pyproject.toml", "it uses pytest", None));
            said.push("pytest");
        }
        if text.contains("[tool.ruff") {
            commands.push(cmd("lint", "ruff check .", "pyproject.toml", "it sets up ruff", None));
            said.push("ruff");
        }
        if text.contains("[tool.mypy") {
            commands.push(cmd("typecheck", "mypy .", "pyproject.toml", "it sets up mypy", None));
            said.push("mypy");
        }
        lines.push(line(format!("Read pyproject.toml{}", if said.is_empty() { String::new() } else { format!(" · {}", said.join(", ")) }), Some("pyproject.toml")));
    }
    if !any_build_file {
        lines.push(line("No package.json, Cargo.toml, go.mod or pyproject.toml", None));
    }

    // CI.
    let wf = Path::new(dir).join(".github/workflows");
    let mut wfs: Vec<String> = std::fs::read_dir(&wf).map(|r| r.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".yml") || n.ends_with(".yaml")).collect()).unwrap_or_default();
    wfs.sort();
    let mut ci_files = vec![];
    for n in wfs.into_iter().map(|n| format!(".github/workflows/{n}")).chain([".gitlab-ci.yml".to_string(), ".circleci/config.yml".to_string()]) {
        let Some(text) = read(dir, &n) else { continue };
        let steps: Vec<Value> = runs_in(&text).into_iter().filter_map(|r| kind_of(&r).map(|k| json!({ "kind": k, "cmd": r }))).collect();
        let on_main = on_main(&text);
        ci_files.push(n.clone());
        ci.push(json!({ "file": n, "steps": steps, "on_main": on_main }));
    }
    match ci_files.len() {
        0 => lines.push(line("No CI workflows", Some(".github/workflows"))),
        _ => {
            for f in &ci_files {
                let c = ci.iter().find(|c| c["file"] == f.as_str()).unwrap();
                let k = c["steps"].as_array().map_or(0, Vec::len);
                lines.push(line(format!("Read {f} · {k} step{}", if k == 1 { "" } else { "s" }), Some(f)));
            }
        }
    }

    // Release, deploy, install, publish scripts.
    for sub in ["scripts", "bin", ""] {
        let d = Path::new(dir).join(sub);
        let mut names: Vec<String> = std::fs::read_dir(&d).map(|r| r.filter_map(Result::ok).filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
        names.sort();
        for n in names {
            let stem = n.split('.').next().unwrap_or("").to_ascii_lowercase();
            let script = !sub.is_empty() || n.ends_with(".sh");
            if script && ["release", "deploy", "install", "publish"].iter().any(|w| stem.contains(w)) {
                scripts.push(if sub.is_empty() { n } else { format!("{sub}/{n}") });
            }
        }
    }
    if !scripts.is_empty() {
        lines.push(line(format!("Found {}", scripts.join(", ")), Some(&scripts[0])));
    }

    // README's fenced build and test lines.
    if let Some((f, text)) = ["README.md", "readme.md", "README"].into_iter().find_map(|f| read(dir, f).map(|t| (f, t))) {
        let mut n = 0;
        for l in fenced_lines(&text) {
            if let Some(k) = kind_of(&l).filter(|k| *k != "deploy") {
                if !commands.iter().any(|c| c["cmd"] == l.as_str()) {
                    commands.push(cmd(k, &l, f, "its instructions", None));
                    n += 1;
                }
            }
        }
        lines.push(line(if n == 0 { format!("Read {f} · no build or test lines") } else { format!("Read {f} · {n} build or test line{}", if n == 1 { "" } else { "s" }) }, Some(f)));
    }

    (lines, json!({ "commands": commands, "ci": ci, "scripts": scripts }))
}

/// "build, test and lint".
fn list_words(w: &[&str]) -> String {
    match w {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{}, {last}", rest.join(", ")),
    }
}

/// `dir`'s folders one down that aren't hidden or a dependency store, in name order.
fn subfolders(dir: &str) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|r| r.filter_map(Result::ok).filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| !n.starts_with('.') && !matches!(n.as_str(), "node_modules" | "target" | "vendor" | "dist")).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// A Makefile's targets (`name:` at the start of a line), or a justfile's recipes (`name args:`).
fn targets_in(text: &str, make: bool) -> Vec<String> {
    let mut out = vec![];
    for l in text.lines() {
        if l.starts_with(char::is_whitespace) || l.starts_with('#') || l.starts_with('.') {
            continue;
        }
        let Some((head, rest)) = l.split_once(':') else { continue };
        if rest.starts_with('=') {
            continue;
        }
        let names: Vec<&str> = if make { head.split_whitespace().collect() } else { head.split_whitespace().take(1).collect() };
        for n in names {
            if n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                out.push(n.to_string());
            }
        }
    }
    out
}

/// A CI file's commands: GitHub's `run:` (one line, or a `|` block), CircleCI's `command:`, GitLab's `script:` lists.
fn runs_in(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = vec![];
    let mut i = 0;
    let indent = |l: &str| l.len() - l.trim_start().len();
    while i < lines.len() {
        let l = lines[i];
        let t = l.trim_start().trim_start_matches("- ").trim_start();
        let key = ["run:", "command:", "script:"].into_iter().find(|k| t.starts_with(k));
        if let Some(k) = key {
            let rest = t[k.len()..].trim();
            let base = indent(l);
            if rest.is_empty() || rest.starts_with('|') || rest.starts_with('>') {
                // A block: the lines under it, or a list under script:.
                let mut j = i + 1;
                while j < lines.len() && (lines[j].trim().is_empty() || indent(lines[j]) > base || (k == "script:" && indent(lines[j]) == base && lines[j].trim_start().starts_with("- "))) {
                    let s = lines[j].trim().trim_start_matches("- ").trim();
                    if !s.is_empty() && !s.starts_with('#') {
                        out.push(unquote(s));
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            if !rest.starts_with('[') {
                out.push(unquote(rest));
            }
        }
        i += 1;
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let quoted = s.len() >= 2 && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')));
    (if quoted { &s[1..s.len() - 1] } else { s }).to_string()
}

/// Whether a CI file runs on a push to main (or master).
fn on_main(text: &str) -> bool {
    let push = text.lines().any(|l| l.trim_start().starts_with("push:") || l.trim() == "- push" || l.contains("on: push") || l.contains("on: [push"));
    let main = text.lines().any(|l| {
        let t = l.trim();
        (t.starts_with("branches:") || t.starts_with("- ") || t.starts_with("if:")) && (t.contains("main") || t.contains("master"))
    });
    (push && main) || text.contains("refs/heads/main") || text.contains("refs/heads/master")
}

/// The command lines in a README's fenced code blocks (shell ones, or ones that name no language), `$ ` taken off.
fn fenced_lines(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut inside = false;
    let mut shell = false;
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with("```") {
            if inside {
                inside = false;
            } else {
                inside = true;
                let lang = t.trim_start_matches('`').trim().to_ascii_lowercase();
                shell = matches!(lang.as_str(), "" | "sh" | "bash" | "shell" | "console" | "zsh" | "shell-session");
            }
            continue;
        }
        if inside && shell && !t.is_empty() && !t.starts_with('#') {
            // A comment after the command is the README's, not the command's.
            let t = t.split_once(" #").map_or(t, |(c, _)| c);
            out.push(t.trim_start_matches("$ ").trim().to_string());
        }
    }
    out
}

// ---------------------------------------------------------------- git

/// The most a git run of the look may take (a folder on a stuck mount mustn't hold it).
const GIT_CAP: Duration = Duration::from_secs(4);

/// All the git runs of one part of the look together: at most PART_CAP, and once one runs out of time the rest
/// are skipped (`stuck`), so a folder on a stuck mount can't hold a part.
struct Git<'a> {
    dir: &'a str,
    until: std::time::Instant,
    stuck: std::cell::Cell<bool>,
}

/// The most one part's git runs may take together.
const PART_CAP: Duration = Duration::from_secs(6);

impl<'a> Git<'a> {
    fn new(dir: &'a str) -> Self {
        Git { dir, until: std::time::Instant::now() + PART_CAP, stuck: std::cell::Cell::new(false) }
    }

    /// git's output, None when it failed, took over GIT_CAP, or the part's time is up.
    fn run(&self, args: &[&str]) -> Option<String> {
        let left = self.until.saturating_duration_since(std::time::Instant::now());
        if self.stuck.get() || left.is_zero() {
            self.stuck.set(true);
            return None;
        }
        let w = crate::safe_git::words(self.dir, &[], args);
        let mut c = std::process::Command::new(crate::safe_git::git_bin());
        c.args(&w[1..]);
        match crate::terminal::focus::capped(&mut c, GIT_CAP.min(left)) {
            Ok(o) => o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string()),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::TimedOut {
                    self.stuck.set(true);
                }
                None
            }
        }
    }

    /// Whether the folder is in a git repository.
    fn in_repo(&self) -> bool {
        self.run(&["rev-parse", "--show-toplevel"]).is_some_and(|t| !t.is_empty())
    }

    /// The branch changes land on: main or master when there's one here, else the one origin points at, else the
    /// one checked out.
    fn main_branch(&self) -> Option<String> {
        for b in ["main", "master"] {
            if self.run(&["rev-parse", "--verify", "--quiet", &format!("refs/heads/{b}")]).is_some() {
                return Some(b.into());
            }
        }
        if let Some(o) = self.run(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]).filter(|o| !o.is_empty()) {
            return Some(o);
        }
        self.run(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| !b.is_empty() && b != "HEAD")
    }
}

/// The line a part says when git took too long in the folder.
const GIT_STUCK: &str = "git didn't answer in this folder in time";

/// The last 30 first-parent changes on main: { repo, branch, total, prs, merges, direct }.
pub fn history(dir: &str) -> (Vec<Value>, Value) {
    let g = Git::new(dir);
    if !g.in_repo() {
        if g.stuck.get() {
            return (vec![line(format!("{GIT_STUCK}: no changes read"), None)], json!({ "repo": false, "stuck": true }));
        }
        return (vec![line("Not a git repository: no changes to read", None)], json!({ "repo": false }));
    }
    let Some(branch) = g.main_branch() else {
        return (vec![line("No changes on main yet", None)], json!({ "repo": true, "total": 0 }));
    };
    let n = HISTORY_TAKE.to_string();
    let out = g.run(&["log", "--first-parent", "-n", &n, "--format=%P%x1f%s", &branch, "--"]).unwrap_or_default();
    if g.stuck.get() {
        return (vec![line(format!("{GIT_STUCK}: no changes read"), None)], json!({ "repo": true, "total": 0, "stuck": true }));
    }
    let (mut prs, mut merges, mut direct) = (0, 0, 0);
    for l in out.lines().filter(|l| !l.is_empty()) {
        let (parents, subject) = l.split_once('\u{1f}').unwrap_or((l, ""));
        match change_kind(parents.split_whitespace().count(), subject) {
            "pr" => prs += 1,
            "merge" => merges += 1,
            _ => direct += 1,
        }
    }
    let total = prs + merges + direct;
    let shown = branch.trim_start_matches("origin/");
    let text = if total == 0 {
        format!("No changes on {shown} yet")
    } else {
        let mut parts = vec![];
        for (k, w, ws) in [(prs, "pull request", "pull requests"), (merges, "merge", "merges"), (direct, "direct", "direct")] {
            if k > 0 {
                parts.push(format!("{k} {}", if k == 1 { w } else { ws }));
            }
        }
        format!("Last {total} changes on {shown}: {}", parts.join(", "))
    };
    (vec![line(text, None)], json!({ "repo": true, "branch": shown, "total": total, "prs": prs, "merges": merges, "direct": direct }))
}

/// How one change came onto main, from its parents and subject: "pr" (a merged pull request, or a squashed one
/// ending "(#12)"), "merge" or "direct".
fn change_kind(parents: usize, subject: &str) -> &'static str {
    let s = subject.trim();
    if s.starts_with("Merge pull request #") || squash_pr(s) {
        "pr"
    } else if parents > 1 {
        "merge"
    } else {
        "direct"
    }
}

/// Whether a subject ends "(#12)", as GitHub's squash merge writes it.
fn squash_pr(s: &str) -> bool {
    s.strip_suffix(')').and_then(|s| s.rsplit_once("(#")).is_some_and(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

// ---------------------------------------------------------------- GitHub

/// "owner/repo" of a GitHub remote URL.
fn github_repo(url: &str) -> Option<String> {
    let u = url.trim();
    let rest = u.strip_prefix("https://github.com/").or_else(|| u.strip_prefix("git@github.com:")).or_else(|| u.strip_prefix("ssh://git@github.com/")).or_else(|| u.strip_prefix("http://github.com/"))?;
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut it = rest.split('/');
    let (o, r) = (it.next()?, it.next()?);
    (!o.is_empty() && !r.is_empty()).then(|| format!("{o}/{r}"))
}

/// What GitHub says main needs: { reached, needs_pr, reviews, branch }. Asked with `gh` (only reads), at most GH_CAP.
pub fn github(dir: &str) -> (Vec<Value>, Value) {
    let out_of_reach = |why: &str| (vec![line(format!("GitHub out of reach{why}: going by the last 30 changes alone"), None)], json!({ "reached": false }));
    let g = Git::new(dir);
    let is_repo = g.in_repo();
    let remote = if is_repo { g.run(&["remote", "get-url", "origin"]) } else { None };
    if g.stuck.get() {
        return out_of_reach(&format!(" ({GIT_STUCK})"));
    }
    if !is_repo {
        return (vec![line("Not a git repository: nothing to ask GitHub", None)], json!({ "reached": false }));
    }
    let Some(repo) = remote.and_then(|u| github_repo(&u)) else {
        return (vec![line("No GitHub remote: going by the last 30 changes alone", None)], json!({ "reached": false }));
    };
    let branch = g.main_branch().map(|b| b.trim_start_matches("origin/").to_string()).unwrap_or_else(|| "main".into());
    // gh gets what's left of the part's time, so git and gh together stay within it.
    let cap = GH_CAP.min(g.until.saturating_duration_since(std::time::Instant::now()));
    if g.stuck.get() || cap.is_zero() {
        return out_of_reach(&format!(" ({GIT_STUCK})"));
    }
    let gh = crate::desk::hub::landing::pr::gh_bin();
    let ask = |path: String| {
        let gh = gh.clone();
        std::thread::spawn(move || {
            let mut c = std::process::Command::new(gh);
            c.args(["api", "-H", "Accept: application/vnd.github+json", &path]);
            crate::terminal::focus::capped(&mut c, cap).ok().map(|o| (o.status.success(), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string()))
        })
    };
    let rules = ask(format!("repos/{repo}/rules/branches/{branch}"));
    let prot = ask(format!("repos/{repo}/branches/{branch}/protection"));
    let (rules, prot) = (rules.join().ok().flatten(), prot.join().ok().flatten());
    let (reached, needs_pr, reviews) = github_answer(rules.as_ref(), prot.as_ref());
    if !reached {
        return out_of_reach("");
    }
    let text = if needs_pr {
        match reviews {
            0 => format!("GitHub: {branch} needs a pull request"),
            1 => format!("GitHub: {branch} needs a pull request with 1 review"),
            n => format!("GitHub: {branch} needs a pull request with {n} reviews"),
        }
    } else {
        format!("GitHub: {branch} takes changes without a pull request")
    };
    (vec![line(text, None)], json!({ "reached": true, "needs_pr": needs_pr, "reviews": reviews, "branch": branch }))
}

/// What gh's two answers say: (reached, needs a pull request, reviews it needs). Each answer is (succeeded, stdout,
/// stderr). GitHub counts as reached only when the rules call answered with a list, or the protection call answered
/// or said "Branch not protected"; anything else (no access, not found, no network) is out of reach.
pub fn github_answer(rules: Option<&(bool, String, String)>, prot: Option<&(bool, String, String)>) -> (bool, bool, u64) {
    let mut reached = false;
    let mut needs_pr = false;
    let mut reviews = 0u64;
    if let Some((true, out, _)) = rules {
        let list = serde_json::from_str::<Value>(out).ok().and_then(|v| v.as_array().cloned());
        reached |= list.is_some();
        for r in list.unwrap_or_default() {
            if r["type"] == "pull_request" {
                needs_pr = true;
                reviews = reviews.max(r["parameters"]["required_approving_review_count"].as_u64().unwrap_or(0));
            }
        }
    }
    match prot {
        Some((true, out, _)) => {
            reached = true;
            let v: Value = serde_json::from_str(out).unwrap_or(Value::Null);
            if let Some(p) = v.get("required_pull_request_reviews").filter(|p| p.is_object()) {
                needs_pr = true;
                reviews = reviews.max(p["required_approving_review_count"].as_u64().unwrap_or(0));
            }
        }
        // GitHub answered: main simply isn't protected.
        Some((false, out, err)) if out.contains("Branch not protected") || err.contains("Branch not protected") => reached = true,
        _ => {}
    }
    (reached, needs_pr, reviews)
}

// ---------------------------------------------------------------- the rows

/// What the folder is, by its own look: Cue's own source, a repository, or neither.
fn place_here(dir: &str) -> Place {
    crate::logs::repos::first_look(dir);
    match crate::logs::repos::facts(dir) {
        Some(f) if f.cue => Place::Cue,
        Some(f) if f.top.is_none() => Place::NoRepo,
        Some(_) => Place::Repo,
        None if Git::new(dir).in_repo() => Place::Repo,
        None => Place::NoRepo,
    }
}

fn place_key(p: Place) -> &'static str {
    match p {
        Place::Cue => "cue",
        Place::Repo => "repo",
        Place::NoRepo => "folder",
    }
}

/// A row's source.
fn from_file(f: &str) -> Value {
    json!({ "kind": "file", "file": f })
}

fn row(part: &str, tag: &str, choice: Option<&str>, steps: Vec<String>, sources: Vec<Value>, why: String) -> Value {
    let label = match part {
        "build" => "Builds with",
        "check" => "Check first",
        "how" => "Lands on main as",
        "after" => "Then runs",
        _ => "Send to GitHub",
    };
    let mut r = json!({ "part": part, "label": label, "tag": tag, "steps": steps, "sources": sources, "why": why });
    if let Some(c) = choice {
        r["choice"] = json!(c);
    }
    r
}

/// A step can go in: it doesn't push.
fn safe_step(s: &str) -> bool {
    lands::refused_steps(&[s.to_string()]).is_ok()
}

/// The rows and the full setting from the four parts' answers: { place, summary, rows, lands }.
pub fn rows(folder: &str, found: &Value) -> Result<Value, String> {
    let dir = expand(folder);
    if dir.is_empty() || !Path::new(&dir).is_dir() {
        return Err(format!("No folder at {}", said(&dir)));
    }
    Ok(rows_in(place_here(&dir), found))
}

/// `rows` for a folder in `place`.
pub fn rows_in(place: Place, found: &Value) -> Value {
    let empty = Value::Null;
    let part = |k: &str| found.get(k).filter(|v| v.is_object()).unwrap_or(&empty);
    let (files, github, history, sessions) = (part("files"), part("github"), part("history"), part("sessions"));
    let mut l = lands::defaults(place);
    let mut out = vec![];

    if place == Place::Cue {
        let why = "Cue's own source keeps Cue's own setup.".to_string();
        out.push(row("build", "usual", None, vec![], vec![], "Cue's own check builds the app from a clean copy.".into()));
        out.push(row("check", "usual", Some(l.check), vec![], vec![], why.clone()));
        out.push(row("how", "usual", Some(l.how), vec![], vec![], why.clone()));
        out.push(row("after", "usual", Some(l.after), vec![], vec![], why.clone()));
        out.push(row("push", "usual", Some(l.push), vec![], vec![], why));
        return finish(place, out, &l);
    }

    let cmds: Vec<&Value> = files["commands"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    let ci: Vec<&Value> = files["ci"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    let s_cmds: Vec<&Value> = sessions["commands"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    let top_session = |kinds: &[&str]| s_cmds.iter().filter(|c| kinds.contains(&c["kind"].as_str().unwrap_or(""))).max_by_key(|c| c["count"].as_u64().unwrap_or(0)).copied();
    let sessions_src = |c: &Value| json!({ "kind": "sessions", "names": c["names"].clone(), "count": c["count"].as_u64().unwrap_or(0) });
    // Why files win, when sessions ran something else for the same thing.
    let disagree = |c: &Value, kinds: &[&str]| -> String {
        let Some(s) = top_session(kinds) else { return String::new() };
        let ran = s["cmd"].as_str().unwrap_or("");
        let file_says = c["body"].as_str().or(c["cmd"].as_str()).unwrap_or("");
        if ran.is_empty() || ran == file_says {
            return String::new();
        }
        let what = c["what"].as_str().unwrap_or("").trim_start_matches("its ");
        format!(" Your past sessions ran {ran}; {}'s {what} is {file_says}, so that's used.", c["file"].as_str().unwrap_or(""))
    };

    // Build.
    let ci_build = ci.iter().find_map(|c| c["steps"].as_array()?.iter().find(|s| s["kind"] == "build").map(|s| (c["file"].as_str().unwrap_or(""), s["cmd"].as_str().unwrap_or(""))));
    let mut build_step: Option<String> = None;
    if let Some(c) = cmds.iter().find(|c| c["kind"] == "build") {
        let run = c["cmd"].as_str().unwrap_or("").to_string();
        let file = c["file"].as_str().unwrap_or("");
        let why = format!("From {file}, {}.{}", c["what"].as_str().unwrap_or(""), disagree(c, &["build"]));
        out.push(row("build", "found", None, vec![labelled("build", &run)], vec![from_file(file)], why));
        build_step = Some(run);
    } else if let Some((file, run)) = ci_build {
        out.push(row("build", "found", None, vec![labelled("build", run)], vec![from_file(file)], format!("The build step your CI runs, from {file}.")));
        build_step = Some(run.to_string());
    } else if let Some(s) = top_session(&["build"]) {
        let run = s["cmd"].as_str().unwrap_or("").to_string();
        out.push(row("build", "found", None, vec![labelled("build", &run)], vec![sessions_src(s)], format!("Your past sessions built it with {run}; no file says how.")));
        build_step = Some(run);
    } else {
        out.push(row("build", "usual", None, vec![], vec![], "Nothing says how it builds, so no build step runs first.".into()));
    }
    let room = STEPS_MAX - usize::from(build_step.is_some());

    // Check: CI's own steps first, then the files', then past sessions'.
    let check_kinds = ["test", "lint", "typecheck", "check"];
    let mut check_steps: Vec<(String, String)> = vec![];
    let ci_checks: Vec<(String, String, String)> = ci
        .iter()
        .flat_map(|c| c["steps"].as_array().into_iter().flatten().filter(|s| check_kinds.contains(&s["kind"].as_str().unwrap_or(""))).map(move |s| (c["file"].as_str().unwrap_or("").to_string(), s["kind"].as_str().unwrap_or("").to_string(), s["cmd"].as_str().unwrap_or("").to_string())))
        .collect();
    let push_step = |steps: &mut Vec<(String, String)>, k: &str, c: &str| {
        if steps.len() < room && safe_step(c) && !steps.iter().any(|(_, x)| x == c) {
            steps.push((k.to_string(), c.to_string()));
        }
    };
    if !ci_checks.is_empty() {
        let mut files_named: Vec<String> = vec![];
        for (f, k, c) in &ci_checks {
            push_step(&mut check_steps, k, c);
            if !files_named.contains(f) {
                files_named.push(f.clone());
            }
        }
        let why = format!("The same steps your GitHub checks run, from {}.", files_named.join(" and "));
        out.push(row("check", "found", Some("command"), check_steps.iter().map(|(k, c)| labelled(k, c)).collect(), files_named.iter().map(|f| from_file(f)).collect(), why));
    } else if cmds.iter().any(|c| check_kinds.contains(&c["kind"].as_str().unwrap_or(""))) {
        let mut files_named: Vec<String> = vec![];
        let mut whys = String::new();
        let mut whats: Vec<String> = vec![];
        // Lint and types before tests: the quick ones first.
        for kinds in [&["lint", "typecheck", "check"][..], &["test"][..]] {
            for c in cmds.iter().filter(|c| kinds.contains(&c["kind"].as_str().unwrap_or(""))) {
                let k = c["kind"].as_str().unwrap_or("");
                // One step of each kind: the first file that says it.
                if check_steps.iter().any(|(x, _)| x == k) {
                    continue;
                }
                push_step(&mut check_steps, k, c["cmd"].as_str().unwrap_or(""));
                let f = c["file"].as_str().unwrap_or("").to_string();
                if !files_named.contains(&f) {
                    files_named.push(f.clone());
                }
                let said = format!("{f}, {}", c["what"].as_str().unwrap_or(""));
                if !whats.contains(&said) {
                    whats.push(said);
                }
                whys.push_str(&disagree(c, &[k]));
            }
        }
        let why = format!("From {}.{whys}", whats.join("; "));
        out.push(row("check", "found", Some("command"), check_steps.iter().map(|(k, c)| labelled(k, c)).collect(), files_named.iter().map(|f| from_file(f)).collect(), why));
    } else if s_cmds.iter().any(|c| check_kinds.contains(&c["kind"].as_str().unwrap_or(""))) {
        let mut by_count: Vec<&&Value> = s_cmds.iter().filter(|c| check_kinds.contains(&c["kind"].as_str().unwrap_or(""))).collect();
        by_count.sort_by_key(|c| std::cmp::Reverse(c["count"].as_u64().unwrap_or(0)));
        let mut srcs = vec![];
        for c in by_count.iter().take(3) {
            push_step(&mut check_steps, c["kind"].as_str().unwrap_or(""), c["cmd"].as_str().unwrap_or(""));
        }
        if let Some(c) = by_count.first() {
            srcs.push(sessions_src(c));
        }
        let ran: Vec<&str> = check_steps.iter().map(|(_, c)| c.as_str()).collect();
        out.push(row("check", "found", Some("command"), check_steps.iter().map(|(k, c)| labelled(k, c)).collect(), srcs, format!("Your past sessions ran {}; no file says what to check.", ran.join(", "))));
    } else {
        out.push(row("check", "unsure", Some("none"), vec![], vec![], "Nothing says how to check a change. Until you pick, nothing is checked.".into()));
    }
    l.check_steps = build_step.iter().cloned().chain(check_steps.iter().map(|(_, c)| c.clone())).collect();
    if !l.check_steps.is_empty() {
        l.check = "command";
    }

    // How it lands (a repository only).
    let n = |v: &Value, k: &str| v[k].as_u64().unwrap_or(0);
    let mut how = None;
    if place == Place::Repo {
        let total = n(history, "total");
        let landed_sessions = n(sessions, "pr_create");
        let (tag, choice, srcs, why): (&str, &str, Vec<Value>, String) = if github["reached"] == true && github["needs_pr"] == true {
            let r = n(github, "reviews");
            let with = match r {
                0 => String::new(),
                1 => " with 1 review".into(),
                r => format!(" with {r} reviews"),
            };
            ("found", "pr", vec![json!({ "kind": "github" })], format!("GitHub says {} needs a pull request{with}.", github["branch"].as_str().unwrap_or("main")))
        } else if total > 0 && n(history, "prs") * 2 > total {
            ("found", "pr", vec![json!({ "kind": "history" })], format!("{} of the last {total} changes on main came as pull requests.", n(history, "prs")))
        } else if total > 0 && n(history, "merges") * 2 > total {
            ("found", "merge", vec![json!({ "kind": "history" })], format!("{} of the last {total} changes on main came as merges.", n(history, "merges")))
        } else if total > 0 && n(history, "direct") * 2 > total {
            ("found", "squash", vec![json!({ "kind": "history" })], format!("{} of the last {total} changes on main went straight in, one commit each.", n(history, "direct")))
        } else if landed_sessions > 0 {
            let src = json!({ "kind": "sessions", "names": sessions["pr_names"].clone(), "count": landed_sessions });
            ("found", "pr", vec![src], format!("{landed_sessions} past session{} here opened a pull request.", if landed_sessions == 1 { "" } else { "s" }))
        } else {
            ("unsure", l.how, vec![], "Nothing says how changes reach main. Until you pick, each item lands as one commit.".into())
        };
        l.how = choice;
        how = Some(choice);
        out.push(row("how", tag, Some(choice), vec![], srcs, why));
    }

    // After landing.
    let s_after: Vec<&Value> = sessions["after"].as_array().map(|a| a.iter().filter(|c| safe_step(c["cmd"].as_str().unwrap_or(""))).collect()).unwrap_or_default();
    let ci_deploy: Vec<(String, String)> = ci
        .iter()
        .filter(|c| c["on_main"] == true)
        .flat_map(|c| c["steps"].as_array().into_iter().flatten().filter(|s| s["kind"] == "deploy").map(move |s| (c["file"].as_str().unwrap_or("").to_string(), s["cmd"].as_str().unwrap_or("").to_string())))
        .filter(|(_, c)| safe_step(c))
        .collect();
    let scripts: Vec<&str> = files["scripts"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if !s_after.is_empty() {
        let mut by_count = s_after.clone();
        by_count.sort_by_key(|c| std::cmp::Reverse(c["count"].as_u64().unwrap_or(0)));
        let steps: Vec<String> = by_count.iter().take(STEPS_MAX).map(|c| c["cmd"].as_str().unwrap_or("").to_string()).collect();
        let why = format!("Your past sessions ran {} after their work landed.", steps.join(", "));
        out.push(row("after", "found", Some("command"), steps.iter().map(|s| labelled("deploy", s)).collect(), vec![sessions_src(by_count[0])], why));
        l.after = "command";
        l.after_steps = steps;
    } else if !ci_deploy.is_empty() {
        let steps: Vec<String> = ci_deploy.iter().take(STEPS_MAX).map(|(_, c)| c.clone()).collect();
        let file = ci_deploy[0].0.clone();
        out.push(row("after", "found", Some("command"), steps.iter().map(|s| labelled("deploy", s)).collect(), vec![from_file(&file)], format!("Your CI runs this on each push to main, from {file}.")));
        l.after = "command";
        l.after_steps = steps;
    } else if let Some(s) = scripts.first() {
        out.push(row("after", "unsure", Some("nothing"), vec![], vec![from_file(s)], format!("Found {s}, but nothing says it runs after each change. Until you pick, nothing runs.")));
    } else {
        out.push(row("after", "usual", Some("nothing"), vec![], vec![], "Nothing found runs after a change lands, so landing is the last step.".into()));
    }

    // Push (a repository only).
    if place == Place::Repo {
        let landed = n(sessions, "landed");
        let pushed = n(sessions, "pushed_after_landing");
        let (tag, choice, srcs, why) = if how == Some("pr") {
            ("follows", "after", vec![], "A pull request sends each item's branch to GitHub, so finished work goes right away.".to_string())
        } else if pushed > 0 && pushed * 2 > landed {
            let src = json!({ "kind": "sessions", "names": sessions["pushed_names"].clone(), "count": pushed });
            ("found", "after", vec![src], format!("{pushed} of {landed} past sessions that landed work here pushed it right after."))
        } else if landed > 0 {
            ("unsure", "ask", vec![], "Past sessions here didn't always push what they landed. Until you pick, you're asked first.".to_string())
        } else {
            ("usual", "ask", vec![], "Nothing says when work goes to GitHub, so you're asked first.".to_string())
        };
        l.push = choice;
        out.push(row("push", tag, Some(choice), vec![], srcs, why));
    }

    finish(place, out, &l)
}

fn finish(place: Place, rows: Vec<Value>, l: &lands::Lands) -> Value {
    let count = |t: &[&str]| rows.iter().filter(|r| t.contains(&r["tag"].as_str().unwrap_or(""))).count();
    let (found, pick) = (count(&["found", "follows"]), count(&["unsure"]));
    let summary = match (found, pick) {
        (0, 0) => "The usual settings".to_string(),
        (f, 0) => format!("{f} found"),
        (0, p) => format!("{p} to pick"),
        (f, p) => format!("{f} found · {p} to pick"),
    };
    json!({ "place": place_key(place), "summary": summary, "rows": rows, "lands": l.to_json() })
}

/// The folder Cue's past-session look reads for `dir` (tests: any root).
pub fn transcripts_dir(root: &Path, dir: &str) -> PathBuf {
    let name: String = dir.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    root.join(name)
}
