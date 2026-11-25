//! A commit made from a git index older than the branch never undoes the commits made since (#807). #796's holder
//! built its own index from HEAD (`GIT_INDEX_FILE=… git read-tree HEAD`), #791 landed a change to the same files,
//! and #796's `git commit` from that index put the older copies back over it (5cff09a, the exact reverse of 80e1f7c).
//! The shared index goes stale the same way when a commit is made with `commit-tree` and `update-ref`.
//!
//! So before a session's `git commit` or `git write-tree` runs (the agent's PreToolUse hook, hub/guard.rs), the index
//! it would use is read: a file there that's an older copy than HEAD's, one of its earlier versions, while the file in
//! the folder isn't that copy, means the index was built before a later commit, and the call is stopped, saying which
//! commit it would undo and how to bring the index up to date. A copy that matches the file in the folder (a revert
//! made on purpose) passes, as does a commit of named paths (`--only`, `-a`, `--`), which git makes from HEAD.

use crate::desk::hub::landing::git::git;

/// How far back a file's earlier copies are looked for.
const VERSIONS: &str = "-40";
/// At most this many changed files of an index are looked at.
const MAX_FILES: usize = 200;

/// One git call of a shell command: where it runs, its verb, its words after it, and the index it uses (None: the
/// repository's own; Some(None): one named by a `$VAR` this command doesn't set, so it can't be read).
#[derive(Debug, PartialEq)]
struct Call {
    dir: String,
    verb: String,
    args: Vec<String>,
    index: Option<Option<String>>,
}

/// A shell word's quotes taken off.
fn unquote(w: &str) -> String {
    w.trim_matches(|c| c == '"' || c == '\'').to_string()
}

/// `NAME=value` as (NAME, value), its value unquoted.
fn assignment(w: &str) -> Option<(String, String)> {
    let (name, value) = w.split_once('=')?;
    let ok = !name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    ok.then(|| (name.to_string(), unquote(value)))
}

/// `s` with `$NAME` and `${NAME}` of `vars` put in; None when a `$` is left (a name this command doesn't set).
fn expand(s: &str, vars: &[(String, String)]) -> Option<String> {
    let mut out = s.to_string();
    for (name, value) in vars.iter().rev() {
        out = out.replace(&format!("${{{name}}}"), value);
        let mut at = 0;
        while let Some(i) = out[at..].find(&format!("${name}")).map(|i| i + at) {
            let end = i + 1 + name.len();
            if out[end..].starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
                at = end;
                continue;
            }
            out.replace_range(i..end, value);
            at = i + value.len();
        }
    }
    (!out.contains('$')).then_some(out)
}

/// `p` as a path from `from`.
fn resolve(p: &str, from: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    match p {
        p if p.starts_with('/') => p.to_string(),
        "~" => home,
        p if p.starts_with("~/") => format!("{home}/{}", &p[2..]),
        p => format!("{}/{p}", from.trim_end_matches('/')),
    }
}

/// The git calls of shell `command` run from `cwd`, with the shell's own variables of it followed (`S=…`,
/// `export GIT_INDEX_FILE=$S/index`, `GIT_INDEX_FILE=… git commit`) and its `cd`s.
fn calls(command: &str, cwd: &str) -> Vec<Call> {
    let mut at = cwd.to_string();
    let mut vars: Vec<(String, String)> = Vec::new();
    let mut out = Vec::new();
    for part in command.split(['\n', ';', '|', '&', '(', ')']) {
        let mut words: Vec<&str> = part.split_whitespace().collect();
        if words.first() == Some(&"export") {
            words.remove(0);
            for w in words {
                if let Some((n, v)) = assignment(w) {
                    let v = expand(&v, &vars).unwrap_or_else(|| v.replace('$', "\u{0}"));
                    vars.push((n, v));
                }
            }
            continue;
        }
        let mut env: Vec<(String, String)> = Vec::new();
        while let Some((n, v)) = words.first().and_then(|w| assignment(w)) {
            let v = expand(&v, &vars).unwrap_or_else(|| v.replace('$', "\u{0}"));
            env.push((n, v));
            words.remove(0);
        }
        if words.is_empty() {
            vars.extend(env);
            continue;
        }
        if words[0] == "env" {
            words.remove(0);
        }
        match words.first().copied() {
            Some("cd") => {
                if let Some(d) = words.get(1).and_then(|d| expand(&unquote(d), &vars)) {
                    at = resolve(&d, &at);
                }
            }
            Some("git") => {
                let mut dir = at.clone();
                let mut i = 1;
                while i < words.len() && words[i].starts_with('-') {
                    if words[i] == "-C" {
                        if let Some(d) = words.get(i + 1).and_then(|d| expand(&unquote(d), &vars)) {
                            dir = resolve(&d, &at);
                        }
                        i += 1;
                    } else if words[i] == "-c" {
                        i += 1;
                    }
                    i += 1;
                }
                let Some(verb) = words.get(i) else { continue };
                let named = env.iter().rev().chain(vars.iter().rev()).find(|(n, _)| n == "GIT_INDEX_FILE").map(|(_, v)| v.clone());
                let index = named.map(|v| (!v.contains('\u{0}')).then(|| resolve(&v, &dir)));
                out.push(Call { dir, verb: verb.to_string(), args: words[i + 1..].iter().map(|w| unquote(w)).collect(), index });
            }
            _ => {}
        }
    }
    out
}

/// A file of an index that's older than HEAD's: its path, and the newest commit that changed it ("abc1234 subject").
#[derive(Debug, PartialEq)]
pub(crate) struct Stale {
    pub(crate) path: String,
    pub(crate) undoes: String,
}

/// The files of the index `index` (None: the repository's own) of the repository at `dir` that hold an older copy
/// than HEAD's, while the file in the folder isn't that copy.
pub(crate) fn stale_files(dir: &str, index: Option<&str>) -> Vec<Stale> {
    if index.is_some_and(|i| !std::path::Path::new(i).exists()) {
        return Vec::new();
    }
    let env: Vec<(&str, &str)> = index.map(|i| vec![("GIT_INDEX_FILE", i)]).unwrap_or_default();
    let Some(top) = git("", dir, &[], &["rev-parse", "--show-toplevel"]).map(|t| t.trim().to_string()) else { return Vec::new() };
    if git("", &top, &[], &["rev-parse", "-q", "--verify", "HEAD^{commit}"]).is_none() {
        return Vec::new();
    }
    let Some(changed) = git("", &top, &env, &["diff", "--cached", "--name-only", "--no-renames", "-z", "HEAD"]) else { return Vec::new() };
    let mut out = Vec::new();
    for path in changed.split('\0').filter(|p| !p.is_empty()).take(MAX_FILES) {
        let staged = git("", &top, &env, &["ls-files", "-s", "-z", "--", path]).and_then(|l| l.split_whitespace().nth(1).map(str::to_string));
        let head = git("", &top, &[], &["rev-parse", "-q", "--verify", &format!("HEAD:{path}")]).map(|h| h.trim().to_string());
        let file = std::path::Path::new(&top).join(path);
        let in_folder = if file.is_file() { git("", &top, &[], &["hash-object", "--", path]).map(|h| h.trim().to_string()) } else { None };
        let Some(log) = git("", &top, &[], &["log", VERSIONS, "--no-abbrev", "--raw", "--no-renames", "--format=@%h %s", "--", path]) else { continue };
        let newest = log.lines().find_map(|l| l.strip_prefix('@')).unwrap_or("").to_string();
        let earlier: Vec<&str> = log.lines().filter(|l| l.starts_with(':')).flat_map(|l| l.split_whitespace().skip(2).take(2)).filter(|b| b.chars().any(|c| c != '0')).collect();
        let older = match (&staged, &head) {
            // An earlier copy of the file, not HEAD's, and not what the folder has now.
            (Some(s), Some(h)) => s != h && earlier.contains(&s.as_str()) && in_folder.as_deref() != Some(s.as_str()),
            // HEAD has a file the index doesn't, and the folder still has it: the index is from before it was added.
            (None, Some(_)) => in_folder.is_some(),
            // HEAD took the file out, the folder too, and the index still has an earlier copy of it.
            (Some(s), None) => in_folder.is_none() && earlier.contains(&s.as_str()),
            (None, None) => false,
        };
        if older {
            out.push(Stale { path: path.to_string(), undoes: newest });
        }
    }
    out
}

/// Why shell `command`, run in `cwd`, is stopped: a `git commit` or `git write-tree` in it would make a commit from
/// an index older than HEAD, undoing what came after it. None: it passes.
pub(crate) fn stopped(command: &str, cwd: &str) -> Option<String> {
    if !command.contains("commit") && !command.contains("write-tree") {
        return None;
    }
    let all = calls(command, cwd);
    for (n, c) in all.iter().enumerate() {
        let makes = match c.verb.as_str() {
            "write-tree" => true,
            // Named paths (`--only` is git's default for them), or every tracked file as it is: made from HEAD.
            "commit" => !c.args.iter().any(|a| {
                let short = a.len() > 1 && a.starts_with('-') && !a.starts_with("--") && a[1..].chars().all(|ch| ch.is_ascii_alphabetic());
                matches!(a.as_str(), "--all" | "--only" | "--") || (short && (a[1..].contains('a') || a[1..].contains('o')))
            }),
            _ => false,
        };
        // Its index named by a variable this command doesn't set: it can't be read here.
        if !makes || matches!(c.index, Some(None)) {
            continue;
        }
        let index = c.index.clone().flatten();
        // Built again from HEAD earlier in this command: up to date by then.
        if all[..n].iter().any(|p| p.verb == "read-tree" && p.index.clone().flatten() == index) {
            continue;
        }
        let stale = stale_files(&c.dir, index.as_deref());
        let Some(first) = stale.first() else { continue };
        let more = match stale.len() {
            1 => String::new(),
            n => format!(" (and {} more file{})", n - 1, if n == 2 { "" } else { "s" }),
        };
        let paths: Vec<&str> = stale.iter().map(|s| s.path.as_str()).collect();
        let fix = match &index {
            Some(i) => format!("Build that index from HEAD again (GIT_INDEX_FILE={i} git read-tree HEAD), put your own files in it, then commit"),
            None => format!("Bring those files in the index up to HEAD first (git reset -q -- {}; the files in the folder stay as they are), then stage your own and commit", paths.join(" ")),
        };
        return Some(format!(
            "Cue stopped this: the git index \"git {}\" would use is older than HEAD. It has an earlier copy of {}{more}, so the commit would undo {}. {fix}.",
            c.verb, first.path, if first.undoes.is_empty() { "a later commit".to_string() } else { format!("commit {}", first.undoes) }
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commits_index_is_followed_through_the_shells_variables() {
        let c = calls("S=/tmp/x/796 && export GIT_INDEX_FILE=$S/index2 && git diff --cached | grep -i x ; git commit -q -m \"Named steps\"", "/r");
        let commit = c.iter().find(|c| c.verb == "commit").unwrap();
        assert_eq!((commit.dir.as_str(), commit.index.clone()), ("/r", Some(Some("/tmp/x/796/index2".into()))));
        let c = calls("cd /r/sub && GIT_INDEX_FILE=\"${HOME_X}/i\" git write-tree", "/home");
        assert_eq!(c[0].index, Some(None), "a variable this command doesn't set");
        let c = calls("GIT_INDEX_FILE=my.index git -C /r commit -m x; git commit -m y", "/home");
        assert_eq!(c[0].index, Some(Some("/r/my.index".into())), "relative to where git runs");
        assert_eq!(c[1].index, None, "a prefix counts for its own command only");
        assert_eq!(expand("$SA/$S", &[("S".into(), "1".into())]), None);
        assert_eq!(expand("${S}x/$S", &[("S".into(), "1".into())]).as_deref(), Some("1x/1"));
    }
}
