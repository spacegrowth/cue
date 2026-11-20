//! The main line, for a finishing item (split from desk_finish.rs): which branch is main and where its checkout is,
//! a crew's branch merged into it, and later commits on it that take a finished item's work back out (#213).

use super::desk_git::{git, git_said, word_in};
use super::*;
use crate::desk::DeskItem;

/// The branch that is main in `top`'s repository: "main", else "master", else the branch its main checkout is on.
/// None when there's none of those (a detached main checkout in a repository with neither).
pub(super) fn main_ref(top: &str) -> Option<String> {
    for b in ["main", "master"] {
        if git("", top, &[], &["rev-parse", "--verify", "-q", &format!("refs/heads/{b}")]).is_some() {
            return Some(b.to_string());
        }
    }
    main_checkout(top).map(|(_, b)| b).filter(|b| !b.is_empty())
}

/// `folder`'s repository's main checkout (its first worktree) and the branch it's on ("" for a detached HEAD).
pub(super) fn main_checkout(folder: &str) -> Option<(String, String)> {
    let list = git("", folder, &[], &["worktree", "list", "--porcelain"])?;
    let mut lines = list.lines();
    let top = lines.next()?.strip_prefix("worktree ")?.to_string();
    let branch = lines.take_while(|l| !l.is_empty()).find_map(|l| l.strip_prefix("branch refs/heads/")).unwrap_or("").to_string();
    Some((top, branch))
}

/// `folder`'s work merged into its repository's main checkout, when it's another worktree on a branch with commits
/// the main checkout's branch doesn't have: tried first without touching any file (`git merge-tree`), its added
/// lines word-checked, then merged there (`--no-ff`). The main checkout, when merged; None when there was nothing
/// to merge; else the step and question to stop on ("clash": its work clashes with the main line, back to its
/// session; a word, or git refusing: nothing merged).
pub(super) fn merge_to_main(project: &str, folder: &str, title: &str) -> Result<Option<String>, (&'static str, String)> {
    let Some(top) = git("", folder, &[], &["rev-parse", "--show-toplevel"]).map(|t| t.trim().to_string()) else { return Ok(None) };
    let Some((main, onto)) = main_checkout(folder) else { return Ok(None) };
    if same_dir(&top, &main) || onto.is_empty() {
        return Ok(None);
    }
    let Some(sha) = git("", &top, &[], &["rev-parse", "HEAD"]).map(|s| s.trim().to_string()) else { return Ok(None) };
    let branch = git("", &top, &[], &["symbolic-ref", "--short", "-q", "HEAD"]).map(|b| b.trim().to_string()).filter(|b| !b.is_empty() && *b != onto).unwrap_or_else(|| sha[..sha.len().min(8)].to_string());
    merge_into(project, &main, &onto, &sha, &branch, &format!("in {top}"), title)
}

/// One merge an item's landing makes: a worktree's work, or a branch no worktree has (#481).
pub(super) enum Merge {
    Folder(String),
    Ref { main: String, branch: String, sha: String },
}

/// Branch `branch` (at `sha`) merged into the branch of `main` (a main checkout), as `merge_to_main` merges a
/// worktree's.
pub(super) fn merge_ref(project: &str, main: &str, branch: &str, sha: &str, title: &str) -> Result<Option<String>, (&'static str, String)> {
    let Some((main, onto)) = main_checkout(main) else { return Ok(None) };
    if onto.is_empty() || onto == branch {
        return Ok(None);
    }
    merge_into(project, &main, &onto, sha, branch, &format!("in a worktree of {main} on {branch}"), title)
}

/// `sha` (from `branch`, settled `where_`) merged into `onto`, checked out in the main checkout `main`.
fn merge_into(project: &str, main: &str, onto: &str, sha: &str, branch: &str, where_: &str, title: &str) -> Result<Option<String>, (&'static str, String)> {
    let (main, onto) = (main.to_string(), onto.to_string());
    let ahead = git("", &main, &[], &["rev-list", "--count", &format!("{onto}..{sha}")]).and_then(|n| n.trim().parse::<u64>().ok()).unwrap_or(0);
    if ahead == 0 {
        return Ok(None);
    }
    let filters = crate::safe_git::no_repo_filters("", &main);
    let tried = crate::safe_git::run_env("", &main, &filters, &[], &["merge-tree", "--write-tree", "--name-only", &onto, &sha]);
    let (out, code) = tried.map_err(|e| ("conflict", format!("Not merged: git couldn't try the merge of {branch} into {onto}: {e}")))?;
    if code != 0 {
        let files: Vec<&str> = out.lines().skip(1).take_while(|l| !l.is_empty()).collect();
        return Err(("clash", format!("Its work clashes with {onto} in {}: {where_}, merge {onto} into {branch}, settle the clash there, and say DONE again", files.join(", "))));
    }
    let tree = out.lines().next().unwrap_or("").trim().to_string();
    let diff = git("", &main, &[], &["diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", &onto, &tree]).ok_or(("conflict", format!("Not merged: git couldn't read the merge of {branch}")))?;
    let words = crate::config::never_words(project);
    if let Some((w, at)) = word_in(&diff, &words) {
        return Err(("word", format!("Not merged: \"{w}\" is in {at}")));
    }
    let message = format!("Merge {branch}: {title}");
    if let Some(w) = words.iter().find(|w| message.to_lowercase().contains(&w.to_lowercase())) {
        return Err(("word", format!("Not merged: \"{w}\" is in the merge's message")));
    }
    if let Some(what) = in_progress(&main) {
        return Err(("conflict", format!("Not merged: the main checkout is in the middle of {what}; finish it there first")));
    }
    if let Err(why) = git_said("", &main, &[], &["merge", "--no-ff", "--no-edit", "-q", "-m", &message, &sha]) {
        // Refused before it began (a file there with changes of its own, a hook), or stopped: left as it was. Only
        // this merge is undone (none was in progress before it). git's own words say which.
        if in_progress(&main).is_some() {
            git("", &main, &[], &["merge", "--abort"]);
        }
        let way = super::desk_stop_lead::in_the_way(&main, &onto, &tree);
        let what = if way.is_empty() { format!("Not merged: git wouldn't merge {branch} into {onto} in the main checkout") } else { super::desk_stop_lead::edits_line(&way, &[]) };
        return Err(("conflict", super::desk_stop_lead::because_plain(what, &why)));
    }
    Ok(Some(main))
}

/// Lines worth matching in a diff: with a letter or digit, four characters or more once trimmed (not a lone brace).
fn counts(line: &str) -> bool {
    let t = line.trim();
    t.chars().count() >= 4 && t.chars().any(char::is_alphanumeric)
}

/// Commit `sha`'s added ('+') or removed ('-') lines, by file (its path after the change), trimmed.
fn lines_of(top: &str, sha: &str, sign: char) -> Vec<(String, String)> {
    let diff = git("", top, &[], &["show", "-U0", "--format=", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", sha]).unwrap_or_default();
    let mut file = String::new();
    let mut out: Vec<(String, String)> = Vec::new();
    for l in diff.lines() {
        if let Some(rest) = l.strip_prefix("diff --git ") {
            file = rest.rsplit_once(" b/").map(|(_, b)| b.to_string()).unwrap_or_default();
        } else if l.starts_with("+++ ") || l.starts_with("--- ") {
            continue;
        } else if let Some(body) = l.strip_prefix(sign) {
            if counts(body) {
                out.push((file.clone(), body.trim().to_string()));
            }
        }
    }
    out
}

/// The first of `new` (commits on `top`'s main line, oldest first; `head` the newest) that takes item `it`'s work
/// back out: made after it, not its own, removing at least half its added lines, which `head` no longer has.
/// "<short> (<subject>)", when one does.
fn taken_out_by(top: &str, head: &str, it: &DeskItem, new: &[&str]) -> Option<String> {
    let shas: Vec<&str> = it.commit_in.iter().map(|(_, s)| s.as_str()).collect();
    let is_ancestor = |a: &str, b: &str| matches!(crate::safe_git::run("", top, &[], &["merge-base", "--is-ancestor", a, b]), Ok((_, 0)));
    // Its commits on this main line (a commit taken back by Undo, or in another repository, isn't).
    let on: Vec<&str> = shas.iter().copied().filter(|s| is_ancestor(s, head)).collect();
    if on.is_empty() {
        return None;
    }
    let added: Vec<(String, String)> = on.iter().flat_map(|s| lines_of(top, s, '+')).collect();
    if added.is_empty() {
        return None;
    }
    for c in new {
        if shas.iter().any(|s| c.starts_with(s)) || !on.iter().all(|s| is_ancestor(s, c)) {
            continue;
        }
        let removed = lines_of(top, c, '-');
        if removed.is_empty() {
            continue;
        }
        let mut now_in: Vec<(String, Vec<String>)> = Vec::new();
        let mut gone = 0;
        for (file, line) in &added {
            if !removed.iter().any(|(f, l)| f == file && l == line) {
                continue;
            }
            if !now_in.iter().any(|(f, _)| f == file) {
                let text = git("", top, &[], &["show", &format!("{head}:{file}")]).unwrap_or_default();
                now_in.push((file.clone(), text.lines().map(|l| l.trim().to_string()).collect()));
            }
            if now_in.iter().find(|(f, _)| f == file).is_some_and(|(_, ls)| !ls.contains(line)) {
                gone += 1;
            }
        }
        if gone > 0 && gone * 2 >= added.len() {
            let short = git("", top, &[], &["log", "-1", "--format=%h (%s)", c]).map(|l| l.trim().to_string()).unwrap_or_else(|| c[..c.len().min(8)].to_string());
            return Some(short);
        }
    }
    None
}

/// What `main`'s checkout is in the middle of, when it is: a merge, a rebase, a cherry-pick or a revert.
pub(super) fn in_progress(main: &str) -> Option<&'static str> {
    let path = |p: &str| -> Option<std::path::PathBuf> {
        let at = git("", main, &[], &["rev-parse", "--git-path", p])?.trim().to_string();
        let at = std::path::PathBuf::from(at);
        Some(if at.is_absolute() { at } else { std::path::Path::new(main).join(at) })
    };
    let marks = [("MERGE_HEAD", "a merge"), ("rebase-merge", "a rebase"), ("rebase-apply", "a rebase"), ("CHERRY_PICK_HEAD", "a cherry-pick"), ("REVERT_HEAD", "a revert")];
    marks.into_iter().find(|(p, _)| path(p).is_some_and(|f| f.exists())).map(|(_, what)| what)
}

/// Whether two folders are the same, once their links are followed.
pub(super) fn same_dir(a: &str, b: &str) -> bool {
    let canon = |p: &str| std::fs::canonicalize(p).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| p.trim_end_matches('/').to_string());
    canon(a) == canon(b)
}

impl Hub {
    /// Commits new on a main line since Cue last looked, each checked against the finished items whose commits are
    /// on it (to check, finishing, or done this week): one that takes most of an item's added lines back out (a
    /// revert, a file put back as it was) sends the item back to its holder, saying which commit did (#213).
    pub(super) fn taken_out_tick(&self) {
        let now = now_ms();
        let week_ago = super::desk::secs(now).saturating_sub(7 * 24 * 3600);
        // Only items finished here at a DONE (`finish`): their commits are in this Mac's folders.
        let items: Vec<DeskItem> = self.read(|st| {
            let open = |i: &DeskItem| matches!(i.state.as_str(), "to_check" | "finishing") || (i.state == "done" && i.done_at.is_some_and(|d| d >= week_ago));
            st.desk.items.iter().filter(|i| i.finish.is_some() && !i.commit_in.is_empty() && open(i)).cloned().collect()
        });
        if items.is_empty() {
            return;
        }
        // Each item's main checkouts, and each main checkout's HEAD now.
        let mut tops: Vec<(String, String)> = Vec::new();
        let mut of_folder: Vec<(String, Option<String>)> = Vec::new();
        let mut of_item: Vec<(&DeskItem, Vec<String>)> = Vec::new();
        for it in &items {
            let mut mine: Vec<String> = Vec::new();
            for (folder, _) in &it.commit_in {
                if !of_folder.iter().any(|(f, _)| f == folder) {
                    of_folder.push((folder.clone(), main_checkout(folder).map(|(t, _)| t)));
                }
                let Some(top) = of_folder.iter().find(|(f, _)| f == folder).and_then(|(_, t)| t.clone()) else { continue };
                if !tops.iter().any(|(t, _)| *t == top) {
                    let Some(head) = git("", &top, &[], &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()) else { continue };
                    tops.push((top.clone(), head));
                }
                if !mine.contains(&top) {
                    mine.push(top);
                }
            }
            of_item.push((it, mine));
        }
        let seen: Vec<(String, String, Option<String>)> = self.write(|st| {
            tops.iter().map(|(top, head)| (top.clone(), head.clone(), st.desk.seen_heads.insert(top.clone(), head.clone()))).collect()
        });
        let mut taken: Vec<(String, String)> = Vec::new();
        for (top, head, before) in seen {
            let Some(before) = before.filter(|b| *b != head) else { continue };
            let list = git("", &top, &[], &["rev-list", "--no-merges", "--reverse", "--max-count=50", &format!("{before}..{head}")]).unwrap_or_default();
            let new: Vec<&str> = list.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            if new.is_empty() {
                continue;
            }
            for (it, _) in of_item.iter().filter(|(_, m)| m.contains(&top)) {
                if taken.iter().any(|(id, _)| *id == it.id) {
                    continue;
                }
                if let Some(by) = taken_out_by(&top, &head, it, &new) {
                    taken.push((it.id.clone(), by));
                }
            }
        }
        for (id, by) in taken {
            self.desk_taken_out(&id, &by);
        }
    }

    /// Item `id`'s work was taken back out by commit `by` ("<short> (<its subject>)"): sent back to its holder, as
    /// Send back does, saying so on its page and to its holder.
    fn desk_taken_out(&self, id: &str, by: &str) {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = super::desk::Effects::default();
            let Some(i) = st.desk.item_mut(id) else { return fx };
            let h = i.holder.clone();
            let what = format!("Commit {by} took its work back out");
            (i.state, i.done_at, i.finish, i.updated) = (if h.is_some() { "working" } else { "new" }.into(), None, None, super::desk::secs(now));
            if let Some(c) = i.checks.last_mut() {
                c.back = what.clone();
            }
            let (title, project) = (i.title.clone(), i.project.clone());
            let whom = match &h {
                Some(h) => {
                    let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
                    fx.sends.push((h.to().to_string(), format!("{crew}Desk item #{id} ({title}) is open again: {what}. Put it back, then say DONE: #{id}.\nIts fixes stay in #{id}; don't add new items for them.")));
                    format!("sent back to {}", h.name)
                }
                None => "open again".to_string(),
            };
            super::desk::decide(st, &project, Some(id.to_string()), "Cue", "", &what, &whom, now, &mut fx);
            if let Some(d) = st.desk.decisions.last_mut() {
                d.undo = "none".into();
            }
            fx.items.push(id.to_string());
            fx
        });
        self.desk_done(fx);
    }
}
