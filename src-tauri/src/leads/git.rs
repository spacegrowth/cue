//! What git says about an executor's worktree: whether its packet's files are still staged, or were
//! committed after its report.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A question to git about a worktree: its exit code and output, None when git can't be run there. Each question is
/// asked at most every few seconds (the crew is rescanned often).
fn ask_git(worktree: &str, args: &[&str]) -> Option<(i32, String)> {
    static GIT_ASKED: Mutex<Option<HashMap<String, (std::time::Instant, Option<(i32, String)>)>>> = Mutex::new(None);
    let key = format!("{worktree}\0{}", args.join("\0"));
    let known = crate::locking::lock(&GIT_ASKED, "GIT_ASKED").as_ref().and_then(|c| c.get(&key).cloned());
    if let Some((_, said)) = known.filter(|(at, _)| at.elapsed() < std::time::Duration::from_secs(5)) {
        return said;
    }
    let out = std::process::Command::new("git").arg("-C").arg(worktree).args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok();
    let said = out.and_then(|o| Some((o.status.code()?, String::from_utf8_lossy(&o.stdout).trim().to_string())));
    crate::locking::lock(&GIT_ASKED, "GIT_ASKED").get_or_insert_with(HashMap::new).insert(key, (std::time::Instant::now(), said.clone()));
    said
}

/// Whether any of `files` is still staged in a worktree (what an executor left for the lead to commit), or
/// anything at all when the files aren't known. A commit or a discard empties it. A folder git can't read, or
/// one that's gone, counts as nothing staged.
pub(super) fn has_staged(worktree: &str, files: &[String]) -> bool {
    let mut args = vec!["diff", "--cached", "--quiet", "--"];
    args.extend(files.iter().map(String::as_str));
    !worktree.is_empty() && ask_git(worktree, &args).is_some_and(|(code, _)| code == 1)
}

/// Whether a commit made after `since` (epoch seconds: the packet's report) touched any of `files`: its lead
/// committed the work, by hand or through relay. Many sessions can share one checkout, so what is staged
/// there now says nothing about this packet; a commit of its own files after its report does.
pub(super) fn committed_since(worktree: &str, since: u64, files: &[String]) -> bool {
    if worktree.is_empty() || files.is_empty() {
        return false;
    }
    let mut args = vec!["log", "-1", "--format=%ct", "--"];
    args.extend(files.iter().map(String::as_str));
    ask_git(worktree, &args).and_then(|(code, out)| (code == 0).then(|| out.parse::<u64>().ok()).flatten()).is_some_and(|t| t > since)
}

/// The files a packet staged, from the diff page relay made of it (`NNN-diff.html` holds the diff's text).
/// Empty when there's no page. Read again only when the page changes.
pub(super) fn packet_files(dir: &Path, n: u64) -> Vec<String> {
    static PACKET_FILES: Mutex<Option<HashMap<PathBuf, (std::time::SystemTime, Vec<String>)>>> = Mutex::new(None);
    let p = dir.join(format!("packets/{n:03}-diff.html"));
    let Ok(mtime) = std::fs::metadata(&p).and_then(|m| m.modified()) else { return vec![] };
    let known = crate::locking::lock(&PACKET_FILES, "PACKET_FILES").as_ref().and_then(|c| c.get(&p).cloned());
    if let Some((_, files)) = known.filter(|(at, _)| *at == mtime) {
        return files;
    }
    let page = std::fs::read_to_string(&p).unwrap_or_default();
    let mut files: Vec<String> = page.split("diff --git a/").skip(1).filter_map(|rest| rest.split_once(" b/").map(|(a, _)| a.to_string())).filter(|f| !f.is_empty() && !f.contains(['\\', '"', '<'])).collect();
    files.sort();
    files.dedup();
    crate::locking::lock(&PACKET_FILES, "PACKET_FILES").get_or_insert_with(HashMap::new).insert(p, (mtime, files.clone()));
    files
}

/// When a packet's report was written (epoch seconds), 0 when there's none.
pub(super) fn written_at(p: &Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}
