//! An item whose result is a decision, plan or document, not code (#543): it never takes or waits for a copy of the
//! repository, and its DONE with no commit goes to you to check, sent back or not. #537's case: an item for mocks,
//! sent back while every copy was in use, waited for one it didn't need, then came back "nothing landed".

use super::desk::desk;
use super::desk_commit::{ends, item};
use super::team::lead;
use crate::hosted::HostedEvent;
use crate::testing::World;

fn top(w: &World) -> String {
    let out = String::from_utf8(std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
    out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
}

/// A repository with every copy in use, its Desk lead, and `title` routed to its holder: (project, item id, holder).
fn routed(w: &World, title: &str, done_when: &str) -> (String, String, String) {
    let _ = super::team::main_repo(w);
    let h = lead(w, "hello");
    ends(w, &h, "Hi.");
    w.hub().desk_hold_copies(&top(w), &[1, 2, 3]);
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(w, &d, "I run the Desk now.");
    w.advance(1000);
    ends(w, &d, &format!("ITEM: task | {title} | done when: {done_when}\nROUTE: #new -> cue-teams | why: it knows the list"));
    w.advance(1000);
    let id = item(w, &project, title)["id"].as_str().unwrap().to_string();
    (project, id, h)
}

#[test]
fn an_item_for_mocks_sent_back_while_every_copy_is_in_use_never_waits_for_one_and_goes_to_check() {
    let w = World::new();
    let (project, id, h) = routed(&w, "Mocks: away mode", "three ways on the canvas. No code changes");
    let it = item(&w, &project, "Mocks: away mode");
    assert_eq!(it["state"], "working", "at work with no copy, not queued for one: {it}");
    assert!(it.get("tree_wait").is_none_or(|t| t.is_null() || t == false), "{it}");
    ends(&w, &h, &format!("Three on the canvas.\nDESIGNS: #{id} https://example.com/canvas\nDONE: #{id}"));
    let it = item(&w, &project, "Mocks: away mode");
    assert_eq!((it["state"].as_str(), it["finish"]["mocks"].as_bool()), (Some("to_check"), Some(true)), "{it}");
    // Sent back with every copy still in use: at work again at once, your words with it.
    let before = w.hosted.sent().len();
    w.hub().desk_item_act(&id, "send_back", "Make B denser").unwrap();
    w.settle();
    let it = item(&w, &project, "Mocks: away mode");
    assert_eq!(it["state"], "working", "{it}");
    assert!(it.get("tree_wait").is_none_or(|t| t.is_null() || t == false), "{it}");
    assert!(w.hosted.sent()[before..].iter().any(|(s, t)| *s == h && t.contains("Make B denser")), "its holder has your words");
    // Done again, still no commit: to you to check, not sent back for "nothing landed".
    w.advance(1000);
    let before = w.hosted.sent().len();
    ends(&w, &h, &format!("B is denser on the canvas.\nDONE: #{id}"));
    w.settle();
    let it = item(&w, &project, "Mocks: away mode");
    assert_eq!((it["state"].as_str(), it["finish"]["mocks"].as_bool()), (Some("to_check"), Some(true)), "{it}");
    assert!(!w.hosted.sent()[before..].iter().any(|(_, t)| t.contains("landed")), "{:?}", &w.hosted.sent()[before..]);
}

#[test]
fn an_item_ending_in_a_plan_with_no_commit_goes_to_check() {
    let w = World::new();
    let (project, id, h) = routed(&w, "Plan: one chat per item", "a plan in docs/, no commit");
    assert_eq!(item(&w, &project, "Plan: one chat per item")["state"], "working");
    ends(&w, &h, &format!("The plan is in docs/one-chat.md.\nDONE: #{id}"));
    let it = item(&w, &project, "Plan: one chat per item");
    assert_eq!((it["state"].as_str(), it["finish"]["mocks"].as_bool()), (Some("to_check"), Some(true)), "{it}");
    let d = desk(&w, &project);
    assert!(d["items"].as_array().unwrap().iter().all(|i| i.get("tree_wait").is_none_or(|t| t.is_null() || t == false)), "{d}");
}

#[test]
fn a_doc_only_item_takes_no_copy_even_when_one_is_free() {
    // #547: an item for a doc and mocks took a copy as it started, and held it till the lead took it back, blocking
    // the build waiting for one.
    let w = World::new();
    let _ = super::team::main_repo(&w);
    let h = lead(&w, "hello");
    ends(&w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(&w, &d, "I run the Desk now.");
    w.advance(1000);
    ends(&w, &d, "ITEM: task | Designers' advice on Loops | done when: a local doc and three rows on the canvas; no code changes, no build items filed\nROUTE: #new -> cue-teams | why: it knows Loops");
    let it = item(&w, &project, "Designers' advice on Loops");
    assert_eq!(it["state"], "working", "{it}");
    assert!(it["tree"].is_null(), "no copy taken: {it}");
    // A code item routed next (to a new session: cue-teams is busy with the doc, #576) gets the free copy at once.
    w.advance(1000);
    ends(&w, &d, "ITEM: feature | Loops B build | done when: Loops looks like option B\nROUTE: #new -> new | why: it knows Loops");
    let b = item(&w, &project, "Loops B build");
    assert_eq!((b["state"].as_str(), b["tree"]["slot"].as_u64()), (Some("working"), Some(1)), "{b}");
}

#[test]
fn start_on_a_doc_only_item_queued_after_another_starts_it_with_every_copy_in_use() {
    // #548: a doc-only item queued after another, started by the lead (START) with every copy in use, was queued again
    // for a copy: START didn't take.
    let w = World::new();
    let (project, a, _) = routed(&w, "Designers' advice on Loops", "a local doc and canvas rows; no code changes");
    let d = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    w.advance(1000);
    ends(&w, &d, &format!("ITEM: task | How you start a loop | done when: a local doc and mock rows; no code changes\nROUTE: #new -> new | after: #{a} | why: follows #{a}'s direction"));
    let b = item(&w, &project, "How you start a loop");
    assert_eq!((b["state"].as_str(), b["after"].as_str()), (Some("queued"), Some(a.as_str())), "{b}");
    let id = b["id"].as_str().unwrap().to_string();
    w.advance(1000);
    ends(&w, &d, &format!("START: #{id} | why: #{a}'s direction is ready"));
    let b = item(&w, &project, "How you start a loop");
    assert!(matches!(b["state"].as_str(), Some("new" | "working")), "started, not queued again: {b}");
    assert!(b.get("tree_wait").is_none_or(|t| t.is_null() || t == false) && b["tree"].is_null(), "{b}");
    // Its new session takes it: at work, still no copy and no wait for one.
    w.settle();
    let b = item(&w, &project, "How you start a loop");
    if let Some(sid) = b["holder"]["sid"].as_str() {
        w.hosted_says(sid, HostedEvent::Taken { text: "go".into() });
        w.settle();
    }
    let b = item(&w, &project, "How you start a loop");
    assert_ne!(b["state"], "queued", "{b}");
    assert!(b.get("tree_wait").is_none_or(|t| t.is_null() || t == false) && b["tree"].is_null(), "{b}");
}

#[test]
fn start_on_a_code_item_queued_after_another_starts_it_with_every_copy_in_use() {
    // The same for an item with code (#548): START means now, where its holder is, not queued again. Its code is to
    // land, so it stays in line for the next free copy (#566, #581), unlike a doc-only item.
    let w = World::new();
    let (project, a, _) = routed(&w, "Designers' advice on Loops", "a local doc; no code changes");
    let d = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    w.advance(1000);
    ends(&w, &d, &format!("ITEM: bug | Footer moves | done when: it stays put\nROUTE: #new -> cue-teams | after: #{a} | why: same area"));
    let id = item(&w, &project, "Footer moves")["id"].as_str().unwrap().to_string();
    assert_eq!(item(&w, &project, "Footer moves")["state"], "queued");
    w.advance(1000);
    ends(&w, &d, &format!("START: #{id} | why: no need to wait"));
    w.settle();
    let b = item(&w, &project, "Footer moves");
    assert_eq!((b["state"].as_str(), b["tree"].is_null()), (Some("working"), true), "at work now, where its holder is: {b}");
    assert_eq!(b["tree_wait"], true, "in line for the next free copy: {b}");
    let held = w.hub().read(|st| st.desk.item(&id).cloned()).unwrap();
    assert!(crate::desk::hub::landing::trees::in_line(&held), "{b}");
}

#[test]
fn an_item_done_off_the_repo_takes_no_copy_and_goes_to_check_with_no_commit() {
    // #568: setting up another machine over ssh; its done when says nothing in the repo changes. It waited for a copy
    // with every one in use, though none of its work happens in the repo.
    let w = World::new();
    let title = "Set up another machine for Cue work";
    let (project, id, h) = routed(&w, title, "on the other machine, cargo build finishes and a short report says which tests pass; nothing in the repo changes");
    let it = item(&w, &project, title);
    assert_eq!(it["state"], "working", "at work at once, not queued for a copy: {it}");
    assert!(it["tree"].is_null() && it.get("tree_wait").is_none_or(|t| t.is_null() || t == false), "{it}");
    ends(&w, &h, &format!("It builds there; the report is in the chat.\nDONE: #{id}"));
    let it = item(&w, &project, title);
    assert_eq!(it["state"], "to_check", "to you to check, with no commit: {it}");
}

#[test]
fn done_whens_that_keep_the_repo_unchanged_are_read_and_others_are_not() {
    use crate::desk::hub::landing::unlanded::rules_out_code;
    for w in ["three rows on the canvas; no code changes", "a plan, no commit", "nothing in the repo changes", "No changes to the repository.", "the repo stays unchanged", "it doesn't change the repo"] {
        assert!(rules_out_code(w), "{w}");
    }
    for w in ["the header stays put when you scroll", "fix the code so nothing in the list changes order", "no code path panics on an empty list", "commit the change to the repo"] {
        assert!(!rules_out_code(w), "{w}");
    }
}
