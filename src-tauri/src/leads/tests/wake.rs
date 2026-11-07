//! Late reports and missed wake-ups, in this Mac's zone and a machine's own.

use super::*;

fn done_exec() -> Member {
    Member { role: "executor".into(), plugin: "relay".into(), status: "reported".into(), lead: "L".into(), lead_armed: Some(true), ready_late: true, ..Default::default() }
}

#[test]
fn a_report_is_late_once_it_has_waited_ninety_seconds() {
    let at = epoch_secs("2026-10-04T09:50:00", None).unwrap();
    assert_eq!(epoch_secs("2026-10-04T09:50:00.123456", None), Some(at), "a fraction is ignored");
    assert_eq!(epoch_secs("2026-10-04T09:51:00", None), Some(at + 60));
    assert_eq!(epoch_secs("not a time", None), None);
    assert_eq!(epoch_secs("", None), None);
    let e = |status: &str| serde_json::json!({ "status": status, "updated": "2026-10-04T09:50:00" });
    assert!(!ready_late(&e("reported"), at + 89, None), "not yet");
    assert!(ready_late(&e("reported"), at + 90, None));
    assert!(ready_late(&e("idle"), at + 600, None));
    assert!(!ready_late(&e("busy"), at + 600, None), "still at work: no report to miss");
    assert!(!ready_late(&serde_json::json!({ "status": "reported" }), at + 600, None), "no timestamp: no claim");
}

#[test]
fn a_machines_timestamps_are_read_in_its_own_zone() {
    // 09:50 at +0900 is 00:50 UTC; at -0800 it's 17:50 UTC.
    assert_eq!(epoch_secs("2026-10-04T09:50:00", Some(9 * 3600)), Some(1_791_075_000));
    assert_eq!(epoch_secs("2026-10-04T09:50:00", Some(0)), Some(1_791_075_000 + 9 * 3600));
    assert_eq!(epoch_secs("2026-10-04T09:50:00", Some(-8 * 3600)), Some(1_791_075_000 + 17 * 3600));
    assert_eq!(tz_of("x\n__CUE_LIVE__\nL\n__CUE_TZ__:+0530\n"), Some(19_800));
    assert_eq!(tz_of("__CUE_TZ__:-0800\n"), Some(-28_800));
    assert_eq!(tz_of("__CUE_TZ__:+0000"), Some(0));
    assert_eq!(tz_of("__CUE_TZ__:"), None);
    assert_eq!(tz_of("__CUE_TZ__:oops"), None);
    assert_eq!(tz_of("no offset said"), None);
    let ready = |zone: Option<i64>, ts: &str| ready_late(&serde_json::json!({ "status": "reported", "updated": ts }), 1_791_075_000 + 120, zone);
    assert!(ready(Some(9 * 3600), "2026-10-04T09:50:00"), "ready 120 s ago there");
    assert!(!ready(Some(9 * 3600), "2026-10-04T09:51:00"), "ready 60 s ago there");
    assert!(!ready(Some(-8 * 3600), "2026-10-04T09:50:00"), "in a zone 17 h behind, 09:50 hasn't happened yet");
}

/// A machine's answer to REMOTE_LIST: its UTC offset, and one reported executor whose record was
/// last written `secs_ago` seconds ago by that machine's clock (its local time, not ours).
fn listing_in_zone(off: i64, secs_ago: i64) -> String {
    let t = (now_secs() as i64 + off - secs_ago) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::gmtime_r(&t, &mut tm) };
    let updated = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec);
    let z = format!("{}{:02}{:02}", if off < 0 { '-' } else { '+' }, off.abs() / 3600, off.abs() % 3600 / 60);
    format!("BIN:/r/relay\n{{\"leads\": [{{\"session_id\": \"ZL\", \"project\": \"p\"}}], \"executors\": [{{\"agent\": \"relay-executor\", \"session_id\": \"z-1\", \"status\": \"reported\", \"current_packet\": 1, \"owner_lead\": \"ZL\", \"claude_session\": \"ZC\", \"updated\": \"{updated}\"}}]}}\n__CUE_LIVE__\nZL\n__CUE_TZ__:{z}\n")
}

#[test]
fn a_machine_in_another_zone_is_judged_by_its_own_clock() {
    for off in [9 * 3600, -8 * 3600, 5 * 3600 + 1800, 0] {
        assert!(parse_remote(&listing_in_zone(off, 300), "far").1["ZC"].ready_late, "ready 5 min ago at {off}");
        assert!(!parse_remote(&listing_in_zone(off, 30), "far").1["ZC"].ready_late, "ready 30 s ago at {off}");
    }
    // Without its offset the timestamp can only be read here: a machine 9 h ahead looks 9 h in the future.
    let blind = listing_in_zone(9 * 3600, 300).replace("__CUE_TZ__:+0900", "");
    assert!(!parse_remote(&blind, "far").1["ZC"].ready_late || local_offset_secs() >= 9 * 3600, "no offset: read as this Mac's local time");
}

/// This Mac's UTC offset now, in seconds.
fn local_offset_secs() -> i64 {
    let t = now_secs() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm.tm_gmtoff as i64
}

#[test]
fn a_missed_wake_needs_every_condition() {
    assert!(missed_wake(&done_exec(), true));
    assert!(missed_wake(&Member { status: "idle".into(), ..done_exec() }, true), "idle is done too");
    assert!(!missed_wake(&Member { status: "busy".into(), ..done_exec() }, true), "not done");
    assert!(!missed_wake(&Member { status: "stalled".into(), ..done_exec() }, true), "stalled never reported");
    assert!(!missed_wake(&Member { woken: true, ..done_exec() }, true), "its lead was woken for it");
    assert!(!missed_wake(&Member { reviewed: true, ..done_exec() }, true), "its lead reviewed it");
    assert!(!missed_wake(&Member { ready_late: false, ..done_exec() }, true), "ready under 90 seconds ago");
    assert!(!missed_wake(&Member { lead_armed: Some(false), ..done_exec() }, true), "its lead isn't listening");
    assert!(!missed_wake(&Member { lead_armed: None, ..done_exec() }, true));
    assert!(!missed_wake(&done_exec(), false), "its lead is working, compacting or asking");
    assert!(!missed_wake(&Member { role: "lead".into(), ..done_exec() }, true));
}

#[test]
fn the_scan_marks_a_late_report_and_lists_it_when_its_lead_is_free() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    // Reported long ago, never seen; a listening lead (a machine's live list).
    let out = r#"BIN:/r/relay
{"leads": [{"session_id": "ML", "project": "p"}],
 "executors": [{"agent": "relay-executor", "session_id": "old-1", "status": "reported", "current_packet": 1, "owner_lead": "ML", "claude_session": "MC1", "updated": "2020-01-01T00:00:00"},
               {"agent": "relay-executor", "session_id": "seen-1", "status": "reported", "current_packet": 1, "report_seen_packet": 1, "owner_lead": "ML", "claude_session": "MC2", "updated": "2020-01-01T00:00:00"},
               {"agent": "relay-executor", "session_id": "fresh-1", "status": "reported", "current_packet": 1, "owner_lead": "ML", "claude_session": "MC3", "updated": "2999-01-01T00:00:00"}]}
__CUE_LIVE__
ML
"#;
    let index = parse_remote(out, "box").1;
    assert!(index["MC1"].ready_late && !index["MC3"].ready_late);
    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
    assert_eq!(missed(|_| true), ["MC1"], "seen and fresh reports aren't missed");
    assert!(missed(|_| false).is_empty(), "its lead is busy");
    assert_eq!(missed(|lead| name_of(lead) != "\u{0}"), ["MC1"], "a lead check that reads the index itself doesn't wait on it");
}
