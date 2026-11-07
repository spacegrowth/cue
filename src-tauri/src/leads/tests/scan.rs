//! This Mac's relay and pi-lead folders read into the index, and what's read alongside (plans, verdicts).

use super::*;

#[test]
fn a_relay_id_that_would_leave_relays_folder_is_skipped() {
    assert!(super::safe_id("finder-1"));
    for bad in ["", "../x", "a/b", "a\\b", "..", "x/../../etc"] {
        assert!(!super::safe_id(bad), "{bad:?}");
    }
    // From a machine's listing: the bad one is dropped, the good one kept.
    let out = "BIN:/r\n{\"leads\": [], \"executors\": [{\"agent\": \"relay-executor\", \"session_id\": \"../../.ssh\", \"claude_session\": \"CBAD\", \"status\": \"reported\"}, {\"agent\": \"relay-executor\", \"session_id\": \"ok-1\", \"claude_session\": \"COK\", \"status\": \"busy\"}]}\n";
    let (_, index) = parse_remote(out, "build-box");
    assert!(!index.contains_key("CBAD"));
    assert_eq!(index["COK"].id, "ok-1");
    // From relay's folder on this Mac: the same.
    let root = std::env::temp_dir().join(format!("cue-leads-badid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let write = |p: std::path::PathBuf, t: &str| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    };
    write(root.join("evil/session.json"), r#"{"session_id":"../../x","agent":"relay-executor","claude_session":"C1","status":"reported"}"#);
    write(root.join("fine/session.json"), r#"{"session_id":"fine","agent":"relay-executor","claude_session":"C2","status":"reported"}"#);
    let mut index = std::collections::HashMap::new();
    super::scan_relay(&root, &std::collections::HashSet::new(), &mut index);
    let _ = std::fs::remove_dir_all(&root);
    assert!(!index.contains_key("C1"));
    assert_eq!(index["C2"].id, "fine");
}

#[test]
fn relay_and_pilead_leads_own_their_executors() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("cue-leads-{}", std::process::id()));
    let (relay, pi, cs) = (root.join("relay"), root.join("pi"), root.join("claude-sessions"));
    // L1 is open in a live process (this test's own pid); GHOST has a marker but no process.
    write(&cs.join("1.json"), &format!(r#"{{"pid":{},"sessionId":"L1"}}"#, std::process::id()));
    write(&cs.join("2.json"), r#"{"pid":999999,"sessionId":"GHOST"}"#);
    write(&relay.join("lead/GHOST/marker.json"), r#"{"session_id":"GHOST","project":"crashed"}"#);
    write(&relay.join("ghosted-1/session.json"), r#"{"session_id":"ghosted-1","agent":"relay-executor","claude_session":"C4","owner_lead":"GHOST","topic":"ghosted","status":"reported"}"#);
    write(&relay.join("lead/L1/marker.json"), r#"{"session_id":"L1","project":"chart-lines","color":[132,180,180],"autonomous":true}"#);
    write(&relay.join("finder-1/packets/002-packet.md"), "GOAL: tune the finder from this week's corrections\n\n## Context\n…");
    write(&relay.join("finder-1/packets/002-report.md"), "\nGates rise on 3 of 4 sets; staged.\n\nStatus: clean\n");
    write(&relay.join("finder-1/session.json"), r#"{"session_id":"finder-1","agent":"relay-executor","claude_session":"C1","owner_lead":"L1","owner_project":"old-name","topic":"finder","model":"claude-sonnet-5-5","status":"reported","current_packet":2}"#);
    write(&relay.join("old-1/session.json"), r#"{"session_id":"old-1","agent":"relay-executor","claude_session":"C2","owner_lead":"L1","topic":"old","status":"closed"}"#);
    write(&relay.join("orphan-1/session.json"), r#"{"session_id":"orphan-1","agent":"relay-executor","claude_session":"C3","owner_lead":"GONE","owner_project":"claude-relay","topic":"orphan","status":"reported"}"#);
    write(&pi.join("leads/P1.json"), r#"{"sid":"P1","project":"pi-lead"}"#);
    write(&pi.join("leads/P1.inbox.md"), "");
    write(&pi.join("sessions/b12-1/meta.json"), r#"{"sid":"b12-1","lead":"P1","topic":"b12","model":"deepseek/deepseek-flash","status":"busy","packets":1}"#);
    write(&pi.join("sessions/b12-1/status"), "reported\n");

    let index = scan(&relay, &pi, &cs);
    let c1 = &index["C1"];
    assert_eq!((c1.role.as_str(), c1.name.as_str(), c1.lead_name.as_str(), c1.packet), ("executor", "finder", "chart-lines", Some(2)));
    assert_eq!(c1.color, Some([132, 180, 180]));
    assert_eq!((c1.goal.as_str(), c1.outcome.as_str()), ("tune the finder from this week's corrections", "Gates rise on 3 of 4 sets; staged."));
    assert_eq!(index["C2"].goal, "", "a closed executor's packet isn't read");
    assert_eq!(index["C3"].lead_armed, Some(false));
    assert_eq!(index["C4"].lead_armed, Some(false), "a marker with no live process is a ghost");
    assert_eq!(index["C4"].lead_name, "crashed");
    assert_eq!(index["C3"].lead_name, "claude-relay");
    assert_eq!(index["b12-1"].status, "reported", "the status file wins over meta.json");
    assert_eq!(index["b12-1"].lead_name, "pi-lead");
    assert_eq!(index["P1"].role, "lead");

    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
    assert_eq!(resolve_driver("C1", ""), "its relay lead (chart-lines)");
    assert_eq!(resolve_driver("b12-1", ""), "its pi-lead lead (pi-lead)");
    assert_eq!(resolve_driver("C3", "its relay lead (claude-relay)"), "", "no armed lead: its report is yours");
    assert_eq!(resolve_driver("L1", ""), "");
    assert_eq!(resolve_driver("unknown", "its relay lead (x)"), "its relay lead (x)", "not in the index: the hook's word stands");

    assert_eq!(act("C1", "review"), Ok(Act::Send { to: "L1".into(), text: "/relay:review finder-1".into() }));
    assert_eq!(act("b12-1", "review"), Ok(Act::Send { to: "P1".into(), text: "/pilead:review b12-1".into() }));
    assert!(act("P1", "board").is_err(), "no board from Cue");
    assert!(act("C3", "review").unwrap_err().contains("isn't listening"), "no review sent to a lead nobody's running");
    assert!(act("L1", "review").is_err());
    assert!(act("nobody", "diff").is_err());

    assert!(close_plan("L1").unwrap().unwrap_err().contains("leads 1 executor still open"), "a lead with open executors isn't closed from Cue");
    assert_eq!(close_plan("nobody"), None);
    assert_eq!(close_plan("C2"), None, "an executor relay already closed (its agent resumed elsewhere) is ended as any session");

    let known: HashSet<String> = ["L1", "P1", "nobody"].iter().map(|s| s.to_string()).collect();
    let v = view(&known);
    let names: Vec<&str> = v["L1"]["executors"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["finder"], "closed executors and other leads' are left out");
    assert_eq!(v["P1"]["executors"][0]["session_id"], "b12-1");
    assert!(v.get("nobody").is_none());

    // A record that changes is read again; one that's removed drops out of the cache.
    std::thread::sleep(std::time::Duration::from_millis(20));
    write(&relay.join("finder-1/session.json"), r#"{"session_id":"finder-1","agent":"relay-executor","claude_session":"C1","owner_lead":"L1","topic":"finder","status":"closed","current_packet":2}"#);
    std::fs::remove_dir_all(relay.join("old-1")).unwrap();
    let again = scan(&relay, &pi, &cs);
    assert_eq!(again["C1"].status, "closed");
    assert!(!again.contains_key("C2"));
    assert!(!crate::locking::lock(&PARSED, "PARSED").as_ref().unwrap().files.keys().any(|p| p.starts_with(relay.join("old-1"))));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn reads_a_plans_queue() {
    let plan: Value = serde_json::from_str(r#"{"items":[
            {"n":1,"packet":"_staging/a-packet.md","executor":"a-1","done":"2026-09-13T14:57:51"},
            {"n":2,"packet":"_staging/b-packet.md","executor":"b-1","done":null},
            {"n":3,"packet":"_staging/docs-refresh-packet.md","note":"","executor":null},
            {"n":4,"packet":"_staging/d-packet.md","note":"row 90"}]}"#).unwrap();
    assert_eq!(plan_queue(&plan), ("docs-refresh".to_string(), 2), "done and bound items aren't queued");
    assert_eq!(plan_queue(&serde_json::json!({})), (String::new(), 0));
}

#[test]
fn reads_relay_verifys_verdict() {
    let ok = "  VERDICT: COUNTS-MATCH\n  STAGED REALITY\n    claimed, NOT staged:  0\n";
    assert_eq!(parse_verify(ok), Some(Verdict { verdict: "COUNTS-MATCH".into(), note: String::new() }));
    let off = "  VERDICT: MISMATCH\n    claimed and staged:   3\n    claimed, NOT staged:  2\n";
    assert_eq!(parse_verify(off).unwrap().note, "2 changes it reports are not staged");
    assert_eq!(parse_verify("no such session: x"), None);
}
