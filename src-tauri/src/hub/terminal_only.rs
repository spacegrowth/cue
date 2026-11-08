//! Claude Code's own screens, for a hosted session (docs/dev/cue-hosted-sessions.md §9, H5): commands only its
//! terminal interface has (`/config`, `/login`, `/theme`…). Claude says which at start (`terminal_slash_commands`);
//! a few are never offered headless at all (signing in). Sent to a hosted session one would do nothing, so Cue opens
//! it in a terminal tab instead: `claude /config` in the session's folder (on its machine, over SSH), a terminal of
//! its own, never on the session's conversation (two processes on one conversation would both write it).

use super::*;
use crate::terminal::launch::Place;

/// Claude Code's commands that need its terminal whatever it reports: signing in and out, and its own settings
/// screens.
const ALWAYS: [&str; 8] = ["login", "logout", "config", "theme", "terminal-setup", "doctor", "ide", "vim"];

impl Hub {
    /// The commands Claude said only its terminal has, at the session's start.
    pub(crate) fn note_terminal_only(&self, session_id: &str, names: Vec<String>) {
        self.write(|st| st.terminal_only.insert(session_id.to_string(), names));
    }

    /// Whether `/name` needs a terminal in hosted session `session_id`.
    pub(crate) fn needs_terminal(&self, session_id: &str, name: &str) -> bool {
        ALWAYS.contains(&name) || self.read(|st| st.terminal_only.get(session_id).is_some_and(|l| l.iter().any(|n| n == name)))
    }

    /// `text`'s command, when it's one a hosted Claude Code session can only run in a terminal.
    pub(crate) fn terminal_command(&self, session_id: &str, text: &str) -> Option<String> {
        let origin = self.hosted_origin(session_id).filter(|o| o.harness == "claude")?;
        let name = text.trim().strip_prefix('/')?.split_whitespace().next()?.to_string();
        self.needs_terminal(&origin.session_id, &name).then_some(name)
    }

    /// `text` (a command of Claude Code's own screens) run in a terminal tab: `claude <text>` in the session's
    /// folder, here or on its machine. What it opened.
    pub(crate) fn open_terminal_command(&self, session_id: &str, text: &str, name: &str) -> Result<String, String> {
        let o = self.hosted_origin(session_id).ok_or("that session is gone")?;
        let run = format!("claude {}", crate::machines::shq(text.trim()));
        if o.machine.is_empty() {
            let tab = self.launcher().here(&run, &o.cwd, &format!("/{name}"), Place::chosen(Some(false)))?;
            return Ok(format!("Opened /{name} in {}: it's one of Claude Code's own screens, which only a terminal shows", tab.what));
        }
        let m = crate::machines::get(&o.machine).ok_or(format!("{} isn't one of your machines any more", o.machine))?;
        let there = crate::machines::remote_command_err(&format!("cd {} && exec {run}", crate::machines::remote_dir(&o.cwd)));
        let line = format!("ssh {} -t {} {}", crate::machines::shared().join(" "), m.host, crate::machines::shq(&there));
        let tab = crate::terminal::focus::open_tab_with(&line)?;
        Ok(format!("Opened /{name} on {} in {}: it's one of Claude Code's own screens, which only a terminal shows", crate::machines::display_name(&m.name), tab.what))
    }
}

#[cfg(test)]
mod tests {
    use super::ALWAYS;

    #[test]
    fn signing_in_and_claude_codes_settings_screens_always_need_its_terminal() {
        for c in ["login", "config", "theme"] {
            assert!(ALWAYS.contains(&c), "{c}");
        }
        assert!(!ALWAYS.contains(&"compact"), "a command that runs headless isn't sent elsewhere");
    }
}
