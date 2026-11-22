//! Merge (How it lands, desk/lands.rs): an item lands as a merge commit on the main line, its own commits kept under
//! it, instead of one squash. The merge commit's tree is the same one a squash would land (main plus the item, as
//! desk/hub/landing/trees.rs brings it up to the main line); only its parents differ: the main line, then the item's
//! last commit. Work its holder left uncommitted in its copy is committed on top of its own commits first, so nothing
//! in the copy is left out. With no commits of its own (or none that start from where it was cut), its work is one
//! commit of its own under the merge.

use crate::desk::hub::landing::git::{git, word_in};
use crate::desk::hub::landing::tree_files::rev;
use crate::desk::hub::landing::trees::Landed;
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, ItemTree};

impl Hub {
    /// When item `it`'s project lands by merge: the last of its own commits (its work `work` from `base`, anything
    /// uncommitted committed on top), its history word-checked. None: it lands as one commit.
    pub(super) fn land_merge_tip(&self, id: &str, it: &DeskItem, t: &ItemTree, base: &str, work: &str, words: &[String]) -> Result<Option<String>, Landed> {
        if crate::desk::lands::lands_of(crate::desk::hub::landing::other_repo::lands_project(it), &t.repo).how != "merge" {
            return Ok(None);
        }
        let ml = item_tip(&t.repo, &t.path, base, work, &it.title, id);
        let Some(tip) = ml else { return Err(self.land_wait(id, "git couldn't make its commit")) };
        if let Some((w, at)) = word_in_history(&t.repo, base, &tip, words) {
            self.finish_ask(id, "word", &format!("Not landed: \"{w}\" is in {at}"), "");
            return Err(Landed::Done);
        }
        Ok(Some(tip))
    }
}

/// The commit item `id`'s merge takes as its second parent: the last of its own commits in its copy at `path` (from
/// `base`), with its work `work` (a tree) as the copy has it now. None: git failed.
pub(crate) fn item_tip(repo: &str, path: &str, base: &str, work: &str, title: &str, id: &str) -> Option<String> {
    let head = rev(path, "HEAD").filter(|h| h != base && is_ancestor(repo, base, h));
    let Some(head) = head else {
        return commit(repo, work, base, &format!("{title} (#{id})"));
    };
    if rev(repo, &format!("{head}^{{tree}}")).as_deref() == Some(work) {
        return Some(head);
    }
    commit(repo, work, &head, &format!("{title} (#{id}): the rest of its work"))
}

/// The commit an item lands as, its tree `merged`, on top of `main`: a merge of `tip` (its own commits) when it lands
/// by merge, else one commit.
pub(crate) fn land_commit(repo: &str, merged: &str, main: &str, tip: Option<&str>, message: &str) -> Option<String> {
    let mut args = vec!["commit-tree", merged, "-p", main];
    args.extend(tip.map(|t| ["-p", t]).into_iter().flatten());
    args.extend(["-m", message]);
    git("", repo, &[], &args).map(|c| c.trim().to_string()).filter(|c| !c.is_empty())
}

/// The first of `words` in the item's own commits from `base` to `tip` that a merge would put on the main line: in
/// a message or a name on them, or in a line one of them adds (a word added then taken out again still lands, in
/// its history). The word, and where.
pub(crate) fn word_in_history(repo: &str, base: &str, tip: &str, words: &[String]) -> Option<(String, String)> {
    let range = format!("{base}..{tip}");
    let said = git("", repo, &[], &["log", "--format=%an%n%ae%n%cn%n%ce%n%B", &range]).unwrap_or_default().to_lowercase();
    if let Some(w) = words.iter().filter(|w| !w.trim().is_empty()).find(|w| said.contains(&w.to_lowercase())) {
        return Some((w.clone(), "one of its commits' messages or names".into()));
    }
    let diffs = git("", repo, &[], &["log", "-p", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", "--format=", &range]).unwrap_or_default();
    word_in(&diffs, words)
}

fn commit(repo: &str, tree: &str, parent: &str, message: &str) -> Option<String> {
    git("", repo, &[], &["commit-tree", tree, "-p", parent, "-m", message]).map(|c| c.trim().to_string()).filter(|c| !c.is_empty())
}

fn is_ancestor(repo: &str, a: &str, b: &str) -> bool {
    matches!(crate::safe_git::run("", repo, &[], &["merge-base", "--is-ancestor", a, b]), Ok((_, 0)))
}
