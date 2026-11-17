//! The Desk (desk/, hub/desk.rs, hub/desk_lead.rs): a project's lead (a session you pick, or a new one Cue starts),
//! the lines it and the project's sessions say at the end of a turn, and what `state.desk` shows.

use super::team::{lead, says, writes};
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
    // On the Desk, so its lead routes to it without asking you (desk_members.rs has the ones that aren't).
    w.hub().desk_set_member(&project, &sid, true).unwrap();
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
    let project = w.snapshot().session(&sid)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &sid, true).unwrap();
    sid
}

pub(super) fn desk(w: &World, project: &str) -> Value {
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
            Line::Item { kind: "feature".into(), title: "Dark mode".into(), done_when: "it follows the Mac".into(), why: String::new() },
            Line::Item { kind: "task".into(), title: "Plain title".into(), done_when: String::new(), why: String::new() },
            Line::Route { id: "new".into(), to: "cue-teams".into(), after: Some("a1b2".into()), why: "it knows the header".into(), kind: None, crew: None },
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
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "needs_you");
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
fn a_desk_lead_is_told_to_only_dispatch_with_no_crew_and_isnt_sent_reviews() {
    let skill = crate::desk::LEAD_SKILL;
    assert!(!skill.contains("-> me") && !skill.contains("crew") && skill.contains("ROUTE: #<id> -> new"), "{skill}");
    let w = World::new();
    let dir = super::team::repo(&w);
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    // Started with the Desk's instructions, not the crew lead's.
    let args = w.hosted.launched().last().map(|l| l.args.clone()).unwrap_or_default();
    let prompt = args.iter().position(|a| a == "--append-system-prompt").and_then(|i| args.get(i + 1)).cloned().unwrap_or_default();
    assert!(prompt.starts_with("# Running this project's Desk") && !prompt.contains("crew lead"), "{prompt}");
    assert!(!w.hub().desk_summary(&project, &l, false).0.contains("-> me"));
    // A teammate it started anyway finishes: no review goes to it.
    w.hub().set_crew_commit(&l, "wait").unwrap();
    says(&w, &l, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    ends(&w, &l, "started rows");
    w.hub().drain_reviews();
    w.settle();
    assert!(!sent_to(&w, &l).iter().any(|t| t.starts_with("Review your teammate")), "{:?}", sent_to(&w, &l));
}
#[test]
fn a_route_to_me_can_name_the_teammate_doing_it() {
    assert_eq!(
        lines("ROUTE: #a1 -> me | crew: `fixer` | why: two files"),
        vec![Line::Route { id: "a1".into(), to: "me".into(), after: None, why: "two files".into(), kind: None, crew: Some("fixer".into()) }]
    );
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
    let it = w.hub().desk_add_with(&project, "", "Logo blurs", "", None, &[png]).unwrap();
    assert_eq!(it.images.len(), 1);
    let path = &it.images[0];
    assert!(path.contains("/uploads/kept/") && std::fs::read(path).unwrap() == b"not really a png", "{path}");
    w.advance(1000);
    assert!(sent_to(&w, &l).pop().unwrap().contains(&format!("New item #{}: Logo blurs | pictures: `{path}`", it.id)));
    ends(&w, &l, &format!("ROUTE: #{} -> cue-teams | kind: bug | why: it knows the header", it.id));
    assert!(sent_to(&w, &h).last().unwrap().contains(&format!("\nPictures with it (open each to see it): `{path}`")));
    // Not an image: nothing is added.
    let txt = crate::model::Upload { name: "notes.txt".into(), mime: "text/plain".into(), data: "aGk=".into() };
    assert!(w.hub().desk_add_with(&project, "", "Notes", "", None, &[txt]).is_err());
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
    let routes = block.lines().find(|l| l.starts_with("Sessions on the Desk, you can route to:")).unwrap();
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
fn a_decided_line_in_the_turn_that_says_done_is_the_finished_items() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: Header\nROUTE: #new -> cue-teams | why: first");
    let a = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    ends(&w, &d, &format!("ITEM: Footer\nROUTE: #new -> cue-teams | after: #{a} | why: same file"));
    ends(&w, &h, &format!("Moved it.\nDECIDED: kept the class name — renaming it touches the tests\nDONE: #{a}"));
    let now = desk(&w, &project);
    assert_eq!(item(&now, "Footer")["state"], "working", "the next one started");
    let c = now["decisions"].as_array().unwrap().iter().find(|c| c["text"].as_str().unwrap().starts_with("kept the class name")).unwrap();
    assert_eq!(c["item_id"].as_str(), Some(a.as_str()), "{now}");
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

/// What a holder made, named in the turn it says DONE (a page's link, a mock or a build by its full path), is kept
/// on the item for the Desk to link to; a path that's only code (no kind made to be looked at) isn't.
#[test]
fn what_the_turn_that_says_done_made_is_kept_on_the_item() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: task | Mocks for the list | done when: three mocks\nROUTE: #new -> cue-teams | why: it knows the list");
    let id = item(&desk(&w, &project), "Mocks for the list")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Three mocks: [the page](https://claude.ai/artifact/abc123), and `/tmp/mocks/list.html`.\nChanged `/src/ui/desk.js` too.\nDONE: #{id}"));
    let now = desk(&w, &project);
    let it = item(&now, "Mocks for the list");
    assert_eq!(it["state"], "to_check");
    assert_eq!(it["made"], serde_json::json!(["https://claude.ai/artifact/abc123", "/tmp/mocks/list.html"]), "{it}");
}

/// The Desk's lead runs in fast mode: Cue turns it on as it starts the lead and as it resumes it. A session the lead
/// starts for an item starts as any other.
#[test]
fn the_lead_starts_and_resumes_in_fast_mode_and_no_other_session_does() {
    let fast = |args: &[String]| args.windows(2).any(|a| a[0] == "--settings" && a[1] == r#"{"fastMode":true}"#);
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    assert!(fast(&w.hosted.launched().last().unwrap().args), "started in fast mode");
    ends(&w, &l, "ITEM: Typo in the footer\nROUTE: #new -> new | why: nobody's near it");
    w.settle();
    let started = w.hosted.launched().last().unwrap().args.clone();
    assert!(!started.iter().any(|a| a.contains("You run this project's Desk in Cue")), "the item's session: {started:?}");
    assert!(!fast(&started), "the item's session starts as it is");
    w.hub().add_parked(w.hub().session_record(&l).unwrap());
    crate::ipc::desk_send_with(w.hub(), &project, "back again").unwrap();
    w.settle();
    assert!(fast(&w.hosted.launched().last().unwrap().args), "resumed in fast mode");
}

/// A picture pasted into the lead's box reaches the lead with your words, as a session chat's does: sent to it live
/// (shown in its chat), or named by where it is when the lead is resumed with it.
#[test]
fn a_picture_in_the_leads_box_reaches_the_lead_live_or_resumed() {
    use base64::Engine;
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let png = || crate::model::Upload { name: "shot.png".into(), mime: "image/png".into(), data: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b"a png")) };
    crate::ipc::desk_send_images(w.hub(), &project, "This is cut off", &[png()]).unwrap();
    let sent = sent_to(&w, &l).pop().unwrap();
    assert!(sent.contains("This is cut off") && sent.contains("/uploads/") && sent.contains(".png"), "{sent}");
    let thread = w.snapshot().find(&l).unwrap()["thread"].clone();
    let yours = thread.as_array().unwrap().iter().rev().find(|e| e["role"] == "you").unwrap().clone();
    assert_eq!(yours["images"].as_array().map(Vec::len), Some(1), "shown in its chat: {yours}");
    // Only a picture, no words: it still goes.
    crate::ipc::desk_send_images(w.hub(), &project, "", &[png()]).unwrap();
    assert!(sent_to(&w, &l).pop().unwrap().contains(".png"));
    assert!(crate::ipc::desk_send_images(w.hub(), &project, " ", &[]).is_err(), "nothing to send");
    // Parked: resumed with the message, which names the picture.
    w.hub().add_parked(w.hub().session_record(&l).unwrap());
    crate::ipc::desk_send_images(w.hub(), &project, "And this one", &[png()]).unwrap();
    w.settle();
    let resumed = sent_to(&w, &l).pop().unwrap();
    assert!(resumed.contains("And this one") && resumed.contains(".png"), "{resumed}");
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
fn title_and_split_lines_are_read() {
    let text = "TITLE: #12 | Header jumps on scroll\nSPLIT: #12 | \"Header jumps\" | Footer is cut off\nSPLIT: #12 | only one\nTITLE: 12 | no hash";
    assert_eq!(
        lines(text),
        vec![
            Line::Title { id: "12".into(), title: "Header jumps on scroll".into() },
            Line::Split { id: "12".into(), titles: vec!["Header jumps".into(), "Footer is cut off".into()] },
        ]
    );
    // A quote that starts the title, not wrapped around it, is the title's own.
    assert_eq!(
        lines("TITLE: #12 | \"Looks right\" is too loud\nTITLE: #13 | **`Wrapped`**"),
        vec![
            Line::Title { id: "12".into(), title: "\"Looks right\" is too loud".into() },
            Line::Title { id: "13".into(), title: "Wrapped".into() },
        ]
    );
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

/// A holder whose turn ends on its item with no DONE: the item waits on you with its last line (not "working"), and
/// Cue asks it once if it's done. The turn that answers isn't asked again, nor one that ends on a question; a later
/// stall is asked once more; DONE puts it to check.
#[test]
fn an_item_whose_holder_stops_without_done_waits_on_you_and_is_asked_once() {
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
    turn("Header", "Looked at the header.\nThe sticky part is in place.");
    let it = item(&desk(&w, &project), "Header").clone();
    assert_eq!(it["state"], "needs_you", "{it}");
    assert_eq!(it["latest"], "The sticky part is in place.");
    assert!(it["ask"].is_null(), "no ask box for a plain stop: {it}");
    assert_eq!(asked(), 1, "asked once: {:?}", sent_to(&w, &h));
    // Its answer: still no DONE, so it waits on you with what's left, and isn't asked again.
    turn(&words, "The footer still overlaps it.");
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "needs_you");
    assert_eq!(item(&desk(&w, &project), "Header")["latest"], "The footer still overlaps it.");
    assert_eq!(asked(), 1);
    // A turn that ends on a question for you isn't asked either.
    turn("Go on", "Should the footer move too?");
    assert_eq!(asked(), 1);
    // A new stall later: asked once more.
    turn("Yes, move it", "Moved some of it.");
    assert_eq!(asked(), 2);
    turn(&words, &format!("All done.\nDONE: #{id}"));
    assert_eq!(item(&desk(&w, &project), "Header")["state"], "to_check");
    assert_eq!(asked(), 2);
}

/// Not a stall: a holder whose crew is still at work as its turn ends, a crew member holding an item (its lead's
/// turns aren't its own), and an item the Desk's lead holds (its turns are routing). No ask, and none waits on you;
/// once the crew is done, the holder's next turn with no DONE is one.
#[test]
fn a_holder_with_crew_at_work_a_crew_member_or_the_desk_lead_isnt_asked_if_done() {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let header = item(&desk(&w, &project), "Header")["id"].as_str().unwrap().to_string();
    let asks = |sid: &str| sent_to(&w, sid).iter().filter(|t| t.starts_with("Done? Write DONE")).count();
    let state = |title: &str| item(&desk(&w, &project), title)["state"].as_str().unwrap_or("").to_string();
    // The holder hands the header to its crew, still at work as its turn ends.
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "Header".into() });
    let mut started = writes("toolu_F", "agentF", "fixer", &[]);
    let finished = started.pop().unwrap();
    says(&w, &h, started);
    w.advance(1000);
    ends(&w, &h, "Handed the header to fixer.");
    assert_eq!((state("Header"), asks(&h)), ("working".to_string(), 0), "its crew is on it");
    // The Footer goes to that crew member: its lead's turns don't stall it.
    ends(&w, &l, "ITEM: Footer\nROUTE: #new -> fixer | why: it's in there");
    w.settle();
    assert_eq!(item(&desk(&w, &project), "Footer")["holder"]["kind"], "crew");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "Footer too".into() });
    w.advance(1000);
    ends(&w, &h, "Told fixer.");
    assert_eq!((state("Footer"), asks(&h)), ("working".to_string(), 0));
    // The crew is done: the holder's next turn with no DONE on the header is a stall, asked once.
    says(&w, &h, [finished]);
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "fixer finished".into() });
    w.advance(1000);
    ends(&w, &h, "fixer finished its part.");
    assert_eq!(state("Header"), "needs_you");
    assert_eq!(sent_to(&w, &h).iter().filter(|t| **t == format!("Done? Write DONE: #{header}, or say what's left")).count(), 1);
    // An item the Desk's lead holds: its routing turns aren't stalls.
    let added = w.hub().desk_add(&project, "task", "Colours", "", None).unwrap();
    w.hub().desk_give(&added.id, &l).unwrap();
    w.settle();
    w.advance(1000);
    w.hosted_says(&l, HostedEvent::Taken { text: "route".into() });
    w.advance(1000);
    ends(&w, &l, "Nothing to route.");
    assert_eq!((state("Colours"), asks(&l)), ("working".to_string(), 0));
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
        assert_eq!(state(), "needs_you");
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

/// What you write on an item's page, to its lead or its holder, and what you send back, are in its Steps: your words,
/// by you, with whom they went to. Writing isn't news about the item (its `updated` stays).
#[test]
fn what_you_write_on_an_items_page_is_in_its_steps() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows");
    let it = item(&desk(&w, &project), "Header").clone();
    let id = it["id"].as_str().unwrap().to_string();
    let lead_name = desk(&w, &project)["lead"]["name"].as_str().unwrap().to_string();
    w.hub().desk_item_act(&id, "wrote_lead", "Make it sticky too").unwrap();
    w.hub().desk_item_act(&id, "wrote_holder", " Keep the old colour ").unwrap();
    assert!(w.hub().desk_item_act(&id, "wrote_lead", "  ").is_err());
    let now = desk(&w, &project);
    let mine: Vec<_> = now["decisions"].as_array().unwrap().iter().filter(|d| d["by"] == "You" && d["item_id"] == id.as_str()).map(|d| (d["text"].as_str().unwrap(), d["why"].as_str().unwrap())).collect();
    assert!(mine.contains(&("Make it sticky too", format!("to {lead_name}").as_str())), "{mine:?}");
    assert!(mine.contains(&("Keep the old colour", "to cue-teams")), "{mine:?}");
    assert_eq!(item(&now, "Header")["updated"], it["updated"]);
}
