//! A Desk's items (hub/desk.rs, desk_said.rs, desk_act.rs): adding them (by you or an ITEM: line from any session),
//! kept across a relaunch, lines said again, retitle, split and merge, DESIGNS: links, pictures, undo and what you do to an item.

use super::desk::{desk, ends, holder, item, new_lead, second, sent_to};
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;

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
    assert_eq!(sent_to(&w, &l).len(), before, "told a moment later");
    w.advance(1000);
    let told = sent_to(&w, &l).pop().unwrap();
    assert!(told.contains(&format!("New item #{} (bug): Logo blurs", it["id"].as_str().unwrap())) && told.contains("from cue-teams") && told.ends_with("Route it now: ROUTE it to a session, or -> new."), "{told}");
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
    w.advance(300);
    let b = w.hub().desk_add(&project, "task", "Footer", "", None).unwrap();
    w.settle();
    assert_eq!(sent_to(&w, &l).len(), before);
    w.advance(1000);
    let told = sent_to(&w, &l);
    assert_eq!(told.len(), before + 1, "{told:?}");
    let last = told.last().unwrap();
    assert!(last.starts_with("<desk") && last.contains(&format!("New item #{} (bug): Header", a.id)) && last.contains(&format!("New item #{} (task): Footer", b.id)), "{last}");
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
    assert!(told.contains(&format!("New item #{}: Logo blurs", it.id)) && told.contains("no kind yet") && told.ends_with("| kind: bug, feature or task."), "{told}");
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
