//! The Desk (desk/, hub/desk.rs, hub/desk_lead.rs): a project's lead (a session you pick, or a new one Cue starts),
//! the lines it and the project's sessions say at the end of a turn, and what `state.desk` shows.

use super::team::lead;
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::{json, Value};

fn ends(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

/// A holder ("cue-teams", at its prompt) and the project it's in.
fn holder(w: &World) -> (String, String) {
    let sid = lead(w, "hello");
    ends(w, &sid, "Hi.");
    let project = w.snapshot().session(&sid)["project"].as_str().unwrap().to_string();
    assert!(!project.is_empty());
    (sid, project)
}

/// A new lead for `project`, started by Cue (desk_set_lead without a session), at its prompt: its id.
fn new_lead(w: &World, project: &str) -> String {
    let r = crate::ipc::desk_set_lead_with(w.hub(), project, None, true).unwrap();
    let sid = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&sid, HostedEvent::Taken { text: "hello".into() });
    ends(w, &sid, "I run the Desk now.");
    sid
}

/// Another hosted session of the project, named `name`, at its prompt.
fn second(w: &World, name: &str) -> String {
    let opts = crate::ipc::StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
    let r = crate::ipc::start_session_with(w.hub(), "claude", &w.home().to_string_lossy(), "hi", name, &opts).unwrap();
    let sid = r["session_id"].as_str().unwrap().to_string();
    w.hosted_says(&sid, HostedEvent::Taken { text: "hi".into() });
    ends(w, &sid, "Hi.");
    sid
}

fn desk(w: &World, project: &str) -> Value {
    let snap = w.snapshot();
    snap.0["desk"]["projects"].as_array().unwrap().iter().find(|p| p["project"] == project).cloned().unwrap_or(Value::Null)
}

fn item<'a>(d: &'a Value, title: &str) -> &'a Value {
    d["items"].as_array().unwrap().iter().find(|i| i["title"] == title).unwrap_or_else(|| panic!("no item {title}: {d}"))
}

fn sent_to(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

fn summary(w: &World, project: &str) -> String {
    w.hub().desk_summary(project, "", false).0
}

#[test]
fn lines_are_read_outside_code_and_quotes() {
    let text = "Sure.\n```\nITEM: bug | not me\n```\n> ROUTE: #a -> x\n- **ITEM: feature | Dark mode | done when: it follows the Mac**\nITEM: Plain title\nROUTE: #new → @cue-teams | after: #A1b2 | why: it knows the header\nASK: Which first? | choices: A / B\nDONE: #a1 and #b2\nroute: #x -> y";
    assert_eq!(
        lines(text),
        vec![
            Line::Item { kind: "feature".into(), title: "Dark mode".into(), done_when: "it follows the Mac".into() },
            Line::Item { kind: "task".into(), title: "Plain title".into(), done_when: String::new() },
            Line::Route { id: "new".into(), to: "cue-teams".into(), after: Some("a1b2".into()), why: "it knows the header".into() },
            Line::Ask { question: "Which first?".into(), choices: vec!["A".into(), "B".into()] },
            Line::Done { ids: vec!["a1".into(), "b2".into()] },
        ]
    );
}

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
fn a_new_lead_starts_with_the_lead_skill_and_the_summary_and_gets_each_message() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let launch = w.hosted.launched().into_iter().last().unwrap();
    assert!(launch.args.iter().any(|a| a.contains("You run this project's Desk in Cue")), "{:?}", launch.args);
    assert!(!launch.args.iter().any(|a| a.contains("Dispatch")), "{:?}", launch.args);
    let lead = desk(&w, &project)["lead"].clone();
    assert_eq!(lead, json!({ "sid": l, "name": format!("{project}-lead"), "state": "live" }));
    // A normal session: no hidden flag on it.
    assert!(w.snapshot().session(&l).get("dispatch").is_none());
    let again = crate::ipc::desk_send_with(w.hub(), &project, "and the footer").unwrap();
    assert_eq!(again["sid"], l.as_str());
    w.settle();
    let last = sent_to(&w, &l).pop().unwrap();
    assert!(last.starts_with("<desk") && last.contains("cue-teams") && last.ends_with("and the footer"), "{last}");
    assert!(last.contains(&format!("You: {project}-lead")), "{last}");
    // A second new lead takes the next free name, and the first is told it's replaced.
    let l2 = new_lead(&w, &project);
    assert_eq!(desk(&w, &project)["lead"]["name"], format!("{project}-lead-2"));
    assert!(sent_to(&w, &l).iter().any(|t| t.starts_with(&format!("You no longer run {project}'s Desk: {project}-lead-2"))));
    assert_ne!(l, l2);
}

#[test]
fn a_session_you_make_the_lead_is_sent_the_lead_skill_in_the_block() {
    let w = World::new();
    let (h, project) = holder(&w);
    w.hub().desk_add(&project, "task", "Footer", "", None).unwrap();
    let before = sent_to(&w, &h).len();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, Some(&h), false).unwrap();
    w.settle();
    assert_eq!(r["sid"], h.as_str());
    let launches = w.hosted.launched().len();
    let told = sent_to(&w, &h);
    assert_eq!(told.len(), before + 1, "one message");
    let (block, rest) = told.last().unwrap().split_once("</desk>\n\n").unwrap();
    assert!(block.contains("You run this project's Desk in Cue") && block.contains("Footer"), "{block}");
    assert!(rest.starts_with(&format!("You run {project}'s Desk now")), "{rest}");
    assert_eq!(desk(&w, &project)["lead"], json!({ "sid": h, "name": "cue-teams", "state": "live" }));
    // Picked again while live: nothing more is sent.
    crate::ipc::desk_set_lead_with(w.hub(), &project, Some(&h), false).unwrap();
    assert_eq!(sent_to(&w, &h).len(), before + 1);
    assert_eq!(w.hosted.launched().len(), launches);
    // Another project's session can't lead this one.
    assert!(crate::ipc::desk_set_lead_with(w.hub(), "elsewhere", Some(&h), false).is_err());
}

#[test]
fn a_message_needs_a_lead_that_hasnt_ended() {
    let w = World::new();
    let (_, project) = holder(&w);
    assert_eq!(crate::ipc::desk_send_with(w.hub(), &project, "fix it").unwrap_err(), "no lead");
    let l = new_lead(&w, &project);
    w.hub().drop_session(&l);
    assert_eq!(desk(&w, &project)["lead"]["state"], "ended");
    assert_eq!(desk(&w, &project)["lead"]["name"], format!("{project}-lead"));
    assert_eq!(crate::ipc::desk_send_with(w.hub(), &project, "fix it").unwrap_err(), "lead ended");
}

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
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Say DONE: #{id}"))));
    ends(&w, &h, &format!("Fixed.\nDECIDED: kept the sticky header — it was asked for\nDONE: #{id}"));
    let desk_now = desk(&w, &project);
    assert_eq!(item(&desk_now, "Header jumps")["state"], "to_check");
    assert_eq!(desk_now["decisions"][0]["by"], "cue-teams");
    assert_eq!(desk_now["decisions"][0]["item_id"], id.as_str());
}

#[test]
fn route_to_me_keeps_the_item_with_the_lead() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let before = sent_to(&w, &l).len();
    ends(&w, &l, "ITEM: Typo in the footer\nROUTE: #new -> me | why: one line");
    let now = desk(&w, &project);
    let it = item(&now, "Typo in the footer");
    assert_eq!((it["state"].as_str(), it["holder"]["sid"].as_str()), (Some("working"), Some(l.as_str())), "{now}");
    let id = it["id"].as_str().unwrap().to_string();
    assert_eq!(now["decisions"][0]["text"], format!("Kept #{id} to do itself"));
    assert_eq!(sent_to(&w, &l).len(), before, "it isn't sent its own item");
    ends(&w, &l, &format!("Fixed it.\nDONE: #{id}"));
    assert_eq!(item(&desk(&w, &project), "Typo in the footer")["state"], "to_check");
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
    assert!(told.contains(&format!("New item #{} (bug): Logo blurs", it["id"].as_str().unwrap())) && told.contains("from cue-teams") && told.ends_with("Route it now: ROUTE it to a session, or -> me."), "{told}");
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
    assert_eq!((given.state.as_str(), given.holder.as_ref().map(|x| x.name.as_str())), ("working", Some("cue-teams")));
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
fn a_queued_item_starts_when_the_one_before_it_is_done() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &h, "On it.");
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    assert_eq!(item(&desk(&w, &project), "Footer")["state"], "queued");
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("#{b}"))));
    ends(&w, &h, &format!("DONE: #{a}"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "to_check");
    assert_eq!(item(&now, "Footer")["state"], "working");
    let started = sent_to(&w, &h).into_iter().find(|t| t.contains(&format!("Desk item #{b}"))).unwrap();
    assert!(!started.contains("same file"), "why it was queued isn't why it starts: {started}");
}

#[test]
fn an_item_isnt_queued_after_one_that_waits_on_it() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Logo\nROUTE: #new -> cue-teams | after: #{b} | why: same file"));
    let c = item(&desk(&w, &project), "Logo")["id"].as_str().unwrap().to_string();
    // Header after Logo: Logo waits on Footer, which waits on Header.
    ends(&w, &d, &format!("ROUTE: #{a} -> cue-teams | after: #{c} | why: loop"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "working", "{now}");
    assert!(item(&now, "Header")["after"].is_null(), "{now}");
    assert_eq!(item(&now, "Logo")["state"], "queued");
    // The lead hears of it in its next summary, once.
    let s = summary(&w, &project);
    assert!(s.contains(&format!("Cue couldn't queue #{a} after #{c}, which waits on it")), "{s}");
    crate::ipc::desk_send_with(w.hub(), &project, "and?").unwrap();
    let s = summary(&w, &project);
    assert!(!s.contains("couldn't queue"), "told once: {s}");
}

#[test]
fn a_parked_lead_that_wont_resume_says_so() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    w.hub().add_parked(w.hub().session_record(&d).unwrap());
    assert_eq!(desk(&w, &project)["lead"]["state"], "parked");
    let launches = w.hosted.launched().len();
    w.hosted.refuse_once("it won't resume");
    let e = crate::ipc::desk_send_with(w.hub(), &project, "back again").unwrap_err();
    assert!(e.contains("it won't resume") && e.contains("back again"), "{e}");
    assert_eq!(w.hosted.launched().len(), launches, "no new lead started in its place");
    assert_eq!(desk(&w, &project)["lead"]["sid"], d.as_str());
}

#[test]
fn a_change_of_lead_that_fails_keeps_the_lead_it_had() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let told = sent_to(&w, &l).len();
    // A parked session that won't resume.
    let s = second(&w, "other");
    w.hub().add_parked(w.hub().session_record(&s).unwrap());
    w.hosted.refuse_once("it won't resume");
    let e = crate::ipc::desk_set_lead_with(w.hub(), &project, Some(&s), false).unwrap_err();
    assert!(e.contains("it won't resume"), "{e}");
    assert_eq!(desk(&w, &project)["lead"]["sid"], l.as_str());
    assert_eq!(w.hub().desk_lead_sid(&project).as_deref(), Some(l.as_str()));
    // A new lead that won't start.
    w.hosted.refuse_once("it won't start");
    assert!(crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap_err().contains("it won't start"));
    assert_eq!(desk(&w, &project)["lead"]["sid"], l.as_str());
    // The lead it kept was never told it was replaced, and still gets your messages.
    assert_eq!(sent_to(&w, &l).len(), told);
    assert_eq!(crate::ipc::desk_send_with(w.hub(), &project, "on with it").unwrap()["sid"], l.as_str());
    // Kept across a relaunch too.
    assert_eq!(crate::db::desk_load(0).2.iter().map(|(_, sid, _)| sid.clone()).collect::<Vec<_>>(), vec![l.clone()]);
}

#[test]
fn a_first_lead_that_fails_to_start_leaves_no_lead() {
    let w = World::new();
    let (_, project) = holder(&w);
    w.hosted.refuse_once("it won't start");
    assert!(crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).is_err());
    assert_eq!(desk(&w, &project)["lead"], Value::Null);
    assert!(crate::db::desk_load(0).2.is_empty());
    assert_eq!(crate::ipc::desk_send_with(w.hub(), &project, "fix it").unwrap_err(), "no lead");
}

#[test]
fn a_change_of_lead_waits_for_one_under_way() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    // Another change (or start, or resume) holds the project: this one, even to a live session, doesn't go ahead.
    w.hub().desk_claim_start(&project).unwrap();
    let e = crate::ipc::desk_set_lead_with(w.hub(), &project, Some(&h), false).unwrap_err();
    assert!(e.contains("starting already"), "{e}");
    assert_eq!(w.hub().desk_lead_sid(&project).as_deref(), Some(l.as_str()));
    drop(w.hub().desk_claim_done(&project));
    // Once it's over, it does.
    crate::ipc::desk_set_lead_with(w.hub(), &project, Some(&h), false).unwrap();
    assert_eq!(w.hub().desk_lead_sid(&project).as_deref(), Some(h.as_str()));
}

#[test]
fn a_message_held_while_a_change_of_lead_fails_goes_to_the_lead_put_back() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let told = sent_to(&w, &l).len();
    // A change under way (to a lead that never comes up) fails, and the lead it had is put back.
    let r = send_while_starting(&w, &project, "and the footer", |s| {
        let before = w.hub().desk_lead(&project);
        w.hub().desk_put_lead(&project, "never-up", "ghost");
        w.hub().desk_put_back_lead(&project, before);
        s.back();
    });
    assert_eq!(r.unwrap()["sid"], l.as_str());
    let got = sent_to(&w, &l);
    assert_eq!(got.len(), told + 1);
    assert!(got.last().unwrap().ends_with("and the footer"), "{:?}", got.last());
    // With no lead to put back, you're told: no lead.
    let e = send_while_starting(&w, "nowhere", "and the logo", |s| s.back()).unwrap_err();
    assert!(e.contains("no lead") && e.contains("and the logo"), "{e}");
}

#[test]
fn a_message_held_while_the_lead_changes_reaches_the_new_one_as_it_sees_the_desk() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    let id: String = h.chars().take(8).collect();
    // Sent while the Desk goes from `l` to `h`: it reaches `h`, with the summary as `h` sees it.
    let r = send_while_starting(&w, &project, "and the footer", |s| {
        w.hub().desk_put_lead(&project, &h, "cue-teams");
        drop(s.done(Ok(h.clone())));
    });
    assert_eq!(r.unwrap()["sid"], h.as_str());
    let got = sent_to(&w, &h);
    let (block, rest) = got.last().unwrap().split_once("</desk>\n\n").unwrap();
    assert_eq!(rest, "and the footer");
    let routes = block.lines().find(|l| l.starts_with("Sessions you can route to:")).unwrap();
    assert!(!routes.contains(&id), "itself isn't a route: {routes}");
    assert!(routes.contains(&format!("{project}-lead")), "the last lead is: {routes}");
    assert!(block.contains("\nYou: cue-teams"), "{block}");
    assert!(!sent_to(&w, &l).iter().any(|m| m.contains("and the footer")));
}

#[test]
fn a_session_runs_one_desk() {
    let w = World::new();
    let (h, project) = holder(&w);
    w.hub().desk_put_lead("elsewhere", &h, "cue-teams");
    let e = crate::ipc::desk_set_lead_with(w.hub(), &project, Some(&h), false).unwrap_err();
    assert_eq!(e, "cue-teams already runs elsewhere's Desk");
    assert_eq!(desk(&w, &project)["lead"], Value::Null);
}

#[test]
fn put_before_takes_the_others_place_in_the_queue() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let b = item(&desk(&w, &project), "Footer")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Logo\nROUTE: #new -> cue-teams | after: #{b} | why: same file"));
    let c = item(&desk(&w, &project), "Logo")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Menu\nROUTE: #new -> cue-teams | after: #{c} | why: same file"));
    // Header at work <- Footer <- Logo <- Menu; Logo put before Footer: Header <- Logo <- Footer <- Menu.
    w.hub().desk_item_act(&c, "put_before", "").unwrap();
    w.settle();
    let now = desk(&w, &project);
    assert_eq!((item(&now, "Logo")["state"].as_str(), item(&now, "Logo")["after"].as_str()), (Some("queued"), Some(a.as_str())), "{now}");
    assert_eq!(item(&now, "Footer")["after"], c.as_str());
    assert_eq!(item(&now, "Menu")["after"], b.as_str());
    assert!(!sent_to(&w, &h).iter().any(|t| t.contains(&format!("Desk item #{c}"))), "Logo still waits on Header");
    ends(&w, &h, &format!("DONE: #{a}"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Logo")["state"], "working");
    assert_eq!(item(&now, "Footer")["state"], "queued");
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
fn the_summary_is_one_leading_block_then_a_blank_line() {
    let w = World::new();
    let (_, project) = holder(&w);
    w.hub().desk_add(&project, "task", "Close </desk> early", "", None).unwrap();
    let d = new_lead(&w, &project);
    crate::ipc::desk_send_with(w.hub(), &project, "next").unwrap();
    w.settle();
    let last = sent_to(&w, &d).pop().unwrap();
    let (block, rest) = last.split_once("</desk>\n\n").unwrap();
    assert!(block.starts_with("<desk project=\"") && !block.contains("</desk>"), "{last}");
    assert_eq!(rest, "next");
}

#[test]
fn a_line_said_again_makes_no_second_item_or_send() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let said = "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows";
    ends(&w, &d, said);
    let sends = sent_to(&w, &h).len();
    ends(&w, &d, said);
    let now = desk(&w, &project);
    assert_eq!(now["items"].as_array().unwrap().len(), 1, "{now}");
    assert_eq!(sent_to(&w, &h).len(), sends);
    assert_eq!(now["decisions"].as_array().unwrap().len(), 1, "{now}");
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

#[test]
fn a_message_while_the_lead_resumes_waits_for_it() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    w.hub().add_parked(w.hub().session_record(&d).unwrap());
    let first = w.hub().desk_claim(&project, "one");
    assert!(matches!(first, Ok(crate::hub::DeskClaim::Resume(_))));
    let second = w.hub().desk_claim(&project, "two");
    assert!(matches!(second, Ok(crate::hub::DeskClaim::Wait(_))));
    assert!(w.hub().desk_claim_start(&project).is_err(), "a lead is coming up already");
    let held = w.hub().desk_claim_done(&project);
    assert_eq!(held.iter().map(|h| h.text.as_str()).collect::<Vec<_>>(), vec!["two"]);
    assert!(matches!(w.hub().desk_claim(&project, "three"), Ok(crate::hub::DeskClaim::Resume(_))));
    drop(w.hub().desk_claim_done(&project));
}

/// A message sent to `project`'s lead while another start of it is under way (`start`): what it's told.
fn send_while_starting(w: &World, project: &str, text: &str, start: impl FnOnce(crate::ipc::DeskStart)) -> Result<Value, String> {
    let hub = w.hub().clone();
    let (p, t) = (project.to_string(), text.to_string());
    w.hub().desk_claim_start(project).unwrap();
    let waiter = std::thread::spawn(move || crate::ipc::desk_send_with(&hub, &p, &t));
    while w.hub().desk_held(project) == 0 {
        std::thread::yield_now();
    }
    start(crate::ipc::DeskStart::new(w.hub(), project));
    waiter.join().unwrap()
}

#[test]
fn a_message_held_while_a_lead_fails_to_start_comes_back_to_you() {
    let w = World::new();
    // Nowhere to start one: you're told.
    let e = crate::ipc::desk_set_lead_with(w.hub(), "nowhere", None, true).unwrap_err();
    assert!(e.contains("no folder known"), "{e}");
    assert!(w.hub().desk_claim_start("nowhere").is_ok(), "the claim was let go");
    drop(w.hub().desk_claim_done("nowhere"));
    // Held while a start fails: told, with your words.
    let e = send_while_starting(&w, "nowhere", "and the footer", |s| drop(s.done(Err("it broke".into())))).unwrap_err();
    assert!(e.contains("it broke") && e.contains("and the footer"), "{e}");
    // The start gave up without a word (it panicked): told, and the claim is let go.
    let e = send_while_starting(&w, "nowhere", "and the logo", |s| drop(s)).unwrap_err();
    assert!(e.contains("and the logo"), "{e}");
    assert_eq!(crate::ipc::desk_send_with(w.hub(), "nowhere", "last").unwrap_err(), "no lead");
}

#[test]
fn a_line_in_backticks_is_an_example() {
    assert!(lines("Say `ITEM: bug | x` to add one.\n`ROUTE: #a -> b`").is_empty());
}

#[test]
fn a_queued_item_doesnt_need_you_for_its_holders_ask() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    let origin = w.hub().session_origin(&h).unwrap();
    let input = json!({ "questions": [{ "question": "Sticky?", "header": "Header", "options": [], "multiSelect": false }] });
    w.hub().add_ask(origin, crate::hub::NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() });
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "needs_you");
    assert_eq!(item(&now, "Footer")["state"], "queued");
}

#[test]
fn a_parked_lead_is_resumed_with_its_instructions() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    w.hub().add_parked(w.hub().session_record(&d).unwrap());
    let launches = w.hosted.launched().len();
    let again = crate::ipc::desk_send_with(w.hub(), &project, "back again").unwrap();
    w.settle();
    assert_eq!(again["sid"], d.as_str());
    assert_eq!(desk(&w, &project)["lead"]["state"], "live");
    let launched = w.hosted.launched();
    assert_eq!(launched.len(), launches + 1, "resumed once");
    let resumed = launched.last().unwrap();
    assert!(resumed.args.iter().any(|a| a.contains("You run this project's Desk in Cue")), "{:?}", resumed.args);
    assert!(w.snapshot().find(&d).is_some(), "live again");
}

#[test]
fn a_parked_session_made_the_lead_is_resumed_with_the_desk() {
    let w = World::new();
    let (h, project) = holder(&w);
    w.hub().add_parked(w.hub().session_record(&h).unwrap());
    let launches = w.hosted.launched().len();
    crate::ipc::desk_set_lead_with(w.hub(), &project, Some(&h), false).unwrap();
    w.settle();
    assert_eq!(w.hosted.launched().len(), launches + 1, "resumed");
    assert_eq!(desk(&w, &project)["lead"]["state"], "live");
    assert!(w.snapshot().find(&h).is_some(), "live again");
}

#[test]
fn the_leads_ask_is_an_ask_of_its_session_marked_for_the_desk() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ASK: Header or footer first? | choices: Header / Footer");
    let snap = w.snapshot();
    let card = snap.card(&d);
    assert_eq!(card["desk"], true);
    assert!(card.get("from_dispatch").is_none());
    assert_eq!(card["project"], project.as_str());
    let id = card["id"].as_str().unwrap().to_string();
    w.answer(&id, json!({ "behavior": "allow", "answers": { "Header or footer first?": "Footer" } }));
    w.settle();
    assert!(w.snapshot().cards_of(&d).is_empty());
    assert!(sent_to(&w, &d).iter().any(|t| t.contains("The answer: Footer")), "{:?}", sent_to(&w, &d));
}

#[test]
fn an_item_needs_you_while_its_holder_asks() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "working");
    let origin = w.hub().session_origin(&h).unwrap();
    let input = json!({ "questions": [{ "question": "Keep it sticky?", "header": "Header", "options": [{ "label": "Yes", "description": "" }], "multiSelect": false }] });
    w.hub().add_ask(origin, crate::hub::NewAsk { kind: "question".into(), tool_name: "AskUserQuestion".into(), tool_input: input, suggestions: Value::Null, context: vec![], agent_id: String::new() });
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Header")["state"], "needs_you");
    assert_eq!(item(&now, "Header")["ask"]["text"], "Keep it sticky?");
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
    assert_eq!(w.hub().desk_item_act(&it.id, "drop", "").unwrap().state, "dropped");
    assert!(desk(&w, "web").is_null() || desk(&w, "web")["items"].as_array().unwrap().is_empty());
    assert!(w.hub().desk_item_act("nope", "drop", "").is_err());
}

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

/// A hook session `sid` of project "shop" in tmux pane %`sid` (its box empty), its last reply `left_ms` before its
/// hour's cache runs out with `context` tokens, at its prompt.
fn shop_session(w: &World, sid: &str, left_ms: u64, context: u64) {
    let a = w.session(sid).in_project("shop").in_tmux(&format!("%{sid}"));
    a.transcript().cached_reply(3_600_000 - left_ms, context, 3600);
    a.stop("Done.");
    w.terminal.pane(sid, |p| p.screen = Some("────────\n❯ \n────────".into()));
}

/// "shop"'s lead (lead01) says `text` at its turn's end.
fn shop_lead_says(w: &World, text: &str) {
    w.session("lead01").in_project("shop").in_tmux("%lead01").stop(text);
}

/// The project "shop"'s sessions go under, with its lead (lead01) and an item held by webui1.
fn shop_desk(w: &World) -> String {
    let project = w.snapshot().session("lead01")["project"].as_str().unwrap().to_string();
    w.hub().desk_put_lead(&project, "lead01", "lead");
    w.hub().desk_add(&project, "bug", "Header", "", Some("webui1")).unwrap();
    project
}

fn decisions(w: &World, project: &str) -> Vec<Value> {
    desk(w, project)["decisions"].as_array().cloned().unwrap_or_default()
}

#[test]
fn the_lead_sees_each_sessions_context_and_what_cue_will_do_and_can_say_keep() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 12 * 60_000, 142_000);
    shop_session(&w, "idle01", 30 * 60_000, 160_000);
    let project = shop_desk(&w);
    let lead_summary = || w.hub().desk_summary(&project, "lead01", false).0;
    w.tick(true);
    let s = lead_summary();
    assert!(s.contains("sid:webui1, session, waiting, 142k, cache 12m → Cue will hand off at expiry)"), "{s}");
    assert!(s.contains("sid:idle01, session, waiting, 160k, cache 30m)"), "holds nothing: Cue leaves it be: {s}");
    assert!(s.contains(", 90k, cache 20m. ROUTE"), "the lead under 100k: left be: {s}");

    // The lead says keep: Cue won't, and the lead's call can be undone.
    shop_lead_says(&w, "Leave it.\nCONTEXT: webui1 -> keep | why: its next item needs the detail");
    let d = decisions(&w, &project);
    assert_eq!((d[0]["by"].as_str(), d[0]["text"].as_str(), d[0]["why"].as_str(), d[0]["undo"].as_str()), (Some("shop"), Some("Kept shop's context as it is"), Some("its next item needs the detail"), Some("can")), "{d:?}");
    assert!(lead_summary().contains("142k, cache 12m)"), "{}", lead_summary());
    w.hub().desk_undo(d[0]["id"].as_str().unwrap()).unwrap();
    w.settle();
    assert!(lead_summary().contains("142k, cache 12m → Cue will hand off at expiry)"), "undone: Cue's rule again");

    // Kept again while it waits: for this wait only; its next turn ends it.
    shop_lead_says(&w, "CONTEXT: webui1 -> keep");
    assert!(lead_summary().contains("142k, cache 12m)"), "{}", lead_summary());
    w.advance(50 * 60_000); // (its reply is dated 48 minutes back: after the keep)
    next_turn(&w, "webui1", "m2", 12 * 60_000, 142_000);
    w.tick(true);
    assert!(lead_summary().contains("142k, cache 12m → Cue will hand off at expiry)"), "{}", lead_summary());
}

/// Session `sid` works a turn and ends it with reply `id`, `left_ms` before its hour's cache runs out.
fn next_turn(w: &World, sid: &str, id: &str, left_ms: u64, context: u64) {
    let a = w.session(sid).in_project("shop").in_tmux(&format!("%{sid}"));
    a.prompt("go on");
    a.transcript().cached_reply_as(id, 3_600_000 - left_ms, context, 3600);
    a.stop("Done.");
}

#[test]
fn a_keep_said_while_a_session_works_covers_its_wait_after_that_turn() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 12 * 60_000, 142_000);
    let project = shop_desk(&w);
    let lead_summary = || w.hub().desk_summary(&project, "lead01", false).0;
    w.session("webui1").in_project("shop").in_tmux("%webui1").prompt("go on");
    w.advance(1_000);
    shop_lead_says(&w, "CONTEXT: webui1 -> keep | why: next item builds on it");
    assert_eq!(decisions(&w, &project)[0]["undo"], "can");
    // Its turn ends after the keep: the wait that follows is the one kept.
    w.advance(50 * 60_000); // (its reply is dated 48 minutes back: after the keep)
    next_turn(&w, "webui1", "m2", 12 * 60_000, 142_000);
    w.tick(true);
    assert!(lead_summary().contains("sid:webui1, session, waiting, 142k, cache 12m)"), "{}", lead_summary());
    // The turn after that ends it.
    w.advance(50 * 60_000); // (its reply is dated 48 minutes back: after the keep)
    next_turn(&w, "webui1", "m3", 12 * 60_000, 142_000);
    w.tick(true);
    assert!(lead_summary().contains("sid:webui1, session, waiting, 142k, cache 12m → Cue will hand off at expiry)"), "{}", lead_summary());
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
    assert!(s.contains("sid:webui1, session, waiting, 142k, cache 1m)"), "{s}");
}

#[test]
fn once_a_hand_off_was_sent_this_wait_the_lead_is_not_told_cue_will() {
    let w = World::new();
    shop_session(&w, "lead01", 20 * 60_000, 90_000);
    shop_session(&w, "webui1", 12 * 60_000, 142_000);
    let project = shop_desk(&w);
    w.tick(true);
    assert!(w.hub().desk_summary(&project, "lead01", false).0.contains("142k, cache 12m → Cue will"));
    crate::db::handoff_sent("webui1", "", false, 0);
    w.tick(true);
    let s = w.hub().desk_summary(&project, "lead01", false).0;
    assert!(s.contains("sid:webui1, session, waiting, 142k, cache 12m)"), "{s}");
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
