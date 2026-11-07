//! relay's ledger, and what waits on you until its work is committed.

use super::*;

#[test]
fn the_ledger_is_read_incrementally() {
    let dir = std::env::temp_dir().join(format!("cue-ledger-inc-{}", std::process::id()));
    let path = dir.join("sessions.jsonl");
    let ev = |sid: &str, n: u64, v: &str| format!("{{\"event\":\"report_verify\",\"session_id\":\"{sid}\",\"packet\":{n},\"verdict\":\"{v}\",\"mismatches\":0}}\n");
    let get = |sid: &str, n: u64| read_ledger(&path).verdicts.get(&(sid.to_string(), n)).map(|v| v.verdict.clone());
    let offset = || crate::locking::lock(&LEDGER, "LEDGER").as_ref().and_then(|c| c.get(&path)).map(|l| l.offset).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(get("a", 1), None, "no ledger yet: no verdicts");

    // First read: everything there.
    let first = format!("{}{}", ev("a", 1, "MISMATCH"), ev("b", 1, "COUNTS-MATCH"));
    write(&path, &first);
    assert_eq!((get("a", 1).as_deref(), get("b", 1).as_deref()), (Some("MISMATCH"), Some("COUNTS-MATCH")));
    assert_eq!(offset(), first.len() as u64);

    // Appended lines: only they are read, and a later verdict for the same packet wins.
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    std::io::Write::write_all(&mut f, format!("{}{}", ev("a", 1, "COUNTS-MATCH"), ev("c", 2, "MALFORMED")).as_bytes()).unwrap();
    assert_eq!((get("a", 1).as_deref(), get("c", 2).as_deref()), (Some("COUNTS-MATCH"), Some("MALFORMED")));
    // Proof it didn't start over: corrupt the already-read part in place (same length); the old verdict stands.
    let len = std::fs::metadata(&path).unwrap().len();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[..first.len()].fill(b' ');
    std::fs::write(&path, &bytes).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), len);
    assert_eq!(get("b", 1).as_deref(), Some("COUNTS-MATCH"), "the bytes already read aren't read again");
    let after_appends = offset();

    // A partial last line waits, then counts once it's finished.
    let line = ev("d", 3, "MISMATCH");
    let (head, tail) = line.split_at(line.len() / 2);
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    std::io::Write::write_all(&mut f, head.as_bytes()).unwrap();
    assert_eq!(get("d", 3), None, "half a line isn't a verdict");
    assert_eq!(offset(), after_appends, "the partial line isn't consumed");
    std::io::Write::write_all(&mut f, tail.as_bytes()).unwrap();
    assert_eq!(get("d", 3).as_deref(), Some("MISMATCH"));
    assert_eq!(get("a", 1).as_deref(), Some("COUNTS-MATCH"));

    // The ledger shrinks (replaced by a shorter one, same file): start over from its beginning.
    write(&path, &ev("z", 9, "COUNTS-MATCH"));
    assert_eq!((get("z", 9).as_deref(), get("a", 1)), (Some("COUNTS-MATCH"), None), "only what's in the new ledger");

    // A different file at the same path (a new inode, even a longer one) is read from the start too.
    std::fs::remove_file(&path).unwrap();
    write(&path, &format!("{}{}{}", ev("y", 1, "MALFORMED"), ev("y", 2, "MALFORMED"), ev("y", 3, "MALFORMED")));
    assert_eq!((get("y", 3).as_deref(), get("z", 9)), (Some("MALFORMED"), None));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reads_a_local_executors_verdict_from_relays_ledger() {
    let root = std::env::temp_dir().join(format!("cue-ledger-{}", std::process::id()));
    let (relay, pi, cs) = (root.join("relay"), root.join("pi"), root.join("cs"));
    write(&relay.join("sessions.jsonl"), concat!(
        r#"{"ts":"2026-10-10T10:00:00","event":"spawn","session_id":"ex-1"}"#, "\n",
        r#"{"ts":"2026-10-10T10:05:00","event":"report_verify","session_id":"ex-1","packet":2,"verdict":"MISMATCH","rerun":false,"mismatches":1}"#, "\n",
        "not json but mentions \"report_verify\"\n",
        r#"{"ts":"2026-10-10T10:09:00","event":"report_verify","session_id":"ex-1","packet":2,"verdict":"COUNTS-MATCH","rerun":false,"mismatches":0}"#, "\n",
        r#"{"ts":"2026-10-10T10:09:00","event":"report_verify","session_id":"ex-2","packet":1,"verdict":"MALFORMED","rerun":false,"mismatches":0}"#, "\n"));
    write(&relay.join("ex-1/session.json"), r#"{"session_id":"ex-1","agent":"relay-executor","claude_session":"C1","owner_lead":"L","topic":"one","status":"reported","current_packet":2}"#);
    write(&relay.join("ex-1/packets/002-packet.md"), "GOAL: first\n");
    write(&relay.join("ex-2/session.json"), r#"{"session_id":"ex-2","agent":"relay-executor","claude_session":"C2","owner_lead":"L","topic":"two","status":"reported","current_packet":1}"#);
    write(&relay.join("ex-3/session.json"), r#"{"session_id":"ex-3","agent":"relay-executor","claude_session":"C3","owner_lead":"L","topic":"three","status":"busy","current_packet":4}"#);
    write(&relay.join("ex-4/session.json"), r#"{"session_id":"ex-4","agent":"relay-executor","claude_session":"C4","owner_lead":"L","topic":"four","status":"closed","current_packet":2}"#);
    let index = scan(&relay, &pi, &cs);
    assert_eq!(index["C1"].verify, Some(Verdict { verdict: "COUNTS-MATCH".into(), note: String::new() }), "the last verdict for the packet wins");
    assert_eq!(index["C2"].verify.as_ref().map(|v| v.verdict.as_str()), Some("MALFORMED"));
    assert_eq!(index["C3"].verify, None, "no verdict for packet 4 yet");
    assert_eq!(index["C4"].verify, None, "closed: not read");
    assert_eq!((index["C1"].goal.as_str(), index["C1"].outcome.as_str()), ("first", ""), "goal from the packet; no report yet");
    // The ledger grows: the next scan sees the new line.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let mut text = std::fs::read_to_string(relay.join("sessions.jsonl")).unwrap();
    text.push_str("{\"event\":\"report_verify\",\"session_id\":\"ex-3\",\"packet\":4,\"verdict\":\"MISMATCH\",\"mismatches\":3}\n");
    write(&relay.join("sessions.jsonl"), &text);
    assert_eq!(scan(&relay, &pi, &cs)["C3"].verify, Some(Verdict { verdict: "MISMATCH".into(), note: "3 mismatches".into() }));
    // The window gets it as `verify`, on the executor and in its lead's list.
    assert_eq!(verdict_of(&index["C2"]).map(|v| v.verdict), Some("MALFORMED".to_string()));
    let _ = std::fs::remove_dir_all(&root);
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(dir).args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args).output().unwrap().status.success();
    assert!(ok, "git {args:?}");
}

/// "Woken" and "reviewed" are separate facts: relay waking the lead says nothing about a review. A packet waits
/// on you until its work is committed or discarded, including one relay parked after its review.
#[test]
fn a_woken_lead_is_not_a_review_and_a_packet_waits_on_you_until_its_work_is_committed() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("cue-card-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (relay, pi, cs) = (root.join("relay"), root.join("pi"), root.join("cs"));
    write(&cs.join("1.json"), &format!(r#"{{"pid":{},"sessionId":"L"}}"#, std::process::id()));
    write(&relay.join("lead/L/marker.json"), r#"{"session_id":"L","project":"proj","autonomous":true}"#);
    let ex = |id: &str, extra: &str| write(&relay.join(format!("{id}/session.json")), &format!(r#"{{"session_id":"{id}","agent":"relay-executor","claude_session":"C-{id}","owner_lead":"L","topic":"{id}","current_packet":1,{extra}}}"#));
    ex("woken", r#""status":"reported","report_seen_packet":1"#);
    ex("fresh", r#""status":"reported""#);
    ex("filed", r#""status":"reported","report_seen_packet":1"#);
    write(&relay.join("filed/packets/001-review.md"), "1. A thing\n2. Another\n");
    ex("ledgered", r#""status":"reported""#);
    ex("older", r#""status":"reported","current_packet":2"#);
    write(&relay.join("older/packets/001-review.md"), "old findings");
    let (staged, committed) = (root.join("wt-staged"), root.join("wt-committed"));
    for wt in [&staged, &committed] {
        std::fs::create_dir_all(wt).unwrap();
        git(wt, &["init", "-q"]);
        write(&wt.join("f.txt"), "x\n");
        git(wt, &["add", "f.txt"]);
    }
    git(&committed, &["commit", "-q", "-m", "c"]);
    for (id, wt) in [("parked-staged", &staged), ("parked-committed", &committed)] {
        ex(id, &format!(r#""status":"closed","auto_closed":"reviewed","worktree":"{}""#, wt.display()));
        write(&relay.join(format!("{id}/packets/001-review.md")), "findings");
    }
    ex("closed", r#""status":"closed""#);
    write(&relay.join("sessions.jsonl"), concat!(r#"{"event":"report_reviewed","session_id":"ledgered","packet":1,"mode":"inline"}"#, "\n", r#"{"event":"auto_commit","session_id":"fresh","packet":1,"cleared":true,"files":3,"insertions":94,"deletions":2}"#, "\n"));
    let index = scan(&relay, &pi, &cs);
    let got = |id: &str| index[&format!("C-{id}")].clone();
    assert!(got("woken").woken && !got("woken").reviewed, "woken is not reviewed");
    assert!(!missed_wake(&Member { ready_late: true, ..got("woken") }, true), "a woken lead isn't a missed wake");
    assert!(!got("fresh").woken && !got("fresh").reviewed);
    assert_eq!(got("fresh").diff_size, Some((3, 94, 2)), "the diff's size, when relay recorded it");
    assert!(got("filed").reviewed && got("filed").review.contains("2. Another"), "a review file is a review, with its findings");
    assert!(got("ledgered").reviewed && got("ledgered").review.is_empty(), "ledgered inline: reviewed, no file");
    assert!(!got("older").reviewed, "another packet's review");
    assert!(got("parked-staged").parked_staged && got("parked-staged").reviewed);
    assert!(!got("parked-committed").parked_staged, "committed: nothing staged anymore");

    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
    let mut waiting: Vec<(String, bool)> = needing_you().into_iter().map(|(_, m, auto)| (m.name, auto)).collect();
    waiting.sort();
    let names: Vec<&str> = waiting.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["filed", "fresh", "ledgered", "older", "parked-staged", "woken"], "reported ones and the parked one still staged wait; committed and closed ones don't");
    assert!(waiting.iter().all(|(_, auto)| *auto), "each with whether its lead is on auto mode");
    assert_eq!(packet_of("C-woken"), Some(("woken".to_string(), 1)));
    *crate::locking::lock(&INDEX, "INDEX") = None;
    let _ = std::fs::remove_dir_all(&root);
}


/// Many sessions share one checkout: what's staged there may be another session's. A packet stops waiting on
/// you once a commit after its report touches its own files (its lead committed it by hand), parked or not;
/// another session's staged work, or a commit of other files, doesn't keep it or end it.
#[test]
fn a_packet_its_lead_committed_by_hand_stops_waiting_even_in_a_shared_checkout() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("cue-landed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (relay, pi, cs, wt) = (root.join("relay"), root.join("pi"), root.join("cs"), root.join("shared"));
    write(&cs.join("1.json"), &format!(r#"{{"pid":{},"sessionId":"L"}}"#, std::process::id()));
    write(&relay.join("lead/L/marker.json"), r#"{"session_id":"L","project":"proj"}"#);
    std::fs::create_dir_all(&wt).unwrap();
    git(&wt, &["init", "-q"]);
    for f in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        write(&wt.join(f), "1\n");
    }
    git(&wt, &["add", "."]);
    let hour_ago = format!("@{} +0000", now_secs() - 3600);
    let base = std::process::Command::new("git").arg("-C").arg(&wt).args(["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "base"]).env("GIT_COMMITTER_DATE", &hour_ago).status().unwrap();
    assert!(base.success(), "the base commit, an hour ago");
    // Each packet staged one file; its report was written a minute ago.
    let ex = |id: &str, file: &str, extra: &str| {
        write(&relay.join(format!("{id}/session.json")), &format!(r#"{{"session_id":"{id}","agent":"relay-executor","claude_session":"C-{id}","owner_lead":"L","topic":"{id}","current_packet":1,"worktree":"{}",{extra}}}"#, wt.display()));
        write(&relay.join(format!("{id}/packets/001-diff.html")), &format!(r#"<script>const d = "diff --git a/{file} b/{file}\nindex 1..2\n";</script>"#));
        let report = relay.join(format!("{id}/packets/001-report.md"));
        write(&report, "Done.\n");
        std::fs::File::options().write(true).open(&report).unwrap().set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(60)).unwrap();
    };
    let parked = r#""status":"closed","auto_closed":"reviewed""#;
    ex("by-hand", "a.txt", r#""status":"reported""#);
    ex("parked-by-hand", "b.txt", parked);
    ex("reported-open", "c.txt", r#""status":"reported""#);
    ex("parked-open", "d.txt", parked);
    // The lead commits a.txt and b.txt by hand; c.txt and d.txt are still staged, and so is another session's e.txt.
    for f in ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"] {
        write(&wt.join(f), "2\n");
    }
    git(&wt, &["add", "."]);
    git(&wt, &["commit", "-q", "-m", "by hand", "--", "a.txt", "b.txt"]);
    let index = scan(&relay, &pi, &cs);
    let got = |id: &str| index[&format!("C-{id}")].clone();
    assert_eq!(packet_files(&relay.join("by-hand"), 1), ["a.txt"], "its files, from its diff page");
    assert!(got("by-hand").landed && got("parked-by-hand").landed, "committed after the report: landed");
    assert!(!got("parked-by-hand").parked_staged, "other files staged in the same checkout don't keep it");
    assert!(!got("reported-open").landed && got("parked-open").parked_staged, "its own files still staged: it waits");

    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
    let mut names: Vec<String> = needing_you().into_iter().map(|(_, m, _)| m.name).collect();
    names.sort();
    assert_eq!(names, ["parked-open", "reported-open"], "committed ones are gone, parked or not");
    *crate::locking::lock(&INDEX, "INDEX") = None;
    let _ = std::fs::remove_dir_all(&root);
}
