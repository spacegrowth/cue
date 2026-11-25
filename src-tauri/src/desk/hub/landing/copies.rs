//! Copies cleared away once their work is in the app (#376). As an item lands, its own copy goes (desk/hub/landing/trees.rs), and
//! so do its helpers' (the copies its holder's teammates worked in, `isolation: worktree`) whose work is all on the
//! main line too. A helper copy with work not on the main line stays; one inside the item's copy is moved out first,
//! so removing the item's copy doesn't take it along.
//!
//! Copies from before this, or left some other way, are found once a launch (`desk_old_copies_tick`): every copy of a
//! Desk's repository whose work is already on its main line, with its size. The first time a repository has any, its
//! Desk's lead asks you in its chat (how many, how big: Yes / Not now; asked once, ever, #397). How work lands has
//! Remove old working folders for any time after (`desk_old_copies_list`). Nothing is removed until you say yes;
//! each is looked at again just before it goes.
//!
//! Never touched: a copy with anything not on the main line (a commit, an edit, a new file git keeps), one a session
//! here works in (live or parked), one a teammate at work uses, an item's copy or a dropped item's kept one, the main
//! checkout, and copies in the temp folder or Cue's own folder (other tools' scratch, Cue's installs).

use crate::desk::hub::landing::git::git;
use crate::desk::hub::landing::main_line::{main_checkout, same_dir};
use crate::desk::hub::landing::tree_files::{remove, rev, snapshot};
use crate::desk::hub::prelude::*;
use crate::desk::ItemTree;
use serde::Serialize;
use std::path::Path;

/// A copy whose work is all on its main line: its folder, branch ("" when none), and size on disk (KB).
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct OldCopy {
    pub path: String,
    pub branch: String,
    pub kb: u64,
}

/// What the last look found for a repository: what a yes removes (each looked at again first).
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct OldCopies {
    pub copies: Vec<OldCopy>,
    pub kb: u64,
}

/// Where the repositories whose old copies were asked about are listed, one a line: each is asked once, ever.
fn asked_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("old-copies-asked")
}

/// Whether `repo`'s old copies were asked about; marks it asked when not.
fn ask_once(repo: &str) -> bool {
    let f = asked_file();
    let had = std::fs::read_to_string(&f).unwrap_or_default();
    if had.lines().any(|l| l == repo) {
        return false;
    }
    let _ = std::fs::write(&f, format!("{had}{repo}\n"));
    true
}

/// "1.2 GB", "340 MB", "12 KB": `kb` in words.
pub(crate) fn size_words(kb: u64) -> String {
    match kb {
        k if k >= 1024 * 1024 => format!("{:.1} GB", k as f64 / (1024.0 * 1024.0)),
        k if k >= 1024 => format!("{} MB", k / 1024),
        k => format!("{k} KB"),
    }
}

/// Whether everything in copy `path` is on `repo`'s main line already: its commits, its edits and new files (ignored
/// ones, build output, left out), landed as they were or squashed into another commit. Measured by bringing it up to
/// the main line: nothing of it changes the main line. False whenever git can't say.
pub(crate) fn work_on_main(repo: &str, path: &str) -> bool {
    // Not a copy of its own (no `.git` there): git would read the checkout around it instead.
    if !Path::new(path).join(".git").exists() {
        return false;
    }
    let Some((_, onto)) = main_checkout(repo) else { return false };
    if onto.is_empty() {
        return false;
    }
    let (Some(main), Some(head), Some(work)) = (rev(repo, &onto), rev(path, "HEAD"), snapshot(path)) else { return false };
    let Some(main_tree) = rev(repo, &format!("{main}^{{tree}}")) else { return false };
    if work == main_tree {
        return true;
    }
    let Some(base) = git("", repo, &[], &["merge-base", &head, &main]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) else { return false };
    let Some(c) = git("", repo, &[], &["commit-tree", &work, "-p", &head, "-m", "copy"]).map(|c| c.trim().to_string()) else { return false };
    let filters = crate::safe_git::no_repo_filters("", repo);
    match crate::safe_git::run("", repo, &filters, &["merge-tree", "--write-tree", "--merge-base", &base, &main, &c]) {
        Ok((out, 0)) => out.lines().next().map(str::trim) == Some(main_tree.as_str()),
        _ => false,
    }
}

/// `a` is `b` or inside it (both as the disk has them, symlinks followed, when they exist).
fn within(a: &str, b: &str) -> bool {
    let real = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
    let (a, b) = (real(a), real(b));
    a.starts_with(&b)
}

/// Folders something here works in, each with why: an item's copy (other than item `except`'s; one inside a copy counts
/// too) or a kept one, a session here working in it (live or parked), or a teammate at work in it. Gathered with the
/// store held and compared after (`in_use`), so no file system is touched under it.
fn users(st: &Store, except: &str) -> Vec<(String, &'static str, bool)> {
    let mut out: Vec<(String, &'static str, bool)> = Vec::new();
    out.extend(st.desk.items.iter().filter(|i| i.id != except).filter_map(|i| i.tree.as_ref()).map(|t| (t.path.clone(), "an item works in it", true)));
    out.extend(st.desk.kept.iter().map(|k| (k.path.clone(), "a dropped item's kept copy", false)));
    // Picked up again (#608), its copy not moved back yet.
    out.extend(st.desk.items.iter().filter_map(|i| i.kept_copy.as_ref()).map(|k| (k.path.clone(), "a picked-up item's kept copy", false)));
    let here = |o: &crate::model::Origin| (o.machine.is_empty() && !o.cwd.is_empty()).then(|| (o.cwd.clone(), "a session works in it", false));
    out.extend(st.sessions.iter().filter_map(|s| here(&s.origin)));
    out.extend(st.parked.iter().filter_map(|p| here(&p.session.origin)));
    let mates = st.sessions.iter().filter(|s| s.origin.machine.is_empty()).flat_map(|s| s.team.iter());
    out.extend(mates.filter(|t| t.working() && !t.worktree.is_empty()).map(|t| (t.worktree.clone(), "a teammate works in it", false)));
    out
}

/// Why copy `path` is in use and stays, or None (`users`: what works where).
fn in_use(users: &[(String, &'static str, bool)], path: &str) -> Option<&'static str> {
    users.iter().find(|(p, _, inside)| within(p, path) || (*inside && within(path, p))).map(|u| u.1)
}

/// Branches made only for a copy (an item's, a teammate's): removed with it. Any other stays.
fn copy_branch(b: &str) -> bool {
    b.starts_with("desk/") || b.starts_with("desk-") || b.starts_with("worktree-agent-")
}

/// Copy `path` of `repo` removed, and its branch when it was made for it.
fn remove_copy(repo: &str, path: &str) {
    let branch = git("", path, &[], &["branch", "--show-current"]).map(|b| b.trim().to_string()).unwrap_or_default();
    let branch = if copy_branch(&branch) { branch } else { String::new() };
    remove(&ItemTree { repo: repo.to_string(), path: path.to_string(), branch, ..Default::default() });
}

/// Size of folder `path` on disk, KB (links not followed: a copy's shared build folder isn't its).
fn size_kb(path: &str) -> u64 {
    let out = std::process::Command::new("du").args(["-sk", path]).output();
    out.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().next().and_then(|n| n.parse().ok())).unwrap_or(0)
}

/// `repo`'s copies (worktrees) other than its main checkout, each with its branch: only ones on disk, and none in the
/// temp folder or Cue's own folder when the repository isn't there.
fn copies_of(repo: &str) -> Vec<(String, String)> {
    let Some(list) = git("", repo, &[], &["worktree", "list", "--porcelain"]) else { return Vec::new() };
    let skip = [std::env::temp_dir().to_string_lossy().to_string(), "/tmp".into(), "/private/tmp".into(), crate::server::cue_dir().to_string_lossy().to_string()];
    let mut out = Vec::new();
    for block in list.split("\n\n") {
        let Some(path) = block.lines().find_map(|l| l.strip_prefix("worktree ")) else { continue };
        let branch = block.lines().find_map(|l| l.strip_prefix("branch refs/heads/")).unwrap_or("").to_string();
        if same_dir(path, repo) || block.lines().any(|l| l.starts_with("prunable") || l.starts_with("locked") || l == "bare") || !Path::new(path).is_dir() {
            continue;
        }
        // Another tool's scratch copy (unless the repository itself is there too).
        if skip.iter().any(|s| !s.is_empty() && within(path, s) && !within(repo, s)) {
            continue;
        }
        out.push((path.to_string(), branch));
    }
    out
}

impl Hub {
    /// Item `id` landed on `repo`'s main line: its holder's teammates' copies whose work is all there too are
    /// removed. One with work not there stays; one inside the item's copy `inside` (about to be removed) is moved out
    /// beside it first. Runs git: off the hub's threads.
    pub(crate) fn desk_clear_helpers(&self, id: &str, repo: &str, inside: Option<&str>) {
        let mates: Vec<(String, String, String)> = self.read(|st| {
            let Some(h) = st.desk.item(id).and_then(|i| i.holder.clone()) else { return Vec::new() };
            let lead = h.to().to_string();
            let Some(s) = st.sessions.get(&lead).filter(|s| s.origin.machine.is_empty()) else { return Vec::new() };
            s.team.iter().filter(|t| !t.worktree.is_empty() && !t.working()).map(|t| (lead.clone(), t.call.clone(), t.worktree.clone())).collect()
        });
        for (lead, call, path) in mates {
            if !Path::new(&path).is_dir() || !main_checkout(&path).is_some_and(|(m, _)| same_dir(&m, repo)) {
                continue;
            }
            if in_use(&self.read(|st| users(st, id)), &path).is_none() && work_on_main(repo, &path) {
                remove_copy(repo, &path);
                continue;
            }
            // Its work isn't all on the main line: kept, out of the item's copy that goes now.
            let Some(item_copy) = inside.filter(|c| within(&path, c)) else { continue };
            let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let to = format!("{repo}/{}/{name}", crate::desk::hub::landing::trees::TREES);
            if Path::new(&to).exists() || item_copy.is_empty() {
                eprintln!("cue: desk: #{id}'s helper copy {path} has work in it, and {to} is taken: left in the item's copy");
                continue;
            }
            match crate::desk::hub::landing::git::git_said("", repo, &[], &["worktree", "move", &path, &to]) {
                Ok(_) => {
                    self.write(|st| st.sessions.with_teammate(&lead, &call, |m| m.worktree = to.clone()));
                }
                Err(e) => eprintln!("cue: desk: #{id}'s helper copy stays at {path}: {e}"),
            }
        }
    }

    /// Once a launch, a minute in: each Desk's repository looked at for copies whose work is already on its main
    /// line. The first time one has any, a Desk of it with a lead asks you (`desk_old_copies_ask`).
    pub(crate) fn desk_old_copies_tick(&self) {
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if DONE.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        self.later(60_000, |h| h.desk_old_copies_look());
    }

    /// Each Desk's repository looked at, once each; asked about on the first Desk of it with a lead.
    pub(crate) fn desk_old_copies_look(&self) {
        let desks: Vec<(String, String)> = self.read(|st| {
            st.desk.lead.keys().map(|p| (p.clone(), crate::desk::hub::core::view::desk_folder(st, p))).filter(|(_, f)| !f.is_empty()).collect()
        });
        let mut seen = std::collections::HashSet::new();
        for (desk, folder) in desks {
            let Some((repo, _)) = main_checkout(&folder) else { continue };
            if !seen.insert(repo.clone()) {
                continue;
            }
            let found = self.desk_old_copies_find(&repo);
            if !found.copies.is_empty() && ask_once(&repo) {
                self.desk_old_copies_ask(&desk, &found);
            }
        }
    }

    /// Desk `desk`'s lead asks you, in its chat, whether to remove the old copies `found`.
    fn desk_old_copies_ask(&self, desk: &str, found: &OldCopies) {
        let Some(lead) = self.read(|st| st.desk.lead_sid(desk).map(str::to_string)) else { return };
        let n = found.copies.len();
        let q = format!(
            "{n} old working {} ({}) {} work that's already on main, and nothing is using {}. Remove {}? Any that gets new work or comes into use first stays. You can do this later in How work lands.",
            if n == 1 { "folder" } else { "folders" },
            size_words(found.kb),
            if n == 1 { "has" } else { "have" },
            if n == 1 { "it" } else { "them" },
            if n == 1 { "it" } else { "them" },
        );
        let id = self.desk_ask(&lead, desk, &q, &["Yes".to_string(), "Not now".to_string()], None, None);
        if let Some(id) = id {
            self.write(|st| {
                if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
                    it.tool_input["desk_old"] = serde_json::json!(desk);
                }
            });
        }
    }

    /// The old copies question on Desk `desk` answered: `yes` removes them (off the hub's threads), and what was
    /// done is noted on the Desk. Nothing is sent to the lead: the question was Cue's, not its.
    pub(crate) fn desk_old_copies_answered(&self, desk: String, yes: bool) {
        if !yes {
            return;
        }
        self.later(0, move |h| {
            let words = match h.desk_old_copies_answer(&desk, true) {
                Ok(r) => {
                    let removed = r["removed"].as_u64().unwrap_or(0);
                    let kept = r["kept"].as_array().map_or(0, |k| k.len());
                    let mut w = format!("Removed {removed} old working {} · {} freed", if removed == 1 { "folder" } else { "folders" }, size_words(r["kb"].as_u64().unwrap_or(0)));
                    if kept > 0 {
                        w += &format!(" · {kept} kept: {}", r["kept"][0]["why"].as_str().unwrap_or(""));
                    }
                    w
                }
                Err(e) => format!("Couldn't remove the old working folders: {e}"),
            };
            let mut fx = crate::desk::hub::core::desk::Effects::default();
            h.write(|st| {
                crate::desk::hub::turn::route::decide(st, &desk, None, "You", "", &words, "old working folders", now_ms(), &mut fx);
                if let Some(d) = st.desk.decisions.last_mut() {
                    d.undo = "none".into();
                }
            });
            h.desk_done(fx);
        });
    }

    /// Desk `desk`'s repository looked at now: its old copies, for How work lands' Remove old working folders.
    pub(crate) fn desk_old_copies_list(&self, desk: &str) -> Result<OldCopies, String> {
        let folder = self.read(|st| crate::desk::hub::core::view::desk_folder(st, desk));
        let (repo, _) = main_checkout(&folder).ok_or("This Desk's folder isn't a git repository")?;
        Ok(self.desk_old_copies_find(&repo))
    }

    /// `repo`'s copies whose work is all on its main line and that nothing uses, with their sizes, kept for its Desks.
    pub(crate) fn desk_old_copies_find(&self, repo: &str) -> OldCopies {
        let mut found = OldCopies::default();
        let busy = self.read(|st| users(st, ""));
        for (path, branch) in copies_of(repo) {
            if in_use(&busy, &path).is_some() || !work_on_main(repo, &path) {
                continue;
            }
            let kb = size_kb(&path);
            found.kb += kb;
            found.copies.push(OldCopy { path, branch, kb });
        }
        let shown = found.clone();
        self.write(|st| {
            st.desk.run.old_copies.insert(repo.to_string(), found);
        });
        shown
    }

    /// Your answer to Desk `desk`'s old copies, as last looked at: `yes` removes each (looked at again first: one now
    /// in use, or with work not on the main line, stays); no removes nothing. What was removed and what was kept.
    pub(crate) fn desk_old_copies_answer(&self, desk: &str, yes: bool) -> Result<serde_json::Value, String> {
        let folder = self.read(|st| crate::desk::hub::core::view::desk_folder(st, desk));
        let (repo, _) = main_checkout(&folder).ok_or("This Desk's folder isn't a git repository")?;
        let listed = self.read(|st| st.desk.run.old_copies.get(&repo).cloned()).unwrap_or_default();
        if !yes {
            return Ok(serde_json::json!({ "removed": 0, "kb": 0, "kept": [] }));
        }
        let (mut removed, mut kb, mut kept) = (0, 0, Vec::new());
        let busy = self.read(|st| users(st, ""));
        for c in &listed.copies {
            if !Path::new(&c.path).exists() {
                continue;
            }
            if let Some(why) = in_use(&busy, &c.path) {
                kept.push(serde_json::json!({ "path": c.path, "why": why }));
                continue;
            }
            if !work_on_main(&repo, &c.path) {
                kept.push(serde_json::json!({ "path": c.path, "why": "it has work not on main yet" }));
                continue;
            }
            remove_copy(&repo, &c.path);
            removed += 1;
            kb += c.kb;
        }
        self.write(|st| {
            st.desk.run.old_copies.remove(&repo);
        });
        self.changed();
        Ok(serde_json::json!({ "removed": removed, "kb": kb, "kept": kept }))
    }
}
