//! Moving the main checkout's branch to a landed item's commit while other sessions have edits there they haven't
//! committed (#350). `git merge --ff-only` refuses when one of them touched a file the item changed; this lands it
//! anyway and keeps every such edit. Files the item didn't change aren't touched at all. A file both changed gets the
//! item's change merged into the edit (as `git merge-file` merges: the edit stays, unsaved, on top of the new main
//! line). Only a real clash (the same lines changed both ways, a file the item deletes, one staged or binary) waits,
//! and nothing is written then.

use crate::safe_git;

/// One file both the item and an unsaved edit changed: what's on disk now (`ours`) and what it becomes (`merged`).
struct Kept {
    path: String,
    ours: Vec<u8>,
    merged: Vec<u8>,
}

/// Move the branch checked out in `repo` from `main` to `squash` (a fast-forward), keeping the checkout's edits.
/// Err: why it can't yet (git's own words, or which file clashes); nothing was changed then.
/// While an item works on its own branch in `repo` (desk/hub/landing/in_place.rs), the main line isn't checked out
/// there: only its ref moves, compare-and-swap, and no file is touched.
pub(crate) fn fast_forward_keeping_edits(repo: &str, main: &str, squash: &str) -> Result<(), String> {
    if let Some((_, line)) = crate::desk::hub::landing::in_place::on_item_branch(repo) {
        return crate::desk::hub::landing::in_place::move_main(repo, &line, main, squash);
    }
    let filters = safe_git::no_repo_filters("", repo);
    let mut extra = filters.clone();
    extra.push("--literal-pathspecs".into());
    let changed = text(repo, &extra, &["diff", "--name-only", "--no-renames", "-z", main, squash])?;
    let changed: Vec<&str> = changed.split('\0').filter(|p| !p.is_empty()).collect();
    let (kept, in_the_way) = edits_in_the_way(repo, &filters, &extra, main, squash, &changed)?;
    if kept.is_empty() && in_the_way.is_empty() {
        return safe_git::run_said("", repo, &filters, &[], &["merge", "--ff-only", "-q", squash]).map(|_| ());
    }
    // Each file as the main line has it (its edits held here), so git moves it with the rest; then the edits back,
    // merged. A file written meanwhile isn't held: it waits for the next try.
    let path_of = |p: &str| std::path::Path::new(repo).join(p);
    for k in &kept {
        if std::fs::read(path_of(&k.path)).ok().as_deref() != Some(k.ours.as_slice()) {
            return Err(format!("{} changed as Cue was landing it", k.path));
        }
    }
    let put_back = |upto: usize| {
        for k in &kept[..upto] {
            let _ = std::fs::write(path_of(&k.path), &k.ours);
        }
    };
    for (n, k) in kept.iter().enumerate() {
        let Some(base) = blob(repo, &filters, main, &k.path) else {
            put_back(n);
            return Err(format!("Cue can't read {} on the main line", k.path));
        };
        if let Err(e) = std::fs::write(path_of(&k.path), base) {
            put_back(n);
            return Err(format!("Cue can't write {}: {e}", k.path));
        }
    }
    for p in &in_the_way {
        let _ = std::fs::remove_file(path_of(p));
    }
    if let Err(why) = safe_git::run_said("", repo, &filters, &[], &["merge", "--ff-only", "-q", squash]) {
        put_back(kept.len());
        if let Some(t) = in_the_way.iter().find(|p| !path_of(p).exists()) {
            let _ = blob(repo, &filters, squash, t).map(|b| std::fs::write(path_of(t), b));
        }
        return Err(why);
    }
    for k in &kept {
        // Someone wrote it in the moment it was held: theirs stays, as they wrote it.
        if std::fs::read(path_of(&k.path)).ok().as_deref() == blob(repo, &filters, squash, &k.path).as_deref() {
            let _ = std::fs::write(path_of(&k.path), &k.merged);
        }
    }
    Ok(())
}

/// The checkout's edits to files the item changed (`changed`): those kept (their merge with the item's change made
/// now), and new files the same as the item's (in the way, removed for the move). Err: one that clashes.
fn edits_in_the_way(repo: &str, filters: &[String], extra: &[String], main: &str, squash: &str, changed: &[&str]) -> Result<(Vec<Kept>, Vec<String>), String> {
    let mut kept: Vec<Kept> = vec![];
    let mut in_the_way: Vec<String> = vec![];
    if changed.is_empty() {
        return Ok((kept, in_the_way));
    }
    let mut args = vec!["status", "--porcelain=v1", "-z", "--untracked-files=all", "--no-renames", "--"];
    args.extend(changed.iter().copied());
    let status = text(repo, extra, &args)?;
    for entry in status.split('\0').filter(|e| e.len() > 3) {
        let (xy, path) = entry.split_at(3);
        let (x, y) = (xy.as_bytes()[0], xy.as_bytes()[1]);
        let at = std::path::Path::new(repo).join(path);
        let theirs = blob(repo, filters, squash, path);
        if (x, y) == (b'?', b'?') {
            // A new file of yours where the item adds one: fine when it's the same file, else a clash.
            if theirs.as_deref().is_some_and(|t| std::fs::read(&at).ok().as_deref() == Some(t)) {
                in_the_way.push(path.to_string());
                continue;
            }
            return Err(format!("{path} is new in the main checkout, not committed, and the item adds it too"));
        }
        if x != b' ' {
            return Err(format!("{path} has changes staged in the main checkout"));
        }
        if y != b'M' {
            return Err(format!("{path} is changed in the main checkout and not committed"));
        }
        let Some(theirs) = theirs else { return Err(format!("the item deletes {path}, which has unsaved edits in the main checkout")) };
        let ours = std::fs::read(&at).map_err(|e| format!("Cue can't read {path}: {e}"))?;
        let merged = if ours == theirs { theirs } else { merge(repo, filters, main, path, &ours, &theirs)? };
        kept.push(Kept { path: path.to_string(), ours, merged });
    }
    Ok((kept, in_the_way))
}

/// git's output as text, or why it failed.
fn text(repo: &str, extra: &[String], args: &[&str]) -> Result<String, String> {
    match safe_git::run("", repo, extra, args) {
        Ok((out, 0)) => Ok(out),
        Ok(_) => Err(format!("git {} failed in the main checkout", args[0])),
        Err(e) => Err(e),
    }
}

/// File `path` as commit `rev` has it, byte for byte; None when it has none.
fn blob(repo: &str, filters: &[String], rev: &str, path: &str) -> Option<Vec<u8>> {
    match safe_git::run_bytes(repo, filters, &["cat-file", "blob", &format!("{rev}:{path}")]) {
        Ok((b, 0)) => Some(b),
        _ => None,
    }
}

/// The item's change to `path` (main line `main` → `theirs`) merged into your edit of it (`ours`); Err when the two
/// changed the same lines, or the file isn't text.
fn merge(repo: &str, filters: &[String], main: &str, path: &str, ours: &[u8], theirs: &[u8]) -> Result<Vec<u8>, String> {
    let clash = || format!("its work and unsaved edits in the main checkout change the same lines of {path}");
    let base = blob(repo, filters, main, path).ok_or_else(clash)?;
    let dir = super::tree_files::scratch("merge");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (o, b, t) = (dir.join("ours"), dir.join("base"), dir.join("theirs"));
    let wrote = std::fs::write(&o, ours).and(std::fs::write(&b, &base)).and(std::fs::write(&t, theirs));
    let out = wrote.map_err(|e| e.to_string()).and_then(|_| {
        let s = |p: &std::path::Path| p.to_string_lossy().to_string();
        safe_git::run_bytes(repo, filters, &["merge-file", "-p", &s(&o), &s(&b), &s(&t)])
    });
    let _ = std::fs::remove_dir_all(&dir);
    match out {
        Ok((merged, 0)) => Ok(merged),
        _ => Err(clash()),
    }
}
