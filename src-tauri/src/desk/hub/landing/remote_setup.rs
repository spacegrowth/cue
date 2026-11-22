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
