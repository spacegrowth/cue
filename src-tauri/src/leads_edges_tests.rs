//! relay's state folder caught mid-write (stability plan §4, Relay: "relay state folder half-written"): a file
//! read while relay writes it is skipped or read as far as it's whole, never taken as something else, and the
//! next look reads it whole.

use super::*;

/// A relay state folder of its own under the temp folder, for one case.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("cue-relay-half-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Self(root)
    }

    fn write(&self, rel: &str, text: &str) -> &Self {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
        self
    }

    fn scan(&self) -> HashMap<String, Member> {
        let mut index = HashMap::new();
        scan_relay(&self.0, &HashSet::from(["L".to_string()]), &mut index);
        index
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MARKER: &str = r#"{"session_id":"L","project":"router","started":"2026-11-01T10:00:00"}"#;
const EXEC: &str = r#"{"agent":"relay-executor","session_id":"fills","claude_session":"C1","owner_lead":"L","status":"reported","current_packet":1,"topic":"fills"}"#;

/// The first `n` bytes of `text`: what a reader sees while it's being written.
fn cut(text: &str, n: usize) -> &str {
    &text[..n]
}

#[test]
fn an_executor_whose_session_file_is_half_written_is_skipped_and_the_rest_are_read() {
    let f = Folder::new("exec");
    f.write("lead/L/marker.json", MARKER).write("fills/session.json", cut(EXEC, 40));
    f.write("docs/session.json", &EXEC.replace("fills", "docs").replace("C1", "C2"));
    let index = f.scan();
    assert!(!index.contains_key("C1"), "half a record isn't a record");
    assert_eq!(index["C2"].lead_armed, Some(true), "the whole one next to it is read as always");
    f.write("fills/session.json", EXEC);
    assert_eq!(f.scan()["C1"].status, "reported", "whole on the next look");
}

#[test]
fn a_leads_half_written_marker_leaves_its_executors_listed_but_not_armed() {
    let f = Folder::new("marker");
    f.write("lead/L/marker.json", cut(MARKER, 20)).write("fills/session.json", EXEC);
    let index = f.scan();
    assert!(!index.contains_key("L"));
    assert_eq!((index["C1"].lead_armed, index["C1"].lead.as_str()), (Some(false), "L"), "no lead to wake: its report reaches nobody yet");
}

#[test]
fn a_half_written_plan_leaves_the_lead_with_no_plan_shown() {
    let f = Folder::new("plan");
    let plan = r#"{"items": [{"note": "wire the fills"}, {"note": "docs"}]}"#;
    f.write("lead/L/marker.json", MARKER).write("lead/L/plan.json", cut(plan, 30));
    let index = f.scan();
    assert_eq!((index["L"].role.as_str(), index["L"].plan_next.as_str(), index["L"].plan_queued), ("lead", "", 0));
    f.write("lead/L/plan.json", plan);
    assert_eq!((f.scan()["L"].plan_next.as_str(), f.scan()["L"].plan_queued), ("wire the fills", 2));
}

#[test]
fn an_executor_whose_packet_file_isnt_written_yet_has_no_goal_yet() {
    let f = Folder::new("packet");
    f.write("lead/L/marker.json", MARKER).write("fills/session.json", EXEC);
    assert_eq!(f.scan()["C1"].goal, "");
    f.write("fills/packets/001-packet.md", "GOAL: fill the router's gaps\n");
    assert_eq!(f.scan()["C1"].goal, "fill the router's gaps");
}

#[test]
fn a_ledger_line_still_being_written_is_read_once_it_is_whole() {
    let f = Folder::new("ledger");
    let line = r#"{"event":"lead_handoff","from_lead":"L1","to_lead":"L2","project":"router","ts":"2026-11-01T10:00:00"}"#;
    f.write("sessions.jsonl", cut(line, 50));
    let ledger = f.0.join("sessions.jsonl");
    assert!(ledger_handoffs(&ledger).is_empty(), "no newline yet: not a line");
    f.write("sessions.jsonl", &format!("{line}\n"));
    assert_eq!(ledger_handoffs(&ledger)["L1"].0, "L2");
}

#[test]
fn fifty_executors_of_one_lead_are_all_read() {
    let f = Folder::new("fifty");
    f.write("lead/L/marker.json", MARKER);
    for i in 0..50 {
        f.write(&format!("e{i}/session.json"), &EXEC.replace("\"fills\"", &format!("\"e{i}\"")).replace("C1", &format!("C{i}")));
    }
    let index = f.scan();
    assert_eq!(index.values().filter(|m| m.role == "executor" && m.lead == "L").count(), 50);
    assert_eq!(open_executors(&index, "L").len(), 50);
}
