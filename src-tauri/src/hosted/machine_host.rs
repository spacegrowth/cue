//! cue-host on another machine (docs/dev/cue-agent-host.md §3.4, §6). Cue.app carries cue-host built for Linux
//! (x86_64, aarch64) and macOS (scripts/build-host.sh, in Contents/Resources/host-builds); the first time a machine
//! hosts a session, Cue puts the one it needs at `~/.cue/bin/b-<build>/cue-host` there, checked against its SHA-256,
//! and reaches it with `cue-host connect` over the machine's shared SSH connection. As on this Mac, each build has a
//! folder of its own (`~/.cue/host/b-<build>`), so a Cue with fixed host code never sends a session to an older one.

use super::conn::Dial;
use crate::machines::{self, Machine};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// This Cue's host build: the one it puts on machines.
fn build() -> &'static str {
    cue_host::proto::BUILD
}

/// cue-host's place on a machine, under your home there (each a path a shell there reads after `$HOME/`). The program
/// is called `cue-host` there, as on this Mac, so it shows as that in `ps`; each build in a folder of its own.
pub(super) fn bin() -> String {
    format!(".cue/bin/b-{}/cue-host", build())
}

pub(super) fn home() -> String {
    format!(".cue/host/b-{}", build())
}

/// Where Cue's own copies are: in the app, or (a build run from the repo) where scripts/build-host.sh put them.
fn carried() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent().map(|d| d.join("../Resources/host-builds")).filter(|p| p.is_dir()).unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/host-builds")))
}

/// The file Cue carries for a machine that says `uname -sm` is `uname`.
fn file_for(uname: &str) -> Option<&'static str> {
    let mut f = uname.split_whitespace();
    match (f.next()?, f.next()?) {
        ("Linux", "x86_64" | "amd64") => Some("cue-host-linux-x86_64"),
        ("Linux", "aarch64" | "arm64") => Some("cue-host-linux-aarch64"),
        ("Darwin", _) => Some("cue-host-darwin"),
        _ => None,
    }
}

/// `file`'s SHA-256 as Cue's copies list it (SHA256SUMS beside them), when they're this build's.
fn expected_sum(file: &str) -> Result<String, String> {
    let dir = carried();
    let theirs = std::fs::read_to_string(dir.join("BUILD")).unwrap_or_default();
    if theirs.trim() != build() {
        return Err("this Cue doesn't carry cue-host for other machines (scripts/build-host.sh builds it)".into());
    }
    let sums = std::fs::read_to_string(dir.join("SHA256SUMS")).map_err(|e| format!("cue-host's checksums: {e}"))?;
    sums.lines().find_map(|l| l.split_once("  ").filter(|(_, f)| *f == file).map(|(s, _)| s.to_string())).ok_or_else(|| format!("no checksum for {file}"))
}

/// Machines cue-host is known to be on, this build, while Cue runs (by host).
fn placed() -> &'static Mutex<HashSet<String>> {
    static P: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    P.get_or_init(Mutex::default)
}

/// Cue forgets which machines have cue-host (it's checked again on next use).
#[cfg(test)]
pub(super) fn forget_placed() {
    crate::locking::lock(placed(), "machine hosts placed").clear();
}

/// A shell's word for `$HOME/<rel>` on the machine (in double quotes: `rel` is Cue's own, letters, digits and
/// `./-`, and a `$$` in it is the shell's pid).
fn at_home(rel: &str) -> String {
    debug_assert!(rel.chars().all(|c| c.is_ascii_alphanumeric() || "./-$".contains(c)), "{rel}");
    format!("\"$HOME/{rel}\"")
}

/// Where Cue's Pi extension goes on a machine (loaded with `-e` by a hosted Pi there that doesn't have it installed).
pub(super) const PI_EXTENSION: &str = ".cue/pi/cue.ts";

/// cue-host there, this build, and Cue's Pi extension beside it: each put there unless it already is (its checksum
/// matches). Says why not: a machine Cue has no cue-host for, one it can't reach, a copy that arrived damaged.
pub(super) fn ensure(m: &Machine) -> Result<(), String> {
    if crate::locking::lock(placed(), "machine hosts placed").contains(&m.host) {
        return Ok(());
    }
    let sum_of = |f: &str| format!("f={f}; [ -f \"$f\" ] && {{ sha256sum \"$f\" 2>/dev/null || shasum -a 256 \"$f\"; }} || echo none");
    let said = machines::run(&m.host, &format!("uname -sm; {}; {}; true", sum_of(&at_home(&bin())), sum_of(&at_home(PI_EXTENSION))))?;
    let lines: Vec<&str> = said.lines().collect();
    let uname = lines.first().unwrap_or(&"").trim();
    let file = file_for(uname).ok_or_else(|| format!("Cue has no cue-host for {} ({uname}), so it can't host sessions there", m.name))?;
    let there = |i: usize| lines.get(i).and_then(|l| l.split_whitespace().next()).unwrap_or("");
    for (i, (file, rel, mode)) in [(file, bin(), "700"), ("cue-pi.ts", PI_EXTENSION.to_string(), "600")].into_iter().enumerate() {
        let sum = expected_sum(file)?;
        if there(i + 1) != sum {
            put(m, file, &rel, mode, &sum)?;
        }
    }
    crate::locking::lock(placed(), "machine hosts placed").insert(m.host.clone());
    Ok(())
}

/// Cue's `file` sent to the machine's `$HOME/<rel>`: written beside it under a temporary name, its checksum checked
/// there, then moved into place whole (with `mode`: yours only).
fn put(m: &Machine, file: &str, rel: &str, mode: &str, sum: &str) -> Result<(), String> {
    let bytes = std::fs::read(carried().join(file)).map_err(|e| format!("Cue's copy of {file}: {e}"))?;
    let dir = rel.rsplit_once('/').map_or(".cue", |(d, _)| d);
    let script = format!(
        "umask 077; mkdir -p {dir} || exit 1; t={tmp}; cat >\"$t\" && s=$( (sha256sum \"$t\" 2>/dev/null || shasum -a 256 \"$t\") | cut -d' ' -f1) && [ \"$s\" = {sum} ] && chmod {mode} \"$t\" && mv -f \"$t\" {to} && echo PLACED || {{ rm -f \"$t\"; echo \"NOT PLACED $s\"; }}",
        dir = at_home(dir),
        tmp = at_home(&format!("{rel}.part-$$")),
        sum = machines::shq(sum),
        to = at_home(rel),
    );
    let said = machines::run_with_bytes(&m.host, &script, &bytes, &|_| {})?;
    if said.contains("PLACED") && !said.contains("NOT PLACED") {
        eprintln!("cue: put {file} ({}) on {}", build(), m.name);
        return Ok(());
    }
    Err(format!("{file} didn't arrive whole on {} ({})", m.name, said.trim()))
}

/// How Cue reaches the host on `m`: `connect` there over the shared connection, run straight from its place (no login
/// shell: one that prints something would land among the host's lines), starting the host if none runs.
pub(super) fn dial(m: &Machine) -> Dial {
    let host = m.host.clone();
    let script = format!("exec {} connect --home {}", at_home(&bin()), at_home(&home()));
    Dial::Machine { home: PathBuf::from(format!("{}:~/{}", m.name, home())), command: Box::new(move || machines::direct_command(&host, &script)), once: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_machine_gets_the_build_for_its_system() {
        assert_eq!(file_for("Linux x86_64"), Some("cue-host-linux-x86_64"));
        assert_eq!(file_for("Linux aarch64"), Some("cue-host-linux-aarch64"));
        assert_eq!(file_for("Linux arm64"), Some("cue-host-linux-aarch64"));
        assert_eq!(file_for("Darwin arm64"), Some("cue-host-darwin"));
        assert_eq!(file_for("Linux riscv64"), None);
        assert_eq!(file_for("FreeBSD amd64"), None);
        assert_eq!(file_for(""), None);
    }

    #[test]
    fn its_place_there_is_this_builds_under_your_home() {
        assert_eq!(bin(), format!(".cue/bin/b-{}/cue-host", cue_host::proto::BUILD), "named cue-host there, as on this Mac (a folder per build)");
        assert_eq!(home(), format!(".cue/host/b-{}", cue_host::proto::BUILD));
        assert_eq!(at_home(".cue/bin/x"), "\"$HOME/.cue/bin/x\"");
    }
}
