//! A project's actions as they're written down: its two files (.cue/actions.json, shared with the repo,
//! and .cue/actions.local.json, yours), and the ones Cue finds in its package.json, justfile and Makefile.
//! Reading and writing only: what's allowed, pinned or running lives elsewhere (mod.rs, run.rs).

use serde_json::{json, Map, Value};

/// The shared file, committed with the project.
pub const SHARED: &str = ".cue/actions.json";
/// Yours: Cue says when git doesn't ignore it, and never edits .gitignore itself.
pub const LOCAL: &str = ".cue/actions.local.json";
/// The files Cue reads in a project folder, in this order.
pub const FILES: [&str; 7] = [SHARED, LOCAL, "package.json", "justfile", "Justfile", "Makefile", "makefile"];
/// The lockfiles that say which package manager runs package.json's scripts (only whether each is there).
pub const LOCKS: [&str; 4] = ["pnpm-lock.yaml", "yarn.lock", "bun.lockb", "bun.lock"];
/// The most of one file Cue reads.
pub const FILE_MAX: usize = 256 * 1024;

/// The notes at the top of both files, Cue's own: for the person and for an agent asked to set them up.
/// A file written without them gets them; a file's own are kept as they are.
pub const ABOUT: [&str; 7] = [
    "Actions are buttons in Cue for this project: install, test, start a server, clean a cache.",
    "Each needs a name and a one-line shell command, run by the person's shell in this folder.",
    "\"keep\": true for a server or watcher that keeps running; Cue shows its port, Restart and Stop.",
    "\"suggest\" lists files (globs) whose change in an agent's turn should offer the action under the reply.",
    "\"ask\": true if running it while an agent is working could trip the agent (install, clean): Cue asks.",
    "Only add commands this project really uses (package.json, justfile, Makefile, README, docker-compose).",
    "Cue asks the person before running anything new or changed.",
];

/// What Cue read of a project folder: each file's text (None: not there) and the lockfiles that are.
#[derive(Default, Clone, Debug)]
pub struct Folder {
    pub texts: Vec<(String, String)>,
    pub locks: Vec<String>,
    /// Whether git ignores the local file: None outside a repository (or when git couldn't say).
    pub local_ignored: Option<bool>,
}

impl Folder {
    pub fn text(&self, file: &str) -> Option<&str> {
        self.texts.iter().find(|(f, _)| f == file).map(|(_, t)| t.as_str())
    }
}

/// One action, from a file or found.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Action {
    /// "shared" | "local" | "package.json" | "justfile" | "Makefile".
    pub source: String,
    pub name: String,
    /// The command line Cue runs (a found one's: `npm run build`, `just test`, `make clean`).
    pub run: String,
    pub keep: bool,
    pub ask: bool,
    pub suggest: Vec<String>,
    /// A found one's script or recipe: what its command really runs, so a change to it asks again.
    pub script: Option<String>,
}

impl Action {
    /// Stable across reads: its source and its name.
    pub fn id(&self) -> String {
        format!("{}:{}", self.source, self.name)
    }
    /// What you allow when you allow it: its command, or for a found one what that command runs.
    pub fn trusted(&self) -> String {
        self.script.clone().unwrap_or_else(|| self.run.clone())
    }
    pub fn found(&self) -> bool {
        !matches!(self.source.as_str(), "shared" | "local")
    }
}

/// The actions in one of the two files (`source` "shared" or "local"). An entry without a name or a
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
        let suggest = match &a["suggest"] {
            Value::String(s) => vec![s.clone()],
            v => v.as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).filter(|s| !s.trim().is_empty()).collect(),
        };
        out.push(Action { source: source.into(), name: name.into(), run: run.into(), keep: a["keep"].as_bool().unwrap_or(false), ask: a["ask"].as_bool().unwrap_or(false), suggest, script: None });
    }
    Ok(out)
}

/// The file's JSON to change: its own (keys and order kept), or a new one, with Cue's notes first when it
/// has none. Refuses a file that isn't JSON rather than lose what's in it.
fn editable(text: Option<&str>) -> Result<Map<String, Value>, String> {
    let mut m = match text.filter(|t| !t.trim().is_empty()) {
        Some(t) => match serde_json::from_str::<Value>(t).map_err(|e| format!("Fix the file first: it isn't valid JSON ({e})"))? {
            Value::Object(m) => m,
            _ => return Err("Fix the file first: it isn't a JSON object".into()),
        },
        None => Map::new(),
    };
    if !m.contains_key("about") {
        let mut with = Map::new();
        with.insert("about".into(), json!(ABOUT));
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

/// The file with `a` saved in it: the entry named `old` (else `a.name`) changed in place, its other keys
/// kept, or a new entry at the end. `keep`/`ask` are written only when on, `suggest` only when it has any.
pub fn with_action(text: Option<&str>, old: Option<&str>, a: &Action) -> Result<String, String> {
    let mut m = editable(text)?;
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
    if a.suggest.is_empty() {
        e.remove("suggest");
    } else {
        e.insert("suggest".into(), json!(a.suggest));
    }
    match at {
        Some(i) => list[i] = Value::Object(e),
        None => list.push(Value::Object(e)),
    }
    Ok(written(m))
}

/// The file without the action named `name` (everything else as it was).
pub fn without_action(text: Option<&str>, name: &str) -> Result<String, String> {
    let mut m = editable(text)?;
    if let Some(list) = m.get_mut("actions").and_then(Value::as_array_mut) {
        list.retain(|e| e["name"].as_str().map(str::trim) != Some(name));
    }
    Ok(written(m))
}

/// A word a POSIX shell takes as it is, else quoted.
fn word(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:/@+=,".contains(c)) {
        s.to_string()
    } else {
        crate::terminal::focus::shq(s)
    }
}

/// The package manager that runs package.json's scripts: by its lockfile, else its `packageManager`
/// field, else npm.
pub fn package_manager(locks: &[String], package: &Value) -> &'static str {
    let has = |f: &str| locks.iter().any(|l| l == f);
    if has("pnpm-lock.yaml") {
        return "pnpm";
    }
    if has("yarn.lock") {
        return "yarn";
    }
    if has("bun.lockb") || has("bun.lock") {
        return "bun";
    }
    match package["packageManager"].as_str().and_then(|p| p.split('@').next()) {
        Some("pnpm") => "pnpm",
        Some("yarn") => "yarn",
        Some("bun") => "bun",
        _ => "npm",
    }
}

/// package.json's scripts, each as `<pm> run <name>`. A `pre`/`post` hook of another script is left out
/// (its package manager runs it with that one).
pub fn package_scripts(text: &str, locks: &[String]) -> Vec<Action> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return vec![] };
    let pm = package_manager(locks, &v);
    let Some(scripts) = v["scripts"].as_object() else { return vec![] };
    let hook = |n: &str| ["pre", "post"].iter().any(|p| n.strip_prefix(p).is_some_and(|rest| !rest.is_empty() && scripts.contains_key(rest)));
    scripts
        .iter()
        .filter_map(|(n, body)| Some((n, body.as_str()?)))
        .filter(|(n, b)| !n.trim().is_empty() && !b.trim().is_empty() && !hook(n))
        .map(|(n, b)| Action { source: "package.json".into(), name: n.clone(), run: format!("{pm} run {}", word(n)), script: Some(b.to_string()), ..Default::default() })
        .collect()
}

/// Each target's or recipe's body: the indented lines under it, trimmed and joined.
fn body_after(lines: &[&str], i: usize, indent: impl Fn(&str) -> bool) -> String {
    lines[i + 1..].iter().take_while(|l| indent(l) || l.trim().is_empty()).map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

/// A justfile's recipes, each as `just <name>`: lines that start a recipe (`name args: deps`, `@name:`),
/// not settings, variables, aliases or imports, and not private ones (`_name`, or under `[private]`).
pub fn just_recipes(text: &str) -> Vec<Action> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Action> = vec![];
    let mut private = false;
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with([' ', '\t']) || l.trim().is_empty() || l.starts_with('#') {
            continue;
        }
        let t = l.trim_end();
        if t.starts_with('[') {
            private |= t.contains("private");
            continue;
        }
        let was_private = std::mem::take(&mut private);
        let first = t.split_whitespace().next().unwrap_or("");
        if ["set", "alias", "export", "import", "mod"].contains(&first) || t.contains(":=") {
            continue;
        }
        let Some(head) = t.split_once(':').map(|(h, _)| h) else { continue };
        let name = head.split_whitespace().next().unwrap_or("").trim_start_matches('@');
        if name.is_empty() || name.starts_with('_') || was_private || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        if out.iter().any(|a| a.name == name) {
            continue;
        }
        let body = body_after(&lines, i, |l| l.starts_with([' ', '\t']));
        out.push(Action { source: "justfile".into(), name: name.into(), run: format!("just {name}"), script: Some(if body.is_empty() { format!("just {name}") } else { body }), ..Default::default() });
    }
    out
}

/// A Makefile's targets, each as `make <name>`: plain `name:` lines (and each name of `a b:`), never
/// `.PHONY` or another special target, a pattern (`%.o:`), a variable (`$(X):`), a file (`out/app.o:`)
/// or an assignment (`X := y`, `X = a:b`); nothing inside `define … endef`.
pub fn make_targets(text: &str) -> Vec<Action> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<Action> = vec![];
    let mut in_define = false;
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim();
        if in_define {
            in_define = t != "endef";
            continue;
        }
        if t.starts_with("define ") || t == "define" {
            in_define = true;
            continue;
        }
        if l.starts_with([' ', '\t']) || t.is_empty() || t.starts_with('#') {
            continue;
        }
        let Some(colon) = t.find(':') else { continue };
        let (head, rest) = (&t[..colon], &t[colon..]);
        if head.contains('=') || rest.starts_with(":=") || rest.starts_with("::=") {
            continue;
        }
        for name in head.split_whitespace() {
            let plain = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !plain || name.is_empty() || out.iter().any(|a| a.name == name) {
                continue;
            }
            let body = body_after(&lines, i, |l| l.starts_with('\t'));
            out.push(Action { source: "Makefile".into(), name: name.into(), run: format!("make {name}"), script: Some(if body.is_empty() { format!("make {name}") } else { body }), ..Default::default() });
        }
    }
    out
}

/// Every action Cue finds in the folder's own files (package.json, justfile, Makefile), in that order.
pub fn found(f: &Folder) -> Vec<Action> {
    let mut out = vec![];
    if let Some(t) = f.text("package.json") {
        out.extend(package_scripts(t, &f.locks));
    }
    if let Some(t) = f.text("justfile").or_else(|| f.text("Justfile")) {
        out.extend(just_recipes(t));
    }
    if let Some(t) = f.text("Makefile").or_else(|| f.text("makefile")) {
        out.extend(make_targets(t));
    }
    out
}

/// The found ones that aren't already in a file: none whose name (in any case) or command is a file
/// action's.
pub fn dedupe(files: &[Action], found: Vec<Action>) -> Vec<Action> {
    found.into_iter().filter(|f| !files.iter().any(|a| a.name.eq_ignore_ascii_case(&f.name) || a.run == f.run)).collect()
}

/// Whether `path` (relative to the project folder, `/`-separated) matches `pattern`: `**` any number of
/// folders (none too), `*` anything within one name, `?` one character; else the exact path.
pub fn glob(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.trim().trim_start_matches("./").split('/').filter(|s| !s.is_empty()).collect();
    let s: Vec<&str> = path.trim_start_matches("./").split('/').filter(|s| !s.is_empty()).collect();
    fn segs(p: &[&str], s: &[&str]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some(&"**") => (0..=s.len()).any(|i| segs(&p[1..], &s[i..])),
            Some(first) => !s.is_empty() && name(first.as_bytes(), s[0].as_bytes()) && segs(&p[1..], &s[1..]),
        }
    }
    fn name(p: &[u8], s: &[u8]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some(b'*') => (0..=s.len()).any(|i| name(&p[1..], &s[i..])),
            Some(b'?') => !s.is_empty() && name(&p[1..], &s[1..]),
            Some(c) => s.first() == Some(c) && name(&p[1..], &s[1..]),
        }
    }
    !p.is_empty() && segs(&p, &s)
}

/// `path` as the project sees it: relative to `folder` when it's inside it ("" when it isn't).
pub fn relative(folder: &str, path: &str) -> String {
    let f = folder.trim_end_matches('/');
    if !path.starts_with('/') {
        return path.trim_start_matches("./").to_string();
    }
    path.strip_prefix(f).and_then(|r| r.strip_prefix('/')).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_a_file_keeps_its_notes_its_other_keys_and_each_entrys_own() {
        let text = r#"{"about": ["ours"], "version": 2, "actions": [{"name": "API", "run": "npm run dev:api", "keep": true, "color": "lime"}, {"name": "Lint", "run": "npm run lint"}]}"#;
        let out = with_action(Some(text), Some("API"), &Action { name: "Server".into(), run: "npm run dev".into(), keep: false, ask: true, suggest: vec!["server/**".into()], ..Default::default() }).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["about"], json!(["ours"]), "its own notes stay");
        assert_eq!(v["version"], 2);
        assert_eq!(v["actions"][0], json!({"name": "Server", "run": "npm run dev", "color": "lime", "ask": true, "suggest": ["server/**"]}));
        assert_eq!(v["actions"][1]["name"], "Lint");
        assert!(out.starts_with("{\n  \"about\""), "2-space JSON: {out}");
        assert!(out.ends_with("}\n"));
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["about", "version", "actions"], "order kept");
        // Parsed back as written.
        let a = parse(&out, "shared").unwrap();
        assert_eq!((a[0].name.as_str(), a[0].ask, a[0].keep, a[0].suggest.clone()), ("Server", true, false, vec!["server/**".to_string()]));
        assert_eq!(a[0].id(), "shared:Server");
    }

    #[test]
    fn a_new_file_gets_cues_notes_first_and_a_broken_one_is_never_written_over() {
        let out = with_action(None, None, &Action { name: "Install".into(), run: "npm ci".into(), ..Default::default() }).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["about"], json!(ABOUT));
        assert_eq!(v["actions"], json!([{"name": "Install", "run": "npm ci"}]));
        let no_notes = with_action(Some(r#"{"actions": [], "x": 1}"#), None, &Action { name: "T".into(), run: "t".into(), ..Default::default() }).unwrap();
        let keys: Vec<String> = serde_json::from_str::<Value>(&no_notes).unwrap().as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["about", "actions", "x"]);
        assert!(with_action(Some("{ not json"), None, &Action { name: "T".into(), run: "t".into(), ..Default::default() }).is_err());
        assert!(parse("{ not json", "shared").is_err());
        let two = with_action(Some(&out), None, &Action { name: "install".into(), run: "pnpm i".into(), ..Default::default() });
        assert!(two.unwrap_err().contains("already"), "two of one name");
        let gone = without_action(Some(&out), "Install").unwrap();
        assert_eq!(serde_json::from_str::<Value>(&gone).unwrap()["actions"], json!([]));
    }

    #[test]
    fn a_files_entries_need_a_name_and_a_command() {
        let a = parse(r#"{"actions": [{"name": "A", "run": "a"}, {"name": "", "run": "b"}, {"run": "c"}, {"name": "D"}, {"name": "A", "run": "again"}, {"name": "E", "run": "e", "suggest": "x/*"}]}"#, "local").unwrap();
        assert_eq!(a.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["A", "E"]);
        assert_eq!(a[1].suggest, ["x/*"]);
        assert!(parse("", "local").unwrap().is_empty());
    }

    #[test]
    fn package_scripts_run_with_the_lockfiles_package_manager() {
        let pkg = r#"{"scripts": {"dev": "vite", "build": "vite build", "prebuild": "rm -rf dist", "postinstall": "patch", "test:unit": "vitest", "empty": ""}}"#;
        let names = |locks: &[&str]| package_scripts(pkg, &locks.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let npm = names(&[]);
        assert_eq!(npm.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["dev", "build", "postinstall", "test:unit"], "prebuild goes with build; postinstall has no install script");
        assert_eq!(npm[0].run, "npm run dev");
        assert_eq!(npm[1].script.as_deref(), Some("vite build"));
        assert_eq!(npm[1].trusted(), "vite build", "a change to the script asks again");
        assert_eq!(npm[0].id(), "package.json:dev");
        assert_eq!(names(&["pnpm-lock.yaml"])[0].run, "pnpm run dev");
        assert_eq!(names(&["yarn.lock"])[0].run, "yarn run dev");
        assert_eq!(names(&["bun.lockb"])[0].run, "bun run dev");
        assert_eq!(names(&["bun.lock"])[0].run, "bun run dev");
        assert_eq!(package_scripts(r#"{"packageManager": "pnpm@9.1.0", "scripts": {"a b": "x"}}"#, &[])[0].run, "pnpm run 'a b'");
        assert!(package_scripts("not json", &[]).is_empty());
    }

    #[test]
    fn justfile_recipes_but_not_its_settings_variables_or_private_ones() {
        let j = "set shell := [\"bash\", \"-c\"]\nversion := \"1\"\nalias t := test\n# a comment\n\ndefault:\n  just --list\n\n@test *args: build\n    cargo test {{args}}\n\n_helper:\n  echo\n\n[private]\nhidden:\n  echo\n\n[group('x')]\nrelease version='0':\n  ./release.sh {{version}}\nexport FOO := \"bar\"\n";
        let r = just_recipes(j);
        assert_eq!(r.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["default", "test", "release"]);
        assert_eq!(r[1].run, "just test");
        assert_eq!(r[1].script.as_deref(), Some("cargo test {{args}}"));
    }

    #[test]
    fn makefile_targets_are_plain_names_only() {
        let m = ".PHONY: build test\nCC = gcc\nFLAGS := -O2\nURL = http://x:80\nbuild: deps\n\tgo build ./...\n\ntest lint:\n\tgo test ./...\n%.o: %.c\n\t$(CC) -c $<\n$(OUT): x\n\techo\nout/app.o: a.c\n\techo\n.DEFAULT_GOAL := build\ndefine BLOCK\nfake: thing\nendef\nclean::\n\trm -rf out\n\tinstall:\n";
        let t = make_targets(m);
        assert_eq!(t.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["build", "test", "lint", "clean"]);
        assert_eq!(t[0].run, "make build");
        assert_eq!(t[0].script.as_deref(), Some("go build ./..."));
        assert_eq!(t[3].script.as_deref(), Some("rm -rf out\ninstall:"));
    }

    #[test]
    fn a_found_action_already_in_a_file_is_listed_once() {
        let files = vec![Action { source: "shared".into(), name: "Install".into(), run: "npm ci".into(), ..Default::default() }, Action { source: "local".into(), name: "Web".into(), run: "npm run dev".into(), ..Default::default() }];
        let found = package_scripts(r#"{"scripts": {"install": "x", "dev": "vite", "test": "vitest"}}"#, &[]);
        let kept = dedupe(&files, found);
        assert_eq!(kept.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["test"], "install by name, dev by its command");
    }

    #[test]
    fn suggest_patterns_match_like_globs() {
        assert!(glob("package.json", "package.json"));
        assert!(!glob("package.json", "web/package.json"), "a plain name is the exact path");
        assert!(glob("server/**", "server/a/b.ts") && glob("server/**", "server/x"));
        assert!(!glob("server/**", "client/server/x"));
        assert!(glob("**/*.sql", "db/migrations/001.sql") && glob("**/*.sql", "top.sql"));
        assert!(glob("migrations/*.sql", "migrations/001_init.sql"));
        assert!(!glob("migrations/*.sql", "migrations/old/001.sql"), "* stays in one folder");
        assert!(glob("src/**/test_?.py", "src/a/b/test_1.py"));
        assert!(glob("./Cargo.toml", "Cargo.toml"));
        assert!(!glob("", "x"));
        assert_eq!(relative("/p/app", "/p/app/server/x.ts"), "server/x.ts");
        assert_eq!(relative("/p/app/", "/p/app/a"), "a");
        assert_eq!(relative("/p/app", "/p/application/a"), "");
        assert_eq!(relative("/p/app", "./a/b"), "a/b");
    }
}
