//! Approval history must preserve which Desk item a session is working on.

use crate::hosted::{Ask, AskKind, HostedEvent};
use crate::testing::{until, World};
use serde_json::{json, Value};

fn asks(w: &World, sid: &str, request: &str) -> String {
    w.hosted_says(sid, HostedEvent::Ask(Ask {
        request_id: request.into(), kind: AskKind::Permission, tool_name: "Bash".into(),
        input: json!({ "command": "printf approval-check" }), suggestions: json!([]),
        agent_id: String::new(), tool_use_id: request.into(), server: String::new(), message: String::new(), form: Value::Null,
    }));
    w.snapshot().cards_of(sid).iter().find(|c| c["kind"] == "permission").unwrap()["id"].as_str().unwrap().into()
}

#[test]
fn a_second_codex_approval_stays_visible_on_the_item_after_allow() {
    let w = World::new();
    let opts = crate::ipc::StartOpts { hosted: true, ..Default::default() };
    let started = crate::ipc::start_session_with(w.hub(), "codex", &w.home().to_string_lossy(), "hello", "worker", &opts).unwrap();
    let sid = started["session_id"].as_str().unwrap();
    w.hosted_says(sid, HostedEvent::TurnEnded { text: "Ready".into(), failed: false, cost_usd: 0.0, unprompted: false });
    let project = w.snapshot().session(sid)["project"].as_str().unwrap().to_string();
    let item = w.hub().desk_add(&project, "bug", "Approval delivery", "", None).unwrap();
    w.hub().desk_give(&item.id, sid).unwrap();
    w.settle();
    assert_eq!(super::desk::desk(&w, &project)["items"][0]["now_on"], true);
    let first = asks(&w, sid, "first");
    w.answer(&first, json!({ "behavior": "allow" }));
    until("first approval delivered", || w.hosted.answers().len() == 1);
    let second = asks(&w, sid, "second");
    let desk = super::desk::desk(&w, &project);
    let shown = desk["items"].as_array().unwrap().iter().find(|i| i["id"] == item.id).unwrap();
    assert_eq!(shown["state"], "needs_you");
    assert_eq!(shown["ask"]["item_id"], second);
    w.answer(&second, json!({ "behavior": "allow" }));
    until("second approval delivered", || w.hosted.answers().len() == 2);
    let desk = super::desk::desk(&w, &project);
    assert_eq!(desk["items"][0]["state"], "working");
    assert!(desk["items"][0]["ask"].is_null());
}
