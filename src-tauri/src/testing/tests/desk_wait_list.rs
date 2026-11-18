//! The Desk's one list of waits (#250, hub/desk_waits.rs): a queued item and a commit waiting on another session's
//! file are in it, each saying who it waits on (a name only when Cue knows whose edit it is, else "someone else"); a
//! circle in it is found, shown and told to the lead once; and each wait has what clears it in place of a plain
//! "Try again": cut a link, ask the session to commit, land without the file.

use super::desk::{desk, sent_to};
use super::desk_commit::{commits, done_while_tidier_edits, ends, item, log, tick};
use super::team::{git, says, writes};
use crate::hosted::HostedEvent;
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::World;
use serde_json::{json, Value};

fn waits(w: &World, project: &str) -> Vec<Value> {
    desk(w, project)["waits"].as_array().cloned().unwrap_or_default()
}

fn id_of(w: &World, project: &str, title: &str) -> String {
    item(w, project, title)["id"].as_str().unwrap().to_string()
}

#[test]
fn a_commit_wait_is_in_the_list_naming_the_session_and_ask_them_to_commit_reaches_it() {
    let w = World::new();
    let (dir, project, o) = done_while_tidier_edits(&w);
    let id = id_of(&w, &project, "Header jumps");
    let list = waits(&w, &project);
    assert_eq!(list.len(), 1, "{list:?}");
    let b = dir.join("b.txt").to_string_lossy().to_string();
    let got = |k: &str| list[0][k].as_str().unwrap_or("").to_string();
    assert_eq!((got("item"), got("kind"), got("by"), got("sid"), got("name"), got("how"), got("path")), (id.clone(), "commit".into(), "tidier".into(), o.clone(), "b.txt".into(), "editing".into(), b.clone()));
    assert!(list[0]["since"].as_u64().unwrap() > 0);

    w.hub().desk_item_act(&id, "ask_commit", &b).unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["finish"]["asked"], json!([o]));
    assert_eq!(commits(&dir), "1", "nothing committed by asking");
    // It's mid-turn, so Cue keeps the message for it and it goes as that turn ends.
    ends(&w, &o, "Still on the footer.");
    assert!(sent_to(&w, &o).iter().any(|t| t.contains("commit your own change to b.txt") && t.contains(&format!("#{id}"))), "{:?}", sent_to(&w, &o));
}

#[test]
fn land_without_that_file_commits_the_rest_and_leaves_it_uncommitted_in_the_folder() {
    let w = World::new();
    let (dir, project, _) = done_while_tidier_edits(&w);
    let id = id_of(&w, &project, "Header jumps");
    assert!(w.hub().desk_item_act(&id, "land_without", "/not/waited/on.txt").is_err(), "only a file it waits on");
    w.hub().desk_item_act(&id, "land_without", &dir.join("b.txt").to_string_lossy()).unwrap();
    w.settle();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "Header jumps\n\na.txt");
    assert!(git(&dir, &["status", "--porcelain", "--", "b.txt"]).contains("b.txt"), "b.txt left as it is, uncommitted");
    assert!(waits(&w, &project).is_empty());
}

#[test]
fn a_file_only_a_shell_command_wrote_is_someone_else_never_a_guessed_name() {
    let w = World::new();
    let (dir, project, _) = super::desk_commit::done_with(&w, |_, _, _| {}, |w, h, dir| {
        let f = |n: &str| dir.join(n).to_string_lossy().to_string();
        says(w, h, writes("toolu_A", "agentA", "fixer", &[f("a.txt"), f("b.txt")]));
        let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
        let r = start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "tidy b", "tidier", &opts).unwrap();
        let o = r["session_id"].as_str().unwrap().to_string();
        w.hosted_says(&o, HostedEvent::Taken { text: "tidy b".into() });
        w.advance(1000);
        log(w, &o).tool_use("toolu_9", "Bash", json!({ "command": format!("echo hi > {}", dir.join("b.txt").display()) }));
        w.settle();
    });
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["finish"]["step"].as_str(), shown["finish"]["wait"].as_str()), (Some("commit_wait"), Some("someone else is editing b.txt")), "{shown}");
    let list = waits(&w, &project);
    assert_eq!((list[0]["by"].as_str(), list[0]["sid"].as_str()), (Some(""), Some("")), "{list:?}");
    let id = id_of(&w, &project, "Header jumps");
    assert!(w.hub().desk_item_act(&id, "ask_commit", &dir.join("b.txt").to_string_lossy()).is_err(), "no one to ask");
    assert_eq!(commits(&dir), "1");
}

/// The item waiting on tidier's b.txt, and a second item tidier is given after it: a circle. (repo, project, the
/// first item, the second, tidier, the lead.)
fn circle(w: &World) -> (std::path::PathBuf, String, String, String, String, String) {
    let (dir, project, o) = done_while_tidier_edits(w);
    let a = id_of(w, &project, "Header jumps");
    let lead = desk(w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &o, true).unwrap();
    ends(w, &lead, &format!("ITEM: task | Tidy the footer | done when: it's tidy\nROUTE: #new -> tidier | after: #{a} | why: it knows the files"));
    let b = id_of(w, &project, "Tidy the footer");
    let it = item(w, &project, "Tidy the footer");
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "{it}");
    tick(w);
    (dir, project, a, b, o, lead)
}

#[test]
fn a_circle_is_found_shown_and_told_to_the_lead_once() {
    let w = World::new();
    let (_, project, a, b, _, lead) = circle(&w);
    let d = desk(&w, &project);
    let circles = d["circles"].as_array().unwrap();
    assert_eq!(circles.len(), 1, "{d}");
    let mut items: Vec<String> = circles[0]["items"].as_array().unwrap().iter().map(|i| i.as_str().unwrap().to_string()).collect();
    items.sort();
    let mut want = vec![a.clone(), b.clone()];
    want.sort();
    assert_eq!(items, want);
    assert_eq!(circles[0]["told"], true);
    assert!(circles[0]["at"].as_u64().unwrap() > 0);
    let links = circles[0]["links"].as_array().unwrap();
    assert!(links.iter().any(|l| l["item"] == a.as_str() && l["on"] == b.as_str() && l["why"] == "needs b.txt committed"), "{links:?}");
    assert!(links.iter().any(|l| l["item"] == b.as_str() && l["on"] == a.as_str() && l["why"] == "queued after it"), "{links:?}");
    let told = |w: &World| sent_to(w, &lead).into_iter().filter(|t| t.starts_with("Circle on the Desk")).collect::<Vec<_>>();
    let notes = told(&w);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains(&format!("#{a}")) && notes[0].contains(&format!("#{b}")) && notes[0].contains("CUT: #"), "{}", notes[0]);
    tick(&w);
    tick(&w);
    assert_eq!(told(&w).len(), 1, "told once");
}

#[test]
fn the_lead_cuts_a_queue_link_and_the_queued_item_starts() {
    let w = World::new();
    let (_, project, a, b, o, lead) = circle(&w);
    says(&w, &lead, [HostedEvent::TurnStarted]);
    ends(&w, &lead, &format!("CUT: #{b} -> #{a} | why: tidier can start it now"));
    let it = item(&w, &project, "Tidy the footer");
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("working"), None), "{it}");
    // Its holder is mid-turn: Cue keeps the hand-off for it and it goes as that turn ends.
    ends(&w, &o, "Still on b.");
    assert!(sent_to(&w, &o).iter().any(|t| t.contains(&format!("#{b}"))), "its holder handed it");
    assert!(desk(&w, &project)["decisions"].as_array().unwrap().iter().any(|d| d["text"] == format!("Cut its wait on #{a}").as_str() && d["why"] == "tidier can start it now"));
    tick(&w);
    assert!(desk(&w, &project)["circles"].as_array().unwrap().is_empty(), "the circle is gone");
    // A link that isn't there: the lead is told so.
    says(&w, &lead, [HostedEvent::TurnStarted]);
    ends(&w, &lead, &format!("CUT: #{b} -> #{a} | why: again"));
    assert!(sent_to(&w, &lead).iter().any(|t| t.starts_with("Not cut:")), "{:?}", sent_to(&w, &lead));
}

#[test]
fn cutting_a_commit_link_from_the_window_lands_it_without_that_file_and_starts_the_queued_one() {
    let w = World::new();
    let (dir, project, a, b, _, _) = circle(&w);
    w.hub().desk_item_act(&a, "cut", &b).unwrap();
    w.settle();
    assert_eq!(item(&w, &project, "Header jumps")["state"], "to_check");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=", "HEAD"]).trim(), "a.txt");
    assert_eq!(item(&w, &project, "Tidy the footer")["state"], "working", "released as the first one's commit was made");
    tick(&w);
    assert!(desk(&w, &project)["circles"].as_array().unwrap().is_empty());
}

#[test]
fn try_again_is_gone_and_a_plain_rerun_is_its_own_act() {
    let w = World::new();
    let (_, project, _) = done_while_tidier_edits(&w);
    let id = id_of(&w, &project, "Header jumps");
    assert!(w.hub().desk_item_act(&id, "retry_finish", "").is_err(), "no Try again act");
    assert!(w.hub().desk_item_act(&id, "rerun", "").is_err(), "not stopped on a question: nothing to rerun");
}

#[test]
fn a_cut_line_is_read() {
    use crate::desk::parse::{lines, Line};
    assert_eq!(lines("CUT: #231 -> #228 | why: the commit can go without it"), vec![Line::Cut { id: "231".into(), on: "228".into(), why: "the commit can go without it".into() }]);
    assert_eq!(lines("- **CUT: #a1 → #b2**"), vec![Line::Cut { id: "a1".into(), on: "b2".into(), why: String::new() }]);
    assert_eq!(lines("CUT: 231 -> 228"), vec![]);
}
