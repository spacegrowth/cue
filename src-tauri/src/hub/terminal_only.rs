//! An agent's own commands, in a hosted session (docs/dev/cue-hosted-sessions.md §9, H5). Headless, an agent has none
//! of its terminal interface: sent to it, a command like `/config` does nothing, and Pi and Codex take theirs for a
//! message to the model. So Cue looks at what you send first:
//! - Claude Code's own screens (`/config`, `/login`, `/theme`…: what Claude reports as `terminal_slash_commands`, plus
//!   signing in and its settings, which headless never offers) open in a terminal tab: `claude /config` in the
//!   session's folder (on its machine, over SSH).
//! - Pi's sign-in and settings screens open Pi in a terminal tab, and Cue says which command to type there (a command
//!   given to Pi as it starts goes to the model, even in its terminal).
//! - Codex signs in with `codex login` / `codex logout`, run in a terminal tab.
//! - Pi's and Codex's other own commands act on the session itself, which a hosted one can't do from here: refused,
//!   saying so, never sent to the model.
//! Never on the session's conversation (two processes on one conversation would both write it).

use super::*;
use crate::terminal::launch::Place;

/// Claude Code's commands that need its terminal whatever it reports: signing in and out, and its own settings
/// screens.
const CLAUDE_ALWAYS: [&str; 8] = ["login", "logout", "config", "theme", "terminal-setup", "doctor", "ide", "vim"];
/// Pi's commands that change Pi itself (its sign-in and settings), not a session: done in Pi in a terminal.
const PI_SCREENS: [&str; 7] = ["login", "logout", "settings", "scoped-models", "hotkeys", "changelog", "trust"];

/// What an agent's own command becomes in a hosted session.
#[derive(Debug, PartialEq)]
pub(crate) enum Own {
    /// Run `line` in a terminal tab in the session's folder; `then` is what to do there, if anything.
    Terminal { line: String, then: String },
    /// One a hosted session doesn't have: refused, saying so.
    NotHosted,
}

/// What `/name` (with `text`, all of it) is for `harness`, in a hosted session: None for one that goes to it as sent
/// (a skill, an extension's command, one that runs headless). `terminal_only`: what Claude reported at its start.
pub(crate) fn own(harness: &str, name: &str, text: &str, terminal_only: &[String]) -> Option<Own> {
    let sh = crate::machines::shq;
    match harness {
        "claude" if CLAUDE_ALWAYS.contains(&name) || terminal_only.iter().any(|n| n == name) => Some(Own::Terminal { line: format!("claude {}", sh(text.trim())), then: String::new() }),
        "pi" if PI_SCREENS.contains(&name) => Some(Own::Terminal { line: "pi --no-session".into(), then: format!("type {} there", text.trim()) }),
        "pi" if crate::commands::pi_builtin(name) => Some(Own::NotHosted),
        "codex" if name == "login" || name == "logout" => Some(Own::Terminal { line: format!("codex {name}"), then: String::new() }),
        "codex" if crate::commands::codex_builtin(name) => Some(Own::NotHosted),
        _ => None,
    }
}

impl Hub {
    /// The commands Claude said only its terminal has, at the session's start.
    pub(crate) fn note_terminal_only(&self, session_id: &str, names: Vec<String>) {
        self.write(|st| st.terminal_only.insert(session_id.to_string(), names));
    }

    /// What `/name` is in hosted session `session_id`, when it's one of its agent's own (see `own`).
    pub(crate) fn own_command(&self, session_id: &str, name: &str, text: &str) -> Option<Own> {
        let origin = self.hosted_origin(session_id)?;
        let reported = self.read(|st| st.terminal_only.get(session_id).cloned()).unwrap_or_default();
        own(&origin.harness, name, text, &reported)
    }

    /// `text`, when it's one of a hosted session's agent's own commands: opened in a terminal tab, or refused. What
    /// it did (Err: refused). None: it's an ordinary message, for the session.
    pub(crate) fn run_own_command(&self, session_id: &str, text: &str) -> Option<Result<String, String>> {
        let name = text.trim().strip_prefix('/')?.split_whitespace().next()?.to_string();
        let o = self.hosted_origin(session_id)?;
        let agent = crate::hosted::Agent::named(&o.harness).map_or("the agent", |a| a.name());
        Some(match self.own_command(session_id, &name, text)? {
            Own::NotHosted => Err(format!("/{name} is one of {agent}'s own commands, which a hosted session doesn't have: Move to terminal to use it")),
            Own::Terminal { line, then } => self.open_in_terminal(&o, &line, &name).map(|tab| {
                let then = if then.is_empty() { String::new() } else { format!(": {then}") };
                format!("Opened {agent} in {tab}{then}. /{name} is one of {agent}'s own screens, which only a terminal shows")
            }),
        })
    }

    /// `line` run in a terminal tab in `o`'s folder, here or on its machine: where it opened.
    fn open_in_terminal(&self, o: &Origin, line: &str, name: &str) -> Result<String, String> {
        if o.machine.is_empty() {
            return self.launcher().here(line, &o.cwd, &format!("/{name}"), Place::chosen(Some(false))).map(|t| t.what);
        }
        let m = crate::machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more", o.machine))?;
        let there = crate::machines::remote_command_err(&format!("cd {} && exec {line}", crate::machines::remote_dir(&o.cwd)));
        let ssh = format!("ssh {} -t {} {}", crate::machines::shared().join(" "), m.host, crate::machines::shq(&there));
        crate::terminal::focus::open_tab_with(&ssh).map(|t| format!("{} on {}", t.what, crate::machines::display_name(&m.name)))
    }
}

#[cfg(test)]
mod tests {
    use super::{own, Own};

    fn terminal(line: &str, then: &str) -> Option<Own> {
        Some(Own::Terminal { line: line.into(), then: then.into() })
    }

    #[test]
    fn claude_codes_screens_open_in_a_terminal_and_its_headless_commands_go_to_it() {
        assert_eq!(own("claude", "config", "/config", &[]), terminal("claude '/config'", ""));
        assert_eq!(own("claude", "login", "/login", &[]), terminal("claude '/login'", ""));
        assert_eq!(own("claude", "statusline", "/statusline", &["statusline".into()]), terminal("claude '/statusline'", ""), "what Claude reported at start");
        assert_eq!(own("claude", "compact", "/compact", &[]), None);
    }

    #[test]
    fn pis_sign_in_and_settings_open_pi_in_a_terminal_its_session_commands_are_refused_and_extensions_go_through() {
        assert_eq!(own("pi", "login", "/login anthropic", &[]), terminal("pi --no-session", "type /login anthropic there"));
        assert_eq!(own("pi", "settings", "/settings", &[]), terminal("pi --no-session", "type /settings there"));
        assert_eq!(own("pi", "compact", "/compact", &[]), Some(Own::NotHosted), "never sent to the model as a message");
        assert_eq!(own("pi", "pilead:mode", "/pilead:mode", &[]), None, "an extension's command runs in the session");
    }

    #[test]
    fn codex_signs_in_with_its_own_program_and_its_session_commands_are_refused() {
        assert_eq!(own("codex", "login", "/login", &[]), terminal("codex login", ""));
        assert_eq!(own("codex", "logout", "/logout", &[]), terminal("codex logout", ""));
        assert_eq!(own("codex", "model", "/model", &[]), Some(Own::NotHosted));
        assert_eq!(own("codex", "fix-tests", "/fix-tests", &[]), None);
    }
}
