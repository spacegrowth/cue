//! Commands, closing, hand-offs and successors.

use super::*;

#[test]
fn a_command_here_says_what_it_printed_or_its_last_complaint() {
    let ok = Cmd { bin: "/bin/sh".into(), args: vec!["-c".into(), "echo done".into()], machine: String::new() };
    assert_eq!(ok.run().unwrap().trim(), "done");
    let bad = Cmd { bin: "/bin/sh".into(), args: vec!["-c".into(), "echo first >&2; echo no such session: x >&2; exit 1".into()], machine: String::new() };
    assert_eq!(bad.run().unwrap_err(), "no such session: x");
}

#[test]
fn closing_a_stalled_executor_runs_relays_close_where_it_runs() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    // A stalled executor that never got a Claude session (it sat at the trust prompt) is still listed.
    let out = r#"BIN:/r/relay
{"leads": [{"session_id": "ML", "project": "p"}],
 "executors": [{"agent": "relay-executor", "session_id": "stuck-1", "topic": "stuck", "status": "stalled", "owner_lead": "ML"},
               {"agent": "relay-executor", "session_id": "busy-1", "status": "busy", "owner_lead": "ML"},
               {"agent": "relay-executor", "session_id": "gone-1", "topic": "gone", "status": "dead", "owner_lead": "ML", "claude_session": "MC9"}]}
__CUE_LIVE__
ML
"#;
    let (bin, index) = parse_remote(out, "box");
    assert!(index.contains_key("relay:stuck-1"), "no Claude session: filed under relay's id");
    assert!(!index.contains_key("relay:busy-1"), "only a stalled or dead one gets that");
    *crate::locking::lock(&REMOTE, "REMOTE") = Some(HashMap::from([("box".to_string(), (bin, index.clone()))]));
    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
    let close = Cmd { bin: "/r/relay".into(), args: vec!["close".into(), "stuck-1".into()], machine: "box".into() };
    assert_eq!(act("relay:stuck-1", "close"), Ok(Act::Run(close, "Closed".into())));
    assert_eq!(act("MC9", "close"), Ok(Act::Run(Cmd { bin: "/r/relay".into(), args: vec!["close".into(), "gone-1".into()], machine: "box".into() }, "Closed".into())));
    assert!(act("ML", "close").is_err(), "a lead isn't closed that way");
    let v = view(&HashSet::from(["ML".to_string()]));
    let rows: Vec<(&str, &str)> = v["ML"]["executors"].as_array().unwrap().iter().map(|e| (e["session_id"].as_str().unwrap(), e["status"].as_str().unwrap())).collect();
    assert!(rows.contains(&("relay:stuck-1", "stalled")), "its row stays under the lead: {rows:?}");
    assert!(rows.contains(&("MC9", "dead")), "a dead one too, with its Claude session: {rows:?}");
    assert!(!rows.iter().any(|r| r.0 == "relay:busy-1"));
    // Closed, relay's record says so on the next listing, and the row goes.
    let after = parse_remote(&out.replace("\"stalled\"", "\"closed\""), "box").1;
    assert!(!view_rows(&after, "ML").iter().any(|s| s == "relay:stuck-1"));
}

#[test]
fn a_new_lead_is_a_successor_once_and_only_when_it_just_started() {
    let lead = |predecessor: &str, started_ms: u64| Member { role: "lead".into(), predecessor: predecessor.into(), started_ms, ..Default::default() };
    let now = 10_000_000_000;
    let index = HashMap::from([
        ("NEW".to_string(), lead("OLD", now - 60_000)),
        ("ANCIENT".to_string(), lead("EARLIER", now - 2 * FRESH_HANDOFF_MS)),
        ("UNDATED".to_string(), lead("BEFORE", 0)),
        ("FIRST".to_string(), lead("", now)),
        ("EXEC".to_string(), Member { role: "executor".into(), predecessor: "X".into(), started_ms: now, ..Default::default() }),
    ]);
    let mut seen = HashSet::new();
    assert_eq!(new_successors(&index, &mut seen, now), vec![("OLD".to_string(), "NEW".to_string())], "relay successor: just the fresh hand-off");
    assert!(seen.contains(&("EARLIER".to_string(), "ANCIENT".to_string())), "an old one is noted, so it's never carried later");
    assert!(new_successors(&index, &mut seen, now).is_empty(), "each link once");
}

fn view_rows(index: &HashMap<String, Member>, lead: &str) -> Vec<String> {
    index.iter().filter(|(_, e)| e.role == "executor" && e.lead == lead && !GONE.contains(&e.status.as_str())).map(|(sid, _)| sid.clone()).collect()
}

#[test]
fn closing_a_relay_lead_closes_its_open_executors_then_steps_it_down() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let out = r#"BIN:/r/relay
{"leads": [{"session_id": "EMPTY", "project": "a"}, {"session_id": "BUSY", "project": "b"}],
 "executors": [{"agent": "relay-executor", "session_id": "done-1", "status": "closed", "owner_lead": "EMPTY", "claude_session": "X1"},
               {"agent": "relay-executor", "session_id": "work-1", "topic": "work", "status": "busy", "owner_lead": "BUSY", "claude_session": "X2"},
               {"agent": "relay-executor", "session_id": "rep-1", "topic": "rep", "status": "reported", "owner_lead": "BUSY", "claude_session": "X3"}]}
__CUE_LIVE__
EMPTY
BUSY
"#;
    let (bin, mut index) = parse_remote(out, "box");
    index.insert("PL".into(), Member { role: "lead".into(), plugin: "pilead".into(), ..Default::default() });
    *crate::locking::lock(&REMOTE, "REMOTE") = Some(HashMap::from([("box".to_string(), (bin, index.clone()))]));
    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
    let on_box = |args: &[&str]| Cmd { bin: "/r/relay".into(), args: args.iter().map(|a| a.to_string()).collect(), machine: "box".into() };
    assert_eq!(lead_close("EMPTY"), Some(Ok(LeadClose { executors: vec![], stop: on_box(&["close", "--self", "EMPTY"]) })), "run on its machine; a closed executor is left alone");
    let mut busy = lead_close("BUSY").unwrap().unwrap();
    busy.executors.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(busy.executors, vec![("X2".into(), "work".into(), on_box(&["close", "work-1"])), ("X3".into(), "rep".into(), on_box(&["close", "rep-1"]))], "every open executor, busy or reported");
    assert_eq!(busy.stop, on_box(&["close", "--self", "BUSY"]));
    assert_eq!(lead_close("PL"), None, "pi-lead leads are left as they are");
    assert_eq!(lead_close("X1"), None, "an executor is closed by close_plan");
    assert_eq!(lead_close("nobody"), None);
    assert_eq!(close_plan("EMPTY"), None, "close_plan itself is unchanged for it");
    assert!(close_plan("BUSY").unwrap().is_err(), "and still refuses one with executors (Park, Resume in Cue)");
}

#[test]
fn a_lead_that_took_over_names_the_one_before_it_while_that_one_is_open() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let out = r#"BIN:/r/relay
{"leads": [{"session_id": "NEW", "project": "cue-lead", "started": "2026-10-12T10:00:00", "predecessor": {"session_id": "OLD", "tab_label": "[Lead] cue-lead"}},
           {"session_id": "AGAIN", "project": "x", "predecessor": {"session_id": "STILL"}}, {"session_id": "STILL", "project": "y"}],
 "executors": [{"agent": "relay-executor", "session_id": "e-1", "status": "busy", "owner_lead": "NEW", "claude_session": "E1"}]}
__CUE_LIVE__
NEW
__CUE_TZ__:+0000
"#;
    let (_, index) = parse_remote(out, "box");
    assert_eq!(index["NEW"].predecessor, "OLD");
    assert_eq!(index["NEW"].started_ms, epoch_secs("2026-10-12T10:00:00", Some(0)).unwrap() * 1000);
    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
    let known: HashSet<String> = ["OLD", "NEW", "STILL", "AGAIN"].iter().map(|s| s.to_string()).collect();
    let h = handoffs(&known);
    assert_eq!(h["OLD"]["to"], "NEW");
    assert_eq!(h["OLD"]["name"], "cue-lead");
    assert_eq!(h["OLD"]["executors"], 1);
    assert_eq!(h["OLD"]["machine"], "box");
    assert!(h.get("STILL").is_none(), "one that leads (again) isn't a handed-off lead");
    assert!(handoffs(&HashSet::new()).as_object().unwrap().is_empty(), "only sessions Cue knows");
}

#[test]
fn a_handed_off_lead_points_at_the_last_lead_of_its_chain_after_relay_forgot_them() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("cue-handoff-chain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // relay's markers for B and C are gone (stepped down, ended): only the ledger knows. A line still
    // being written is left for later.
    std::fs::write(dir.join("sessions.jsonl"), concat!(
        r#"{"ts": "2026-10-10T12:21:17", "event": "lead_handoff", "from_lead": "A", "to_lead": "B", "project": "cri"}"#, "\n",
        r#"{"ts": "2026-10-10T13:00:00", "event": "lead_started", "session_id": "B"}"#, "\n",
        r#"{"ts": "2026-10-10T22:22:29", "event": "lead_handoff", "from_lead": "B", "to_lead": "C", "project": "cri"}"#, "\n",
        r#"{"ts": "2026-10-11T09:00:00", "event": "lead_handoff", "from_lead": "C", "to_lead": "D""#,
    )).unwrap();
    std::env::set_var("RELAY_STATE_ROOT", &dir);
    *crate::locking::lock(&INDEX, "INDEX") = Some(HashMap::new());
    let known: HashSet<String> = ["A", "B"].iter().map(|s| s.to_string()).collect();
    let h = handoffs(&known);
    std::env::remove_var("RELAY_STATE_ROOT");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(h["A"]["to"], "C", "the work is with the last lead: {h}");
    assert_eq!(h["A"]["hops"], 2);
    assert_eq!(h["A"]["name"], "cri");
    assert_eq!(h["A"]["at_ms"], epoch_secs("2026-10-10T12:21:17", None).unwrap() * 1000, "when A handed off");
    assert_eq!(h["B"]["to"], "C");
    assert!(h.get("C").is_none(), "C's handoff isn't finished in the ledger yet, and Cue doesn't know C");
}

#[test]
fn executors_whose_lead_is_gone_are_closed_or_handed_on_together() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let out = r#"BIN:/r/relay
{"leads": [{"session_id": "LIVE", "project": "p"}],
 "executors": [{"agent": "relay-executor", "session_id": "a-1", "topic": "a", "status": "busy", "owner_lead": "GONE", "claude_session": "A1"},
               {"agent": "relay-executor", "session_id": "a-2", "topic": "b", "status": "reported", "owner_lead": "GONE", "claude_session": "A2"},
               {"agent": "relay-executor", "session_id": "a-3", "status": "closed", "owner_lead": "GONE", "claude_session": "A3"},
               {"agent": "relay-executor", "session_id": "o-1", "status": "busy", "owner_lead": "OTHER", "claude_session": "O1"},
               {"agent": "relay-executor", "session_id": "l-1", "status": "busy", "owner_lead": "LIVE", "claude_session": "L1"}]}
__CUE_LIVE__
LIVE
"#;
    let (bin, mut index) = parse_remote(out, "box");
    index.insert("HERE".into(), Member { role: "lead".into(), plugin: "relay".into(), ..Default::default() });
    *crate::locking::lock(&REMOTE, "REMOTE") = Some(HashMap::from([("box".to_string(), (bin, index.clone()))]));
    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
    let mut left = close_left_behind("A1").unwrap();
    left.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(left.iter().map(|(sid, _, c)| (sid.as_str(), c.args.join(" "))).collect::<Vec<_>>(), [("A1", "close a-1".to_string()), ("A2", "close a-2".to_string())], "the same gone lead's, still open; not another lead's");
    assert!(close_left_behind("L1").is_err(), "its lead is running");
    let mut hand = hand_left_behind("A2", "LIVE").unwrap();
    hand.sort_by(|a, b| a.args.cmp(&b.args));
    assert_eq!(hand[0], Cmd { bin: "/usr/bin/env".into(), args: vec!["CLAUDE_CODE_SESSION_ID=LIVE".into(), "/r/relay".into(), "adopt".into(), "a-1".into()], machine: "box".into() });
    assert_eq!(hand.len(), 2);
    assert!(hand_left_behind("A1", "HERE").unwrap_err().contains("another machine"));
    assert!(hand_left_behind("A1", "A2").is_err(), "only to a lead");
}
