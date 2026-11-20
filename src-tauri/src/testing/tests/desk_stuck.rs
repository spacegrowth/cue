//! A Desk item's holder stuck mid-turn (desk/hub/stall/stuck.rs, #502): silent 20 minutes while working, or sitting on a
//! permission prompt: its lead is told once a spell, and told again only after it moved and got stuck again. A
//! session whose background command still prints isn't stuck.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::hosted::{Ask, AskKind, HostedEvent};
use crate::hub::{PROMPT_MS, SILENT_MS};
use crate::testing::{Transcript, World};
use serde_json::{json, Value};

const MIN: u64 = 60_000;

/// A holder at work on item "Header" its lead routed to it: (holder, lead, item, its log).
fn at_work(w: &World) -> (String, String, String, String) {
    let (h, project) = holder(w);
    let l = new_lead(w, &project);
    ends(w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(w, &project), "Header")["id"].as_str().unwrap().to_string();
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: format!("Desk item #{id}: Header") });
    w.settle();
    assert_eq!(item(&desk(w, &project), "Header")["state"], "working");
    let log = w.hub().session_origin(&h).unwrap().transcript_path;
    Transcript::at(&log).prompt("Header").assistant("Looking at the header.");
    (h, l, id, log)
}

/// The lead's notices that item `id` is stuck.
fn told(w: &World, l: &str, id: &str) -> Vec<String> {
    sent_to(w, l).into_iter().filter(|t| t.starts_with(&format!("#{id} (Header) "))).collect()
}

fn pass(w: &World, ms: u64) {
    w.advance(ms);
    w.tick(false);
}

#[test]
fn a_holder_silent_for_twenty_minutes_is_told_to_the_lead_once_a_spell() {
    let w = World::new();
    let (_h, l, id, log) = at_work(&w);
    pass(&w, SILENT_MS - MIN);
    assert!(told(&w, &l, &id).is_empty(), "not 20 minutes yet: {:?}", told(&w, &l, &id));
    pass(&w, 2 * MIN);
    let t = told(&w, &l, &id);
    assert_eq!(t.len(), 1, "{t:?}");
    assert!(t[0].contains("looks stuck") && t[0].contains("no new output for 21 minutes") && t[0].contains(&format!("NOTE: #{id}")), "{}", t[0]);
    // Still silent: not said again every minute.
    for _ in 0..5 {
        pass(&w, MIN);
    }
    assert_eq!(told(&w, &l, &id).len(), 1);
    // It moves again (new output): cleared; quiet another 20 minutes, a new spell is told.
    Transcript::at(&log).assistant("Found it.");
    pass(&w, 1000);
    pass(&w, SILENT_MS - 2 * MIN);
    assert_eq!(told(&w, &l, &id).len(), 1, "moved: its quiet starts over");
    pass(&w, 3 * MIN);
    assert_eq!(told(&w, &l, &id).len(), 2, "{:?}", told(&w, &l, &id));
}

#[test]
fn a_holder_sitting_on_a_permission_prompt_is_told_with_what_it_asks() {
    let w = World::new();
    let (h, l, id, _) = at_work(&w);
    w.hosted_says(&h, HostedEvent::Ask(Ask {
        request_id: "r1".into(), kind: AskKind::Permission, tool_name: "Bash".into(), input: json!({ "command": "rm -rf build" }),
        suggestions: json!([]), agent_id: String::new(), tool_use_id: "t1".into(), server: String::new(), message: String::new(), form: Value::Null,
    }));
    w.settle();
    assert_eq!(w.snapshot().session(&h)["state"], "deciding");
    pass(&w, PROMPT_MS - MIN);
    assert!(told(&w, &l, &id).is_empty(), "a moment on a prompt isn't stuck");
    pass(&w, 2 * MIN);
    let t = told(&w, &l, &id);
    assert_eq!(t.len(), 1, "{t:?}");
    assert!(t[0].contains("stuck on a permission prompt") && t[0].contains("Bash: rm -rf build") && t[0].contains("6 minutes"), "{}", t[0]);
    pass(&w, 30 * MIN);
    assert_eq!(told(&w, &l, &id).len(), 1, "once while it sits there");
    // Answered: it moves again, and the spell is over.
    let card = w.snapshot().cards_of(&h).iter().find(|c| c["kind"] == "permission").unwrap()["id"].as_str().unwrap().to_string();
    w.answer(&card, json!({ "behavior": "allow" }));
    w.settle();
    pass(&w, 1000);
    assert_eq!(w.snapshot().session(&h)["state"], "working");
    // Cleared: silent long enough after that, it's a new spell, told as such.
    pass(&w, SILENT_MS + MIN);
    let t = told(&w, &l, &id);
    assert_eq!(t.len(), 2, "{t:?}");
    assert!(t[1].contains("looks stuck"), "{}", t[1]);
}

#[test]
fn a_holder_whose_long_command_still_prints_is_not_stuck() {
    let w = World::new();
    let (_h, l, id, log) = at_work(&w);
    let out = std::path::Path::new(&log).with_extension("b1.output");
    let launch = format!("Command running in background with ID: b1. Output is being written to: {}. You will be notified when it completes.", out.display());
    let print = |line: &str| {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&out).unwrap();
        std::io::Write::write_all(&mut f, format!("{line}\n").as_bytes()).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(crate::clock::now_ms())).unwrap();
    };
    print("   Compiling cue");
    Transcript::at(&log)
        .tool_use("call-b1", "Bash", json!({ "command": "cargo test", "description": "Run the tests", "run_in_background": true }))
        .entry(json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "call-b1", "content": launch }] }, "toolUseResult": { "stdout": "", "stderr": "", "interrupted": false, "backgroundTaskId": "b1" } }));
    // 40 minutes with nothing new in its conversation, but its tests print every few minutes.
    for n in 0..10 {
        pass(&w, 4 * MIN);
        print(&format!("test {n} ... ok"));
    }
    assert!(told(&w, &l, &id).is_empty(), "still printing: {:?}", told(&w, &l, &id));
    // Its command goes quiet too: now it's stuck.
    pass(&w, SILENT_MS + MIN);
    assert_eq!(told(&w, &l, &id).len(), 1, "{:?}", told(&w, &l, &id));
}
