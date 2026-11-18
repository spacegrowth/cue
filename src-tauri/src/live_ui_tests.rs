//! The window's files served in place, against a git repo and a folder of each test's own.

use super::*;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn put(dir: &Path, path: &str, text: &str) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// `path` written with `text` and committed: the new HEAD.
fn commit(dir: &Path, path: &str, text: &str) -> String {
    put(dir, path, text);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", path]);
    git(dir, &["rev-parse", "HEAD"])
}

/// A repo with a window (ui/index.html) and an app side (src-tauri/main.rs): (its folder, the test's own, HEAD).
fn repo(name: &str) -> (PathBuf, PathBuf, String) {
    let root = std::env::temp_dir().join(format!("cue-live-ui-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("repo");
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.email", "t@example.com"]);
    git(&dir, &["config", "user.name", "t"]);
    put(&dir, "ui/index.html", "<p>one</p>");
    let head = commit(&dir, "src-tauri/main.rs", "fn main() {}");
    (dir, root, head)
}

fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

#[test]
fn only_the_windows_files_changed_reloads_anything_in_the_app_installs() {
    let (dir, root, built) = repo("plan");
    let ui = commit(&dir, "ui/desk.js", "x");
    assert_eq!(plan(&s(&dir), &built, None, &ui), Plan::Reload);
    let tests = commit(&dir, "tests/ui/specs/desk.spec.mjs", "t");
    assert_eq!(plan(&s(&dir), &built, None, &tests), Plan::Reload, "tests don't go into the app");
    assert_eq!(plan(&s(&dir), &built, Some(&ui), &tests), Plan::Nothing, "the window shows that commit's files already");
    let app = commit(&dir, "src-tauri/main.rs", "fn main() { 1; }");
    assert_eq!(plan(&s(&dir), &built, None, &app), Plan::Install);
    let ui_after = commit(&dir, "ui/desk.js", "y");
    assert_eq!(plan(&s(&dir), &built, None, &ui_after), Plan::Install, "the main line has an app change the running Cue lacks");
    assert_eq!(plan(&s(&dir), &app, None, &ui_after), Plan::Reload, "a Cue built with it");
    let script = commit(&dir, "scripts/self-install.sh", "#");
    assert_eq!(plan(&s(&dir), &ui_after, None, &script), Plan::Install, "scripts go into the app");
    let note = commit(&dir, "HANDOFF.md", "n");
    assert_eq!(plan(&s(&dir), &script, None, &note), Plan::Nothing);
    assert_eq!(plan(&s(&dir), "not-a-commit", None, &note), Plan::Install, "git can't say: install");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_copy_is_git_s_commit_never_the_checkout_s_unsaved_edits() {
    let (dir, root, built) = repo("take");
    let head = commit(&dir, "ui/styles/desk.css", "a{}");
    put(&dir, "ui/index.html", "<p>unsaved</p>");
    let live = root.join("live");
    let to = take_in(&live, &s(&dir), &head, &built).unwrap();
    assert_eq!(std::fs::read_to_string(to.join("index.html")).unwrap(), "<p>one</p>");
    assert_eq!(file_in(&to, "/styles/desk.css").as_deref(), Some(&b"a{}"[..]));
    assert_eq!(current_in(&live).map(|c| (c.commit, c.base)), Some((head.clone(), built.clone())));
    assert_eq!(file_in(&to, "/../repo/ui/index.html"), None, "nothing outside the copy");
    assert_eq!(file_in(&to, "/missing.js"), None, "Cue's own then");

    // A newer one replaces it; the older copy is removed.
    let next = commit(&dir, "ui/desk.js", "z");
    let to2 = take_in(&live, &s(&dir), &next, &built).unwrap();
    assert!(!to.exists() && to2.join("desk.js").is_file());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_copy_is_served_after_a_restart_of_the_same_cue_and_dropped_by_a_new_one() {
    let (dir, root, built) = repo("start");
    let head = commit(&dir, "ui/desk.js", "x");
    let live = root.join("live");
    let to = take_in(&live, &s(&dir), &head, &built).unwrap();
    assert_eq!(kept_in(&live, Some(&built)), Some(to.clone()), "the same Cue restarted");
    assert_eq!(kept_in(&live, Some(&head)), None, "a Cue built from another commit has its own files");
    assert!(!live.exists(), "and the copy is gone");
    assert_eq!(kept_in(&live, Some(&built)), None);
    let _ = std::fs::remove_dir_all(root);
}

/// Cue's own window check, committed into a test repo with Cue's node_modules linked in (git ignores the link), and a
/// window of two scripts: core.js declares `helper`, desk.js calls it.
fn repo_with_check(name: &str) -> (PathBuf, PathBuf, String) {
    let (dir, root, _) = repo(name);
    let cue = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    std::os::unix::fs::symlink(cue.join("node_modules"), dir.join("node_modules")).unwrap();
    put(&dir, ".gitignore", "node_modules\n");
    put(&dir, "scripts/check-window.mjs", &std::fs::read_to_string(cue.join("scripts/check-window.mjs")).unwrap());
    put(&dir, "ui/index.html", "<div id=\"app\"></div>\n<script src=\"core.js\"></script>\n<script src=\"desk.js\"></script>\n");
    put(&dir, "ui/core.js", "function helper() { return 1; }\n");
    let head = commit(&dir, "ui/desk.js", "function openDesk() { return helper() + Math.max(1, 2); }\n");
    (dir, root, head)
}

#[test]
fn a_window_whose_names_are_all_declared_passes_its_check() {
    let (dir, root, head) = repo_with_check("check-ok");
    assert_eq!(check_in(&root.join("live"), &s(&dir), &head), Ok(()));
    assert_eq!(checks(&root.join("live")), 0, "its copy is removed");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_commit_calling_a_function_that_is_only_in_the_folder_uncommitted_is_refused() {
    let (dir, root, _) = repo_with_check("check-missing");
    // desk.js calls deskApprovals, committed; the file that declares it sits in the folder, never committed. The
    // folder's own window would load, so tests run there pass: the commit's doesn't.
    put(&dir, "ui/index.html", "<div id=\"app\"></div>\n<script src=\"core.js\"></script>\n<script src=\"desk.js\"></script>\n<script src=\"desk_approvals.js\"></script>\n");
    put(&dir, "ui/desk_approvals.js", "function deskApprovals() { return []; }\n");
    put(&dir, "ui/desk.js", "function openDesk() { return helper() + deskApprovals().length; }\n");
    git(&dir, &["add", "ui/index.html", "ui/desk.js"]);
    git(&dir, &["commit", "-qm", "desk.js"]);
    let head = git(&dir, &["rev-parse", "HEAD"]);
    let said = check_in(&root.join("live"), &s(&dir), &head).unwrap_err();
    assert!(said.contains("missing file: ui/index.html loads ui/desk_approvals.js"), "{said}");

    // With the file committed but its function only in the folder: the name is what's missing.
    put(&dir, "ui/desk_approvals.js", "function deskOther() { return []; }\n");
    git(&dir, &["add", "ui/desk_approvals.js"]);
    let head = commit(&dir, "ui/core.js", "function helper() { return 1; }\n// two\n");
    put(&dir, "ui/desk_approvals.js", "function deskOther() { return []; }\nfunction deskApprovals() { return []; }\n");
    let said = check_in(&root.join("live"), &s(&dir), &head).unwrap_err();
    assert!(said.contains("missing: deskApprovals, used in ui/desk.js:1"), "{said}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_name_declared_in_two_of_the_windows_scripts_is_refused() {
    let (dir, root, _) = repo_with_check("check-twice");
    let head = commit(&dir, "ui/desk.js", "function openDesk() { return helper(); }\nconst helper = () => 2;\n");
    let said = check_in(&root.join("live"), &s(&dir), &head).unwrap_err();
    assert!(said.contains("declared twice: helper, in ui/core.js:1 and ui/desk.js:2"), "{said}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_commit_from_before_the_check_has_none_to_run() {
    let (dir, root, _) = repo("check-none");
    let head = commit(&dir, "ui/desk.js", "missing();\n");
    assert_eq!(check_in(&root.join("live"), &s(&dir), &head), Ok(()));
    let _ = std::fs::remove_dir_all(root);
}

/// The window checks' folders in `live`.
fn checks(live: &Path) -> usize {
    std::fs::read_dir(live).map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".check")).count()).unwrap_or(0)
}

#[test]
fn two_checks_of_one_commit_at_once_each_have_their_own_copy() {
    // Two items done at the same commit are checked at once: one ending took the other's copy from under it (#318).
    let (dir, root, head) = repo_with_check("check-at-once");
    let live = root.join("live");
    let runs: Vec<_> = (0..2).map(|_| { let (live, repo, head) = (live.clone(), s(&dir), head.clone()); std::thread::spawn(move || check_in(&live, &repo, &head)) }).collect();
    for r in runs {
        assert_eq!(r.join().unwrap(), Ok(()));
    }
    assert_eq!(checks(&live), 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn serving_a_copy_leaves_a_check_at_work_alone() {
    let (dir, root, built) = repo("check-tidy");
    let head = commit(&dir, "ui/desk.js", "x");
    let live = root.join("live");
    let at_work = live.join(format!("{head}.1-0.check"));
    std::fs::create_dir_all(&at_work).unwrap();
    take_in(&live, &s(&dir), &head, &built).unwrap();
    assert!(at_work.is_dir(), "a check at work keeps its copy");
    let _ = std::fs::remove_dir_all(root);
}
