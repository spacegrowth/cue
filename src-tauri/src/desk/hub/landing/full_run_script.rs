//! The app's own full runner (full_run_kit.rs `FullRunner`): scripts/full-run.sh as a child process on a thread of
//! its own. On this machine it runs from the project's main checkout. On another machine the commit is sent straight
//! to the project's clone there, over the ssh Cue already uses (a git push of the commit to `refs/cue/full-run` in that
//! folder: never to the project's online home, never a branch), the script is run there as it is at that commit, and
//! its result file is read back. Nothing of the machine's folders is looked at from here.
#![cfg_attr(test, allow(dead_code))]

use crate::desk::hub::landing::full_run_kit::{parse, FullRunAsk, FullRunEnd, FullRunner};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The ref the commit is sent to in the machine's clone.
const REF: &str = "refs/cue/full-run";

/// The longest a run may take before its process is ended: a backstop (a run is about half an hour; the search for
/// the commit that broke it can double that).
const CAP: Duration = Duration::from_secs(3 * 3600);

/// The line the script run on a machine prints before the result file.
const MARK: &str = "cue-full-run exit ";

/// One run: the flag that stops it, and how it ended once it has.
struct Run {
    stop: Arc<AtomicBool>,
    end: Option<FullRunEnd>,
}

#[derive(Default)]
pub(crate) struct ScriptRunner {
    runs: Mutex<HashMap<String, Run>>,
}

/// The app's one runner.
pub(crate) fn runner() -> Arc<ScriptRunner> {
    static ONE: std::sync::OnceLock<Arc<ScriptRunner>> = std::sync::OnceLock::new();
    ONE.get_or_init(Arc::default).clone()
}

impl ScriptRunner {
    /// `repo`'s run, the one stopped by `stop`, ended with `end` (a run given up on since says nothing).
    fn ended(&self, repo: &str, stop: &Arc<AtomicBool>, end: FullRunEnd) {
        let mut runs = crate::locking::lock(&self.runs, "full runs");
        if let Some(r) = runs.get_mut(repo).filter(|r| Arc::ptr_eq(&r.stop, stop)) {
            r.end = Some(end);
        }
    }
}

impl FullRunner for ScriptRunner {
    fn start(&self, ask: &FullRunAsk) -> Result<(), String> {
        if !is_commit(&ask.at) || !(ask.since.is_empty() || is_commit(&ask.since)) {
            return Err("that isn't a commit".into());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let mut runs = crate::locking::lock(&self.runs, "full runs");
        if runs.contains_key(&ask.repo) {
            return Err("a full run of it is going already".into());
        }
        runs.insert(ask.repo.clone(), Run { stop: stop.clone(), end: None });
        drop(runs);
        let ask = ask.clone();
        std::thread::spawn(move || {
            let end = if ask.machine.is_empty() { here(&ask, &stop) } else { there(&ask, &stop) };
            runner().ended(&ask.repo, &stop, end);
        });
        Ok(())
    }

    fn going(&self, repo: &str) -> bool {
        let runs = crate::locking::lock(&self.runs, "full runs");
        runs.contains_key(repo)
    }

    fn take(&self, repo: &str) -> Option<FullRunEnd> {
        let mut runs = crate::locking::lock(&self.runs, "full runs");
        if runs.get(repo).is_some_and(|r| r.end.is_none()) {
            return None;
        }
        runs.remove(repo).and_then(|r| r.end)
    }

    fn stop(&self, repo: &str) {
        let mut runs = crate::locking::lock(&self.runs, "full runs");
        if let Some(r) = runs.remove(repo) {
            r.stop.store(true, Ordering::SeqCst);
        }
    }
}

/// A full commit name, and nothing a shell would read as more.
fn is_commit(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// " --since <commit>" when there's one.
fn since_arg(ask: &FullRunAsk) -> String {
    if ask.since.is_empty() { String::new() } else { format!(" --since {}", ask.since) }
}

/// Where this run's files are kept on this machine: Cue's own folder, never the project's.
fn work_dir() -> Result<std::path::PathBuf, String> {
    let dir = crate::server::cue_dir().join("full-run");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// `cmd` run in a process group of its own, its output in `out` and `err`, until it ends, `stop` is set or CAP has
/// passed (then the group is ended): its exit code.
fn run(mut cmd: std::process::Command, out: &std::path::Path, err: &std::path::Path, stop: &AtomicBool) -> Result<i32, String> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    let (o, e) = (std::fs::File::create(out).map_err(|e| e.to_string())?, std::fs::File::create(err).map_err(|e| e.to_string())?);
    let mut child = cmd.stdin(Stdio::null()).stdout(o).stderr(e).process_group(0).spawn().map_err(|e| e.to_string())?;
    let until = Instant::now() + CAP;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return Ok(s.code().unwrap_or(-1)),
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
        let late = Instant::now() >= until;
        if late || stop.load(Ordering::SeqCst) {
            unsafe { libc::kill(-(child.id() as i32), libc::SIGTERM) };
            let _ = child.wait();
            return Err(if late { "it took over three hours and was stopped" } else { "stopped" }.into());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// What a file says, for a reason shown: its last lines.
fn last_of(file: &std::path::Path) -> String {
    crate::safe_git::last_words(&std::fs::read_to_string(file).unwrap_or_default())
}

/// What your login shell runs for a run on this machine, in the project's main checkout: the script as it is at the
/// commit (never the folder's working file, which an item may be changing), its result in `result`.
fn here_line(ask: &FullRunAsk, result: &std::path::Path) -> String {
    format!("git show {at}:scripts/full-run.sh | bash -s -- --at {at}{} --out {}", since_arg(ask), crate::safe_git::sh_quote(&result.to_string_lossy()), at = ask.at)
}

/// The run on this machine, with your login shell (its PATH is yours). A result left by an earlier run stays: asked
/// for the same commits again (Cue quit under that run), the script answers with it instead of running twice.
fn here(ask: &FullRunAsk, stop: &AtomicBool) -> FullRunEnd {
    let dir = match work_dir() {
        Ok(d) => d,
        Err(e) => return FullRunEnd::NotMade(e),
    };
    let (result, log) = (dir.join("result.json"), dir.join("here.log"));
    let line = here_line(ask, &result);
    let mut cmd = std::process::Command::new(crate::desk::lands::login_shell());
    cmd.arg("-lc").arg(line).current_dir(&ask.repo);
    match run(cmd, &log, &dir.join("here.err"), stop) {
        // Another commit's result is an older run's: this one made none (the commit has no script).
        Ok(0 | 1) => match std::fs::read_to_string(&result).ok().and_then(|t| parse(&t)).filter(|r| r.at.is_empty() || r.at == ask.at) {
            Some(r) => FullRunEnd::Result(r),
            None => FullRunEnd::NotMade("it left no result".into()),
        },
        Ok(_) => FullRunEnd::NotMade(format!("{} {}", last_of(&log), last_of(&dir.join("here.err"))).trim().to_string()),
        Err(e) => FullRunEnd::NotMade(e),
    }
}

/// The address git pushes to for `folder` on `host`: over ssh, `~` your home there (`~` alone: the home itself),
/// with what an address reads as something else (a space, `#`, `%`, `?`) written as its code.
fn push_url(host: &str, folder: &str) -> String {
    let folder = folder.trim();
    let coded = |path: &str| {
        let mut out = String::new();
        for c in path.chars() {
            match c {
                ' ' => out.push_str("%20"),
                '#' => out.push_str("%23"),
                '%' => out.push_str("%25"),
                '?' => out.push_str("%3F"),
                c => out.push(c),
            }
        }
        out
    };
    if folder == "~" {
        return format!("ssh://{host}/~/");
    }
    match folder.strip_prefix("~/") {
        Some(rest) => format!("ssh://{host}/~/{}", coded(rest.trim_start_matches('/'))),
        None if folder.starts_with('/') => format!("ssh://{host}{}", coded(folder)),
        None => format!("ssh://{host}/~/{}", coded(folder)),
    }
}

/// What git is asked to send commit `at` to `url`: no hook of the project's runs for it (`--no-verify`).
fn push_args(url: &str, at: &str) -> Vec<String> {
    vec!["push".into(), "-q".into(), "--no-verify".into(), url.to_string(), format!("+{at}:{REF}")]
}

/// What runs on the machine: in the clone, the script as it is at the commit, then the mark with its exit code and
/// the result file (or, when no run was made, the end of what it said). Always ends 0: ssh's own failures stay apart.
fn script(ask: &FullRunAsk) -> String {
    format!(
        r#"d={d}
cd "$d" 2>/dev/null || {{ echo "{MARK}2"; echo "the project's folder isn't there"; exit 0; }}
log=$(mktemp)
git show {at}:scripts/full-run.sh | bash -s -- --at {at}{since} >"$log" 2>&1
code=$?
echo "{MARK}$code"
if [ "$code" -le 1 ]; then cat "$(tail -n 1 "$log")"; else tail -n 5 "$log"; fi
rm -f "$log"
exit 0"#,
        d = crate::machines::remote_dir(&ask.folder),
        at = ask.at,
        since = since_arg(ask),
    )
}

/// What the machine said, read: the result after the mark, or why there's none.
fn read(said: &str) -> FullRunEnd {
    let Some((_, after)) = said.split_once(MARK) else { return FullRunEnd::NotMade("it gave no answer".into()) };
    let (code, rest) = after.split_once('\n').unwrap_or((after, ""));
    match code.trim().parse::<i32>() {
        Ok(0 | 1) => parse(rest).map_or_else(|| FullRunEnd::NotMade("its result couldn't be read".into()), FullRunEnd::Result),
        _ => FullRunEnd::NotMade(crate::safe_git::last_words(rest)),
    }
}

/// The run on another machine: the commit pushed to its clone, the script run there, its result read back.
fn there(ask: &FullRunAsk, stop: &AtomicBool) -> FullRunEnd {
    let dir = match work_dir() {
        Ok(d) => d,
        Err(e) => return FullRunEnd::NotMade(e),
    };
    // The same ssh Cue's other trips use; a push that needs a password or a host's key fails instead of asking.
    let ssh = format!("{} -o BatchMode=yes", crate::safe_git::sh_quote(&crate::machines::ssh_program().to_string_lossy()));
    let env = [("GIT_TERMINAL_PROMPT", "0"), ("GIT_SSH_COMMAND", ssh.as_str())];
    let args = push_args(&push_url(&ask.host, &ask.folder), &ask.at);
    let sent = crate::desk::hub::landing::git::git_said("", &ask.repo, &env, &args.iter().map(String::as_str).collect::<Vec<_>>());
    if let Err(e) = sent {
        return FullRunEnd::NotMade(crate::desk::hub::landing::git::because("the commit couldn't be sent there".into(), &e));
    }
    let (out, err) = (dir.join("there.out"), dir.join("there.err"));
    match run(crate::machines::ssh_command(&ask.host, &script(ask)), &out, &err, stop) {
        Ok(0) => read(&std::fs::read_to_string(&out).unwrap_or_default()),
        Ok(_) => FullRunEnd::NotMade(last_of(&err)),
        Err(e) => FullRunEnd::NotMade(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask() -> FullRunAsk {
        FullRunAsk { repo: "/r".into(), at: "a".repeat(40), since: "b".repeat(40), machine: "box".into(), host: "box".into(), folder: "~/code/cue".into() }
    }

    #[test]
    fn the_commit_goes_to_the_clones_folder_over_ssh_with_tilde_as_home_there() {
        assert_eq!(push_url("box", "~/code/cue"), "ssh://box/~/code/cue");
        assert_eq!(push_url("me@box", "/srv/cue"), "ssh://me@box/srv/cue");
        assert_eq!(push_url("box", "code/cue"), "ssh://box/~/code/cue");
        assert_eq!(push_url("box", "~"), "ssh://box/~/", "the home itself, not a folder named ~ in it");
        assert_eq!(push_url("box", "~//code"), "ssh://box/~/code");
        assert_eq!(push_url("box", "~/my code/cue #2"), "ssh://box/~/my%20code/cue%20%232");
        assert_eq!(push_url("box", "/srv/a b/100%"), "ssh://box/srv/a%20b/100%25");
    }

    #[test]
    fn the_push_runs_no_hook_of_the_project() {
        let args = push_args("ssh://box/~/code/cue", "abc1234");
        assert_eq!(args, ["push", "-q", "--no-verify", "ssh://box/~/code/cue", "+abc1234:refs/cue/full-run"]);
    }

    #[test]
    fn this_machine_runs_the_script_as_it_is_at_the_commit_too() {
        let (at, since) = ("a".repeat(40), "b".repeat(40));
        let line = here_line(&ask(), std::path::Path::new("/c/full run/result.json"));
        assert_eq!(line, format!("git show {at}:scripts/full-run.sh | bash -s -- --at {at} --since {since} --out '/c/full run/result.json'"));
        assert!(!line.contains("bash scripts/full-run.sh"), "never the folder's working file: {line}");
    }

    #[test]
    fn the_machine_runs_the_script_as_it_is_at_the_commit_from_the_clone() {
        let s = script(&ask());
        let (at, since) = ("a".repeat(40), "b".repeat(40));
        assert!(s.starts_with("d=\"$HOME\"'/code/cue'\ncd \"$d\""), "{s}");
        assert!(s.contains(&format!("git show {at}:scripts/full-run.sh | bash -s -- --at {at} --since {since} >")), "{s}");
        let first = script(&FullRunAsk { since: String::new(), ..ask() });
        assert!(first.contains(&format!("bash -s -- --at {at} >")), "no --since on the first run: {first}");
    }

    #[test]
    fn what_the_machine_said_is_its_result_after_the_mark() {
        let said = "motd\ncue-full-run exit 1\n{\"passed\":false,\"failed\":[\"a::b\"],\"culprit\":null,\"culprit_note\":\"it failed before this batch\",\"tail\":\"x\"}\n";
        let FullRunEnd::Result(r) = read(said) else { panic!("not a result") };
        assert_eq!((r.passed, r.failed, r.culprit_note.as_deref()), (false, vec!["a::b".to_string()], Some("it failed before this batch")));
        assert_eq!(read("cue-full-run exit 2\nfull-run: no such commit\n"), FullRunEnd::NotMade("full-run: no such commit".into()));
        assert_eq!(read(""), FullRunEnd::NotMade("it gave no answer".into()));
    }

    #[test]
    fn only_a_commits_name_is_run() {
        assert!(is_commit(&"a".repeat(40)));
        assert!(!is_commit("main") && !is_commit("abc; rm -rf x") && !is_commit(""));
    }
}
