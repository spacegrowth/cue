//! Sessions stay in their own copy (#483). An item's holder was told to work only in the copy Cue gave it, and still
//! edited the main checkout (#456's merge then stopped on those edits), or made a worktree and a branch of its own
//! (#455). Being told wasn't enough, so the agent's PreToolUse hook (hook.rs `guard`) asks Cue before each edit and
//! each git command, and Cue stops one that leaves the copy, saying where to work instead:
//! - an edit (Edit, Write, MultiEdit, NotebookEdit) in the item's repository outside its copy: the main checkout,
//!   another item's copy, or a worktree it made itself;
//! - git's writing commands (commit, merge, checkout, add, …) run in such a folder (its `cd`, `git -C`, or the
//!   session's own folder);
//! - a worktree or a branch of its own, whether or not Cue gave it a copy.
//!
//! Only a session holding an item at work here on this Mac; everything else passes as before. A file changed from the
//! shell (a redirect, `sed -i`) isn't read: its edits are named at the landing instead (desk/hub/lead/stop_lead.rs).

use crate::desk::hub::landing::main_line::{main_checkout, same_dir};
use crate::desk::hub::prelude::*;
use crate::desk::ItemTree;

/// git verbs that change a checkout's files or its branch.
const WRITES: &[&str] = &["commit", "merge", "rebase", "cherry-pick", "revert", "reset", "checkout", "switch", "restore", "add", "rm", "mv", "stash", "apply", "am", "pull"];

/// The file an edit tool changes.
fn edited_path(tool: &str, input: &Value) -> Option<String> {
    let key = match tool {
        "Edit" | "Write" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        _ => return None,
    };
    input[key].as_str().filter(|p| !p.is_empty()).map(str::to_string)
}

/// `p` with its nearest folder that exists read through links (a file not made yet keeps its name).
fn canon(p: &str) -> String {
    let mut at = std::path::PathBuf::from(p);
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(c) = std::fs::canonicalize(&at) {
            let mut c = c;
            for r in rest.iter().rev() {
                c.push(r);
            }
            return c.to_string_lossy().to_string();
        }
        match (at.file_name().map(|n| n.to_os_string()), at.parent().map(|p| p.to_path_buf())) {
            (Some(n), Some(parent)) => {
                rest.push(n);
                at = parent;
            }
            _ => return p.trim_end_matches('/').to_string(),
        }
    }
}

/// Whether `path` is `dir` or in it.
fn inside(path: &str, dir: &str) -> bool {
    let (p, d) = (canon(path), canon(dir));
    !d.is_empty() && (p == d || p.starts_with(&format!("{}/", d.trim_end_matches('/'))))
}

/// The nearest folder of `path` that exists.
fn existing(path: &str) -> Option<String> {
    std::path::Path::new(path).ancestors().find(|a| a.is_dir()).map(|a| a.to_string_lossy().to_string())
}

/// Whether `path` is in repository `repo` (its main checkout, or any worktree of it) but in none of `copies`.
fn outside(path: &str, repo: &str, copies: &[ItemTree]) -> bool {
    if copies.iter().any(|t| inside(path, &t.path)) {
        return false;
    }
    if inside(path, repo) {
        return true;
    }
    existing(path).and_then(|d| main_checkout(&d)).is_some_and(|(m, _)| same_dir(&m, repo))
}

/// One git command: its folder (`-C`), its verb and its words after it.
struct GitCall {
    dir: Option<String>,
    verb: String,
    args: Vec<String>,
}

/// The git commands in shell `command` run from `cwd`, each with the folder it runs in (after any `cd`).
fn git_calls(command: &str, cwd: &str) -> Vec<(String, GitCall)> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut at = cwd.to_string();
    let mut out = Vec::new();
    for part in command.split(['\n', ';', '|', '&', '(', ')']) {
        let words: Vec<String> = part.split_whitespace().map(|w| w.trim_matches(|c| c == '"' || c == '\'').to_string()).collect();
        let words: Vec<String> = words.into_iter().skip_while(|w| w.contains('=') && !w.starts_with('-')).collect();
        let resolve = |d: &str, from: &str| match d {
            d if d.starts_with('/') => d.to_string(),
            "~" => home.clone(),
            d if d.starts_with("~/") => format!("{home}/{}", &d[2..]),
            d => format!("{}/{d}", from.trim_end_matches('/')),
        };
        match words.first().map(String::as_str) {
            Some("cd") => {
                if let Some(d) = words.get(1) {
                    at = resolve(d, &at);
                }
            }
            Some("git") => {
                let mut dir = None;
                let mut i = 1;
                while i < words.len() && words[i].starts_with('-') {
                    if words[i] == "-C" {
                        dir = words.get(i + 1).map(|d| resolve(d, &at));
                        i += 1;
                    } else if words[i] == "-c" {
                        i += 1;
                    }
                    i += 1;
                }
                let Some(verb) = words.get(i).cloned() else { continue };
                out.push((at.clone(), GitCall { dir, verb, args: words[i + 1..].to_vec() }));
            }
            _ => {}
        }
    }
    out
}

/// Whether a git command makes a worktree or a branch.
fn makes_own(g: &GitCall) -> Option<&'static str> {
    let has = |f: &[&str]| g.args.iter().any(|a| f.contains(&a.as_str()));
    match g.verb.as_str() {
        "worktree" if g.args.first().is_some_and(|a| a == "add") => Some("a worktree of your own"),
        "checkout" if has(&["-b", "-B", "--orphan"]) => Some("a branch of your own"),
        "switch" if has(&["-c", "-C", "--create", "--force-create", "--orphan"]) => Some("a branch of your own"),
        "branch" if !g.args.is_empty() && g.args.iter().all(|a| !a.starts_with('-')) => Some("a branch of your own"),
        _ => None,
    }
}

/// Why `tool` (with `input`, run in `cwd`) leaves the copies `copies` of `repo` (none: Cue gave it none), if it does.
fn leaves(tool: &str, input: &Value, cwd: &str, repo: &str, copies: &[ItemTree]) -> Option<String> {
    if let Some(p) = edited_path(tool, input) {
        return (!copies.is_empty() && outside(&p, repo, copies)).then(|| format!("{p} is outside it"));
    }
    let command = input["command"].as_str()?;
    for (at, g) in git_calls(command, cwd) {
        if let Some(what) = makes_own(&g) {
            return Some(format!("\"git {} {}\" makes {what}", g.verb, g.args.join(" ")).replace("  ", " "));
        }
        let dir = g.dir.clone().unwrap_or(at);
        if !copies.is_empty() && WRITES.contains(&g.verb.as_str()) && outside(&dir, repo, copies) {
            return Some(format!("\"git {}\" would run in {dir}, outside it", g.verb));
        }
    }
    None
}

impl Hub {
    /// Why session `origin`'s tool call is stopped for leaving its item's copy (said to it), or None: it passes.
    pub(crate) fn copy_guard(&self, origin: &Origin, tool: &str, input: &Value) -> Option<String> {
        if !origin.machine.is_empty() || !matches!(tool, "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "Bash") {
            return None;
        }
        let sid = origin.session_id.as_str();
        let held: Vec<(String, Option<ItemTree>)> = self.read(|st| {
            st.desk
                .items
                .iter()
                .filter(|i| i.state == "working" && i.holder.as_ref().is_some_and(|h| h.sid == sid || crate::desk::hub::core::desk::crew_parts(h).is_some_and(|(lead, _)| lead == sid)))
                .map(|i| (i.id.clone(), i.tree.clone()))
                .collect()
        });
        let (id, tree) = held.iter().find(|(_, t)| t.is_some()).or(held.first())?.clone();
        let copies: Vec<ItemTree> = held.iter().filter_map(|(_, t)| t.clone()).collect();
        let repo = copies.first().map(|t| t.repo.clone()).unwrap_or_default();
        let what = leaves(tool, input, &origin.cwd, &repo, &copies)?;
        let place = match tree {
            Some(t) => format!("Desk item #{id} works only in its own copy, {} (branch {})", t.path, t.branch),
            None => format!("Desk item #{id} works where you are, on the branch that's there"),
        };
        Some(format!("Cue stopped this: {place}, and {what}. Change files only there, with no worktree or branch of your own; at your DONE Cue lands it on main."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_commands_are_read_with_their_folders() {
        let calls = git_calls("cd /r && git -C sub commit -m 'x'; git worktree add ../mine -b mine", "/home");
        assert_eq!(calls.len(), 2);
        assert_eq!((calls[0].0.as_str(), calls[0].1.dir.as_deref(), calls[0].1.verb.as_str()), ("/r", Some("/r/sub"), "commit"));
        assert_eq!(makes_own(&calls[1].1), Some("a worktree of your own"));
        let reads = git_calls("git status && git branch --show-current && git log -1", "/r");
        assert!(reads.iter().all(|(_, g)| makes_own(g).is_none() && !WRITES.contains(&g.verb.as_str())));
        assert_eq!(makes_own(&git_calls("git switch -c fix", "/r")[0].1), Some("a branch of your own"));
        assert_eq!(makes_own(&git_calls("git branch fix-grip", "/r")[0].1), Some("a branch of your own"));
    }
}
