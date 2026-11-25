//! An item worked on in its holder's own folder goes to you to check only once its work is on main (#830). Its DONE
//! commits the files Cue knows it changed, but a file Cue never saw it change (written by a shell command) stayed
//! uncommitted, and the item still came to check with nothing of it on main. Now, after its commit, something of it
//! must be on main: a commit Cue made for it, a branch of its merged, a file of it in git and unchanged now (its
//! holder committed it itself), or a commit whose message names it. And none of its files may be
//! left uncommitted, but those Cue leaves for another session at work on them (an item's commit never waits on
//! another's). Otherwise it goes back to its holder to commit its own lines. An item whose "done when" says the
//! repository doesn't change (mocks, plans, decisions: desk/hub/landing/unlanded.rs `rules_out_code`) isn't held to it.
//!
//! Sent back for no commit alone (nothing of it uncommitted), its next DONE goes on as it is: its holder looked again
//! and it needed no change in the repository. Files still uncommitted send it back every time.

use crate::desk::hub::landing::git::git;
use crate::desk::hub::landing::main_line::main_ref;
use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;

/// The start of the words an item goes back with when nothing of it is on main.
pub(crate) const NO_COMMIT: &str = "Nothing of it is committed on main";

/// What item `id` goes back to its holder with: its files left `uncommitted` (each in the folder it was changed in),
/// or, with none, no commit of it on main.
pub(crate) fn not_committed_words(id: &str, uncommitted: &[String]) -> String {
    let how = format!("commit only your own lines, with #{id} in the commit's message, then say DONE");
    if uncommitted.is_empty() {
        return format!("{NO_COMMIT}: no commit there names #{id}. If it changed files, {how}. If it truly changes nothing in the repository, say DONE again and it goes to check as it is");
    }
    let shown: Vec<&str> = uncommitted.iter().take(5).map(String::as_str).collect();
    let more = if uncommitted.len() > shown.len() { format!(" and {} more", uncommitted.len() - shown.len()) } else { String::new() };
    format!("Its changes aren't all committed: {}{more} still uncommitted. {}", shown.join(", "), how[..1].to_uppercase() + &how[1..])
}

/// Whether a commit on `top`'s main line names item `id` (#830, not #8300) in its message.
pub(crate) fn named_on_main(top: &str, id: &str) -> bool {
    let Some(main) = main_ref(top) else { return false };
    let grep = format!("--grep=#{id}([^0-9]|$)");
    git("", top, &[], &["log", "-1", "-E", "--format=%H", &grep, &main, "--"]).is_some_and(|o| !o.trim().is_empty())
}

/// Whether a commit on `top`'s main line that names item `id` changed `file` (absolute, in `top`'s repository).
fn named_on_main_in(top: &str, id: &str, file: &str) -> bool {
    let Some(main) = main_ref(top) else { return false };
    let Some(rel) = std::path::Path::new(file).strip_prefix(top).ok().map(|r| r.to_string_lossy().to_string()) else { return false };
    let grep = format!("--grep=#{id}([^0-9]|$)");
    git("", top, &[], &["log", "-1", "-E", "--format=%H", &grep, &main, "--", &rel]).is_some_and(|o| !o.trim().is_empty())
}

/// Whether item `it`'s round before this one was sent back by Cue for no commit alone.
fn told_no_commit(it: &DeskItem) -> bool {
    let n = it.checks.len();
    n >= 2 && it.checks[n - 2].back_by == "Cue" && it.checks[n - 2].back.starts_with(NO_COMMIT)
}

impl Hub {
    /// Item `it`'s own changes git says still aren't committed after its commit (absolute paths): what's left in its
    /// files that is the item's, as its commit tells them apart (desk/hub/theirs/theirs.rs `own_of`), but files Cue left
    /// out for another session on purpose (`Finish::without`: one at work editing it, or whose changes in it can't
    /// be told apart from the item's): an item's commit never waits on another's. Then whether a file of it is in git
    /// and unchanged now (its holder committed its work itself), and whether every file of it is one git ignores (a
    /// scratch script: nothing of it was ever for the repository).
    fn own_uncommitted(&self, it: &DeskItem) -> (Vec<String>, bool, bool) {
        let Ok((_, machine, sources)) = self.desk_change_sources(&it.id) else { return (Vec::new(), false, false) };
        if !machine.is_empty() {
            return (Vec::new(), false, false);
        }
        let without: Vec<String> = it.finish.as_ref().map(|f| f.without.clone()).unwrap_or_default();
        let mut landed = false;
        let changed: Vec<(String, Vec<String>)> = sources.iter().filter_map(|(folder, paths)| {
            let c = crate::desk::hub::landing::commit::changed_of("", folder, paths).unwrap_or_default();
            landed |= paths.iter().any(|p| !c.contains(p) && in_head(folder, p));
            (!c.is_empty()).then(|| (folder.clone(), c))
        }).collect();
        let (own, parts, _) = self.own_of(it, changed);
        let mut out = Vec::new();
        for (folder, paths) in &own {
            for p in paths {
                // Its part of a file another session changed too, already committed: only theirs is left in it.
                let part = parts.iter().find(|x| x.folder == *folder && x.path == *p);
                if part.is_some_and(|x| head_has(folder, p, &x.content)) {
                    continue;
                }
                let abs = format!("{}/{p}", folder.trim_end_matches('/'));
                if !without.contains(&abs) && !out.contains(&abs) {
                    out.push(abs);
                }
            }
        }
        let all_ignored = sources.iter().any(|(_, ps)| !ps.is_empty()) && sources.iter().all(|(folder, paths)| paths.iter().all(|p| ignored(folder, p)));
        (out, landed, all_ignored)
    }

    /// Item `id`, its DONE's commit made and merged (its main lines `tops`; `made`: this round committed some of it):
    /// true when it went back to its holder because its own changes aren't all committed, or nothing of it is on main
    /// (#830). A commit from an earlier round doesn't count (#810: an old round's commit let it through with this
    /// round's file left out). Files of it left out for another session hold it back when nothing else of it was
    /// committed, whatever its "done when" says. Otherwise not for an item whose "done when" says the repository
    /// doesn't change, nor one whose work isn't on main for a reason its landing says itself (on a branch of its own,
    /// in a clone; reopened with nothing new: `finish_after`).
    pub(crate) fn sent_back_uncommitted(&self, id: &str, tops: &[String], made: bool) -> bool {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return false };
        if it.holder.is_none() {
            return false;
        }
        let merged = it.finish.as_ref().is_some_and(|f| !f.merges.is_empty());
        let left: Vec<String> = it.finish.as_ref().map(|f| f.without.clone()).unwrap_or_default();
        let repos = if tops.is_empty() { self.item_repos(&it) } else { tops.to_vec() };
        // Its own lines in a file another session changed too: none committed, none named in a commit of its own.
        if !made && !merged && !left.is_empty() && !repos.iter().any(|t| left.iter().any(|f| named_on_main_in(t, id, f))) {
            self.finish_back(&it, &not_committed_words(id, &left), "uncommitted", "", true);
            return true;
        }
        if crate::desk::hub::landing::unlanded::rules_out_code(&it.done_when) {
            return false;
        }
        if self.desk_nothing_new(id) || self.desk_unlanded(id) > 0 || !self.holder_work(&it).off.is_empty() {
            return false;
        }
        let (uncommitted, landed, all_ignored) = self.own_uncommitted(&it);
        if uncommitted.is_empty() && (made || merged || landed || all_ignored || told_no_commit(&it) || repos.iter().any(|t| named_on_main(t, id))) {
            return false;
        }
        self.finish_back(&it, &not_committed_words(id, &uncommitted), "uncommitted", "", true);
        true
    }
}

/// Whether `path` (relative to `folder`) at HEAD is `content`.
fn head_has(folder: &str, path: &str, content: &[u8]) -> bool {
    matches!(crate::safe_git::run("", folder, &[], &["show", &format!("HEAD:{path}")]), Ok((out, 0)) if out.as_bytes() == content)
}

/// Whether `path` (relative to `folder`) is in its HEAD commit.
fn in_head(folder: &str, path: &str) -> bool {
    matches!(crate::safe_git::run("", folder, &[], &["cat-file", "-e", &format!("HEAD:{path}")]), Ok((_, 0)))
}

/// Whether git ignores `path` (relative to `folder`).
fn ignored(folder: &str, path: &str) -> bool {
    matches!(crate::safe_git::run("", folder, &[], &["check-ignore", "-q", "--", path]), Ok((_, 0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_name_the_files_left_or_say_nothing_is_on_main() {
        let none = not_committed_words("830", &[]);
        assert!(none.starts_with(NO_COMMIT) && none.contains("#830 in the commit's message") && none.contains("say DONE again"), "{none}");
        let files: Vec<String> = (1..=7).map(|n| format!("/r/f{n}.rs")).collect();
        let some = not_committed_words("830", &files);
        assert!(some.contains("/r/f1.rs, /r/f2.rs, /r/f3.rs, /r/f4.rs, /r/f5.rs and 2 more still uncommitted. Commit only your own lines"), "{some}");
        assert!(!some.starts_with(NO_COMMIT), "{some}");
    }
}
