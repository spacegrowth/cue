//! Looks right, with a commit: the item's files that git says changed (not those another session at work is editing),
//! each folder's in its own repository with `git commit --only`, its title the message, once none of their added
//! lines (nor a new file's name, nor the message) has one of the project's words never committed. Never pushed. Then
//! the item is done, with its commit; Undo takes the commit back while it's still the last one. When another session
//! at work is editing some of its files, the item is done at once and the commit waits (`DeskItem::commit_wait`, kept
//! with the item, so across a restart too): Cue's tick makes it once they're free.

use super::*;
use crate::desk::{BusyFile, CommitWait, DeskItem};

/// One file of an item's to commit: the folder it was changed in, and its path there.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(super) struct DeskFile {
    pub folder: String,
    pub path: String,
}

/// The tree with nothing in it: what a repository with no commit yet is compared with.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

impl Hub {
    /// Commit item `id`'s `files` (as the plan gave them; each must be one of its files) with `message`, after the
    /// word check, then mark it done with its commit. Nothing is committed when a word is found, a file isn't the
    /// item's, or a folder isn't in a repository.
    pub(super) fn desk_commit(&self, id: &str, files: &[DeskFile], message: &str) -> Result<DeskItem, String> {
        let message = message.trim();
        if message.is_empty() {
            return Err("Give it a commit message".into());
        }
        if files.is_empty() {
            return Err("No file is ticked: nothing to commit".into());
        }
        let (it, machine, sources) = self.desk_commit_sources(id)?;
        // Each folder's ticked files, in the order the plan had them.
        let mut todo: Vec<(String, Vec<String>)> = Vec::new();
        for f in files {
            if !sources.iter().any(|(folder, paths)| *folder == f.folder && paths.contains(&f.path)) {
                return Err(format!("{} isn't one of this item's files", f.path));
            }
            match todo.iter_mut().find(|(folder, _)| *folder == f.folder) {
                Some((_, paths)) if paths.contains(&f.path) => {}
                Some((_, paths)) => paths.push(f.path.clone()),
                None => todo.push((f.folder.clone(), vec![f.path.clone()])),
            }
        }
        let made = commit_files(&self.read(|st| st.desk.project_of(&it.project).to_string()), &machine, &todo, message)?;
        self.write(|st| {
            if let Some(i) = st.desk.item_mut(id) {
                i.commit = shas_of(&made);
                i.commit_in = made;
            }
        });
        self.desk_item_act(id, "looks_right", "")
    }

    /// Item `id` (ready to check), and where its files were changed: its machine and each folder's files: its crew's
    /// (hub/desk.rs `item_sources`) and, held by a session itself, what that session changed while it held it.
    pub(super) fn desk_commit_sources(&self, id: &str) -> Result<(DeskItem, String, Vec<(String, Vec<String>)>), String> {
        let found = self.desk_change_sources(id)?;
        if found.0.state != "to_check" {
            return Err("It isn't ready to check".into());
        }
        Ok(found)
    }

    /// Item `id` in any state, and where its files were changed so far, as `desk_commit_sources` (its page's Files
    /// tab while it's being worked on: hub/desk_diff.rs).
    pub(super) fn desk_change_sources(&self, id: &str) -> Result<(DeskItem, String, Vec<(String, Vec<String>)>), String> {
        let found = self.read(|st| {
            st.desk.item(id).map(|i| {
                let (machine, sources) = super::desk::item_sources(st, i);
                (i.clone(), machine, sources, own_log(st, i))
            })
        });
        let (it, machine, mut sources, own) = found.ok_or("no such item")?;
        // Its log is read here, with the store let go.
        if let Some(o) = own {
            let files = in_folder(crate::logs::steps::edited_between(&o.log, o.from, o.until), &o.folder);
            add_sources(&mut sources, &o.folder, files);
        }
        Ok((it, machine, sources))
    }
}

impl Hub {
    /// Looks right on item `id`: its changed files committed at once (its title the message, after the word check,
    /// never pushed) and the item done with the commit; with nothing to commit, just done. When another session at
    /// work now is editing some of them, it's done at once and the commit waits for them (`desk_commit_waiting`):
    /// { commit, files, waiting: [{ path, by }] }.
    pub(crate) fn desk_looks_right(&self, id: &str) -> Result<Value, String> {
        let (it, machine, sources) = self.desk_commit_sources(id)?;
        let busy = if machine.is_empty() { self.busy_files(&it) } else { Vec::new() };
        let mut files: Vec<DeskFile> = Vec::new();
        let mut waiting: Vec<BusyFile> = Vec::new();
        for (folder, paths) in &sources {
            for p in changed_of(&machine, folder, paths).unwrap_or_default() {
                let abs = format!("{}/{p}", folder.trim_end_matches('/'));
                if let Some((_, by)) = busy.iter().find(|(f, _)| *f == abs) {
                    waiting.push(BusyFile { path: p.clone(), by: by.clone() });
                }
                files.push(DeskFile { folder: folder.clone(), path: p });
            }
        }
        if !waiting.is_empty() {
            let wait = CommitWait { files: files.iter().map(|f| (f.folder.clone(), f.path.clone())).collect(), machine, busy: waiting.clone(), error: String::new() };
            self.write(|st| {
                if let Some(i) = st.desk.item_mut(id) {
                    i.commit_wait = Some(wait);
                }
            });
            self.desk_item_act(id, "looks_right", "")?;
            return Ok(json!({ "commit": "", "files": files.len(), "waiting": waiting }));
        }
        let it = if files.is_empty() { self.desk_item_act(id, "looks_right", "")? } else { self.desk_commit(id, &files, &it.title)? };
        Ok(json!({ "commit": it.commit, "files": files.len(), "waiting": [] }))
    }

    /// Each done item's waiting commit (`desk_looks_right`), made once none of its files is being edited by a session
    /// at work: committed as Looks right would (the word check, never pushed), or why not kept on it. Run by Cue's
    /// tick (every ~10 s and as a turn ends), so after a restart too; one run at a time.
    pub(super) fn desk_commit_waiting(&self) {
        static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let waits: Vec<DeskItem> = self.read(|st| st.desk.items.iter().filter(|i| i.state == "done" && i.commit_wait.as_ref().is_some_and(|w| w.error.is_empty())).cloned().collect());
        if waits.is_empty() || RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        for it in &waits {
            self.desk_commit_wait(it);
        }
        RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn desk_commit_wait(&self, it: &DeskItem) {
        let Some(w) = it.commit_wait.clone() else { return };
        let busy = if w.machine.is_empty() { self.busy_files(it) } else { Vec::new() };
        let still: Vec<BusyFile> = w.files.iter().filter_map(|(folder, p)| {
            let abs = format!("{}/{p}", folder.trim_end_matches('/'));
            busy.iter().find(|(f, _)| *f == abs).map(|(_, by)| BusyFile { path: p.clone(), by: by.clone() })
        }).collect();
        let (commit, wait) = if !still.is_empty() {
            if still == w.busy {
                return;
            }
            (None, Some(CommitWait { busy: still, ..w.clone() }))
        } else {
            // Free: those of its files still changed, committed.
            let todo: Vec<(String, Vec<String>)> = by_folder(&w.files).into_iter().filter_map(|(folder, paths)| {
                let left = changed_of(&w.machine, &folder, &paths).unwrap_or_default();
                (!left.is_empty()).then_some((folder, left))
            }).collect();
            if todo.is_empty() {
                (None, None)
            } else {
                match commit_files(&self.read(|st| st.desk.project_of(&it.project).to_string()), &w.machine, &todo, &it.title) {
                    Ok(made) => (Some(made), None),
                    Err(e) => (None, Some(CommitWait { error: e, ..w.clone() })),
                }
            }
        };
        let saved = self.desk_saving(|| {
            let it = self.write(|st| {
                let i = st.desk.item_mut(&it.id).filter(|i| i.commit_wait.is_some())?;
                if let Some(sha) = &commit {
                    i.commit = shas_of(sha);
                    i.commit_in = sha.clone();
                }
                i.commit_wait = wait;
                Some(i.clone())
            });
            it.inspect(crate::db::desk_put_item).is_some()
        });
        if saved {
            self.changed();
        }
    }

    /// Undo on a Looks right: the item ready to check again, as it was before, and never a change to its code. A
    /// commit Looks right made is taken back as a commit only (`git reset --soft` one commit, then its files
    /// unstaged; every change stays in the folder), while it's still the last commit in each place it went; when
    /// another came after it, the commit stays as it is (taking it back then would mean changing code) and the item
    /// still goes back to check, its commit kept on it so the window can say so. A commit still waiting just stops
    /// waiting; an item done with nothing to commit just goes back.
    pub(crate) fn desk_commit_undo(&self, id: &str) -> Result<DeskItem, String> {
        let it = self.read(|st| st.desk.item(id).cloned()).ok_or("no such item")?;
        if it.state != "done" {
            return Err("It isn't done: nothing to undo".into());
        }
        if it.commit.is_empty() {
            return self.desk_reopen_checked(id, |i| i.commit_wait = None);
        }
        let (machine, folders) = self.read(|st| {
            let (m, src) = super::desk::item_sources(st, &it);
            let mut f: Vec<String> = src.into_iter().map(|(f, _)| f).collect();
            f.extend(own_log(st, &it).map(|o| o.folder));
            (m, f)
        });
        let shas: Vec<&str> = it.commit.split_whitespace().collect();
        let mut tops: Vec<String> = Vec::new();
        for f in &folders {
            if let Some(top) = git(&machine, f, &[], &["rev-parse", "--show-toplevel"]).map(|t| t.trim().to_string()) {
                if !tops.contains(&top) {
                    tops.push(top);
                }
            }
        }
        let heads: Vec<String> = tops.iter().filter(|t| git(&machine, t, &[], &["rev-parse", "--short", "HEAD"]).is_some_and(|h| shas.contains(&h.trim()))).cloned().collect();
        if heads.len() != shas.len() {
            return self.desk_reopen_checked(id, |_| {});
        }
        for top in &heads {
            let paths = git(&machine, top, &[], &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]).unwrap_or_default();
            let p: Vec<&str> = paths.lines().filter(|l| !l.is_empty()).collect();
            git(&machine, top, &[], &["reset", "-q", "--soft", "HEAD~1"]).ok_or("Can't undo it: git couldn't take the commit back")?;
            git(&machine, top, &[], &[&["reset", "-q", "--"][..], &p].concat());
        }
        self.desk_reopen_checked(id, |i| (i.commit, i.commit_in) = (String::new(), Vec::new()))
    }

    /// Item `id` ready to check again, with `f` done to it too; saved.
    fn desk_reopen_checked(&self, id: &str, f: impl FnOnce(&mut DeskItem)) -> Result<DeskItem, String> {
        let it = self.desk_saving(|| {
            let it = self.write(|st| {
                let i = st.desk.item_mut(id)?;
                (i.state, i.done_at, i.updated) = ("to_check".into(), None, super::desk::secs(now_ms()));
                f(i);
                Some(i.clone())
            });
            it.inspect(crate::db::desk_put_item)
        });
        self.changed();
        it.ok_or_else(|| "no such item".into())
    }

    /// Files sessions at work now are editing (absolute), and who: each session's own edits since its turn began
    /// (read from its log), and its working teammates'. Not the holder of `it`.
    fn busy_files(&self, it: &DeskItem) -> Vec<(String, String)> {
        let holder = it.holder.as_ref().map(|h| h.to().to_string()).unwrap_or_default();
        let (logs, mut out) = self.read(|st| {
            let mut logs: Vec<(String, String, String, u64)> = Vec::new();
            let mut out: Vec<(String, String)> = Vec::new();
            for s in st.sessions.iter().filter(|s| s.origin.session_id != holder && s.origin.machine.is_empty()) {
                let name = super::desk::shown_name(s);
                for t in s.team.iter().filter(|t| t.working()) {
                    let dir = t.folder(&s.origin.cwd).trim_end_matches('/').to_string();
                    out.extend(t.files.iter().map(|f| (if f.starts_with('/') { f.clone() } else { format!("{dir}/{f}") }, name.clone())));
                }
                if matches!(s.state.as_str(), "working" | "deciding" | "agent") && !s.origin.transcript_path.is_empty() {
                    let asked = s.thread.iter().rev().find(|e| e.role == "you").map_or(s.since_ms, |e| e.at_ms.min(s.since_ms));
                    logs.push((s.origin.transcript_path.clone(), s.origin.cwd.clone(), name, asked));
                }
            }
            (logs, out)
        });
        for (log, cwd, name, from) in logs {
            let dir = cwd.trim_end_matches('/').to_string();
            for f in crate::logs::steps::edited_between(&log, from, u64::MAX) {
                out.push((if f.starts_with('/') { f } else { format!("{dir}/{f}") }, name.clone()));
            }
        }
        out
    }
}

/// Each folder's `paths` committed with `message` once none of their added lines (nor a new file's name, nor the
/// message) has one of `project`'s never-committed words: the commits, short, space apart.
fn commit_files(project: &str, machine: &str, todo: &[(String, Vec<String>)], message: &str) -> Result<Vec<(String, String)>, String> {
    let words = crate::config::never_words(project);
    if let Some(w) = words.iter().find(|w| message.to_lowercase().contains(&w.to_lowercase())) {
        return Err(format!("Not committed: \"{w}\" is in the message"));
    }
    for (folder, paths) in todo {
        if let Some((w, at)) = word_in(&added_diff(machine, folder, paths)?, &words) {
            return Err(format!("Not committed: \"{w}\" is in {at}"));
        }
    }
    let mut shas: Vec<(String, String)> = Vec::new();
    let mut done: Vec<String> = Vec::new();
    for (folder, paths) in todo {
        match commit_only(machine, folder, paths, message) {
            Ok(sha) => {
                done.push(format!("{} ({sha})", crate::model::folder_of(folder)));
                shas.push((folder.clone(), sha));
            }
            // One repository's commit failed after another's was made: said with what was made.
            Err(e) if !done.is_empty() => return Err(format!("{e}. Committed already: {}", done.join(", "))),
            Err(e) => return Err(e),
        }
    }
    Ok(shas)
}

/// The commits, as an item keeps them: short, space apart.
fn shas_of(made: &[(String, String)]) -> String {
    made.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(" ")
}

/// `files` ((folder, path)) by folder, in their order.
pub(super) fn by_folder(files: &[(String, String)]) -> Vec<(String, Vec<String>)> {
    let mut todo: Vec<(String, Vec<String>)> = Vec::new();
    for (folder, path) in files {
        match todo.iter_mut().find(|(f, _)| f == folder) {
            Some((_, paths)) if paths.contains(path) => {}
            Some((_, paths)) => paths.push(path.clone()),
            None => todo.push((folder.clone(), vec![path.clone()])),
        }
    }
    todo
}

/// Where a session's own changes for an item are read from: its log, its folder, and when it held the item (ms).
pub(super) struct OwnLog {
    log: String,
    pub(super) folder: String,
    from: u64,
    until: u64,
}

/// `it`'s holder's own log, when a session here holds it itself (not a crew member, nor a teammate it named) with a
/// known start. A session on another machine has its log there: none (its crew's files still count).
pub(super) fn own_log(st: &Store, it: &DeskItem) -> Option<OwnLog> {
    let h = it.holder.as_ref().filter(|h| h.lead_sid.is_empty() && it.crew.is_empty())?;
    let s = st.sessions.get(&h.sid).filter(|s| s.origin.machine.is_empty() && !s.origin.transcript_path.is_empty())?;
    let (from, until) = super::desk::held_window(st, &h.sid, it)?;
    Some(OwnLog { log: s.origin.transcript_path.clone(), folder: s.origin.cwd.clone(), from, until })
}

/// Of `files` (absolute, or relative to `folder`), those inside `folder` and not git's own, relative to it.
fn in_folder(files: Vec<String>, folder: &str) -> Vec<String> {
    let base = format!("{}/", folder.trim_end_matches('/'));
    files.into_iter().filter_map(|f| {
        let rel = if f.starts_with('/') { f.strip_prefix(&base)?.to_string() } else { f.trim_start_matches("./").to_string() };
        let ok = !rel.is_empty() && !rel.starts_with('~') && rel != ".git" && !rel.starts_with(".git/") && !std::path::Path::new(&rel).components().any(|c| c == std::path::Component::ParentDir);
        ok.then_some(rel)
    }).collect()
}

/// `files` added to `folder`'s in `sources`, each once.
fn add_sources(sources: &mut Vec<(String, Vec<String>)>, folder: &str, files: Vec<String>) {
    if files.is_empty() {
        return;
    }
    let at = match sources.iter().position(|(f, _)| f.trim_end_matches('/') == folder.trim_end_matches('/')) {
        Some(at) => at,
        None => {
            sources.push((folder.to_string(), Vec::new()));
            sources.len() - 1
        }
    };
    for f in files {
        if !sources[at].1.contains(&f) {
            sources[at].1.push(f);
        }
    }
}

/// git in `dir` on `machine` ("" here), with the repository's own filters off (safe_git.rs): its output, when it
/// worked.
pub(super) fn git(machine: &str, dir: &str, env: &[(&str, &str)], args: &[&str]) -> Option<String> {
    let filters = crate::safe_git::no_repo_filters(machine, dir);
    crate::safe_git::run_env(machine, dir, &filters, env, args).ok().filter(|(_, code)| *code == 0).map(|(o, _)| o)
}

/// Of `paths` (relative to `folder`), those git says changed: tracked with changes since HEAD, or new and not
/// ignored. None when `folder` isn't in a repository git can read.
pub(super) fn changed_of(machine: &str, folder: &str, paths: &[String]) -> Option<Vec<String>> {
    let p: Vec<&str> = paths.iter().map(String::as_str).collect();
    git(machine, folder, &[], &["rev-parse", "--show-toplevel"])?;
    let base = if git(machine, folder, &[], &["rev-parse", "-q", "--verify", "HEAD^{commit}"]).is_some() { "HEAD" } else { EMPTY_TREE };
    let tracked = git(machine, folder, &[], &[&["diff", "--name-only", "--relative", base, "--"][..], &p].concat()).unwrap_or_default();
    let fresh = git(machine, folder, &[], &[&["ls-files", "--others", "--exclude-standard", "--"][..], &p].concat()).unwrap_or_default();
    let said: Vec<&str> = tracked.lines().chain(fresh.lines()).map(str::trim).filter(|l| !l.is_empty()).collect();
    Some(paths.iter().filter(|f| said.contains(&f.as_str())).cloned().collect())
}

/// What committing `paths` would add, as a diff with no context: made in a scratch index (HEAD's tree with those
/// files added), so your own index and files stay as they are.
fn added_diff(machine: &str, folder: &str, paths: &[String]) -> Result<String, String> {
    staged_diff(machine, folder, paths, &["-U0"])
}

/// The same, with `opts` for git diff (its context, prefixes, paths relative to `folder`).
pub(super) fn staged_diff(machine: &str, folder: &str, paths: &[String], opts: &[&str]) -> Result<String, String> {
    let name = crate::model::folder_of(folder);
    let git_dir = git(machine, folder, &[], &["rev-parse", "--absolute-git-dir"]).ok_or_else(|| format!("Not committed: {name} isn't in a git repository"))?;
    let has_head = git(machine, folder, &[], &["rev-parse", "-q", "--verify", "HEAD^{commit}"]).is_some();
    let base = if has_head { "HEAD" } else { EMPTY_TREE };
    let index = format!("{}/cue-desk-index-{}", git_dir.trim().trim_end_matches('/'), crate::clock::now_ms());
    let env = [("GIT_INDEX_FILE", index.as_str())];
    let p: Vec<&str> = paths.iter().map(String::as_str).collect();
    let add: Vec<&str> = [&["add", "-A", "--"][..], &p].concat();
    let diff: Vec<&str> = [&["diff", "--cached"][..], opts, &["--no-color", "--no-ext-diff", "--no-textconv", base, "--"][..], &p].concat();
    let read = if has_head { vec!["read-tree", "HEAD"] } else { vec!["read-tree", "--empty"] };
    let out = git(machine, folder, &env, &read).and_then(|_| git(machine, folder, &env, &add)).and_then(|_| git(machine, folder, &env, &diff));
    super::team::remove_file(machine, &index);
    out.ok_or_else(|| format!("Not committed: git couldn't read the changes in {name}"))
}

/// The first of `words` (any case) in `diff`'s added lines, or in the name of a file it adds: the word, and where
/// ("path: the line").
fn word_in(diff: &str, words: &[String]) -> Option<(String, String)> {
    let words: Vec<(String, String)> = words.iter().filter(|w| !w.trim().is_empty()).map(|w| (w.clone(), w.to_lowercase())).collect();
    let find = |text: &str| {
        let low = text.to_lowercase();
        words.iter().find(|(_, l)| low.contains(l.as_str())).map(|(w, _)| w.clone())
    };
    let mut file = String::new();
    let mut fresh = false;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ ") {
            file = path.strip_prefix("b/").unwrap_or(path).to_string();
            if fresh {
                if let Some(w) = find(&file) {
                    return Some((w, format!("the name {file}")));
                }
            }
            continue;
        }
        if line.starts_with("--- ") {
            fresh = line == "--- /dev/null";
            continue;
        }
        if let Some(added) = line.strip_prefix('+') {
            if let Some(w) = find(added) {
                let shown: String = added.trim().chars().take(80).collect();
                return Some((w, format!("{file}: {shown}")));
            }
        }
    }
    None
}

/// `paths` (relative to `folder`) committed on the branch `folder` is on, and only they: `git commit --only`, so
/// whatever else is staged stays staged and out of it. The commit, short.
fn commit_only(machine: &str, folder: &str, paths: &[String], message: &str) -> Result<String, String> {
    let name = crate::model::folder_of(folder);
    let p: Vec<&str> = paths.iter().map(String::as_str).collect();
    // Added: the ones in the index or new in the folder. A file already taken out with `git rm` is in neither, and
    // `git add` stops on it ("did not match any files"); `commit --only` takes its removal in as it is.
    let known = git(machine, folder, &[], &[&["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--"][..], &p].concat()).unwrap_or_default();
    let known: Vec<&str> = known.split('\0').collect();
    let addable: Vec<&str> = p.iter().copied().filter(|f| known.contains(f)).collect();
    if !addable.is_empty() {
        git(machine, folder, &[], &[&["add", "-A", "--"][..], &addable].concat()).ok_or_else(|| format!("Not committed: git couldn't add the files in {name}"))?;
    }
    git(machine, folder, &[], &[&["commit", "--only", "-q", "-m", message, "--"][..], &p].concat()).ok_or_else(|| format!("Not committed: git didn't commit the files in {name} (its hooks, or nothing to commit)"))?;
    Ok(git(machine, folder, &[], &["rev-parse", "--short", "HEAD"]).map(|o| o.trim().to_string()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::word_in;

    fn w(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_word_in_an_added_line_is_found_in_any_case_and_said_where() {
        let diff = "diff --git a/ui/a.js b/ui/a.js\n--- a/ui/a.js\n+++ b/ui/a.js\n@@ -1 +1 @@\n-old line with Secret\n+const x = \"Secret plan\";\n";
        assert_eq!(word_in(diff, &w(&["secret"])), Some(("secret".into(), "ui/a.js: const x = \"Secret plan\";".into())));
    }

    #[test]
    fn a_removed_line_or_an_old_files_name_is_fine_a_new_files_name_isnt() {
        let removed = "--- a/secret.txt\n+++ b/secret.txt\n@@ -1 +0,0 @@\n-the secret\n";
        assert_eq!(word_in(removed, &w(&["secret"])), None);
        let fresh = "--- /dev/null\n+++ b/notes/secret.md\n@@ -0,0 +1 @@\n+hello\n";
        assert_eq!(word_in(fresh, &w(&["SECRET"])), Some(("SECRET".into(), "the name notes/secret.md".into())));
    }

    #[test]
    fn no_words_finds_nothing() {
        assert_eq!(word_in("--- a/x\n+++ b/x\n+anything\n", &w(&[" ", ""])), None);
    }
}
