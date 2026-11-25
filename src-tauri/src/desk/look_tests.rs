//! New Desk's look (desk/look.rs, desk/look_sessions.rs) on temp folders and fake transcripts.

use crate::desk::lands::{self, Place};
use crate::desk::look::{self, rows_in};
use crate::desk::look_sessions;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// A fresh temp folder of this test's own.
fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cue-look-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new(crate::safe_git::git_bin())
        .args(["-c", "user.name=t", "-c", "user.email=t@example.test", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
}

/// A repository on main whose commits have these subjects, oldest first.
fn repo(name: &str, subjects: &[&str]) -> PathBuf {
    let d = temp(name);
    git(&d, &["init", "-q"]);
    for (i, s) in subjects.iter().enumerate() {
        write(&d, "n.txt", &i.to_string());
        git(&d, &["add", "."]);
        git(&d, &["commit", "-q", "-m", s]);
    }
    d
}

fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

fn row<'a>(look: &'a Value, part: &str) -> &'a Value {
    look["rows"].as_array().unwrap().iter().find(|r| r["part"] == part).unwrap_or_else(|| panic!("no {part} row in {look}"))
}

/// A fake transcript: one Bash call per command, on `branch`, with a title.
fn transcript(root: &Path, dir: &str, id: &str, title: &str, branch: &str, cmds: &[&str]) {
    let folder = look::transcripts_dir(root, dir);
    std::fs::create_dir_all(&folder).unwrap();
    let mut text = format!("{}\n", json!({ "type": "custom-title", "customTitle": title, "sessionId": id }));
    text.push_str(&format!("{}\n", json!({ "type": "user", "message": { "role": "user", "content": "please fix the cart" }, "gitBranch": branch })));
    for c in cmds {
        text.push_str(&format!("{}\n", json!({ "type": "assistant", "gitBranch": branch, "message": { "content": [{ "type": "tool_use", "name": "Bash", "input": { "command": c } }] } })));
    }
    std::fs::write(folder.join(format!("{id}.jsonl")), text).unwrap();
}

fn files_found(dir: &Path) -> Value {
    look::files(&s(dir)).1
}

#[test]
fn ci_steps_come_first_for_the_check_and_package_json_gives_the_build() {
    let d = temp("ci");
    write(&d, "package.json", r#"{ "scripts": { "build": "vite build", "test": "vitest run", "lint": "eslint ." } }"#);
    write(&d, ".github/workflows/ci.yml", "on:\n  pull_request:\njobs:\n  t:\n    steps:\n      - uses: actions/checkout@v4\n      - run: npm ci\n      - run: npm run lint\n      - name: tests\n        run: |\n          npm test\n");
    let (lines, files) = look::files(&s(&d));
    assert!(lines.iter().any(|l| l["file"] == "package.json" && l["text"].as_str().unwrap().contains("build, test, lint")), "{lines:?}");
    let look = rows_in(Place::Repo, &json!({ "files": files }));
    let check = row(&look, "check");
    assert_eq!((check["tag"].as_str(), check["choice"].as_str()), (Some("found"), Some("command")));
    assert_eq!(check["steps"], json!(["[Lint] npm run lint", "[Test] npm test"]));
    assert_eq!(check["sources"], json!([{ "kind": "file", "file": ".github/workflows/ci.yml" }]));
    assert!(check["why"].as_str().unwrap().contains("GitHub checks"));
    assert_eq!(row(&look, "build")["steps"], json!(["[Build] npm run build"]));
    assert_eq!(look["lands"]["check"], "command");
    assert_eq!(look["lands"]["check_steps"], json!(["npm run build", "npm run lint", "npm test"]), "the build first, then the check");
}

#[test]
fn a_makefile_gives_its_targets() {
    let d = temp("make");
    write(&d, "Makefile", "VAR := 1\n.PHONY: build test\nbuild:\n\tcc main.c\ntest: build\n\t./run-tests\nlint:\n\tclang-tidy main.c\n");
    let look = rows_in(Place::Repo, &json!({ "files": files_found(&d) }));
    assert_eq!(row(&look, "build")["steps"], json!(["[Build] make build"]));
    assert_eq!(row(&look, "check")["steps"], json!(["[Lint] make lint", "[Test] make test"]));
    assert_eq!(row(&look, "check")["sources"], json!([{ "kind": "file", "file": "Makefile" }]));
}

#[test]
fn readme_lines_are_used_when_nothing_else_says() {
    let d = temp("readme");
    write(&d, "README.md", "# App\n\n```sh\n$ ./configure\n$ make build\n$ pytest -q\n```\n\n```python\nprint('test')\n```\n");
    let (lines, files) = look::files(&s(&d));
    assert!(lines.iter().any(|l| l["text"] == "No Makefile"), "{lines:?}");
    let look = rows_in(Place::Repo, &json!({ "files": files }));
    assert_eq!(row(&look, "build")["steps"], json!(["[Build] make build"]));
    assert_eq!(row(&look, "check")["steps"], json!(["[Test] pytest -q"]));
    assert_eq!(row(&look, "check")["sources"], json!([{ "kind": "file", "file": "README.md" }]));
}

#[test]
fn squashed_pull_requests_on_main_mean_pull_request_and_push_follows() {
    let d = repo("squash", &["first", "Add cart (#12)", "Fix cart (#13)", "Tidy (#14)"]);
    let (lines, history) = look::history(&s(&d));
    assert_eq!(lines[0]["text"], "Last 4 changes on main: 3 pull requests, 1 direct");
    let look = rows_in(Place::Repo, &json!({ "history": history }));
    let how = row(&look, "how");
    assert_eq!((how["tag"].as_str(), how["choice"].as_str()), (Some("found"), Some("pr")));
    assert_eq!(how["sources"], json!([{ "kind": "history" }]));
    let push = row(&look, "push");
    assert_eq!((push["tag"].as_str(), push["choice"].as_str()), (Some("follows"), Some("after")));
    assert_eq!((look["lands"]["how"].as_str(), look["lands"]["where"].as_str(), look["lands"]["push"].as_str()), (Some("pr"), Some("copy"), Some("after")));
}

#[test]
fn direct_commits_mean_one_commit_each() {
    let d = repo("direct", &["one", "two", "three"]);
    let look = rows_in(Place::Repo, &json!({ "history": look::history(&s(&d)).1 }));
    let how = row(&look, "how");
    assert_eq!((how["tag"].as_str(), how["choice"].as_str()), (Some("found"), Some("squash")));
    assert_eq!(row(&look, "push")["choice"], "ask");
}

#[test]
fn nothing_to_go_on_keeps_the_safe_choices_as_couldnt_tell() {
    let look = rows_in(Place::Repo, &json!({}));
    assert_eq!((row(&look, "how")["tag"].as_str(), row(&look, "how")["choice"].as_str()), (Some("unsure"), Some("squash")));
    assert_eq!((row(&look, "check")["tag"].as_str(), row(&look, "check")["choice"].as_str()), (Some("unsure"), Some("none")));
    assert_eq!(row(&look, "push")["choice"], "ask");
    assert_eq!(look["lands"], lands::defaults(Place::Repo).to_json());
    let order: Vec<&str> = look["rows"].as_array().unwrap().iter().map(|r| r["part"].as_str().unwrap()).collect();
    assert_eq!(order, ["build", "check", "how", "after", "push"]);
    assert_eq!(look["summary"], "2 to pick");
}

#[test]
fn files_win_over_past_sessions_and_the_why_says_both() {
    let d = temp("disagree");
    write(&d, "package.json", r#"{ "scripts": { "test": "vitest run" } }"#);
    let root = temp("disagree-root");
    let dir = s(&d);
    transcript(&root, &dir, "a1", "fix-cart", "work", &["npm test 2>&1 | tail -20", "git status"]);
    transcript(&root, &dir, "b2", "api-tests", "work", &["cd web && npm test -- cart"]);
    let (lines, sessions) = look_sessions::sessions(&root, &dir, &|_| None);
    assert_eq!(lines[0]["text"], "2 past sessions here: 2 ran tests, 0 pushed");
    let look = rows_in(Place::Repo, &json!({ "files": files_found(&d), "sessions": sessions }));
    let check = row(&look, "check");
    assert_eq!(check["steps"], json!(["[Test] npm test"]));
    assert_eq!(check["sources"], json!([{ "kind": "file", "file": "package.json" }]));
    let why = check["why"].as_str().unwrap();
    assert!(why.contains("Your past sessions ran npm test; package.json's test script is vitest run, so that's used."), "{why}");
}

#[test]
fn past_sessions_alone_can_give_found_and_cue_names_them() {
    let d = temp("alone");
    let root = temp("alone-root");
    let dir = s(&d);
    transcript(&root, &dir, "a1", "untitled", "work", &["CI=1 cargo test --lib cart"]);
    transcript(&root, &dir, "b2", "two", "work", &["cargo test"]);
    let names = |id: &str| (id == "a1").then(|| "fix-cart".to_string());
    let look = rows_in(Place::Repo, &json!({ "sessions": look_sessions::sessions(&root, &dir, &names).1 }));
    let check = row(&look, "check");
    assert_eq!((check["tag"].as_str(), check["steps"].clone()), (Some("found"), json!(["[Test] cargo test"])));
    assert_eq!(check["sources"][0]["kind"], "sessions");
    assert_eq!(check["sources"][0]["count"], 2);
    let mut names: Vec<String> = serde_json::from_value(check["sources"][0]["names"].clone()).unwrap();
    names.sort();
    assert_eq!(names, ["fix-cart", "two"], "Cue's name first, else the transcript's title");
}

#[test]
fn sessions_that_pushed_after_merging_mean_push_right_away_and_their_deploy_runs_after() {
    let d = repo("pushed", &["one", "two", "three"]);
    let root = temp("pushed-root");
    let dir = s(&d);
    transcript(&root, &dir, "a1", "fix-cart", "main", &["git merge work/cart", "git push origin main", "./scripts/deploy.sh prod"]);
    transcript(&root, &dir, "b2", "api", "main", &["git merge work/api && git push", "git push --force"]);
    transcript(&root, &dir, "c3", "notes", "work", &["git commit -m wip"]);
    let sessions = look_sessions::sessions(&root, &dir, &|_| None).1;
    let look = rows_in(Place::Repo, &json!({ "history": look::history(&dir).1, "sessions": sessions }));
    let push = row(&look, "push");
    assert_eq!((push["tag"].as_str(), push["choice"].as_str()), (Some("found"), Some("after")));
    assert_eq!(push["sources"][0]["count"], 2);
    let after = row(&look, "after");
    assert_eq!((after["tag"].as_str(), after["choice"].as_str()), (Some("found"), Some("command")));
    assert_eq!(look["lands"]["after_steps"], json!(["./scripts/deploy.sh"]));
    assert!(look["lands"]["after_steps"].as_array().unwrap().iter().all(|s| !s.as_str().unwrap().contains("push")), "never a step that pushes");
}

#[test]
fn a_deploy_script_with_nothing_saying_when_is_couldnt_tell() {
    let d = temp("deploy");
    write(&d, "scripts/deploy.sh", "#!/bin/sh\necho deploy\n");
    let look = rows_in(Place::Repo, &json!({ "files": files_found(&d) }));
    let after = row(&look, "after");
    assert_eq!((after["tag"].as_str(), after["choice"].as_str()), (Some("unsure"), Some("nothing")));
    assert_eq!(after["why"], "Found scripts/deploy.sh, but nothing says it runs after each change. Until you pick, nothing runs.");
    assert_eq!(look["lands"]["after"], "nothing");
}

#[test]
fn ci_deploying_on_push_to_main_runs_after() {
    let d = temp("cideploy");
    write(&d, ".github/workflows/deploy.yml", "on:\n  push:\n    branches: [main]\njobs:\n  d:\n    steps:\n      - run: npm run deploy\n      - run: git push origin gh-pages\n");
    let look = rows_in(Place::Repo, &json!({ "files": files_found(&d) }));
    assert_eq!(row(&look, "after")["tag"], "found");
    assert_eq!(look["lands"]["after_steps"], json!(["npm run deploy"]));
}

#[test]
fn a_folder_that_isnt_a_repository_has_no_how_or_push_row() {
    let d = temp("plain");
    write(&d, "Makefile", "test:\n\t./t\n");
    let dir = s(&d);
    let found = json!({
        "files": look::look(&dir, "files", &|_| None).unwrap()["found"],
        "github": look::look(&dir, "github", &|_| None).unwrap()["found"],
        "history": look::look(&dir, "history", &|_| None).unwrap()["found"],
    });
    assert_eq!(found["github"]["reached"], false);
    let look = look::rows(&dir, &found).unwrap();
    assert_eq!(look["place"], "folder");
    let parts: Vec<&str> = look["rows"].as_array().unwrap().iter().map(|r| r["part"].as_str().unwrap()).collect();
    assert_eq!(parts, ["build", "check", "after"]);
    assert_eq!((look["lands"]["where"].as_str(), look["lands"]["how"].as_str()), (Some("shared"), Some("owner")));
    assert!(look::look(&format!("{dir}/nope"), "files", &|_| None).unwrap_err().starts_with("No folder at "));
}

#[test]
fn the_lands_it_gives_pass_save() {
    let home = temp("save-home");
    let _turn = crate::db::test_turn();
    let was = std::env::var("CUE_HOME").ok();
    std::env::set_var("CUE_HOME", &home);
    crate::config::refresh();
    let d = repo("save", &["Add cart (#1)", "Fix (#2)"]);
    write(&d, "package.json", r#"{ "scripts": { "build": "vite build", "test": "vitest run" } }"#);
    write(&d, "scripts/deploy.sh", "echo\n");
    let dir = s(&d);
    let look = rows_in(Place::Repo, &json!({ "files": files_found(&d), "history": look::history(&dir).1 }));
    let saved = lands::save("look-test", Place::Repo, &look["lands"]);
    let folder = rows_in(Place::NoRepo, &json!({ "files": files_found(&d) }));
    let saved_folder = lands::save("look-test-folder", Place::NoRepo, &folder["lands"]);
    match was {
        Some(v) => std::env::set_var("CUE_HOME", v),
        None => std::env::remove_var("CUE_HOME"),
    }
    crate::config::refresh();
    let saved = saved.unwrap();
    assert_eq!((saved.how, saved.check, saved.push), ("pr", "command", "after"));
    assert_eq!(saved.check_steps, ["npm run build", "npm test"]);
    assert_eq!(saved_folder.unwrap().at, "shared");
}

/// This repository's own look, printed (run by hand with --ignored).
#[test]
#[ignore]
fn print_this_repos_look() {
    let dir = s(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap());
    let files = look::look(&dir, "files", &|_| None).unwrap();
    let history = look::look(&dir, "history", &|_| None).unwrap();
    println!("{}", serde_json::to_string_pretty(&json!([files, history])).unwrap());
    let found = json!({ "files": files["found"], "history": history["found"] });
    println!("{}", serde_json::to_string_pretty(&rows_in(Place::Repo, &found)).unwrap());
    println!("{}", serde_json::to_string_pretty(&look::rows(&dir, &found).unwrap()).unwrap());
    // Past sessions of the main checkout this worktree was made from: how long the read takes, and what it says.
    let common = std::process::Command::new(crate::safe_git::git_bin()).args(["-C", &dir, "rev-parse", "--path-format=absolute", "--git-common-dir"]).output().unwrap();
    let main = s(Path::new(String::from_utf8_lossy(&common.stdout).trim()).parent().unwrap());
    let t = std::time::Instant::now();
    // Tests run with a home folder of their own: the real one is the account's.
    // SAFETY: getpwuid reads this account's entry; its fields are read before any other call.
    let home = unsafe { std::ffi::CStr::from_ptr((*libc::getpwuid(libc::getuid())).pw_dir) }.to_string_lossy().to_string();
    let (lines, found) = look_sessions::sessions(&Path::new(&home).join(".claude/projects"), &main, &|_| None);
    let sessions = json!({ "lines": lines, "found": found });
    println!("sessions in {:?}: {}", t.elapsed(), sessions["lines"]);
    let c = &sessions["found"]["commands"];
    println!("{} commands, top: {:?}", c.as_array().map_or(0, Vec::len), c.as_array().into_iter().flatten().filter(|c| c["count"].as_u64() > Some(2)).map(|c| (c["cmd"].as_str().unwrap_or(""), c["count"].as_u64().unwrap_or(0))).collect::<Vec<_>>());
    println!("pushed_after_landing {} landed {} pr_create {} after {}", sessions["found"]["pushed_after_landing"], sessions["found"]["landed"], sessions["found"]["pr_create"], sessions["found"]["after"]);
}

#[test]
fn a_release_flag_is_a_build_not_a_deploy() {
    assert_eq!(look::kind_of("cargo build --release"), Some("build"));
    assert_eq!(look::kind_of("npm run release"), Some("deploy"));
    assert_eq!(look::kind_of("./scripts/release.sh 1.2"), Some("deploy"));
    assert_eq!(look::kind_of("make deploy"), Some("deploy"));
    let d = temp("relflag");
    write(&d, ".github/workflows/ci.yml", "on:\n  push:\n    branches: [main]\njobs:\n  b:\n    steps:\n      - run: cargo build --release\n      - run: cargo test --release\n");
    let look = rows_in(Place::Repo, &json!({ "files": files_found(&d) }));
    assert_eq!(row(&look, "build")["steps"], json!(["[Build] cargo build --release"]));
    assert_eq!(row(&look, "check")["steps"], json!(["[Test] cargo test --release"]));
    assert_eq!(row(&look, "after")["choice"], "nothing");
}

#[test]
fn git_switches_before_the_command_are_skipped() {
    let root = temp("gitc-root");
    let dir = "/nowhere/gitc";
    transcript(&root, dir, "a1", "web", "main", &["git -C web merge work/cart", "git -c core.pager=cat --git-dir .git push origin main"]);
    let found = look_sessions::sessions(&root, dir, &|_| None).1;
    assert_eq!((found["merged"].as_u64(), found["landed"].as_u64(), found["pushed_after_landing"].as_u64()), (Some(1), Some(1), Some(1)));
}

#[test]
fn catching_a_branch_up_with_main_is_not_landing() {
    let root = temp("catchup-root");
    let dir = "/nowhere/catchup";
    transcript(&root, dir, "a1", "work", "work/cart", &["git merge origin/main", "git push", "./scripts/deploy.sh"]);
    transcript(&root, dir, "b2", "pull", "work/api", &["git merge main"]);
    let found = look_sessions::sessions(&root, dir, &|_| None).1;
    assert_eq!((found["merged"].as_u64(), found["landed"].as_u64(), found["pushed_after_landing"].as_u64()), (Some(0), Some(0), Some(0)));
    assert_eq!(found["after"], json!([]), "nothing ran after landing: nothing landed");
}

#[test]
fn a_merge_on_a_feature_branch_lands_nothing_and_merge_switches_are_read_past() {
    let root = temp("featmerge-root");
    let dir = "/nowhere/featmerge";
    transcript(&root, dir, "a1", "api", "work/api", &["git merge work/other", "git push", "./scripts/deploy.sh"]);
    let found = look_sessions::sessions(&root, dir, &|_| None).1;
    assert_eq!((found["merged"].as_u64(), found["landed"].as_u64(), found["pushed_after_landing"].as_u64()), (Some(0), Some(0), Some(0)));
    let root = temp("mergeflags-root");
    let dir = "/nowhere/mergeflags";
    transcript(&root, dir, "a1", "up", "main", &["git merge -X theirs origin/main", "git merge -s ours main"]);
    transcript(&root, dir, "b2", "in", "main", &["git merge --no-ff -X theirs work/cart"]);
    let found = look_sessions::sessions(&root, dir, &|_| None).1;
    assert_eq!(found["landed"].as_u64(), Some(1), "only the work merged into main lands");
    let root = temp("mergemsg-root");
    let dir = "/nowhere/mergemsg";
    transcript(&root, dir, "a1", "up", "main", &["git merge -m \"Sync main\" origin/main"]);
    transcript(&root, dir, "b2", "in", "main", &["git merge -m 'Add the cart' work/cart"]);
    let found = look_sessions::sessions(&root, dir, &|_| None).1;
    assert_eq!(found["landed"].as_u64(), Some(1), "a quoted message is read past: the catch-up doesn't land, the cart does");
}

#[test]
fn deploy_and_release_count_only_as_the_program_or_script_run() {
    assert_eq!(look::kind_of("cargo test --manifest-path release/Cargo.toml"), Some("test"));
    assert_eq!(look::kind_of("cargo test deploy_tests"), Some("test"));
    assert_eq!(look::kind_of("fly deploy"), Some("deploy"));
    assert_eq!(look::kind_of("sh scripts/deploy.sh"), Some("deploy"));
    assert_eq!(look::kind_of("npm publish"), Some("deploy"));
    assert_eq!(look::kind_of("./install.sh --app"), Some("deploy"));
}

#[test]
fn a_github_that_refuses_or_doesnt_know_the_repo_is_out_of_reach() {
    let fail = |err: &str| (false, String::new(), err.to_string());
    assert_eq!(look::github_answer(Some(&fail("gh: Not Found (HTTP 404)")), Some(&fail("gh: Not Found (HTTP 404)"))), (false, false, 0));
    assert_eq!(look::github_answer(Some(&fail("gh: Forbidden (HTTP 403)")), Some(&fail("gh: Forbidden (HTTP 403)"))), (false, false, 0));
    assert_eq!(look::github_answer(None, None), (false, false, 0), "gh missing or too slow");
    assert_eq!(look::github_answer(Some(&fail("HTTP 404")), Some(&(false, String::new(), "gh: Branch not protected (HTTP 404)".into()))), (true, false, 0));
    assert_eq!(look::github_answer(Some(&(true, "[]".into(), String::new())), Some(&fail("gh: Forbidden (HTTP 403)"))), (true, false, 0), "the rules answered: none");
    let rules = r#"[{"type":"pull_request","parameters":{"required_approving_review_count":2}}]"#;
    assert_eq!(look::github_answer(Some(&(true, rules.into(), String::new())), None), (true, true, 2));
}
