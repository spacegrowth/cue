//! An item whose code lives in another repository on this Mac (#635). The lead says so on its route:
//! `ROUTE: #<id> -> new | repo: <folder or project> | why: …`. Cue then starts the new session in that repository (or,
//! routed to a session by name, gives that session a copy of it), makes the item's copy of that repository
//! (`<its main checkout>/.claude/worktrees/desk-<n>`, branch `desk/<id>`), and at its DONE lands it on that repository's
//! main line, as any item lands on its own (desk/hub/landing/trees.rs). Its landing follows that repository's project:
//! its check, how it lands and what runs after; its never-commit words and the Desk's own both apply.
//!
//! Unlike COPY (desk/hub/landing/named_copy.rs), which keeps work on a branch the lead names, this lands on the main
//! line. Only on this Mac: a session on another machine works in its own clone (desk/hub/landing/remote_item.rs).

use crate::desk::hub::landing::main_line::main_checkout;
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, InRepo};

/// What a ROUTE's `repo:` names, as said and as found: a repository's main checkout and its project, or why not.
pub(crate) type Found = Vec<(String, Result<InRepo, String>)>;

impl Hub {
    /// The repositories a reply's ROUTE lines name with `repo:`, each looked up (`repo_of`). Read before the store is
    /// taken: it looks at the disk.
    pub(crate) fn repos_routed(&self, text: &str) -> Found {
        let mut out: Found = Vec::new();
        for line in crate::desk::parse::lines(text) {
            if let crate::desk::parse::Line::Route { repo: Some(repo), .. } = line {
                if !out.iter().any(|(r, _)| *r == repo) {
                    let found = self.repo_of(&repo);
                    out.push((repo, found));
                }
            }
        }
        out
    }

    /// The repository `said` names: a folder in it (its full path, or from ~), else a project Cue knows a folder of.
    fn repo_of(&self, said: &str) -> Result<InRepo, String> {
        if said.eq_ignore_ascii_case("add-on") || said.eq_ignore_ascii_case("addon") {
            return Err("the add-on's work goes on a branch of its own: COPY: #<id> | in: add-on | branch: <name>".into());
        }
        let folder = if let Some(rest) = said.strip_prefix("~/") {
            std::env::var("HOME").map(|h| format!("{h}/{rest}")).map_err(|_| "Cue can't read your home folder".to_string())?
        } else if said.starts_with('/') {
            said.to_string()
        } else {
            self.desk_folder(said).ok_or_else(|| format!("Cue knows no repository called {said}: name its folder (repo: <full path>)"))?
        };
        if !std::path::Path::new(&folder).is_dir() {
            return Err(format!("{folder} isn't a folder on this Mac"));
        }
        let (main, _) = main_checkout(&folder).ok_or_else(|| format!("{folder} isn't in a git repository"))?;
        let main = std::fs::canonicalize(&main).map(|p| p.to_string_lossy().into_owned()).unwrap_or(main);
        let project = crate::model::project_at(&main, "");
        Ok(InRepo { repo: main, project })
    }
}

/// The ROUTE of item `id` to `to` naming `repo` (looked up in `found`): what to set on the item once the route is made,
/// or why it's refused, as the lead is told. `remote`: the route starts it on another machine (`on:`).
pub(crate) fn route_repo(st: &Store, id: &str, to: &str, repo: &str, found: &Found, remote: bool) -> Result<InRepo, String> {
    let named = found.iter().find(|(r, _)| r == repo).map(|(_, f)| f.clone()).unwrap_or_else(|| Err("Cue didn't look it up".into()));
    let r = named.map_err(|e| format!("Refused: repo: {repo}: {e}. #{id} wasn't routed."))?;
    if remote {
        return Err(format!("Refused: repo: is a repository on this Mac, and on: starts #{id} on another machine. #{id} wasn't routed; route it with one or the other."));
    }
    if !to.eq_ignore_ascii_case("new") && !to.eq_ignore_ascii_case("me") {
        let there = st.sessions.iter().find(|s| crate::desk::hub::core::desk::shown_name(s).eq_ignore_ascii_case(to)).map(|s| s.origin.machine.clone());
        if there.is_some_and(|m| !m.is_empty()) {
            return Err(format!("Refused: {to} runs on another machine, and repo: is a repository on this Mac. #{id} wasn't routed; route it to a session here, or to new."));
        }
    }
    if st.desk.item(id).is_some_and(|i| i.copy_on.is_some()) {
        return Err(format!("Refused: #{id} already has a copy on a branch the lead named (COPY), so it can't land on {}'s main line too. #{id} wasn't routed.", r.repo));
    }
    Ok(r)
}

/// The project whose landing settings item `it`'s landing follows (desk/lands.rs): its repository's, when it was routed
/// to another (#635), else its Desk's.
pub(crate) fn lands_project(it: &DeskItem) -> &str {
    it.in_repo.as_ref().map_or(it.project.as_str(), |r| r.project.as_str())
}

/// The words item `it`'s landing is checked for (desk/hub/landing/push_check.rs): its Desk's project's (`project`),
/// and, routed to another repository, that repository's project's too.
pub(crate) fn never_words(project: &str, it: &DeskItem) -> Vec<String> {
    let mut words = crate::desk::hub::landing::push_check::words_for(project);
    if let Some(r) = it.in_repo.as_ref().filter(|r| r.project != project) {
        for w in crate::desk::hub::landing::push_check::words_for(&r.project) {
            if !words.iter().any(|x| x.eq_ignore_ascii_case(&w)) {
                words.push(w);
            }
        }
    }
    words
}
