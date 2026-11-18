//! Ask me before (one_way.rs, hub/guard.rs): asked once as a project's Desk gets a lead, an earlier One-way setting
//! carried over, sent to every session on the Desk, and each ticked step checked for real: the session's hook (or a
//! hosted Pi's extension) waits on a card.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::hosted::{Answer, Ask, AskKind, HostedEvent};
use crate::testing::{until, World};
use serde_json::{json, Value};

fn ask_before(w: &World, project: &str) -> Value {
    desk(w, project)["ask_before"].clone()
}

#[test]
fn the_question_is_asked_once_and_an_earlier_one_way_setting_carries_over() {
    let w = World::new();
    let (h, project) = holder(&w);
    // An earlier One-way setting is what the question starts from, once the Desk gets a lead.
    crate::config::set("project.oneway", json!({ project.as_str(): ["push", "deploy"] })).unwrap();
    let d = new_lead(&w, &project);
    let ab = ask_before(&w, &project);
    assert_eq!((ab["asked"].clone(), ab["ticks"].clone()), (json!(false), json!(["push", "deploy"])), "{ab}");
    // Your answer: saved (asked from then on), a step Cue can't check kept, and every session on the Desk told.
    let saved = w.hub().ask_before_set(&project, Some(&["main".to_string(), "deploy".to_string()])).unwrap();
    w.settle();
    assert_eq!(saved, vec!["main", "deploy"]);
    assert_eq!(ask_before(&w, &project)["asked"], json!(true));
    for sid in [&h, &d] {
        assert!(sent_to(&w, sid).iter().any(|t| t.starts_with("Ask me before, changed by the user just now") && t.contains("replaces any earlier")), "{sid}: {:?}", sent_to(&w, sid));
    }
    // An item routed carries the rule.
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let id = item(&desk(&w, &project), "Header jumps")["id"].as_str().unwrap().to_string();
    let words = sent_to(&w, &h).into_iter().find(|t| t.contains(&format!("#{id}"))).unwrap();
    assert!(words.contains("Ask me before, as the user set it for this project") && words.contains("a push to main or master"), "{words}");
}

#[test]
fn not_now_keeps_the_setting_and_never_asks_again() {
    let w = World::new();
    let (_, project) = holder(&w);
    new_lead(&w, &project);
    let ab = ask_before(&w, &project);
    assert_eq!((ab["asked"].clone(), ab["ticks"].clone()), (json!(false), json!(["main", "release", "data"])), "never set: all three ticked: {ab}");
    assert_eq!(w.hub().ask_before_set(&project, None).unwrap(), vec!["main"]);
    w.settle();
    let ab = ask_before(&w, &project);
    assert_eq!(ab["asked"], json!(true), "{ab}");
    assert_eq!(crate::one_way::kinds(&project), vec!["main"], "the default, as it was");
}

#[test]
fn each_ticked_step_waits_on_your_yes_and_only_on_a_desk() {
    let w = World::new();
    let (h, project) = holder(&w);
    let s = w.session(&h).in_folder(w.home());
    // No Desk on the project yet: nothing is checked.
    assert!(s.guard_bash("git push origin main", "main").is_none(), "no Desk: it passes");
    new_lead(&w, &project);
    // A push to main waits on a one-way card; your yes lets it run.
    let ask = s.guard_bash("git push origin HEAD:main", "crew/x").expect("a push to main waits");
    let snap = w.snapshot();
    let card = snap.cards().into_iter().find(|c| c["id"] == ask.id.as_str()).expect("its card");
    assert_eq!(card["one_way"], json!(true), "{card}");
    w.answer(&ask.id, json!({"behavior": "allow"}));
    assert_eq!(ask.told().unwrap()["behavior"], "allow");
    // A crew branch pushed isn't main; a plain push from main is.
    assert!(s.guard_bash("git push origin crew/x", "crew/x").is_none());
    assert!(s.guard_bash("git push", "main").is_some());
    // Deleting files: not in the default, asked about once ticked. Outside the project (a temp file) never.
    assert!(s.guard_bash("rm -rf src", "main").is_none(), "not ticked");
    w.hub().ask_before_set(&project, Some(&["main".into(), "release".into(), "data".into()])).unwrap();
    assert!(s.guard_bash("rm -rf src", "main").is_some());
    assert!(s.guard_bash("rm -rf /tmp/cue-scratch", "main").is_none());
    assert!(s.guard_bash("scripts/release.sh 0.2.0 \"notes\"", "main").is_some());
    assert!(s.guard_bash("cat scripts/release.sh", "main").is_none(), "reading it isn't releasing");
}

fn guard_ask(request_id: &str, command: &str, branch: &str) -> HostedEvent {
    let input = json!({ "command": command });
    HostedEvent::Ask(Ask { request_id: request_id.into(), kind: AskKind::Guard, tool_name: "bash".into(), input, suggestions: json!([]), agent_id: String::new(), tool_use_id: "t1".into(), server: String::new(), message: branch.into(), form: Value::Null })
}

fn answered(w: &World, request_id: &str) -> Option<String> {
    w.hosted.answers().into_iter().find(|(_, r, _)| r == request_id).map(|(_, _, a)| match a {
        Answer::Decision(d) => d.behavior,
        Answer::Input { .. } => "input".into(),
    })
}

#[test]
fn a_hosted_pi_is_checked_the_same_way() {
    let w = World::new();
    let (h, project) = holder(&w);
    new_lead(&w, &project);
    // Not ticked: it passes at once.
    w.hosted_says(&h, guard_ask("g1", "rm -rf src", "main"));
    w.settle();
    assert_eq!(answered(&w, "g1").as_deref(), Some("pass"));
    // Ticked: a card, and your no goes back to it.
    w.hosted_says(&h, guard_ask("g2", "git push origin main", "main"));
    w.settle();
    assert_eq!(answered(&w, "g2"), None, "waits for you");
    let snap = w.snapshot();
    let id = snap.cards_of(&h).into_iter().find(|c| c["one_way"] == json!(true)).expect("a one-way card")["id"].as_str().unwrap().to_string();
    w.answer(&id, json!({"behavior": "deny"}));
    until("the hosted guard answered", || answered(&w, "g2").is_some());
    assert_eq!(answered(&w, "g2").as_deref(), Some("deny"));
}
