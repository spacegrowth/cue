//! Starting a session: + New (hosted by Cue, here or on a machine), and the new sessions Cue starts itself (a Desk's
//! lead, a hand-off's next session, a fork: `StartOpts::fork_of`). The window's commands are in ipc/sessions.rs and
//! ipc/hosted.rs.

use super::{Hub, HostedStart};
use crate::hosted::{Agent, Start};
use crate::logs::live;
use crate::model::{self, Origin};
use crate::terminal::focus;
use std::sync::Arc;

/// The work behind "+ New session": `agent` started hosted in `cwd`. Blocking: it waits for the agent to start.
pub(crate) fn start_session(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str) -> Result<serde_json::Value, String> {
    start_session_with(h, agent, cwd, message, name, &StartOpts::default())
}

/// What + New session can choose beyond the folder: where it runs (None: Settings' default), how much it
/// may do without asking, and any extra flags.
#[derive(Default)]
pub(crate) struct StartOpts {
    /// Ignored: every session starts hosted. Kept until the window stops sending it.
    #[allow(dead_code)]
    pub tmux: Option<bool>,
    pub perm: String,
    pub extra: String,
    /// A machine from + New → Machine to run it on (empty: this Mac): hosted by cue-host there.
    pub machine: String,
    /// The MCP servers it runs with (a Claude Code session on this Mac): written as its selection
    /// (logs/mcp.rs) under the id Cue picks, and passed as `--mcp-config <file> --strict-mcp-config`.
    /// None: whatever its folder has on.
    pub mcp: Option<Vec<String>>,
    /// Images with the first message (pasted or dropped into its box): saved, their paths typed after it.
    pub images: Vec<model::Upload>,
    /// Ignored: every session starts hosted by Cue, with no terminal. Kept until its callers stop setting it.
    #[allow(dead_code)]
    pub hosted: bool,
    /// You said you trust the folder (Cue asked, as Claude Code's terminal would: `trusted` below).
    pub trust: bool,
    /// The project whose Desk it starts as the lead of (desk/): it starts with the Desk's instructions. "" for any
    /// other.
    pub desk_lead: String,
    /// The Claude Code session it starts as a fork of (Fork): its conversation copied under the new id
    /// (`--resume <it> --fork-session`), its MCP selection with it. "" for a new conversation.
    pub fork_of: String,
}

pub(crate) fn start_session_with(h: &Arc<Hub>, agent: &str, cwd: &str, message: &str, name: &str, opts: &StartOpts) -> Result<serde_json::Value, String> {
    // Every session Cue starts is hosted, here or on a machine (cue-host there): no terminal.
    let hosted = Agent::named(agent).ok_or("Cue starts Claude Code, Pi and Codex")?;
    let saved = crate::uploads::save(&opts.images)?;
    if !opts.machine.is_empty() {
        return start_hosted(h, hosted, cwd.trim(), message, name, opts, &saved);
    }
    let dir = cwd.trim();
    let dir = if let Some(rest) = dir.strip_prefix("~") { format!("{}{rest}", std::env::var("HOME").unwrap_or_default()) } else { dir.to_string() };
    if !std::path::Path::new(&dir).is_dir() {
        return Err(format!("{dir} isn't a folder"));
    }
    start_hosted(h, hosted, &dir, message, name, opts, &saved)
}

/// + New session, hosted: `agent` started by Cue in `dir` with no terminal, its first message (and images)
/// handed to it once it's up. Its id is known when it has started (Cue's pick, or Codex's), so the session is in
/// Active at once.
pub(crate) fn start_hosted(h: &Arc<Hub>, agent: Agent, dir: &str, message: &str, name: &str, opts: &StartOpts, saved: &[crate::uploads::Saved]) -> Result<serde_json::Value, String> {
    let start = Start::fresh();
    let sid = start.session_id().to_string();
    let (mut args, model) = model_apart(agent, flag_words(&opts.extra)?);
    if !opts.fork_of.is_empty() {
        if agent != Agent::Claude {
            return Err("Fork works with Claude Code sessions".into());
        }
        // The fork keeps a copy of its MCP selection (unless one was picked for it).
        let (was, now) = (crate::logs::mcp::selection_path(&opts.fork_of), crate::logs::mcp::selection_path(&sid));
        if opts.machine.is_empty() && opts.mcp.is_none() && was.exists() && std::fs::copy(&was, &now).is_ok() {
            args.extend(crate::logs::mcp::flags_for(&now));
        }
    }
    let (dir, transcript_path) = if opts.machine.is_empty() {
        // An MCP selection is Claude Code's (Pi has no MCP servers of its own), and this Mac's: a machine's agent
        // has the servers set up there.
        if let Some(names) = opts.mcp.as_ref().filter(|_| agent == Agent::Claude) {
            let path = crate::logs::mcp::selection_path(&sid);
            crate::logs::mcp::write_selection(&path, &crate::logs::mcp::definitions(dir), names)?;
            args.extend(crate::logs::mcp::flags_for(&path));
        }
        // Claude Code's conversation is a file Cue can name now; Pi's is named by Pi when it starts.
        (dir.to_string(), if agent == Agent::Claude { live::claude_transcript_path(dir, &sid).to_string_lossy().into_owned() } else { String::new() })
    } else {
        on_machine(&opts.machine, agent, dir, &sid)?
    };
    if agent == Agent::Claude && !trusted(&opts.machine, &dir) {
        if !opts.trust {
            return Err(format!("{UNTRUSTED}{dir}"));
        }
        trust(&opts.machine, &dir)?;
    }
    let origin = Origin { session_id: sid.clone(), harness: agent.program().into(), cwd: dir, transcript_path, hosted: true, machine: opts.machine.clone(), ..Default::default() };
    // A fork copies the first written conversation it comes from (a fork not yet written is followed back), and has
    // its name for Claude Code's own list too (/resume), as a fork in a terminal had it.
    let fork_from = (!opts.fork_of.is_empty()).then(|| super::hosted::fork_source(&origin, opts.fork_of.clone()));
    if let Some(from) = &fork_from {
        args.extend(["--resume".to_string(), from.clone(), "--fork-session".to_string(), "-n".to_string(), name.to_string()]);
    }
    if !opts.desk_lead.is_empty() {
        h.desk_lead_starting(&opts.desk_lead, &sid, name);
    }
    let started = h.run_hosted(&origin, start, &HostedStart { permissions: opts.perm.clone(), args, model });
    let origin = match started {
        Ok(origin) => origin,
        Err(e) => {
            if !opts.desk_lead.is_empty() { let _ = h.desk_set_member(&opts.desk_lead, &sid, false); }
            return Err(e);
        }
    };
    // A fork is kept as one only once it has started: a start that failed leaves nothing behind.
    if let Some(from) = &fork_from {
        super::hosted::note_fork(&sid, from, name);
    }
    if !opts.desk_lead.is_empty() && origin.session_id != sid {
        // Codex assigns its own ID when the thread opens. Replace the provisional lead and membership before
        // recording or messaging the session, so the Desk points to the session that actually exists.
        h.desk_put_lead(&opts.desk_lead, &origin.session_id, name);
        h.desk_set_member(&opts.desk_lead, &sid, false)?;
    }
    let sid = origin.session_id.clone();
    let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
    h.started(origin, "", &[], false);
    h.name_new(&sid, name);
    if !message.trim().is_empty() || !paths.is_empty() {
        // Under its typing gate (no lock held here): a reply sent meanwhile waits for this one.
        h.send_to_hosted(&sid, message, saved, false)?;
    }
    let where_ = if opts.machine.is_empty() { "hosted in Cue".to_string() } else { format!("hosted on {}", crate::machines::display_name(&opts.machine)) };
    Ok(serde_json::json!({ "session_id": sid, "detail": format!("Started {}, {where_}", agent.name()) }))
}

/// What + New is told when Claude Code hasn't been trusted in a folder: the window asks you, and starts it again with
/// `trust` if you say so. Followed by the folder.
pub(crate) const UNTRUSTED: &str = "untrusted folder: ";

/// Whether Claude Code may be hosted in `dir` (on `machine`) without asking. In its terminal Claude Code asks "Do you
/// trust the files in this folder?" before it runs anything there (a folder's own settings can run commands); hosted,
/// it never asks, so Cue does: a folder you trusted in Claude Code on this Mac, or once in Cue (`hosted.trusted`).
fn trusted(machine: &str, dir: &str) -> bool {
    trusted_with(&crate::config::load(), machine, dir, || focus::claude_trusts(dir))
}

/// `trusted`, from Settings `c` and whether Claude Code trusts the folder on this Mac (asked only for one here).
fn trusted_with(c: &serde_json::Value, machine: &str, dir: &str, claude_trusts: impl FnOnce() -> bool) -> bool {
    (machine.is_empty() && claude_trusts()) || trusted_list(c).contains(&trust_key(machine, dir))
}

/// You trust `dir` (on `machine`) for hosting: kept in Settings (`hosted.trusted`).
fn trust(machine: &str, dir: &str) -> Result<(), String> {
    let key = trust_key(machine, dir);
    crate::config::update_with("hosted.trusted", |c| {
        let mut all = trusted_list(c);
        if !all.contains(&key) {
            all.push(key);
        }
        Ok((Some(serde_json::json!(all)), ()))
    })
}

fn trusted_list(c: &serde_json::Value) -> Vec<String> {
    c.pointer("/hosted/trusted").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
}

/// How a folder is kept in the list: its path, after its machine's name on a machine.
fn trust_key(machine: &str, dir: &str) -> String {
    let dir = dir.trim_end_matches('/');
    if machine.is_empty() { dir.to_string() } else { format!("{machine}:{dir}") }
}

/// A session's folder on machine `name` (`dir` as you typed it: `~` is your home there), as its full path there, and
/// for Claude Code the copy on this Mac of where its conversation will be (machines/log_mirror.rs: copied as it's written,
/// so the chat reads it as it would a local one). Says so when the folder isn't there.
fn on_machine(name: &str, agent: Agent, dir: &str, sid: &str) -> Result<(String, String), String> {
    let m = crate::machines::get(name).ok_or(format!("{name} isn't one of your machines (+ New → Machine)"))?;
    let said = crate::machines::run(&m.host, &format!("cd {} 2>/dev/null || {{ echo NODIR; exit 0; }}; pwd; pwd -P; printf '%s\\n' \"${{CLAUDE_CONFIG_DIR:-$HOME/.claude}}\"", crate::machines::remote_dir(dir)))?;
    let l: Vec<&str> = said.lines().map(str::trim).collect();
    if l.first() == Some(&"NODIR") || l.len() < 3 {
        return Err(format!("{dir} isn't a folder on {}", m.name));
    }
    let transcript = if agent == Agent::Claude { crate::machines::log_mirror::local_copy(&m, &format!("{}/projects/{}/{sid}.jsonl", l[2], live::claude_project_folder(l[1]))) } else { String::new() };
    Ok((l[0].to_string(), transcript))
}

/// Codex takes its model in what Cue sends it, not as an argument: `--model x` / `-m x` taken out of + New's flags
/// for it (the rest, its own settings as `-c key=value`, go on its command line). Other agents keep theirs.
fn model_apart(agent: Agent, words: Vec<String>) -> (Vec<String>, Option<String>) {
    if agent != Agent::Codex {
        return (words, None);
    }
    let (mut rest, mut model) = (vec![], None);
    let mut it = words.into_iter();
    while let Some(w) = it.next() {
        match w.as_str() {
            "--model" | "-m" => model = it.next(),
            _ => match w.strip_prefix("--model=") {
                Some(m) => model = Some(m.to_string()),
                None => rest.push(w),
            },
        }
    }
    (rest, model)
}

/// + New's extra flags, as the arguments they are. Taken word by word: a hosted session's Claude gets them
/// as arguments, with no shell to read quotes, so a quoted flag is refused rather than passed in pieces.
/// SHORTCUT: no quoting at all, fine for the flags + New is used with (--model, --effort); read them with a
/// shell-words parser if a quoted value (a system prompt) is ever wanted for a hosted session.
fn flag_words(extra: &str) -> Result<Vec<String>, String> {
    if extra.contains(['"', '\'', '\\']) {
        return Err("a hosted session takes flags without quotes for now: start it in a terminal for that".into());
    }
    Ok(extra.split_whitespace().map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    use super::{flag_words, trust_key, trusted_with};
    use serde_json::json;

    #[test]
    fn claude_code_is_hosted_without_asking_only_where_you_trusted_the_folder() {
        let none = json!({});
        assert!(trusted_with(&none, "", "/w/app", || true), "trusted in Claude Code on this Mac");
        assert!(!trusted_with(&none, "", "/w/app", || false));
        assert!(!trusted_with(&none, "box", "/w/app", || panic!("Claude Code's own trust here says nothing of a machine")));
        let kept = json!({ "hosted": { "trusted": [trust_key("", "/w/app/"), trust_key("box", "/srv/app")] } });
        assert!(trusted_with(&kept, "", "/w/app", || false), "trusted once in Cue (a trailing / or not)");
        assert!(trusted_with(&kept, "box", "/srv/app", || false));
        assert!(!trusted_with(&kept, "other", "/srv/app", || false), "the same path on another machine is another folder");
        assert!(!trusted_with(&kept, "", "/w/app/sub/../../evil", || false));
    }

    #[test]
    fn flags_are_taken_word_by_word_and_quoted_ones_are_refused() {
        assert_eq!(flag_words(" --model opus  --verbose ").unwrap(), vec!["--model", "opus", "--verbose"]);
        assert!(flag_words("--append-system-prompt 'be brief'").is_err());
    }
}
