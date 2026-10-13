//! A session's log on a machine (Claude Code's transcript, Codex's rollout log), copied to this Mac as it's
//! written: one `tail -F` over the machine's shared SSH connection, its output straight into a file in Cue's
//! data folder. The session then points at the copy, so everything Cue reads from a log on this Mac (the
//! chat's steps, context, who sent a prompt, an answer given in the terminal) works the same for it. New
//! lines arrive within tens of milliseconds. The hooks still say when things happen and ask permission.
//!
//! Each copy keeps the machine's path beside it (`<copy>.from`), so when Cue launches it picks the copies of
//! the sessions it knows up again, and drops copies nothing has written to in a week.
use crate::machines::{self, Machine};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{Mutex, OnceLock};

/// What each copy runs on the machine, after this mark ("cue-copy" tells these ssh processes apart).
const MARK: &str = "# cue-copy";

fn copies() -> &'static Mutex<HashMap<String, Child>> {
    static C: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Where the copy of `remote` (a path on machine `m`) lives on this Mac. Its file name is kept, since a
/// Codex log is known by it.
fn local_path(m: &Machine, remote: &str) -> Option<PathBuf> {
    let name = remote.rsplit('/').next().filter(|n| !n.is_empty() && *n != "." && *n != "..")?;
    Some(crate::server::cue_dir().join("machines").join(dir_name(m)).join(name))
}

/// The folder a machine's copies go in: its name, with anything odd in it as `_`.
fn dir_name(m: &Machine) -> String {
    m.name.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect()
}

/// The copy of `remote` on this Mac, started if it isn't running (or stopped since). Empty if there's
/// nothing to copy: then the session keeps the machine's path, as before.
pub fn local_copy(m: &Machine, remote: &str) -> String {
    if !remote.starts_with('/') {
        return String::new();
    }
    let Some(path) = local_path(m, remote) else { return String::new() };
    let key = path.to_string_lossy().to_string();
    let mut c = crate::locking::lock(copies(), "copies");
    let running = c.get_mut(&key).is_some_and(|ch| matches!(ch.try_wait(), Ok(None)));
    if !running && start(m, remote, &path).map(|ch| c.insert(key.clone(), ch)).is_err() {
        return String::new();
    }
    key
}

/// A background agent's log (Claude Code writes it to `<session log, minus .jsonl>/subagents/agent-<id>.jsonl`,
/// beside its meta file): for a session whose log is a copy (`copy`), copied the same way to the same place
/// beside the copy, where the chat reads it. Its meta file (the agent's type) comes over once. Nothing to do
/// for a log on this Mac, or one already being copied. Starts the copy and returns; never waits on the machine.
pub fn copy_helper(copy: &str, agent_id: &str) {
    let Some((m, remote, local)) = helper_paths(copy, agent_id) else { return };
    let key = local.to_string_lossy().to_string();
    let mut c = crate::locking::lock(copies(), "copies");
    if c.get_mut(&key).is_some_and(|ch| matches!(ch.try_wait(), Ok(None))) {
        return;
    }
    if let Ok(ch) = start(&m, &remote, &local) {
        c.insert(key, ch);
    }
    drop(c);
    let Some(meta_remote) = remote.strip_suffix(".jsonl").map(|r| format!("{r}.meta.json")) else { return };
    let meta_local = local.with_extension("meta.json");
    std::thread::spawn(move || {
        let Ok(out) = std::fs::File::create(&meta_local) else { return };
        let _ = machines::stream_command(&m.host, &format!("{MARK}\nexec cat {}", machines::shq(&meta_remote))).stdout(out).status();
    });
}

/// Stop copying a background agent's log (it reported back: its log is complete).
pub fn stop_helper(copy: &str, agent_id: &str) {
    let Some((_, _, local)) = helper_paths(copy, agent_id) else { return };
    end(&local);
}

/// For a background agent of the session whose copy is `copy`: its machine, its log there, and where it's
/// copied to. None when `copy` isn't a copy (no note of the machine's path beside it) or the id is odd.
fn helper_paths(copy: &str, agent_id: &str) -> Option<(Machine, String, PathBuf)> {
    if agent_id.is_empty() || !agent_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    let copy = Path::new(copy);
    let remote = std::fs::read_to_string(from_file(copy)).ok()?;
    let dir = copy.parent()?.file_name()?.to_str()?;
    let m = machines::list().into_iter().find(|m| dir_name(m) == dir)?;
    let (remote, local) = helper_of(remote.trim(), copy, agent_id)?;
    Some((m, remote, local))
}

/// The agent's log on the machine and its place beside the copy: `<log minus .jsonl>/subagents/agent-<id>.jsonl`.
fn helper_of(remote: &str, copy: &Path, agent_id: &str) -> Option<(String, PathBuf)> {
    let file = format!("agent-{agent_id}.jsonl");
    let remote = format!("{}/subagents/{file}", remote.strip_suffix(".jsonl")?);
    let local = PathBuf::from(copy.to_str()?.strip_suffix(".jsonl")?).join("subagents").join(file);
    Some((remote, local))
}

/// End the copy writing to `local`, if one is.
fn end(local: &Path) {
    let gone = {
        let mut c = crate::locking::lock(copies(), "copies");
        c.remove(&*local.to_string_lossy())
    };
    if let Some(mut ch) = gone {
        let _ = ch.kill();
        let _ = ch.wait();
    }
}

/// Where a copy notes the machine's path it copies.
fn from_file(copy: &Path) -> PathBuf {
    PathBuf::from(format!("{}.from", copy.display()))
}

/// The whole log, then each line as it's written, into `path` (started over each time).
fn start(m: &Machine, remote: &str, path: &PathBuf) -> Result<Child, String> {
    std::fs::create_dir_all(path.parent().ok_or("no folder")?).map_err(|e| e.to_string())?;
    let out = std::fs::File::create(path).map_err(|e| e.to_string())?;
    std::fs::write(from_file(path), remote).map_err(|e| e.to_string())?;
    let script = format!("{MARK}\nexec tail -c +1 -F {}", machines::shq(remote));
    machines::stream_command(&m.host, &script).stdout(out).spawn().map_err(|e| e.to_string())
}

/// Stop copying `remote` (its session ended). The copy stays until Cue forgets the session.
pub fn stop(m: &Machine, remote: &str) {
    let Some(path) = local_path(m, remote) else { return };
    end(&path);
    // Its background agents' copies too.
    let Some(agents) = path.to_str().and_then(|p| p.strip_suffix(".jsonl")).map(|p| format!("{p}/subagents/")) else { return };
    let theirs: Vec<String> = {
        let c = crate::locking::lock(copies(), "copies");
        c.keys().filter(|k| k.starts_with(&agents)).cloned().collect()
    };
    for k in theirs {
        end(Path::new(&k));
    }
}

/// At launch: copies an earlier Cue left running write files this one doesn't track. End them.
pub fn stop_strays() {
    machines::end_streams(MARK);
}

/// At launch, for the sessions Cue knows ((machine, copy) pairs): copy their logs again, so their chats
/// are current before they next do anything. Then drop copies nobody knows that haven't changed in a week.
pub fn resume(known: &[(String, String)]) {
    let mut keep = HashSet::new();
    for (name, copy) in known {
        let Some(m) = machines::get(name) else { continue };
        let Ok(remote) = std::fs::read_to_string(from_file(Path::new(copy))) else { continue };
        if !local_copy(&m, remote.trim()).is_empty() {
            keep.insert(PathBuf::from(copy));
        }
    }
    clean(&crate::server::cue_dir().join("machines"), &keep, std::time::Duration::from_secs(7 * 24 * 3600));
}

/// Remove the copies under `dir` (and their notes) that aren't in `keep` and are older than `age`.
fn clean(dir: &Path, keep: &HashSet<PathBuf>, age: std::time::Duration) {
    let old = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|e| e > age);
    for machine in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        for f in std::fs::read_dir(machine.path()).into_iter().flatten().flatten() {
            let p = f.path();
            // A session's folder (its background agents' copies): goes with its session's copy.
            if p.is_dir() {
                if !keep.contains(&p.with_extension("jsonl")) && old(&p) {
                    let _ = std::fs::remove_dir_all(&p);
                }
                continue;
            }
            if p.extension().is_some_and(|e| e == "from") || keep.contains(&p) || !old(&p) {
                continue;
            }
            let _ = std::fs::remove_file(from_file(&p));
            let _ = std::fs::remove_file(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_keeps_the_log_name_under_the_machine_and_refuses_odd_paths() {
        let m = Machine { name: "my box".into(), host: "u@h".into(), label: String::new() };
        let p = local_path(&m, "/home/u/.codex/sessions/2026/rollout-1-abc.jsonl").unwrap();
        assert!(p.ends_with("machines/my_box/rollout-1-abc.jsonl"), "{p:?}");
        assert!(local_path(&m, "/home/u/").is_none());
        assert!(local_path(&m, "/home/u/..").is_none());
        assert_eq!(local_copy(&m, "relative.jsonl"), "");
    }

    #[test]
    fn a_background_agents_log_is_copied_beside_its_sessions_copy_as_on_the_machine() {
        let copy = Path::new("/cue/machines/box/abc-123.jsonl");
        let (remote, local) = helper_of("/home/u/.claude/projects/-x/abc-123.jsonl", copy, "a0f65").unwrap();
        assert_eq!(remote, "/home/u/.claude/projects/-x/abc-123/subagents/agent-a0f65.jsonl");
        assert_eq!(local, PathBuf::from("/cue/machines/box/abc-123/subagents/agent-a0f65.jsonl"));
        // Where the chat reads it: the same place it reads a session's agent on this Mac.
        assert_eq!(Some(local), crate::logs::steps::helper_path_for_test(copy.to_str().unwrap(), "a0f65"));
        assert!(helper_paths("/nowhere/abc.jsonl", "a0f65").is_none(), "a log on this Mac (no note of a machine's path): nothing to copy");
        assert!(helper_paths("/nowhere/abc.jsonl", "../x").is_none());
    }

    #[test]
    fn old_copies_nobody_knows_go_and_the_rest_stay() {
        let dir = std::env::temp_dir().join(format!("cue-logs-{}", std::process::id()));
        let box_ = dir.join("box");
        std::fs::create_dir_all(&box_).unwrap();
        let (old, known, fresh) = (box_.join("old.jsonl"), box_.join("known.jsonl"), box_.join("fresh.jsonl"));
        for p in [&old, &known, &fresh] {
            std::fs::write(p, "{}\n").unwrap();
            std::fs::write(from_file(p), "/home/u/x.jsonl").unwrap();
        }
        // A session's background agents' copies: a folder beside its copy, going with it.
        let (old_dir, known_dir) = (box_.join("old"), box_.join("known"));
        for d in [&old_dir, &known_dir] {
            std::fs::create_dir_all(d.join("subagents")).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&fresh, "{}\n{}\n").unwrap();
        clean(&dir, &HashSet::from([known.clone()]), std::time::Duration::from_secs(1));
        assert!(!old.exists() && !from_file(&old).exists(), "an old copy nobody knows goes, with its note");
        assert!(known.exists(), "a copy a known session reads stays");
        assert!(fresh.exists() && from_file(&fresh).exists(), "a copy written to lately stays");
        assert!(!old_dir.exists(), "an old copy's agents go with it");
        assert!(known_dir.exists(), "a known session's agents stay");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
