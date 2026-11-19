//! ROUTE lines that are made (hub/desk_route.rs): an item routed to a holder, to new or to the lead, to nobody, to
//! another holder, said again; a new session that can't start, Start now, and an item's files.
//! Routes Cue can't make and their answers: desk_routes.rs.

use super::desk::{desk, ends, holder, item, new_lead, second, sent_to, summary};
use super::team::{says, writes};
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::json;

#[test]
fn an_item_routed_goes_to_its_holder_and_done_puts_it_to_check() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let desk_now = desk(&w, &project);
    let it = item(&desk_now, "Header jumps");
    assert_eq!(it["state"], "working");
    assert_eq!(it["holder"]["name"], "cue-teams");
    assert_eq!(it["from"], format!("{project}-lead"));
    assert!(it["holder"]["kind"] == "session" || it["holder"]["kind"] == "lead");
    let id = it["id"].as_str().unwrap().to_string();
    assert!(desk_now["decisions"][0]["text"].as_str().unwrap().starts_with("Routed"), "{desk_now}");
    assert_eq!(desk_now["decisions"][0]["by"], format!("{project}-lead"));
    assert!(desk_now["decisions"][0].get("call").is_none(), "a route is a step, not a call: {desk_now}");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Say DONE: #{id}"))));
    ends(&w, &h, &format!("Fixed.\nDECIDED: kept the sticky header — it was asked for\nDONE: #{id}"));
    let desk_now = desk(&w, &project);
    assert_eq!(item(&desk_now, "Header jumps")["state"], "to_check");
    assert_eq!(desk_now["decisions"][0]["by"], "cue-teams");
    assert_eq!(desk_now["decisions"][0]["item_id"], id.as_str());
    assert_eq!(desk_now["decisions"][0]["call"], true, "a DECIDED line is a call: {desk_now}");
}

/// An item's files are those changed for it: by its holder's crew started after it went to work, inside their
/// folder. Work from before it, and files outside the folder, aren't its. The lead sees five, then a count.
#[test]
fn an_items_files_are_only_those_changed_for_it_inside_the_folder() {
    let w = World::new();
    let (h, project) = holder(&w);
    let dir = w.home().to_path_buf();
    let outside = std::env::temp_dir().join(format!("cue-desk-outside-{}.txt", std::process::id())).to_string_lossy().to_string();
    says(&w, &h, writes("toolu_A", "agentA", "earlier", &[dir.join("old.txt").to_string_lossy().to_string(), outside.clone()]));
    let d = new_lead(&w, &project);
    w.advance(1000);
    ends(&w, &d, "ITEM: bug | Header jumps\nROUTE: #new -> cue-teams | why: it made the header");
    assert!(item(&desk(&w, &project), "Header jumps")["files"].as_array().is_none_or(|f| f.is_empty()), "no work for it yet");
    w.advance(1000);
    let mut mine: Vec<String> = (1..=7).map(|n| dir.join(format!("f{n}.txt")).to_string_lossy().to_string()).collect();
    // A name that starts with dots is a file inside; a "../" step is outside.
    mine.push(dir.join("..foo").to_string_lossy().to_string());
    mine.push(outside.clone());
    says(&w, &h, writes("toolu_B", "agentB", "fixer", &mine));
    let _ = std::fs::remove_file(&outside);
    let files = item(&desk(&w, &project), "Header jumps")["files"].clone();
    assert_eq!(files, json!(["f1.txt", "f2.txt", "f3.txt", "f4.txt", "f5.txt", "f6.txt", "f7.txt", "..foo"]));
    let sum = summary(&w, &project);
    assert!(sum.contains("| files: f1.txt, f2.txt, f3.txt, f4.txt, f5.txt +3 more"), "{sum}");
    assert!(!sum.contains("old.txt") && !sum.contains(&outside), "{sum}");
}

#[test]
fn a_route_to_new_or_to_the_lead_starts_a_session_that_gets_the_item() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let before = sent_to(&w, &l).len();
    let count = || w.snapshot().0["sessions"].as_array().map_or(0, |s| s.len());
    let sessions = count();
    ends(&w, &l, "ITEM: Typo in the footer\nROUTE: #new -> new | why: nobody's near it");
    w.settle();
    let now = desk(&w, &project);
    let it = item(&now, "Typo in the footer").clone();
    let id = it["id"].as_str().unwrap().to_string();
    let name = format!("{project}-{id}");
    let sid = it["holder"]["sid"].as_str().unwrap().to_string();
    assert_eq!((it["state"].as_str(), it["holder"]["name"].as_str()), (Some("working"), Some(name.as_str())), "{now}");
    assert_ne!(sid, l);
    assert_eq!(count(), sessions + 1);
    assert!(sent_to(&w, &sid).iter().any(|t| t.contains(&format!("#{id}")) && t.contains("nobody's near it")), "{:?}", sent_to(&w, &sid));
    assert_eq!((now["decisions"][0]["text"].as_str(), now["decisions"][0]["sid"].as_str()), (Some(format!("Started {name} for #{id}").as_str()), Some(l.as_str())));
    // The lead isn't sent the item, nor told of a route made (#248).
    assert_eq!(sent_to(&w, &l).len(), before, "the lead isn't sent the item");
    // Said again, or to the lead itself ("me", its own name), it isn't started twice: it's held.
    ends(&w, &l, &format!("ROUTE: #{id} -> new | why: nobody's near it\nROUTE: #{id} -> me | why: small"));
    w.settle();
    assert_eq!(count(), sessions + 1);
    // "me" on an item nobody holds starts one too: the lead never keeps work.
    ends(&w, &l, "ITEM: Footer colour\nROUTE: #new -> me | why: small");
    w.settle();
    let it = item(&desk(&w, &project), "Footer colour").clone();
    assert!(it["holder"]["sid"].as_str().is_some_and(|s| s != l), "{it}");
    assert_eq!(count(), sessions + 2);
}

#[test]
fn start_now_on_an_item_nobody_holds_starts_a_session_for_it() {
    let w = World::new();
    let (_, project) = holder(&w);
    let it = w.hub().desk_add(&project, "", "Header", "", None).unwrap();
    w.hub().desk_item_act(&it.id, "start_now", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["holder"]["name"].as_str()), (Some("working"), Some(format!("{project}-{}", it["id"].as_str().unwrap()).as_str())), "{it}");
}

/// A ROUTE to new whose session can't start: the lead is told why at once (not with your next message), the item
/// says it couldn't start (not stuck in its queue), and Start now tries again; once it starts, that's gone.
#[test]
fn a_new_session_that_cant_start_tells_the_lead_at_once_and_can_be_tried_again() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let before = sent_to(&w, &l).len();
    w.hosted.refuse("the folder is gone");
    ends(&w, &l, "ITEM: Typo in the footer\nROUTE: #new -> new | why: nobody's near it");
    w.settle();
    let it = item(&desk(&w, &project), "Typo in the footer").clone();
    let id = it["id"].as_str().unwrap().to_string();
    assert_eq!(it["state"], "new", "{it}");
    assert!(it["holder"].is_null() && it["route_failed"].is_null(), "{it}");
    assert!(it["start_failed"].as_str().is_some_and(|e| e.contains("the folder is gone")), "{it}");
    let told = sent_to(&w, &l)[before..].to_vec();
    assert!(told.iter().any(|t| t.contains(&format!("Cue couldn't start a new session for #{id}")) && t.contains("the folder is gone")), "{told:?}");
    assert!(summary(&w, &project).contains("couldn't start a new session: "));
    // Tried again (its page's Retry): it starts, and "couldn't start" goes.
    w.hosted.accept();
    w.hub().desk_item_act(&id, "start_now", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &project), "Typo in the footer").clone();
    assert_eq!((it["state"].as_str(), it["holder"]["name"].as_str()), (Some("working"), Some(format!("{project}-{id}").as_str())), "{it}");
    assert!(it["start_failed"].is_null(), "{it}");
}

/// A Desk in a folder you put in a project (another repository, whose own name isn't the project's): a ROUTE to new
/// starts a session in that folder and it's given the item, not refused as "isn't a session of" the project.
#[test]
fn a_route_to_new_from_a_folder_put_in_the_project_gets_the_item() {
    let w = World::new();
    let project = crate::model::project_at(&w.home().to_string_lossy(), "");
    let other = w.home().join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    let other = std::fs::canonicalize(&other).unwrap().to_string_lossy().to_string();
    crate::config::set("project.folders", json!({ other.clone(): project.clone() })).unwrap();
    assert_eq!(crate::model::project_at(&other, ""), project);
    assert_ne!(crate::model::own_project(&other, ""), project);
    let r = crate::ipc::desk_start_named(w.hub(), &other, true, Some("Put")).unwrap();
    let (space, l) = (r["id"].as_str().unwrap().to_string(), r["sid"].as_str().unwrap().to_string());
    w.hosted_says(&l, HostedEvent::Taken { text: "hello".into() });
    ends(&w, &l, "ITEM: Typo in the footer\nROUTE: #new -> new | why: nobody's near it");
    w.settle();
    let it = item(&desk(&w, &space), "Typo in the footer").clone();
    assert_eq!(it["state"], "working", "{it}");
    assert!(it["start_failed"].is_null() && it["route_failed"].is_null(), "{it}");
    let sid = it["holder"]["sid"].as_str().unwrap();
    assert!(sent_to(&w, sid).iter().any(|t| t.contains(&format!("#{}", it["id"].as_str().unwrap()))));
}

/// A working item whose holder was cut off (a crash) before DONE: routed again to the same holder, or Restart
/// (start_now), it's sent again. While the holder is mid-turn, neither sends it twice.
#[test]
fn a_working_item_whose_holder_stopped_is_sent_again_on_a_route_or_restart() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let sent = || sent_to(&w, &h).iter().filter(|t| t.contains(&format!("Say DONE: #{id}"))).count();
    assert_eq!(sent(), 1);
    // Mid-turn on it: the same route said again sends nothing, and Restart is refused.
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    w.settle();
    ends(&w, &l, &format!("ROUTE: #{id} -> cue-teams | why: it knows"));
    assert_eq!(sent(), 1);
    assert!(w.hub().desk_item_act(&id, "start_now", "").is_err());
    // Sent back while it's at work, with a new why: the item's Steps show it with its why, and it's sent that (after
    // the turn it's on). That route said again isn't noted or sent twice.
    for _ in 0..2 {
        ends(&w, &l, &format!("ROUTE: #{id} -> cue-teams | why: the owner says it's still off"));
        w.settle();
    }
    let now = desk(&w, &project);
    let again: Vec<_> = now["decisions"].as_array().unwrap().iter().filter(|d| d["text"] == format!("Sent #{id} to cue-teams again")).collect();
    assert_eq!(again.len(), 1, "{now}");
    assert_eq!(again[0]["why"], "the owner says it's still off", "{now}");
    // Cut off with no DONE: the item waits on you, and a route to the same holder sends it again.
    ends(&w, &h, "On it.");
    w.settle();
    assert_eq!(sent(), 2);
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("Why you: the owner says it's still off")), "{:?}", sent_to(&w, &h));
    assert_eq!(item(&desk(&w, &project), "Header")["stopped"].as_bool(), Some(true));
    ends(&w, &l, &format!("ROUTE: #{id} -> cue-teams | why: it knows"));
    w.settle();
    assert_eq!(sent(), 3);
    let now = desk(&w, &project);
    assert_eq!(now["decisions"][0]["text"], format!("Sent #{id} to cue-teams again"), "{now}");
    // Cut off again: Restart sends it once more.
    ends(&w, &h, "On it.");
    w.hub().desk_item_act(&id, "start_now", "").unwrap();
    w.settle();
    assert_eq!(sent(), 4);
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "working");
}

#[test]
fn a_route_to_me_can_name_the_teammate_doing_it() {
    assert_eq!(
        lines("ROUTE: #a1 -> me | crew: `fixer` | why: two files"),
        vec![Line::Route { id: "a1".into(), to: "me".into(), after: None, why: "two files".into(), kind: None, crew: Some("fixer".into()) }]
    );
}

#[test]
fn a_route_to_nobody_leaves_the_item_new_and_the_lead_is_told() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> nobody-here | why: guess");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "new");
    let s = summary(&w, &project);
    assert!(s.contains("couldn't route") && s.contains("nobody-here"), "{s}");
    // Not cleared until a message telling the lead has gone: then told once.
    assert!(summary(&w, &project).contains("couldn't route"));
    crate::ipc::desk_send_with(w.hub(), &project, "go on").unwrap();
    w.settle();
    assert!(sent_to(&w, &d).last().unwrap().contains("couldn't route"));
    assert!(!summary(&w, &project).contains("couldn't route"));
}

#[test]
fn a_session_of_another_project_isnt_a_route() {
    let w = World::new();
    let (_, project) = holder(&w);
    let other = w.home().join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    let opts = crate::ipc::StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = crate::ipc::start_session_with(w.hub(), "claude", &other.to_string_lossy(), "hi", "far-one", &opts).unwrap();
    let far = r["session_id"].as_str().unwrap().to_string();
    assert_ne!(w.snapshot().session(&far)["project"], project.as_str());
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> far-one | why: guess");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "new");
    assert!(sent_to(&w, &far).iter().all(|t| !t.contains("Desk item")));
}

#[test]
fn a_route_to_another_holder_tells_the_first_its_moved() {
    let w = World::new();
    let (h, project) = holder(&w);
    let s = second(&w, "second");
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    ends(&w, &h, "On it.");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ROUTE: #{id} -> second | why: free now"));
    assert_eq!(item(&desk(&w, &project), "Header")["holder"]["name"], "second");
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("#{id}")) && t.contains("has moved to second")), "{:?}", sent_to(&w, &h));
    assert!(sent_to(&w, &s).iter().any(|t| t.contains(&format!("Say DONE: #{id}"))));
}

#[test]
fn a_route_said_again_leaves_an_item_to_check_with_you() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let said = "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows";
    ends(&w, &d, said);
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("DONE: #{id}"));
    let sends = sent_to(&w, &h).len();
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | why: it knows"));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "to_check");
    assert_eq!(sent_to(&w, &h).len(), sends);
    // Nor queued again after another, nor handed to someone else: it stays with you.
    ends(&w, &d, "ITEM: Footer\nROUTE: #new -> me | why: small");
    let f = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | after: #{f} | why: same file\nROUTE: #{id} -> me | why: mine now"));
    let now = desk(&w, &project);
    assert_eq!((item(&now, "Header")["state"].as_str(), item(&now, "Header")["holder"]["name"].as_str()), (Some("to_check"), Some("cue-teams")), "{now}");
    assert_eq!(sent_to(&w, &h).len(), sends);
    // The lead is told why not, in its next <desk> block.
    let s = summary(&w, &project);
    assert!(s.contains(&format!("Cue didn't reopen #{id}")) && s.contains("only for cue-teams"), "{s}");
}

#[test]
fn a_route_to_its_holder_reopens_an_item_to_check() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("DONE: #{id}"));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "to_check");
    // The same route said again isn't a reopen: the lead is told how to make one, once.
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | why: it knows"));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "to_check");
    let s = summary(&w, &project);
    assert!(s.contains(&format!("Cue didn't reopen #{id}")) && s.contains("give a new why"), "{s}");
    crate::ipc::desk_send_with(w.hub(), &project, "go on").unwrap();
    w.settle();
    assert!(!summary(&w, &project).contains("didn't reopen"));
    // With a new why: back to work, and the holder is sent it again with the why.
    let sends = sent_to(&w, &h).len();
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | why: the footer still overlaps"));
    let now = desk(&w, &project);
    assert_eq!((item(&now, "Header")["state"].as_str(), item(&now, "Header")["holder"]["name"].as_str()), (Some("working"), Some("cue-teams")), "{now}");
    let sent = sent_to(&w, &h);
    assert_eq!(sent.len(), sends + 1, "{sent:?}");
    let last = sent.last().unwrap();
    assert!(last.contains(&format!("Desk item #{id}")) && last.contains("Why you: the footer still overlaps") && last.contains(&format!("Say DONE: #{id}")), "{last}");
    assert!(!summary(&w, &project).contains("didn't reopen"));
    // And its DONE puts it to check again.
    ends(&w, &h, &format!("DONE: #{id}"));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "to_check");
}

#[test]
fn a_route_to_nobody_leaves_a_held_item_with_its_holder() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ROUTE: #{id} -> nobody-here | why: guess"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "working");
    assert_eq!(item(&now, "Header")["holder"]["name"], "cue-teams");
    let s = summary(&w, &project);
    assert!(s.contains("nobody-here") && s.contains("It stays with cue-teams.") && !s.contains("still new"), "{s}");
}
