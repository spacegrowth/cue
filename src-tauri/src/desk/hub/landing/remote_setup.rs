//! A Desk item's new session on a machine whose folder for the project isn't there yet (#664): Cue downloads the
//! project into that folder first, from its online home (the remote this Mac's main line is pushed to), then starts
//! the session. You said yes to the machine first: you picked it (the Desk's settings, the item's page), or the lead
//! asked you before the Desk's first item there (remote_item.rs `route_on`). If it can't, the item says why.

use crate::desk::hub::landing::main_line::main_checkout;
use crate::desk::hub::landing::remote_item::remote_of;

/// How long a download may take before Cue gives up on it.
const LIMIT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// What the machine said about the folder, as the script below prints it on its first line.
#[derive(Debug, PartialEq)]
enum There {
    /// A clone of a repository already: nothing to do.
    Repo,
    /// Downloaded just now.
    Cloned,
    /// Something else is in it.
    NotRepo,
    /// No git on the machine.
    NoGit,
    /// The download failed: git's words.
    Failed(String),
}

/// The script run there: `folder` a clone already, else downloaded from `url` into it (made, or empty). Never asks for
/// a password or a host's key: a download that needs one fails, said on the item. Always ends 0 and says what happened
/// on its first line (the machine's environment file may hide what git writes to stderr).
fn script(folder: &str, url: &str) -> String {
    let d = crate::machines::remote_dir(folder);
    let url = crate::safe_git::sh_quote(url);
    format!(
        r#"d={d}
if [ -d "$d" ] && git -C "$d" rev-parse --git-dir >/dev/null 2>&1; then echo repo; exit 0; fi
if [ -d "$d" ] && [ -n "$(ls -A "$d" 2>/dev/null)" ]; then echo notrepo; exit 0; fi
if [ -e "$d" ] && [ ! -d "$d" ]; then echo notrepo; exit 0; fi
command -v git >/dev/null 2>&1 || {{ echo nogit; exit 0; }}
mkdir -p "$d" 2>/dev/null
if out=$(GIT_TERMINAL_PROMPT=0 GIT_SSH_COMMAND="${{GIT_SSH_COMMAND:-ssh -o BatchMode=yes}}" git clone -q -- {url} "$d" 2>&1 </dev/null); then echo cloned; else echo failed; printf '%s\n' "$out"; fi
exit 0"#
    )
}

fn read(said: &str) -> There {
    let mut lines = said.lines();
    match lines.next().map(str::trim) {
        Some("repo") => There::Repo,
        Some("cloned") => There::Cloned,
        Some("notrepo") => There::NotRepo,
        Some("nogit") => There::NoGit,
        _ => There::Failed(crate::safe_git::last_words(&lines.collect::<Vec<_>>().join("\n"))),
    }
}

/// Where Desk `local`'s project lives online: the address of the remote its main line is pushed to.
pub(crate) fn online_home(local: &str) -> Result<String, String> {
    let (repo, onto) = main_checkout(local).ok_or("Cue couldn't read this project's repository here")?;
    let remote = remote_of("", &repo, &onto).ok_or("this project has no online home to download it from (its main line isn't pushed anywhere)")?;
    crate::desk::hub::landing::git::git("", &repo, &[], &["remote", "get-url", &remote])
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .ok_or_else(|| format!("Cue couldn't read where {remote} is"))
}

/// The project in `folder` on `machine`, downloaded from `local`'s online home when it isn't there yet. Ok(true): it
/// was downloaded just now. Err: why it can't be there, in plain words, as the item says.
pub(crate) fn ensure(machine: &str, folder: &str, local: &str) -> Result<bool, String> {
    let shown = crate::machines::display_name(machine);
    let host = crate::machines::get(machine).ok_or_else(|| format!("Cue doesn't know a machine called {shown}"))?.host;
    // Asked about the folder first, so one that's there costs no look at this project's remote.
    let probe = format!("d={}\nif [ -d \"$d\" ] && git -C \"$d\" rev-parse --git-dir >/dev/null 2>&1; then echo repo; else echo missing; fi", crate::machines::remote_dir(folder));
    let said = crate::safe_git::on_machine(machine, &probe).map_err(|e| format!("{shown} didn't answer ({e})"))?;
    if said.trim() == "repo" {
        return Ok(false);
    }
    let url = online_home(local).map_err(|e| format!("{folder} isn't on {shown} yet, and {e}"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    let s = script(folder, &url);
    std::thread::spawn(move || {
        let _ = tx.send(crate::machines::run(&host, &s));
    });
    let said = rx.recv_timeout(LIMIT).unwrap_or_else(|_| Err("it took longer than 15 minutes".into())).map_err(|e| format!("Cue couldn't download the project onto {shown}: {e}"))?;
    match read(&said) {
        There::Repo => Ok(false),
        There::Cloned => Ok(true),
        There::NotRepo => Err(format!("{folder} on {shown} has other files in it and isn't a copy of the project; pick another folder in the Desk's settings")),
        There::NoGit => Err(format!("{shown} has no git, so Cue couldn't download the project there")),
        There::Failed(why) => Err(format!("Cue couldn't download the project onto {shown} into {folder}: {}", if why.is_empty() { "git said nothing".into() } else { why })),
    }
}

/// Where item copies are kept in a clone on a machine (as here: desk/hub/landing/trees.rs `TREES`).
const TREES: &str = crate::desk::hub::landing::trees::TREES;

/// The script that gives item `id` a copy of its own in the clone `folder` there (#679): a git worktree on the branch
/// `branch`, made from `remote`/`onto` (fetched first) or kept when it's there already, kept out of the clone's
/// `git status`. Copies there of items not in `keep` (closed, their sessions gone) go first, each only when nothing in
/// it is uncommitted, ignored files too (its branch stays). Says on its first line what happened, then the copy's full
/// path, or git's words when it failed.
fn own_copy_script(folder: &str, id: &str, branch: &str, remote: &str, onto: &str, keep: &[String]) -> String {
    let d = crate::machines::remote_dir(folder);
    let q = crate::safe_git::sh_quote;
    let t = q(&format!("{TREES}/desk-{id}"));
    let (b, r, from) = (q(branch), q(remote), q(&format!("{remote}/{onto}")));
    let skip = q(&format!("/{TREES}/"));
    let trees = q(TREES);
    let keep: Vec<&str> = keep.iter().map(String::as_str).filter(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).collect();
    let keep = q(&format!(" {} ", keep.join(" ")));
    format!(
        r#"cd {d} 2>/dev/null || {{ echo failed; echo "the folder isn't there"; exit 0; }}
if [ -d {t} ] && git -C {t} rev-parse --git-dir >/dev/null 2>&1; then echo there; (cd {t} && pwd); exit 0; fi
x=$(git rev-parse --git-common-dir 2>/dev/null) || {{ echo failed; echo "it isn't a copy of the project"; exit 0; }}
for c in {trees}/desk-*; do [ -d "$c" ] || continue; case {keep} in *" ${{c##*/desk-}} "*) continue;; esac; [ -z "$(git -C "$c" status --porcelain --ignored 2>/dev/null)" ] && git worktree remove "$c" >/dev/null 2>&1; done
mkdir -p "$x/info" 2>/dev/null
grep -qxF {skip} "$x/info/exclude" 2>/dev/null || printf '%s\n' {skip} >> "$x/info/exclude"
GIT_TERMINAL_PROMPT=0 GIT_SSH_COMMAND="${{GIT_SSH_COMMAND:-ssh -o BatchMode=yes}}" git fetch -q {r} </dev/null >/dev/null 2>&1
git worktree prune >/dev/null 2>&1
if git show-ref -q --verify "refs/heads/"{b}; then out=$(git worktree add -q {t} {b} 2>&1 </dev/null); else out=$(git worktree add -q -b {b} {t} {from} 2>&1 </dev/null); fi
if [ $? -eq 0 ]; then echo made; (cd {t} && pwd); else echo failed; printf '%s\n' "$out"; fi
exit 0"#
    )
}

/// Item `id`'s own copy in the clone `folder` on `machine` (#679), on its branch `branch` from `remote`/`onto`: two
/// items on one machine never share a folder, so neither's work lands under the other's name. Ok: the copy's full path
/// there, where its session starts. Err: why not, in plain words, as the item says.
/// `keep`: the items whose copies there stay (open, or a session still in it).
pub(crate) fn own_copy(machine: &str, folder: &str, id: &str, branch: &str, remote: &str, onto: &str, keep: &[String]) -> Result<String, String> {
    let shown = crate::machines::display_name(machine);
    let said = crate::safe_git::on_machine(machine, &own_copy_script(folder, id, branch, remote, onto, keep)).map_err(|e| format!("{shown} didn't answer ({e})"))?;
    let mut lines = said.lines().map(str::trim);
    match lines.next() {
        Some("made" | "there") => lines.next().filter(|p| p.starts_with('/')).map(str::to_string).ok_or_else(|| format!("Cue couldn't read where #{id}'s copy is on {shown}")),
        _ => {
            let why = crate::safe_git::last_words(&lines.collect::<Vec<_>>().join("\n"));
            Err(format!("Cue couldn't make a copy of its own for #{id} in {folder} on {shown}: {}", if why.is_empty() { "git said nothing".into() } else { why }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_machine_said_is_read_on_its_first_line() {
        assert_eq!(read("repo\n"), There::Repo);
        assert_eq!(read("cloned\n"), There::Cloned);
        assert_eq!(read("notrepo\n"), There::NotRepo);
        assert_eq!(read("nogit\n"), There::NoGit);
        assert_eq!(read("failed\nfatal: could not read from remote repository\n"), There::Failed("fatal: could not read from remote repository".into()));
        assert_eq!(read(""), There::Failed(String::new()));
    }
}
