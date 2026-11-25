//! Looks right, with a commit: the item's files that git says changed (not those another session at work is editing),
//! each folder's in its own repository with `git commit --only`, its title the message, once none of their added
//! lines (nor a new file's name, nor the message) has one of the project's words never committed. Never pushed. Then
//! the item is done, with its commit; Undo takes the commit back while it's still the last one. When another session
//! at work is editing some of its files, the item is done at once and the commit waits (`DeskItem::commit_wait`, kept
//! with the item, so across a restart too): Cue's tick makes it once they're free.

use crate::desk::hub::prelude::*;
use crate::desk::hub::landing::git::{because, git, git_said, word_in};
use crate::desk::hub::theirs::theirs::Part;
use crate::desk::{BusyFile, CommitWait, DeskItem};

/// One file of an item's to commit: the folder it was changed in, and its path there.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct DeskFile {
    pub folder: String,
    pub path: String,
}

/// The tree with nothing in it: what a repository with no commit yet is compared with.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

impl Hub {
    /// Commit item `id`'s `files` (as the plan gave them; each must be one of its files) with `message`, after the
    /// word check, then mark it done with its commit. Nothing is committed when a word is found, a file isn't the
    /// item's, or a folder isn't in a repository.
    pub(crate) fn desk_commit(&self, id: &str, files: &[DeskFile], message: &str) -> Result<DeskItem, String> {
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
        // Other sessions' changes in the same files stay out of it (desk/hub/theirs/theirs.rs).
        let (todo, parts, mixed) = if machine.is_empty() { self.own_of(&it, todo) } else { (todo, Vec::new(), Vec::new()) };
        if let Some((p, by)) = mixed.first() {
            return Err(format!("Not committed: {by} also changed {p}, and its changes can't be told apart from this item's"));
        }
        let made = if todo.is_empty() { Vec::new() } else { commit_files(&self.read(|st| st.desk.project_of(&it.project).to_string()), &machine, &todo, &parts, message)? };
        self.write(|st| {
            if let Some(i) = st.desk.item_mut(id) {
                i.commit = shas_of(&made);
                i.commit_in = made;
            }
        });
        self.desk_item_act(id, "looks_right", "")
    }

    /// Item `id` (ready to check, or finishing), and where its files were changed: its machine and each folder's files: its crew's
    /// (desk/hub/core/desk.rs `item_sources`) and, held by a session itself, what that session changed while it held it.
    pub(crate) fn desk_commit_sources(&self, id: &str) -> Result<(DeskItem, String, Vec<(String, Vec<String>)>), String> {
        let found = self.desk_change_sources(id)?;
        if !matches!(found.0.state.as_str(), "to_check" | "finishing") {
            return Err("It isn't ready to check".into());
        }
        Ok(found)
    }

    /// Item `id` in any state, and where its files were changed so far, as `desk_commit_sources` (its page's Files
    /// tab while it's being worked on: desk/hub/landing/diff.rs).
    pub(crate) fn desk_change_sources(&self, id: &str) -> Result<(DeskItem, String, Vec<(String, Vec<String>)>), String> {
        let found = self.read(|st| {
            st.desk.item(id).map(|i| {
                let (machine, sources) = crate::desk::hub::core::desk::item_sources(st, i);
                (i.clone(), machine, sources, own_log(st, i), former_logs(st, i))
            })
        });
        let (it, machine, mut sources, own, former) = found.ok_or("no such item")?;
        // Its logs are read here, with the store let go: its holder's, then each former holder's for its stint on it
        // (#830: #810's change was its first holder's, and its next holder's DONE saw no files of it).
        let ended = former.iter().filter(|(_, o)| o.is_none()).filter_map(|(sid, _)| crate::db::ended_session(sid).map(|(_, s)| (sid.clone(), s)));
        let ended: Vec<(String, crate::sessions::Session)> = ended.collect();
        let former = former.into_iter().filter_map(|(sid, o)| o.or_else(|| ended.iter().find(|(e, _)| *e == sid).and_then(|(_, s)| former_of(s, &it, &sid))));
        for o in own.into_iter().chain(former.flatten()) {
            let files = in_folder(crate::logs::steps::edited_between(&o.log, o.from, o.until), &o.folder);
            add_sources(&mut sources, &o.folder, files);
        }
        Ok((it, machine, sources))
    }

    /// The files item `id`'s holder changed outside its own folder while it held it (absolute paths, from its own
    /// log): work in a repository beside its project's, such as a private one linked into it. Never committed with the
    /// item (Looks right takes only its folder's); a reopened item's DONE counts them once they're on that main
    /// (desk/hub/landing/unlanded.rs).
    pub(crate) fn desk_edits_elsewhere(&self, id: &str) -> Vec<String> {
        let Some(o) = self.read(|st| st.desk.item(id).and_then(|i| own_log(st, i))) else { return Vec::new() };
        let base = format!("{}/", o.folder.trim_end_matches('/'));
        crate::logs::steps::edited_between(&o.log, o.from, o.until).into_iter().filter(|f| f.starts_with('/') && !f.starts_with(&base)).collect()
    }
}

impl Hub {
    /// Looks right on item `id`: its changed files committed at once (its title the message, after the word check,
    /// never pushed) and the item done with the commit; with nothing to commit, just done. When another session at
    /// work now is editing some of them, it's done at once and the commit waits for them (`desk_commit_waiting`):
    /// { commit, files, waiting: [{ path, by }] }.
    /// An item finished at its DONE here (desk/hub/landing/finish.rs: its commit made then) is only closed; a remote session's
    /// work never closes uncommitted.
    pub(crate) fn desk_looks_right(&self, id: &str) -> Result<Value, String> {
        let (it, machine, sources) = self.desk_commit_sources(id)?;
        if it.finish.is_some() && (machine.is_empty() || !it.commit_in.is_empty()) {
            if it.state != "to_check" {
                return Err("It isn't ready to check".into());
            }
            let it = self.desk_item_act(id, "looks_right", "")?;
            return Ok(json!({ "commit": it.commit, "files": 0, "waiting": [] }));
        }
        // Its project's work is yours to commit (desk/lands.rs): checked, it's done, its files left as they are.
        if self.item_lands(&it).how == "owner" {
            let it = self.desk_item_act(id, "looks_right", "")?;
            return Ok(json!({ "commit": it.commit, "files": 0, "waiting": [] }));
        }
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
        if machine.is_empty() && waiting.is_empty() {
            // Files another session has changed too, in a way that can't be told apart: they wait as well.
            let (_, _, mixed) = self.own_of(&it, by_folder(&files.iter().map(|f| (f.folder.clone(), f.path.clone())).collect::<Vec<_>>()));
            waiting.extend(mixed.into_iter().map(|(path, by)| BusyFile { path, by }));
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
    pub(crate) fn desk_commit_waiting(&self) {
        static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        // Items finishing (desk/hub/landing/finish.rs): their commits and installs.
        self.desk_finish_tick();
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
            let (todo, parts, mixed) = if w.machine.is_empty() { self.own_of(it, todo) } else { (todo, Vec::new(), Vec::new()) };
            let mixed: Vec<BusyFile> = mixed.into_iter().map(|(path, by)| BusyFile { path, by }).collect();
            if !mixed.is_empty() {
                if mixed == w.busy {
                    return;
                }
                (None, Some(CommitWait { busy: mixed, ..w.clone() }))
            } else if todo.is_empty() {
                (None, None)
            } else {
                match commit_files(&self.read(|st| st.desk.project_of(&it.project).to_string()), &w.machine, &todo, &parts, &it.title) {
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
        // Finished at its DONE (desk/hub/landing/finish.rs): its commit, made then, stays.
        if it.commit.is_empty() || it.finish.is_some() {
            return self.desk_reopen_checked(id, |i| i.commit_wait = None);
        }
        let (machine, folders) = self.read(|st| {
            let (m, src) = crate::desk::hub::core::desk::item_sources(st, &it);
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
                (i.state, i.done_at, i.updated) = ("to_check".into(), None, crate::desk::hub::core::desk::secs(now_ms()));
                f(i);
                Some(i.clone())
            });
            it.inspect(crate::db::desk_put_item)
        });
        self.changed();
        it.ok_or_else(|| "no such item".into())
    }

    /// Files sessions at work now are editing (absolute), and who: each session's own edits since its turn began
    /// (read from its log), and its working teammates'. Not the holder of `it`. Who is "someone else" when Cue only
    /// guesses it (a file read from a shell command, not named by an edit): never a name it doesn't know (#250).
    pub(crate) fn busy_files(&self, it: &DeskItem) -> Vec<(String, String)> {
        self.busy_by(it).into_iter().map(|(f, name, _)| (f, if name.is_empty() { SOMEONE_ELSE.to_string() } else { name })).collect()
    }

    /// `busy_files`, each with its session's name and id: both "" when Cue only guesses it.
    pub(crate) fn busy_by(&self, it: &DeskItem) -> Vec<(String, String, String)> {
        let holder = it.holder.as_ref().map(|h| h.to().to_string()).unwrap_or_default();
        let (logs, mut out) = self.read(|st| {
            let mut logs: Vec<(String, String, String, String, u64)> = Vec::new();
            let mut out: Vec<(String, String, String)> = Vec::new();
            for s in st.sessions.iter().filter(|s| s.origin.session_id != holder && s.origin.machine.is_empty()) {
                let name = crate::desk::hub::core::desk::shown_name(s);
                let sid = s.origin.session_id.clone();
                for t in s.team.iter().filter(|t| t.working()) {
                    let dir = t.folder(&s.origin.cwd).trim_end_matches('/').to_string();
                    out.extend(t.files.iter().map(|f| (if f.starts_with('/') { f.clone() } else { format!("{dir}/{f}") }, name.clone(), sid.clone())));
                }
                if matches!(s.state.as_str(), "working" | "deciding" | "agent") && !s.origin.transcript_path.is_empty() {
                    let asked = s.thread.iter().rev().find(|e| e.role == "you").map_or(s.since_ms, |e| e.at_ms.min(s.since_ms));
                    logs.push((s.origin.transcript_path.clone(), s.origin.cwd.clone(), name, sid, asked));
                }
            }
            (logs, out)
        });
        for (log, cwd, name, sid, from) in logs {
            let dir = cwd.trim_end_matches('/').to_string();
            for (f, named) in crate::logs::steps::edited_between_named(&log, from, u64::MAX) {
                let (name, sid) = if named { (name.clone(), sid.clone()) } else { (String::new(), String::new()) };
                out.push((if f.starts_with('/') { f } else { format!("{dir}/{f}") }, name, sid));
            }
        }
        // A file both named and guessed: the name stands.
        out.sort_by_key(|(_, name, _)| name.is_empty());
        out
    }
}

/// Who a wait names when Cue doesn't know whose edits they are (#250).
pub(crate) const SOMEONE_ELSE: &str = "someone else";

/// Each folder's `paths` committed with `message` once none of their added lines (nor a new file's name, nor the
/// message) has one of `project`'s never-committed words: the commits, short, space apart.
pub(crate) fn commit_files(project: &str, machine: &str, todo: &[(String, Vec<String>)], parts: &[Part], message: &str) -> Result<Vec<(String, String)>, String> {
    words_checked(project, machine, todo, parts, message)?;
    match commit_each(machine, todo, parts, message) {
        (made, None) => Ok(made),
        (_, Some(e)) => Err(e),
    }
}

/// Err when `message`, or a line `todo` adds (a file in `parts`: its part only), has one of `project`'s
/// never-committed words.
pub(crate) fn words_checked(project: &str, machine: &str, todo: &[(String, Vec<String>)], parts: &[Part], message: &str) -> Result<(), String> {
    let words = crate::desk::hub::landing::push_check::words_for(project);
    if let Some(w) = words.iter().find(|w| message.to_lowercase().contains(&w.to_lowercase())) {
        return Err(format!("Not committed: \"{w}\" is in the message"));
    }
    for (folder, paths) in todo {
        if let Some((w, at)) = word_in(&added_diff(machine, folder, paths, parts)?, &words) {
            return Err(format!("Not committed: \"{w}\" is in {at}"));
        }
    }
    Ok(())
}

/// Each folder's `paths` committed (words already checked; a file in `parts` with its part only): the commits made,
/// kept when a later one failed, and what stopped it.
pub(crate) fn commit_each(machine: &str, todo: &[(String, Vec<String>)], parts: &[Part], message: &str) -> (Vec<(String, String)>, Option<String>) {
    let mut shas: Vec<(String, String)> = Vec::new();
    let mut done: Vec<String> = Vec::new();
    for (folder, paths) in todo {
        let mine: Vec<&Part> = parts.iter().filter(|p| p.folder == *folder && paths.contains(&p.path)).collect();
        let made = if mine.is_empty() { commit_only(machine, folder, paths, message) } else { commit_parts(folder, paths, &mine, message) };
        match made {
            Ok(sha) => {
                done.push(format!("{} ({sha})", crate::model::folder_of(folder)));
                shas.push((folder.clone(), sha));
            }
            // One repository's commit failed after another's was made: said with what was made.
            Err(e) if !done.is_empty() => return (shas, Some(format!("{e}. Committed already: {}", done.join(", ")))),
            Err(e) => return (shas, Some(e)),
        }
    }
    (shas, None)
}

/// The commits, as an item keeps them: short, space apart.
pub(crate) fn shas_of(made: &[(String, String)]) -> String {
    made.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(" ")
}

/// `files` ((folder, path)) by folder, in their order.
pub(crate) fn by_folder(files: &[(String, String)]) -> Vec<(String, Vec<String>)> {
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
pub(crate) struct OwnLog {
    pub(crate) log: String,
    pub(crate) folder: String,
    pub(crate) from: u64,
    pub(crate) until: u64,
}

/// `it`'s holder's own log, when a session here holds it itself (not a crew member, nor a teammate it named) with a
/// known start. A session on another machine has its log there: none (its crew's files still count).
pub(crate) fn own_log(st: &Store, it: &DeskItem) -> Option<OwnLog> {
    let h = it.holder.as_ref().filter(|h| h.lead_sid.is_empty() && it.crew.is_empty())?;
    let s = st.sessions.get(&h.sid).filter(|s| s.origin.machine.is_empty() && !s.origin.transcript_path.is_empty())?;
    let (from, until) = crate::desk::hub::core::desk::held_window(st, &h.sid, it)?;
    Some(OwnLog { log: s.origin.transcript_path.clone(), folder: s.origin.cwd.clone(), from, until })
}

/// Each of `it`'s former holders (`held_before`) by session id, with its log for its stint on the item when Cue
/// still has that session (None: looked up among the ended ones, off the store). Crews' and other machines' are left out.
fn former_logs(st: &Store, it: &DeskItem) -> Vec<(String, Option<Vec<OwnLog>>)> {
    let mut out: Vec<(String, Option<Vec<OwnLog>>)> = Vec::new();
    for (sid, _, _) in &it.held_before {
        if it.holder.as_ref().is_some_and(|h| h.sid == *sid) || out.iter().any(|(s, _)| s == sid) {
            continue;
        }
        let here = st.sessions.get(sid).or_else(|| st.parked.iter().map(|p| &p.session).find(|s| s.origin.session_id == *sid)).or_else(|| st.ended.iter().map(|(_, s)| s).find(|s| s.origin.session_id == *sid));
        out.push((sid.clone(), here.and_then(|s| former_of(s, it, sid))));
    }
    out
}

/// Former holder `sid`'s (session `s`) log, once for each of its stints on `it`.
fn former_of(s: &crate::sessions::Session, it: &DeskItem, sid: &str) -> Option<Vec<OwnLog>> {
    if !s.origin.machine.is_empty() || s.origin.transcript_path.is_empty() {
        return None;
    }
    let o = &s.origin;
    Some(it.held_before.iter().filter(|(b, _, _)| b == sid).map(|(_, from, until)| OwnLog { log: o.transcript_path.clone(), folder: o.cwd.clone(), from: *from, until: *until }).collect())
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

/// Of `paths` (relative to `folder`), those git says changed: tracked with changes since HEAD, or new and not
/// ignored. None when `folder` isn't in a repository git can read.
pub(crate) fn changed_of(machine: &str, folder: &str, paths: &[String]) -> Option<Vec<String>> {
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
fn added_diff(machine: &str, folder: &str, paths: &[String], parts: &[Part]) -> Result<String, String> {
    let mine: Vec<&Part> = parts.iter().filter(|p| p.folder == folder).collect();
    staged_diff_with(machine, folder, paths, &mine, &["-U0"])
}

/// The same, with `opts` for git diff (its context, prefixes, paths relative to `folder`).
pub(crate) fn staged_diff(machine: &str, folder: &str, paths: &[String], opts: &[&str]) -> Result<String, String> {
    staged_diff_with(machine, folder, paths, &[], opts)
}

/// The same, a file in `parts` with its part only.
fn staged_diff_with(machine: &str, folder: &str, paths: &[String], parts: &[&Part], opts: &[&str]) -> Result<String, String> {
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
    let out = git(machine, folder, &env, &read).and_then(|_| git(machine, folder, &env, &add)).and_then(|_| put_parts(folder, &git_dir, &env, parts)).and_then(|_| git(machine, folder, &env, &diff));
    crate::hub::team::remove_file(machine, &index);
    out.ok_or_else(|| format!("Not committed: git couldn't read the changes in {name}"))
}

/// Err while `folder` (here) is the project's own folder on another item's own branch ("A branch",
/// desk/hub/landing/in_place.rs): a commit there would go onto that item's branch, not the main line. Nothing is
/// committed; it's tried again once that item has landed or been dropped.
pub(crate) fn not_on_item_branch(machine: &str, folder: &str, name: &str) -> Result<(), String> {
    if !machine.is_empty() {
        return Ok(());
    }
    let Some((top, _)) = crate::desk::hub::landing::main_line::main_checkout(folder) else { return Ok(()) };
    // Only in the main checkout itself: another worktree of the repository is on a branch of its own.
    let here = git("", folder, &[], &["rev-parse", "--show-toplevel"]).map(|t| t.trim().to_string()).unwrap_or_default();
    if !crate::desk::hub::landing::main_line::same_dir(&here, &top) {
        return Ok(());
    }
    match crate::desk::hub::landing::in_place::on_item_branch(&top) {
        Some((b, _)) => Err(format!("Not committed: {name} is on {b}, another Desk item's own branch, till that item lands or is dropped; say DONE again then")),
        None => Ok(()),
    }
}

/// `paths` (relative to `folder`) committed on the branch `folder` is on, and only they: `git commit --only`, so
/// whatever else is staged stays staged and out of it. The commit, short.
fn commit_only(machine: &str, folder: &str, paths: &[String], message: &str) -> Result<String, String> {
    let name = crate::model::folder_of(folder);
    not_on_item_branch(machine, folder, &name)?;
    let p: Vec<&str> = paths.iter().map(String::as_str).collect();
    // Added: the ones in the index or new in the folder. A file already taken out with `git rm` is in neither, and
    // `git add` stops on it ("did not match any files"); `commit --only` takes its removal in as it is.
    let known = git(machine, folder, &[], &[&["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--"][..], &p].concat()).unwrap_or_default();
    let known: Vec<&str> = known.split('\0').collect();
    let addable: Vec<&str> = p.iter().copied().filter(|f| known.contains(f)).collect();
    if !addable.is_empty() {
        git_said(machine, folder, &[], &[&["add", "-A", "--"][..], &addable].concat()).map_err(|why| because(format!("Not committed: git couldn't add the files in {name}"), &why))?;
    }
    git_said(machine, folder, &[], &[&["commit", "--only", "-q", "-m", message, "--"][..], &p].concat()).map_err(|why| because(format!("Not committed: git didn't commit the files in {name}"), &why))?;
    Ok(git(machine, folder, &[], &["rev-parse", "--short", "HEAD"]).map(|o| o.trim().to_string()).unwrap_or_default())
}

/// `parts` (files here) put in the index `env` names, as they're to be committed: each a blob of its own, the
/// file's mode kept. Some(()) when all went in.
fn put_parts(folder: &str, git_dir: &str, env: &[(&str, &str)], parts: &[&Part]) -> Option<()> {
    for p in parts {
        let tmp = format!("{}/cue-desk-part-{}", git_dir.trim().trim_end_matches('/'), crate::clock::now_ms());
        std::fs::write(&tmp, &p.content).ok()?;
        let blob = git("", folder, &[], &["hash-object", "-w", "--no-filters", &tmp]);
        let _ = std::fs::remove_file(&tmp);
        let blob = blob?.trim().to_string();
        let exec = std::fs::metadata(format!("{}/{}", folder.trim_end_matches('/'), p.path)).is_ok_and(|m| {
            use std::os::unix::fs::PermissionsExt;
            m.permissions().mode() & 0o111 != 0
        });
        let mode = if exec { "100755" } else { "100644" };
        git("", folder, env, &["update-index", "--add", "--cacheinfo", &format!("{mode},{blob},{}", p.path)])?;
    }
    Some(())
}

/// `paths` (relative to `folder`, here) committed with `parts` holding only the item's part of theirs: the commit is
/// made from a scratch index (HEAD's tree, `paths` from the folder, `parts` as given), so the files stay as they are
/// and what's not the item's stays uncommitted; your index then has those paths as committed. The commit, short.
fn commit_parts(folder: &str, paths: &[String], parts: &[&Part], message: &str) -> Result<String, String> {
    let name = crate::model::folder_of(folder);
    not_on_item_branch("", folder, &name)?;
    let fail = |what: &str| format!("Not committed: git couldn't {what} in {name}");
    let git_dir = git("", folder, &[], &["rev-parse", "--absolute-git-dir"]).ok_or_else(|| format!("Not committed: {name} isn't in a git repository"))?;
    let head = git("", folder, &[], &["rev-parse", "-q", "--verify", "HEAD^{commit}"]).map(|h| h.trim().to_string());
    let index = format!("{}/cue-desk-index-{}", git_dir.trim().trim_end_matches('/'), crate::clock::now_ms());
    let env = [("GIT_INDEX_FILE", index.as_str())];
    let read = if head.is_some() { vec!["read-tree", "HEAD"] } else { vec!["read-tree", "--empty"] };
    let whole: Vec<&str> = paths.iter().filter(|p| !parts.iter().any(|x| x.path == **p)).map(String::as_str).collect();
    let made = (|| {
        git("", folder, &env, &read).ok_or_else(|| fail("read HEAD"))?;
        if !whole.is_empty() {
            git("", folder, &env, &[&["add", "-A", "--"][..], &whole].concat()).ok_or_else(|| fail("add the files"))?;
        }
        put_parts(folder, &git_dir, &env, parts).ok_or_else(|| fail("add the item's part of a file"))?;
        // HEAD moved while it was made: its tree would undo that commit.
        if git("", folder, &[], &["rev-parse", "-q", "--verify", "HEAD^{commit}"]).map(|h| h.trim().to_string()) != head {
            return Err(format!("Not committed: {name} got a new commit meanwhile"));
        }
        git_said("", folder, &env, &["commit", "-q", "-m", message]).map_err(|why| because(format!("Not committed: git didn't commit the files in {name}"), &why))
    })();
    let _ = std::fs::remove_file(&index);
    made?;
    let p: Vec<&str> = paths.iter().map(String::as_str).collect();
    git("", folder, &[], &[&["reset", "-q", "--"][..], &p].concat());
    Ok(git("", folder, &[], &["rev-parse", "--short", "HEAD"]).map(|o| o.trim().to_string()).unwrap_or_default())
}
