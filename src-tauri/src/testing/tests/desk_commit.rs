//! An item's commit, made as its holder says DONE (hub/desk_finish.rs, with Looks right's commit code in
//! hub/desk_commit.rs): the item's files git says changed are committed at once (never pushed); a word never committed
//! stops it and asks you; the item is to check with its commit's short hash, and Looks right only closes it. When
//! another session at work is editing some of them, the commit waits, made on its own once they're free (across a
//! restart too), and you're asked once it has waited 15 minutes.

use super::desk::desk;
use super::team::{git, lead, says, writes};
use crate::hosted::HostedEvent;
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::World;
use serde_json::{json, Value};

pub(super) fn ends(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

pub(super) fn item(w: &World, project: &str, title: &str) -> Value {
    desk(w, project)["items"].as_array().unwrap().iter().find(|i| i["title"] == title).cloned().unwrap_or_else(|| panic!("no item {title}"))
}

/// A repository, a session whose crew wrote a.txt and b.txt for an item now to check: (repo, project, item id).
pub(super) fn ready(w: &World) -> (std::path::PathBuf, String, String) {
    ready_with(w, |_, _, _| {}, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    })
}

/// The same, the item's work being what `work` does (with the session and the repository) once it's routed, and
/// `before`'s before it was. Where its holder's DONE leaves it is the caller's to check.
pub(super) fn done_with(w: &World, before: impl Fn(&World, &str, &std::path::Path), work: impl Fn(&World, &str, &std::path::Path)) -> (std::path::PathBuf, String, String) {
    done_saying(w, before, work, "Fixed.")
}

/// `done_with`, its holder's DONE turn saying `words` first.
pub(super) fn done_saying(w: &World, before: impl Fn(&World, &str, &std::path::Path), work: impl Fn(&World, &str, &std::path::Path), words: &str) -> (std::path::PathBuf, String, String) {
    let (dir, project, id, h) = routed_with(w, before, work);
    ends(w, &h, &format!("{words}\nDONE: #{id}"));
    (dir, project, id)
}

/// `done_with` up to its holder's DONE: (repo, project, item id, holder).
pub(super) fn routed_with(w: &World, before: impl Fn(&World, &str, &std::path::Path), work: impl Fn(&World, &str, &std::path::Path)) -> (std::path::PathBuf, String, String, String) {
    let dir = super::team::main_repo(w);
    let h = lead(w, "hello");
    ends(w, &h, "Hi.");
    before(w, &h, &dir);
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(w, &d, "I run the Desk now.");
    w.advance(1000);
    ends(w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    w.advance(1000);
    work(w, &h, &dir);
    let id = item(w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
    (dir, project, id, h)
}

/// `done_with`, the item to check after it.
fn ready_with(w: &World, before: impl Fn(&World, &str, &std::path::Path), work: impl Fn(&World, &str, &std::path::Path)) -> (std::path::PathBuf, String, String) {
    let r = done_with(w, before, work);
    let it = item(w, &r.1, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    r
}

/// Cue's tick, as it runs every ~10 s (desk_commit_waiting, and through it desk_finish_tick).
pub(super) fn tick(w: &World) {
    w.tick(true);
    w.advance(20_000);
}

/// How many commits `dir`'s branch has.
pub(super) fn commits(dir: &std::path::Path) -> String {
    git(dir, &["rev-list", "--count", "HEAD"]).trim().to_string()
}

/// Session `sid`'s log, as Claude Code writes it.
pub(super) fn log(w: &World, sid: &str) -> crate::testing::transcript::Transcript {
    crate::testing::transcript::Transcript::at(w.hub().session_origin(sid).unwrap().transcript_path)
}

/// Session `sid`'s Edit of `path`, in its log.
pub(super) fn log_edit(w: &World, sid: &str, path: &std::path::Path) {
    log(w, sid).tool_use(&format!("toolu_e{}", crate::clock::now_ms()), "Edit", json!({ "file_path": path }));
    w.settle();
}

/// Looks right on `id`: { commit, files, waiting }.
fn looks_right(w: &World, id: &str) -> Value {
    let r = w.hub().desk_looks_right(id).unwrap();
    w.settle();
    r
}

#[test]
fn done_commits_the_items_changed_files_at_once_never_pushes_and_looks_right_only_closes_it() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "Header jumps\n\na.txt\nb.txt", "its title the message");
    let sha = git(&dir, &["rev-parse", "--short", "HEAD"]).trim().to_string();
    assert!(git(&dir, &["remote"]).trim().is_empty() && git(&dir, &["branch", "--format=%(upstream)"]).trim().is_empty(), "nothing to push to, nothing pushed");
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["commit"].as_str(), shown["finish"]["step"].as_str()), (Some("to_check"), Some(sha.as_str()), Some("checked")), "not this Cue's source: nothing to install: {shown}");
    let r = looks_right(&w, &id);
    assert_eq!(r["commit"].as_str(), Some(sha.as_str()), "{r}");
    assert_eq!(commits(&dir), "2", "no second commit");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
}

#[test]
fn a_file_already_taken_out_with_git_rm_is_committed_as_removed() {
    let w = World::new();
    let (dir, _, _) = ready_with(
        &w,
        |_, _, dir| {
            std::fs::write(dir.join("shared.txt"), "old\n").unwrap();
            git(dir, &["add", "shared.txt"]);
            git(dir, &["commit", "-q", "-m", "shared"]);
        },
        |w, h, dir| {
            let f = |n: &str| dir.join(n).to_string_lossy().to_string();
            says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("shared.txt")]));
            git(dir, &["rm", "-q", "-f", "shared.txt"]);
        },
    );
    assert_eq!(git(&dir, &["show", "--name-status", "--format=", "HEAD"]).trim(), "A\ta.txt\nD\tshared.txt", "the removal goes in with the rest");
}

/// #337: you tell the lead an item is done, and its DONE closes it as Looks right would; one it can't close (no such
/// item) it's told why, once.
#[test]
fn the_leads_done_on_an_item_to_check_closes_it_and_tells_the_lead_why_not_once_for_one_it_cant() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let told = |w: &World| w.hosted.sent().into_iter().filter(|(s, t)| s == &lead && t.contains("not closed")).map(|(_, t)| t).collect::<Vec<_>>();
    ends(&w, &lead, &format!("The user checked it.\nDONE: #{id}, #zz9"));
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "done", "closed as Looks right would: {shown}");
    assert!(shown.get("close_on_check").is_none_or(|v| v == false), "nothing left to close: {shown}");
    assert_eq!(commits(&dir), "2", "its DONE's commit, no second one");
    let said = told(&w);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].starts_with("#zz9 not closed: this Desk has no item #zz9\n"), "{}", said[0]);
    assert!(said[0].ends_with("Don't say DONE for these again."), "its words for the lead last");
    // Said again: the item is done (told once), #zz9 unchanged (not told again).
    ends(&w, &lead, &format!("DONE: #{id}, #zz9"));
    w.settle();
    let said = told(&w);
    assert_eq!(said.len(), 2, "{said:?}");
    assert_eq!(said[1].lines().next(), Some(format!("#{id} not closed: it's done already").as_str()));
}

#[test]
fn a_sessions_follow_up_stays_in_its_item_and_only_a_separate_one_with_a_why_is_added() {
    let w = World::new();
    let (_, project, id) = ready(&w);
    let h = item(&w, &project, "Header jumps")["holder"]["sid"].as_str().unwrap().to_string();
    let count = |w: &World| desk(w, &project)["items"].as_array().unwrap().len();
    let before = count(&w);
    ends(&w, &h, "Found one more spot.\nITEM: bug | Header jumps on narrow windows | done when: it stays put there too");
    assert_eq!(count(&w), before, "a follow-up isn't a new item");
    let told: Vec<String> = w.hosted.sent().into_iter().filter(|(s, t)| s == &h && t.contains("Not added")).map(|(_, t)| t).collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains(&format!("stays in #{id}")) && told[0].contains("| why:"), "{}", told[0]);
    ends(&w, &h, "ITEM: feature | Export the board | why: a new ask, nothing to do with the header");
    assert_eq!(count(&w), before + 1, "a separate one with a why is added");
    assert_eq!(item(&w, &project, "Export the board")["why"], "a new ask, nothing to do with the header");
}

#[test]
fn an_items_why_needs_its_colon_so_a_title_starting_why_stays_the_title() {
    use crate::desk::parse::{lines, Line};
    let got = lines("ITEM: bug | Why the header jumps | done when: it stays put | why: not the item I'm on");
    let want = Line::Item { kind: "bug".into(), title: "Why the header jumps".into(), see: String::new(), done_when: "it stays put".into(), why: "not the item I'm on".into() };
    assert_eq!(got, vec![want]);
}

#[test]
fn a_word_never_committed_stops_the_commit_asks_you_and_the_next_done_tries_again() {
    let w = World::new();
    let never = |w: &World, h: &str, _: &std::path::Path| {
        let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
        w.hub().set_setting("project.never", json!({ project: ["Fixer"] })).unwrap();
    };
    let (dir, project, id) = done_with(&w, never, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("needs_you"), Some("word")), "{shown}");
    let q = shown["latest"].as_str().unwrap();
    assert!(q.contains("\"Fixer\"") && q.contains(": fixer wrote this"), "{q}");
    assert_eq!(commits(&dir), "1", "nothing committed");
    assert!(git(&dir, &["diff", "--cached", "--name-only"]).trim().is_empty(), "the index untouched");

    // Taken out, and DONE again: committed then.
    w.hub().set_setting("project.never", json!({})).unwrap();
    let h = shown["holder"]["sid"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Took it out.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    assert_eq!(commits(&dir), "2");
}

#[test]
fn an_item_with_no_changed_files_goes_to_check_with_no_commit_and_looks_right_closes_it() {
    let w = World::new();
    let (dir, project, id) = ready_with(&w, |_, _, _| {}, |_, _, _| {});
    assert_eq!(item(&w, &project, "Header jumps")["commit"].as_str().unwrap_or(""), "");
    let r = looks_right(&w, &id);
    assert_eq!((r["commit"].as_str(), r["files"].as_u64()), (Some(""), Some(0)));
    assert_eq!(commits(&dir), "1");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
}

/// A session "tidier" at work editing b.txt in `dir`: its id.
pub(super) fn tidier_editing(w: &World, dir: &std::path::Path) -> String {
    let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "tidy b", "tidier", &opts).unwrap();
    let o = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&o, HostedEvent::Taken { text: "tidy b".into() });
    w.advance(1000);
    log(w, &o).tool_use("toolu_9", "Edit", json!({ "file_path": dir.join("b.txt") }));
    w.settle();
    o
}

/// An item whose crew wrote a.txt and b.txt, said DONE while "tidier" is at work editing b.txt: (repo, project,
/// tidier).
pub(super) fn done_while_tidier_edits(w: &World) -> (std::path::PathBuf, String, String) {
    let o = std::cell::RefCell::new(String::new());
    let (dir, project, _) = done_with(w, |_, _, _| {}, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
        *o.borrow_mut() = tidier_editing(w, dir);
    });
    (dir, project, o.into_inner())
}

#[test]
fn a_file_another_session_is_editing_is_left_out_and_the_rest_lands_at_once() {
    let w = World::new();
    let (dir, project, _) = done_while_tidier_edits(&w);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "the commit never waits on another session: {shown}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "Header jumps\n\na.txt");
    assert!(git(&dir, &["status", "--porcelain"]).contains("b.txt"), "b.txt stays in the folder, uncommitted");
    assert!(shown["finish"]["without"].as_array().unwrap().iter().any(|p| p.as_str().unwrap().ends_with("b.txt")), "{shown}");
}

#[test]
fn undo_on_a_done_item_puts_it_back_to_check_and_its_commit_stays() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    let sha = item(&w, &project, "Header jumps")["commit"].as_str().unwrap().to_string();
    looks_right(&w, &id);
    let it = w.hub().desk_commit_undo(&id).unwrap();
    assert_eq!((it.state.as_str(), it.commit.as_str()), ("to_check", sha.as_str()));
    assert_eq!(commits(&dir), "2", "the commit stays");
    assert!(git(&dir, &["status", "--porcelain"]).trim().is_empty(), "code untouched");
    assert!(w.hub().desk_commit_undo(&id).is_err(), "it isn't done now");
}

#[test]
fn a_session_doing_the_item_itself_commits_what_it_changed_while_it_held_it() {
    let w = World::new();
    let earlier = |w: &World, h: &str, dir: &std::path::Path| {
        std::fs::write(dir.join("shared.txt"), "earlier work\n").unwrap();
        log(w, h).tool_use("toolu_0", "Edit", json!({ "file_path": dir.join("shared.txt") }));
    };
    let (dir, _, _) = ready_with(&w, earlier, |w, h, dir| {
        std::fs::write(dir.join("a.txt"), "edited\n").unwrap();
        std::fs::write(dir.join("b.txt"), "from the shell\n").unwrap();
        std::fs::write(dir.join("by_builder.txt"), "read only\n").unwrap();
        log(w, h)
            .tool_use("toolu_1", "Edit", json!({ "file_path": dir.join("a.txt") }))
            .tool_use("toolu_2", "Bash", json!({ "command": "echo hi > b.txt" }))
            .tool_use("toolu_3", "Read", json!({ "file_path": dir.join("by_builder.txt") }))
            .tool_use("toolu_4", "Edit", json!({ "file_path": "/elsewhere/x.txt" }));
    });
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt\nb.txt", "its own edits while it held the item; not one from before, a read, or one outside its folder");
}

#[test]
fn each_done_leaves_a_card_for_the_lead_with_who_what_to_test_and_its_files_kept_after_a_restart() {
    let w = World::new();
    let (dir, project, id) = ready(&w);
    let cards = |w: &World| desk(w, &project)["checks"].as_array().unwrap().clone();
    let c = &cards(&w)[0];
    assert_eq!((c["id"].as_str(), c["title"].as_str(), c["kind"].as_str()), (Some(id.as_str()), Some("Header jumps"), Some("bug")));
    assert_eq!((c["by"].as_str(), c["done_when"].as_str(), c["state"].as_str(), c["last"].as_bool()), (Some("cue-teams"), Some("it stays put"), Some("to_check"), Some(true)));
    assert_eq!(c["files"], json!(["a.txt", "b.txt"]), "the files its commit took: {c}");
    assert_eq!(c["cue"], false, "not made in this Cue's source");
    assert_eq!(c["finish"]["step"], "checked", "{c}");
    assert!(w.snapshot().0["desk"]["installed_ms"].as_u64().is_some_and(|t| t > 0));

    // Kept: what Cue loads as it starts has it.
    let (items, _, _) = crate::db::desk_load(crate::clock::now_ms());
    let kept = items.iter().find(|i| i.id == id).unwrap();
    assert_eq!((kept.checks.len(), kept.checks[0].by.as_str(), kept.checks[0].files.len()), (1, "cue-teams", 2));

    // Sent back and done again: a second card; the first is no longer the one to act on.
    w.hub().desk_item_act(&id, "send_back", "It still jumps").unwrap();
    w.settle();
    // In its Steps too, by you (#136).
    let decs = w.snapshot().0["desk"]["projects"][0]["decisions"].clone();
    assert!(decs.as_array().unwrap().iter().any(|d| d["by"] == "You" && d["text"] == "It still jumps" && d["why"] == "sent back to cue-teams"), "{decs}");
    let snap = w.snapshot();
    let h = snap.session_ids().into_iter().find(|s| snap.session(s)["name"] == "cue-teams").unwrap();
    // A real fix (a reopened item with nothing new on main goes back to its session, #448).
    std::fs::write(dir.join("a.txt"), "fixed properly\n").unwrap();
    log_edit(&w, &h, &dir.join("a.txt"));
    ends(&w, &h, &format!("Fixed it properly.\nDONE: #{id}"));
    let all = cards(&w);
    assert_eq!(all.len(), 2, "{all:?}");
    assert_eq!((all[0]["last"].as_bool(), all[1]["last"].as_bool()), (Some(false), Some(true)));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check", "finished again");
}

#[test]
fn send_back_takes_your_words_to_its_holder_and_puts_it_to_work_with_none_it_asks_for_a_reason() {
    let w = World::new();
    let (_, project, id) = ready(&w);
    let e = w.hub().desk_item_act(&id, "send_back", "  ").unwrap_err();
    assert!(e.contains("Say what isn't right"), "{e}");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check", "unchanged");

    let it = w.hub().desk_item_act(&id, "send_back", "It still jumps on scroll").unwrap();
    w.settle();
    assert_eq!(it.holder.as_ref().map(|h| h.name.as_str()), Some("cue-teams"));
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working");
    assert!(it.finish.is_none(), "its next DONE finishes it again");
    let snap = w.snapshot();
    let h = snap.session_ids().into_iter().find(|s| snap.session(s)["name"] == "cue-teams").unwrap();
    let sent: Vec<String> = w.hosted.sent().into_iter().filter(|(s, _)| *s == h).map(|(_, t)| t).collect();
    assert!(sent.iter().any(|t| t.contains("isn't done: It still jumps on scroll") && t.contains(&format!("DONE: #{id}"))), "{sent:?}");
    assert!(!sent.iter().any(|t| t.contains("have another look")), "no made-up words");
}

#[test]
fn a_card_sent_back_keeps_why_and_how_many_pictures_went_with_it_across_a_restart() {
    let mut w = World::new();
    let (_, project, id) = ready(&w);
    w.hub().desk_item_act(&id, "send_back", "It still jumps\nPictures with it (open each to see it): `/u/a.png` `/u/b.png`").unwrap();
    w.settle();
    w.relaunch();
    let card = desk(&w, &project)["checks"].as_array().unwrap().iter().find(|c| c["id"] == id.as_str()).cloned().unwrap();
    assert_eq!((card["back"].as_str(), card["back_pics"].as_u64(), card["state"].as_str()), (Some("It still jumps"), Some(2), Some("working")), "{card}");
}

#[test]
fn files_changed_is_its_commit_once_its_done_and_after_a_restart() {
    let mut w = World::new();
    let (dir, _, id) = ready(&w);
    std::fs::write(dir.join("yours.txt"), "not the item's\n").unwrap();
    let files = |w: &World| -> Vec<String> {
        let d = w.hub().desk_item_diff(&id).unwrap();
        d["diff"].as_str().unwrap().lines().filter_map(|l| l.strip_prefix("+++ b/")).map(str::to_string).collect()
    };
    assert_eq!(files(&w), ["a.txt", "b.txt"], "its commit, not your file");
    assert!(git(&dir, &["diff", "--cached", "--name-only"]).trim().is_empty(), "your index untouched");
    looks_right(&w, &id);
    assert_eq!(files(&w), ["a.txt", "b.txt"], "done: its commit");
    w.relaunch();
    assert_eq!(files(&w), ["a.txt", "b.txt"], "after a restart too");
}

/// What the holder says as each turn on an item ends is kept on it, its Desk lines left out, the DONE one marked done:
/// the item page's Thread shows them as its replies (#140).
#[test]
fn a_holders_words_at_each_turn_end_are_kept_on_its_item() {
    let w = World::new();
    let (_dir, project, id) = ready_with(&w, |_, _, _| {}, |w, h, _| ends(w, h, "Looking at **header.js** now."));
    let said = item(&w, &project, "Header jumps")["said"].as_array().cloned().unwrap_or_default();
    let got: Vec<(String, bool)> = said.iter().map(|r| (r["text"].as_str().unwrap().to_string(), r["done"].as_bool().unwrap_or(false))).collect();
    assert_eq!(got, vec![("Looking at **header.js** now.".to_string(), false), ("Fixed.".to_string(), true)], "{said:?} on #{id}");
    assert!(said.iter().all(|r| r["by"] == "cue-teams" && r["at"].as_u64().unwrap() > 0), "{said:?}");
}

/// An item whose crew wrote a.txt and b.txt, then session "tidier" (its turn over, its change not committed)
/// changed a.txt as `change` does to it and its log: (repo, project).
pub(super) fn done_after_tidier_changed(w: &World, change: impl Fn(&World, &str, &std::path::Path)) -> (std::path::PathBuf, String) {
    let (dir, project, _) = done_with(w, |_, _, _| {}, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
        let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
        let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "tidy a", "tidier", &opts).unwrap();
        let o = r["session_id"].as_str().unwrap().to_string();
        w.hosted_says(&o, HostedEvent::Taken { text: "tidy a".into() });
        w.advance(1000);
        change(w, &o, &dir.join("a.txt"));
        ends(w, &o, "Tidied.");
    });
    (dir, project)
}

#[test]
fn another_sessions_uncommitted_edit_in_the_same_file_stays_out_of_the_commit_and_in_the_file() {
    let w = World::new();
    let (dir, project) = done_after_tidier_changed(&w, |w, o, a| {
        std::fs::write(a, "fixer wrote this\ntidier's line\n").unwrap();
        log(w, o).tool_use("toolu_t1", "Edit", json!({ "file_path": a, "old_string": "fixer wrote this\n", "new_string": "fixer wrote this\ntidier's line\n" }));
        w.settle();
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert_eq!(git(&dir, &["show", "HEAD:a.txt"]), "fixer wrote this\n", "only the item's part is committed");
    assert_eq!(git(&dir, &["show", "HEAD:b.txt"]), "fixer wrote this\n");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "fixer wrote this\ntidier's line\n", "the file stays as it is");
    assert_eq!(git(&dir, &["status", "--porcelain", "--", "a.txt", "b.txt"]).trim(), "M a.txt", "tidier's line stays uncommitted");
    assert_eq!(git(&dir, &["diff", "--cached", "--name-only"]).trim(), "", "nothing left staged");
}

#[test]
fn another_sessions_change_that_cant_be_told_apart_is_left_out_of_the_commit() {
    let w = World::new();
    let (dir, project) = done_after_tidier_changed(&w, |w, o, a| {
        std::fs::write(a, "tidier rewrote it\n").unwrap();
        log(w, o).tool_use("toolu_t1", "Write", json!({ "file_path": a, "content": "tidier rewrote it\n" }));
        w.settle();
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "Header jumps\n\nb.txt");
    assert!(shown["finish"]["without"].as_array().unwrap().iter().any(|p| p.as_str().unwrap().ends_with("a.txt")), "{shown}");

}

#[test]
fn a_teammate_started_for_an_item_that_moved_to_another_session_stays_out_of_the_items_commit_it_stayed_with() {
    // #302: cue-teams holds Header, then Footer; its crew writes a.txt for Header and b.txt for Footer; Footer moves to
    // "second". Header's DONE commits a.txt only: b.txt was Footer's, though Footer is no longer cue-teams'.
    let w = World::new();
    let dir = super::team::main_repo(&w);
    let h = lead(&w, "hello");
    ends(&w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let s = super::desk::second(&w, "second");
    let d = super::desk::new_lead(&w, &project);
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    w.advance(1000);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    w.advance(1000);
    says(&w, &h, writes("toolu_A", "agentA", "header-fix", &[f("a.txt")]));
    w.advance(1000);
    ends(&w, &d, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the footer");
    w.advance(1000);
    says(&w, &h, writes("toolu_B", "agentB", "footer-fix", &[f("b.txt")]));
    w.advance(1000);
    let footer = item(&w, &project, "Footer jumps")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ROUTE: #{footer} -> second | why: free now"));
    let moved = item(&w, &project, "Footer jumps");
    assert_eq!((moved["holder"]["sid"].as_str(), moved["state"].as_str()), (Some(s.as_str()), Some("working")), "{moved}");
    w.advance(1000);
    let header = item(&w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Fixed.\nDONE: #{header}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt", "its own teammate's file, not the one started for Footer");
    assert!(git(&dir, &["status", "--porcelain"]).contains("b.txt"), "Footer's file stays for Footer");
}

#[test]
fn a_teammate_an_item_was_given_to_before_it_moved_away_stays_out_of_its_old_holders_other_item() {
    // #302, crew side: Footer went to cue-teams's teammate footer-fix, then moved to "second". Header, which cue-teams
    // keeps, still leaves footer-fix's b.txt out.
    let w = World::new();
    let dir = super::team::main_repo(&w);
    let h = lead(&w, "hello");
    ends(&w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let s = super::desk::second(&w, "second");
    let d = super::desk::new_lead(&w, &project);
    let f = |n: &str| dir.join(n).to_string_lossy().to_string();
    w.advance(1000);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    w.advance(1000);
    says(&w, &h, writes("toolu_A", "agentA", "header-fix", &[f("a.txt")]));
    says(&w, &h, writes("toolu_B", "agentB", "footer-fix", &[f("b.txt")]));
    w.advance(1000);
    ends(&w, &d, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> footer-fix | why: it knows the footer");
    let given = item(&w, &project, "Footer jumps");
    assert_eq!(given["holder"]["kind"], "crew", "{given}");
    w.advance(1000);
    let footer = given["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ROUTE: #{footer} -> second | why: free now"));
    assert_eq!(item(&w, &project, "Footer jumps")["holder"]["sid"].as_str(), Some(s.as_str()));
    w.advance(1000);
    let header = item(&w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Fixed.\nDONE: #{header}"));
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt", "footer-fix's file was Footer's");
}
