//! An item whose holder waits on its own crew (#511): at work, never waiting on you, unless it asks you something.

use super::desk::{desk, ends, holder, item, new_lead};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::{json, Value};

/// #498's first turn, word for word: five teammates started, one bullet naming Cue's "Waiting on you" list.
const STARTED: &str = r#"I've started five teammates, one per area. Each writes tests from what the Desk is meant to do (not from the code), fixes what the tests catch, and writes notes for the gap list:
- **routing-tests**: who takes an item, the lead's lines and Cue's refusals, hand-offs
- **queue-tests**: items queued behind other items, items waiting on each other, Start now / Put before
- **landing-tests**: each item's copy, landing (merge, check, install, failures), to check / done / send back
- **asks-notes-tests**: notes, quick phrases, Waiting on you and asks, what Cue says to sessions and the lead
- **desk-ui-tests**: Esc / send / pull-back in Desk chats, quick phrases in the window, the Waiting on you list

DECIDED (minor): one teammate per area, each in its own copy. I'll bring their work into #498's copy, run the whole Desk suite there and write docs/desk-test-gaps.md.

I'll report back when they finish."#;

/// A Desk with item "Desk tests" routed to a session, which took it and started `n` teammates in the background.
fn with_crew(n: usize) -> (World, String, String) {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: Desk tests\nROUTE: #new -> cue-teams | why: it knows");
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::Taken { text: "Desk item".into() });
    for k in 0..n {
        let call = format!("toolu_{k}");
        w.hosted_says(&h, HostedEvent::ToolCall { id: call.clone(), name: "Agent".into(), input: json!({ "name": format!("mate{k}"), "description": "its part", "prompt": "go", "run_in_background": true, "isolation": "worktree" }), agent: None });
        w.hosted_says(&h, HostedEvent::Task { what: "task_started".into(), detail: json!({ "subtype": "task_started", "tool_use_id": call, "task_id": format!("agent{k}"), "task_type": "local_agent" }) });
    }
    w.settle();
    w.advance(1000);
    (w, h, project)
}

fn shown(w: &World, project: &str) -> Value {
    item(&desk(w, project), "Desk tests").clone()
}

/// #498's case: its holder's turn ends with five teammates at work, then a turn of its own ends on the same; idle,
/// stored working. At work on it, saying so, through half an hour of them working, and nothing waits on you.
#[test]
fn an_item_whose_holder_waits_on_its_teammates_is_at_work() {
    let (w, h, project) = with_crew(5);
    ends(&w, &h, STARTED);
    w.hosted_says(&h, HostedEvent::TurnStarted);
    w.settle();
    w.advance(1000);
    w.hosted_says(&h, HostedEvent::TurnEnded { text: "The baseline build of #498's copy is done. The five teammates are still running; I'll merge and run the full Desk suite once they report.".into(), failed: false, cost_usd: 0.0, unprompted: true });
    w.settle();
    for _ in 0..30 {
        assert_eq!(w.snapshot().session(&h)["state"], "waiting");
        let i = shown(&w, &project);
        assert_eq!(i["state"], "working", "{i}");
        assert_eq!(i["latest"], "5 teammates running");
        assert!(i["asks"].is_null() && i["ask"].is_null() && i["stopped"] != true, "{i}");
        w.advance(60_000);
        w.settle();
    }
}

/// Its teammates at work, but its turn asks you something: waiting on you with that.
#[test]
fn a_question_with_teammates_at_work_waits_on_you() {
    let (w, h, project) = with_crew(2);
    ends(&w, &h, "Two teammates are on it.\n\nShould the queue tests cover Put before too?");
    let i = shown(&w, &project);
    assert_eq!(i["state"], "needs_you", "{i}");
    assert_eq!(i["latest"], "Should the queue tests cover Put before too?");
}

/// No teammates and no DONE: a stop, the lead's to settle as before, not at work on teammates and not waiting on you.
#[test]
fn a_holder_that_stopped_with_no_teammates_is_stopped() {
    let (w, h, project) = with_crew(0);
    ends(&w, &h, "I've looked at the tests. I'll report back when they finish.");
    let i = shown(&w, &project);
    assert_eq!(i["state"], "working", "{i}");
    assert_eq!(i["stopped"], true, "{i}");
    assert_ne!(i["latest"], "0 teammates running");
}
