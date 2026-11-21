//! Holds checked against what copies really changed (#540, docs/desk-coordination-plan.md, option B step 4). Cue
//! knows the files each open item's copy has changed (the diff of the copy, committed or not, against the main line
//! where it started), and uses that fact twice: as a hold is taken, the open items on other Desks on the repository
//! that already changed a held file are listed to both leads; and at landing, an item from another Desk whose diff
//! touches a held file waits in the landing line while the hold stands ("Waits for cue #509's hold on …"). It lands by
//! itself once the hold lifts, brought up to the new main line first as every landing is (desk/hub/landing/trees.rs).
//! A hold's area is globs relative to the repository's root: `*` within a folder, `**` across folders, `?` one
//! character, and a folder (`ui/desk/`, or a name with no wildcard) holds everything under it.

use crate::desk::holds::{holds_on, repo_key};
use crate::desk::hub::landing::git::git;
use crate::desk::hub::landing::main_line::main_checkout;
use crate::desk::hub::landing::tree_files::{moved_base_of, rev, snapshot};
use crate::desk::{DeskItem, ItemTree};
use crate::hub::Store;

/// The files open items' copies changed: (item, files), as worked out with the store let go (`copies_changed`).
pub(crate) type Changes = Vec<(String, Vec<String>)>;

/// Whether path `path` (relative to the repository's root) is in hold area entry `glob`.
pub(crate) fn matches(glob: &str, path: &str) -> bool {
    let g = clean(glob);
    let p = clean(path);
    if g.is_empty() || p.is_empty() {
        return false;
    }
    if let Some(folder) = g.strip_suffix('/') {
        return under(folder, p) || (has_wild(folder) && glob_match(folder.as_bytes(), first_dirs(p, folder)));
    }
    if !has_wild(g) {
        return p == g || under(g, p);
    }
    glob_match(g.as_bytes(), p.as_bytes())
}

/// Whether `path` is in any entry of `area`.
pub(crate) fn in_area(area: &[String], path: &str) -> bool {
    area.iter().any(|g| matches(g, path))
}

fn clean(s: &str) -> &str {
    let s = s.trim().trim_matches('`');
    let s = s.strip_prefix("./").unwrap_or(s);
    s.trim_matches('`').trim_start_matches('/')
}

fn has_wild(s: &str) -> bool {
    s.contains(['*', '?'])
}

fn under(folder: &str, path: &str) -> bool {
    path.strip_prefix(folder).is_some_and(|r| r.starts_with('/'))
}

/// A folder glob's match is checked against `path`'s leading folders, as many as the glob has.
fn first_dirs<'a>(path: &'a str, folder: &str) -> &'a [u8] {
    if folder.contains("**") {
        return path.rsplit_once('/').map_or(&[][..], |(d, _)| d.as_bytes());
    }
    let n = folder.matches('/').count() + 1;
    let mut cut = path.len();
    for (seen, (i, _)) in path.match_indices('/').enumerate() {
        if seen + 1 == n {
            cut = i;
            break;
        }
    }
    if cut == path.len() { &[] } else { &path.as_bytes()[..cut] }
}

/// `*` any run within one folder, `**` any run across folders (`**/` also none), `?` one character but `/`.
fn glob_match(g: &[u8], p: &[u8]) -> bool {
    match g.first() {
        None => p.is_empty(),
        Some(b'*') if g.get(1) == Some(&b'*') => {
            let rest = &g[2..];
            if let Some(after) = rest.strip_prefix(b"/") {
                if glob_match(after, p) {
                    return true;
                }
            }
            (0..=p.len()).any(|i| glob_match(rest, &p[i..]))
        }
        Some(b'*') => {
            let rest = &g[1..];
            for i in 0..=p.len() {
                if glob_match(rest, &p[i..]) {
                    return true;
                }
                if p.get(i) == Some(&b'/') {
                    break;
                }
            }
            false
        }
        Some(b'?') => p.first().is_some_and(|c| *c != b'/') && glob_match(&g[1..], &p[1..]),
        Some(c) => p.first() == Some(c) && glob_match(&g[1..], &p[1..]),
    }
}

/// The files that differ between commits or trees `from` and `to` of `repo`, relative to its root.
pub(crate) fn changed(repo: &str, from: &str, to: &str) -> Vec<String> {
    git("", repo, &[], &["diff", "--name-only", "--no-renames", "-z", from, to])
        .map(|out| out.split('\0').filter(|f| !f.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

/// The files copy `t` has changed (committed there or not) against where it meets the main line: none for a copy
/// on a branch the lead named, which never lands on main.
pub(crate) fn copy_changed(t: &ItemTree) -> Vec<String> {
    if t.keep {
        return Vec::new();
    }
    let Some((_, onto)) = main_checkout(&t.repo).filter(|(_, o)| !o.is_empty()) else { return Vec::new() };
    let base = if t.base.is_empty() { rev(&t.repo, &onto) } else { Some(t.base.clone()) };
    let (Some(base), Some(work)) = (base, snapshot(&t.path)) else { return Vec::new() };
    let base = moved_base_of(&t.repo, &t.path, &onto, &base).unwrap_or(base);
    changed(&t.repo, &base, &work)
}

/// The copies of open items (`copies` as read from the store: item, copy), each with the files it changed. Run with
/// the store let go: it reads every copy on disk.
pub(crate) fn copies_changed(copies: Vec<(String, ItemTree)>) -> Changes {
    copies.into_iter().map(|(id, t)| (id, copy_changed(&t))).filter(|(_, f)| !f.is_empty()).collect()
}

/// The open items with a copy, to work out their changes for a hold (`copies_changed`).
pub(crate) fn open_copies(st: &Store) -> Vec<(String, ItemTree)> {
    st.desk.items.iter().filter(|i| open(i)).filter_map(|i| i.tree.clone().filter(|t| !t.keep).map(|t| (i.id.clone(), t))).collect()
}

fn open(i: &DeskItem) -> bool {
    i.state != "done" && i.state != "held"
}

/// One clash: item `id` of Desk `desk` already changed `files`, held.
pub(crate) struct Clash {
    pub desk: String,
    pub id: String,
    pub files: Vec<String>,
}

/// The open items on Desks other than `desk`, on its repository, whose copy already changed a file in `area`.
pub(crate) fn clashes(st: &Store, desk: &str, area: &[String], changes: &Changes) -> Vec<Clash> {
    let key = repo_key(st, desk);
    let mut out: Vec<Clash> = changes
        .iter()
        .filter_map(|(id, files)| {
            let it = st.desk.item(id).filter(|i| open(i) && i.project != desk && repo_key(st, &i.project) == key)?;
            let files: Vec<String> = files.iter().filter(|f| in_area(area, f)).cloned().collect();
            (!files.is_empty()).then(|| Clash { desk: it.project.clone(), id: id.clone(), files })
        })
        .collect();
    out.sort_by(|a, b| (st.desk.name_of(&a.desk).to_lowercase(), &a.id).cmp(&(st.desk.name_of(&b.desk).to_lowercase(), &b.id)));
    out
}

/// The clashes in words, for the holder's lead: "1 item there already changed a held file: loops #533
/// ui/desk/thread.js." or "No item on another Desk has changed a held file yet."
pub(crate) fn clash_words(st: &Store, list: &[Clash]) -> String {
    if list.is_empty() {
        return "No item on another Desk has changed a held file yet.".into();
    }
    let each: Vec<String> = list.iter().map(|c| format!("{} #{} {}", st.desk.name_of(&c.desk), c.id, files_words(&c.files))).collect();
    let (n, file) = if list.len() == 1 { ("1 item there".to_string(), if list[0].files.len() == 1 { "a held file" } else { "held files" }) } else { (format!("{} items there", list.len()), "held files") };
    format!("{n} already changed {file}: {}.", each.join("; "))
}

/// For the lead of Desk `desk`, its own items in `list`: " Yours already changed a held file: #533 ui/desk/thread.js."
/// (nothing when none of its did).
pub(crate) fn own_clash_words(desk: &str, list: &[Clash]) -> String {
    let mine: Vec<String> = list.iter().filter(|c| c.desk == desk).map(|c| format!("#{} {}", c.id, files_words(&c.files))).collect();
    if mine.is_empty() {
        return String::new();
    }
    format!(" Yours already changed a held file: {}.", mine.join("; "))
}

fn files_words(files: &[String]) -> String {
    const SHOWN: usize = 3;
    if files.len() <= SHOWN {
        return files.join(", ");
    }
    format!("{} and {} more", files[..SHOWN].join(", "), files.len() - SHOWN)
}

/// Whether item `it`, whose diff changes `files`, waits at landing for a hold: the plain line it waits with ("Waits for
/// cue #509's hold on ui/desk/thread.js"), when a hold another Desk took on its repository covers one of them. Its own
/// Desk's holds never stop it.
pub(crate) fn landing_wait(st: &Store, it: &DeskItem, files: &[String]) -> Option<String> {
    holds_on(st, &it.project).into_iter().filter(|h| h.desk != it.project && h.item != it.id).find_map(|h| {
        let held: Vec<String> = files.iter().filter(|f| in_area(&h.area, f)).cloned().collect();
        (!held.is_empty()).then(|| format!("{HOLD_WAIT}{} #{}'s hold on {}", st.desk.name_of(&h.desk), h.item, files_words(&held)))
    })
}

/// How a wait for a hold starts: the landing line retries such an item as soon as another lands (a hold may have
/// ended with it), not only on the next tick.
pub(crate) const HOLD_WAIT: &str = "Waits for ";

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn globs_folders_and_files_match_as_a_lead_writes_them() {
        assert!(matches("src-tauri/src/hub/desk_*.rs", "src-tauri/src/hub/desk_view.rs"));
        assert!(!matches("src-tauri/src/hub/desk_*.rs", "src-tauri/src/hub/desk/view.rs"), "* stays in its folder");
        assert!(!matches("src-tauri/src/hub/desk_*.rs", "src-tauri/src/hub/other.rs"));
        assert!(matches("ui/desk_*.js", "ui/desk_list.js") && !matches("ui/desk_*.js", "ui/desk/list.js"));
        assert!(matches("ui/desk/", "ui/desk/thread.js") && matches("ui/desk/", "ui/desk/a/b.js"));
        assert!(!matches("ui/desk/", "ui/desk.js") && !matches("ui/desk/", "ui/desktop/x.js"));
        assert!(matches("ui/desk", "ui/desk/thread.js"), "a plain name is a folder too");
        assert!(matches("ui/core.js", "ui/core.js") && !matches("ui/core.js", "ui/core.jsx"));
        assert!(matches("src/**/*.rs", "src/a/b/c.rs") && matches("src/**/*.rs", "src/c.rs"));
        assert!(matches("**/thread.js", "ui/desk/thread.js") && matches("src/**", "src/x/y"));
        assert!(matches("ui/desk/?.js", "ui/desk/a.js") && !matches("ui/desk/?.js", "ui/desk/ab.js"));
        assert!(matches("./`ui/desk/`", "ui/desk/x.js") && matches("/ui/core.js", "ui/core.js"));
        assert!(matches("ui/desk_*/", "ui/desk_one/x.js") && !matches("ui/desk_*/", "ui/desk_one.js"));
        assert!(matches("src/**/", "src/a/b.rs"));
        assert!(!matches("", "a") && !matches("*", "a/b") && matches("*", "a.txt"));
    }
}
