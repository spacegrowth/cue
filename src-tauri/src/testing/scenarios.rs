//! The browser UI harness's fixtures (tests/ui/scenarios/world-<name>.json), each the window's state as a
//! World builds it: the real snapshot's shape, so a UI test never runs on a state the hub can't make.
//! Written on demand, not by every test run:
//!   cargo test --no-default-features write_ui_scenarios -- --ignored
//! The file format is in tests/ui/scenarios/README.md.

use super::{Takes, World};
use crate::leads::{install_index, reported_exec, Member};
use serde_json::{json, Value};

/// Each scenario: its name and how the World gets there.
const SCENARIOS: &[(&str, fn(&World))] = &[
    ("empty", |_| {}),
    ("one-ask", one_ask),
    ("many-sessions", many_sessions),
    ("relay-review", relay_review),
    ("status-list", status_list),
    ("parked", parked),
    ("restarting", restarting),
    ("stalled-terminal", stalled_terminal),
    ("long-names", long_names),
    ("fifty-sessions", fifty_sessions),
    ("handoff-pending", handoff_pending),
    ("settings", settings),
    ("four-choices", four_choices),
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
    w.session("helper").in_project("order-engine").prompt("packet 2").stop_driven("Report written.", "its relay lead (eng)");
    w.session("pi").pi().in_project("notes").prompt("Summarise today").stop("Summary in notes/today.md");
    w.session("cx").codex().in_project("api-server").prompt("Port the handler").stop("Ported; tests pass.");
}

fn relay_review(w: &World) {
    let lead = Member { role: "lead".into(), plugin: "relay".into(), name: "order-engine".into(), ..Default::default() };
    let exec = |name: &str, packet: u64| Member { name: name.into(), id: format!("relay-{name}"), packet: Some(packet), goal: format!("{name}: the packet's goal"), outcome: "Done and staged.".into(), lead: "lead".into(), lead_name: "order-engine".into(), updated: format!("2026-11-01T10:00:0{packet}"), ..reported_exec() };
    install_index([("lead".to_string(), lead), ("ex-fills".to_string(), exec("fills", 3)), ("ex-docs".to_string(), exec("docs", 1))].into());
    let l = w.session("lead").in_project("order-engine");
    l.prompt("Lead the router refactor");
    w.session("ex-fills").in_project("order-engine").prompt("packet 3").stop_driven("Packet 3 done.", "its relay lead (order-engine)");
    w.session("ex-docs").in_project("order-engine").prompt("packet 1").stop_driven("Docs done.", "its relay lead (order-engine)");
    // The lead is working: the first review waits in its queue.
    w.hub().review("lead", "ex-fills", "/relay:review fills").unwrap();
}

fn status_list(w: &World) {
    let a = w.session("relay").in_project("agent-relay");
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
    w.session("relay").in_project("agent-relay").prompt("/handoff").stop("Hand-off note written to HANDOFF.md.");
    crate::db::handoff_sent("relay", "handoff", true, 0);
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

#[test]
#[ignore = "writes tests/ui/scenarios/world-*.json: run on demand"]
fn write_ui_scenarios() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/ui/scenarios");
    for (name, build) in SCENARIOS {
        let w = World::at(AT_MS);
        build(&w);
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
