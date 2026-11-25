//! The command lines that start, resume and fork an agent's session (`cd <folder> && claude …`), every
//! word quoted so a message or a flag can never run as more shell; and whether Claude Code trusts a folder.

/// Single-quoted for the shell: 'it'\''s' (a typed command line, so a message can't run as code).
pub fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A random session id (UUID v4), for an agent Cue starts: Cue knows the session before it says anything.
pub fn new_session_id() -> String {
    use std::io::Read;
    let mut b = [0u8; 16];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// How much a new session may do without asking you (+ New session → Permissions), per agent, as its
/// flags. "ask": asks before anything it isn't allowed, whatever your own default mode is (Claude's
/// `defaultMode` in your settings could be "auto"); empty: as it normally does (a resume). Pi asks through Cue's own gate (Settings → Pi), so "skip" turns
/// that off for this session alone.
/// (A session Cue hosts takes the same flags: hosted::launch.)
pub fn permission_flags(agent: &str, perm: &str) -> Result<(Option<(&'static str, &'static str)>, &'static [&'static str]), String> {
    // (a variable set for the program, flags after it)
    Ok(match (agent, perm) {
        ("claude", "ask") => (None, &["--permission-mode", "default"]),
        (_, "ask" | "") => (None, &[]),
        ("claude", "auto") => (None, &["--permission-mode", "auto"]),
        ("claude", "edits") => (None, &["--permission-mode", "acceptEdits"]),
        ("claude", "plan") => (None, &["--permission-mode", "plan"]),
        ("claude", "skip") => (None, &["--dangerously-skip-permissions"]),
        ("codex", "edits") => (None, &["--sandbox", "workspace-write"]),
        ("codex", "skip") => (None, &["--dangerously-bypass-approvals-and-sandbox"]),
        ("pi", "skip") => (Some(("CUE_PI_GATE", "off")), &[]),
        _ => return Err(format!("{agent} has no “{perm}” permission mode")),
    })
}

/// Whether Claude Code trusts `dir` already: it or a folder above it was accepted ("Yes, I trust this
/// folder"), as recorded in ~/.claude.json. If not, a session started there first asks in its terminal.
/// Unreadable config: assume it does (no false alarm).
pub fn claude_trusts(dir: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let Ok(text) = std::fs::read_to_string(format!("{home}/.claude.json")) else { return true };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return true };
    trusted_in(&v, dir)
}

/// Claude Code records trust under the folder's real path (/tmp is "/private/tmp" on a Mac), so a
/// folder counts as trusted if it, its real path, or a parent of either was accepted.
fn trusted_in(config: &serde_json::Value, dir: &str) -> bool {
    if trusted_as_written(config, dir) {
        return true;
    }
    match std::fs::canonicalize(dir) {
        Ok(real) if real.to_string_lossy() != dir => trusted_as_written(config, &real.to_string_lossy()),
        _ => false,
    }
}

fn trusted_as_written(config: &serde_json::Value, dir: &str) -> bool {
    let mut p = Some(std::path::Path::new(dir));
    while let Some(d) = p {
        if config.pointer(&format!("/projects/{}", d.to_string_lossy().replace('~', "~0").replace('/', "~1"))).and_then(|x| x.get("hasTrustDialogAccepted")).and_then(|x| x.as_bool()) == Some(true) {
            return true;
        }
        p = d.parent();
    }
    false
}

/// The same with extra arguments, each quoted whole: `cd <folder> && claude --resume <id> <args…>`
/// (a session's MCP selection: `--mcp-config <file> --strict-mcp-config`).
pub fn resume_line_with(cwd: &str, session_id: &str, args: &[String]) -> Result<String, String> {
    if session_id.is_empty() || !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("that isn't a Claude Code session id".into());
    }
    let mut line = format!("cd {} && claude --resume {}", shq(cwd), shq(session_id));
    for a in args {
        line += &format!(" {}", shq(a));
    }
    Ok(line)
}

/// A Claude Code session on this Mac resumed as Cue left it: on its model (model_args) and with its MCP
/// selection when it has one (logs/mcp.rs), so a choice made in Cue sticks across park, close and restart.
pub fn resume_line_local(cwd: &str, session_id: &str, transcript: &str) -> Result<String, String> {
    let mut args = model_args(transcript, "");
    args.extend(crate::logs::mcp::selection_flags(session_id));
    resume_line_with(cwd, session_id, &args)
}

/// `--model <its model>` to resume a Claude Code session on (its 1M window as `[1m]`; logs::steps::model_of),
/// when its log can be read here: this Mac's, or a machine's copy (machines::log_mirror; a machine's own path is
/// never read from here). Resumed without it, Claude Code comes back on its default model and window.
pub fn model_args(transcript: &str, machine: &str) -> Vec<String> {
    let readable = !transcript.is_empty() && (machine.is_empty() || std::path::Path::new(transcript).starts_with(crate::server::cue_dir().join("machines")));
    readable.then(|| crate::logs::steps::model_of(transcript)).flatten().map(|m| vec!["--model".to_string(), m]).unwrap_or_default()
}

/// The command line that picks a parked session back up, for its agent: `claude --resume <id>`,
/// `codex resume <id>`, `pi --session <id>`, each in its folder.
pub fn resume_line_for(harness: &str, cwd: &str, session_id: &str) -> Result<String, String> {
    resume_line_for_with(harness, cwd, session_id, &[])
}

/// The same with extra arguments for Claude Code (see resume_line_with); other agents take none.
pub fn resume_line_for_with(harness: &str, cwd: &str, session_id: &str, args: &[String]) -> Result<String, String> {
    if session_id.is_empty() || !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Cue doesn't have a session id it can resume".into());
    }
    let program = match harness {
        "claude" => return resume_line_with(cwd, session_id, args),
        "codex" => "codex resume",
        "pi" => "pi --session",
        other => return Err(format!("Cue can't resume a {other} session")),
    };
    Ok(format!("cd {} && {program} {}", shq(cwd), shq(session_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_trusted_if_it_or_a_parent_was_accepted() {
        let c = serde_json::json!({ "projects": { "/a/b": { "hasTrustDialogAccepted": true }, "/x": { "hasTrustDialogAccepted": false } } });
        assert!(trusted_in(&c, "/a/b"));
        assert!(trusted_in(&c, "/a/b/c/d"));
        assert!(!trusted_in(&c, "/a"));
        assert!(!trusted_in(&c, "/x/y"));
    }

    #[test]
    fn a_folder_reached_through_a_symlink_is_trusted_under_its_real_path() {
        // Claude Code stores /private/tmp for a session started in /tmp: the link must still count.
        let base = std::env::temp_dir().join(format!("cue-trust-{}", std::process::id()));
        let real = base.join("real");
        let link = base.join("link");
        std::fs::create_dir_all(real.join("deeper")).unwrap();
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let real_s = std::fs::canonicalize(&real).unwrap().to_string_lossy().into_owned();
        let c = serde_json::json!({ "projects": { real_s.clone(): { "hasTrustDialogAccepted": true } } });
        assert!(trusted_in(&c, &link.to_string_lossy()));
        assert!(trusted_in(&c, &link.join("deeper").to_string_lossy()), "a child of the link resolves through it too");
        let none = serde_json::json!({ "projects": {} });
        assert!(!trusted_in(&none, &link.to_string_lossy()));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_parked_session_resumes_with_its_own_agents_command() {
        assert_eq!(resume_line_for("claude", "/x/my repo", "3f2a-91").unwrap(), "cd '/x/my repo' && claude --resume '3f2a-91'");
        assert_eq!(resume_line_for("codex", "/x", "a91f").unwrap(), "cd '/x' && codex resume 'a91f'");
        assert_eq!(resume_line_for("pi", "/x", "P1").unwrap(), "cd '/x' && pi --session 'P1'");
        assert!(resume_line_for("pi", "/x", "a; rm -rf ~").is_err(), "only an id, never more shell");
        assert!(resume_line_for("codex", "/x", "").is_err());
        assert!(resume_line_for("bash", "/x", "S1").is_err());
        let mcp = vec!["--mcp-config".to_string(), "/a b/mcp/3f2a-91.json".to_string(), "--strict-mcp-config".to_string()];
        assert_eq!(resume_line_for_with("claude", "/x", "3f2a-91", &mcp).unwrap(), "cd '/x' && claude --resume '3f2a-91' '--mcp-config' '/a b/mcp/3f2a-91.json' '--strict-mcp-config'");
        assert_eq!(resume_line_for_with("codex", "/x", "a91f", &mcp).unwrap(), "cd '/x' && codex resume 'a91f'");
    }

}
