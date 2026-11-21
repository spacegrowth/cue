//! An item's copy on disk (split from desk/hub/landing/trees.rs): made from its branch, linked to the shared build folder, its
//! check run on a clean copy, and removed; the leftovers of a crash cleared at start.

use crate::desk::hub::landing::git::{git, git_said};
use crate::desk::hub::landing::main_line::{main_checkout, same_dir};
use crate::desk::hub::landing::trees::TREES;
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, ItemTree};

/// The build folder Cue's copies share, under `TREES`.
pub(crate) const SHARED_TARGET: &str = "desk-target";

/// The main checkout `folder` is, when an item worked from it gets a copy: a repository's main checkout (not one of
/// its worktrees), on a branch with a commit.
pub(crate) fn tree_repo(folder: &str) -> Option<String> {
    let top = git("", folder, &[], &["rev-parse", "--show-toplevel"])?.trim().to_string();
    let (main, onto) = main_checkout(&top)?;
    (same_dir(&top, &main) && !onto.is_empty() && rev(&main, &onto).is_some()).then_some(main)
}

/// Where copy `path`'s branch meets the main line `onto` now, when that's not the commit it was made from (`base`):
/// its branch moved onto a rewritten main line (`base` gone from its history), or main was merged into it or it was
/// rebased onto main (the meeting point is past `base`; measured from `base`, main's own changes since would clash
/// with its copies of them, #358). None while they meet at `base`.
pub(crate) fn moved_base_of(repo: &str, path: &str, onto: &str, base: &str) -> Option<String> {
    let head = rev(path, "HEAD")?;
    let main = rev(repo, onto)?;
    let meet = git("", repo, &[], &["merge-base", &head, &main]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())?;
    (meet != base).then_some(meet)
}

/// The full commit `what` names in `repo`.
pub(crate) fn rev(repo: &str, what: &str) -> Option<String> {
    git("", repo, &[], &["rev-parse", "-q", "--verify", what]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Copy `t` made (or made again) for item `id` from the main line's tip, on its own branch: the commit it starts
/// from, and its folder. A copy left at `t.path` (Cue stopped as it removed it) is removed first only when it holds no
/// work; one with work stays, and this copy goes beside it (`desk-<n>-<id>`).
pub(crate) fn prepare(t: &ItemTree, id: &str) -> Result<(String, String), String> {
    if t.keep {
        return crate::desk::hub::landing::named_copy::prepare_named(t, id);
    }
    let (_, onto) = main_checkout(&t.repo).ok_or("no main checkout")?;
    let base = rev(&t.repo, &onto).ok_or("no main line")?;
    exclude_trees(&t.repo);
    let _ = git("", &t.repo, &[], &["worktree", "prune"]);
    let mut path = t.path.clone();
    if std::path::Path::new(&path).exists() {
        if left_has_work(&t.repo, &path, &base) {
            eprintln!("cue: desk: {path} has work in it: left as it is");
            path = format!("{}-{id}", t.path);
            if std::path::Path::new(&path).exists() && left_has_work(&t.repo, &path, &base) {
                return Err(format!("{} and {path} both have work in them", t.path));
            }
        }
        if std::path::Path::new(&path).exists() {
            remove(&ItemTree { path: path.clone(), branch: String::new(), ..t.clone() });
        }
    }
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Its branch still has work main doesn't (a round that never landed, #448): the copy starts from it, not over it,
    // so that work lands with this round's.
    let base = if crate::desk::hub::landing::unlanded::left_on(&t.repo, &t.branch) > 0 {
        git_said("", &t.repo, &[], &["worktree", "add", "-q", &path, &t.branch]).map_err(|e| crate::desk::hub::landing::git::because("git worktree add failed".into(), &e))?;
        git("", &t.repo, &[], &["merge-base", &t.branch, &base]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or(base)
    } else {
        git_said("", &t.repo, &[], &["worktree", "add", "-q", "-B", &t.branch, &path, &base]).map_err(|e| crate::desk::hub::landing::git::because("git worktree add failed".into(), &e))?;
        base
    };
    if crate::desk::hub::lead::checks::builds_this_cue(&t.repo) {
        link_shared(&t.repo, &path);
    }
    Ok((base, path))
}

/// Whether a copy left at `path` (its item's start unknown) holds work: a change not committed, a commit not on the
/// main line (`main`), or a folder git can't read with anything in it.
pub(crate) fn left_has_work(repo: &str, path: &str, main: &str) -> bool {
    // Not a copy of its own (no `.git` there: git would read the main checkout around it instead).
    let head = std::path::Path::new(path).join(".git").exists().then(|| rev(path, "HEAD")).flatten();
    let Some(head) = head else {
        return std::fs::read_dir(path).map(|mut d| d.next().is_some()).unwrap_or(false);
    };
    let changed = git("", path, &[], &["status", "--porcelain", "--", ".", ":(exclude)node_modules", ":(exclude)src-tauri/target"]);
    let Some(changed) = changed else { return true };
    let on_main = matches!(crate::safe_git::run("", repo, &[], &["merge-base", "--is-ancestor", &head, main]), Ok((_, 0)));
    !changed.trim().is_empty() || !on_main
}

/// Whether item `it` is still finishing in the landing line in its copy at `path`.
pub(crate) fn still_landing(it: Option<&DeskItem>, path: &str) -> bool {
    it.is_some_and(|i| i.state == "finishing" && i.tree.as_ref().is_some_and(|t| t.path == path) && i.finish.as_ref().is_some_and(|f| matches!(f.step.as_str(), "checking" | "line")))
}

/// Whether message `text` is about Desk item `id` ("Desk item #12" is not #1's).
pub(crate) fn names_item(text: &str, id: &str) -> bool {
    let at = format!("Desk item #{id}");
    text.match_indices(&at).any(|(i, _)| !text[i + at.len()..].starts_with(|c: char| c.is_ascii_alphanumeric()))
}

/// Your own check's `steps` run on `commit` of `repo` (main plus the item, the tree Cue's own check reads): in a
/// detached copy of it in Cue's folder, removed after. Err says which step and what it said. No lock is held here.
pub(crate) fn check_steps_on(repo: &str, commit: &str, steps: &[String]) -> Result<(), String> {
    static RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = crate::server::cue_dir().join("checks").join(format!("{}-{run}", std::process::id()));
    let path = dir.to_string_lossy().to_string();
    let _ = std::fs::remove_dir_all(&dir);
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // A check cut off by a crash left its copy registered: forgotten first, so its place can be taken.
    let _ = git("", repo, &[], &["worktree", "prune"]);
    git_said("", repo, &[], &["worktree", "add", "-q", "--detach", &path, commit])
        .map_err(|e| crate::desk::hub::landing::git::because("git couldn't make a copy of main plus the item to check".into(), &e))?;
    if crate::desk::hub::lead::checks::builds_this_cue(repo) {
        link_shared(repo, &path);
    }
    let ran = crate::desk::lands::run_steps(&path, steps).map_err(|e| format!("\"{}\" {}\n{}", e.step, e.why, e.said).trim_end().to_string());
    remove(&ItemTree { repo: repo.to_string(), path, branch: String::new(), ..Default::default() });
    ran
}

/// The copies under `checks` (`<Cue folder>/checks`) that a check cut off by a crash or a quit left behind, removed:
/// each `<pid>-<n>` whose Cue isn't running any more. Only Cue's own scratch copies are ever there; nothing else is
/// touched. Their registration in the repository goes at the next check's `git worktree prune`. Run at start-up.
pub(crate) fn clear_old_checks(checks: &std::path::Path) {
    for e in std::fs::read_dir(checks).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(pid) = name.split_once('-').and_then(|(p, n)| n.parse::<u64>().ok().and(p.parse::<i32>().ok())) else { continue };
        // SAFETY: kill with signal 0 sends nothing: it only asks whether that process is there.
        let running = pid == std::process::id() as i32 || unsafe { libc::kill(pid, 0) } == 0;
        if !running && e.file_type().is_ok_and(|t| t.is_dir()) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// The links a copy of Cue's repository shares: its main checkout's `node_modules`, and one build folder for all
/// its copies.
pub(crate) fn link_shared(repo: &str, path: &str) {
    let modules = std::path::Path::new(repo).join("node_modules");
    let target = std::path::Path::new(repo).join(TREES).join(SHARED_TARGET);
    let _ = std::fs::create_dir_all(&target);
    let links = [(modules, std::path::Path::new(path).join("node_modules")), (target, std::path::Path::new(path).join("src-tauri/target"))];
    for (from, at) in links {
        if from.exists() && std::fs::symlink_metadata(&at).is_err() {
            #[cfg(unix)]
            if let Err(e) = std::os::unix::fs::symlink(&from, &at) {
                eprintln!("cue: desk: couldn't link {}: {e}", at.display());
            }
        }
    }
}

/// What builds left in shared build folder `target` that no build reads again, cleared as an item lands (#377): the
/// object files in each profile's `deps` (`<target>/debug`, `<target>/<triple>/release`, …). A build of Cue no longer
/// leaves them (Cargo.toml, split-debuginfo), but one from before did, a set per rebuild, never removed: 87 GB in a
/// week. A program built before only loses its file and line numbers. A profile folder cargo holds locked (a build
/// under way) is left for the next landing; one cleared is held locked meanwhile, so a build waits for it.
pub(crate) fn clear_build_leftovers(target: &std::path::Path) {
    let mut profiles = Vec::new();
    for e in std::fs::read_dir(target).into_iter().flatten().flatten() {
        profiles.push(e.path());
        profiles.extend(std::fs::read_dir(e.path()).into_iter().flatten().flatten().map(|e| e.path()));
    }
    for dir in profiles.into_iter().filter(|d| d.join("deps").is_dir()) {
        let locks: Vec<std::fs::File> = [".cargo-lock", ".cargo-build-lock"].iter().filter_map(|n| std::fs::File::open(dir.join(n)).ok()).collect();
        if locks.iter().any(|l| std::fs::File::try_lock(l).is_err()) {
            continue;
        }
        for e in std::fs::read_dir(dir.join("deps")).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy().ends_with(".rcgu.o") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// The shared links taken out of copy `path` (only links: what they point at stays).
pub(crate) fn unlink_shared(path: &str) {
    for rel in ["node_modules", "src-tauri/target"] {
        let at = std::path::Path::new(path).join(rel);
        if std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            let _ = std::fs::remove_file(&at);
        }
    }
}

/// Copy `t` and its branch removed (a branch the lead named, `keep`, stays: it's where the work is, #557).
pub(crate) fn remove(t: &ItemTree) {
    unlink_shared(&t.path);
    if git("", &t.repo, &[], &["worktree", "remove", "--force", "--force", &t.path]).is_none() && std::path::Path::new(&t.path).exists() {
        let _ = std::fs::remove_dir_all(&t.path);
    }
    let _ = git("", &t.repo, &[], &["worktree", "prune"]);
    if !t.branch.is_empty() && !t.keep {
        let _ = git("", &t.repo, &[], &["branch", "-q", "-D", &t.branch]);
    }
}

/// Whether copy `t` has work in it: a change not committed, or a commit past where it started.
pub(crate) fn has_work(t: &ItemTree) -> bool {
    let Some(work) = snapshot(&t.path) else { return std::path::Path::new(&t.path).exists() };
    let base = if t.base.is_empty() { None } else { rev(&t.repo, &format!("{}^{{tree}}", t.base)) };
    base.as_deref() != Some(work.as_str())
}

/// The tree of everything in copy `path` (committed or not, new files too, ignored ones and the shared links left
/// out), written with an index of its own so the copy's isn't touched.
pub(crate) fn snapshot(path: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("cue-land-{}-{}", std::process::id(), now_ms()));
    let index = dir.to_string_lossy().to_string();
    let env = [("GIT_INDEX_FILE", index.as_str())];
    let out = (|| {
        git("", path, &env, &["read-tree", "HEAD"])?;
        // Everything, then the shared links put back as the copy's HEAD has them (left out). Not an exclude in the
        // add: git takes a path there as named on purpose, and fails when the repository ignores it or a folder
        // above it.
        git("", path, &env, &["add", "-A", "--", "."])?;
        git("", path, &env, &["reset", "-q", "--", "node_modules", "src-tauri/target"])?;
        git("", path, &env, &["write-tree"]).map(|t| t.trim().to_string())
    })();
    let _ = std::fs::remove_file(&dir);
    out
}

/// `.claude/worktrees/` left out of `repo`'s changes (this checkout's own exclude list), so its copies never show
/// as new files in the main checkout or go into a commit there.
pub(crate) fn exclude_trees(repo: &str) {
    if git("", repo, &[], &["check-ignore", "-q", &format!("{TREES}/x")]).is_some() {
        return;
    }
    let Some(common) = git("", repo, &[], &["rev-parse", "--git-common-dir"]).map(|c| c.trim().to_string()) else { return };
    let common = if std::path::Path::new(&common).is_absolute() { std::path::PathBuf::from(common) } else { std::path::Path::new(repo).join(common) };
    let file = common.join("info/exclude");
    let _ = std::fs::create_dir_all(common.join("info"));
    let mut text = std::fs::read_to_string(&file).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("/{TREES}/\n"));
    let _ = std::fs::write(&file, text);
}

#[cfg(test)]
mod tests {
    use super::names_item;

    /// A check cut off by a crash leaves its copy in `<Cue folder>/checks`: cleared at start-up, while a running Cue's
    /// copy and anything not named as a check's copy stay.
    #[test]
    fn desk_checks_left_by_a_crash_are_cleared_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("cue-test-checks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let gone = dir.join("999999-0");
        let mine = dir.join(format!("{}-3", std::process::id()));
        let other = dir.join("notes");
        for d in [&gone, &mine, &other] {
            std::fs::create_dir_all(d.join("src")).unwrap();
            std::fs::write(d.join("src/a.txt"), "1\n").unwrap();
        }
        super::clear_old_checks(&dir);
        assert!(!gone.exists(), "the crashed Cue's copy is cleared");
        assert!(mine.join("src/a.txt").exists(), "a running Cue's copy stays");
        assert!(other.join("src/a.txt").exists(), "nothing else is touched");
        super::clear_old_checks(&dir.join("missing"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A build under way holds its profile folder locked (cargo's `.cargo-lock`): nothing in it is cleared then (#377).
    #[test]
    fn a_build_folder_a_build_holds_locked_is_left_as_it_is() {
        let dir = std::env::temp_dir().join(format!("cue-test-leftovers-{}", std::process::id()));
        let deps = dir.join("debug/deps");
        std::fs::create_dir_all(&deps).unwrap();
        let left = deps.join("cue_lib-0123456789abcdef.0a1b2c.rcgu.o");
        std::fs::write(&left, "1\n").unwrap();
        let held = std::fs::File::create(dir.join("debug/.cargo-lock")).unwrap();
        held.lock().unwrap();
        super::clear_build_leftovers(&dir);
        assert!(left.exists(), "left while a build holds it");
        held.unlock().unwrap();
        super::clear_build_leftovers(&dir);
        assert!(!left.exists(), "cleared once it's free");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_items_message_is_told_by_its_whole_number() {
        assert!(names_item("Desk item #1: Header jumps", "1"));
        assert!(names_item("Desk item #1 (bug): Header jumps", "1"));
        assert!(!names_item("Desk item #12: Footer", "1"));
        assert!(!names_item("See #1 too", "1"));
        assert!(names_item("Desk item #12: x\nDesk item #1: y", "1"));
    }
}

/// Why folder `t.path`, on item `id`'s record, isn't that item's to remove (#558): another item's copy is there now,
/// or the copy there is on another branch. None when it's the item's own (or gone).
pub(crate) fn not_its_own(items: &[DeskItem], id: &str, t: &ItemTree) -> Option<String> {
    if let Some(o) = items.iter().find(|i| i.id != id && i.tree.as_ref().is_some_and(|o| same_dir(&o.path, &t.path))) {
        return Some(format!("#{}'s copy is there now", o.id));
    }
    // Only a copy of its own says its branch (no `.git` there: git would read the main checkout around it).
    if t.branch.is_empty() || !std::path::Path::new(&t.path).join(".git").exists() {
        return None;
    }
    let on = git("", &t.path, &[], &["symbolic-ref", "--short", "-q", "HEAD"]).map(|b| b.trim().to_string()).unwrap_or_default();
    (!on.is_empty() && on != t.branch).then(|| format!("the copy there is on {on}, not {}", t.branch))
}
