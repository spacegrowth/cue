//! The check before every push Cue makes (main from this Mac after a landing, an item's branch for its pull request,
//! an item's branch from another machine): each commit the remote doesn't have yet (not reachable from any of its
//! remote-tracking refs, so every commit when it has none) is read whole, its message, who made it and every line it
//! adds, for the project's words never committed (config `project.never`) and your own names (config `owner.names`).
//! Anything found refuses the push, naming each find: its commit, and the file and line, or its message or author.

use crate::desk::hub::landing::git::git_said;

/// One find: the commit (short), where in it ("commit message", "commit author", "path line 12", "the name path"),
/// and the word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hit {
    pub commit: String,
    pub place: String,
    pub word: String,
}

/// At most this many finds named in the refusal; the rest counted.
const SHOWN: usize = 8;

/// What no push of `project` may carry: its words never committed, your own names, and the words Cue finds by itself
/// (found_words.rs: your accounts' names, and your private projects' names when `project` is public), any case.
pub(crate) fn words_for(project: &str) -> Vec<String> {
    let mut words = crate::config::never_words(project);
    for n in crate::config::owner_names().into_iter().chain(crate::desk::hub::landing::found_words::words_for(project)) {
        if !words.iter().any(|w| w.eq_ignore_ascii_case(&n)) {
            words.push(n);
        }
    }
    words.retain(|w| !w.trim().is_empty());
    words
}

/// Whether pushing `src` (a branch, a ref or a commit) from `dir` on `machine` ("" here) to `remote` may go: Ok, or
/// why not, each find named. Commits Cue can't read refuse it too: nothing goes out unchecked.
pub(crate) fn push_allowed(machine: &str, dir: &str, project: &str, remote: &str, src: &str) -> Result<(), String> {
    let words = words_for(project);
    if words.is_empty() {
        return Ok(());
    }
    let hits = hits(machine, dir, remote, src, &words).map_err(|e| crate::desk::hub::landing::git::because("Not pushed: Cue couldn't read the commits to check them".into(), &e))?;
    if hits.is_empty() { Ok(()) } else { Err(refusal(&hits)) }
}

/// The finds in `src`'s commits that `remote` doesn't have yet.
pub(crate) fn hits(machine: &str, dir: &str, remote: &str, src: &str, words: &[String]) -> Result<Vec<Hit>, String> {
    let not = format!("--remotes={remote}");
    // One record per commit: NUL, the commit, who wrote and committed it, its message, then its diff (merges: only
    // the lines that differ from every parent, what the merge itself wrote).
    let log = git_said(
        machine,
        dir,
        &[],
        &["log", "-p", "--cc", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", "--format=%x00%H%x01%an <%ae>%n%cn <%ce>%x01%B%x01", src, "--not", &not, "--"],
    )?;
    Ok(hits_in(&log, words))
}

/// The finds in `log` (as `hits` asks git for it), commit by commit, in the order git gave them.
pub(crate) fn hits_in(log: &str, words: &[String]) -> Vec<Hit> {
    let words: Vec<(String, String)> = words.iter().filter(|w| !w.trim().is_empty()).map(|w| (w.clone(), w.to_lowercase())).collect();
    let find = |text: &str| -> Vec<String> {
        let low = text.to_lowercase();
        words.iter().filter(|(_, l)| low.contains(l.as_str())).map(|(w, _)| w.clone()).collect()
    };
    let mut out = vec![];
    for rec in log.split('\0').filter(|r| !r.trim().is_empty()) {
        let mut parts = rec.splitn(4, '\u{1}');
        let (Some(sha), Some(who), Some(message)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let diff = parts.next().unwrap_or("");
        let commit: String = sha.trim().chars().take(7).collect();
        let mut add = |place: String, word: String| out.push(Hit { commit: commit.clone(), place, word });
        for w in find(message) {
            add("commit message".into(), w);
        }
        for w in find(who) {
            add("commit author".into(), w);
        }
        for (place, text) in added_lines(diff) {
            for w in find(&text) {
                add(place.clone(), w);
            }
        }
    }
    out
}

/// Each added line in `diff` (plain or combined, -U0) with where it is ("path line N"), and each new file's name
/// ("the name path").
fn added_lines(diff: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let (mut file, mut fresh, mut in_hunk) = (String::new(), false, false);
    // Parents of a combined diff's hunk (one for a plain diff), and the next line's number in the new file.
    let (mut cols, mut line) = (1usize, 0u64);
    for l in diff.lines() {
        if l.starts_with("diff --git ") || l.starts_with("diff --cc ") || l.starts_with("diff --combined ") {
            (in_hunk, fresh) = (false, false);
            continue;
        }
        if !in_hunk {
            if l.starts_with("--- ") {
                fresh = l == "--- /dev/null";
            } else if let Some(path) = l.strip_prefix("+++ ") {
                file = path.strip_prefix("b/").unwrap_or(path).to_string();
                if fresh && path != "/dev/null" {
                    out.push((format!("the name {file}"), file.clone()));
                }
            } else if l.starts_with("@@") {
                (in_hunk, cols, line) = hunk(l);
            }
            continue;
        }
        if l.starts_with("@@") {
            (in_hunk, cols, line) = hunk(l);
            continue;
        }
        if l.starts_with('\\') {
            continue;
        }
        let (mark, text) = l.split_at(cols.min(l.len()));
        if mark.contains('-') {
            continue;
        }
        if mark.contains('+') {
            out.push((format!("{file} line {line}"), text.to_string()));
        }
        line += 1;
    }
    out
}

/// A hunk header ("@@ -1,2 +3,4 @@", "@@@ -1 -2 +3,4 @@@"): in a hunk, its parents, and its first line in the new
/// file.
fn hunk(header: &str) -> (bool, usize, u64) {
    let ats = header.chars().take_while(|c| *c == '@').count();
    let start = header.split_whitespace().find_map(|w| w.strip_prefix('+')).and_then(|r| r.split(',').next()).and_then(|n| n.parse().ok()).unwrap_or(1);
    (true, ats.saturating_sub(1).max(1), start)
}

/// The refusal, each find named: `Not pushed: … "word" in abc1234: commit message; "word" in abc1234: src/a.rs line 3`.
pub(crate) fn refusal(hits: &[Hit]) -> String {
    let named: Vec<String> = hits.iter().take(SHOWN).map(|h| format!("\"{}\" in {}: {}", h.word, h.commit, h.place)).collect();
    let more = if hits.len() > SHOWN { format!("; and {} more", hits.len() - SHOWN) } else { String::new() };
    format!("Not pushed: words that must not go public are in commits the remote doesn't have yet: {}{more}", named.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git").args(args).current_dir(dir).env("GIT_CONFIG_NOSYSTEM", "1").output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A repo with a local bare repo as its "remote" (nothing leaves the machine), one commit pushed there.
    fn repo(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("cue-push-check-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (dir, bare) = (root.join("work"), root.join("remote.git"));
        std::fs::create_dir_all(&dir).unwrap();
        git(&root, &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.name", "Tester"]);
        git(&dir, &["config", "user.email", "tester@example.com"]);
        git(&dir, &["remote", "add", "origin", bare.to_str().unwrap()]);
        commit(&dir, "README", "hello\n", "start");
        git(&dir, &["push", "-q", "-u", "origin", "main"]);
        (dir, root)
    }

    fn commit(dir: &Path, file: &str, text: &str, message: &str) -> String {
        std::fs::write(dir.join(file), text).unwrap();
        git(dir, &["add", file]);
        git(dir, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", message]);
        git(dir, &["rev-parse", "--short=7", "HEAD"])
    }

    fn words() -> Vec<String> {
        vec!["Zorblax".into(), "quuxcorp".into()]
    }

    #[test]
    fn a_clean_push_passes() {
        let (dir, root) = repo("clean");
        commit(&dir, "a.txt", "plain words\n", "Plain change");
        let d = dir.to_str().unwrap();
        assert_eq!(hits("", d, "origin", "main", &words()).unwrap(), vec![]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_word_in_a_commit_message_is_refused_and_names_the_commit() {
        let (dir, root) = repo("message");
        let sha = commit(&dir, "a.txt", "plain\n", "Tell ZORBLAX about it");
        let found = hits("", dir.to_str().unwrap(), "origin", "main", &words()).unwrap();
        assert_eq!(found, vec![Hit { commit: sha.clone(), place: "commit message".into(), word: "Zorblax".into() }]);
        let said = refusal(&found);
        assert!(said.starts_with("Not pushed") && said.contains(&sha) && said.contains("commit message"), "{said}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_word_in_an_added_line_is_refused_and_names_commit_file_and_line() {
        let (dir, root) = repo("line");
        let first = commit(&dir, "src.txt", "one\ntwo\n", "First");
        let second = commit(&dir, "src.txt", "one\ntwo\nthree\nsee quuxcorp here\n", "Second");
        let found = hits("", dir.to_str().unwrap(), "origin", "main", &words()).unwrap();
        assert_eq!(found, vec![Hit { commit: second.clone(), place: "src.txt line 4".into(), word: "quuxcorp".into() }], "only the line that adds it, in {first}..{second}");
        let said = refusal(&found);
        assert!(said.contains(&second) && said.contains("src.txt line 4") && said.contains("quuxcorp"), "{said}");
        // Once the remote has it, it's not checked again: only commits it doesn't have yet.
        git(&dir, &["push", "-q", "origin", "main"]);
        assert_eq!(hits("", dir.to_str().unwrap(), "origin", "main", &words()).unwrap(), vec![]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_remote_with_no_copy_has_every_commit_checked() {
        let (dir, root) = repo("fresh");
        git(&dir, &["remote", "add", "other", root.join("none.git").to_str().unwrap()]);
        let found = hits("", dir.to_str().unwrap(), "other", "main", &["hello".into()]).unwrap();
        assert_eq!(found.iter().map(|h| h.place.as_str()).collect::<Vec<_>>(), ["README line 1"], "the first commit, never pushed there: {found:?}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn authors_new_file_names_and_merges_are_read_too() {
        let log = "\0abcdef1234\u{1}Zorblax Q <z@example.com>\nTester <t@example.com>\u{1}Plain\n\u{1}\ndiff --git a/notes/quuxcorp.md b/notes/quuxcorp.md\nnew file mode 100644\n--- /dev/null\n+++ b/notes/quuxcorp.md\n@@ -0,0 +1 @@\n+fine\n\0fedcba98\u{1}T <t@example.com>\nT <t@example.com>\u{1}Merge\n\u{1}\ndiff --cc x.rs\n--- a/x.rs\n+++ b/x.rs\n@@@ -5,1 -5,1 +5,2 @@@\n- old\n -older\n++zorblax resolved\n+ kept\n";
        let found = hits_in(log, &words());
        let places: Vec<(&str, &str)> = found.iter().map(|h| (h.commit.as_str(), h.place.as_str())).collect();
        assert_eq!(places, [("abcdef1", "commit author"), ("abcdef1", "the name notes/quuxcorp.md"), ("fedcba9", "x.rs line 5")], "{found:?}");
    }
}
