//! How work goes out to a hosted lead's team (docs/dev/cue-teams.md §6, T4, T5): the team skill every hosted Claude
//! lead starts with, the Team switch on its box (auto, on, off; `/team` for one message), a task dragged onto the
//! lead to hand out as a new teammate (with the files it would likely touch that a teammate already touches), and the
//! team carried over to the lead that carries on after a handoff. The user never types any of it.

use super::*;
use crate::sessions::team::carried_note;

/// The team skill's text, with `$more` (rules the optional add-on adds; "" without it) after its first rule.
macro_rules! team_skill {
    ($($more:tt)*) => {
        concat!("# Working as a team lead (Cue)
You run inside Cue, which shows each teammate you start under you; the user looks over and commits their work there.
- When a task has two or more independent parts (different files, no part needing another's result) and each is real work, not a one-line change, don't do them yourself: start one teammate per part with the Agent tool, each with a short plain `name` for its part (letters, digits, - or _, no spaces, like \"parser-tests\"), `run_in_background: true` and `isolation: \"worktree\"`.
", $($more)*, "- One teammate per part, never two on the same files. Tell each what to do, which files are its own, and not to commit.
- Don't set a `model`: teammates run on yours.
- Then wait: you're told as each one finishes. When all have, say in plain words what each did. Don't commit their work.
- One part, small quick changes, or parts that share files: do it yourself.
- A message that ends with \"(Team: on …)\" or \"(Team: off …)\" is the user's choice for that message.")
    };
}
#[cfg(feature = "ext")]
use super::ext::team_rules as more_rules;
#[cfg(not(feature = "ext"))]
macro_rules! more_rules {
    () => {
        ""
    };
}

/// The team skill, appended to a hosted Claude lead's system prompt at every start (`--append-system-prompt`): when
/// a task splits, the lead forms a team on its own. Kept short: it's in every turn's context.
pub(crate) const TEAM_SKILL: &str = team_skill!(more_rules!());

/// The Team switch's words after your message: "on" splits it among teammates, "off" keeps the lead on it alone;
/// auto adds nothing (the skill decides).
const TEAM_ON: &str = "(Team: on. Split this into its independent parts and hand each to its own named background teammate, as your team instructions say.)";
const TEAM_OFF: &str = "(Team: off. Do this yourself: start no teammates.)";

/// `args` (yours) with the team skill added: Claude Code's only (Pi and Codex have no Agent tool).
pub(super) fn with_skill(agent: crate::hosted::Agent, args: &[String]) -> Vec<String> {
    let mut out = args.to_vec();
    if agent != crate::hosted::Agent::Claude {
        return out;
    }
    const FLAG: &str = "--append-system-prompt";
    // Yours already appends a prompt: the skill goes after your text in the same option (Claude takes one), so both
    // reach it. A prompt appended from a file (`--append-system-prompt-file`) can't be merged: the skill is left out.
    if out.iter().any(|a| a == "--append-system-prompt-file" || a.starts_with("--append-system-prompt-file=")) {
        return out;
    }
    if let Some(i) = out.iter().position(|a| a == FLAG) {
        if let Some(yours) = out.get_mut(i + 1) {
            *yours = format!("{yours}\n\n{TEAM_SKILL}");
            return out;
        }
        out.push(TEAM_SKILL.into());
        return out;
    }
    if let Some(yours) = out.iter_mut().find(|a| a.starts_with("--append-system-prompt=")) {
        *yours = format!("{yours}\n\n{TEAM_SKILL}");
        return out;
    }
    out.extend([FLAG.to_string(), TEAM_SKILL.to_string()]);
    out
}

/// `typed` as it goes to a lead whose Team switch is `mode`: "/team …" is on for that one message (the command taken
/// off); a command or a shell line goes as typed.
pub(crate) fn with_team(typed: &str, mode: &str) -> String {
    let t = typed.trim();
    let (rest, mode) = match t.strip_prefix("/team") {
        Some(r) if r.is_empty() || r.starts_with(char::is_whitespace) => (r.trim(), "on"),
        _ if t.starts_with('/') || t.starts_with('!') => return typed.to_string(),
        _ => (t, mode),
    };
    match mode {
        _ if rest.is_empty() => String::new(),
        "on" => format!("{rest}\n\n{TEAM_ON}"),
        "off" => format!("{rest}\n\n{TEAM_OFF}"),
        _ => typed.to_string(),
    }
}

/// The message that hands a dragged task to the lead as a new teammate.
fn hand_out_text(task: &str) -> String {
    format!("Hand this to a new teammate: one named background teammate with `isolation: \"worktree\"`, as your team instructions say. Don't do it yourself, and don't hold it for your current work.\n\n{}", task.trim())
}

/// SHORTCUT: a task's files are only the paths it names, so a task that names none shows nothing taken. Upgrade
/// path (the canvas's "a guess from the task and the files nearby"): ask a small model for the files it would touch.
/// File paths named in `text` ("ui/board.js", "`src/hub/team.rs:40`", "a.txt"): a word with an extension that has a
/// letter in it, without quotes, backticks, a line number or the punctuation after it. Not a link or a version.
pub(crate) fn paths_named(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for w in text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '[' | ']' | '"' | '\'' | '`' | '<' | '>')) {
        if w.contains("://") {
            continue;
        }
        let w = w.trim_start_matches('*').trim_end_matches(|c: char| matches!(c, '.' | ':' | '!' | '?' | '*'));
        // "file.rs:40" or "file.rs:40:2": its line off.
        let w = w.split(':').next().unwrap_or("");
        let w = w.strip_prefix("./").unwrap_or(w);
        let Some((stem, ext)) = w.rsplit_once('.') else { continue };
        let name = stem.rsplit('/').next().unwrap_or("");
        // "e.g", "i.e": an abbreviation, not a file.
        let ok = !name.is_empty()
            && !(name.len() == 1 && ext.len() == 1 && !w.contains('/'))
            && (1..=6).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
            && w.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
            && !stem.chars().all(|c| c.is_ascii_digit() || c == '.');
        if ok && !out.iter().any(|o| o == w) {
            out.push(w.to_string());
        }
    }
    out
}

/// Whether a path named in a task and a file someone touches are the same file: equal, or one ends the other at a
/// folder ("board.js" is "ui/board.js").
fn same_file(named: &str, touched: &str) -> bool {
    named == touched || touched.ends_with(&format!("/{named}")) || named.ends_with(&format!("/{touched}"))
}

impl Hub {
    /// The Team switch on `sid`'s box: "on", "off", or "auto".
    pub fn set_team_mode(&self, sid: &str, mode: &str) -> Result<(), String> {
        if !matches!(mode, "on" | "off" | "auto") {
            return Err(format!("“{mode}” isn't a Team setting"));
        }
        if !self.write(|st| st.sessions.set_team_mode(sid, mode)) {
            return Err("that session is gone".into());
        }
        self.changed();
        Ok(())
    }

    /// A message you typed in a session's box: to a hosted Claude lead with its Team switch's words (`with_team`).
    pub fn send_yours(&self, sid: &str, text: &str, images: &[crate::model::Upload], now: bool) -> Result<String, String> {
        let lead = self.read(|st| st.sessions.origin(sid)).filter(|o| o.hosted && o.harness == "claude");
        let text = match lead {
            Some(_) => with_team(text, &self.read(|st| st.sessions.team_mode(sid))),
            None => text.to_string(),
        };
        self.send_to_session(sid, &text, images, now)
    }

    /// Before a dragged task goes to lead `sid` as a new teammate: the files it names, each with who touches it now
    /// (a teammate still in the lead's team, or the optional add-on's work in its folder; "" when it's free), and
    /// whether any is taken.
    #[cfg_attr(not(feature = "ext"), allow(unused_mut, unused_variables))]
    pub fn hand_out_preview(&self, sid: &str, task: &str) -> Result<Value, String> {
        let (origin, team) = self.read(|st| (st.sessions.origin(sid), st.sessions.team(sid)));
        let origin = origin.filter(|o| o.hosted && o.harness == "claude").ok_or("only a hosted Claude Code lead takes a teammate")?;
        let mut touched: Vec<(String, String)> = team.iter().filter(|t| t.in_team()).flat_map(|t| t.files.iter().map(move |f| (f.clone(), format!("{} is changing it", t.name)))).collect();
        #[cfg(feature = "ext")]
        touched.extend(self.ext_touching(&origin.cwd));
        let files: Vec<Value> = paths_named(task)
            .into_iter()
            .map(|p| {
                let by = touched.iter().find(|(f, _)| same_file(&p, f)).map(|(_, who)| who.clone()).unwrap_or_default();
                json!({ "path": p, "by": by })
            })
            .collect();
        let overlap = files.iter().any(|f| f["by"].as_str().is_some_and(|b| !b.is_empty()));
        Ok(json!({ "lead": self.session_name(sid), "files": files, "overlap": overlap }))
    }

    /// A dragged task sent to lead `sid` to hand out as a new teammate (queued while it works, as anything you send).
    pub fn hand_out(&self, sid: &str, task: &str) -> Result<String, String> {
        if task.trim().is_empty() {
            return Err("nothing to hand out".into());
        }
        self.read(|st| st.sessions.origin(sid)).filter(|o| o.hosted && o.harness == "claude").ok_or("only a hosted Claude Code lead takes a teammate")?;
        self.send_to_session(sid, &hand_out_text(task), &[], false)?;
        Ok(format!("Handed to {} as a new teammate", self.session_name(sid)))
    }

    /// A handoff: the lead `to` that carries on from `from` takes its team (sessions/team.rs `carried`) and is told
    /// who is in it, after its first message. Nothing when `from` had no team.
    pub(crate) fn carry_team(&self, from: &str, to: &str) {
        let took = self.write(|st| st.sessions.carry_team(from, to));
        if took.is_empty() {
            return;
        }
        self.changed();
        if let Err(e) = self.send_to_session(to, &carried_note(&took), &[], false) {
            eprintln!("cue: couldn't tell {to} about the team it carries on with: {e}");
        }
    }
}

/// Two folders are one when equal, trailing slashes aside.
#[cfg_attr(not(feature = "ext"), allow(dead_code))]
pub(super) fn same_folder(a: &str, b: &str) -> bool {
    !a.is_empty() && a.trim_end_matches('/') == b.trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_team_switch_adds_its_words_and_slash_team_is_on_for_one_message() {
        assert_eq!(with_team("fix it", ""), "fix it", "auto: as typed");
        assert_eq!(with_team("fix it", "on"), format!("fix it\n\n{TEAM_ON}"));
        assert_eq!(with_team("fix it", "off"), format!("fix it\n\n{TEAM_OFF}"));
        assert_eq!(with_team("/team fix a and b", ""), format!("fix a and b\n\n{TEAM_ON}"));
        assert_eq!(with_team("/team fix a and b", "off"), format!("fix a and b\n\n{TEAM_ON}"), "/team wins over the switch");
        assert_eq!(with_team("/compact", "on"), "/compact", "a command goes as typed");
        assert_eq!(with_team("! ls", "on"), "! ls");
        assert_eq!(with_team("/teams", "on"), "/teams", "another command");
    }

    #[test]
    fn the_skill_is_appended_once_merged_after_a_prompt_you_append_yourself() {
        use crate::hosted::Agent;
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let flag = "--append-system-prompt";
        assert_eq!(with_skill(Agent::Claude, &v(&["--verbose"])), v(&["--verbose", flag, TEAM_SKILL]));
        let mine = format!("Be terse.\n\n{TEAM_SKILL}");
        assert_eq!(with_skill(Agent::Claude, &v(&[flag, "Be terse.", "--verbose"])), v(&[flag, &mine, "--verbose"]), "merged into yours, one option");
        assert_eq!(with_skill(Agent::Claude, &v(&["--append-system-prompt=Be terse."])), v(&[&format!("--append-system-prompt=Be terse.\n\n{TEAM_SKILL}")]));
        assert_eq!(with_skill(Agent::Claude, &v(&[flag])), v(&[flag, TEAM_SKILL]), "the option with no text yet");
        assert_eq!(with_skill(Agent::Claude, &v(&["--append-system-prompt-file", "p.md"])), v(&["--append-system-prompt-file", "p.md"]), "a file: left as yours");
        assert_eq!(with_skill(Agent::Codex, &v(&["-x"])), v(&["-x"]), "Claude only");
    }

    #[test]
    fn the_skills_example_name_is_one_claude_code_takes_and_each_teammate_has_a_worktree() {
        // Claude Code refuses an Agent `name` outside ^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$ (an InputValidationError).
        let takes = |n: &str| n.len() <= 64 && n.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        let example = TEAM_SKILL.split("like \"").nth(1).and_then(|s| s.split('"').next()).unwrap();
        assert_eq!(example, "parser-tests");
        assert!(takes(example) && !takes("parser tests"));
        assert!(TEAM_SKILL.contains("no spaces"));
        // Each teammate in a worktree of its own.
        assert!(TEAM_SKILL.contains("`run_in_background: true` and `isolation: \"worktree\"`"));
    }

    #[test]
    fn a_tasks_files_are_the_paths_it_names() {
        assert_eq!(paths_named("Fix the typo in a.txt and add a line to `b.txt`."), ["a.txt", "b.txt"]);
        assert_eq!(paths_named("see ui/board.js:40 and (src-tauri/src/hub/team.rs), ./README.md"), ["ui/board.js", "src-tauri/src/hub/team.rs", "README.md"]);
        assert!(paths_named("bump 0.1.22 to 1.2, see https://example.com/a.html, e.g. this. Done.").is_empty());
        assert!(same_file("board.js", "ui/board.js") && !same_file("oard.js", "ui/board.js"));
    }
}
