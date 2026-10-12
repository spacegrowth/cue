//! Git as Cue runs it, on this Mac or on one of your machines: never a program a repository sets up.
//! A repo's own config can make git start one (a file-system monitor, a pager, an external diff, a text
//! conversion, the filters its .gitattributes picks); every run here switches those off. Your own git
//! config stays as it is (Git LFS and the like keep working): only the repository's is distrusted.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

/// What every run starts with: no file-system monitor, no pager, no lock files.
const BASE: [&str; 5] = ["-c", "core.fsmonitor=false", "-c", "core.pager=cat", "--no-optional-locks"];
/// For a diff: no external diff program, no text conversion, no color.
pub const QUIET_DIFF: [&str; 3] = ["--no-color", "--no-ext-diff", "--no-textconv"];
/// The longest a run may take on a machine.
pub const LIMIT: Duration = Duration::from_secs(20);
/// The most output kept.
const OUT_LIMIT: usize = 4 << 20;

/// `s` as one word for a POSIX shell.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
/// Every word quoted, so nothing in them is read as more shell.
pub fn sh_words<S: AsRef<str>>(words: &[S]) -> String {
    words.iter().map(|w| sh_quote(w.as_ref())).collect::<Vec<_>>().join(" ")
}

/// git, found where an app started from Finder can find it (its PATH is short).
fn git_bin() -> PathBuf {
    let path = std::env::var("PATH").unwrap_or_default();
    let found = path
        .split(':')
        .chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"])
        .map(|d| std::path::Path::new(d).join("git"))
        .find(|p| p.is_file());
    found.unwrap_or_else(|| PathBuf::from("/usr/bin/git"))
}

/// The words of one run: git, the switches above, `extra` (from no_repo_filters), then `rest`, in `dir`.
pub fn words(dir: &str, extra: &[String], rest: &[&str]) -> Vec<String> {
    let mut w: Vec<String> = std::iter::once("git").chain(BASE).map(String::from).collect();
    w.extend(extra.iter().cloned());
    w.extend(["-C".to_string(), dir.to_string()]);
    w.extend(rest.iter().map(|s| s.to_string()));
    w
}

/// Run git in `dir`, here (`machine` empty) or on that machine: stdout and the exit code.
pub fn run(machine: &str, dir: &str, extra: &[String], rest: &[&str]) -> Result<(String, i32), String> {
    let w = words(dir, extra, rest);
    if machine.is_empty() {
        let out = Command::new(git_bin()).args(&w[1..]).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
        return Ok((String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(OUT_LIMIT)]).to_string(), out.status.code().unwrap_or(1)));
    }
    let script = format!("{}; printf '\\n@@cue-exit %s\\n' $?", sh_words(&w));
    let o = on_machine(machine, &script)?;
    Ok(match o.rfind("\n@@cue-exit ") {
        Some(i) => (o[..i].to_string(), o[i + 12..].trim().parse().unwrap_or(1)),
        None => (o, 1),
    })
}

/// A script on one of Cue's machines, at most LIMIT: what it printed.
pub fn on_machine(machine: &str, script: &str) -> Result<String, String> {
    let host = crate::machines::get(machine).ok_or_else(|| format!("Cue doesn't know a machine called {machine}"))?.host;
    let (tx, rx) = std::sync::mpsc::channel();
    let script = script.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(crate::machines::run(&host, &script));
    });
    rx.recv_timeout(LIMIT).unwrap_or_else(|_| Err("it took too long".into()))
}

/// The filters the repository's own config defines (`filter.<name>.clean`, `.smudge`, `.process`:
/// programs git runs on files as it reads them), each switched off: clean and smudge pass a file through
/// as it is, and no process runs. Reading config runs nothing.
pub fn no_repo_filters(machine: &str, dir: &str) -> Vec<String> {
    let read = |scope: &'static str| ["config", scope, "--name-only", "--get-regexp", "^filter\\."];
    let out = if machine.is_empty() {
        ["--local", "--worktree"].iter().filter_map(|s| run("", dir, &[], &read(s)).ok()).map(|(o, _)| o + "\n").collect::<String>()
    } else {
        let line = |scope| sh_words(&words(dir, &[], &read(scope)));
        on_machine(machine, &format!("{} 2>/dev/null; {} 2>/dev/null; true", line("--local"), line("--worktree"))).unwrap_or_default()
    };
    let mut names: Vec<String> = vec![];
    for key in out.lines() {
        if let Some((name, _)) = key.trim().strip_prefix("filter.").and_then(|k| k.rsplit_once('.')) {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
    }
    names.iter().flat_map(|n| ["-c".to_string(), format!("filter.{n}.clean=cat"), "-c".into(), format!("filter.{n}.smudge=cat"), "-c".into(), format!("filter.{n}.process=")]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_all_quoted() {
        assert_eq!(sh_words(&["git", "-C", "/a b", "it's"]), "'git' '-C' '/a b' 'it'\\''s'");
    }

    #[test]
    fn a_repositorys_own_programs_never_run_and_yours_stay() {
        let dir = std::env::temp_dir().join(format!("cue-safegit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.canonicalize().unwrap().display().to_string();
        let sh = |c: &str| assert!(Command::new("/bin/sh").arg("-c").arg(c).current_dir(&dir).status().unwrap().success(), "{c}");
        sh("git init -q && git config user.email t@t && git config user.name t && printf 'a\\n' > f.txt && git add f.txt && git commit -qm i");
        // Everything a repo's config can make git run while reading.
        sh("printf '*.txt filter=evil diff=evil\\n' > .gitattributes && git config filter.evil.clean 'touch ran-clean; cat' && git config filter.evil.smudge 'touch ran-smudge; cat' \
            && git config filter.evil.process 'touch ran-process; false' && git config diff.evil.textconv 'touch ran-textconv; cat' && git config diff.external 'touch ran-external' \
            && git config core.fsmonitor 'touch ran-fsmonitor; true' && git config core.pager 'touch ran-pager; cat' && printf 'a\\nb\\n' > f.txt");
        let f = no_repo_filters("", &d);
        assert!(f.iter().any(|a| a == "filter.evil.process="), "{f:?}");
        let (out, _) = run("", &d, &f, &[&["diff", "HEAD"][..], &QUIET_DIFF[..]].concat()).unwrap();
        assert!(out.contains("+b"), "{out}");
        run("", &d, &f, &["stash", "create"]).unwrap();
        run("", &d, &f, &["ls-files", "--others", "--exclude-standard"]).unwrap();
        run("", &d, &f, &[&["diff", "--numstat", "HEAD"][..], &QUIET_DIFF[..]].concat()).unwrap();
        let ran: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.starts_with("ran-")).collect();
        assert!(ran.is_empty(), "these ran: {ran:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
