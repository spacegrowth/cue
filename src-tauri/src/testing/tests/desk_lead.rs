//! The Desk's lead (desk/hub/lead/run.rs): starting a new lead or making a session the lead, changing it, parking and
//! resuming it, and what happens to a message you send it while it starts, changes or resumes.

use super::desk::{desk, ends, holder, new_lead, second, sent_to};
use super::team::{says, writes};
use crate::testing::World;
use serde_json::{json, Value};

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
    assert!(last.contains(&format!("Desk lead: {project}-lead (you)")), "{last}");
    assert!(last.contains("Answer the owner's question or request directly in plain language before any Desk command lines."), "{last}");
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
    let (_, summary) = block.split_once(">\n").unwrap();
    assert!(summary.starts_with("Desk lead: cue-teams (you)\nHow work lands now (LANDS: to propose a change): ") && summary.contains(" · at once: 3\nNew sessions start on: Claude Code's own pick (the Desk's default; name another on a route to new with | model: <name>).\n\nNeeds routing:\n"), "{block}");
    assert!(summary.lines().nth(5).unwrap().contains("Footer"), "{block}");
    assert!(block.contains("You run this project's Desk in Cue"), "{block}");
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
fn a_desk_lead_is_told_to_only_dispatch_with_no_crew_and_isnt_sent_reviews() {
    let skill = crate::desk::LEAD_SKILL;
    assert!(!skill.contains("-> me") && !skill.contains("crew") && skill.contains("ROUTE: #<id> -> new"), "{skill}");
    assert!(skill.contains("Call a piece of work an item") && skill.contains("never a card"), "items, not cards, in titles (#679)");
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
    let (_, routes) = block.split_once("\nDesk routes (or use ROUTE: #<id> -> new):\n").unwrap();
    assert!(!routes.contains(&id), "itself isn't a route: {routes}");
    assert!(routes.contains(&format!("{project}-lead")), "the last lead is: {routes}");
    assert!(block.contains("\nDesk lead: cue-teams (you)"), "{block}");
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

/// The lead's ASK as the option C card (#450): a line under each choice, its suggestion marked Recommended, no "Desk"
/// header; your pick goes back to it in the lead's own words.
#[test]
fn the_leads_ask_carries_a_line_per_choice_and_its_suggestion() {
    let w = World::new();
    let (_, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ASK: Header or footer first? | choices: Header (recommended) — it's what you see first / Footer - smaller, done today / Neither");
    let snap = w.snapshot();
    let card = snap.card(&d);
    let q = &card["tool_input"]["questions"][0];
    assert_eq!(q["header"], "");
    let opts: Vec<(String, String)> = q["options"].as_array().unwrap().iter().map(|o| (o["label"].as_str().unwrap().into(), o["description"].as_str().unwrap().into())).collect();
    assert_eq!(opts, vec![
        ("Header (Recommended)".to_string(), "it's what you see first".to_string()),
        ("Footer".into(), "smaller, done today".into()),
        ("Neither".into(), "".into()),
    ]);
    let id = card["id"].as_str().unwrap().to_string();
    w.answer(&id, json!({ "behavior": "allow", "answers": { "Header or footer first?": "Header (Recommended)" } }));
    w.settle();
    assert!(sent_to(&w, &d).iter().any(|t| t.contains("The answer: Header\n") || t.ends_with("The answer: Header")), "{:?}", sent_to(&w, &d));
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

/// Your message to an idle lead (#587): handed to it before the send returns, with no wait (no draw, no step after), in
/// its chat at once; taken the moment its turn starts. The next one, after that turn ends, the same: every message
/// starts with the same Desk summary, and the first one's turn mustn't be taken for the second's.
#[test]
fn a_message_to_an_idle_lead_is_handed_over_at_once_and_taken_when_its_turn_starts() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    assert_eq!(w.snapshot().state(&l), "waiting", "idle");
    for (n, words) in ["you are ok?", "I see a bug, I'll file it tomorrow"].into_iter().enumerate() {
        let before = sent_to(&w, &l).len();
        crate::ipc::desk_send_with(w.hub(), &project, words).unwrap();
        // Nothing settled: it went with the send itself.
        let sent = sent_to(&w, &l);
        assert_eq!(sent.len(), before + 1, "handed over before the send returned");
        assert!(sent.last().unwrap().ends_with(words), "{:?}", sent.last());
        let s = w.snapshot();
        let mine = s.last_yours(&l);
        assert!(mine["text"].as_str().unwrap().ends_with(words), "in its chat at once: {mine}");
        assert_eq!(mine["pending"], true, "handed over, not yet taken");
        w.hosted_says(&l, crate::hosted::HostedEvent::TurnStarted);
        let s = w.snapshot();
        assert_eq!(s.state(&l), "working");
        let mine = s.last_yours(&l);
        assert!(mine["text"].as_str().unwrap().ends_with(words) && mine["pending"].is_null() && mine["unsent"].is_null(), "message {n} taken: {mine}");
        ends(&w, &l, "Fine.");
    }
    // Neither is left looking as if it wasn't taken.
    let s = w.snapshot();
    let thread = s.session(&l)["thread"].as_array().unwrap().clone();
    let yours: Vec<&Value> = thread.iter().filter(|e| e["role"] == "you").collect();
    assert!(yours.iter().all(|e| e["pending"].is_null() && e["unsent"].is_null()), "{yours:?}");
}

#[test]
fn what_you_say_by_voice_to_the_lead_goes_as_your_typed_words_do_and_its_bubble_has_the_mic() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let said = crate::voice::talk::message("add an item for the notch", false).unwrap();
    let to = crate::voice::talk::to_lead(w.hub(), &project, &said).unwrap();
    assert_eq!(to, l);
    w.settle();
    // After the Desk's block, as a typed message to the lead (#617).
    let last = sent_to(&w, &l).pop().unwrap();
    assert!(last.starts_with("<desk") && last.ends_with(said.as_str()), "{last}");
    // In its chat as yours, said by voice.
    let thread = w.snapshot().session(&l)["thread"].clone();
    let mine = thread.as_array().unwrap().iter().rev().find(|e| e["role"] == "you").cloned().unwrap_or(Value::Null);
    assert!(mine["text"].as_str().unwrap_or("").ends_with(said.as_str()), "{thread}");
    assert_eq!(mine["voice"], true, "{mine}");
}

/// #630: ⌘↵ in the lead's chat, as in a session's. With the lead at work, a plain send is kept in Cue for its turn's
/// end; one sent now goes at once and stops the turn it's on, so it reads it next.
#[test]
fn a_message_sent_now_to_a_lead_at_work_goes_at_once() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    crate::ipc::desk_send_with(w.hub(), &project, "look at the board").unwrap();
    w.hosted_says(&l, crate::hosted::HostedEvent::TurnStarted);
    assert_eq!(w.snapshot().state(&l), "working");
    let before = sent_to(&w, &l).len();
    crate::ipc::desk_send_now(w.hub(), &project, "and the footer", &[], false).unwrap();
    assert_eq!(sent_to(&w, &l).len(), before, "kept for its turn's end");
    assert!(w.snapshot().session(&l)["held"]["text"].as_str().unwrap_or("").ends_with("and the footer"));
    w.hub().unhold(&l);
    crate::ipc::desk_send_now(w.hub(), &project, "stop, the footer is fine", &[], true).unwrap();
    let sent = sent_to(&w, &l);
    assert_eq!(sent.len(), before + 1, "handed over now");
    assert!(sent.last().unwrap().starts_with("<desk") && sent.last().unwrap().ends_with("stop, the footer is fine"), "{sent:?}");
    assert_eq!(w.hosted.interrupted(), [l.clone()], "the turn it was on is stopped");
    assert!(w.snapshot().session(&l)["held"].is_null());
}

#[test]
fn the_lead_before_is_kept_for_show_earlier_and_a_failed_change_leaves_no_trace() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    assert!(w.hub().desk_lead_before(&project, &l).is_null());
    // It hands off: its note kept, the one carrying on runs the Desk.
    crate::db::handoff_noted(&l, "cue", "HANDOFF-lead.md", 5, true);
    crate::db::handoff_continued(&l, "next-lead");
    w.hub().desk_put_lead(&project, "next-lead", "desk-lead-14");
    let m = w.hub().desk_lead_before(&project, "next-lead");
    assert_eq!((m["sid"].as_str(), m["name"].as_str(), m["handoff"].as_bool(), m["note"].as_str()), (Some(l.as_str()), Some(format!("{project}-lead").as_str()), Some(true), Some("HANDOFF-lead.md")));
    assert!(m["at_ms"].as_u64().unwrap() > 0 && m["count"].as_u64().is_some(), "{m}");
    // A change to a lead that never came up, put back: the lead put back still has the same one before it.
    let before = w.hub().desk_lead(&project);
    w.hub().desk_put_lead(&project, "never-up", "ghost");
    w.hub().desk_put_back_lead(&project, before);
    assert_eq!(w.hub().desk_lead_before(&project, "next-lead")["sid"], l.as_str());
    // A lead picked by hand (no hand-off) is an earlier one all the same, said so.
    w.hub().desk_put_lead(&project, "picked", "desk-lead-15");
    let m = w.hub().desk_lead_before(&project, "picked");
    assert_eq!((m["sid"].as_str(), m["name"].as_str(), m["handoff"].as_bool()), (Some("next-lead"), Some("desk-lead-14"), Some(false)));
}

#[test]
fn a_lead_from_before_cue_kept_earlier_leads_is_found_by_its_hand_off() {
    let _w = World::new();
    crate::db::handoff_sent("old-lead", "cue", false, 0);
    crate::db::handoff_continued("old-lead", "now-lead");
    let (sid, name, _, handoff) = crate::db::desk_lead_before("some-desk", "now-lead").unwrap();
    assert_eq!((sid.as_str(), name.as_str(), handoff), ("old-lead", "", true));
}
