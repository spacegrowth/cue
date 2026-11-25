//! A landing that stops goes to the Desk lead, not to you (#483). #456's merge stopped on edits sitting in the main
//! checkout, and the item showed as needing you with git's own text; there was nothing for you to choose. So a stop
//! while finishing (a merge git refused, a clash, a failed check or install, work not on main in time) is said in one
//! plain line, git's words kept with its log; with a live lead it's the lead's: told once, it sends the item back to
//! its holder or clears the way and says DONE to land it again, and the item shows as stopped while it does. Only a
//! real choice (a holder's ask, a step you're asked before, the lead's own ASK) waits on you. Without a live lead the
//! plain line waits on you, as before.
//!
//! Edits in the main checkout that a merge can't go over are named with the session that made them and the item it
//! holds, read from its log.

use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;

/// A stop's question split: its first line, what's wrong in plain words; the lines after it, git's own words.
pub(crate) fn plain_and_said(question: &str) -> (String, String) {
    match question.split_once('\n') {
        Some((plain, said)) => (plain.trim().to_string(), said.trim().to_string()),
        None => (question.trim().to_string(), String::new()),
    }
}

/// `what` failed, git's words for it on the lines after (kept out of what you see; `plain_and_said`).
pub(crate) fn because_plain(what: String, said: &str) -> String {
    if said.trim().is_empty() { what } else { format!("{what}\n{}", said.trim()) }
}

const EDITS: &str = "Not merged: the main checkout has edits of its own in ";
const NOT_COMMITTED: &str = " that aren't committed";

/// The plain line for edits in the main checkout in `files` that a merge can't go over, `by` whom when known.
pub(crate) fn edits_line(files: &[String], by: &[String]) -> String {
    let by = if by.is_empty() { String::new() } else { format!(", made by {}", by.join(" and ")) };
    format!("{EDITS}{}{NOT_COMMITTED}{by}", files.join(", "))
}

/// The files `edits_line` named (None: it's another line).
pub(crate) fn edits_of(line: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix(EDITS)?;
    let (files, _) = rest.split_once(NOT_COMMITTED)?;
    Some(files.split(", ").map(str::to_string).collect())
}

/// The files in main checkout `main` with edits of their own (changed, or new and not in git) that merging `tree`
/// (the merge's result, from `onto`) would change: what's in its way. git's own words are cut to their last lines,
/// so they can't be counted on to list them.
pub(crate) fn in_the_way(main: &str, onto: &str, tree: &str) -> Vec<String> {
    let git = |args: &[&str]| crate::desk::hub::landing::git::git("", main, &[], args).unwrap_or_default();
    let changed: Vec<String> = git(&["diff", "--name-only", "--no-renames", onto, tree]).lines().map(str::to_string).collect();
    let dirty: Vec<String> = git(&["status", "--porcelain", "--untracked-files=all", "--no-renames"]).lines().filter_map(|l| l.get(3..)).map(|p| p.trim_matches('"').to_string()).collect();
    changed.into_iter().filter(|f| dirty.contains(f)).collect()
}

/// What the lead is told of item `id`'s stop.
fn lead_words(id: &str, title: &str, plain: &str, said: &str, holder: Option<&str>) -> String {
    let said = if said.is_empty() { String::new() } else { format!("\ngit said:\n{said}") };
    let back = match holder {
        Some(h) => format!("Send it back with ROUTE: #{id} -> {h} | why: <what to fix>"),
        None => format!("Give it to a session with ROUTE: #{id} -> <session> | why: <what to fix>"),
    };
    format!(
        "Desk item #{id} ({title}) stopped while landing: {plain}.{said}\nIt's yours to settle, not the owner's. {back}, or clear the way yourself and say DONE: #{id} to land it again. ASK: #{id} | … only if it needs the owner's choice."
    )
}

impl Hub {
    /// Item `id`, finishing and stopped on its question, given to its Desk's lead when it has a live one: told once
    /// (with git's words, `said`), the item shown as stopped while the lead has it. Without one it waits on you.
    pub(crate) fn finish_to_lead(&self, id: &str, said: &str) {
        let fx = self.write(|st| {
            let mut fx = crate::desk::hub::core::desk::Effects::default();
            let Some(it) = st.desk.item(id).filter(|i| i.state == "finishing") else { return fx };
            let Some(plain) = it.finish.as_ref().map(|f| f.question.clone()).filter(|q| !q.is_empty()) else { return fx };
            let project = it.project.clone();
            let Some(lead) = st.desk.lead_sid(&project).filter(|_| crate::desk::hub::stall::stall::lead_live(st, &project)).map(str::to_string) else { return fx };
            let holder = it.holder.as_ref().filter(|h| h.kind != "crew").map(|h| st.sessions.get(&h.sid).map(crate::desk::hub::core::desk::shown_name).unwrap_or_else(|| h.name.clone())).filter(|n| !n.is_empty());
            let words = lead_words(id, &it.title, &plain, said, holder.as_deref());
            if let Some(f) = st.desk.item_mut(id).and_then(|i| i.finish.as_mut()) {
                f.lead = true;
            }
            fx.sends.push(crate::desk::hub::core::desk::kinded((lead, words), crate::sessions::Kind::Question));
            fx.items.push(id.to_string());
            fx
        });
        self.desk_done(fx);
    }

    /// Who made the edits in `files` (absolute) that aren't item `it`'s, read from their logs: each session by name,
    /// with the items it holds ("cue-12 on #456"); a change only guessed from a shell command isn't named.
    pub(crate) fn who_changed(&self, it: &DeskItem, files: &[String]) -> Vec<String> {
        let names = self.theirs(it, files).named();
        self.read(|st| {
            names
                .iter()
                .map(|n| {
                    let held: Vec<String> = st
                        .desk
                        .items
                        .iter()
                        .filter(|i| i.id != it.id && matches!(i.state.as_str(), "working" | "queued" | "finishing" | "to_check"))
                        .filter(|i| i.holder.as_ref().is_some_and(|h| st.sessions.get(&h.sid).is_some_and(|s| crate::desk::hub::core::desk::shown_name(s) == *n)))
                        .map(|i| format!("#{}", i.id))
                        .collect();
                    if held.is_empty() { n.clone() } else { format!("{n} on {}", held.join(", ")) }
                })
                .collect()
        })
    }

    /// Each spelling of main checkout `main` a session here uses: logs name files as their session's folder is spelled
    /// (a link like /var for /private/var kept).
    pub(crate) fn spellings_of(&self, main: &str) -> Vec<String> {
        let real = |p: &str| std::fs::canonicalize(p).map(|c| c.to_string_lossy().to_string()).unwrap_or_else(|_| p.to_string());
        let target = real(main);
        let mut spellings: Vec<String> = vec![main.trim_end_matches('/').to_string()];
        let cwds: Vec<String> = self.read(|st| st.sessions.iter().map(|s| s.origin.cwd.clone()).collect());
        for c in cwds {
            let c = c.trim_end_matches('/').to_string();
            if !c.is_empty() && !spellings.contains(&c) && real(&c) == target {
                spellings.push(c);
            }
        }
        spellings
    }

    /// A merge into the main checkout refused (`question`): when edits there were in its way, its plain line names
    /// who made them.
    pub(crate) fn main_edits_named(&self, it: &DeskItem, main: &str, question: &str) -> String {
        let (plain, said) = plain_and_said(question);
        let Some(files) = edits_of(&plain) else { return question.to_string() };
        let spellings = self.spellings_of(main);
        let abs: Vec<String> = spellings.iter().flat_map(|m| files.iter().map(move |f| format!("{m}/{f}"))).collect();
        because_plain(edits_line(&files, &self.who_changed(it, &abs)), &said)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_words_are_split_off_and_the_edits_line_read_back() {
        let q = "Not merged: git wouldn't merge desk/4 into main\nerror: Your local changes would be overwritten / Aborting";
        let (plain, said) = plain_and_said(q);
        assert_eq!(plain, "Not merged: git wouldn't merge desk/4 into main");
        assert!(said.starts_with("error: Your local changes"));
        assert_eq!(plain_and_said("Build failed"), ("Build failed".into(), String::new()));
        let files = vec!["a.txt".to_string(), "src/b.rs".to_string()];
        let line = edits_line(&files, &[]);
        assert_eq!(line, "Not merged: the main checkout has edits of its own in a.txt, src/b.rs that aren't committed");
        assert_eq!(edits_of(&line), Some(files));
        assert_eq!(edits_of(&plain), None);
    }
}
