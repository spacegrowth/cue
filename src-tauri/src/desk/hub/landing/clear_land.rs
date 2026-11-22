//! Clear and land (#659): a landing stopped on edits in the main folder that are already on the item's own branch
//! (the same file there, byte for byte: a leftover copy of its work) can be cleared with one OK. Each such file's
//! diff is saved to work/ in the main folder, the file is set back to main, and the landing runs again. A file that
//! also holds another session's edits is named and left alone; one that isn't the same as the branch isn't offered.

use crate::desk::hub::landing::git::git;
use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;

/// Of `files` in the way in main checkout `main`, those already on branch tip `sha` (the file there now is the
/// branch's, or both have none): what Clear and land may set back.
pub(crate) fn on_branch(main: &str, sha: &str, files: &[String]) -> Vec<String> {
    files.iter().filter(|f| same_as_branch(main, sha, f)).cloned().collect()
}

/// File `path` in `main` is as it is at `sha` (both gone counts).
fn same_as_branch(main: &str, sha: &str, path: &str) -> bool {
    let theirs = git("", main, &[], &["rev-parse", "-q", "--verify", &format!("{sha}:{path}")]).map(|s| s.trim().to_string());
    let here = std::path::Path::new(main).join(path);
    if !here.is_file() {
        return theirs.is_none() && !here.exists();
    }
    let ours = git("", main, &[], &["hash-object", "--no-filters", "--", path]).map(|s| s.trim().to_string());
    ours.is_some() && ours == theirs
}

/// Each of `files`' diff from HEAD in `main` (a new file's whole text), one after another: what's saved to work/.
fn diffs_of(main: &str, files: &[String]) -> String {
    let filters = crate::safe_git::no_repo_filters("", main);
    let mut out = String::new();
    for f in files {
        let tracked = git("", main, &[], &["ls-files", "--error-unmatch", "--", f]).is_some();
        let d = if tracked {
            git("", main, &[], &["diff", "--no-color", "--no-ext-diff", "--binary", "HEAD", "--", f]).unwrap_or_default()
        } else {
            // --no-index exits 1 when the two differ: its output is the diff all the same.
            crate::safe_git::run("", main, &filters, &["diff", "--no-color", "--no-ext-diff", "--binary", "--no-index", "--", "/dev/null", f]).map(|(o, _)| o).unwrap_or_default()
        };
        out.push_str(&d);
        if !d.is_empty() && !d.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// File `f` in `main` set back to HEAD: a tracked one checked out again, a new one taken out of the index and removed.
fn set_back(main: &str, f: &str) -> Result<(), String> {
    if git("", main, &[], &["cat-file", "-e", &format!("HEAD:{f}")]).is_some() {
        return crate::desk::hub::landing::git::git_said("", main, &[], &["checkout", "-q", "HEAD", "--", f]).map(|_| ()).map_err(|e| format!("{f} couldn't be set back: {e}"));
    }
    git("", main, &[], &["reset", "-q", "--", f]);
    let p = std::path::Path::new(main).join(f);
    if p.exists() {
        std::fs::remove_file(&p).map_err(|e| format!("{f} couldn't be removed: {e}"))?;
    }
    Ok(())
}

impl Hub {
    /// Item `it`'s landing stopped on `files` in the way in main checkout `main`, its branch at `sha`: which of them
    /// Clear and land may set back (already on the branch, nobody else's edits in them), and those left alone with
    /// who else has edits there. Kept on its `finish` for the window.
    pub(crate) fn finish_clearable(&self, it: &DeskItem, main: &str, sha: &str, files: &[String]) {
        if files.is_empty() || sha.is_empty() {
            return;
        }
        let others = self.others_in(it, main, files);
        let free: Vec<String> = files.iter().filter(|f| !others.iter().any(|(p, _)| p == *f)).cloned().collect();
        let clear = on_branch(main, sha, &free);
        if clear.is_empty() {
            return;
        }
        let keep: Vec<(String, String)> = others;
        let (main, sha) = (main.to_string(), sha.to_string());
        self.finish_fin(&it.id, |f| (f.clear, f.clear_keep, f.clear_in, f.clear_sha) = (clear, keep, main, sha));
    }

    /// Clear and land (#659), your OK: item `id`'s files in the way that are already on its branch, each one's diff
    /// saved to work/ in the main folder, set back to main, and its landing run again. A file that changed since it
    /// was offered (no longer the branch's, or someone else's edits in it now) is left alone.
    pub(crate) fn desk_clear_and_land(&self, id: &str) -> Result<DeskItem, String> {
        let (it, fin) = self
            .read(|st| st.desk.item(id).filter(|i| i.state == "finishing").and_then(|i| i.finish.clone().map(|f| (i.clone(), f))))
            .ok_or("It isn't stopped while landing")?;
        if fin.clear.is_empty() || fin.clear_in.is_empty() {
            return Err("Nothing in the main folder is Cue's to clear for it".into());
        }
        let main = fin.clear_in.clone();
        let others = self.others_in(&it, &main, &fin.clear);
        let free: Vec<String> = fin.clear.iter().filter(|f| !others.iter().any(|(p, _)| p == *f)).cloned().collect();
        let files = on_branch(&main, &fin.clear_sha, &free);
        if files.is_empty() {
            return Err("Its files in the main folder changed since: none is the same as its branch now".into());
        }
        let patch = diffs_of(&main, &files);
        let dir = std::path::Path::new(&main).join("work");
        std::fs::create_dir_all(&dir).map_err(|e| format!("work/ couldn't be made: {e}"))?;
        let name = format!("{id}-cleared-{}.patch", now_ms() / 1000);
        std::fs::write(dir.join(&name), patch).map_err(|e| format!("work/{name} couldn't be saved: {e}"))?;
        for f in &files {
            set_back(&main, f)?;
        }
        eprintln!("cue: desk: #{id}: cleared {} from the main folder (diffs in work/{name}) and landing it again", files.join(", "));
        self.desk_rerun_landing(id)
    }
}
