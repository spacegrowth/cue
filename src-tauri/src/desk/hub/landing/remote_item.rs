//! Desk items worked on another machine (#500, docs/desk-remote-items-plan.md). The lead can start a new session for
//! an item on a machine (`ROUTE: #id -> new | on: <machine> | in: <folder there>`), or route it to a session already
//! there. As such a holder starts on it, Cue tells it to work on a branch of its own (`desk/<id>`) in its clone there,
//! made from the shared remote's main line, and never to push. At its DONE Cue, from this Mac:
//!
//! 1. looks at the machine: one that doesn't answer waits ("Machine not answering"), tried again each minute;
//! 2. finds the branch there with every change committed on it, else hands the item back saying what's missing;
//! 3. word-checks every commit on it past the remote's main line, messages too, against this Desk's never-commit
//!    words, before anything leaves the machine;
//! 4. pushes the branch from there to the shared remote (never main), fetches it here and checks it's the same commit;
//! 5. puts it in a copy of its own here and in the landing line (desk/hub/landing/trees.rs), like any item: brought up
//!    to main, word-checked, checked, landed as one commit, then installed or to check.
//!
//! A clash or a failed check goes back to the session on the machine, which fixes its branch there and says DONE
//! again. A machine quiet for half an hour with an item on it is the lead's to ask you about.

use crate::desk::hub::core::desk::Effects;
use crate::desk::hub::landing::git::{because, git, git_said};
use crate::desk::hub::landing::main_line::main_checkout;
use crate::desk::hub::landing::tree_files::{exclude_trees, names_item, remove, rev, take_away};
use crate::desk::hub::landing::trees::TREES;
use crate::desk::hub::prelude::*;
use crate::desk::{DeskItem, ItemTree, OnMachine, RemoteWork};

/// Why a remote item's DONE waits, as its row says.
pub(crate) const NOT_ANSWERING: &str = "Machine not answering";
pub(crate) const NO_BRANCH: &str = "Waiting for its branch";
/// How often a waiting DONE is tried again.
const RETRY_MS: u64 = 60_000;
/// Tries in a row a DONE that won't mend by itself gets before it asks you.
const STUCK_AFTER: u32 = 3;
/// How long a machine with an item on it may be quiet before the lead asks you: wait, or move the item.
pub(crate) const QUIET_ASK_MS: u64 = 30 * 60_000;

/// Where fetched branches are kept here, by item, until they land.
fn fetched_ref(id: &str) -> String {
    format!("refs/cue/remote/{id}")
}

/// Whether the lead's `on:` names this Mac (`on: this Mac`, `here`), not another machine.
fn this_mac(on: &str) -> bool {
    let on = on.trim().to_ascii_lowercase();
    ["this mac", "mac", "here", "local", "this machine"].contains(&on.as_str())
}

/// The folder of Desk `desk`'s clone on `machine`, if the Desk has used it (desk/machine_pref.rs, then what it knew
/// before that setting).
pub(crate) fn desk_folder_on(st: &Store, desk: &str, machine: &str) -> Option<String> {
    crate::desk::machine_pref::of(desk).folder(machine).map(str::to_string).or_else(|| st.desk.spaces.get(desk).and_then(|d| d.machine_folders.get(machine).cloned()))
}

/// Where a new session for item `it` starts (#597): None on this Mac, else the machine and its clone's folder there.
/// The lead's `on:` first, then where you chose for it, then where its Desk's sessions start. Err: why it can't start
/// there (no folder known for that machine), as the item says.
pub(crate) fn start_place(st: &Store, it: &DeskItem) -> Result<Option<OnMachine>, String> {
    if let Some(o) = &it.on_machine {
        return Ok((!o.machine.is_empty()).then(|| o.clone()));
    }
    let m = match &it.start_on {
        Some(m) => m.clone(),
        None => crate::desk::machine_pref::of(&it.project).machine,
    };
    if m.is_empty() {
        return Ok(None);
    }
    let folder = desk_folder_on(st, &it.project, &m).ok_or_else(|| format!("Cue doesn't know the folder of this repository's clone on {}", crate::machines::display_name(&m)))?;
    Ok(Some(OnMachine { machine: m, folder }))
}

/// Err when session `to` is on a machine, in a folder another open item's work is in (#679): both would commit in one
/// folder, and one's work would land under the other's name.
fn folder_free(st: &Store, id: &str, to: &str) -> Result<(), String> {
    if let Some((m, cwd)) = st.sessions.iter().find(|s| crate::desk::hub::core::desk::shown_name(s).eq_ignore_ascii_case(to)).map(|s| (s.origin.machine.clone(), s.origin.cwd.clone())).filter(|(m, _)| !m.is_empty()) {
        let other = st.desk.items.iter().find(|i| i.id != id && matches!(i.state.as_str(), "working" | "finishing") && i.remote.as_ref().is_some_and(|r| r.machine == m && r.folder == cwd));
        if let Some(o) = other {
            return Err(format!(
                "Refused: {to} works in {cwd} on {}, where #{} is being worked, so #{id} wasn't routed: two items in one folder mix their work. Route it to new (it gets a copy of its own there), or wait until #{} is done.",
                crate::machines::display_name(&m),
                o.id,
                o.id
            ));
        }
    }
    Ok(())
}

/// The lead's `on:` and `in:` on a ROUTE of item `id` to `to`: where a new session for it starts, set on the item once
/// the route is made (Some: what to set; a route to new without `on:` starts it where you chose, else where the Desk's
/// sessions start; `on: this Mac` here). Your choice on the item wins, and a Desk's first item on a machine waits for
/// you: the lead asks you, then gives the folder there (`in:`). Err: why it's refused, as the lead is told.
pub(crate) fn route_on(st: &Store, project: &str, id: &str, to: &str, on: Option<&str>, in_dir: Option<&str>) -> Result<Option<Option<OnMachine>>, String> {
    let Some(it) = st.desk.item(id).filter(|i| i.project == project) else { return Ok(None) };
    let fresh = to.eq_ignore_ascii_case("new");
    let pick = it.start_on.as_deref();
    let pick_shown = |p: &str| if p.is_empty() { "this Mac".to_string() } else { crate::machines::display_name(p) };
    if !fresh {
        folder_free(st, id, to)?;
    }
    let Some(on) = on else {
        // To a session already at work: it has to be where you chose.
        if let (false, Some(p)) = (fresh, pick) {
            let there = st.sessions.iter().find(|s| crate::desk::hub::core::desk::shown_name(s).eq_ignore_ascii_case(to)).map(|s| s.origin.machine.clone());
            if there.is_some_and(|m| m != p) {
                return Err(format!("Refused: the user chose to start #{id} on {}, and {to} isn't there, so #{id} wasn't routed. Route it to a session there, or to new.", pick_shown(p)));
            }
        }
        return Ok(fresh.then_some(None));
    };
    if !fresh {
        return Err(format!("Refused: on: starts a new session on a machine, so it goes with -> new. To give #{id} to a session already on a machine, route it to that session by name."));
    }
    let machines = crate::machines::list();
    let named = |m: &crate::machines::Machine| [m.name.clone(), m.label.clone(), crate::machines::short_name(&m.name)].iter().any(|n| !n.is_empty() && n.eq_ignore_ascii_case(on));
    let found = if this_mac(on) { None } else { machines.iter().find(|m| named(m)) };
    let target = found.map(|m| m.name.as_str()).unwrap_or("");
    if let Some(p) = pick.filter(|p| *p != target && (found.is_some() || this_mac(on))) {
        return Err(format!("Refused: the user chose to start #{id} on {}, so it wasn't routed elsewhere. Route it again without on:.", pick_shown(p)));
    }
    if this_mac(on) {
        return Ok(Some(Some(OnMachine::default())));
    }
    let Some(m) = found else {
        let known: Vec<String> = machines.iter().map(|m| crate::machines::display_name(&m.name)).collect();
        let known = if known.is_empty() { "none yet (the user adds one in + New → Machine)".to_string() } else { known.join(", ") };
        return Err(format!("Refused: Cue has no machine called {on}, so #{id} wasn't routed. Its machines: {known}. Route it again with one of them, or without on: to start it on this Mac."));
    };
    let shown = crate::machines::display_name(&m.name);
    let known = desk_folder_on(st, project, &m.name);
    let given = in_dir.map(|f| f.trim().trim_end_matches('/').to_string()).filter(|f| !f.is_empty());
    // The Desk's first item there: you're asked first, and you say where its clone is.
    if known.is_none() && given.is_none() {
        return Err(format!("Refused: this Desk hasn't used {shown} yet, so #{id} wasn't routed. Ask the user first (ASK: #{id} | Start it on {shown}, in <the folder of this repository's clone there>? If the project isn't there yet, Cue downloads it into that folder first. | Yes | No, this Mac). Once they say yes, route it again with | on: {shown} | in: <folder there>."));
    }
    let Some(folder) = given.or(known) else { unreachable!() };
    if !folder.starts_with('/') && !folder.starts_with('~') {
        return Err(format!("Refused: in: needs the folder's full path on the machine (it was {folder}), so #{id} wasn't routed."));
    }
    Ok(Some(Some(OnMachine { machine: m.name.clone(), folder })))
}

/// What a holder on another machine is told of its item's branch. `from`: the work last pushed from the machine its
/// last holder was on (#598), which it starts from.
pub(crate) fn remote_line(r: &RemoteWork, id: &str, from: Option<&RemoteWork>) -> String {
    if is_own_copy_there(&r.folder, id) {
        return own_copy_line(r, id, from);
    }
    let line = format!(
        "You work on another machine: in {folder}, fetch {onto} from {remote} and make the branch {branch} from it \
         (git fetch {remote} && git switch -c {branch} {remote}/{onto}; if it's there already, switch to it and merge \
         {remote}/{onto}). Commit all your work on {branch}. Don't push, and don't change {onto}: at your DONE, Cue \
         checks it for words that must not go public, pushes {branch} from there, and lands it on {onto} like any item. \
         Say in your DONE which checks you ran and which you couldn't run there, and name your branch and its last \
         commit in it: {done}. Your fixes for #{id} go on {branch} too.",
        done = done_form(r, id),
        folder = r.folder,
        onto = r.onto,
        remote = r.remote,
        branch = r.branch,
    );
    let Some(f) = from else { return line };
    format!(
        "{line}\nIt moved here from {shown}: its work there was last pushed {when}, as {fb} (commit {sha}). Start from \
         it, not from {onto}: git fetch {remote} && git switch -c {branch} {remote}/{fb}, then merge {remote}/{onto}. \
         Anything not pushed stays on {shown}.",
        shown = crate::machines::display_name(&f.machine),
        when = pushed_when(f.pushed_at),
        fb = f.branch,
        sha = short(&f.sha),
        onto = r.onto,
        remote = r.remote,
        branch = r.branch,
    )
}

/// Whether `folder` on a machine is item `id`'s own copy there (#679, remote_setup.rs `own_copy`), not a clone that
/// other sessions work in too.
fn is_own_copy_there(folder: &str, id: &str) -> bool {
    folder.trim_end_matches('/').ends_with(&format!("/{TREES}/desk-{id}"))
}

/// `remote_line` for a holder in its item's own copy on the machine (#679): the branch is made there already.
fn own_copy_line(r: &RemoteWork, id: &str, from: Option<&RemoteWork>) -> String {
    let start = match from {
        Some(f) => format!(
            "It moved here from {shown}: its work there was last pushed {when}, as {fb} (commit {sha}). Start from it: \
             git fetch {remote} && git merge {remote}/{fb} && git merge {remote}/{onto}. Anything not pushed stays on {shown}.",
            shown = crate::machines::display_name(&f.machine),
            when = pushed_when(f.pushed_at),
            fb = f.branch,
            sha = short(&f.sha),
            remote = r.remote,
            onto = r.onto,
        ),
        None => format!("Bring it up to date first: git fetch {remote} && git merge {remote}/{onto}.", remote = r.remote, onto = r.onto),
    };
    format!(
        "You work on another machine, in {folder}: a copy of its own for #{id}, on the branch {branch}, made from \
         {remote}/{onto}. Other items' sessions work in other copies on that machine, so keep all your work in this \
         folder and on {branch}; don't switch branches. {start} Commit all your work on {branch}. Don't push, and don't \
         change {onto}: at your DONE, Cue checks it for words that must not go public, pushes {branch} from there, and \
         lands it on {onto} like any item. Say in your DONE which checks you ran and which you couldn't run there, and \
         name your branch and its last commit in it: {done}. Your fixes for #{id} go on {branch} too.",
        folder = r.folder,
        branch = r.branch,
        remote = r.remote,
        onto = r.onto,
        done = done_form(r, id),
    )
}

/// When work was pushed (ms), as a holder or the lead is told: "4 min ago", "2 h ago".
pub(crate) fn pushed_when(at: u64) -> String {
    let mins = now_ms().saturating_sub(at) / 60_000;
    if mins < 60 { format!("{} min ago", mins.max(1)) } else { format!("{} h ago", mins / 60) }
}

/// The DONE line a holder of item `id` on another machine says (#596): its branch and the last commit on it, which
/// Cue checks is the one it pushes and fetches.
pub(crate) fn done_form(r: &RemoteWork, id: &str) -> String {
    format!("DONE: #{id} | branch: {b} | commit: <its last commit in full, from git rev-parse {b}>", b = r.branch)
}

/// The remote main checkout `repo`'s branch `onto` is pushed to: its own setting, else "origin", else the only one.
pub(crate) fn remote_of(machine: &str, repo: &str, onto: &str) -> Option<String> {
    if let Some(r) = git(machine, repo, &[], &["config", &format!("branch.{onto}.remote")]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != ".") {
        return Some(r);
    }
    let all = git(machine, repo, &[], &["remote"])?;
    let all: Vec<&str> = all.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if all.contains(&"origin") {
        return Some("origin".into());
    }
    (all.len() == 1).then(|| all[0].to_string())
}

/// Item `id` started on `machine` from `remote`/`onto` while main here is `ahead` commits past it (None: Cue couldn't
/// compare them), nobody asked to push first: its lead told on its feed.
pub(crate) fn remote_behind_note(st: &mut Store, id: &str, machine: &str, remote: &str, onto: &str, ahead: Option<u32>, now: u64) {
    let Some(it) = st.desk.item(id) else { return };
    let shown = crate::machines::display_name(machine);
    let how = match ahead {
        Some(n) => format!("but {onto} here is {n} commit{} ahead of it", if n == 1 { "" } else { "s" }),
        None => format!("but Cue couldn't compare {onto} here with it (the remote didn't answer), so it may be behind"),
    };
    let words = format!(
        "#{id} ({}) starts on {shown} from {remote}/{onto}, {how}. Ask the user to push {onto} (or say it's fine), so the session there works on current code.",
        it.title
    );
    let project = it.project.clone();
    let key = format!("ahead:{}:{now}", ahead.map_or("unknown".to_string(), |n| n.to_string()));
    crate::desk::hub::lead::feed::note(st, &project, id, crate::desk::feed::ChangeKind::Machine, words, &key, now);
}

impl Hub {
    /// You chose where item `id`'s session starts (#597), on its page before it started: `machine` ("" for this Mac),
    /// one the Desk has a folder on. The item as shown.
    pub(crate) fn desk_start_on(&self, id: &str, machine: &str) -> Result<DeskItem, String> {
        let machine = machine.trim();
        let fx = self.write(|st| -> Result<Effects, String> {
            let it = st.desk.item(id).ok_or("no such item")?;
            if it.holder.is_some() || matches!(it.state.as_str(), "done" | "dropped") {
                return Err("it has started already: it stays where it is".into());
            }
            if !machine.is_empty() {
                let shown = crate::machines::get(machine).map(|m| crate::machines::display_name(&m.name)).ok_or_else(|| format!("Cue has no machine called {machine}"))?;
                if desk_folder_on(st, &it.project, machine).is_none() {
                    return Err(format!("set the folder of this repository's clone on {shown} in the Desk's settings first"));
                }
            }
            let it = st.desk.item_mut(id).ok_or("no such item")?;
            it.start_on = Some(machine.to_string());
            it.on_machine = None;
            it.start_failed.clear();
            Ok(Effects { items: vec![id.to_string()], ..Default::default() })
        })?;
        self.desk_done(fx);
        self.item_shown(id).ok_or_else(|| "no such item".into())
    }

    /// Items in `fx` that just went to work with a holder on another machine, in a project whose items land from
    /// copies: each told to work on a branch of its own there, which lands here at its DONE. When main here is ahead of
    /// the shared remote, so the machine would start from old work, you're asked to push first (push_main.rs). Run as the Desk's changes are
    /// saved (with `desk_trees`), before the holder's message goes.
    pub(crate) fn desk_remote_starts(&self, fx: &mut Effects) {
        // Moved to another holder (here, or on another machine or folder): its old branch there is no longer its work.
        let moved: Vec<String> = self.read(|st| fx.items.iter().filter(|id| st.desk.item(id).is_some_and(|i| i.state == "working" && !holder_has(st, i))).cloned().collect());
        for id in moved {
            self.remote_drop(&id);
        }
        let asked = self.read(|st| remote_starts_of(st, &fx.items));
        for (id, project, machine, folder, to) in asked {
            let Some((repo, onto, remote, l)) = self.remote_branch_of(&project) else { continue };
            let r = RemoteWork { machine: machine.clone(), folder, branch: format!("desk/{id}"), remote: remote.clone(), onto: onto.clone(), ..Default::default() };
            // Moved from a machine that pushed its work (#598): it starts from that, taken off the item.
            let set = self.write(|st| {
                let it = st.desk.item_mut(&id).filter(|i| i.remote.is_none())?;
                it.remote = Some(r.clone());
                Some(it.pushed_before.take())
            });
            let Some(from) = set else { continue };
            let line = remote_line(&r, &id, from.as_ref());
            // A project set to never push: it starts now, nothing fetched or asked.
            if l.push != "never" {
                // Its holder's message held while main here is compared with the remote the machine starts from, off
                // this thread (a capped fetch can take a minute): even, it goes; behind or not known, pushed first on
                // "after each landing", asked first on "ask" (push_main.rs).
                let text = held_start_text(fx, &to, &id, &line);
                let s = crate::desk::hub::landing::push_main::HeldStart { desk: project, top: repo, id, machine, remote, onto, push: l.push.to_string(), to, text };
                self.later(0, move |h| h.desk_remote_start_checked(s));
                continue;
            }
            match fx.sends.iter_mut().find(|(s, text)| *s == to && names_item(text, &id)) {
                Some((_, text)) => {
                    text.push('\n');
                    text.push_str(&line);
                }
                None => fx.sends.push((to, format!("Desk item #{id}: {line}"))),
            }
        }
    }

    /// Whether Desk `project`'s items on another machine work on a branch of their own there, landed from here at their
    /// DONE (its items land from copies): its main checkout here, the main line's branch, the remote it's pushed to,
    /// and how it lands.
    pub(crate) fn remote_branch_of(&self, project: &str) -> Option<(String, String, String, crate::desk::lands::Lands)> {
        let local = self.desk_folder(project)?;
        let (repo, onto) = main_checkout(&local).filter(|(_, b)| !b.is_empty())?;
        let l = crate::desk::lands::lands_of(project, &repo);
        if l.at != "copy" || l.how == "owner" || l.how == "pr" {
            return None;
        }
        let remote = remote_of("", &repo, &onto)?;
        Some((repo, onto, remote, l))
    }

    /// The folder a new session for item `id` starts in on a machine (#679): a copy of its own in the Desk's clone
    /// `folder` there, on its branch, when the Desk's items work in copies (however they land: from here, by pull
    /// request or by you); else the clone. Two items on one machine never share a folder, so one's work can't land
    /// under the other's name. Copies there of closed items whose sessions are gone are cleared as it's made. Err: why
    /// it can't start, as the item says.
    pub(crate) fn remote_start_dir(&self, project: &str, id: &str, machine: &str, folder: &str) -> Result<String, String> {
        let code = self.read(|st| st.desk.item(id).is_some_and(|i| !crate::desk::hub::landing::unlanded::rules_out_code(&i.done_when)));
        let Some(local) = code.then(|| self.desk_folder(project)).flatten() else { return Ok(folder.to_string()) };
        let Some((repo, onto)) = main_checkout(&local).filter(|(_, b)| !b.is_empty()) else { return Ok(folder.to_string()) };
        if crate::desk::lands::lands_of(project, &repo).at != "copy" {
            return Ok(folder.to_string());
        }
        let Some(there) = remote_of(machine, folder, &onto).or_else(|| remote_of("", &repo, &onto)) else { return Ok(folder.to_string()) };
        let keep = self.read(|st| copies_kept_on(st, machine, id));
        crate::desk::hub::landing::remote_setup::own_copy(machine, folder, id, &format!("desk/{id}"), &there, &onto, &keep)
    }

    /// Item `id`'s DONE, when its work is on another machine: pushed from there, fetched here and put in the landing
    /// line, or waiting (said on it, tried again by the tick), or back to its holder. False: not such an item.
    pub(crate) fn desk_remote_done(&self, id: &str) -> bool {
        let Some(it) = self.read(|st| st.desk.item(id).cloned()) else { return false };
        let Some(r) = it.remote.clone() else { return false };
        // Its holder isn't the one on that machine any more (moved): its work is where the holder is now.
        if !self.read(|st| holder_has(st, &it)) {
            self.remote_drop(id);
            return false;
        }
        if it.state != "finishing" || !it.finish.as_ref().is_some_and(|f| f.step == "commit") {
            return true;
        }
        let now = now_ms();
        // A tick's retry waits a minute; a DONE said since the last try goes at once.
        let done_at = it.finish.as_ref().map(|f| f.at).unwrap_or(0);
        if !r.wait.is_empty() && done_at <= r.tried_at && now < r.tried_at + RETRY_MS {
            return true;
        }
        self.remote_set(id, |r| r.tried_at = now);
        let Some((there, range)) = self.remote_ready(&it, &r) else { return true };
        if !self.remote_clean(&it, &r, &there, &range) {
            return true;
        }
        if let Some((repo, onto, sha)) = self.remote_pushed(&it, &r, &there, now) {
            self.remote_to_line(&it, &r, (&repo, &onto, &sha), &there);
        }
        true
    }

    /// Steps 1 and 2 of a DONE there: the machine answers, and its branch is there with everything committed on it.
    /// Its remote there and the range of its own commits; None: said on the item (or nothing to push, checked as such).
    fn remote_ready(&self, it: &DeskItem, r: &RemoteWork) -> Option<(String, String)> {
        let (m, dir, branch) = (r.machine.as_str(), r.folder.as_str(), r.branch.as_str());
        if crate::safe_git::on_machine(m, "echo ok").is_err() {
            self.remote_wait(&it.id, NOT_ANSWERING);
            return None;
        }
        let shown = crate::machines::display_name(m);
        let Some(head) = git(m, dir, &[], &["rev-parse", "-q", "--verify", &format!("refs/heads/{branch}")]).map(|h| h.trim().to_string()) else {
            self.remote_missing(it, &format!("Cue can't find the branch {branch} in {dir} on {shown}. Make it there from {}/{} as you were told, commit your work on it, and say {}", r.remote, r.onto, done_form(r, &it.id)));
            return None;
        };
        let loose = git(m, dir, &[], &["status", "--porcelain", "--untracked-files=normal"]).unwrap_or_default();
        let on = git(m, dir, &[], &["symbolic-ref", "--short", "-q", "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default();
        if on == branch && !loose.trim().is_empty() {
            let files: Vec<&str> = loose.lines().filter_map(|l| l.get(3..)).take(5).collect();
            self.remote_back(it, &format!("Not landed: some of its work on {shown} isn't committed ({}). Commit it on {branch} and say DONE again", files.join(", ")));
            return None;
        }
        // The remote there: the branch's own, else the main line's.
        let there = remote_of(m, dir, &r.onto).unwrap_or_else(|| r.remote.clone());
        let range = format!("{there}/{}..refs/heads/{branch}", r.onto);
        let Some(count) = git(m, dir, &[], &["rev-list", "--count", &range]).and_then(|n| n.trim().parse::<u32>().ok()) else {
            self.remote_back(it, &format!("Not landed: Cue can't compare {branch} with {there}/{} in {dir} on {shown}. Fetch {there} there and say DONE again", r.onto));
            return None;
        };
        // Nothing past the main line: nothing in the repo changed, so it's to check as any such item.
        if count == 0 {
            self.remote_set(&it.id, |r| r.wait.clear());
            self.desk_finish_shared(&it.id);
            return None;
        }
        if !self.remote_named(it, r, &head) {
            return None;
        }
        Some((there, range))
    }

    /// Step 3, there, before anything leaves the machine: the check before every push (push_check.rs: no never-commit
    /// word nor your name in a message, an author or an added line of any commit the remote doesn't have yet), and no
    /// email git made up from the machine. False: handed back to its holder.
    fn remote_clean(&self, it: &DeskItem, r: &RemoteWork, there: &str, range: &str) -> bool {
        let (m, dir, branch) = (r.machine.as_str(), r.folder.as_str(), r.branch.as_str());
        let project = self.read(|st| st.desk.project_of(&it.project).to_string());
        if let Err(e) = crate::desk::hub::landing::push_check::push_allowed(m, dir, &project, there, &format!("refs/heads/{branch}")) {
            self.remote_back(it, &format!("{e}. Every commit on {branch} there is pushed, so take each out of the branch's history (amend or rebase it, not a new commit on top; set user.name and user.email there for an author) and say DONE again"));
            return false;
        }
        if let Some(e) = guessed_email(m, dir, range) {
            self.remote_back(it, &format!("Not pushed: a commit on {branch} has the email \"{e}\", which git made up from the machine. Set user.email there, redo those commits (git rebase --reset-author), and say DONE again"));
            return false;
        }
        true
    }

    /// Step 4: pushed from there (its own branch only, never main), then fetched here and compared. The main checkout
    /// here, its branch and the commit; None: said on the item.
    fn remote_pushed(&self, it: &DeskItem, r: &RemoteWork, there: &str, now: u64) -> Option<(String, String, String)> {
        let (m, dir, branch, id) = (r.machine.as_str(), r.folder.as_str(), r.branch.as_str(), it.id.as_str());
        let Some(sha) = git(m, dir, &[], &["rev-parse", &format!("refs/heads/{branch}")]).map(|s| s.trim().to_string()) else {
            self.remote_wait(id, NOT_ANSWERING);
            return None;
        };
        // No prompt for a password: a push that needs one fails, said on the item. (The clone's own ssh settings stay.)
        let env = [("GIT_TERMINAL_PROMPT", "0")];
        if let Err(e) = git_said(m, dir, &env, &["push", "-q", there, &format!("+refs/heads/{branch}:refs/heads/{branch}")]) {
            let shown = crate::machines::display_name(m);
            self.remote_stuck(id, &because(format!("{NO_BRANCH}: not pushed from {shown}"), &e));
            return None;
        }
        self.remote_set(id, |r| (r.sha, r.pushed_at) = (sha.clone(), now));
        let Some(local) = self.desk_folder(&it.project) else {
            self.remote_stuck(id, "Cue can't find this Desk's repository here");
            return None;
        };
        let Some((repo, onto)) = main_checkout(&local).filter(|(_, b)| !b.is_empty()) else {
            self.remote_stuck(id, "The main checkout here isn't on a branch");
            return None;
        };
        let here = remote_of("", &repo, &onto).unwrap_or_else(|| r.remote.clone());
        if let Err(e) = self.net_git(&repo, &["fetch", "-q", &here, &format!("+refs/heads/{branch}:{}", fetched_ref(id))]) {
            self.remote_stuck(id, &because(format!("{NO_BRANCH}: not fetched here"), &e));
            return None;
        }
        if rev(&repo, &fetched_ref(id)).as_deref() != Some(sha.as_str()) || (!r.said_commit.is_empty() && !sha.starts_with(&r.said_commit)) {
            self.remote_stuck(id, &format!("{NO_BRANCH}: the remote's {branch} isn't the commit pushed"));
            return None;
        }
        Some((repo, onto, sha))
    }

    /// Step 5: a copy of its own here (`repo`, onto `onto`), on the fetched commit `sha`, and into the landing line.
    fn remote_to_line(&self, it: &DeskItem, r: &RemoteWork, (repo, onto, sha): (&str, &str, &str), there: &str) {
        let id = it.id.as_str();
        let Some(base) = git("", repo, &[], &["merge-base", sha, onto]).map(|b| b.trim().to_string()).filter(|b| !b.is_empty()) else {
            self.remote_back(it, &format!("Not landed: {} shares no history with {onto} here. Make it from {there}/{} and say DONE again", r.branch, r.onto));
            return;
        };
        let t = match self.remote_copy(it, repo, sha, &base) {
            Ok(t) => t,
            Err(e) => return self.remote_stuck(id, &format!("Cue couldn't make its copy here: {e}")),
        };
        let landed = self.finish_set(id, |i| {
            i.tree = Some(t);
            if let Some(r) = i.remote.as_mut() {
                (r.wait, r.fails) = (String::new(), 0);
            }
            if let Some(f) = i.finish.as_mut() {
                (f.step, f.wait) = ("line".into(), String::new());
            }
        });
        if landed.is_some() {
            self.desk_land_line();
        }
    }

    /// Item `it`'s fetched work (`sha`) in a copy of its own in `repo` (`remote-<id>`, made again each round), from `base`.
    fn remote_copy(&self, it: &DeskItem, repo: &str, sha: &str, base: &str) -> Result<ItemTree, String> {
        let branch = format!("desk/{}", it.id);
        if let Some(old) = it.tree.as_ref() {
            remove(old);
        }
        exclude_trees(repo);
        let _ = git("", repo, &[], &["worktree", "prune"]);
        // What was fetched for items no longer open (closed mid-finish, say) goes.
        let open: Vec<String> = self.read(|st| st.desk.items.iter().filter(|i| i.state != "done" && (i.remote.is_some() || i.pushed_before.is_some())).map(|i| i.id.clone()).collect());
        let fetched = git("", repo, &[], &["for-each-ref", "--format=%(refname)", "refs/cue/remote/"]).unwrap_or_default();
        for name in fetched.lines().map(str::trim).filter(|n| n.strip_prefix("refs/cue/remote/").is_some_and(|x| !open.iter().any(|o| o == x))) {
            let _ = git("", repo, &[], &["update-ref", "-d", name]);
        }
        let path = format!("{repo}/{TREES}/remote-{}", it.id);
        if std::path::Path::new(&path).exists() {
            take_away(repo, &path);
        }
        git_said("", repo, &[], &["worktree", "add", "-q", "-B", &branch, &path, sha])?;
        Ok(ItemTree { repo: repo.to_string(), path, branch, base: base.to_string(), slot: 0, keep: false, addon_of: String::new(), in_place: false, main_line: String::new(), landed: String::new(), landed_work: String::new() })
    }

    /// Item `id`'s remote work forgotten (its holder moved): its copy here of that work goes, and what was fetched.
    fn remote_drop(&self, id: &str) {
        let old = self.desk_saving(|| {
            let it = self.write(|st| {
                let i = st.desk.item_mut(id).filter(|i| i.remote.is_some())?;
                // Work pushed from there: its next holder starts from it (#598), its fetched commit kept here for that.
                if let Some(r) = i.remote.take().filter(|r| r.pushed_at > 0 && !r.sha.is_empty()) {
                    i.pushed_before = Some(r);
                }
                let t = i.tree.take_if(|t| is_remote_copy(t, id));
                Some((i.clone(), t))
            });
            if let Some((i, _)) = &it {
                crate::db::desk_put_item(i);
            }
            it
        });
        let Some((_, t)) = old else { return };
        if let Some(t) = t {
            remove(&t);
        }
        self.changed();
    }

    /// `f` done to item `id`'s remote work, saved.
    fn remote_set(&self, id: &str, f: impl FnOnce(&mut RemoteWork)) {
        let it = self.desk_saving(|| {
            let it = self.write(|st| {
                let i = st.desk.item_mut(id)?;
                f(i.remote.as_mut()?);
                Some(i.clone())
            });
            it.inspect(crate::db::desk_put_item)
        });
        if it.is_some() {
            self.changed();
        }
    }

    /// Item `id`'s DONE waits on `why` (said on it), tried again by the tick a minute on.
    fn remote_wait(&self, id: &str, why: &str) {
        let why = why.to_string();
        let w = why.clone();
        self.remote_set(id, |r| r.wait = w);
        self.finish_fin(id, |f| f.wait = why);
    }

    /// Item `it` back to its holder on the machine with `question`, at work again: its next DONE goes at once.
    fn remote_back(&self, it: &DeskItem, question: &str) {
        self.remote_set(&it.id, |r| (r.wait, r.fails) = (String::new(), 0));
        self.finish_back(it, question, "remote", "", true);
    }

    /// Whether item `it`'s DONE named its branch and last commit, and that commit is `head`, its branch's head there
    /// (#596). A DONE naming none goes back to its holder with the line to say; another branch or commit waits for its
    /// branch, tried again by the tick, and its holder is told once what Cue found.
    fn remote_named(&self, it: &DeskItem, r: &RemoteWork, head: &str) -> bool {
        let shown = crate::machines::display_name(&r.machine);
        if r.said_commit.is_empty() {
            self.remote_back(it, &format!("Not landed: your DONE didn't name the last commit on {}. Say it as {}", r.branch, done_form(r, &it.id)));
            return false;
        }
        let branch_ok = r.said_branch.is_empty() || r.said_branch == r.branch;
        if branch_ok && r.said_commit.len() >= 7 && head.starts_with(&r.said_commit) {
            return true;
        }
        let named = if branch_ok { format!("commit {}", r.said_commit) } else { format!("branch {}", r.said_branch) };
        self.remote_missing(it, &format!("your DONE names {named}, but {} on {shown} is at {}. Commit your work on {}, and say {}", r.branch, short(head), r.branch, done_form(r, &it.id)));
        false
    }

    /// Item `it`'s branch there isn't (yet) the one its DONE names, or isn't there (#596): it waits for its branch,
    /// tried again by the tick (asking you after `STUCK_AFTER` tries), and its holder is told `what` once.
    fn remote_missing(&self, it: &DeskItem, what: &str) {
        let told = it.remote.as_ref().is_some_and(|r| r.fails > 0);
        self.remote_stuck(&it.id, &format!("{NO_BRANCH}: {}", what.split(". ").next().unwrap_or(what)));
        let Some(h) = it.holder.clone().filter(|_| !told) else { return };
        let mut fx = Effects::default();
        fx.sends.push((h.to().to_string(), format!("Desk item #{} ({}) waits for its branch: {what}. Cue tries again each minute.", it.id, it.title)));
        self.desk_done(fx);
    }

    /// Item `id`'s DONE stopped on `why`, which a minute won't mend by itself (a push refused, a fetch here failing):
    /// tried again by the tick, and after `STUCK_AFTER` tries in a row it stops and asks you.
    fn remote_stuck(&self, id: &str, why: &str) {
        let fails = self.write(|st| st.desk.item_mut(id).and_then(|i| i.remote.as_mut()).map(|r| r.fails + 1)).unwrap_or(0);
        if fails < STUCK_AFTER {
            self.remote_set(id, |r| r.fails = fails);
            return self.remote_wait(id, why);
        }
        self.remote_set(id, |r| (r.wait, r.fails) = (String::new(), 0));
        self.finish_ask(id, "remote", &format!("{why} (Cue tried {STUCK_AFTER} times)"), "");
    }

    /// Session `sid`'s turn ended saying `text` (#598): each item it holds at work on another machine that answers has
    /// its branch pushed from there and fetched here, off this thread, so its work isn't only on that machine and its
    /// next holder can start from it. Not for an item this turn says DONE on: its DONE pushes it.
    pub(crate) fn desk_remote_stops(&self, sid: &str, text: &str) {
        let ids: Vec<String> = self.read(|st| {
            st.desk
                .items
                .iter()
                .filter(|i| i.state == "working" && i.holder.as_ref().is_some_and(|h| h.is(sid)) && holder_has(st, i))
                .filter(|i| i.remote.as_ref().is_some_and(|r| crate::hub::machine_quiet_since(st, &r.machine).is_none()))
                .map(|i| i.id.clone())
                .filter(|id| !text.contains(&format!("DONE: #{id}")))
                .collect()
        });
        for id in ids {
            self.later(0, move |h| h.remote_stop_push(&id));
        }
    }

    /// Item `id`'s branch on its machine pushed at a stop (#598), when it has new commits past the main line: checked
    /// as at DONE first (no never-commit word, no made-up email). Nothing to say when it can't go: it stays on the
    /// machine, and its DONE says why.
    fn remote_stop_push(&self, id: &str) {
        let Some(it) = self.read(|st| st.desk.item(id).filter(|i| i.state == "working" && holder_has(st, i)).cloned()) else { return };
        let Some(r) = it.remote.clone() else { return };
        let (m, dir, branch) = (r.machine.as_str(), r.folder.as_str(), r.branch.as_str());
        let Some(head) = git(m, dir, &[], &["rev-parse", "-q", "--verify", &format!("refs/heads/{branch}")]).map(|h| h.trim().to_string()).filter(|h| !h.is_empty()) else { return };
        if head == r.sha {
            return;
        }
        let there = remote_of(m, dir, &r.onto).unwrap_or_else(|| r.remote.clone());
        let range = format!("{there}/{}..refs/heads/{branch}", r.onto);
        if git(m, dir, &[], &["rev-list", "--count", &range]).and_then(|n| n.trim().parse::<u32>().ok()).unwrap_or(0) == 0 {
            return;
        }
        let project = self.read(|st| st.desk.project_of(&it.project).to_string());
        if let Err(e) = crate::desk::hub::landing::push_check::push_allowed(m, dir, &project, &there, &format!("refs/heads/{branch}")) {
            eprintln!("cue: desk: #{id} not pushed at a stop: {e}");
            return;
        }
        if guessed_email(m, dir, &range).is_some() {
            return;
        }
        if let Err(e) = git_said(m, dir, &[("GIT_TERMINAL_PROMPT", "0")], &["push", "-q", &there, &format!("+{head}:refs/heads/{branch}")]) {
            eprintln!("cue: desk: #{id} not pushed at a stop: {e}");
            return;
        }
        let now = now_ms();
        self.remote_set(id, |r| (r.sha, r.pushed_at) = (head.clone(), now));
        // Fetched here too, so a holder here can start from it without the network.
        if let Some((repo, onto)) = self.desk_folder(&it.project).and_then(|l| main_checkout(&l)).filter(|(_, b)| !b.is_empty()) {
            let here = remote_of("", &repo, &onto).unwrap_or_else(|| r.remote.clone());
            if let Err(e) = self.net_git(&repo, &["fetch", "-q", &here, &format!("+refs/heads/{branch}:{}", fetched_ref(id))]) {
                eprintln!("cue: desk: #{id}'s push not fetched here: {e}");
            }
        }
    }

    /// Items on a machine that has been quiet for `QUIET_ASK_MS`: the lead is told once per quiet spell, to ask you
    /// whether to wait or move the item. Run by the finishing tick.
    pub(crate) fn desk_remote_quiet(&self) {
        let now = now_ms();
        let told: Vec<DeskItem> = self.write(|st| {
            let mut told = Vec::new();
            let quiet: Vec<(String, String, String, u64)> = st
                .desk
                .items
                .iter()
                .filter(|i| matches!(i.state.as_str(), "working" | "finishing"))
                .filter_map(|i| {
                    let r = i.remote.as_ref()?;
                    let since = crate::hub::machine_quiet_since(st, &r.machine);
                    Some((i.id.clone(), i.project.clone(), r.machine.clone(), since.unwrap_or(0)))
                })
                .collect();
            for (id, project, machine, since) in quiet {
                let Some(r) = st.desk.item_mut(&id).and_then(|i| i.remote.as_mut()) else { continue };
                if since == 0 {
                    if std::mem::take(&mut r.quiet_told) {
                        told.extend(st.desk.item(&id).cloned());
                    }
                    continue;
                }
                if r.quiet_told || now < since + QUIET_ASK_MS {
                    continue;
                }
                r.quiet_told = true;
                let pushed = (r.pushed_at > 0).then(|| (r.branch.clone(), pushed_when(r.pushed_at)));
                let title = st.desk.item(&id).map(|i| i.title.clone()).unwrap_or_default();
                let mins = (now - since) / 60_000;
                let from = match pushed {
                    Some((b, when)) => format!("it starts from {b}, last pushed {when}; what's newer stays there"),
                    None => "nothing of it was pushed yet, so it starts over from main".to_string(),
                };
                let words = format!(
                    "#{id} ({title}) is on {}, which hasn't answered for {mins} minutes. Its work there is safe while the machine is. ASK: #{id} | Wait for {0}, or move #{id} to another session ({from})? | Wait | Move it",
                    crate::machines::display_name(&machine)
                );
                crate::desk::hub::lead::feed::note(st, &project, &id, crate::desk::feed::ChangeKind::Machine, words, &format!("quiet:{since}"), now);
                told.extend(st.desk.item(&id).cloned());
            }
            told
        });
        if !told.is_empty() {
            self.desk_saving(|| told.iter().for_each(crate::db::desk_put_item));
            self.changed();
            self.later(0, |h| h.desk_feed_send());
        }
    }
}

/// Item `id`'s new copy here (`path`, made in `repo` from main at `base`) when it moved from another machine (#598):
/// put on its work last pushed from there (`from`, fetched here as it was pushed), its base then where that work
/// meets main. The base, and the words its holder is told.
pub(crate) fn carry_moved(repo: &str, path: &str, base: &str, from: &RemoteWork, id: &str) -> (String, String) {
    let shown = crate::machines::display_name(&from.machine);
    let fetched = crate::desk::hub::landing::tree_files::rev(repo, &fetched_ref(id)).filter(|s| *s == from.sha);
    let moved = fetched.as_deref().and_then(|sha| {
        git_said("", path, &[], &["reset", "-q", "--hard", sha]).ok()?;
        git("", repo, &[], &["merge-base", sha, base]).map(|b| b.trim().to_string()).filter(|b| !b.is_empty())
    });
    match moved {
        Some(b) => (b, format!("It moved here from {shown}: your copy starts from its work last pushed from there {} (commit {}), not from main. Read it first (git log). Anything not pushed stays on {shown}.", pushed_when(from.pushed_at), short(&from.sha))),
        None => (base.to_string(), format!("It moved here from {shown}, but Cue couldn't bring its pushed work here ({}), so your copy starts from main.", from.branch)),
    }
}

/// An email git made up from machine `m` (user@host, or none) on a commit in `range` of the clone `dir` there: who
/// made each commit goes with it when pushed.
fn guessed_email(m: &str, dir: &str, range: &str) -> Option<String> {
    let who = git(m, dir, &[], &["log", "--format=%an%n%ae%n%cn%n%ce", range]).unwrap_or_default();
    who.lines().skip(1).step_by(2).map(str::trim).find(|e| e.is_empty() || e.split_once('@').is_some_and(|(_, host)| !host.contains('.'))).map(str::to_string)
}

/// Commit `sha` as shown: its first 10 characters.
fn short(sha: &str) -> &str {
    sha.get(..10).unwrap_or(sha)
}

/// Whether item `it`'s remote work (if any) is its holder's: the holder is on that machine, in that folder.
fn holder_has(st: &Store, it: &DeskItem) -> bool {
    let Some(r) = it.remote.as_ref() else { return true };
    let s = it.holder.as_ref().and_then(|h| st.sessions.get(h.to()));
    s.is_some_and(|s| s.origin.machine == r.machine && s.origin.cwd == r.folder)
}

/// The items whose own copies on `machine` stay as `id`'s is made (#679): every open item, `id`, and any item a session
/// there still works in (its folder is that item's copy).
fn copies_kept_on(st: &Store, machine: &str, id: &str) -> Vec<String> {
    let open = st.desk.items.iter().filter(|i| !matches!(i.state.as_str(), "done" | "dropped")).map(|i| i.id.clone());
    let marker = format!("/{TREES}/desk-");
    let in_use = st.sessions.iter().filter(|s| s.origin.machine == machine).filter_map(|s| s.origin.cwd.trim_end_matches('/').rsplit_once(marker.as_str()).map(|(_, x)| x.to_string()));
    let mut keep: Vec<String> = open.chain(in_use).chain(std::iter::once(id.to_string())).collect();
    keep.sort();
    keep.dedup();
    keep
}

/// Whether copy `t` is the one Cue made here of item `id`'s fetched work.
pub(crate) fn is_remote_copy(t: &ItemTree, id: &str) -> bool {
    !t.keep && t.path.ends_with(&format!("/remote-{id}"))
}

/// What `it` shows of its machine: the machine its holder is on, and its remote work's quiet time (`since`, ms).
pub(crate) fn shown_machine(st: &Store, it: &mut DeskItem) {
    let machine = it.holder.as_ref().and_then(|h| st.sessions.get(h.to())).map(|s| s.origin.machine.clone()).filter(|m| !m.is_empty());
    let machine = machine.or_else(|| it.remote.as_ref().map(|r| r.machine.clone()));
    // Not started yet (#597): the machine its session will start on.
    let machine = machine.or_else(|| (it.holder.is_none() && !matches!(it.state.as_str(), "done" | "dropped")).then(|| start_place(st, it).ok().flatten().map(|o| o.machine)).flatten());
    it.machine = machine.as_deref().map(crate::machines::display_name).unwrap_or_default();
    if let Some(r) = it.remote.as_mut() {
        r.quiet_since = crate::hub::machine_quiet_since(st, &r.machine).unwrap_or(0);
    }
}

/// Items among `ids` that just went to work with a holder on another machine, with no copy or remote work yet and
/// done when allows code: each with its project, that machine, the holder's folder there, and the holder.
fn remote_starts_of(st: &Store, ids: &[String]) -> Vec<(String, String, String, String, String)> {
    ids.iter()
        .filter_map(|id| {
            let it = st.desk.item(id).filter(|i| i.state == "working" && i.tree.is_none() && i.remote.is_none() && i.copy_on.is_none())?;
            if crate::desk::hub::landing::unlanded::rules_out_code(&it.done_when) {
                return None;
            }
            let h = it.holder.as_ref()?;
            let s = st.sessions.get(h.to()).filter(|s| !s.origin.machine.is_empty() && !s.origin.cwd.is_empty())?;
            Some((id.clone(), it.project.clone(), s.origin.machine.clone(), s.origin.cwd.clone(), h.to().to_string()))
        })
        .collect()
}

/// Holder `to`'s messages naming item `id`, taken out of `fx` to be held, with `line` (where to work) added to the
/// last; just that line when there were none.
fn held_start_text(fx: &mut Effects, to: &str, id: &str, line: &str) -> String {
    let mut texts: Vec<String> = fx.sends.iter().filter(|(s, text)| *s == to && names_item(text, id)).map(|(_, t)| t.clone()).collect();
    fx.sends.retain(|(s, text)| !(*s == to && names_item(text, id)));
    if texts.is_empty() {
        texts.push(format!("Desk item #{id}: {line}"));
    } else {
        let last = texts.len() - 1;
        texts[last].push('\n');
        texts[last].push_str(line);
    }
    texts.join("\n\n")
}
