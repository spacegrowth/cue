//! A holder's turn that ends without DONE (hub/desk_said.rs, desk_stall.rs): asked once, then the Desk lead; stalls,
//! Restart, and what the item says its holder is doing.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::hosted::HostedEvent;
use crate::testing::{Transcript, World};
use serde_json::json;

/// A holder whose turn ends on its item with no DONE and nothing to ask you: Cue asks it once if it's done, and the
/// item is stopped, not waiting on you (#182). The turn that answers with still no DONE goes to the Desk lead to settle
/// (told once); one that ends on a question waits on you with that question (Answer). A later stall is asked once more;
/// DONE puts it to check.
#[test]
fn an_item_whose_holder_stops_without_done_is_asked_once_then_goes_to_the_desk_lead() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let words = format!("Done? Write DONE: #{id}, or say what's left");
    let asked = || sent_to(&w, &h).iter().filter(|t| **t == words).count();
    let turn = |prompt: &str, reply: &str| {
        w.advance(1000);
        w.hosted_says(&h, HostedEvent::Taken { text: prompt.into() });
        w.settle();
        assert_eq!(item(&desk(&w, &project), "Header")["state"], "working", "at work on it");
        w.advance(1000);
        ends(&w, &h, reply);
    };
    let told = || sent_to(&w, &l).iter().filter(|t| t.starts_with(&format!("#{id} (Header) stopped:"))).count();
    turn("Header", "Looked at the header.\nThe sticky part is in place.");
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool()), (Some("working"), Some(true)), "stopped, not waiting on you: {it}");
    assert_eq!(it["latest"], "The sticky part is in place.");
    assert!(it["ask"].is_null(), "no ask box for a plain stop: {it}");
    assert_eq!(asked(), 1, "asked once: {:?}", sent_to(&w, &h));
    assert_eq!(told(), 0, "Cue's own ask goes first");
    // Its answer: still no DONE, so the Desk lead settles it (told once, with its last line); it isn't asked again.
    turn(&words, "The footer still overlaps it.");
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool(), it["latest"].as_str()), (Some("working"), Some(true), Some("The footer still overlaps it.")), "{it}");
    assert_eq!(asked(), 1);
    assert_eq!(told(), 1, "{:?}", sent_to(&w, &l));
    let lead_words = sent_to(&w, &l).into_iter().find(|t| t.starts_with(&format!("#{id} (Header) stopped:"))).unwrap();
    assert!(lead_words.contains("The footer still overlaps it.") && lead_words.contains(&format!("ASK: #{id} |")), "{lead_words}");
    // A turn that ends on a question for you isn't asked: it waits on you with that question, in one line.
    turn("Go on", "I moved the header.\n\nThe footer is close to it. Should the footer move too?");
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["latest"].as_str()), (Some("needs_you"), Some("Should the footer move too?")), "{it}");
    assert_eq!((asked(), told()), (1, 1));
    // A new stall later: asked once more.
    turn("Yes, move it", "Moved some of it.");
    assert_eq!(asked(), 2);
    turn(&words, &format!("All done.\nDONE: #{id}"));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "to_check");
    assert_eq!(asked(), 2);
}

/// A holder that said what's left, or who it waits on: its plain turns after that ("ok" → "Understood.") aren't asked
/// "Done?" again while the item is unchanged (#168); sent the item again, a stall is asked once more.
#[test]
fn a_holder_that_said_whats_left_or_who_it_waits_on_isnt_asked_again() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let asked = || sent_to(&w, &h).iter().filter(|t| t.starts_with("Done? Write DONE")).count();
    let turn = |prompt: &str, reply: &str| {
        w.advance(1000);
        w.hosted_says(&h, HostedEvent::Taken { text: prompt.into() });
        w.settle();
        w.advance(1000);
        ends(&w, &h, reply);
    };
    turn("Header", "It's in another session's files.\nWaiting on cue-161 to take it.");
    assert_eq!(asked(), 0, "said who it waits on");
    // Waiting isn't stopping (#249): shown as waiting on that, and the lead isn't told.
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool(), it["waiting"].as_str()), (Some("working"), None, Some("Waiting on cue-161 to take it.")), "{it}");
    assert_eq!(sent_to(&w, &l).iter().filter(|t| t.contains(") stopped:")).count(), 0);
    turn("ok", "Understood.");
    turn("ok", "Nothing changed here.");
    assert_eq!(asked(), 0, "{:?}", sent_to(&w, &h));
    // A turn that no longer says it waits, with nothing to ask you: the Desk lead settles it, told once.
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["stopped"].as_bool(), it["waiting"].as_str()), (Some("working"), Some(true), None), "{it}");
    assert_eq!(sent_to(&w, &l).iter().filter(|t| t.contains(") stopped:")).count(), 1);
    // Sent again: a new stall, asked once.
    ends(&w, &l, &format!("ROUTE: #{id} -> {h} | why: back to it"));
    turn("Header again", "Looked again.");
    assert_eq!(asked(), 1, "{:?}", sent_to(&w, &h));
}

/// A stalled item sent again (Restart, or a route to the same holder) is at work, not stalled, until that holder's
/// next turn ends.
#[test]
fn a_stalled_item_sent_again_is_at_work_until_its_next_turn_ends() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let state = || item(&desk(&w, &project), "Header")["state"].as_str().unwrap_or("").to_string();
    let stall = || {
        w.advance(1000);
        w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
        w.advance(1000);
        ends(&w, &h, "Part of it.");
        assert_eq!(item(&desk(&w, &project), "Header")["stopped"].as_bool(), Some(true));
    };
    stall();
    w.advance(1000);
    let it = w.hub().desk_item_act(&id, "start_now", "").unwrap();
    w.settle();
    assert_eq!((it.state.as_str(), state()), ("working", "working".to_string()), "restarted");
    stall();
    w.advance(1000);
    ends(&w, &l, &format!("ROUTE: #{id} -> cue-teams | why: it knows"));
    w.settle();
    assert_eq!(state(), "working", "routed to it again");
}

/// A Stop hook sent the holder back to work after its answer (a checklist): the item's reply is the answer, the
/// hook's follow-up kept apart to fold under it as the chat does (#310); a turn no hook sent back has nothing apart.
#[test]
fn a_stop_hooks_follow_up_is_kept_apart_from_the_reply() {
    let w = World::new();
    let (h, project) = holder(&w);
    let a = w.hub().desk_add(&project, "bug", "First thing", "", None).unwrap();
    w.hub().desk_give(&a.id, &h).unwrap();
    w.settle();
    let words = sent_to(&w, &h).into_iter().rev().find(|t| t.contains(&format!("Desk item #{}", a.id))).unwrap();
    w.hosted_says(&h, HostedEvent::Taken { text: words.clone() });
    Transcript::at(w.hub().session_origin(&h).unwrap().transcript_path)
        .prompt(&words)
        .assistant("Fixed the header.")
        .entry(json!({ "type": "system", "subtype": "stop_hook_summary", "hookCount": 1 }))
        .entry(json!({ "type": "user", "isMeta": true, "message": { "role": "user", "content": "Stop hook feedback:\nUnaudited edits this turn." } }))
        .assistant("1. SHAPE: fine.");
    ends(&w, &h, "1. SHAPE: fine.");
    let said = item(&desk(&w, &project), "First thing")["said"][0].clone();
    assert_eq!((said["text"].as_str(), said["after"].as_str()), (Some("Fixed the header."), Some("1. SHAPE: fine.")), "{said}");
    let next = format!("About #{}: the footer too", a.id);
    w.hosted_says(&h, HostedEvent::Taken { text: next.clone() });
    Transcript::at(w.hub().session_origin(&h).unwrap().transcript_path).prompt(&next).assistant("And the footer too.");
    ends(&w, &h, "And the footer too.");
    let said = item(&desk(&w, &project), "First thing")["said"][1].clone();
    assert_eq!((said["text"].as_str(), said.get("after")), (Some("And the footer too."), None), "{said}");
}

/// A stop the Desk lead doesn't settle in 10 minutes waits on you, saying so; with no lead at hand it waits on you at
/// once. The lead's ASK about the item (ASK: #id | …) shows on it as waiting on you, and its answer goes to the lead.
#[test]
fn a_stop_the_lead_cant_settle_waits_on_you_and_its_ask_shows_on_the_item() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let turn = |prompt: &str, reply: &str| {
        w.advance(1000);
        w.hosted_says(&h, HostedEvent::Taken { text: prompt.into() });
        w.advance(1000);
        ends(&w, &h, reply);
    };
    turn("Header", "Part of it.");
    turn(&format!("Done? Write DONE: #{id}, or say what's left"), "More of it.");
    assert_eq!(item(&desk(&w, &project), "Header")["stopped"].as_bool(), Some(true));
    // The lead asks you about it: it waits on you with that ask.
    ends(&w, &l, &format!("ASK: #{id} | Keep the old header for a release? | choices: Yes / No"));
    w.settle();
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!((it["state"].as_str(), it["ask"]["text"].as_str(), it["stopped"].as_bool()), (Some("needs_you"), Some("Keep the old header for a release?"), None), "{it}");
    let card = it["ask"]["item_id"].as_str().unwrap().to_string();
    w.hub().respond(&card, crate::model::Decision { behavior: "allow".into(), answers: Some(serde_json::json!({ "q": "No" })), ..Default::default() });
    w.settle();
    w.advance(1000);
    assert!(sent_to(&w, &l).iter().any(|t| t == &format!("Your ASK about #{id}: Keep the old header for a release?\nThe answer: No")), "{:?}", sent_to(&w, &l));
    assert_eq!(item(&desk(&w, &project), "Header")["stopped"].as_bool(), Some(true), "the lead has it again");
    // Not settled in 10 minutes: it waits on you, saying so.
    w.advance(10 * 60 * 1000);
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!(it["state"], "needs_you", "{it}");
    assert!(it["latest"].as_str().unwrap().contains("couldn't settle it"), "{it}");
}

/// Restart on an item whose holder can't be reached (its host refuses to start it) says why on the item and in its
/// Steps; once the holder can start, Restart sends it and says it went (#291).
#[test]
fn restart_says_whether_it_reached_the_holder() {
    let mut w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    w.settle();
    // Cue restarted mid-turn: the holder comes back paused, and its host can't start it.
    w.relaunch();
    // Its conversation last written over a minute ago.
    w.advance(61_000);
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "needs_resume");
    w.hosted.refuse("Too many open files (os error 24)");
    let sent = || sent_to(&w, &h).iter().filter(|t| t.contains(&format!("Say DONE: #{id}"))).count();
    let before = sent();
    w.hub().desk_item_act(&id, "start_now", "").unwrap();
    w.settle();
    let now = desk(&w, &project);
    let it = item(&now, "Header");
    assert_eq!(sent(), before, "{now}");
    assert!(it["restart"].as_str().is_some_and(|r| r.starts_with("Restart couldn't reach cue-teams") && r.contains("Too many open files")), "{now}");
    let step = now["decisions"].as_array().unwrap().iter().find(|d| d["text"] == "Restart" && d["item_id"] == id.as_str()).cloned().unwrap_or_default();
    assert!(step["why"].as_str().is_some_and(|w| w.contains("couldn't reach it") && w.contains("Too many open files")), "{now}");
    // It can start again: Restart sends it, and says so.
    w.hosted.accept();
    w.hub().desk_item_act(&id, "start_now", "").unwrap();
    w.settle();
    assert_eq!(sent(), before + 1);
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    w.settle();
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["restart"], "Restart went to cue-teams", "{now}");
    assert_eq!(item(&now, "Header")["state"], "working", "{now}");
}

/// A holder whose conversation was written in the last minute is at work, even when a restart showed it stopped: its
/// item doesn't need resuming, and the session shows as working (#291).
#[test]
fn a_holder_that_wrote_just_now_is_never_shown_stopped() {
    let mut w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    w.settle();
    // cue-host keeps it running through Cue's restart, but can't say it's mid-turn.
    w.hosted.under_host(&h);
    w.relaunch();
    // Its conversation last written over a minute ago.
    w.advance(61_000);
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "needs_resume");
    assert_eq!(w.snapshot().session(&h)["state"], "stopped");
    // Its host kept it running: its conversation is written to now.
    let path = w.snapshot().session(&h)["transcript_path"].as_str().unwrap().to_string();
    let f = std::fs::OpenOptions::new().append(true).create(true).open(&path).unwrap();
    f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(crate::clock::now_ms())).unwrap();
    w.tick(false);
    assert_eq!(w.snapshot().session(&h)["state"], "working");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "working");
}

/// A working item's holder: what it does now and when (`act`), stalling 10 quiet minutes later; an open ask is
/// waiting on you, never stalled (#285).
#[test]
fn a_working_item_says_what_its_holder_does_and_when_it_stalls() {
    let w = World::new();
    let a = w.session("pi-1").pi().in_project("web");
    a.started();
    w.hub().desk_set_member("web", "pi-1", true).unwrap();
    let id = w.hub().desk_add("web", "task", "Footer", "", Some("pi-1")).unwrap().id;
    a.prompt(&format!("Work on #{id}"));
    a.doing("Reading model.rs");
    w.settle();
    let it = item(&desk(&w, "web"), "Footer").clone();
    assert_eq!(it["state"], "working", "{it}");
    let act = &it["act"];
    assert_eq!((act["now"].as_str(), act["was"].as_str()), (Some("Reading model.rs"), Some("last read model.rs")), "{it}");
    let at = act["at_ms"].as_u64().unwrap();
    assert_eq!(act["stalls_at"].as_u64().unwrap() - at, crate::desk::model::QUIET_MS, "one 10-minute threshold");
    // A newer step moves it on.
    w.advance(60_000);
    a.doing("Editing desk.rs");
    w.settle();
    let act = item(&desk(&w, "web"), "Footer")["act"].clone();
    assert_eq!(act["now"], "Editing desk.rs");
    assert!(act["at_ms"].as_u64().unwrap() >= at + 60_000, "{act}");
    // An open ask: waiting on you, so no act (the window counts it neither moving nor stalled).
    let _ask = a.ask_bash("rm -rf build");
    let it = item(&desk(&w, "web"), "Footer").clone();
    assert_eq!(it["state"], "needs_you", "{it}");
    assert!(it["act"].is_null(), "{it}");
}
