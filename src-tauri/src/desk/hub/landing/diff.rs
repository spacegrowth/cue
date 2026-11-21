//! A Desk item's Files changed (its page, and Cue's Diff view on it): to check, what Looks right would commit (the
//! same files, the same scratch index, so your own index stays as it is); done, its commit, or what waits to be;
//! still being worked on, what's changed so far.

use crate::desk::hub::landing::commit::{by_folder, changed_of, staged_diff};
use crate::desk::hub::landing::git::git;
use crate::desk::DeskItem;
use crate::desk::hub::prelude::*;

impl Hub {
    /// Item `id`'s changes as one unified diff, paths from each folder (with more than one, under that folder's name):
    /// { diff, machine }. Nothing to show (done with no commit, say): an empty diff.
    pub(crate) fn desk_item_diff(&self, id: &str) -> Result<Value, String> {
        // A dropped item's kept copy, or one picked up again whose copy hasn't moved back yet (#608): what's in it.
        let kept = self.read(|st| match st.desk.item(id) {
            Some(i) => i.kept_copy.clone().filter(|_| i.tree.is_none()),
            None => st.desk.kept.iter().find(|k| k.id == id).cloned(),
        });
        if let Some(k) = kept {
            return Ok(json!({ "diff": crate::desk::hub::landing::tree_files::kept_diff(&k), "machine": "" }));
        }
        let it = self.read(|st| st.desk.item(id).cloned()).ok_or("no such item")?;
        let (machine, diff) = match (it.state.as_str(), &it.commit_wait) {
            // Committed as its holder said DONE (desk/hub/landing/finish.rs): its commit.
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
            // On another machine (desk/hub/landing/remote_item.rs): its branch there, committed or not, from where it
            // left the main line; once fetched here, what's fetched.
            ("working" | "finishing", _) if it.remote.is_some() => {
                let r = it.remote.as_ref().expect("matched");
                let base = it.tree.as_ref().map(|t| t.base.clone()).filter(|b| !b.is_empty());
                let fetched = it.tree.as_ref().and_then(|t| base.as_ref().map(|b| (t.repo.clone(), b.clone())));
                let diff = match fetched {
                    Some((repo, base)) => git("", &repo, &[], &["diff", "--no-color", "--no-ext-diff", "--no-textconv", &base, &format!("refs/cue/remote/{}", it.id)]),
                    None => git(&r.machine, &r.folder, &[], &["diff", "--no-color", "--no-ext-diff", "--no-textconv", "--merge-base", &format!("{}/{}", r.remote, r.onto)]),
                };
                let machine = if it.tree.is_some() { String::new() } else { r.machine.clone() };
                (machine, diff.unwrap_or_default())
            }
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
                let (m, sources) = crate::desk::hub::core::desk::item_sources(st, it);
                let mut f: Vec<String> = sources.into_iter().map(|(f, _)| f).collect();
                f.extend(crate::desk::hub::landing::commit::own_log(st, it).map(|o| o.folder));
                f.extend(it.holder.as_ref().and_then(|h| st.sessions.get(&h.sid)).map(|s| s.origin.cwd.clone()));
                (m, f)
            });
            folders.dedup();
            let made: Vec<(String, String)> = it.commit.split_whitespace().filter_map(|sha| {
                folders.iter().find(|f| git(&machine, f, &[], &["cat-file", "-e", &format!("{sha}^{{commit}}")]).is_some()).map(|f| (f.clone(), sha.to_string()))
            }).collect();
            (machine, made)
        } else {
            // Landed from another machine: its commit is here, on the main line.
            let machine = if it.remote.is_some() { Some(String::new()) } else { None };
            let machine = machine.or_else(|| self.read(|st| it.holder.as_ref().and_then(|h| st.sessions.get(crate::desk::hub::core::desk::crew_parts(h).map_or(h.sid.as_str(), |(l, _)| l))).map(|s| s.origin.machine.clone()))).unwrap_or_default();
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
