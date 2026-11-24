//! Main kept current on the shared remote (#599, docs/desk-remote-items-plan.md "Keeping main current on the remote"):
//! the other machines start from what main was last pushed as, so the Desk offers the push at the two moments it
//! matters.
//!
//! - **After an install.** Once the running Cue has an install's work, a main ahead of its remote is offered as a
//!   Yes / No card in the Desk lead's chat ("Update from GitHub, then upload N landings? Yes rebases …"), Cue's own
//!   question as the old copies one is (nothing is sent to the lead). A project on "after each landing" is pushed
//!   then without asking; one on "never" is never pushed nor asked.
//! - **Before an item starts on another machine.** Its holder's message is held while you're asked to push first. A yes
//!   pushes, then the message goes; a no sends it at once, saying the item starts from older work.
//!
//! Every push goes through one checked path (`desk_push_main`): main rebased onto what's new on the remote, then the word check of every commit the remote doesn't have
//! yet (push_check.rs) first, refused with each find named. Never forced.

use crate::desk::hub::core::desk::Effects;
use crate::desk::hub::landing::git::{because, git, git_said};
use crate::desk::hub::landing::main_line::main_checkout;
use crate::desk::hub::landing::remote_item::remote_of;
use crate::desk::hub::prelude::*;

/// How many commits `onto` in `repo` has that `remote` doesn't (none of its remote-tracking refs has them; every
/// commit when it has none). None when git can't say.
pub(crate) fn ahead(repo: &str, remote: &str, onto: &str) -> Option<u32> {
    let not = format!("--remotes={remote}");
    git("", repo, &[], &["rev-list", "--count", &format!("refs/heads/{onto}"), "--not", &not]).and_then(|n| n.trim().parse().ok())
}

/// "1 landing" / "3 landings".
fn landings(n: u32) -> String {
    format!("{n} landing{}", if n == 1 { "" } else { "s" })
}

/// Where main is pushed to, by name for you: "GitHub" for a remote there, else "the shared copy".
pub(crate) fn place_of(repo: &str, remote: &str) -> String {
    let url = git("", repo, &[], &["remote", "get-url", remote]).unwrap_or_default();
    if url.to_lowercase().contains("github.com") { "GitHub".to_string() } else { "the shared copy".to_string() }
}

/// What Yes does, said on every push question (#675): the rebase first, then the word check, then the push.
fn what_yes_does(at: &str, them: &str) -> String {
    format!("Yes rebases main onto the newest on {at}, checks {them} for private words, then pushes. If a private word turns up, nothing is uploaded.")
}

/// The offer after an install, `n` landings ahead (None: not known) of `at` (`place_of`).
pub(crate) fn offer_words(n: Option<u32>, at: &str) -> String {
    match n {
        Some(n) => format!("Update from {at}, then upload {}? {}", landings(n), what_yes_does(at, if n == 1 { "it" } else { "them" })),
        None => format!("Cue couldn't compare main with {at} (it didn't answer). Update from {at}, then upload main? {}", what_yes_does(at, "it")),
    }
}

/// The ask before an item starts on another machine.
pub(crate) fn start_words(n: Option<u32>, at: &str) -> String {
    match n {
        Some(n) => format!(
            "Update from {at}, then upload {} before the other machine starts? Without them it starts from old work. {}",
            landings(n),
            what_yes_does(at, if n == 1 { "it" } else { "them" })
        ),
        None => format!(
            "Cue couldn't compare main with {at} (it didn't answer), so the other machine may start from old work. Update from {at}, then upload main first? {}",
            what_yes_does(at, "it")
        ),
    }
}

/// What a holder on another machine is told when its item starts without main pushed.
pub(crate) fn older_words(n: Option<u32>) -> String {
    let why = match n {
        Some(n) => format!("Main here is {} ahead of the shared copy and wasn't pushed", landings(n)),
        None => "Cue couldn't compare main here with the shared copy, and main wasn't pushed".to_string(),
    };
    format!("Note: this item starts from older work. {why}, so fetch it again before your DONE if the user pushes it.")
}

/// An item's start on another machine, held while main is compared with the shared copy (off the thread that
/// changed the Desk) and, when it's behind, pushed or asked about first.
pub(crate) struct HeldStart {
    pub(crate) desk: String,
    pub(crate) top: String,
    pub(crate) id: String,
    pub(crate) machine: String,
    pub(crate) remote: String,
    pub(crate) onto: String,
    /// The project's push choice: "ask" or "after".
    pub(crate) push: String,
    pub(crate) to: String,
    pub(crate) text: String,
}

/// The open push card about Desk `desk`'s project, if there is one: one at a time per project.
fn open_push_card(st: &Store, desk: &str) -> Option<String> {
    st.items.iter().find(|i| i.status == "pending" && i.tool_input["desk_push"]["desk"].as_str() == Some(desk)).map(|i| i.id.clone())
}

/// Main (`onto`, checked out in `top`) rebased onto `remote`'s copy of it (already fetched), when that has commits
/// main doesn't. Nothing to do when it hasn't, or has no copy yet. Refused, with main left as it was, when main
/// isn't checked out there, the checkout is mid-way through something, has changes of its own, or the rebase clashes
/// (undone at once).
pub(crate) fn rebase_onto_remote(top: &str, remote: &str, onto: &str) -> Result<(), String> {
    let theirs = format!("refs/remotes/{remote}/{onto}");
    if git("", top, &[], &["rev-parse", "--verify", "-q", &theirs]).is_none() {
        return Ok(());
    }
    let new: u32 = git("", top, &[], &["rev-list", "--count", &format!("refs/heads/{onto}..{theirs}")]).and_then(|n| n.trim().parse().ok()).ok_or("Not pushed: Cue couldn't compare main with what's new on the remote")?;
    if new == 0 {
        return Ok(());
    }
    let on = git("", top, &[], &["symbolic-ref", "--short", "-q", "HEAD"]).map(|b| b.trim().to_string()).unwrap_or_default();
    if on != onto {
        return Err(format!("Not pushed: {remote} has {new} new commit{} and {onto} isn't checked out in {top}, so Cue couldn't rebase onto them", if new == 1 { "" } else { "s" }));
    }
    if let Some(what) = crate::desk::hub::landing::main_line::in_progress(top) {
        return Err(format!("Not pushed: {top} is in the middle of {what}, so Cue couldn't rebase {onto} onto {remote}"));
    }
    if let Err(e) = git_said("", top, &[], &["rebase", "-q", &theirs]) {
        if crate::desk::hub::landing::main_line::in_progress(top).is_some() {
            let _ = git_said("", top, &[], &["rebase", "--abort"]);
        }
        return Err(because(format!("Not pushed: couldn't rebase {onto} onto {remote}'s newest, so it was left as it was"), &e));
    }
    Ok(())
}

impl Hub {
    /// Main of `top` fetched from its remote (capped, as every network git of the Desk's), then how far ahead it is:
    /// (remote, branch, count). None when it has no main line or no remote. The count is None when the fetch failed
    /// or git couldn't count: not known, never taken as even.
    pub(crate) fn desk_main_ahead(&self, top: &str) -> Option<(String, String, Option<u32>)> {
        let (top, onto) = main_checkout(top).filter(|(_, b)| !b.is_empty())?;
        let remote = remote_of("", &top, &onto)?;
        if let Err(e) = self.net_git(&top, &["fetch", "-q", &remote, &format!("+refs/heads/{onto}:refs/remotes/{remote}/{onto}")]) {
            eprintln!("cue: desk: couldn't fetch {onto} from {remote} to count what's not pushed: {e}");
            return Some((remote, onto, None));
        }
        let n = ahead(&top, &remote, &onto);
        Some((remote, onto, n))
    }

    /// Main of `top` pushed to its remote, once the word check of every commit it doesn't have yet passes
    /// (push_check.rs): what was done, or why not (a refusal naming each find). Never forced.
    pub(crate) fn desk_push_main(&self, project: &str, top: &str) -> Result<String, String> {
        let (top, onto) = main_checkout(top).filter(|(_, b)| !b.is_empty()).ok_or("Not pushed: Cue can't find this project's main line")?;
        let r = remote_of("", &top, &onto).ok_or("Not pushed: this project has no remote")?;
        // Updated from the remote first (#675): what's new there fetched, main rebased onto it.
        if let Err(e) = self.net_git(&top, &["fetch", "-q", &r, &format!("+refs/heads/{onto}:refs/remotes/{r}/{onto}")]) {
            // A remote without main yet has nothing to rebase onto: the push makes it.
            if !e.contains("couldn't find remote ref") {
                return Err(because(format!("Not pushed: couldn't update from {r}"), &e));
            }
        }
        rebase_onto_remote(&top, &r, &onto)?;
        crate::desk::hub::landing::push_check::push_allowed("", &top, project, &r, &format!("refs/heads/{onto}"))?;
        match self.net_git(&top, &["push", "-q", &r, &format!("refs/heads/{onto}:refs/heads/{onto}")]) {
            Ok(_) => Ok(format!("Pushed to {r}")),
            Err(e) => Err(because(format!("Not pushed to {r}"), &e)),
        }
    }

    /// An install of Desk `desk`'s work (main line `top`) is in the running Cue: main offered for a push when it's
    /// ahead of its remote, or when Cue couldn't tell. "never": nothing. "after": pushed now, no ask. "ask": a Yes / No
    /// card in the lead's chat, not while one about this project is open (one push question per project). Not for a
    /// project whose work lands by pull request (main comes from the remote). Run off the install tick's thread.
    pub(crate) fn desk_push_offer(&self, desk: &str, top: &str) {
        let l = crate::desk::lands::lands_of(desk, top);
        if l.push == "never" || l.how == "pr" {
            return;
        }
        let Some((remote, _, n)) = self.desk_main_ahead(top) else { return };
        if n == Some(0) {
            return;
        }
        if l.push == "after" {
            let said = self.desk_push_main(desk, top).unwrap_or_else(|e| e);
            self.desk_push_note(desk, None, said);
            return;
        }
        let Some(lead) = self.read(|st| st.desk.lead_sid(desk).map(str::to_string)) else { return };
        if self.read(|st| open_push_card(st, desk).is_some()) {
            return;
        }
        self.desk_push_ask(&lead, desk, &offer_words(n, &place_of(top, &remote)), json!({ "desk": desk, "top": top, "ahead": n, "starts": [] }));
    }

    /// Cue's own push question `q` in lead `lead`'s chat on Desk `desk`, `push` kept on the card for the answer.
    fn desk_push_ask(&self, lead: &str, desk: &str, q: &str, push: Value) -> Option<String> {
        let id = self.desk_ask(lead, desk, q, &["Yes".to_string(), "No".to_string()], None, None)?;
        self.write(|st| {
            if let Some(it) = st.items.iter_mut().find(|i| i.id == id) {
                it.tool_input["desk_push"] = push;
            }
        });
        Some(id)
    }

    /// Start `s`, held: main compared with the shared copy after a capped fetch. Even: it starts. "after": pushed
    /// (checked), then it starts, saying older work when the push didn't go. "ask", behind or not known: you're asked
    /// to push first, its message held on the card (added to the project's open push card when there is one, so one
    /// question covers every start); with no lead to ask in it starts and the lead's told. Run off the thread that
    /// changed the Desk (`later`), so a remote that doesn't answer holds nothing for its minute.
    pub(crate) fn desk_remote_start_checked(&self, s: HeldStart) {
        let ahead = self.desk_main_ahead(&s.top);
        let at = ahead.as_ref().map(|(r, _, _)| place_of(&s.top, r)).unwrap_or_else(|| "the shared copy".to_string());
        let n = ahead.and_then(|(_, _, n)| n);
        if n == Some(0) {
            self.desk_held_start_send(&s.id, &s.to, s.text);
            return;
        }
        if s.push == "after" {
            let pushed = self.desk_push_main(&s.desk, &s.top);
            let said = pushed.clone().unwrap_or_else(|e| e);
            self.desk_push_note(&s.desk, Some(s.id.clone()), said);
            let text = if pushed.is_ok() { s.text } else { format!("{}\n{}", s.text, older_words(n)) };
            self.desk_held_start_send(&s.id, &s.to, text);
            return;
        }
        let start = json!({ "item": s.id, "to": s.to, "text": s.text, "ahead": n });
        let joined = self.write(|st| {
            let Some(card) = open_push_card(st, &s.desk) else { return false };
            let Some(it) = st.items.iter_mut().find(|i| i.id == card) else { return false };
            let push = &mut it.tool_input["desk_push"];
            if !push["starts"].is_array() {
                push["starts"] = json!([]);
            }
            if let Some(all) = push["starts"].as_array_mut() {
                all.push(start.clone());
            }
            true
        });
        if joined {
            return;
        }
        if let Some(lead) = self.read(|st| st.desk.lead_sid(&s.desk).map(str::to_string)) {
            let push = json!({ "desk": s.desk, "top": s.top, "ahead": n, "starts": [start] });
            if self.desk_push_ask(&lead, &s.desk, &start_words(n, &at), push).is_some() {
                return;
            }
        }
        // No lead to ask in: it starts now, and the lead's told when there is one.
        let now = now_ms();
        self.write(|st| crate::desk::hub::landing::remote_item::remote_behind_note(st, &s.id, &s.machine, &s.remote, &s.onto, n, now));
        self.desk_held_start_send(&s.id, &s.to, s.text);
    }

    /// Item `id`'s held start `text` sent to its holder `to`, unless it moved or was dropped meanwhile.
    fn desk_held_start_send(&self, id: &str, to: &str, text: String) {
        let still = self.read(|st| st.desk.item(id).is_some_and(|i| i.state == "working" && i.remote.is_some() && i.holder.as_ref().is_some_and(|x| x.to() == to)));
        if !still {
            return;
        }
        if let Err(e) = self.tell(to, &text, crate::sessions::Kind::Route) {
            eprintln!("cue: desk: couldn't send #{id}'s start to its holder: {e}");
        }
    }

    /// A push card closed without an answer here (its session ended, dismissed, replied to in words): the starts it
    /// held go now, each saying it starts from older work, as a no would. Nothing pushed.
    pub(crate) fn desk_push_card_closed(&self, push: Option<Value>) {
        let Some(push) = push.filter(|v| v.is_object()) else { return };
        if push["starts"].as_array().is_some_and(|s| !s.is_empty()) {
            self.desk_push_answered(push, false);
        }
    }

    /// The push question (`push`: its card's `desk_push`) answered: a yes pushes once through the checked path (unless
    /// the project was set to never since), and every start held on it goes then, saying when it starts from older
    /// work. What was done is noted on the Desk. Nothing is sent to the lead: the question was Cue's.
    pub(crate) fn desk_push_answered(&self, push: Value, yes: bool) {
        self.later(0, move |h| {
            let desk = push["desk"].as_str().unwrap_or_default().to_string();
            let top = push["top"].as_str().unwrap_or_default().to_string();
            let starts = push["starts"].as_array().cloned().unwrap_or_default();
            // The decision is about the one item it held, when it held one.
            let item = if starts.len() == 1 { starts[0]["item"].as_str().map(str::to_string) } else { None };
            let pushed = if !yes {
                Err(String::new())
            } else if crate::desk::lands::lands_of(&desk, &top).push == "never" {
                Err("Not pushed: this project is set to never push".to_string())
            } else {
                h.desk_push_main(&desk, &top)
            };
            match &pushed {
                Ok(said) => h.desk_push_note(&desk, item.clone(), said.clone()),
                Err(said) if !said.is_empty() => h.desk_push_note(&desk, item.clone(), said.clone()),
                Err(_) => {}
            }
            for s in starts {
                let (Some(id), Some(to), Some(text)) = (s["item"].as_str(), s["to"].as_str(), s["text"].as_str()) else { continue };
                let mut text = text.to_string();
                if pushed.is_err() {
                    text.push('\n');
                    text.push_str(&older_words(s["ahead"].as_u64().map(|n| n as u32)));
                }
                h.desk_held_start_send(id, to, text);
            }
        });
    }

    /// `words` (a push or why not) noted on Desk `desk`, about `item` when one waited on it.
    fn desk_push_note(&self, desk: &str, item: Option<String>, words: String) {
        let mut fx = Effects::default();
        self.write(|st| {
            crate::desk::hub::turn::route::decide(st, desk, item, "You", "", &words, "push main", now_ms(), &mut fx);
            if let Some(d) = st.desk.decisions.last_mut() {
                d.undo = "none".into();
            }
        });
        self.desk_done(fx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git").args(args).current_dir(dir).env("GIT_CONFIG_NOSYSTEM", "1").output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A repo with a local bare repo as its remote (nothing leaves the machine), one commit pushed there.
    fn repo(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("cue-push-main-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (dir, bare) = (root.join("work"), root.join("remote.git"));
        std::fs::create_dir_all(&dir).unwrap();
        git(&root, &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.name", "Tester"]);
        git(&dir, &["config", "user.email", "tester@example.com"]);
        git(&dir, &["config", "commit.gpgsign", "false"]);
        git(&dir, &["remote", "add", "origin", bare.to_str().unwrap()]);
        commit(&dir, "README", "hello\n", "start");
        git(&dir, &["push", "-q", "-u", "origin", "main"]);
        (dir, root)
    }

    fn commit(dir: &Path, file: &str, text: &str, message: &str) {
        std::fs::write(dir.join(file), text).unwrap();
        git(dir, &["add", file]);
        git(dir, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", message]);
    }

    #[test]
    fn ahead_counts_the_commits_the_remote_doesnt_have() {
        let (dir, root) = repo("ahead");
        let d = dir.to_str().unwrap();
        assert_eq!(ahead(d, "origin", "main"), Some(0), "even");
        commit(&dir, "a.txt", "a\n", "one");
        commit(&dir, "b.txt", "b\n", "two");
        assert_eq!(ahead(d, "origin", "main"), Some(2));
        git(&dir, &["push", "-q", "origin", "main"]);
        assert_eq!(ahead(d, "origin", "main"), Some(0), "pushed");
        // A remote with no copy: every commit.
        git(&dir, &["remote", "add", "other", root.join("none.git").to_str().unwrap()]);
        assert_eq!(ahead(d, "other", "main"), Some(3));
        // Not a repository: not known, never even.
        assert_eq!(ahead(root.to_str().unwrap(), "origin", "main"), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_words_say_what_yes_does() {
        assert_eq!(
            offer_words(Some(5), "GitHub"),
            "Update from GitHub, then upload 5 landings? Yes rebases main onto the newest on GitHub, checks them for private words, then pushes. If a private word turns up, nothing is uploaded."
        );
        assert!(offer_words(Some(1), "GitHub").starts_with("Update from GitHub, then upload 1 landing? Yes rebases"));
        let start = start_words(Some(3), "the shared copy");
        assert!(start.starts_with("Update from the shared copy, then upload 3 landings before the other machine starts?"), "{start}");
        assert!(start.contains("rebases") && start.contains("private words"), "{start}");
        assert!(older_words(Some(2)).contains("starts from older work"));
        assert!(offer_words(None, "GitHub").contains("couldn't compare") && offer_words(None, "GitHub").contains("rebases"));
        assert!(start_words(None, "GitHub").contains("upload main first?"));
    }

    #[test]
    fn the_place_is_github_for_a_github_remote() {
        let (dir, root) = repo("place");
        let d = dir.to_str().unwrap();
        assert_eq!(place_of(d, "origin"), "the shared copy");
        git(&dir, &["remote", "add", "gh", "git@github.com:someone/thing.git"]);
        assert_eq!(place_of(d, "gh"), "GitHub");
        let _ = std::fs::remove_dir_all(root);
    }

    /// A second clone of `root`'s remote with one commit of `file` pushed from it: what's new on the remote.
    fn pushed_elsewhere(root: &Path, file: &str, text: &str) {
        let other = root.join("other");
        git(root, &["clone", "-q", root.join("remote.git").to_str().unwrap(), other.to_str().unwrap()]);
        git(&other, &["config", "user.name", "Tester"]);
        git(&other, &["config", "user.email", "tester@example.com"]);
        commit(&other, file, text, "from elsewhere");
        git(&other, &["push", "-q", "origin", "main"]);
    }

    #[test]
    fn main_is_rebased_onto_whats_new_on_the_remote() {
        let (dir, root) = repo("rebase");
        let d = dir.to_str().unwrap();
        // Even: nothing to do.
        assert_eq!(rebase_onto_remote(d, "origin", "main"), Ok(()));
        commit(&dir, "mine.txt", "mine\n", "mine");
        pushed_elsewhere(&root, "theirs.txt", "theirs\n");
        git(&dir, &["fetch", "-q", "origin"]);
        assert_eq!(rebase_onto_remote(d, "origin", "main"), Ok(()));
        // Theirs under, mine on top: one ahead, nothing behind, so the push goes as a fast-forward.
        assert_eq!(git(&dir, &["log", "--format=%s", "-2"]), "mine\nfrom elsewhere");
        assert_eq!(ahead(d, "origin", "main"), Some(1));
        git(&dir, &["push", "-q", "origin", "main"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_clashing_rebase_is_undone_and_main_left_as_it_was() {
        let (dir, root) = repo("clash");
        let d = dir.to_str().unwrap();
        commit(&dir, "README", "mine\n", "mine");
        pushed_elsewhere(&root, "README", "theirs\n");
        git(&dir, &["fetch", "-q", "origin"]);
        let before = git(&dir, &["rev-parse", "HEAD"]);
        let said = rebase_onto_remote(d, "origin", "main").unwrap_err();
        assert!(said.starts_with("Not pushed: couldn't rebase main"), "{said}");
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), before);
        assert_eq!(git(&dir, &["symbolic-ref", "--short", "HEAD"]), "main");
        assert!(crate::desk::hub::landing::main_line::in_progress(d).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn main_not_checked_out_is_not_rebased() {
        let (dir, root) = repo("away");
        let d = dir.to_str().unwrap();
        pushed_elsewhere(&root, "theirs.txt", "theirs\n");
        git(&dir, &["fetch", "-q", "origin"]);
        git(&dir, &["switch", "-q", "-c", "side"]);
        let said = rebase_onto_remote(d, "origin", "main").unwrap_err();
        assert!(said.contains("isn't checked out"), "{said}");
        let _ = std::fs::remove_dir_all(root);
    }
}
