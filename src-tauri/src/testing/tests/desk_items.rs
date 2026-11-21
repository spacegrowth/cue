//! A Desk's items (desk/hub/core/desk.rs, desk_said.rs, desk_act.rs): adding them (by you or an ITEM: line from any session),
//! kept across a relaunch, lines said again, retitle, split and merge, DESIGNS: links, pictures, undo and what you do to an item.

use super::desk::{desk, ends, holder, item, new_lead, second, sent_to};
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::{json, Value};

#[test]
fn an_item_you_add_is_kept_across_a_relaunch() {
    let mut w = World::new();
    let it = w.hub().desk_add("web", "bug", "Header jumps", "it stays put", None).unwrap();
    assert_eq!(it.state, "new");
    w.relaunch();
    let d = desk(&w, "web");
    assert_eq!(item(&d, "Header jumps")["id"], it.id.as_str());
    assert_eq!(item(&d, "Header jumps")["done_when"], "it stays put");
    assert!(d["lead"].is_null());
}

#[test]
fn an_item_line_from_any_session_is_the_projects_and_its_lead_is_told() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let before = sent_to(&w, &l).len();
    ends(&w, &h, "Found one.\nITEM: bug | Logo blurs | done when: it's sharp");
    let it = item(&desk(&w, &project), "Logo blurs").clone();
    assert_eq!((it["state"].as_str(), it["from"].as_str()), (Some("new"), Some("cue-teams")));
    assert!(it["holder"].is_null());
    assert_eq!(sent_to(&w, &l).len(), before + 1, "told at once, its lead being free");
    let told = sent_to(&w, &l).pop().unwrap();
    assert!(told.contains(&format!("New item #{} (bug): Logo blurs", it["id"].as_str().unwrap())) && told.contains("from cue-teams") && told.contains("Route it now: ROUTE it to a session, or -> new.") && told.ends_with("(Desk changes 1)"), "{told}");
    // Said again: no second item.
    ends(&w, &h, "ITEM: bug | Logo blurs");
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().len(), 1);
}

#[test]
fn items_you_add_together_reach_the_lead_in_one_message() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let before = sent_to(&w, &l).len();
    let a = w.hub().desk_add(&project, "bug", "Header", "", None).unwrap();
    let b = w.hub().desk_add(&project, "task", "Footer", "", None).unwrap();
    w.settle();
    let told = sent_to(&w, &l);
    assert_eq!(told.len(), before + 1, "{told:?}");
    let last = told.last().unwrap();
    assert!(!last.contains("<desk") && last.ends_with("(Desk changes 1–2)") && last.contains(&format!("New item #{} (bug): Header", a.id)) && last.contains(&format!("New item #{} (task): Footer", b.id)), "{last}");
    // No lead: an item stays new, and nobody is told.
    let none = w.hub().desk_add("web", "task", "Menu", "", None).unwrap();
    w.advance(2000);
    assert_eq!(item(&desk(&w, "web"), "Menu")["state"], "new");
    assert!(!w.hosted.sent().iter().any(|(_, t)| t.contains(&format!("#{}", none.id))));
}

#[test]
fn an_item_you_add_has_no_kind_till_the_lead_routes_it_with_one() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "", "Logo blurs", "", None).unwrap();
    assert_eq!(it.kind, "");
    w.advance(1000);
    let told = sent_to(&w, &l).pop().unwrap();
    assert!(told.contains(&format!("New item #{}: Logo blurs", it.id)) && told.contains("no kind yet") && told.contains("| kind: bug, feature or task.\n\n(Desk changes 1)"), "{told}");
    ends(&w, &l, &format!("ROUTE: #{} -> cue-teams | kind: Bug | why: it knows the header", it.id));
    assert_eq!(item(&desk(&w, &project), "Logo blurs")["kind"], "bug");
    assert!(sent_to(&w, &h).last().unwrap().contains(&format!("Desk item #{} (bug): Logo blurs", it.id)));
    // A kind that isn't one leaves it as it was.
    ends(&w, &l, &format!("ROUTE: #{} -> cue-teams | kind: chore", it.id));
    assert_eq!(item(&desk(&w, &project), "Logo blurs")["kind"], "bug");
}

#[test]
fn pictures_you_attach_are_kept_and_named_to_the_lead_and_to_whoever_takes_it() {
    use base64::Engine;
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let png = crate::model::Upload { name: "shot.png".into(), mime: "image/png".into(), data: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b"not really a png")) };
    let it = w.hub().desk_add_with(&project, "", "Logo blurs", "", "", None, &[png]).unwrap();
    assert_eq!(it.images.len(), 1);
    let path = &it.images[0];
    assert!(path.contains("/uploads/kept/") && std::fs::read(path).unwrap() == b"not really a png", "{path}");
    w.advance(1000);
    assert!(sent_to(&w, &l).pop().unwrap().contains(&format!("New item #{}: Logo blurs | pictures: `{path}`", it.id)));
    ends(&w, &l, &format!("ROUTE: #{} -> cue-teams | kind: bug | why: it knows the header", it.id));
    assert!(sent_to(&w, &h).last().unwrap().contains(&format!("\nPictures with it (open each to see it): `{path}`")));
    // Not an image: nothing is added.
    let txt = crate::model::Upload { name: "notes.txt".into(), mime: "text/plain".into(), data: "aGk=".into() };
    assert!(w.hub().desk_add_with(&project, "", "Notes", "", "", None, &[txt]).is_err());
    assert!(!desk(&w, &project)["items"].as_array().unwrap().iter().any(|i| i["title"] == "Notes"));
}

#[test]
fn an_item_added_with_a_holder_is_its_work_already() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let (to_h, to_l) = (sent_to(&w, &h).len(), sent_to(&w, &l).len());
    let it = w.hub().desk_add(&project, "task", "Sticky header", "", Some(&h)).unwrap();
    w.advance(2000);
    assert_eq!(it.state, "working");
    assert_eq!(it.holder.as_ref().map(|x| x.sid.as_str()), Some(h.as_str()));
    assert_eq!((sent_to(&w, &h).len(), sent_to(&w, &l).len()), (to_h, to_l), "nothing sent");
    assert!(w.hub().desk_add("elsewhere", "task", "No", "", Some(&h)).is_err(), "another project's session");
}

#[test]
fn an_item_you_give_goes_to_that_session_and_is_your_decision() {
    let w = World::new();
    let (h, project) = holder(&w);
    let it = w.hub().desk_add(&project, "bug", "Header", "it stays put", None).unwrap();
    let given = w.hub().desk_give(&it.id, &h).unwrap();
    w.settle();
    assert_eq!(given.holder.as_ref().map(|x| x.name.as_str()), Some("cue-teams"));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "working");
    let to_h = sent_to(&w, &h);
    assert!(to_h.iter().any(|t| t.contains(&format!("Desk item #{}", it.id)) && t.contains("it stays put") && t.contains(&format!("Say DONE: #{}", it.id))), "{to_h:?}");
    let dec = desk(&w, &project)["decisions"][0].clone();
    assert_eq!((dec["by"].as_str(), dec["undo"].as_str()), (Some("You"), Some("none")), "{dec}");
    assert_eq!(dec["text"], format!("Gave #{} to cue-teams", it.id));
    // Given on to another: the first is told it moved.
    let s = second(&w, "second");
    w.hub().desk_give(&it.id, &s).unwrap();
    w.settle();
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("has moved to second")));
    assert!(w.hub().desk_give("nope", &s).is_err());
}

#[test]
fn a_line_said_again_makes_no_second_item_or_send() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let said = "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows";
    ends(&w, &d, said);
    let sends = sent_to(&w, &h).len();
    // The holder took it and is at work on it (a holder that stopped short of DONE is sent it again: see
    // a_working_item_whose_holder_stopped_is_sent_again_on_a_route_or_restart).
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    w.settle();
    ends(&w, &d, said);
    let now = desk(&w, &project);
    assert_eq!(now["items"].as_array().unwrap().len(), 1, "{now}");
    assert_eq!(sent_to(&w, &h).len(), sends);
    assert_eq!(now["decisions"].as_array().unwrap().len(), 1, "{now}");
}

#[test]
fn a_same_title_of_another_kind_or_done_is_a_new_item() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header");
    ends(&w, &d, "ITEM: feature | Header");
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().len(), 2);
    ends(&w, &d, "ITEM: bug | Header");
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().len(), 2);
}

#[test]
fn housekeeping_calls_are_marked_and_going_back_carries_your_answer() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    ends(&w, &h, "DECIDED: kept the sticky header — it was asked for\nDECIDED (minor): put the notes in docs/dev — they're dev notes");
    let decs = desk(&w, &project)["decisions"].as_array().unwrap().clone();
    let key = decs.iter().find(|c| c["text"] == "kept the sticky header").unwrap();
    let minor = decs.iter().find(|c| c["text"] == "put the notes in docs/dev").unwrap();
    assert!(key.get("minor").is_none(), "a key call carries no mark");
    assert_eq!(minor["minor"], true);
    assert_eq!(minor["call"], true, "housekeeping is still a call, kept behind Show all");
    w.hub().desk_undo_with(key["id"].as_str().unwrap(), "drop the sticky header").unwrap();
    w.settle();
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with("Go back on your call") && t.contains("kept the sticky header") && t.contains("The user's answer instead: drop the sticky header.")));
}

#[test]
fn only_product_calls_make_the_decisions_list_routing_and_sends_stay_off_it() {
    let mut w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let id = desk(&w, &project)["items"][0]["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("DECIDED: #{id} lands before the footer — they touch the same file\nDECIDED: Sent the landing clash back to its session — it's theirs\nDECIDED: Hold all work until the 1:30 reset — out of tokens\nDECIDED: Away mode is one switch for all Desks — simpler"));
    ends(&w, &h, "DECIDED: Your new order also changes which item the lead routes first — that's what it's for\nDECIDED: Sent it back to the session holding #9 — the clash was its own");
    let shown = |w: &World| -> Vec<String> {
        let decs: Vec<_> = desk(w, &project)["decisions"].as_array().unwrap().iter().filter(|c| c["call"] == true).cloned().collect();
        assert_eq!(decs.len(), 6, "all still kept as calls (behind Show all): {decs:?}");
        decs.iter().filter(|c| c.get("minor").is_none()).map(|c| c["text"].as_str().unwrap().to_string()).collect()
    };
    let want = vec!["Your new order also changes which item the lead routes first".to_string(), "Away mode is one switch for all Desks".to_string()];
    assert_eq!(shown(&w), want);
    // Calls kept from before are sorted the same way at the next start.
    for c in desk(&w, &project)["decisions"].as_array().unwrap().iter().filter(|c| c["call"] == true) {
        let mut c: crate::desk::DeskDecision = serde_json::from_value(c.clone()).unwrap();
        c.minor = false;
        crate::db::desk_put_decision(&c);
    }
    w.relaunch();
    assert_eq!(shown(&w), want);
}

#[test]
fn a_call_is_dispatch_work_by_its_words_and_who_made_it() {
    use crate::desk::model::dispatch_call;
    for t in ["#366 lands before #373", "Took back the repository copy held by #356", "No changes to routing", "Resume work now instead of at 1:30", "Filed the clash-and-Send-back fix after the Away follow-ups", "Away mode mocks go to cue-402", "Gave cue-180 three open items at once", "I didn't send #125 back"] {
        assert!(dispatch_call(t, true), "{t}");
    }
    for t in ["Away mode is one switch for all Desks, not one per Desk", "default Away budget is $10, changeable on the rules card", "while Away, the lead never builds an unpicked mock"] {
        assert!(!dispatch_call(t, true), "{t}");
    }
    for t in ["Start sets a held item back to new and tells the lead", "Resume sends the session a short note"] {
        assert!(!dispatch_call(t, false), "{t}");
    }
    assert!(dispatch_call("Sent #361's landing clash back to its session", false));
}

/// #420: today's real kinds of item sessions' calls. How a session worked (its copy, tests, files, commits, reviews,
/// mocks' canvas) stays off the Decisions list; how Cue looks or behaves, what's left out and trade-offs stay on it.
#[test]
fn a_sessions_notes_on_how_it_worked_stay_off_the_decisions_list() {
    use crate::desk::model::dispatch_call;
    for t in [
        "I worked in a new working copy, desk-411 (branch desk/411, now on current main)",
        "no change to Cue itself",
        "`desk_finish.rs` (994 lines) and `desk_theirs.rs` (928) are left as they are",
        "The new tests file is named `desk_routing.rs`",
        "I made main's `desk.js` stop needing the other session's code rather than committing its uncommitted files",
        "One teammate does both the fixes and the new button, rather than starting a second one",
        "I let clean-check skip setting git's commit dates in the tests (`GIT_COMMITTER_DATE`), as the review had asked",
        "kept the misleading commit message as it is, because 367d1191 is already on origin/main",
        "skipped the new review request for routing-review",
        "For finding 3, clean-check only names the gap in a `SHORTCUT:` line",
        "Turned off the separate debug files for Cue's own crate instead of pruning them after each build",
        "the tests use a local stand-in remote and a fake PR tool, never a real one",
        "suggested H on the canvas",
        "read \"D and calls 1\" as your pick of D and Calls 1",
        "#414 gets its own canvas page rather than more rows on \"Away without a column\"",
        "Changed the #145 test so a new reply no longer pulls you down while you're scrolled up",
        "Mac only. The other card belongs in the private app's own repo",
        "I rebased #251 onto the newest main three times and left the shared folder's uncommitted edits in place",
        "I copied the main checkout's build cache into the shared copy cache, which was empty",
        "install built from a clean copy of main with the add-on",
        "Left the notch panel's Allow/No order alone",
    ] {
        assert!(dispatch_call(t, false), "{t}");
    }
    for t in [
        "If any Desk is away, the whole window goes dark",
        "The calls line starts open, and Cue remembers per Desk if you fold it",
        "refuse `git push` inside steps",
        "cap a whole check or after-landing run at 30 minutes",
        "I guarded Cut, Copy and Select All as well as Paste",
        "\"Tests\" counts as reached when the item's session runs a command that looks like a test run (cargo test, playwright, jest and similar)",
        "PR landing needs the item in its own copy, so saving PR together with \"shared folder\" is refused",
        "Only branches Cue made for copies (`desk/…`, `worktree-agent-…`) are deleted with them",
        "on an item ready to review, the \"You'll see\" line is left off because the row's \"Try it\" line already says the same thing",
        "A folded question with no recommended answer shows only Choose…, so answering it takes two clicks",
        "Your new order also changes which item the lead routes first, not just which item gets the next free folder",
        "the recent row shows every Desk you opened recently (up to 7) instead of capping at 3",
    ] {
        assert!(!dispatch_call(t, false), "{t}");
    }
}

/// #420: a session that says its DECIDED lines again in later replies (one Desk had one item's 4 calls 13 times) gets
/// each on the list once; repeats kept from before are sorted off at the next start. The lead's dispatch calls are
/// Dispatcher Actions.
#[test]
fn a_call_said_again_is_listed_once_and_the_leads_dispatch_calls_are_its_actions() {
    let mut w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let calls = "DECIDED: cap a whole check at 30 minutes — a stuck check shouldn't hold the Desk\nDECIDED: refuse `git push` inside steps — pushes ask you";
    for _ in 0..3 {
        ends(&w, &h, calls);
    }
    ends(&w, &h, "DECIDED: Cap a whole check at 30 minutes. — said again");
    ends(&w, &d, "DECIDED: Away mode mocks go to cue-402 — it made the last ones\nDECIDED: going back is drawn as a curved arrow — easier to follow");
    let all = |w: &World| -> Vec<Value> { desk(w, &project)["decisions"].as_array().unwrap().iter().filter(|c| c["call"] == true).cloned().collect() };
    let shown = |w: &World| -> Vec<String> { all(w).iter().filter(|c| c.get("minor").is_none()).map(|c| c["text"].as_str().unwrap().to_string()).rev().collect() };
    let want = vec!["cap a whole check at 30 minutes".to_string(), "refuse `git push` inside steps".to_string(), "going back is drawn as a curved arrow".to_string()];
    assert_eq!(shown(&w), want);
    assert_eq!(all(&w).len(), 4, "a repeat isn't recorded again");
    let moved = all(&w).into_iter().find(|c| c["text"] == "Away mode mocks go to cue-402").unwrap();
    assert_eq!((moved["minor"].clone(), moved["act"].clone()), (json!(true), json!("call")), "in Dispatcher Actions");
    // Kept from before: repeats saved as their own entries, and the lead's routing hidden but no action.
    let mut c: crate::desk::DeskDecision = serde_json::from_value(all(&w)[0].clone()).unwrap();
    for n in 0..3 {
        c.id = format!("{}-again{n}", c.id);
        c.at += n + 1;
        crate::db::desk_put_decision(&c);
    }
    let mut m: crate::desk::DeskDecision = serde_json::from_value(moved).unwrap();
    m.act.clear();
    crate::db::desk_put_decision(&m);
    w.relaunch();
    assert_eq!(shown(&w), want);
    assert_eq!(all(&w).len(), 7);
    assert_eq!(all(&w).into_iter().find(|c| c["text"] == "Away mode mocks go to cue-402").unwrap()["act"], "call");
    let moves = crate::db::desk_moves_page(&project, None, 50);
    assert!(moves.iter().any(|x| x.text == "Away mode mocks go to cue-402"), "{moves:?}");
}

#[test]
fn undo_asks_whoever_decided_to_go_back() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    ends(&w, &h, "DECIDED: kept the sticky header — it was asked for");
    let dec = desk(&w, &project)["decisions"][0].clone();
    assert_eq!(dec["undo"], "can");
    w.hub().desk_undo(dec["id"].as_str().unwrap()).unwrap();
    w.settle();
    assert_eq!(desk(&w, &project)["decisions"][0]["undo"], "asked");
    assert!(sent_to(&w, &h).iter().any(|t| t.starts_with("Go back on your call") && t.contains("kept the sticky header")));
}

#[test]
fn what_you_do_to_an_item() {
    let w = World::new();
    let it = w.hub().desk_add("web", "task", "Footer", "", None).unwrap();
    assert!(w.hub().desk_item_act(&it.id, "looks_right", "").is_ok());
    assert_eq!(item(&desk(&w, "web"), "Footer")["state"], "done");
    // Undo on a done item: back to check, done_at gone; a second Undo has nothing to undo.
    let back = w.hub().desk_item_act(&it.id, "reopen", "").unwrap();
    assert_eq!((back.state.as_str(), back.done_at), ("to_check", None));
    assert!(w.hub().desk_item_act(&it.id, "reopen", "").is_err());
    assert_eq!(w.hub().desk_item_act(&it.id, "drop", "").unwrap().state, "dropped");
    assert!(desk(&w, "web").is_null() || desk(&w, "web")["items"].as_array().unwrap().is_empty());
    assert!(w.hub().desk_item_act("nope", "drop", "").is_err());
}

#[test]
fn an_item_line_said_again_while_its_item_is_to_check_makes_no_second_item() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: feature | Lead only dispatches\nROUTE: #new -> cue-teams | why: it built it");
    let id = item(&desk(&w, &project), "Lead only dispatches")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Done.\nDONE: #{id}"));
    assert_eq!(item(&desk(&w, &project), "Lead only dispatches")["state"], "to_check");
    ends(&w, &l, "ITEM: feature | Lead only dispatches");
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().len(), 1);
}

/// DESIGNS: #id <link> from any session of the project sets the item's canvas link; a newer one replaces it (the
/// designs moved), kept across a relaunch; one from a code block, or for another project's item, doesn't.
#[test]
fn a_designs_line_sets_the_items_canvas_link_and_a_newer_one_replaces_it() {
    let mut w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: feature | New board | done when: it shows\nROUTE: #new -> cue-teams | why: it made the board");
    let id = item(&desk(&w, &project), "New board")["id"].as_str().unwrap().to_string();
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("say DESIGNS: #{id} <canvas link>"))));
    assert!(item(&desk(&w, &project), "New board").get("designs").is_none());
    ends(&w, &h, &format!("Mocks are up.\n- **DESIGNS:** #{id} <https://claude.ai/artifact/AAA>."));
    assert_eq!(item(&desk(&w, &project), "New board")["designs"], "https://claude.ai/artifact/AAA");
    ends(&w, &h, &format!("```\nDESIGNS: #{id} https://example.com/old\n```"));
    assert_eq!(item(&desk(&w, &project), "New board")["designs"], "https://claude.ai/artifact/AAA");
    // Moved: the lead (or any session of the project) says the new place.
    ends(&w, &l, &format!("DESIGNS: #{id} [canvas](https://claude.ai/artifact/BBB)"));
    assert_eq!(item(&desk(&w, &project), "New board")["designs"], "https://claude.ai/artifact/BBB");
    w.relaunch();
    assert_eq!(item(&desk(&w, &project), "New board")["designs"], "https://claude.ai/artifact/BBB");
    assert_eq!(lines(&format!("DESIGNS: #{id} no link here")), vec![]);
    assert_eq!(lines("DESIGNS: #a1 see http://x.io/c, then"), vec![Line::Designs { id: "a1".into(), url: "http://x.io/c".into() }]);
}

#[test]
fn the_lead_can_retitle_an_item_and_your_words_stay_on_it() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let said = "when i scroll down the header kind of jumps around and its annoying";
    let it = w.hub().desk_add(&project, "", said, "", None).unwrap();
    w.advance(1000);
    assert!(sent_to(&w, &l).pop().unwrap().contains("TITLE it"), "the lead is asked to title or split it");
    ends(&w, &l, &format!("TITLE: #{} | Header jumps on scroll\nROUTE: #{} -> cue-teams | kind: bug | why: it knows the header", it.id, it.id));
    let d = desk(&w, &project);
    let got = item(&d, "Header jumps on scroll");
    assert_eq!(got["words"], said);
    assert!(sent_to(&w, &h).last().unwrap().contains(&format!("Desk item #{} (bug): Header jumps on scroll", it.id)));
    // Retitled again: your words stay the first ones.
    ends(&w, &l, &format!("TITLE: #{} | Header shakes", it.id));
    assert_eq!(item(&desk(&w, &project), "Header shakes")["words"], said);
}

#[test]
fn the_lead_can_split_an_item_with_two_problems_into_two() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let said = "the header jumps when i scroll, also the footer text is cut off on small windows";
    let it = w.hub().desk_add(&project, "", said, "both fixed", None).unwrap();
    w.advance(1000);
    let before = sent_to(&w, &l).len();
    ends(&w, &l, &format!("SPLIT: #{} | Header jumps on scroll | Footer cut off on small windows", it.id));
    let d = desk(&w, &project);
    let items = d["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{d}");
    let a = item(&d, "Header jumps on scroll");
    let b = item(&d, "Footer cut off on small windows");
    assert_eq!(a["id"], it.id.as_str());
    assert_eq!((a["words"].as_str(), b["words"].as_str()), (Some(said), Some(said)));
    assert_eq!((b["split_from"].as_str(), b["from"].as_str(), b["state"].as_str()), (Some(it.id.as_str()), Some("you"), Some("new")));
    // The lead is told both, with their numbers, to route them.
    w.advance(1000);
    let told = sent_to(&w, &l);
    assert_eq!(told.len(), before + 1, "{told:?}");
    let last = told.last().unwrap();
    assert!(last.contains(&format!("#{}: Header jumps on scroll", it.id)) && last.contains(&format!("#{}: Footer cut off on small windows | split from #{}", b["id"].as_str().unwrap(), it.id)), "{last}");
    assert!(!last.contains("SPLIT it first"), "not asked to split them again: {last}");
}

#[test]
fn an_item_already_given_out_isnt_split() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "task", "Header and footer", "", Some(&h)).unwrap();
    ends(&w, &l, &format!("SPLIT: #{} | Header | Footer", it.id));
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().len(), 1);
    assert!(sent_to(&w, &l).last().unwrap().contains(&format!("#{} not split", it.id)));
}

/// MERGE: #copy into #first (#321): the copy leaves the list, its words and pictures go to the first (whose holder,
/// at work on it, hears them), what waited on the copy waits on the first, and no session is started for the copy.
#[test]
fn the_lead_can_fold_a_repeated_ask_into_the_item_it_repeats() {
    use base64::Engine;
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let count = || w.snapshot().0["sessions"].as_array().map_or(0, |s| s.len());
    ends(&w, &l, "ITEM: bug | Header jumps on scroll | done when: it stays\nROUTE: #new -> cue-teams | why: it knows the header");
    let first = item(&desk(&w, &project), "Header jumps on scroll")["id"].as_str().unwrap().to_string();
    let png = crate::model::Upload { name: "shot.png".into(), mime: "image/png".into(), data: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b"png")) };
    let copy = w.hub().desk_add_with(&project, "", "the header still jumps when scrolling fast", "", "", None, &[png]).unwrap();
    let later = w.hub().desk_add(&project, "", "Footer cut off", "", None).unwrap();
    ends(&w, &l, &format!("ROUTE: #{} -> cue-teams | after: #{} | why: same area", later.id, copy.id));
    w.settle();
    let sessions = count();
    ends(&w, &l, &format!("MERGE: #{} into #{first} | why: same ask", copy.id));
    w.settle();
    let d = desk(&w, &project);
    assert!(d["items"].as_array().unwrap().iter().all(|i| i["id"] != copy.id.as_str()), "the copy is gone: {d} {:?}", sent_to(&w, &l).last());
    let f = item(&d, "Header jumps on scroll");
    assert!(f["words"].as_str().unwrap().contains(&format!("From #{}: the header still jumps when scrolling fast", copy.id)), "{f}");
    assert_eq!(f["images"].as_array().map(Vec::len), Some(1));
    assert_eq!(item(&d, "Footer cut off")["after"], first.as_str(), "what waited on the copy waits on the first");
    assert_eq!(count(), sessions, "no session started for the copy");
    assert!(sent_to(&w, &h).last().unwrap().contains(&format!("#{} was folded into #{first}", copy.id)));
    assert!(d["decisions"].as_array().unwrap().iter().any(|x| x["text"] == format!("Folded #{} into it", copy.id).as_str()));
    // One begun can't be folded: the lead is told why.
    ends(&w, &l, &format!("MERGE: #{first} into #{}", later.id));
    assert!(sent_to(&w, &l).last().unwrap().contains(&format!("#{first} not merged: ")));
    assert!(desk(&w, &project)["items"].as_array().unwrap().iter().any(|i| i["id"] == first.as_str()));
}

#[test]
fn pictures_you_send_from_an_items_page_reach_its_holder_as_pictures_not_paths() {
    use base64::Engine;
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let it = w.hub().desk_add(&project, "task", "Steps folded", "", None).unwrap();
    w.advance(1000);
    ends(&w, &l, &format!("ROUTE: #{} -> cue-teams | why: it knows the chat", it.id));
    // Two pictures, saved and named after your words as ipc desk_item_act does.
    let png = |name: &str| crate::model::Upload { name: name.into(), mime: "image/png".into(), data: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b"not really a png")) };
    let saved = crate::uploads::save(&[png("a.png"), png("b.png")]).unwrap();
    let paths: Vec<String> = saved.iter().map(|s| s.path.clone()).collect();
    let named: Vec<String> = paths.iter().map(|p| format!("`{p}`")).collect();
    let text = format!("is A this?{}{}", crate::desk::YOUR_PICTURES, named.join(" "));
    w.hub().desk_item_act(&it.id, "note", &text).unwrap();
    w.settle();
    // In its chat: your words, with the two pictures as pictures, and no paths in the words.
    let thread = w.snapshot().session(&h)["thread"].clone();
    let mine = thread.as_array().unwrap().iter().rev().find(|e| e["role"] == "you" && e["text"].as_str().unwrap_or("").contains("is A this?")).cloned().unwrap();
    assert_eq!(mine["images"], json!(paths), "{mine}");
    assert!(!mine["text"].as_str().unwrap().contains('`') && !mine["text"].as_str().unwrap().contains("Pictures"), "{mine}");
    // The session still reads where they are: their paths after the words, as a picture from its own chat goes.
    let went = sent_to(&w, &h).pop().unwrap();
    assert!(paths.iter().all(|p| went.contains(p.as_str())) && !went.contains('`'), "{went}");
    // Its page keeps your words without the paths.
    let mine = desk(&w, &project)["decisions"].as_array().unwrap().iter().find(|c| c["by"] == "You" && c["item_id"] == it.id.as_str()).cloned().unwrap();
    assert_eq!(mine["text"], "is A this?");
}
