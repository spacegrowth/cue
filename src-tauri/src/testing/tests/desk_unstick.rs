//! The Desk lead unsticks its Desk itself (#347, hub/desk_unstick.rs): CLOSE an item whose work is already on main,
//! TAKE COPY back from a stuck or finished item, LAND FIRST to move one up in the landing line, START a waiting item
//! now. Each is one of the item's steps; what can't be done is told to the lead.

use super::desk::desk;
use super::desk_commit::{ends, item, routed_with};
use super::team::git;
use crate::desk::parse::{lines, Line};
use crate::testing::World;
use serde_json::Value;

fn lead_of(w: &World, project: &str) -> String {
    desk(w, project)["lead"]["sid"].as_str().unwrap().to_string()
}

/// The main checkout's top folder, as git lists it.
pub(super) fn top(w: &World) -> String {
    let out = String::from_utf8(std::process::Command::new("git").arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
    out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
}

/// Item `id`'s steps, as the Desk lists them (newest first).
fn steps(w: &World, project: &str, id: &str) -> Vec<Value> {
    desk(w, project)["decisions"].as_array().unwrap().iter().filter(|d| d["item_id"] == id).cloned().collect()
}

fn sent_to(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

/// A second item, "Footer", routed to the same holder.
fn footer(w: &World, project: &str) -> String {
    ends(w, &lead_of(w, project), "ITEM: bug | Footer | done when: it shows\nROUTE: #new -> cue-teams | why: it made the footer");
    w.advance(1000);
    item(w, project, "Footer")["id"].as_str().unwrap().to_string()
}

#[test]
fn the_lines_that_unstick_an_item_are_read() {
    let l = lines("CLOSE: #12 | commit: ab12cd3 | why: it landed by hand\nTAKE COPY: #4 | why: it's done\nLAND FIRST: #5 | why: the user waits on it\nSTART: #6 | the wait is moot\nPAUSE: #7 | why: #9 is urgent\nRESUME: #7 | why: #9 is done");
    let got: Vec<(String, String, String, String)> = l.into_iter().map(|x| match x {
        Line::Unstick { what, id, commit, why } => (what, id, commit, why),
        other => panic!("{other:?}"),
    }).collect();
    let want = [("close", "12", "ab12cd3", "it landed by hand"), ("take_copy", "4", "", "it's done"), ("land_first", "5", "", "the user waits on it"), ("start", "6", "", "the wait is moot"), ("pause", "7", "", "#9 is urgent"), ("resume", "7", "", "#9 is done")];
    assert_eq!(got, want.map(|(a, b, c, d)| (a.to_string(), b.to_string(), c.to_string(), d.to_string())));
    assert!(lines("CLOSE: #12 | commit: not-a-sha").is_empty(), "a commit that isn't one");
    assert!(lines("START: the build").is_empty(), "prose, not an item");
}

#[test]
fn the_lead_starts_an_item_waiting_for_a_copy_now_and_it_works_where_its_holder_is() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[1, 2, 3]), |_, _, _| {});
    assert_eq!(item(&w, &project, "Header jumps")["state"], "queued");
    ends(&w, &lead_of(&w, &project), &format!("START: #{id} | why: the user needs it now"));
    w.advance(1000);
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree_wait"].as_bool().unwrap_or(false), it["tree"].is_null()), (Some("working"), false, true), "{it}");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{id}")) && t.contains("the user needs it now")), "its holder is told");
    let s = steps(&w, &project, &id);
    assert!(s.iter().any(|d| d["text"] == format!("Started #{id} now, without waiting for a copy") && d["why"] == "the user needs it now" && d["act"] == "start"), "{s:?}");
    w.hub().desk_tick_now();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working", "not queued for a copy again");
}

#[test]
fn the_lead_cant_start_an_item_that_isnt_waiting_and_is_told_why() {
    let w = World::new();
    let (_, project, id, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let d = lead_of(&w, &project);
    ends(&w, &d, &format!("START: #{id} | why: go"));
    w.advance(2000);
    w.settle();
    assert!(sent_to(&w, &d).iter().any(|t| t.contains(&format!("#{id} not started: it isn't waiting (it's working)"))), "{:?}", sent_to(&w, &d));
}

#[test]
fn the_lead_takes_back_a_stuck_items_copy_its_work_is_kept_and_the_item_waiting_for_one_starts_in_it() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |w, _, _| w.hub().desk_hold_copies(&top(w), &[2, 3]), |_, _, _| {});
    let it = item(&w, &project, "Header jumps");
    let path = it["tree"]["path"].as_str().unwrap_or_else(|| panic!("no copy: {it}")).to_string();
    std::fs::write(std::path::Path::new(&path).join("a.txt"), "half done\n").unwrap();
    let f = footer(&w, &project);
    assert_eq!(item(&w, &project, "Footer")["tree_wait"], true, "all copies held");
    ends(&w, &lead_of(&w, &project), &format!("TAKE COPY: #{id} | why: it stalled and #{f} needs a copy"));
    w.advance(1000);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"].is_null(), it["holder"].is_null()), (Some("new"), true, true), "new again: {it}");
    assert!(git(std::path::Path::new(&top(&w)), &["branch", "--list", &format!("desk/{id}")]).contains(&format!("desk/{id}")), "its work kept on its branch");
    assert_eq!(desk(&w, &project)["kept"][0]["id"], id.as_str(), "and its copy listed");
    let footer = item(&w, &project, "Footer");
    assert_eq!((footer["state"].as_str(), footer["tree"]["slot"].as_u64()), (Some("working"), Some(1)), "{footer}");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{id}: the Desk lead took back its copy"))), "its holder is told to stop");
    let s = steps(&w, &project, &id);
    assert!(s.iter().any(|d| d["text"].as_str().unwrap().starts_with(&format!("Took back #{id}'s copy")) && d["act"] == "take_copy" && d["why"].as_str().unwrap().starts_with("it stalled")), "{s:?}");
}

#[test]
fn the_lead_moves_an_item_up_in_the_landing_line_and_it_lands_first() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let f = footer(&w, &project);
    let copy = |title: &str| std::path::PathBuf::from(item(&w, &project, title)["tree"]["path"].as_str().unwrap_or_else(|| panic!("no copy for {title}")));
    std::fs::write(copy("Header jumps").join("a.txt"), "a\n").unwrap();
    std::fs::write(copy("Footer").join("b.txt"), "b\n").unwrap();
    // The main checkout mid-merge: both wait in the line, the header first (said DONE first).
    std::fs::write(dir.join(".git/MERGE_HEAD"), git(&dir, &["rev-parse", "HEAD"])).unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    w.advance(1000);
    ends(&w, &h, &format!("Fixed too.\nDONE: #{f}"));
    w.advance(1000);
    assert_eq!(item(&w, &project, "Footer")["finish"]["step"], "line", "{}", item(&w, &project, "Footer"));
    ends(&w, &lead_of(&w, &project), &format!("LAND FIRST: #{f} | why: the user waits on the footer"));
    w.advance(1000);
    w.settle();
    let s = steps(&w, &project, &f);
    assert!(s.iter().any(|d| d["text"] == format!("Moved #{f} up in the landing line") && d["act"] == "land_first"), "{s:?}");
    std::fs::remove_file(dir.join(".git/MERGE_HEAD")).unwrap();
    w.hub().desk_tick_now();
    w.settle();
    let log = git(&dir, &["log", "-2", "--format=%s"]);
    assert_eq!(log.trim(), format!("Header jumps (#{id})\nFooter (#{f})"), "the footer landed first");
}

#[test]
fn the_lead_closes_an_item_whose_work_is_already_on_main_and_its_copy_goes() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let path = std::path::PathBuf::from(item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap());
    // Its work landed on main by hand.
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "header by hand"]);
    let sha = git(&dir, &["rev-parse", "--short=9", "HEAD"]).trim().to_string();
    ends(&w, &lead_of(&w, &project), &format!("CLOSE: #{id} | commit: {sha} | why: it landed by hand"));
    w.advance(1000);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"].is_null()), (Some("done"), true), "{it}");
    assert!(!path.exists(), "its copy, with nothing of its own, is gone");
    let short = &sha[..7];
    let s = steps(&w, &project, &id);
    assert!(s.iter().any(|d| d["text"] == format!("Closed #{id}: its work is on main ({short})") && d["why"] == "it landed by hand" && d["act"] == "close"), "{s:?}");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{id} is closed"))), "its holder stops");
}

#[test]
fn the_lead_cant_close_an_item_whose_commit_isnt_on_main_and_is_told_where_cue_looked() {
    let w = World::new();
    let (dir, project, id, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    git(&dir, &["switch", "-q", "-c", "aside"]);
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "aside"]);
    let sha = git(&dir, &["rev-parse", "--short", "HEAD"]).trim().to_string();
    git(&dir, &["switch", "-q", "main"]);
    let d = lead_of(&w, &project);
    ends(&w, &d, &format!("CLOSE: #{id} | commit: {sha} | why: done"));
    w.advance(1000);
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "working");
    assert!(sent_to(&w, &d).iter().any(|t| t.contains(&format!("Desk item #{id} not closed: {sha} isn't on main"))), "{:?}", sent_to(&w, &d));
}

/// The lead pauses an item at work to save tokens for an urgent one (#365): its holder is told to stop at a clean point
/// and keeps it; it shows as paused, its holder's turn ending on it isn't a stop (no "Done?", the lead isn't told),
/// and RESUME sends it on from where it stopped.
#[test]
fn the_lead_pauses_an_item_at_work_and_resumes_it_later() {
    let w = World::new();
    let (_, project, id, h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let l = lead_of(&w, &project);
    w.hosted_says(&h, crate::hosted::HostedEvent::Taken { text: format!("Desk item #{id}: Header jumps") });
    w.hosted_says(&h, crate::hosted::HostedEvent::TurnStarted);
    w.settle();
    ends(&w, &l, &format!("PAUSE: #{id} | why: #9 is urgent"));
    w.advance(1000);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["paused"][1].as_str(), it["holder"]["sid"].as_str()), (Some("paused"), Some("#9 is urgent"), Some(h.as_str())), "{it}");
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("Desk item #{id} is paused by the Desk lead (#9 is urgent). Stop at the next clean point"))), "{:?}", sent_to(&w, &h));
    let s = steps(&w, &project, &id);
    assert!(s.iter().any(|d| d["text"] == format!("Paused #{id}") && d["act"] == "pause" && d["why"] == "#9 is urgent"), "{s:?}");
    // It stops where it is: not a stop for the lead, nor asked if it's done.
    w.advance(1000);
    ends(&w, &h, "Stopped after the header test. Next: the scroll offset.");
    w.advance(1000);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool().unwrap_or(false)), (Some("paused"), false), "{it}");
    assert!(!sent_to(&w, &h).iter().any(|t| t.starts_with("Done? Write DONE")), "not asked if it's done");
    assert!(!sent_to(&w, &l).iter().any(|t| t.contains(&format!("#{id} (Header jumps) stopped"))), "the lead isn't told it stopped");
    assert!(lead_summary(&w, &project, &l).contains(&format!("#{id} bug: Header jumps | paused | held by")), "{}", lead_summary(&w, &project, &l));
    // Paused twice: told so.
    ends(&w, &l, &format!("PAUSE: #{id} | why: again"));
    w.advance(1000);
    w.settle();
    assert!(sent_to(&w, &l).iter().any(|t| t.contains(&format!("#{id} not paused: it's paused already"))), "{:?}", sent_to(&w, &l));
    // Resumed: at work again, its holder told to carry on.
    ends(&w, &l, &format!("RESUME: #{id} | why: #9 is done"));
    w.advance(1000);
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert!(it["paused"].is_null() && it["state"] != "paused", "{it}");
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with(&format!("Desk item #{id} is resumed (#9 is done): carry on from where you stopped"))), "{:?}", sent_to(&w, &h));
    assert!(steps(&w, &project, &id).iter().any(|d| d["text"] == format!("Resumed #{id}") && d["act"] == "resume"));
    ends(&w, &l, &format!("RESUME: #{id} | why: go"));
    w.advance(1000);
    w.settle();
    assert!(sent_to(&w, &l).iter().any(|t| t.contains(&format!("#{id} not resumed: it isn't paused"))), "{:?}", sent_to(&w, &l));
}

/// Only an item at work can be paused; the lead is told why not.
#[test]
fn the_lead_cant_pause_an_item_that_isnt_at_work() {
    let w = World::new();
    let (_, project, _, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let l = lead_of(&w, &project);
    ends(&w, &l, "ITEM: bug | Footer | done when: it shows");
    w.advance(1000);
    let f = item(&w, &project, "Footer")["id"].as_str().unwrap().to_string();
    ends(&w, &l, &format!("PAUSE: #{f} | why: later"));
    w.advance(1000);
    w.settle();
    assert!(sent_to(&w, &l).iter().any(|t| t.contains(&format!("#{f} not paused: only an item at work can be paused (it's new)"))), "{:?}", sent_to(&w, &l));
}

fn lead_summary(w: &World, project: &str, l: &str) -> String {
    w.hub().desk_summary(project, l, false).0
}
