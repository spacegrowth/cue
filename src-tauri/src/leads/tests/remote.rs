//! Your machines' listings: relay's and pi-lead's crews there, live sessions, zones, tmux sessions and screens.

use super::*;

#[test]
fn a_machines_crews_come_from_relays_listing_there() {
    let out = r#"BIN:/home/me/.claude/plugins/cache/claude-relay/relay/0.5.11/bin/relay
{"leads": [{"session_id": "L9", "project": "smoke", "color": [200, 140, 135], "model": "claude-opus-5-5", "autonomous": true, "last_active": "2026-10-04T09:57:37"},
           {"session_id": "L8", "project": "old", "last_active": "2026-10-03T18:30:38", "ended": true}],
 "executors": [{"agent": "relay-executor", "session_id": "hello-md", "topic": "hello-md", "status": "reported", "current_packet": 2, "owner_lead": "L9", "claude_session": "C9", "model": "claude-sonnet-5-5", "updated": "2026-10-04T09:50:00"},
               {"agent": "relay-executor", "session_id": "ls-root", "topic": "", "status": "busy", "owner_lead": "L8", "owner_project": "old", "claude_session": "C8"},
               {"agent": "something-else", "session_id": "x", "claude_session": "CX"}]}
__CUE_LIVE__
L9
C9
"#;
    let (bin, index) = parse_remote(out, "build-box");
    assert_eq!(bin, "/home/me/.claude/plugins/cache/claude-relay/relay/0.5.11/bin/relay");
    let l9 = &index["L9"];
    assert_eq!((l9.role.as_str(), l9.name.as_str(), l9.machine.as_str(), l9.auto), ("lead", "smoke", "build-box", Some(true)));
    let c9 = &index["C9"];
    assert_eq!((c9.role.as_str(), c9.id.as_str(), c9.lead.as_str(), c9.lead_name.as_str(), c9.lead_armed, c9.packet), ("executor", "hello-md", "L9", "smoke", Some(true), Some(2)));
    assert_eq!(c9.color, Some([200, 140, 135]), "its lead's color");
    assert_eq!(index["C8"].lead_armed, Some(false), "its lead's session isn't running there");
    assert_eq!(index["C8"].name, "ls-root", "no topic: its relay id");
    assert!(!index.contains_key("CX"), "not a relay executor");
    assert!(parse_remote("NORELAY\n", "build-box").1.is_empty(), "no relay there: no crews");
}

#[test]
fn the_listing_script_gives_one_live_session_per_line() {
    // A machine as Claude Code and relay leave it: two live sessions whose files end without a
    // newline (as Claude Code writes them), one dead, and relay's command in the installed plugin.
    let home = std::env::temp_dir().join(format!("cue-remote-list-{}", std::process::id()));
    let plugin = home.join("plugins/cache/claude-relay/relay/9.9.9");
    std::fs::create_dir_all(plugin.join("bin")).unwrap();
    std::fs::create_dir_all(home.join("sessions")).unwrap();
    std::fs::write(home.join("plugins/installed_plugins.json"), format!("{{\n  \"plugins\": {{\n    \"relay@claude-relay\": [{{\n      \"installPath\": \"{}\"\n    }}]\n  }}\n}}\n", plugin.display())).unwrap();
    let relay = plugin.join("bin/relay");
    std::fs::write(&relay, "#!/bin/sh\necho '{\"leads\": [{\"session_id\": \"L1\", \"project\": \"p\"}], \"executors\": [{\"agent\": \"relay-executor\", \"session_id\": \"e\", \"owner_lead\": \"L1\", \"claude_session\": \"C1\", \"status\": \"reported\"}]}'\n").unwrap();
    std::os::unix::fs::PermissionsExt::set_mode(&mut std::fs::metadata(&relay).unwrap().permissions(), 0o755);
    std::fs::set_permissions(&relay, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let me = std::process::id();
    std::fs::write(home.join(format!("sessions/{me}.json")), r#"{"pid":1,"sessionId":"L1","cwd":"/tmp"}"#).unwrap();
    std::fs::write(home.join(format!("sessions/{}.json", std::os::unix::process::parent_id())), r#"{"pid":2,"sessionId":"C1","cwd":"/tmp"}"#).unwrap();
    std::fs::write(home.join("sessions/999999.json"), r#"{"pid":3,"sessionId":"DEAD"}"#).unwrap();
    let out = std::process::Command::new("/bin/sh").env("CLAUDE_CONFIG_DIR", &home).env("HOME", &home).env("TMUX_TMPDIR", &home).env_remove("TMUX").args(["-c", REMOTE_LIST]).output().unwrap();
    let out = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.contains("NOPILEAD") && pilead_bin_of(&out).is_empty(), "no pilead there: {out:?}");
    let (bin, index) = parse_remote(&out, "box");
    assert_eq!(bin, relay.display().to_string());
    assert_eq!(index["C1"].lead_armed, Some(true), "its lead is live there: {out:?}");
    let live: Vec<&str> = out.split("__CUE_LIVE__").nth(1).unwrap().split("__CUE_SCREENS__").next().unwrap().lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(TZ_MARK)).collect();
    assert_eq!(live.len(), 2, "one id per line, the dead one left out: {live:?}");
    assert!(tz_of(&out).is_some_and(|z| z.abs() <= 14 * 3600), "it says its UTC offset: {out:?}");
    assert_eq!(live_of(&out).len(), 2, "the offset line isn't a session");

    // The same machine with pilead on its PATH: its crews come in the same answer.
    let pilead = home.join("bin/pilead");
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::write(&pilead, format!("#!/bin/sh\ncat <<'EOF'\n{PI_LISTING}\nEOF\n")).unwrap();
    std::fs::set_permissions(&pilead, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let path = format!("{}:/usr/bin:/bin", home.join("bin").display());
    let out = std::process::Command::new("/bin/sh").env("CLAUDE_CONFIG_DIR", &home).env("HOME", &home).env("PATH", path).env("TMUX_TMPDIR", &home).env_remove("TMUX").args(["-c", REMOTE_LIST]).output().unwrap();
    let out = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(pilead_bin_of(&out), pilead.display().to_string(), "{out:?}");
    let (bin, index) = parse_remote(&out, "box");
    assert_eq!(bin, relay.display().to_string(), "relay is still found");
    assert!(index.contains_key("C1") && index["pe-1"].plugin == "pilead" && index["PL1"].role == "lead", "both tools' crews: {:?}", index.keys());
    assert_eq!(live_of(&out).len(), 2);
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn an_executor_counts_as_seen_once_its_lead_was_woken_for_this_packets_report() {
    let e = |v: &str| serde_json::from_str::<Value>(v).unwrap();
    assert!(report_woken(&e(r#"{"current_packet": 2, "report_seen_packet": 2}"#)));
    assert!(!report_woken(&e(r#"{"current_packet": 2, "report_seen_packet": 1}"#)), "woken: the last packet's, not this one's");
    assert!(!report_woken(&e(r#"{"current_packet": 1, "report_seen_packet": null}"#)));
    assert!(!report_woken(&e(r#"{"current_packet": 1}"#)));
    let out = "BIN:/r\n{\"leads\": [], \"executors\": [{\"agent\": \"relay-executor\", \"session_id\": \"a\", \"claude_session\": \"CA\", \"status\": \"reported\", \"current_packet\": 1, \"report_seen_packet\": 1}]}\n__CUE_LIVE__\n";
    let ca = &parse_remote(out, "box").1["CA"];
    assert!(ca.woken, "on a machine too");
    assert!(!ca.reviewed, "woken is not reviewed");
}

#[test]
fn a_machines_relay_may_or_may_not_list_goal_outcome_and_verify() {
    let out = r#"BIN:/r/relay
{"leads": [{"session_id": "L1", "project": "p"}],
 "executors": [{"agent": "relay-executor", "session_id": "new-1", "topic": "new", "status": "reported", "current_packet": 1, "owner_lead": "L1", "claude_session": "C1",
                "goal": "add CSV export", "outcome": "Export writes trades.csv", "verify": {"verdict": "MISMATCH", "ts": "2026-10-10T10:00:00", "rerun": false, "mismatches": 2}},
               {"agent": "relay-executor", "session_id": "new-2", "topic": "pending", "status": "busy", "current_packet": 1, "owner_lead": "L1", "claude_session": "C2", "goal": "tune it", "outcome": "", "verify": null},
               {"agent": "relay-executor", "session_id": "old-1", "topic": "older", "status": "reported", "current_packet": 1, "owner_lead": "L1", "claude_session": "C3"},
               {"agent": "relay-executor", "session_id": "bad-1", "topic": "odd", "status": "busy", "owner_lead": "L1", "claude_session": "C4", "goal": 7, "verify": "COUNTS-MATCH"},
               {"agent": "relay-executor", "session_id": "gone-1", "topic": "gone", "status": "closed", "owner_lead": "L1", "claude_session": "C5", "goal": "stale"}]}
__CUE_LIVE__
"#;
    let index = parse_remote(out, "box").1;
    let c1 = &index["C1"];
    assert_eq!((c1.goal.as_str(), c1.outcome.as_str()), ("add CSV export", "Export writes trades.csv"));
    assert_eq!(c1.verify, Some(Verdict { verdict: "MISMATCH".into(), note: "2 mismatches".into() }));
    assert_eq!((index["C2"].goal.as_str(), index["C2"].outcome.as_str(), index["C2"].verify.clone()), ("tune it", "", None), "no report yet, verify null");
    let o = &index["C3"];
    assert_eq!((o.goal.as_str(), o.outcome.as_str(), o.verify.clone(), o.status.as_str()), ("", "", None, "reported"), "an older relay sends none of them: nothing to show, no error");
    assert_eq!((index["C4"].goal.as_str(), index["C4"].verify.clone()), ("", None), "a field of the wrong type is ignored");
    assert_eq!(index["C5"].goal, "", "a closed executor's isn't kept");
    let long = format!(r#"BIN:/r/relay
{{"leads": [], "executors": [{{"agent": "relay-executor", "session_id": "l", "claude_session": "CL", "status": "busy", "goal": "{}"}}]}}
"#, "x".repeat(400));
    assert_eq!(parse_remote(&long, "box").1["CL"].goal.chars().count(), 300, "clipped, however long a machine sends it");
}

#[test]
fn a_machines_relay_and_pilead_executors_are_known_as_executors_and_their_leads_arent() {
    let relay = r#"{"leads": [{"session_id": "L9", "project": "smoke"}],
 "executors": [{"agent": "relay-executor", "session_id": "hello-md", "topic": "hello-md", "status": "busy", "owner_lead": "L9", "claude_session": "C9"},
               {"agent": "relay-executor", "session_id": "old", "topic": "old", "status": "closed", "owner_lead": "L9", "claude_session": "C7"}]}"#;
    let out = format!("BIN:/x/relay\n{relay}\n__CUE_PILEAD__\nPBIN:/x/pilead\n{PI_LISTING}\n__CUE_LIVE__\nL9\n");
    let (_, index) = parse_remote(&out, "box");
    for sid in ["C9", "C7", "pe-1", "pe-2"] {
        assert!(executor_in(&index, sid), "{sid}: an executor, its lead's to bring back");
    }
    for sid in ["L9", "PL1", "someone-else"] {
        assert!(!executor_in(&index, sid), "{sid}: not an executor");
    }
    // Only a relay lead is one: not its executors, not pi-lead's lead.
    assert!(relay_lead_in(&index, "L9"), "relay's lead");
    for sid in ["C9", "C7", "pe-1", "PL1", "someone-else"] {
        assert!(!relay_lead_in(&index, sid), "{sid}: not a relay lead");
    }
}

#[test]
fn a_machines_panes_on_a_screen_before_claude_code_starts_are_read_from_its_listing() {
    let out = "NORELAY\n__CUE_PILEAD__\nNOPILEAD\n__CUE_LIVE__\nC1\n__CUE_TZ__:+0000\n__CUE_SCREENS__\n\
@@PANE %4 relay-hello-md_2\nSecurity notes:\n1. Claude can make mistakes.\nPress Enter to continue…\n\
@@PANE %5 cue-app\n❯ No, exit\n  Yes, I trust this folder\nEnter to confirm · Esc to cancel\n\
@@PANE %6 work\n✻ Done\n❯ \n";
    let screens = screens_of(out);
    assert_eq!(screens.get("%4"), Some(&("relay-hello-md_2".to_string(), crate::terminal::dialog::Setup::Enter)));
    assert_eq!(screens.get("%5").map(|s| s.1), Some(crate::terminal::dialog::Setup::Trust));
    assert!(!screens.contains_key("%6"), "at its prompt: nothing to say");
    assert_eq!(live_of(out), HashSet::from(["C1".to_string()]), "the panes never read as live sessions");
    assert_eq!(tz_of(out), Some(0));
}

#[test]
fn a_machines_executors_name_their_own_tmux_session_when_their_listing_does() {
    let relay = r#"{"leads": [{"session_id": "L9", "project": "smoke", "tmux_session": "relay-lead-smoke"}],
 "executors": [{"agent": "relay-executor", "session_id": "hello-md", "topic": "hello-md", "status": "busy", "owner_lead": "L9", "claude_session": "C9", "tmux_session": "relay-hello-md_2"},
               {"agent": "relay-executor", "session_id": "no-key", "topic": "no-key", "status": "busy", "owner_lead": "L9", "claude_session": "C8"},
               {"agent": "relay-executor", "session_id": "odd", "topic": "odd", "status": "busy", "owner_lead": "L9", "claude_session": "C7", "tmux_session": "x; rm -rf ~"},
               {"agent": "relay-executor", "session_id": "colon", "topic": "colon", "status": "busy", "owner_lead": "L9", "claude_session": "C6", "tmux_session": "lead:0.1"},
               {"agent": "relay-executor", "session_id": "blank", "topic": "blank", "status": "busy", "owner_lead": "L9", "claude_session": "C5", "tmux_session": "  "}]}"#;
    let pi = r#"{"leads": [{"sid": "PL1", "project": "pi-proj", "live": "live"}],
 "executors": [{"sid": "pe-1", "lead": "PL1", "status": "busy", "topic": "parser", "tmux_session": "pilead-pe-1"},
               {"sid": "pe-2", "lead": "PL1", "status": "busy", "topic": "wire"},
               {"sid": "pe-3", "lead": "PL1", "status": "busy", "topic": "quoted", "tmux_session": "pe 3"}]}"#;
    let out = format!("BIN:/x/relay\n{relay}\n__CUE_PILEAD__\nPBIN:/x/pilead\n{pi}\n__CUE_LIVE__\nL9\nPL1\n");
    let (_, index) = parse_remote(&out, "box");
    // Present: that executor's own session, on its machine (relay's and pi-lead's alike).
    assert_eq!(exec_tmux_in(&index, "C9"), Some(("box".into(), "relay-hello-md_2".into())));
    assert_eq!(exec_tmux_in(&index, "pe-1"), Some(("box".into(), "pilead-pe-1".into())));
    // Missing (an older relay / pi-lead): nothing, so its terminal opens as it did before.
    for sid in ["C8", "pe-2"] {
        assert_eq!((index[sid].tmux_session.as_str(), exec_tmux_in(&index, sid)), ("", None), "{sid}");
    }
    // Anything but letters, digits, _ and -: refused, never handed to the shell or to tmux.
    for sid in ["C7", "C6", "C5", "pe-3"] {
        assert_eq!((index[sid].tmux_session.as_str(), exec_tmux_in(&index, sid)), ("", None), "{sid}");
    }
    // A lead is never what an executor's terminal attaches to, and a lead has none of its own here.
    assert_eq!(exec_tmux_in(&index, "L9"), None);
    assert_eq!(index["L9"].tmux_session, "");
    // On this Mac (no machine): not this path either.
    let mut local = index["C9"].clone();
    local.machine.clear();
    assert_eq!(exec_tmux_in(&HashMap::from([("C9".to_string(), local)]), "C9"), None);
    // The window gets it on the lead's executor list, for the rows it draws.
    let v = serde_json::to_value(&index["C9"]).unwrap();
    assert_eq!(v["tmux_session"], "relay-hello-md_2");
    assert!(serde_json::to_value(&index["C8"]).unwrap().get("tmux_session").is_none());
}

const PI_LISTING: &str = r#"{"leads": [{"sid": "PL1", "project": "pi-proj", "live": "live", "model": "deepseek/flash", "started": "2026-10-10T09:00:00Z", "waiting": 0, "wake": "on"},
                                       {"sid": "PL2", "project": "dead-proj", "live": "ghost", "started": "2026-10-01T09:00:00Z"}],
 "executors": [{"sid": "pe-1", "lead": "PL1", "project": "pi-proj", "status": "reported", "topic": "parser", "label": "[Exec] pe-1", "packet": 2, "goal": "write the parser", "outcome": "Parser handles nested lists", "reported": true, "worktree": "/w/1", "model": "deepseek/flash", "created": "2026-10-10T09:05:00Z"},
               {"sid": "pe-2", "lead": "PL1", "project": "pi-proj", "status": "busy", "topic": "", "packet": "1", "goal": "wire it up", "outcome": "", "reported": false},
               {"sid": "pe-3", "lead": "PL2", "project": "dead-proj", "status": "busy", "topic": "orphan", "packet": 1},
               {"sid": "pe-4", "lead": "PL1", "project": "pi-proj", "status": "closed", "topic": "done", "goal": "old goal"}]}"#;

#[test]
fn a_machines_pilead_crews_sit_beside_relays_in_the_same_tree() {
    let out = format!("BIN:/r/relay\n{{\"leads\": [{{\"session_id\": \"L1\", \"project\": \"rl\"}}], \"executors\": []}}\n__CUE_PILEAD__\nPBIN:/home/me/pi-lead/bin/pilead\n{PI_LISTING}\n__CUE_LIVE__\nL1\n__CUE_TZ__:+0000\n");
    let (bin, index) = parse_remote(&out, "build-box");
    assert_eq!(bin, "/r/relay", "relay's command is relay's");
    assert_eq!(pilead_bin_of(&out), "/home/me/pi-lead/bin/pilead");
    assert_eq!(index["L1"].plugin, "relay");
    let l = &index["PL1"];
    assert_eq!((l.role.as_str(), l.plugin.as_str(), l.name.as_str(), l.machine.as_str()), ("lead", "pilead", "pi-proj", "build-box"));
    let e = &index["pe-1"];
    assert_eq!((e.role.as_str(), e.plugin.as_str(), e.name.as_str(), e.lead.as_str(), e.lead_name.as_str(), e.lead_armed, e.packet, e.machine.as_str(), e.id.as_str()),
        ("executor", "pilead", "parser", "PL1", "pi-proj", Some(true), Some(2), "build-box", "pe-1"));
    assert_eq!((e.status.as_str(), e.goal.as_str(), e.outcome.as_str()), ("reported", "write the parser", "Parser handles nested lists"));
    assert_eq!((index["pe-2"].name.as_str(), index["pe-2"].packet, index["pe-2"].outcome.as_str()), ("pe-2", Some(1), ""), "no topic: its id; packet as a string reads too");
    assert_eq!(index["pe-3"].lead_armed, Some(false), "its lead is a ghost record");
    assert_eq!(index["pe-4"].goal, "", "closed: no goal kept");
    assert_eq!(live_of(&out), HashSet::from(["L1".to_string()]), "the live list is still what follows its own marker");

    // pilead alone (no relay there).
    let only = format!("NORELAY\n__CUE_PILEAD__\nPBIN:/p/pilead\n{PI_LISTING}\n__CUE_LIVE__\n");
    let (bin, index) = parse_remote(&only, "box");
    assert_eq!(bin, "");
    assert!(index.contains_key("pe-1") && index.contains_key("PL1") && !index.contains_key("L1"));
}

#[test]
fn a_machine_without_pilead_lists_exactly_as_before() {
    let relay = r#"BIN:/r/relay
{"leads": [{"session_id": "L1", "project": "p"}], "executors": [{"agent": "relay-executor", "session_id": "e1", "claude_session": "C1", "owner_lead": "L1", "status": "busy"}]}
"#;
    let before = parse_remote(&format!("{relay}__CUE_LIVE__\nL1\n"), "box");
    let without = parse_remote(&format!("{relay}__CUE_PILEAD__\nNOPILEAD\n__CUE_LIVE__\nL1\n"), "box");
    assert_eq!(before, without, "the new marker adds nothing");
    assert_eq!(without.1.keys().cloned().collect::<HashSet<_>>(), HashSet::from(["L1".to_string(), "C1".to_string()]));
    assert_eq!(pilead_bin_of(&format!("{relay}__CUE_PILEAD__\nNOPILEAD\n")), "");
    assert_eq!(pilead_bin_of(relay), "");
    let neither = parse_remote("NORELAY\n__CUE_PILEAD__\nNOPILEAD\n__CUE_LIVE__\n__CUE_TZ__:+0000\n", "box");
    assert!(neither.1.is_empty() && neither.0.is_empty());
    // pilead that printed something that isn't JSON (a traceback, an older CLI): no crews, no error.
    assert!(parse_remote("NORELAY\n__CUE_PILEAD__\nPBIN:/p/pilead\nTraceback (most recent call last):\n__CUE_LIVE__\n", "box").1.is_empty());
}

#[test]
fn only_the_buttons_pilead_has_a_command_for_are_wired_for_its_remote_executors() {
    let _g = GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let out = format!("NORELAY\n__CUE_PILEAD__\nPBIN:/p/pilead\n{PI_LISTING}\n__CUE_LIVE__\n");
    let index = parse_remote(&out, "box").1;
    *crate::locking::lock(&INDEX, "INDEX") = Some(index);
    *crate::locking::lock(&REMOTE_PILEAD, "REMOTE_PILEAD") = Some(HashMap::from([("box".to_string(), "/p/pilead".to_string())]));
    assert_eq!(act("pe-1", "review"), Ok(Act::Send { to: "PL1".into(), text: "/pilead:review pe-1".into() }));
    assert!(act("pe-1", "diff").unwrap_err().contains("can't bring it here"), "no remote diff page for pilead");
    assert_eq!(close_plan("pe-1"), Some(Ok(Cmd { bin: "/p/pilead".into(), args: vec!["close".into(), "pe-1".into()], machine: "box".into() })));
    *crate::locking::lock(&REMOTE_PILEAD, "REMOTE_PILEAD") = None;
    assert!(close_plan("pe-1").unwrap().unwrap_err().contains("can't find pilead's command on box"));
    *crate::locking::lock(&INDEX, "INDEX") = None;
}
