//! A card in the lead's chat each time a session says DONE for an item (ui/desk_commit.js): the item, who did it, what
//! to test (its "done when"), the files it touched and whether the installed Cue has it yet. Each DONE is kept on its
//! item (`DeskItem::checks`, saved with it), so the cards are there after a restart.

use super::{Hub, Store};
use crate::desk::DeskItem;
use serde_json::{json, Value};

/// The most cards a project's lead chat gets, newest kept.
const SHOWN: usize = 40;

impl Hub {
    /// A moment later: items a lead gave to a new session started (desk_lead.rs), and the files of items just said
    /// DONE read for their cards.
    pub(super) fn desk_later(&self, starts: Vec<super::desk::NewStart>, checked: Vec<String>) {
        for s in starts {
            self.later(0, move |h| h.desk_start_new(s));
        }
        for id in checked {
            // Its files read for its card before its commit (desk_finish.rs) takes them.
            self.later(0, move |h| {
                h.desk_check_files(&id);
                h.desk_finish_commit(&id);
            });
        }
    }

    /// Item `id`'s latest DONE: the files it touched (as Looks right would offer them) and whether it was made in
    /// this Cue's own source, read with the store let go, then kept.
    pub(super) fn desk_check_files(&self, id: &str) {
        let Ok((it, machine, sources)) = self.desk_commit_sources(id) else { return };
        let many = sources.iter().filter(|(_, f)| !f.is_empty()).count() > 1;
        let mut files: Vec<String> = Vec::new();
        for (folder, list) in &sources {
            let name = folder.trim_end_matches('/').rsplit('/').next().unwrap_or("");
            for f in list {
                let f = if many { format!("{name}/{f}") } else { f.clone() };
                if !files.contains(&f) {
                    files.push(f);
                }
            }
        }
        let holder_cwd = self.read(|st| it.holder.as_ref().and_then(|h| st.sessions.get(h.to())).map(|s| s.origin.cwd.clone()).unwrap_or_default());
        let cue = machine.is_empty() && sources.iter().map(|(f, _)| f.as_str()).chain([holder_cwd.as_str()]).any(builds_this_cue);
        // When its work last changed: its newest file on this Mac, else when this round began (its last send-back,
        // else when it was taken).
        let mut edited = 0u64;
        if cue {
            for (folder, list) in &sources {
                for f in list {
                    let at = std::fs::metadata(std::path::Path::new(folder).join(f)).and_then(|m| m.modified()).ok();
                    let ms = at.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64);
                    edited = edited.max(ms);
                }
            }
        }
        if edited == 0 {
            edited = it.checks.iter().rev().nth(1).map(|c| c.at).or_else(|| it.held_from.as_ref().map(|h| h.1)).unwrap_or(0);
        }
        let saved = self.desk_saving(|| {
            let it = self.write(|st| {
                let it = st.desk.item_mut(id)?;
                let c = it.checks.last_mut()?;
                c.files = files;
                c.cue = cue;
                c.edited = edited;
                Some(it.clone())
            });
            it.inspect(crate::db::desk_put_item).is_some()
        });
        if saved {
            self.changed();
        }
    }
}

/// Whether `folder` (or a folder above it, a few up) holds the source this Cue is built from: its Tauri config names
/// this app.
pub(crate) fn builds_this_cue(folder: &str) -> bool {
    let mut at = std::path::Path::new(folder);
    for _ in 0..4 {
        if at.as_os_str().is_empty() {
            return false;
        }
        let conf = std::fs::read_to_string(at.join("src-tauri/tauri.conf.json")).unwrap_or_default();
        if conf.contains("\"dev.spacegrowth.cue\"") {
            return true;
        }
        let Some(up) = at.parent() else { return false };
        at = up;
    }
    false
}

/// When the running Cue was installed (ms): its program's last change, read as the hub starts (`note_installed`).
static INSTALLED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Reads when the running Cue was installed, once, before the store is in use.
pub(super) fn note_installed() {
    let at = std::env::current_exe().and_then(|p| std::fs::metadata(p)?.modified()).ok();
    let ms = at.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64);
    INSTALLED.store(ms, std::sync::atomic::Ordering::Relaxed);
}

pub(super) fn installed_ms() -> u64 {
    INSTALLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// `project`'s cards, oldest first: one per DONE, with where its item is now (`last`: its latest DONE, the one
/// Looks right and Send back act on while it's ready to check).
pub(super) fn checks_view(st: &Store, project: &str) -> Vec<Value> {
    let mut all: Vec<(&DeskItem, usize)> = st.desk.items.iter().filter(|i| i.project == project).flat_map(|i| (0..i.checks.len()).map(move |n| (i, n))).collect();
    all.sort_by_key(|(i, n)| i.checks[*n].at);
    let skip = all.len().saturating_sub(SHOWN);
    all.into_iter()
        .skip(skip)
        .map(|(i, n)| {
            let c = &i.checks[n];
            json!({ "id": i.id, "kind": i.kind, "title": i.title, "see": i.see, "done_when": i.done_when, "state": i.state, "commit": i.commit, "commit_wait": i.commit_wait, "finish": i.finish, "designs": i.designs,
                "at": c.at, "by": c.by, "files": c.files, "cue": c.cue, "edited": c.edited, "back": c.back, "back_pics": c.back_pics, "last": n + 1 == i.checks.len() })
        })
        .collect()
}
