//! The self-install, against fakes in a folder of each test's own: a git repo to build from, an "installed" app
//! whose program is a small one that only waits, a build that makes such an app (or fails), and a launcher that runs
//! it and says it's running (or doesn't). Nothing here touches the installed Cue, and nothing is stopped but a PID
//! the test started itself (running its own fake app's program, which the helper checks before it stops one).

use super::*;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/self-install.sh");

/// The waiter's source. With CUE_TEST_HELPER set it first starts that shell command as its child, the way Cue starts
/// the helper from the Desk (the helper then has Cue among its ancestors).
const WAITER: &str = "#include <stdlib.h>\n#include <unistd.h>\nint main(void) {\n  const char *h = getenv(\"CUE_TEST_HELPER\");\n  if (h && fork() == 0) { setsid(); execl(\"/bin/sh\", \"sh\", \"-c\", h, (char *)0); _exit(127); }\n  for (;;) pause();\n}\n";

/// A program that waits until it's stopped, whatever its arguments: the fake apps' `Contents/MacOS/cue`. Compiled
/// once (a copy of a system program is refused by macOS when run from elsewhere).
fn waiter() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("cue-self-install-waiter-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("w.c"), WAITER).unwrap();
        let ok = Command::new("cc").arg("-o").arg(dir.join("waiter")).arg(dir.join("w.c")).status().unwrap().success();
        assert!(ok, "cc");
        dir.join("waiter")
    })
}

/// What the fake launcher does: runs the app and says it's running the commit in it, runs it and says nothing, or
/// doesn't run it at all.
#[derive(Clone, Copy, PartialEq)]
enum Launcher {
    Says,
    Silent,
    Nothing,
}

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    home: PathBuf,
    apps: PathBuf,
    built: PathBuf,
    /// PIDs this test started itself.
    started: Vec<i32>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Every PID this test started: its own, and the ones its fake launcher wrote down.
        let launched: Vec<i32> = read(&self.root.join("launched")).lines().filter_map(|l| l.trim().parse().ok()).collect();
        for &pid in self.started.iter().chain(&launched) {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        let _ = Command::new("chmod").arg("-R").arg("u+w").arg(&self.root).status();
        let _ = std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(&self.repo).args(["worktree", "prune"]).status();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(dir).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

fn commit(repo: &Path, text: &str) -> String {
    std::fs::write(repo.join("file.txt"), text).unwrap();
    git(repo, &["add", "file.txt"]);
    git(repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", text]);
    git(repo, &["rev-parse", "HEAD"])
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

/// Still running: there and not a zombie. A stopped Cue (detached, its parent is launchd) is a zombie until launchd
/// collects it, and still answers `kill -0` meanwhile; under load that's long enough to be looked at.
fn alive(pid: u32) -> bool {
    crate::server::alive(pid as i32) && !Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim_start().starts_with('Z')).unwrap_or(false)
}

/// `exe` with these arguments, started so it isn't this process's child (a child of ours, once stopped, would linger
/// unreaped and still answer `kill -0`): its PID.
fn detached(exe: &str, args: &[&str]) -> u32 {
    let o = Command::new("/bin/sh").arg("-c").arg("\"$0\" \"$@\" </dev/null >/dev/null 2>&1 & echo $!").arg(exe).args(args).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().parse().unwrap()
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("cue-self-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join("src-tauri")).unwrap();
        git(&repo, &["init", "-q"]);
        let f = Fixture { home: root.join("home"), apps: root.join("apps"), built: root.join("built/Cue.app"), root, repo, started: vec![] };
        std::fs::create_dir_all(f.home.join("install")).unwrap();
        // The installed app, "old".
        std::fs::create_dir_all(f.apps.join("Cue.app/Contents/MacOS")).unwrap();
        std::fs::copy(waiter(), f.exe()).unwrap();
        std::fs::write(f.apps.join("Cue.app/Contents/version"), "old").unwrap();
        f
    }

    /// The installed app's program, as the helper names it.
    fn exe(&self) -> String {
        format!("{}/Cue.app/Contents/MacOS/cue", self.apps.display())
    }

    /// `exe` run with `args`, by this test: its PID.
    fn start(&mut self, exe: &str, args: &[&str]) -> u32 {
        let pid = detached(exe, args);
        self.started.push(pid as i32);
        pid
    }

    /// A process that isn't Cue: its PID.
    fn sleeper(&mut self) -> u32 {
        self.start("/bin/sleep", &["60"])
    }

    /// The installed (old) Cue running: its PID. `noted` writes running.json for it, running `commit`.
    fn old_cue_running(&mut self, commit: &str, noted: bool) -> u32 {
        let exe = self.exe();
        let pid = self.start(&exe, &[]);
        if noted {
            write_running(&self.home.join("install"), commit, pid, 1).unwrap();
        }
        pid
    }

    /// The helper's environment: every path in the fixture, the build making a fake app that holds the commit built
    /// (`build_ok`) or failing, and the launcher as `launcher` says. Quitting stops the PIDs the helper names.
    fn env(&self, build_ok: bool, launcher: Launcher) -> Vec<(String, String)> {
        let (r, l) = (self.root.display().to_string(), self.root.join("calls").display().to_string());
        let build = if build_ok {
            format!(
                "echo \"$CUE_INSTALL_EXT_ARGS\" > '{r}/ext_args'; pwd -P > '{r}/built_in'; mkdir -p \"$CUE_INSTALL_BUILT_APP/Contents/MacOS\" && cp '{w}' \"$CUE_INSTALL_BUILT_APP/Contents/MacOS/cue\" && git rev-parse HEAD > \"$CUE_INSTALL_BUILT_APP/Contents/version\"; echo build >> '{l}'",
                w = waiter().display()
            )
        } else {
            format!("echo 'error: it broke here'; echo build >> '{l}'; exit 1")
        };
        let run = format!("pid=$( ( \"$CUE_INSTALL_APP/Contents/MacOS/cue\" </dev/null >/dev/null 2>&1 & echo $! ) ); echo $pid >> '{r}/launched'");
        let say = "printf '{\"commit\":\"%s\",\"pid\":%s,\"at\":%s}' \"$(cat \"$CUE_INSTALL_APP/Contents/version\")\" $pid \"$(( $(date +%s) * 1000 ))\" > \"$CUE_HOME/install/running.json\"";
        let launch = match launcher {
            Launcher::Says => format!("echo launch >> '{l}'; {run}; {say}"),
            Launcher::Silent => format!("echo launch >> '{l}'; {run}"),
            Launcher::Nothing => format!("echo launch >> '{l}'"),
        };
        [
            ("CUE_HOME", self.home.display().to_string()),
            ("CUE_INSTALL_APP_DIR", self.apps.display().to_string()),
            ("CUE_INSTALL_TARGET_DIR", self.root.join("target").display().to_string()),
            ("CUE_INSTALL_BUILT_APP", self.built.display().to_string()),
            ("CUE_INSTALL_BUILD", build),
            ("CUE_INSTALL_LAUNCH", launch),
            ("CUE_INSTALL_QUIT", format!("echo \"quit $CUE_INSTALL_PIDS\" >> '{l}'; kill $CUE_INSTALL_PIDS")),
            ("CUE_INSTALL_SIGN", ":".to_string()),
            ("CUE_INSTALL_START_WAIT", "2".to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }

    /// The helper run here, to its end: its exit code.
    fn run(&self, commit: &str, env: &[(String, String)]) -> i32 {
        let out = Command::new("/bin/bash").arg(SCRIPT).arg(&self.repo).arg(commit).arg("item-1").envs(env.iter().map(|(k, v)| (k, v))).output().unwrap();
        eprintln!("helper: {}{}", String::from_utf8_lossy(&out.stderr), read(&self.home.join("install/log")));
        out.status.code().unwrap_or(-1)
    }

    fn status(&self) -> Status {
        status_in(&self.home.join("install")).expect("status.json")
    }
    /// The commit (or "old") the installed app holds.
    fn installed(&self) -> String {
        read(&self.apps.join("Cue.app/Contents/version")).trim().to_string()
    }
    fn calls(&self) -> Vec<String> {
        read(&self.root.join("calls")).lines().map(|l| l.trim().to_string()).collect()
    }
    fn launched(&self) -> Vec<u32> {
        read(&self.root.join("launched")).lines().filter_map(|l| l.trim().parse().ok()).collect()
    }
}

#[test]
fn a_commit_is_hex_and_nothing_else() {
    assert!(looks_like_commit("4f86239c"));
    assert!(looks_like_commit(&"a".repeat(40)));
    assert!(!looks_like_commit("abc"));
    assert!(!looks_like_commit("--all"));
    assert!(!looks_like_commit("HEAD~1.."));
}

#[test]
fn running_json_is_what_status_and_the_helper_read() {
    let f = Fixture::new("running");
    let dir = f.home.join("install");
    write_running(&dir, "abc1234", 42, 7).unwrap();
    let v: serde_json::Value = serde_json::from_str(&read(&dir.join("running.json"))).unwrap();
    assert_eq!(v, serde_json::json!({ "commit": "abc1234", "pid": 42, "at": 7 }));
    assert_eq!(status_in(&dir), None, "no status before an install");
    std::fs::write(dir.join("status.json"), r#"{"tag":"t","commit":"c","step":"building","log":"","at":5}"#).unwrap();
    assert_eq!(status_in(&dir), Some(Status { tag: "t".into(), commit: "c".into(), step: "building".into(), log: String::new(), at: 5 }));
}

#[test]
fn a_commit_is_in_the_running_cue_when_it_is_the_one_built_or_before_it() {
    let f = Fixture::new("ancestor");
    let a = commit(&f.repo, "a");
    let b = commit(&f.repo, "b");
    let repo = f.repo.to_str().unwrap();
    assert!(is_ancestor(repo, &a, &b));
    assert!(is_ancestor(repo, &b, &b));
    assert!(!is_ancestor(repo, &b, &a), "a later commit isn't in an earlier build");
    assert!(!is_ancestor(repo, &"f".repeat(40), &b), "an unknown commit isn't in it");
}

#[test]
fn this_build_knows_its_commit() {
    // Built from this git checkout, the stamp is a full commit name.
    let c = running_commit().expect("CUE_COMMIT is stamped");
    assert!(c.len() == 40 && looks_like_commit(&c), "{c}");
}

#[test]
fn a_lock_whose_helper_is_gone_is_free_and_a_live_one_is_not() {
    let mut f = Fixture::new("lock");
    let dir = f.home.join("install");
    assert_eq!(running_helper(&dir), None);
    let mut done = Command::new("true").spawn().unwrap();
    let gone = done.id();
    done.wait().unwrap();
    std::fs::write(dir.join("lock"), gone.to_string()).unwrap();
    assert_eq!(running_helper(&dir), None, "stale");
    let live = f.sleeper();
    std::fs::write(dir.join("lock"), live.to_string()).unwrap();
    assert_eq!(running_helper(&dir), Some(live as i32));
    let c = commit(&f.repo, "a");
    let err = start_in(&dir, &f.home, f.repo.to_str().unwrap(), &c, "t", &[]).unwrap_err();
    assert!(err.contains("already running"), "{err}");
    // The helper itself refuses too, and says nothing in status.json (the running one's).
    let env = f.env(true, Launcher::Says);
    assert_eq!(f.run(&c, &env), 3);
    assert_eq!(status_in(&dir), None);
}

#[test]
fn start_refuses_what_isnt_a_commit_or_a_checkout() {
    let f = Fixture::new("refuse");
    let dir = f.home.join("install");
    assert!(start_in(&dir, &f.home, f.repo.to_str().unwrap(), "--force", "t", &[]).is_err());
    assert!(start_in(&dir, &f.home, f.root.to_str().unwrap(), "abcdef1", "t", &[]).is_err());
}

#[test]
fn a_build_installs_keeps_the_old_app_and_waits_for_the_new_one_to_say_its_running() {
    let mut f = Fixture::new("installs");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    // Half-done edits in the checkout never reach the build.
    std::fs::write(f.repo.join("file.txt"), "half done").unwrap();
    // The add-on (git-ignored, outside the repo), node_modules and cue-host builds in the checkout.
    let addon = f.root.join("addon");
    std::fs::create_dir_all(&addon).unwrap();
    std::fs::write(addon.join("tauri.conf.json"), "{}").unwrap();
    std::os::unix::fs::symlink(&addon, f.repo.join("src-tauri/ext")).unwrap();
    std::fs::create_dir_all(f.repo.join("node_modules")).unwrap();
    std::fs::create_dir_all(f.repo.join("src-tauri/binaries")).unwrap();
    std::fs::write(f.repo.join("src-tauri/binaries/cue-host-aarch64-apple-darwin"), "host").unwrap();
    let old_pid = f.old_cue_running(&old, true);

    let env = f.env(true, Launcher::Says);
    assert_eq!(f.run(&new, &env), 0);
    let s = f.status();
    assert_eq!((s.step.as_str(), s.commit.as_str(), s.tag.as_str()), ("installed", new.as_str(), "item-1"));
    assert_eq!(f.installed(), new);
    assert_eq!(read(&f.apps.join("Cue.prev.app/Contents/version")), "old", "the old app is kept beside it");
    assert_eq!(f.calls(), vec!["build".to_string(), format!("quit {old_pid}"), "launch".to_string()]);
    assert!(!alive(old_pid));
    assert!(alive(f.launched()[0]), "the new Cue runs");
    assert!(!f.home.join("install/lock").exists(), "the lock is let go");

    let tree = f.home.join("install/tree");
    assert_eq!(read(&f.root.join("built_in")).trim(), std::fs::canonicalize(&tree).unwrap().to_str().unwrap(), "built in the clean copy");
    assert_eq!(read(&tree.join("file.txt")), "new");
    assert_eq!(read(&f.root.join("ext_args")).trim(), "--features ext --config src-tauri/ext/tauri.conf.json");
    assert_eq!(std::fs::read_link(tree.join("src-tauri/ext")).unwrap(), std::fs::canonicalize(&addon).unwrap());
    assert_eq!(std::fs::read_link(tree.join("node_modules")).unwrap(), f.repo.join("node_modules"));
    assert_eq!(read(&tree.join("src-tauri/binaries/cue-host-aarch64-apple-darwin")), "host");

    // The next install reuses the copy, moved to the next commit; without the add-on, it's built without it.
    std::fs::remove_file(f.repo.join("src-tauri/ext")).unwrap();
    git(&f.repo, &["checkout", "-q", "file.txt"]);
    let next = commit(&f.repo, "next");
    assert_eq!(f.run(&next, &env), 0);
    assert_eq!(f.installed(), next);
    assert_eq!(read(&tree.join("file.txt")), "next");
    assert_eq!(read(&f.root.join("ext_args")).trim(), "");
    assert!(!tree.join("src-tauri/ext").exists());
    assert_eq!(git(&f.repo, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 2, "one copy, reused");
}

#[test]
fn cue_is_quit_by_its_program_and_nothing_else_is_stopped() {
    let mut f = Fixture::new("quits");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    // The running Cue never wrote running.json (an older Cue, a first install): it's found by its program.
    let cue = f.old_cue_running(&old, false);
    // The same program run with arguments: a hook an agent may be waiting on.
    let exe = f.exe();
    let hook = f.start(&exe, &["hook", "stop"]);
    // A running.json left behind whose PID is now some other process's.
    let other = f.sleeper();
    write_running(&f.home.join("install"), &old, other, 1).unwrap();

    let env = f.env(true, Launcher::Says);
    assert_eq!(f.run(&new, &env), 0);
    assert_eq!(f.status().step, "installed");
    assert_eq!(f.calls(), vec!["build".to_string(), format!("quit {cue}"), "launch".to_string()]);
    assert!(!alive(cue), "Cue was quit before its app was replaced");
    assert!(alive(hook), "a hook is never stopped");
    assert!(alive(other), "a PID that isn't Cue's is never stopped");
}

#[test]
fn the_cue_that_started_the_install_is_quit_though_it_is_the_helpers_parent() {
    let mut f = Fixture::new("parent");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    // As from the Desk: the running Cue starts the helper itself, so it is among the helper's ancestors.
    let mut env = f.env(true, Launcher::Says);
    let helper = format!("unset CUE_TEST_HELPER; exec /bin/bash '{SCRIPT}' '{}' {new} item-1 >/dev/null 2>&1", f.repo.display());
    env.push(("CUE_TEST_HELPER".into(), helper));
    let exe = f.exe();
    let o = Command::new("/bin/sh").arg("-c").arg("\"$0\" </dev/null >/dev/null 2>&1 & echo $!").arg(&exe).envs(env.iter().map(|(k, v)| (k, v))).output().unwrap();
    let cue: u32 = String::from_utf8_lossy(&o.stdout).trim().parse().unwrap();
    f.started.push(cue as i32);
    write_running(&f.home.join("install"), &old, cue, 1).unwrap();
    let t = Instant::now();
    while !status_in(&f.home.join("install")).is_some_and(|s| matches!(s.step.as_str(), "installed" | "rolled_back" | "failed")) {
        assert!(t.elapsed() < Duration::from_secs(60), "the helper didn't finish: {}", read(&f.home.join("install/log")));
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(f.status().step, "installed", "{}", read(&f.home.join("install/log")));
    assert_eq!(f.calls(), vec!["build".to_string(), format!("quit {cue}"), "launch".to_string()]);
    assert!(!alive(cue), "the Cue that started it was quit before its app was replaced");
    assert_eq!(f.installed(), new);
}

#[test]
fn a_cue_that_wont_quit_is_stopped_by_its_exact_pid() {
    let mut f = Fixture::new("stubborn");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    let cue = f.old_cue_running(&old, true);
    let mut env = f.env(true, Launcher::Says);
    // Asked to quit, it doesn't.
    env.iter_mut().find(|(k, _)| k == "CUE_INSTALL_QUIT").unwrap().1 = ":".into();
    let t = Instant::now();
    assert_eq!(f.run(&new, &env), 0);
    assert!(t.elapsed() >= Duration::from_secs(30), "it was given 30 s first");
    assert!(!alive(cue));
    assert_eq!(f.installed(), new);
}

#[test]
fn a_failed_build_leaves_the_running_cue_alone() {
    let mut f = Fixture::new("fails");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    let old_pid = f.old_cue_running(&old, true);
    let env = f.env(false, Launcher::Says);
    assert_eq!(f.run(&new, &env), 1);
    let s = f.status();
    assert_eq!(s.step, "failed");
    assert!(s.log.contains("error: it broke here"), "{}", s.log);
    assert_eq!(f.installed(), "old");
    assert!(!f.apps.join("Cue.prev.app").exists());
    assert_eq!(f.calls(), vec!["build"], "never quit nor launched");
    assert!(alive(old_pid));
}

/// The built app is signed where it was built, before Cue is quit (signing it in place once installed is refused to
/// anything without App Management); a signature that fails leaves the running Cue and the installed app alone.
#[test]
fn the_built_app_is_signed_before_cue_is_quit_and_a_failed_signature_installs_nothing() {
    let mut f = Fixture::new("sign");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    f.old_cue_running(&old, true);
    let signs = |f: &Fixture, fails: &str| format!("echo \"sign $CUE_INSTALL_APP\" >> '{}'{fails}", f.root.join("calls").display());
    let mut env = f.env(true, Launcher::Says);
    env.retain(|(k, _)| k != "CUE_INSTALL_SIGN");
    env.push(("CUE_INSTALL_SIGN".into(), signs(&f, "")));
    assert_eq!(f.run(&new, &env), 0);
    assert_eq!(f.status().step, "installed");
    let calls_of = |f: &Fixture| f.calls().into_iter().map(|c| c.split(' ').next().unwrap_or("").to_string()).collect::<Vec<_>>();
    assert_eq!(calls_of(&f)[..3], ["build", "sign", "quit"], "signed before Cue is quit");
    assert_eq!(f.calls()[1], format!("sign {}", f.built.display()), "the built app, not the installed one");

    let mut f = Fixture::new("sign-fails");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    let old_pid = f.old_cue_running(&old, true);
    let mut env = f.env(true, Launcher::Says);
    env.retain(|(k, _)| k != "CUE_INSTALL_SIGN");
    env.push(("CUE_INSTALL_SIGN".into(), signs(&f, "; echo 'Operation not permitted'; exit 1")));
    assert_eq!(f.run(&new, &env), 1);
    let s = f.status();
    assert_eq!(s.step, "failed");
    assert!(s.log.contains("Operation not permitted") && s.log.contains("couldn't sign"), "{}", s.log);
    assert_eq!(f.installed(), "old");
    assert_eq!(calls_of(&f), ["build", "sign"], "never quit nor launched");
    assert!(alive(old_pid));
}

/// A commit whose window calls a function no committed script declares (it sat uncommitted in the folder): refused
/// by the window's check before anything is built, the running Cue left alone, and the log names the function.
#[test]
fn a_commit_calling_an_uncommitted_function_is_refused_before_the_build() {
    let mut f = Fixture::new("window");
    let cue = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    std::os::unix::fs::symlink(cue.join("node_modules"), f.repo.join("node_modules")).unwrap();
    let repo = f.repo.clone();
    let put = |path: &str, text: &str| {
        let p = repo.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
        git(&repo, &["add", path]);
    };
    put("scripts/check-window.mjs", &std::fs::read_to_string(cue.join("scripts/check-window.mjs")).unwrap());
    put("ui/index.html", "<div id=\"app\"></div>\n<script src=\"core.js\"></script>\n<script src=\"desk.js\"></script>\n");
    put("ui/core.js", "function helper() { return 1; }\n");
    put("ui/desk.js", "function openDesk() { return helper(); }\n");
    let old = commit(&f.repo, "old");
    put("ui/desk.js", "function openDesk() { return helper() + deskApprovals().length; }\n");
    let new = commit(&f.repo, "new");
    // Where it was written, uncommitted: the folder's own window would load.
    std::fs::write(f.repo.join("ui/core.js"), "function helper() { return 1; }\nfunction deskApprovals() { return []; }\n").unwrap();
    let old_pid = f.old_cue_running(&old, true);
    let env = f.env(true, Launcher::Says);
    assert_eq!(f.run(&new, &env), 1);
    let s = f.status();
    assert_eq!(s.step, "failed");
    assert!(s.log.contains("missing: deskApprovals, used in ui/desk.js:1"), "{}", s.log);
    assert!(s.log.contains("refused: the window"), "{}", s.log);
    assert_eq!(f.installed(), "old");
    assert!(f.calls().is_empty(), "never built, quit nor launched: {:?}", f.calls());
    assert!(alive(old_pid));

    // The same window with the function committed: built and installed.
    put("ui/core.js", "function helper() { return 1; }\nfunction deskApprovals() { return []; }\n");
    let fixed = commit(&f.repo, "fixed");
    assert_eq!(f.run(&fixed, &env), 0);
    assert_eq!(f.installed(), fixed);
}

#[test]
fn an_app_that_cant_be_swapped_after_quitting_is_started_again() {
    let mut f = Fixture::new("noswap");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    let old_pid = f.old_cue_running(&old, true);
    // A Cue.prev.app that can't be cleared away.
    let stuck = f.apps.join("Cue.prev.app/stuck");
    std::fs::create_dir_all(&stuck).unwrap();
    std::fs::write(stuck.join("file"), "").unwrap();
    std::fs::set_permissions(&stuck, std::os::unix::fs::PermissionsExt::from_mode(0o555)).unwrap();
    let env = f.env(true, Launcher::Says);
    assert_eq!(f.run(&new, &env), 1);
    let s = f.status();
    assert_eq!(s.step, "failed");
    assert!(s.log.contains("couldn't clear"), "{}", s.log);
    assert_eq!(f.installed(), "old");
    assert_eq!(f.calls(), vec!["build".to_string(), format!("quit {old_pid}"), "launch".to_string()], "started again");
    assert!(alive(f.launched()[0]), "a Cue runs");
}

#[test]
fn a_new_cue_that_doesnt_say_its_running_is_rolled_back() {
    for launcher in [Launcher::Nothing, Launcher::Silent] {
        let mut f = Fixture::new(if launcher == Launcher::Silent { "rollback-silent" } else { "rollback" });
        let old = commit(&f.repo, "old");
        let new = commit(&f.repo, "new");
        let old_pid = f.old_cue_running(&old, true);
        let env = f.env(true, launcher);
        assert_eq!(f.run(&new, &env), 1);
        let s = f.status();
        assert_eq!(s.step, "rolled_back");
        assert!(s.log.contains("putting the old one back"), "{}", s.log);
        assert_eq!(f.installed(), "old", "the old app is back");
        assert!(!f.apps.join("Cue.prev.app").exists());
        let mut calls = vec!["build".to_string(), format!("quit {old_pid}"), "launch".to_string()];
        if launcher == Launcher::Silent {
            // Running without having said so: quit before the old app goes back.
            let launched = f.launched();
            calls.push(format!("quit {}", launched[0]));
            assert!(!alive(launched[0]), "the new Cue is stopped");
            assert!(alive(launched[1]), "the old one is started again");
        }
        calls.push("launch".to_string());
        assert_eq!(f.calls(), calls, "and started again");
    }
}

#[test]
fn an_old_app_that_cant_be_put_back_is_a_failure_not_a_rollback() {
    let mut f = Fixture::new("noroll");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    f.old_cue_running(&old, true);
    let mut env = f.env(true, Launcher::Nothing);
    // The kept app goes missing while the new one is given its time.
    env.iter_mut().find(|(k, _)| k == "CUE_INSTALL_LAUNCH").unwrap().1.push_str(&format!("; rm -rf '{}'", f.apps.join("Cue.prev.app").display()));
    assert_eq!(f.run(&new, &env), 1);
    let s = f.status();
    assert_eq!(s.step, "failed");
    assert!(s.log.contains("couldn't put the old app back"), "{}", s.log);
}

#[test]
fn a_build_copy_inside_another_repo_is_never_checked_out_there() {
    let f = Fixture::new("nested");
    let new = commit(&f.repo, "new");
    // The copy's folder is inside another repo, with work of its own.
    let other = f.root.join("other");
    std::fs::create_dir_all(other.join("tree")).unwrap();
    git(&other, &["init", "-q"]);
    commit(&other, "theirs");
    std::fs::write(other.join("file.txt"), "their edit").unwrap();
    let mut env = f.env(true, Launcher::Says);
    env.push(("CUE_INSTALL_TREE".into(), other.join("tree").display().to_string()));
    assert_eq!(f.run(&new, &env), 0);
    assert_eq!(read(&other.join("file.txt")), "their edit", "the other repo is untouched");
    assert_eq!(read(&other.join("tree/file.txt")), "new", "the copy was made afresh");
}

#[test]
fn start_detaches_the_helper_and_says_each_step() {
    let mut f = Fixture::new("detached");
    let old = commit(&f.repo, "old");
    let new = commit(&f.repo, "new");
    f.old_cue_running(&old, true);
    let mut env = f.env(true, Launcher::Says);
    // A slow build, so the helper is caught running.
    env.iter_mut().find(|(k, _)| k == "CUE_INSTALL_BUILD").unwrap().1.insert_str(0, "sleep 1; ");
    env.push(("CUE_SELF_INSTALL_SCRIPT".into(), SCRIPT.into()));
    let envs: Vec<(&str, &str)> = env.iter().filter(|(k, _)| k != "CUE_HOME").map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let dir = f.home.join("install");
    let t = Instant::now();
    start_in(&dir, &f.home, f.repo.to_str().unwrap(), &new, "item-2", &envs).unwrap();
    assert!(t.elapsed() < Duration::from_millis(800), "start doesn't wait for the install");
    let helper = running_helper(&dir).expect("the lock names the helper");
    assert_eq!(read_ps(helper as u32, "pgid"), helper.to_string(), "a session and process group of its own");
    assert!(start_in(&dir, &f.home, f.repo.to_str().unwrap(), &new, "item-3", &envs).is_err(), "one at a time");
    let mut steps = vec![];
    while t.elapsed() < Duration::from_secs(30) {
        if let Some(s) = status_in(&dir) {
            if steps.last() != Some(&s.step) {
                steps.push(s.step.clone());
            }
            if s.step == "installed" {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(steps.first().map(String::as_str), Some("building"));
    assert_eq!(steps.last().map(String::as_str), Some("installed"), "{steps:?}");
    assert_eq!(status_in(&dir).unwrap().tag, "item-2");
    assert_eq!(f.installed(), new);
}

fn read_ps(pid: u32, field: &str) -> String {
    let o = Command::new("ps").args(["-o", &format!("{field}="), "-p", &pid.to_string()]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}
