//! How work goes out to a hosted lead's team (docs/dev/cue-teams.md §6, T4, T5): the team skill every hosted Claude
//! lead starts with, the Crew switch on its box (auto, on, off; `/crew` for one message), a task dragged onto the
//! lead to hand out as a new teammate (with the files it would likely touch that a teammate already touches), and the
//! team carried over to the lead that carries on after a handoff. The user never types any of it.

use crate::hub::*;
use crate::sessions::team::carried_note;

/// The team skill's text, with `$more` (rules the optional add-on adds; "" without it) after its first rule.
macro_rules! team_skill {
    ($($more:tt)*) => {
        concat!("# Working as a crew lead (Cue)
You run inside Cue, which shows each teammate you start under you as your crew; the user looks over and commits their work there.
- When a task has two or more independent parts (different files, no part needing another's result) and each is real work, not a one-line change, don't do them yourself: start one teammate per part with the Agent tool, each with a short plain `name` for its part (letters, digits, - or _, no spaces, like \"parser-tests\"), `run_in_background: true` and `isolation: \"worktree\"`.
", $($more)*, "- One teammate per part, never two on the same files. Tell each what to do, which files are its own, not to commit, and to open each image (a mock, a screenshot) once and note what it needs from it.
- Brief each teammate to work in big steps, since every turn re-reads its whole context: read what it needs in one command, not one file or grep per turn; run tests once at the end, not after each edit; past about 80k of context, end the step there, so the next step goes to a fresh teammate. A design teammate reads only the canvas page it changes, not the whole artifact.
- Don't set a `model`: teammates run on yours.
- Then wait: you're told as each one finishes. When all have, say in plain words what each did. Don't commit their work.
- Look over a teammate's work at most twice: once, and once more after its fixes. Do it with a fresh agent given a short brief, never a fork: a fork starts with your whole context.
- One part that will take long (more than about 20 minutes of work): hand it to one teammate too, so you stay free to answer the user while it runs. A list of steps goes out one step per teammate, the next started fresh when the one before ends, never all to one teammate.
- One short part, small quick changes, or parts that share files: do it yourself.
- A message that ends with \"(Crew: on …)\" or \"(Crew: off …)\" is the user's choice for that message.
- When you make a judgment call the user didn't settle (one approach over another, skipping a should-fix, cutting scope, committing despite a note, not testing something), write it in your reply on a line of its own: `DECIDED: what you chose — why`. Cue shows the user each one, and they can ask you to undo it. Not for routine steps: starting a teammate, sending fixes back, a commit you were asked for. If your task asks you to write judgment calls another way (as `DECISION:` lines), write them only that way, never as DECIDED: too.")
    };
}
#[cfg(feature = "ext")]
use crate::hub::ext::team_rules as more_rules;
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
const TEAM_ON: &str = "(Crew: on. Split this into its independent parts and hand each to its own named background teammate, as your team instructions say.)";
const TEAM_OFF: &str = "(Crew: off. Do this yourself: start no teammates.)";

/// The team skill for a lead in a project whose Ask me before is `kinds` (one_way.rs): the skill, then what the lead
/// asks the user first before there (nothing more when nothing's one-way there).
pub(crate) fn skill_for(kinds: &[&str]) -> String {
    let ask = crate::one_way::ask_first(kinds);
    if ask.is_empty() { TEAM_SKILL.to_string() } else { format!("{TEAM_SKILL}\n{ask}") }
}

/// `args` (yours) with the team skill added (`skill`, skill_for's): Claude Code's only (Pi and Codex have no Agent
/// tool).
pub(in crate::hub) fn with_skill(agent: crate::hosted::Agent, args: &[String], skill: &str) -> Vec<String> {
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
            *yours = format!("{yours}\n\n{skill}");
            return out;
        }
        out.push(skill.into());
        return out;
    }
    if let Some(yours) = out.iter_mut().find(|a| a.starts_with("--append-system-prompt=")) {
        *yours = format!("{yours}\n\n{skill}");
        return out;
    }
    out.extend([FLAG.to_string(), skill.to_string()]);
    out
}

/// `typed` as it goes to a lead whose Crew switch is `mode`: "/crew …" is on for that one message (the command taken
/// off); a command or a shell line goes as typed.
pub(crate) fn with_team(typed: &str, mode: &str) -> String {
    let t = typed.trim();
    let (rest, mode) = match t.strip_prefix("/crew") {
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
/// File paths named in `text` ("ui/board.js", "`src/hub/team/mod.rs:40`", "a.txt"): a word with an extension that has a
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
            return Err(format!("“{mode}” isn't a Crew setting"));
        }
        if !self.write(|st| st.sessions.set_team_mode(sid, mode)) {
            return Err("that session is gone".into());
        }
        self.changed();
        Ok(())
    }

    /// What happens to a crew member's work once its review is clean: "clean" commits it on a branch of its own,
    /// "wait" leaves Commit to you.
    pub fn set_crew_commit(&self, sid: &str, mode: &str) -> Result<(), String> {
        // "auto": Cue handles each review itself (the default; "clean", its old name, too); "wait": ask you every time.
        if !matches!(mode, "auto" | "clean" | "wait") {
            return Err(format!("“{mode}” isn't a teammates setting"));
        }
        if !self.write(|st| st.sessions.set_crew_commit(sid, mode)) {
            return Err("that session is gone".into());
        }
        self.changed();
        Ok(())
    }

    /// Which finished crew members Cue reviews by itself: "always", "code" (the default: only one whose files Cue
    /// sees changed in a git repository), or "never" (Review by hand).
    pub fn set_crew_review(&self, sid: &str, mode: &str) -> Result<(), String> {
        if !matches!(mode, "always" | "code" | "never") {
            return Err(format!("“{mode}” isn't a review setting"));
        }
        if !self.write(|st| st.sessions.set_crew_review(sid, mode)) {
            return Err("that session is gone".into());
        }
        self.changed();
        Ok(())
    }

    /// A message you typed in a session's box: to a hosted Claude lead with its Team switch's words (`with_team`).
    pub fn send_yours(&self, sid: &str, text: &str, images: &[crate::model::Upload], now: bool) -> Result<String, String> {
        let sent = self.send_to_session(sid, &self.team_words(sid, text), images, now)?;
        // What it asked you on its items is answered (#475).
        self.desk_you_answered(sid);
        Ok(sent)
    }

    /// What you typed to `sid`, from its box or its reply box: with its Team switch's words when it's a hosted Claude
    /// lead (`with_team`), else as typed. A Desk lead has no crew, so its messages go as typed.
    pub(crate) fn team_words(&self, sid: &str, text: &str) -> String {
        let lead = self.read(|st| st.sessions.origin(sid).filter(|o| o.hosted && o.harness == "claude" && st.desk.lead_of(sid).is_none()));
        match lead {
            Some(_) => with_team(text, &self.read(|st| st.sessions.team_mode(sid))),
            None => text.to_string(),
        }
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
        Ok(format!("Handed to {}'s crew", self.session_name(sid)))
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
pub(in crate::hub) fn same_folder(a: &str, b: &str) -> bool {
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
        assert_eq!(with_team("/crew fix a and b", ""), format!("fix a and b\n\n{TEAM_ON}"));
        assert_eq!(with_team("/crew fix a and b", "off"), format!("fix a and b\n\n{TEAM_ON}"), "/crew wins over the switch");
        assert_eq!(with_team("/compact", "on"), "/compact", "a command goes as typed");
        assert_eq!(with_team("! ls", "on"), "! ls");
        assert_eq!(with_team("/crews", "on"), "/crews", "another command");
        assert_eq!(with_team("/team fix it", "off"), "/team fix it", "not ours any more: as typed");
    }

    #[test]
    fn the_skill_is_appended_once_merged_after_a_prompt_you_append_yourself() {
        use crate::hosted::Agent;
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let flag = "--append-system-prompt";
        assert_eq!(with_skill(Agent::Claude, &v(&["--verbose"]), TEAM_SKILL), v(&["--verbose", flag, TEAM_SKILL]));
        let mine = format!("Be terse.\n\n{TEAM_SKILL}");
        assert_eq!(with_skill(Agent::Claude, &v(&[flag, "Be terse.", "--verbose"]), TEAM_SKILL), v(&[flag, &mine, "--verbose"]), "merged into yours, one option");
        assert_eq!(with_skill(Agent::Claude, &v(&["--append-system-prompt=Be terse."]), TEAM_SKILL), v(&[&format!("--append-system-prompt=Be terse.\n\n{TEAM_SKILL}")]));
        assert_eq!(with_skill(Agent::Claude, &v(&[flag]), TEAM_SKILL), v(&[flag, TEAM_SKILL]), "the option with no text yet");
        assert_eq!(with_skill(Agent::Claude, &v(&["--append-system-prompt-file", "p.md"]), TEAM_SKILL), v(&["--append-system-prompt-file", "p.md"]), "a file: left as yours");
        assert_eq!(with_skill(Agent::Codex, &v(&["-x"]), TEAM_SKILL), v(&["-x"]), "Claude only");
    }

    #[test]
    fn the_lead_says_its_judgment_calls_and_asks_first_before_what_its_project_counts_one_way() {
        // When to write DECIDED:, and when not; in the form Cue reads (sessions/crew_log.rs calls_in).
        assert!(TEAM_SKILL.contains("`DECIDED: what you chose — why`") && TEAM_SKILL.contains("Not for routine steps"), "{TEAM_SKILL}");
        // A task that gives its own form for calls (the add-on's asks for `DECISION:` lines): that form only, never both.
        assert!(TEAM_SKILL.contains("(as `DECISION:` lines), write them only that way, never as DECIDED: too."), "{TEAM_SKILL}");
        let example = TEAM_SKILL.split('`').find(|s| s.starts_with("DECIDED:")).unwrap();
        assert!(crate::sessions::crew_log::calls_in(example).is_empty(), "the instructions' own example, said back, isn't a call");
        assert_eq!(crate::sessions::crew_log::calls_in(&example.replace("what you chose", "kept the cache")).len(), 1, "in the form Cue reads");
        // Its project's Ask me before after the skill: the default asks before a push to main.
        let default = skill_for(crate::one_way::DEFAULT);
        assert!(default.starts_with(TEAM_SKILL) && default.ends_with("ask the user first, and end your turn, before a push to main or master."), "{default}");
        assert!(skill_for(&["deploy"]).ends_with("before a deploy."));
        assert_eq!(skill_for(&[]), TEAM_SKILL, "nothing one-way there: the skill alone");
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
        assert_eq!(paths_named("see ui/board.js:40 and (src-tauri/src/hub/team/mod.rs), ./README.md"), ["ui/board.js", "src-tauri/src/hub/team/mod.rs", "README.md"]);
        assert!(paths_named("bump 0.1.22 to 1.2, see https://example.com/a.html, e.g. this. Done.").is_empty());
        assert!(same_file("board.js", "ui/board.js") && !same_file("oard.js", "ui/board.js"));
    }
}
