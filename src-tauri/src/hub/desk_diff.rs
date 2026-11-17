//! A Desk item's Files changed (its page, and Cue's Diff view on it): to check, what Looks right would commit (the
//! same files, the same scratch index, so your own index stays as it is); done, its commit, or what waits to be;
//! still being worked on, what's changed so far.

use super::desk_commit::{by_folder, changed_of, git, staged_diff};
use crate::desk::DeskItem;
use super::*;

impl Hub {
    /// Item `id`'s changes as one unified diff, paths from each folder (with more than one, under that folder's name):
    /// { diff, machine }. Nothing to show (done with no commit, say): an empty diff.
    pub(crate) fn desk_item_diff(&self, id: &str) -> Result<Value, String> {
        let it = self.read(|st| st.desk.item(id).cloned()).ok_or("no such item")?;
        let (machine, diff) = match (it.state.as_str(), &it.commit_wait) {
            // Committed as its holder said DONE (desk_finish.rs): its commit.
            ("to_check" | "finishing", _) if it.finish.is_some() && !it.commit.is_empty() => self.commit_diff(&it),
            ("to_check", _) => {
                let (_, machine, sources) = self.desk_commit_sources(id)?;
                let todo = still_changed(&machine, sources);
                let diff = folders_diff(&todo, |folder, paths, opts| staged_diff(&machine, folder, paths, opts))?;
                (machine, diff)
            }
            ("done", Some(w)) if it.commit.is_empty() => {
                let todo = still_changed(&w.machine, by_folder(&w.files));
                let diff = folders_diff(&todo, |folder, paths, opts| staged_diff(&w.machine, folder, paths, opts))?;
                (w.machine.clone(), diff)
            }
            ("done", _) if !it.commit.is_empty() => self.commit_diff(&it),
            ("done", _) => (String::new(), String::new()),
            // Still being worked on (#149): what's changed in its files so far, as Looks right would read it then.
            // Not in a git repository: no diff, and its page lists the files as they are.
            _ => {
                let (_, machine, sources) = self.desk_change_sources(id)?;
                let todo = still_changed(&machine, sources);
                let diff = folders_diff(&todo, |folder, paths, opts| staged_diff(&machine, folder, paths, opts)).unwrap_or_default();
                (machine, diff)
            }
        };
        Ok(json!({ "diff": diff, "machine": machine }))
    }

    /// A done item's commit as a diff: each commit in the folder it was made in (an item from before Cue kept that:
    /// looked for in its holder's folders).
    fn commit_diff(&self, it: &DeskItem) -> (String, String) {
        let (machine, made) = if it.commit_in.is_empty() {
            let (machine, mut folders) = self.read(|st| {
                let (m, sources) = super::desk::item_sources(st, it);
                let mut f: Vec<String> = sources.into_iter().map(|(f, _)| f).collect();
                f.extend(super::desk_commit::own_log(st, it).map(|o| o.folder));
                f.extend(it.holder.as_ref().and_then(|h| st.sessions.get(&h.sid)).map(|s| s.origin.cwd.clone()));
                (m, f)
            });
            folders.dedup();
            let made: Vec<(String, String)> = it.commit.split_whitespace().filter_map(|sha| {
                folders.iter().find(|f| git(&machine, f, &[], &["cat-file", "-e", &format!("{sha}^{{commit}}")]).is_some()).map(|f| (f.clone(), sha.to_string()))
            }).collect();
            (machine, made)
        } else {
            let machine = self.read(|st| it.holder.as_ref().and_then(|h| st.sessions.get(super::desk::crew_parts(h).map_or(h.sid.as_str(), |(l, _)| l))).map(|s| s.origin.machine.clone())).unwrap_or_default();
            (machine, it.commit_in.clone())
        };
        let many = made.len() > 1;
        let diff = made.iter().filter_map(|(folder, sha)| {
            let pre = prefixes(folder, many);
            let args: Vec<&str> = [&["show", "--format=", "--no-color", "--no-ext-diff", "--no-textconv", "--relative"][..], &[pre[0].as_str(), pre[1].as_str()], &[sha.as_str()]].concat();
            git(&machine, folder, &[], &args)
        }).collect::<Vec<_>>().join("");
        (machine, diff)
    }
}

/// Of each folder's paths, those git says are still changed; folders with none left out.
fn still_changed(machine: &str, sources: Vec<(String, Vec<String>)>) -> Vec<(String, Vec<String>)> {
    sources.into_iter().filter_map(|(folder, paths)| {
        let left = changed_of(machine, &folder, &paths).unwrap_or_default();
        (!left.is_empty()).then_some((folder, left))
    }).collect()
}

/// Each folder's diff (by `read`, given git diff's options), one after another.
fn folders_diff(todo: &[(String, Vec<String>)], read: impl Fn(&str, &[String], &[&str]) -> Result<String, String>) -> Result<String, String> {
    let many = todo.len() > 1;
    let mut out = String::new();
    for (folder, paths) in todo {
        let pre = prefixes(folder, many);
        let d = read(folder, paths, &["--relative", pre[0].as_str(), pre[1].as_str()]).map_err(|e| e.replace("Not committed: ", ""))?;
        out.push_str(&d);
    }
    Ok(out)
}

/// git's a/ and b/ prefixes: with more than one folder, each path under its folder's name.
fn prefixes(folder: &str, many: bool) -> [String; 2] {
    let name = if many { format!("{}/", crate::model::folder_of(folder)) } else { String::new() };
    [format!("--src-prefix=a/{name}"), format!("--dst-prefix=b/{name}")]
}
