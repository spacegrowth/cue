//! The Desk's lead and each session's context (desk/hub/core/desk.rs, desk/hub/lead/run.rs): what the lead sees of each session's
//! size and cache, what Cue will do with it, and the lead's CONTEXT lines.

use super::desk::desk;
use crate::desk::parse::{lines, Line};
use crate::testing::World;
use serde_json::{json, Value};

#[test]
fn a_context_line_names_a_session_or_me_and_what_to_do() {
    let text = "CONTEXT: web-ui -> keep | why: its next item needs the detail\n- **CONTEXT: me → Keep**\nCONTEXT: <session|me> -> keep | why: …\nCONTEXT: web-ui";
    assert_eq!(
        lines(text),
        vec![
            Line::Context { who: "web-ui".into(), what: "keep".into(), why: "its next item needs the detail".into() },
            Line::Context { who: "me".into(), what: "keep".into(), why: String::new() },
        ]
    );
}

/// Project "shop"'s folder, made in the World's home (a fresh session is started in it).
fn shop_dir(w: &World) -> std::path::PathBuf {
    let d = w.home().join("shop");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A hook session `sid` of project "shop" in tmux pane %`sid` (its box empty), its last reply `left_ms` before its
/// hour's cache runs out with `context` tokens, at its prompt.
fn shop_session(w: &World, sid: &str, left_ms: u64, context: u64) {
    let a = w.session(sid).in_folder(&shop_dir(w)).in_tmux(&format!("%{sid}"));
    a.transcript().cached_reply(3_600_000 - left_ms, context, 3600);
    a.stop("Still to do: the rest of it.");
    w.terminal.pane(sid, |p| p.screen = Some("────────\n❯ \n────────".into()));
}

/// "shop"'s lead (lead01) says `text` at its turn's end.
fn shop_lead_says(w: &World, text: &str) {
    w.session("lead01").in_folder(&shop_dir(w)).in_tmux("%lead01").stop(text);
}

/// The project "shop"'s sessions go under, with its lead (lead01), an item held by webui1, and idle01 on the Desk too
/// (holding nothing), when it was started.
fn shop_desk(w: &World) -> String {
    let project = w.snapshot().session("lead01")["project"].as_str().unwrap().to_string();
    w.hub().desk_put_lead(&project, "lead01", "lead");
    w.hub().desk_add(&project, "bug", "Header", "", Some("webui1")).unwrap();
    if w.snapshot().find("idle01").is_some() {
        w.hub().desk_set_member(&project, "idle01", true).unwrap();
    }
    project
}

fn decisions(w: &World, project: &str) -> Vec<Value> {
    desk(w, project)["decisions"].as_array().cloned().unwrap_or_default()
}

#[test]
fn the_lead_sees_context_for_pending_handoffs_and_can_say_keep() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 12 * 60_000, 142_000);
    shop_session(&w, "idle01", 30 * 60_000, 160_000);
    let project = shop_desk(&w);
    let lead_summary = || w.hub().desk_summary(&project, "lead01", false).0;
    w.tick(true);
    let s = lead_summary();
    assert!(s.contains("sid:webui1, idle, 142k, Cue will hand off when its cache runs out)"), "{s}");
    assert!(s.contains("sid:idle01, idle)") && !s.contains("160k"), "{s}");
    assert!(!s.contains("90k"), "{s}");

    // The lead says keep: Cue won't, and the lead's call can be undone.
    shop_lead_says(&w, "Leave it.\nCONTEXT: webui1 -> keep | why: its next item needs the detail");
    let d = decisions(&w, &project);
    assert_eq!((d[0]["by"].as_str(), d[0]["text"].as_str(), d[0]["why"].as_str(), d[0]["undo"].as_str()), (Some("shop"), Some("Kept shop's context as it is"), Some("its next item needs the detail"), Some("can")), "{d:?}");
    assert!(lead_summary().contains("sid:webui1, idle)") && !lead_summary().contains("142k"), "{}", lead_summary());
    w.hub().desk_undo(d[0]["id"].as_str().unwrap()).unwrap();
    w.settle();
    assert!(lead_summary().contains("142k, Cue will hand off when its cache runs out)"), "undone: Cue's rule again");

    // Kept again while it waits: for this wait only; its next turn ends it.
    shop_lead_says(&w, "CONTEXT: webui1 -> keep");
    assert!(lead_summary().contains("sid:webui1, idle)") && !lead_summary().contains("142k"), "{}", lead_summary());
    w.advance(50 * 60_000); // (its reply is dated 48 minutes back: after the keep)
    next_turn(&w, "webui1", "m2", 12 * 60_000, 142_000);
    w.tick(true);
    assert!(lead_summary().contains("142k, Cue will hand off when its cache runs out)"), "{}", lead_summary());
}

/// Session `sid` works a turn and ends it with reply `id`, `left_ms` before its hour's cache runs out.
fn next_turn(w: &World, sid: &str, id: &str, left_ms: u64, context: u64) {
    let a = w.session(sid).in_folder(&shop_dir(w)).in_tmux(&format!("%{sid}"));
    a.prompt("go on");
    a.transcript().cached_reply_as(id, 3_600_000 - left_ms, context, 3600);
    a.stop("Still to do: the rest of it.");
}

#[test]
fn a_keep_said_while_a_session_works_covers_its_wait_after_that_turn() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 12 * 60_000, 142_000);
    let project = shop_desk(&w);
    let lead_summary = || w.hub().desk_summary(&project, "lead01", false).0;
    w.session("webui1").in_folder(&shop_dir(&w)).in_tmux("%webui1").prompt("go on");
    w.advance(1_000);
    shop_lead_says(&w, "CONTEXT: webui1 -> keep | why: next item builds on it");
    assert_eq!(decisions(&w, &project)[0]["undo"], "can");
    // Its turn ends after the keep: the wait that follows is the one kept.
    w.advance(50 * 60_000); // (its reply is dated 48 minutes back: after the keep)
    next_turn(&w, "webui1", "m2", 12 * 60_000, 142_000);
    w.tick(true);
    assert!(lead_summary().contains("sid:webui1, idle)") && !lead_summary().contains("142k"), "{}", lead_summary());
    // The turn after that ends it.
    w.advance(50 * 60_000); // (its reply is dated 48 minutes back: after the keep)
    next_turn(&w, "webui1", "m3", 12 * 60_000, 142_000);
    w.tick(true);
    assert!(lead_summary().contains("sid:webui1, idle, 142k, Cue will hand off when its cache runs out)"), "{}", lead_summary());
}

#[test]
fn cue_asks_a_session_holding_an_item_for_its_note_and_its_desk_says_so() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 60_000, 142_000);
    shop_session(&w, "idle01", 60_000, 160_000);
    let project = shop_desk(&w);
    w.tick(true);
    let asked = |sid: &str| w.terminal.typed(sid).iter().any(|t| t.contains("Hand off: write a short note"));
    // The ask is typed off the tick's thread.
    let t0 = std::time::Instant::now();
    while !asked("webui1") && t0.elapsed().as_secs() < 5 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    w.settle();
    assert!(asked("webui1"), "{:?}", w.terminal.typed("webui1"));
    assert!(!asked("idle01"), "it holds nothing");
    assert_eq!(crate::db::handoff("webui1").map(|k| k.ahead), Some(crate::ipc::AHEAD_DESK));
    let by_cue = |w: &World| decisions(w, &project).into_iter().filter(|d| d["by"] == "Cue").collect::<Vec<_>>();
    let cue = by_cue(&w);
    assert_eq!(cue.len(), 1, "{cue:?}");
    assert_eq!((cue[0]["text"].as_str(), cue[0]["why"].as_str(), cue[0]["undo"].as_str()), (Some("Asked shop for a hand-off note"), Some("142k, cache ending"), Some("none")));
    w.tick(true);
    assert_eq!(by_cue(&w).len(), 1, "once per idle stretch");
    // Asked: its lead isn't told Cue will ask (again).
    let s = w.hub().desk_summary(&project, "lead01", false).0;
    assert!(!s.contains("→ Cue will"), "{s}");
}

#[test]
fn once_cue_has_tried_this_expiry_the_lead_is_not_told_it_will() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 60_000, 142_000);
    let project = shop_desk(&w);
    // A draft of yours in Cue's box for it: the ask is held, and not tried again before its cache runs out.
    crate::db::set_draft("s:webui1", "half a thought", &json!([]));
    w.tick(true);
    w.settle();
    assert!(w.terminal.typed("webui1").is_empty());
    let s = w.hub().desk_summary(&project, "lead01", false).0;
    assert!(s.contains("sid:webui1, idle)") && !s.contains("142k"), "{s}");
}

#[test]
fn once_a_hand_off_was_sent_this_wait_the_lead_is_not_told_cue_will() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 12 * 60_000, 142_000);
    let project = shop_desk(&w);
    w.tick(true);
    assert!(w.hub().desk_summary(&project, "lead01", false).0.contains("142k, Cue will hand off when its cache runs out"));
    crate::db::handoff_sent("webui1", "", false, 0);
    w.tick(true);
    let s = w.hub().desk_summary(&project, "lead01", false).0;
    assert!(s.contains("sid:webui1, idle)") && !s.contains("142k"), "{s}");
}

#[test]
fn without_a_lead_a_session_holding_an_item_is_left_as_before() {
    let w = World::new();
    shop_session(&w, "webui1", 60_000, 142_000);
    let project = w.snapshot().session("webui1")["project"].as_str().unwrap().to_string();
    w.hub().desk_add(&project, "bug", "Header", "", Some("webui1")).unwrap();
    w.tick(true);
    w.settle();
    assert!(w.terminal.typed("webui1").is_empty() && crate::db::handoff("webui1").is_none());
}

/// Waits (up to 5s) for Cue's ask for a note typed into `sid`'s pane, off the tick's thread.
fn asked_for_a_note(w: &World, sid: &str) -> Option<String> {
    let t0 = std::time::Instant::now();
    loop {
        let typed = w.terminal.typed(sid).into_iter().find(|t| t.contains("Hand off: write a short note"));
        if typed.is_some() || t0.elapsed().as_secs() >= 5 {
            w.settle();
            return typed;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn a_desk_session_over_the_mark_is_asked_for_its_note_and_goes_on_in_a_fresh_session_holding_its_item() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 40 * 60_000, 185_000);
    let project = shop_desk(&w);
    w.tick(true);
    let ask = asked_for_a_note(&w, "webui1").expect("asked: its context is over the mark, its cache far from ending");
    assert_eq!(crate::db::handoff("webui1").map(|k| k.ahead), Some(crate::ipc::AHEAD_FULL));
    let cue: Vec<Value> = decisions(&w, &project).into_iter().filter(|d| d["by"] == "Cue").collect();
    assert_eq!((cue[0]["text"].as_str(), cue[0]["why"].as_str(), cue[0]["item_id"].is_string()), (Some("Asked shop for a hand-off note"), Some("185k, over the 180k mark"), true), "{cue:?}");
    w.tick(true);
    assert_eq!(w.terminal.typed("webui1").len(), 1, "asked once");

    // Its ask's turn: it writes the note, and its turn ends.
    let a = w.session("webui1").in_folder(&shop_dir(&w)).in_tmux("%webui1");
    a.prompt(&ask);
    let t = a.transcript();
    t.prompt(&ask).tool_use("w1", "Write", json!({"file_path": "/r/HANDOFF-webui1.md", "content": "note"})).tool_result("w1");
    a.stop("Note in HANDOFF-webui1.md.");
    w.advance(2_000);
    let k = crate::db::handoff("webui1").unwrap();
    assert!(!k.next.is_empty(), "it went on in a new session: {:?} {:?} {:?}", k.note, crate::db::insight_events_of("webui1"), decisions(&w, &project));
    let first = w.hosted.sent().into_iter().filter(|(s, _)| *s == k.next).map(|(_, t)| t).next().unwrap_or_default();
    assert!(first.contains("HANDOFF-webui1.md"), "started from its note: {first}");
    let item = &desk(&w, &project)["items"][0];
    assert_eq!(item["holder"]["sid"], k.next.as_str(), "the new session holds the item: {item}");
    let handed: Vec<Value> = decisions(&w, &project).into_iter().filter(|d| d["text"].as_str().is_some_and(|t| t.starts_with("Handed shop off to"))).collect();
    assert_eq!(handed.len(), 1, "{:?}", decisions(&w, &project));
    assert_eq!((handed[0]["call"].as_bool(), handed[0]["undo"].as_str()), (Some(true), Some("none")));
}

#[test]
fn a_lead_over_the_mark_hands_its_desk_to_the_session_it_goes_on_in() {
    let w = World::new();
    shop_session(&w, "lead01", 40 * 60_000, 190_000);
    shop_session(&w, "webui1", 40 * 60_000, 90_000);
    let project = shop_desk(&w);
    w.tick(true);
    let ask = asked_for_a_note(&w, "lead01").expect("the lead is asked too");
    assert!(asked_for_a_note(&w, "webui1").is_none(), "under the mark");
    let a = w.session("lead01").in_folder(&shop_dir(&w)).in_tmux("%lead01");
    a.prompt(&ask);
    a.transcript().prompt(&ask);
    a.stop("I couldn't write it.");
    w.advance(2_000);
    let next = crate::db::handoff("lead01").map(|k| k.next).unwrap_or_default();
    assert!(!next.is_empty(), "no note: it starts fresh from its last reply, linked so it isn't asked again");
    assert_eq!(desk(&w, &project)["lead"]["sid"], next.as_str(), "{}", desk(&w, &project)["lead"]);
}


/// A session whose cache ran out over 60k is marked so the lead gives it no new work; a cold small one and a warm big
/// one (webui1, its hand-off pending) are not (docs/desk-routing-merged.md, rule 17).
#[test]
fn a_big_cold_session_is_marked_for_no_new_work() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 30 * 60_000, 142_000);
    let d = shop_dir(&w);
    for (sid, context) in [("idle01", 95_000), ("small1", 40_000)] {
        let a = w.session(sid).in_folder(&d).in_tmux(&format!("%{sid}"));
        a.transcript().cached_reply(2 * 3_600_000, context, 3600);
        a.stop("Done.");
        w.terminal.pane(sid, |p| p.screen = Some("────────\n❯ \n────────".into()));
    }
    let project = shop_desk(&w);
    w.hub().desk_set_member(&project, "small1", true).unwrap();
    w.tick(true);
    let s = w.hub().desk_summary(&project, "lead01", false).0;
    // A session's route: its line, or its part of the one Older line (#363).
    let line = |sid: &str| s.lines().flat_map(|l| l.split("; ")).find(|l| l.contains(&format!("sid:{sid}"))).unwrap_or_default().to_string();
    // Holding nothing, it's only counted (#818): the lead can't route new work to it.
    assert!(line("idle01").is_empty() && s.contains("Also 1 cold sessions too big for new work, holding nothing"), "{s}");
    assert!(!line("small1").contains("cold") && !line("webui1").contains("cold"), "a small cold one and a warm one aren't marked: {s}");
    // Holding an item, it's listed and marked: its own send-backs can still go to it.
    w.hub().desk_add(&project, "bug", "Footer", "", Some("idle01")).unwrap();
    let s = w.hub().desk_summary(&project, "lead01", false).0;
    let line = |sid: &str| s.lines().find(|l| l.contains(&format!("sid:{sid}"))).unwrap_or_default().to_string();
    assert!(line("idle01").ends_with("sid:idle01, idle, 95k, cold: no new work)"), "{s}");
    assert!(crate::desk::LEAD_SKILL.contains("A session marked cold takes no new work"), "the lead's rules say what the mark means");
}
