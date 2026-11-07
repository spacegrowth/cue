use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cue-repos-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A test's own folders, looked up with `look`.
fn folders(look: fn(&str) -> Facts, first_wait: Duration) -> &'static Folders {
    Box::leak(Box::new(Folders::new(look, first_wait)))
}

fn until(what: &str, done: impl Fn() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < Duration::from_secs(5), "{what}");
        std::thread::yield_now();
    }
}

fn on_branch(b: &str) -> Facts {
    Facts { root: Some(PathBuf::from("/r")), top: Some(PathBuf::from("/r")), branch: b.into(), ..Facts::default() }
}

#[test]
fn a_folder_known_is_answered_from_memory() {
    static LOOKS: AtomicUsize = AtomicUsize::new(0);
    fn look(_: &str) -> Facts {
        LOOKS.fetch_add(1, Ordering::SeqCst);
        on_branch("main")
    }
    let f = folders(look, Duration::from_secs(2));
    assert_eq!(f.facts("/r/src").map(|x| x.branch).as_deref(), Some("main"), "the first asker waits for a quick lookup");
    for _ in 0..100 {
        assert_eq!(f.facts("/r/src").map(|x| x.branch).as_deref(), Some("main"));
    }
    assert_eq!(LOOKS.load(Ordering::SeqCst), 1, "looked up once, then memory");
}

#[test]
fn a_refresh_picks_up_a_change_and_says_so() {
    /// The disk: whether fix-it is checked out.
    static FIX_IT: AtomicBool = AtomicBool::new(false);
    fn look(_: &str) -> Facts {
        on_branch(if FIX_IT.load(Ordering::SeqCst) { "fix-it" } else { "main" })
    }
    let f = folders(look, Duration::from_secs(2));
    assert_eq!(f.facts("/r").unwrap().branch, "main");
    f.refresh();
    assert!(!f.refresh(), "nothing changed on disk: nothing to redraw");
    FIX_IT.store(true, Ordering::SeqCst);
    assert_eq!(f.facts("/r").unwrap().branch, "main", "memory until the refresh");
    assert!(f.refresh(), "the refresh saw the checkout");
    assert_eq!(f.facts("/r").unwrap().branch, "fix-it");
}

static RELEASE: AtomicBool = AtomicBool::new(false);
static STALLED_LOOKS: AtomicUsize = AtomicUsize::new(0);
/// A folder on a disk that stops answering (an automounted /home) until the test lets it go.
fn stalls_on_home(cwd: &str) -> Facts {
    if cwd.starts_with("/home/") {
        STALLED_LOOKS.fetch_add(1, Ordering::SeqCst);
        until("the test let the stalled disk go", || RELEASE.load(Ordering::SeqCst));
    }
    on_branch("main")
}

#[test]
fn a_stalled_folder_is_unknown_and_holds_nothing_up() {
    let f = folders(stalls_on_home, Duration::from_millis(50));
    let start = Instant::now();
    assert_eq!(f.facts("/home/me/app"), None, "unknown once its first wait is up");
    assert!(start.elapsed() < Duration::from_secs(1), "the first asker waits a moment, not for the disk");
    let again = Instant::now();
    assert_eq!(f.facts("/home/me/app"), None);
    assert!(again.elapsed() < Duration::from_millis(40), "nobody waits on it a second time");
    assert_eq!(f.facts("/Users/me/web").map(|x| x.branch).as_deref(), Some("main"), "other folders still answer");
    let tick = Instant::now();
    f.refresh();
    assert!(tick.elapsed() < Duration::from_secs(3), "a refresh waits for a stalled folder only so long");
    assert_eq!(STALLED_LOOKS.load(Ordering::SeqCst), 1, "a stalled folder isn't looked up again while its lookup is out");
    RELEASE.store(true, Ordering::SeqCst);
    until("the folder came back", || f.facts("/home/me/app").is_some());
    assert!(f.refresh(), "its coming back is a change to redraw");
}

/// A worktree, a subfolder of it and a submodule each find their repository; a plain folder doesn't.
#[test]
fn repo_of_worktree() {
    let run = |dir: &Path, args: &[&str]| assert!(std::process::Command::new("git").current_dir(dir).args(args).output().unwrap().status.success(), "git {args:?}");
    let base = std::env::temp_dir().join(format!("cue-repo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let main = base.join("app");
    std::fs::create_dir_all(main.join("src")).unwrap();
    run(&main, &["init", "-q"]);
    run(&main, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "x"]);
    run(&main, &["worktree", "add", "-q", "../app-wt/fix-it"]);
    let wt = base.join("app-wt/fix-it");
    std::fs::create_dir_all(wt.join("deep")).unwrap();
    let name = |p: &Path| repo_root(p.to_str().unwrap()).map(|r| repo_name(&r));
    assert_eq!(name(&main.join("src")).as_deref(), Some("app"));
    assert_eq!(name(&wt).as_deref(), Some("app"));
    assert_eq!(name(&wt.join("deep")).as_deref(), Some("app"));
    assert_eq!(crate::model::project_of(wt.to_str().unwrap()), "app");
    assert_eq!(crate::model::folder_of(wt.to_str().unwrap()), "fix-it");
    let plain = base.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    assert_eq!(crate::model::project_of(plain.to_str().unwrap()), "plain");
    assert_eq!(repo_name(Path::new("/x/app.git")), "app");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_folders_project_by_the_three_rules() {
    use std::collections::HashMap;
    let run = |dir: &Path, args: &[&str]| assert!(std::process::Command::new("git").current_dir(dir).args(args).output().unwrap().status.success(), "git {args:?}");
    let base = std::env::temp_dir().canonicalize().unwrap().join(format!("cue-rules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let web = base.join("web-app");
    std::fs::create_dir_all(web.join("ui")).unwrap();
    run(&web, &["init", "-q"]);
    run(&web, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "x"]);
    run(&web, &["worktree", "add", "-q", "../web-app-fix"]);
    let docs = base.join("web-app-docs");
    std::fs::create_dir_all(docs.join(".cue")).unwrap();
    run(&docs, &["init", "-q"]);
    std::fs::write(docs.join(".cue/actions.json"), r#"{ "project": "web-app", "actions": [] }"#).unwrap();
    let scratch = base.join("scratch");
    std::fs::create_dir_all(scratch.join("deep/inner-repo")).unwrap();
    run(&scratch.join("deep/inner-repo"), &["init", "-q"]);
    let s = |p: &Path| p.to_str().unwrap().to_string();
    let said = |p: &Path, put: &HashMap<String, String>| crate::model::project_said(&s(p), "", put);
    let none = HashMap::new();
    // 3. its own: the repository, from a subfolder and a worktree too; else the folder.
    assert_eq!(crate::model::own_project(&s(&web.join("ui")), ""), "web-app");
    assert_eq!(crate::model::own_project(&s(&base.join("web-app-fix")), ""), "web-app");
    assert_eq!(said(&web.join("ui"), &none), None);
    assert_eq!(crate::model::own_project(&s(&scratch), ""), "scratch");
    // 2. a repository saying its project.
    assert_eq!(said(&docs, &none).as_deref(), Some("web-app"));
    // 1. a folder you put in a project: it and everything under it, but not a repository inside it.
    let put = HashMap::from([(s(&scratch), "web-app".to_string())]);
    assert_eq!(said(&scratch, &put).as_deref(), Some("web-app"));
    assert_eq!(said(&scratch.join("deep"), &put).as_deref(), Some("web-app"));
    assert_eq!(said(&scratch.join("deep/inner-repo"), &put), None);
    // A repository put in as a whole, by its main checkout: its worktrees follow.
    let put = HashMap::from([(s(&web), "shop".to_string())]);
    assert_eq!(crate::model::put_key(&s(&base.join("web-app-fix")), ""), s(&web));
    assert_eq!(said(&base.join("web-app-fix"), &put).as_deref(), Some("shop"));
    assert_eq!(said(&web.join("ui"), &put).as_deref(), Some("shop"));
    // Yours beats the repository's.
    let put = HashMap::from([(s(&docs), "docs".to_string())]);
    assert_eq!(said(&docs, &put).as_deref(), Some("docs"));
    // Another machine's: by the folders you put in alone, never a look at the path here.
    let put = HashMap::from([("box:/srv/app".to_string(), "web-app".to_string())]);
    assert_eq!(crate::model::project_said("/srv/app/api", "box", &put).as_deref(), Some("web-app"));
    assert_eq!(crate::model::project_said("/srv/other", "box", &put), None);
    assert_eq!(crate::model::put_key("/srv/app/", "box"), "box:/srv/app");
    let _ = std::fs::remove_dir_all(&base);
}


#[test]
fn reads_the_branch_of_a_repo_and_a_worktree() {
    let d = tmp("git");
    std::fs::create_dir_all(d.join("repo/.git")).unwrap();
    std::fs::create_dir_all(d.join("repo/src/deep")).unwrap();
    std::fs::write(d.join("repo/.git/HEAD"), "ref: refs/heads/chart-lines\n").unwrap();
    assert_eq!(branch(d.join("repo/src/deep").to_str().unwrap()), "chart-lines", "found from a subfolder");
    assert_eq!(branch(d.join("repo").to_str().unwrap()), "chart-lines");
    std::fs::create_dir_all(d.join("repo/.git/worktrees/wt")).unwrap();
    std::fs::write(d.join("repo/.git/worktrees/wt/HEAD"), "4f2a9c1e0b7d\n").unwrap();
    std::fs::create_dir_all(d.join("wt")).unwrap();
    std::fs::write(d.join("wt/.git"), format!("gitdir: {}\n", d.join("repo/.git/worktrees/wt").display())).unwrap();
    assert_eq!(branch(d.join("wt").to_str().unwrap()), "4f2a9c1", "a detached worktree shows its commit");
    std::fs::write(d.join("repo/.git/HEAD"), "ref: refs/heads/main\n").unwrap();
    assert_eq!(branch(d.join("repo").to_str().unwrap()), "chart-lines", "read from memory until a refresh");
    refresh();
    assert_eq!(branch(d.join("repo").to_str().unwrap()), "main", "a checkout is picked up by the refresh");
    assert_eq!(branch("/"), "");
    assert_eq!(branch(""), "");
    let _ = std::fs::remove_dir_all(&d);
}
