//! The browser UI harness's fixtures (tests/ui/scenarios/world-<name>.json), each the window's state as a
//! World builds it: the real snapshot's shape, so a UI test never runs on a state the hub can't make.
//! Written on demand, not by every test run:
//!   cargo test --no-default-features write_ui_scenarios -- --ignored
//! The file format is in tests/ui/scenarios/README.md.

use super::{Takes, World};
use serde_json::{json, Value};

/// Each scenario: its name and how the World gets there.
const SCENARIOS: &[(&str, fn(&World))] = &[
    ("empty", |_| {}),
    ("one-ask", one_ask),
    ("many-sessions", many_sessions),
    ("status-list", status_list),
    ("parked", parked),
    ("restarting", restarting),
    ("stalled-terminal", stalled_terminal),
    ("long-names", long_names),
    ("fifty-sessions", fifty_sessions),
    ("handoff-pending", handoff_pending),
    ("settings", settings),
    ("four-choices", four_choices),
    ("team", team),
];

fn one_ask(w: &World) {
    let a = w.session("api").in_project("api-server");
    a.prompt("Clean the build folder and rebuild");
    a.ask_permission("Bash", json!({"command": "rm -rf build && cargo build", "description": "Clean and rebuild"}));
}

fn four_choices(w: &World) {
    let a = w.session("pipe").in_project("data-pipeline");
    a.prompt("Backfill March");
    a.ask_question("Which source wins on a clash?", &["Vendor", "Exchange", "Newest", "Ask me each time"]);
}

fn many_sessions(w: &World) {
    w.session("web").in_project("web-app").prompt("Make the login page faster").stop("Done: the bundle is 40% smaller.");
    w.session("eng").in_project("order-engine").prompt("Refactor the order router");
    w.session("pipe").in_project("data-pipeline").prompt("Backfill March").ask_question("Which source wins on a clash?", &["Vendor", "Exchange"]);
    let lim = w.session("bot").in_project("filings-bot");
    lim.prompt("Parse every 10-K").failed("rate_limit", "You've hit your session limit · resets 4:10am (America/Los_Angeles)");
    w.session("helper").in_project("order-engine").prompt("task 2").stop_driven("Report written.", "its lead (eng)");
    w.session("pi").pi().in_project("notes").prompt("Summarise today").stop("Summary in notes/today.md");
    w.session("cx").codex().in_project("api-server").prompt("Port the handler").stop("Ported; tests pass.");
}

/// A hosted lead with a team of five, one in each state the chat shows (docs/dev/cue-teams.md §4): hub asks you to
/// run a command, rows is working, skill is done, handoff is reviewed and held (it shares ui/core.js with rows), docs
/// is reviewed and free to commit; and one that left the team (set aside).
fn team(w: &World) {
    use crate::hosted::{Ask, AskKind, HostedEvent as E};
    let lead = w.session("lead").hosted().in_project("cue");
    lead.prompt("Build the team view: hand out hub, rows, skill and handoff");
    let says = |e: E| w.hosted_says("lead", e);
    let task = |what: &str, call: &str, id: &str| E::Task { what: what.into(), detail: json!({ "subtype": what, "tool_use_id": call, "task_id": id, "task_type": "local_agent", "status": "completed" }) };
    let start = |name: &str, what: &str| {
        let call = format!("call-{name}");
        says(E::ToolCall { id: call.clone(), name: "Agent".into(), input: json!({ "name": name, "description": what, "prompt": format!("{what}. Don't commit; finish with a summary."), "run_in_background": true }), agent: None });
        says(task("task_started", &call, &format!("id-{name}")));
    };
    let edit = |name: &str, file: &str| {
        says(E::ToolCall { id: format!("{name}-{file}"), name: "Edit".into(), input: json!({ "file_path": format!("/work/cue/{file}") }), agent: Some(format!("call-{name}")) });
        says(E::ToolResult { id: format!("{name}-{file}"), output: json!("ok"), failed: false, agent: Some(format!("call-{name}")) });
    };
    let finish = |name: &str, summary: &str| says(E::Task { what: "task_notification".into(), detail: json!({ "tool_use_id": format!("call-{name}"), "task_id": format!("id-{name}"), "status": "completed", "summary": summary }) });
    for (name, what) in [("hub", "Keep a team per hosted lead"), ("rows", "Draw the teammate rows"), ("skill", "Write the team skill"), ("handoff", "Carry the team over a handoff"), ("docs", "Document teams"), ("spike", "Try a drag-to-hand-out")] {
        start(name, what);
    }
    edit("handoff", "src-tauri/src/hub/handoff.rs");
    edit("handoff", "ui/core.js");
    finish("handoff", "Handoff carries the team; 9 tests pass");
    edit("docs", "docs/dev/cue-teams.md");
    finish("docs", "Teams documented");
    edit("skill", "skills/team/SKILL.md");
    edit("skill", "skills/team/review.md");
    finish("skill", "Team skill written, 12 tests pass");
    finish("spike", "Tried it; not worth it");
    edit("rows", "ui/team.js");
    edit("rows", "ui/core.js");
    says(E::ToolCall { id: "rows-run".into(), name: "Bash".into(), input: json!({ "command": "node tests/team.test.js" }), agent: Some("call-rows".into()) });
    says(E::TurnEnded { text: "Handed hub, rows, skill and handoff out. skill is done and waits for your review.".into(), failed: false, cost_usd: 0.0, unprompted: false });
    // handoff reviewed: Review typed into the free lead, its reply the review.
    w.hub().team_action("lead", "id-handoff", "review").unwrap();
    says(E::TurnEnded { text: "1. hub/handoff.rs:40 — should-fix — the team list is cut at 8 teammates without saying so.\n2. ui/core.js:212 — note — also changed by rows; commit after rows lands.\nRecommendation: commit".into(), failed: false, cost_usd: 0.0, unprompted: false });
    // The lead takes a moment to show as working after a review is typed in (reviews.rs REVIEW_SETTLE_MS).
    w.advance(10_000);
    w.hub().team_action("lead", "id-docs", "review").unwrap();
    says(E::TurnEnded { text: "1. docs/dev/cue-teams.md:12 — note — reads well.\nRecommendation: commit".into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.hub().team_action("lead", "id-spike", "aside").unwrap();
    // hub asks to run the tests.
    says(E::ToolCall { id: "hub-test".into(), name: "Bash".into(), input: json!({ "command": "cd src-tauri && cargo test --features ext" }), agent: Some("call-hub".into()) });
    says(E::Ask(Ask { request_id: "r-hub".into(), kind: AskKind::Permission, tool_name: "Bash".into(), input: json!({ "command": "cd src-tauri && cargo test --features ext", "description": "Run the tests" }), suggestions: json!([]), agent_id: "id-hub".into(), tool_use_id: "hub-test".into(), server: String::new(), message: String::new(), form: Value::Null }));
    w.settle();
}

fn status_list(w: &World) {
    let a = w.session("board").in_project("agent-board");
    let ask = "End this reply with the status list: **Where we are**";
    a.prompt("Ship the board");
    a.transcript().prompt_with_status_ask("u1", "Ship the board", ask);
    a.stop("Board shipped.\n\n**Where we are**\n✓ Spend column\n→ Footer totals\n○ Docs\nWaiting on you: nothing");
}

fn parked(w: &World) {
    let a = w.session("old").in_project("backtester");
    a.prompt("Try the slower fill model").stop("Done; it's 3% worse.");
    // As Park leaves it: its finished turn filed as parked, its record under Parked.
    let record = w.hub().session_record("old").expect("its record");
    let cards = w.hub().take_cards("old");
    w.hub().file_taken(cards, "gone", "parked");
    w.hub().add_parked(record);
    w.session("gone").in_project("web-app").prompt("go").ended();
}

fn restarting(w: &World) {
    w.session("mcp").in_project("api-server").prompt("Use the new MCP server").stop("Ready.");
    w.hub().restart_begin("mcp", Some(vec!["github".into()]), true);
    w.hub().restart_step("mcp", "start");
}

fn stalled_terminal(w: &World) {
    let a = w.session("stuck").in_project("data-pipeline").in_tmux("%7");
    a.prompt("Fetch the feed").stop("Fetched.");
    w.terminal.pane("stuck", |p| p.takes = Takes::Nothing);
    w.send_to("stuck", "now run the import");
    w.session("net").in_project("web-app").prompt("Download the fixtures").terminal_ask("Claude needs your permission to use network access");
}

fn long_names(w: &World) {
    let name = "an-unusually-long-project-folder-name-that-will-not-fit-anywhere";
    let a = w.session("long").in_project(name);
    a.prompt(&"Please read every file under src and explain, in detail, ".repeat(6));
    a.stop(&"This is a long answer that keeps going well past the width of a card. ".repeat(12));
    w.hub().rename("long", "a session name long enough to be cut short in every row it shows in").unwrap();
}

fn fifty_sessions(w: &World) {
    for i in 0..50 {
        let a = w.session(&format!("s{i:02}")).in_project(&format!("project-{}", i % 7));
        a.prompt(&format!("Task {i}"));
        if i % 3 == 0 {
            a.stop(&format!("Task {i} done."));
        }
    }
}

fn handoff_pending(w: &World) {
    w.session("board").in_project("agent-board").prompt("/handoff").stop("Hand-off note written to HANDOFF.md.");
    crate::db::handoff_sent("board", "handoff", true, 0);
}

fn settings(w: &World) {
    w.hub().set_setting("notify.finished", json!(false)).unwrap();
    w.hub().set_setting("history.keep", json!(50)).unwrap();
    w.session("web").in_project("web-app").prompt("go").stop("Done.");
}

/// The fixtures' clock: the same instant the UI harness records its own scenarios at (tests/ui/record.mjs).
const AT_MS: u64 = 1_793_984_400_000;

/// Where the World keeps its sessions' transcripts, as they'd be on a Mac.
const TRANSCRIPTS: &str = "/Users/me/.claude/projects/-work";

/// Facts the snapshot reads from this Mac rather than from the hub (macOS's Login Items, whether tmux is
/// installed and so whether sessions run in it, which terminal app): pinned, so the fixtures are the same whoever writes them.
fn pin_machine_facts(state: &mut Value) {
    let s = &mut state["settings"];
    s["login"] = json!({"open": false});
    s["sessions"]["tmux_installed"] = json!(true);
    s["sessions"]["tmux"] = json!(true);
    s["sessions"]["terminal"] = json!("Terminal");
    state["usage_file"] = Value::Null;
}

/// The sessions, and the keys of what the hub keeps in hash maps, come out in no set order (the window
/// sorts what it shows itself): by id and by key here, so the same scenario writes the same file.
fn in_a_fixed_order(state: &mut Value) {
    if let Some(rows) = state["sessions"].as_array_mut() {
        rows.sort_by(|a, b| a["session_id"].as_str().cmp(&b["session_id"].as_str()));
    }
    sort_keys(state);
}

fn sort_keys(v: &mut Value) {
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, mut child) in entries {
                sort_keys(&mut child);
                map.insert(k, child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(sort_keys),
        _ => {}
    }
}

/// How long a scenario's agents take to answer: a card comes back a while after your message, as it does.
const THINKS_MS: u64 = 20_000;

/// The World `build` makes, its agents thinking THINKS_MS before each answer, and ending at AT_MS: it's built once
/// to see how long it takes, then again from that long before AT_MS.
fn scene(build: fn(&World)) -> World {
    let took = {
        let w = World::at(AT_MS);
        w.agents_think(THINKS_MS);
        build(&w);
        crate::clock::now_ms() - AT_MS
    };
    let w = World::at(AT_MS - took);
    w.agents_think(THINKS_MS);
    build(&w);
    assert_eq!(crate::clock::now_ms(), AT_MS, "the same scenario takes the same time");
    w
}

#[test]
#[ignore = "writes tests/ui/scenarios/world-*.json: run on demand"]
fn write_ui_scenarios() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/ui/scenarios");
    for (name, build) in SCENARIOS {
        let w = scene(*build);
        let mut state = w.snapshot().0;
        pin_machine_facts(&mut state);
        in_a_fixed_order(&mut state);
        let text = serde_json::to_string_pretty(&state).unwrap().replace(&*w.home().join("claude").to_string_lossy(), TRANSCRIPTS);
        std::fs::write(dir.join(format!("world-{name}.json")), text + "\n").unwrap();
    }
}

/// Every scenario builds (the writer isn't run by `cargo test`, so its builders are checked here).
#[test]
fn every_ui_scenario_builds() {
    for (name, build) in SCENARIOS {
        let w = World::new();
        build(&w);
        let state = w.snapshot().0;
        assert!(state["sessions"].is_array(), "{name}");
    }
}
