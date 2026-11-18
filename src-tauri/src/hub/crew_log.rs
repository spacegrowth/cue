//! The crew's log on its lead's record (sessions/crew_log.rs): Cue's own steps where it acts by itself (a review
//! filed, fixes sent back, a commit made or held, a member set aside), the lead's judgment calls read off its replies
//! (`DECIDED:` lines), and Undo this: the lead told to undo a commit, a set-aside or a call.

use super::*;
use crate::sessions::crew_log::{go_back_words, Decided};
use crate::sessions::team::{fix_findings, Teammate};

impl Hub {
    /// `d` noted on lead `lead`'s crew log, and drawn.
    pub(super) fn crew_note(&self, lead: &str, d: Decided) {
        self.write(|st| st.sessions.crew_decided(lead, now_ms(), d));
        self.changed();
    }

    /// Member `call` of `lead` as it is now, when it's still on the lead's record.
    fn crew_member(&self, lead: &str, call: &str) -> Option<Teammate> {
        self.read(|st| st.sessions.team(lead)).into_iter().find(|t| t.call == call)
    }

    /// Cue sent member `call`'s findings back by itself (`r`: what Send fixes said, or why it couldn't).
    pub(super) fn crew_fixes_noted(&self, lead: &str, call: &str, r: &Result<String, String>) {
        let Some(t) = self.crew_member(lead, call) else { return };
        let d = match r {
            Ok(said) if said.starts_with("Cue sends") => Decided::about(&t, "fixes", "sent back once its lead is free"),
            Ok(_) => Decided::about(&t, "fixes", "sent back"),
            Err(e) => Decided { why: e.clone(), ..Decided::about(&t, "hold", "fixes not sent") },
        };
        self.crew_note(lead, d);
    }

    /// Member `t`'s work committed (`said`: what Commit said). By Cue (`yours` false): always noted; by your Commit:
    /// only as the end of a line Cue started about it (its review that waited for you).
    pub(super) fn crew_committed(&self, lead: &str, t: &Teammate, said: &str, yours: bool) {
        if yours && !self.read(|st| st.sessions.crew_open(lead, &t.call)) {
            return;
        }
        let sha = self.crew_member(lead, &t.call).map(|m| m.commit).unwrap_or_default();
        let branch = branch_in(said);
        let step = branch.as_deref().map_or("committed".to_string(), |b| format!("committed to `{b}`"));
        // One-way as its project counts it (Ask me before; by default a commit on main or master).
        let one_way = branch.as_deref().is_some_and(|b| crate::one_way::commit_one_way(&crate::one_way::kinds(&self.project_shown(lead)), b));
        self.crew_note(lead, Decided { sha, one_way, ..Decided::about(t, "commit", &step) });
    }

    /// Lead `sid`'s turn ended with its reply (`all`: everything it wrote this turn): its `DECIDED:` judgment calls
    /// noted on its crew log at the reply's time (the newest of its replies in its chat, so each sits right after it).
    pub(super) fn crew_calls_noted(&self, sid: &str, all: &str) {
        if !all.to_ascii_lowercase().contains("decided") {
            return;
        }
        let noted = self.write(|st| {
            let at = st.sessions.thread(sid).iter().rev().find(|e| e.role == "agent").map_or_else(now_ms, |e| e.at_ms);
            st.sessions.crew_calls(sid, at, all)
        });
        if noted > 0 {
            self.changed();
        }
    }

    /// The project session `sid` shows under (`project_shown_for`); "" when Cue doesn't know it.
    fn project_shown(&self, sid: &str) -> String {
        self.read(|st| st.sessions.origin(sid)).map(|o| project_shown_for(&o)).unwrap_or_default()
    }

    /// Cue's commit of member `t` didn't go (`e`): held, as its row says.
    pub(super) fn crew_held(&self, lead: &str, t: &Teammate, e: &str) {
        let d = match t.also.first() {
            Some(other) => Decided::about(t, "hold", &format!("commit held: `{other}` has changes in its files")),
            None => Decided { why: e.lines().next().unwrap_or("").to_string(), ..Decided::about(t, "hold", "not committed") },
        };
        self.crew_note(lead, d);
    }

    /// Undo this: lead `sid` is told to undo its decision `id` (through the normal send path: kept in Cue while
    /// it's busy), and the decision is marked so. What Cue did; Err for one it doesn't know, one that's activity (a
    /// step, nothing to undo), or one you went back on already.
    pub fn crew_go_back(&self, sid: &str, id: &str) -> Result<String, String> {
        let d = self.write(|st| st.sessions.crew_go_back(sid, id))?;
        self.changed();
        match self.send_to_session(sid, &go_back_words(&d), &[], false) {
            Ok(said) if super::team::kept(&said) => Ok("Cue tells its lead to undo this when its turn ends".into()),
            Ok(_) => Ok("Asked its lead to undo this".into()),
            Err(e) => {
                self.write(|st| st.sessions.crew_go_back_failed(sid, id));
                self.changed();
                Err(e)
            }
        }
    }
}

/// The project the session of origin `o` shows under, as the window has it (hub/mod.rs settle_projects): moved there
/// in Cue, else its folder's (put in a project by you, or its repository's own).
pub(super) fn project_shown_for(o: &Origin) -> String {
    match crate::config::moved().remove(&o.session_id) {
        Some(to) => to,
        None => crate::model::project_at(&o.cwd, &o.machine),
    }
}

/// The branch Commit's first line says it committed to ("… to crew/rows (3f2a1c)…", "… on team-work (3f2a1c)").
fn branch_in(said: &str) -> Option<String> {
    let line = said.lines().next()?.trim_start_matches("In ");
    [" to ", " on "].iter().find_map(|key| {
        let (_, rest) = line.split_once(key)?;
        let b = rest.split(" (").next()?.trim();
        (!b.is_empty() && !b.contains(' ')).then(|| b.to_string())
    })
}

/// What a review found, in a few words: "2 should-fix", "clean", "1 note".
fn review_found(findings: &[String], recommendation: &str) -> String {
    let n = fix_findings(findings);
    match findings.len() {
        _ if n > 0 => format!("{n} should-fix"),
        0 if recommendation.contains("fix") => "fixes asked, none listed".into(),
        0 => "clean".into(),
        1 => "1 note".into(),
        k => format!("{k} notes"),
    }
}

/// The line a review filed to member `t` notes: a new one ("Reviewed: 2 should-fix"), or, on its open line (a round of
/// fixes went back), "reviewed again: …" (nothing when it's clean and goes on to its commit).
pub(super) fn review_decided<'a>(t: &'a Teammate, findings: &[String], recommendation: &str, clean_commit: bool) -> Decided<'a> {
    let found = review_found(findings, recommendation);
    let step = if clean_commit { String::new() } else { format!("reviewed again: {found}") };
    Decided { head: format!("Reviewed: {found}"), step, ..Decided::about(t, "review", "") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_branch_is_read_off_what_commit_said() {
        assert_eq!(branch_in("Committed 1 file to crew/rows-2 (abc1234); your folder stays on main.").as_deref(), Some("crew/rows-2"));
        assert_eq!(branch_in("Committed rows's 2 files on team-work (abc1234)").as_deref(), Some("team-work"));
        assert_eq!(branch_in("Committed rows's 2 files (abc1234)"), None);
        assert_eq!(review_found(&["1. a — should-fix — x".into(), "2. b — note — y".into()], "commit"), "1 should-fix");
        assert_eq!(review_found(&[], "send fixes"), "fixes asked, none listed");
        assert_eq!(review_found(&["2. b — note — y".into()], "commit"), "1 note");
    }
}
