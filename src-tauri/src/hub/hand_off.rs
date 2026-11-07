//! A hand-off note asked for ahead of a waiting session's cache running out (cache_tick found it due), held
//! back wherever typing could land on something of yours.

use super::*;

impl Hub {
    /// Ask a waiting session for its hand-off note before its cache runs out (cache_tick found it due), unless
    /// typing now could land on something of yours: a draft in Cue's box, anything typed in its terminal's
    /// box (or a box Cue can't see: only a tmux pane's screen can be read), or a turn under way. Nor while its
    /// background agents work: the note would be stale in minutes. What held it is counted (insight_events).
    pub(super) fn hand_off_ahead(&self, o: &Origin, tokens: u64, by: u8) {
        let sid = &o.session_id;
        let held = |why: &str| crate::ipc::record_insight(sid, "ahead", "held", why, tokens);
        // A message of yours kept in Cue for it counts too: the ask would go in its place.
        let busy = self.read(|st| st.sessions.state(sid).as_deref() != Some("waiting") || st.sessions.compacting_since(sid) > 0 || st.sessions.held(sid).is_some());
        if busy {
            return held("busy");
        }
        if crate::steps::running_background(&o.transcript_path) > 0 {
            return held("agents");
        }
        if !crate::db::draft_text(&format!("s:{sid}")).trim().is_empty() {
            return held("draft");
        }
        if o.tmux_pane.is_empty() {
            return held("unseen");
        }
        match crate::focus::session_screen(o).ok().as_deref().and_then(crate::focus::box_empty) {
            Some(true) => {}
            Some(false) => return held("typed"),
            None => return held("unseen"),
        }
        if let Err(e) = crate::ipc::handoff_cue_ahead(self, sid, by) {
            held(&e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Claude Code log whose last reply came `ago_ms` ago with `context` tokens, cached for `ttl_s`.
    fn cached_log(path: &std::path::Path, ago_ms: u64, context: u64, ttl_s: u64) {
        crate::testing::Transcript::at(path).cached_reply(ago_ms, context, ttl_s);
    }

    #[test]
    fn a_hand_off_goes_ahead_only_where_you_turned_it_on_and_holds_where_typing_could_hurt() {
        let dir = std::env::temp_dir().join(format!("cue-ahead-tick-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let hour = 3_600_000;
        // A session waiting on you, its log as given: its card is up.
        let waiting = |sid: &str, ago_ms: u64, context: u64, ttl_s: u64| -> Origin {
            let path = dir.join(format!("{sid}.jsonl"));
            cached_log(&path, ago_ms, context, ttl_s);
            let o = Origin { session_id: sid.into(), harness: "claude".into(), cwd: "/x/proj".into(), transcript_path: path.to_string_lossy().into(), ..Default::default() };
            hub.event(o.clone(), "stopped", "Done.".into(), vec![], "");
            o
        };
        let events = |sid: &str| crate::db::insight_events_of(sid).into_iter().filter(|(r, ..)| r == "ahead").map(|(_, a, d)| format!("{a}:{d}")).collect::<Vec<_>>();
        // 60 s left of an hour's cache, 150k (under what Claude Code idle-compacts itself: config::idle_compacts): due. Ticked, but Cue can't see its box (no tmux pane): held, once.
        let a = waiting("a1", hour - 60_000, 150_000, 3600);
        crate::db::ahead_watch("a1", true);
        // Not ticked, Settings off (its default): nothing.
        waiting("b1", hour - 60_000, 150_000, 3600);
        // Ticked, but a 5-minute cache: the button only.
        let c = waiting("c1", 4 * 60_000, 150_000, 300);
        crate::db::ahead_watch("c1", true);
        // Ticked, but 20 minutes left: not yet.
        waiting("d1", 40 * 60_000, 150_000, 3600);
        crate::db::ahead_watch("d1", true);
        // Ticked, a note already written ahead: not again.
        waiting("e1", hour - 60_000, 150_000, 3600);
        crate::db::ahead_watch("e1", true);
        crate::db::handoff_sent("e1", crate::ipc::CUE_HANDOFF, false, crate::ipc::AHEAD_SESSION);
        // Ticked, but it already continued in a fresh session (still open, its cache renewed by the note): never
        // again, a second note would write over where it continued.
        waiting("k1", hour - 60_000, 150_000, 3600);
        crate::db::ahead_watch("k1", true);
        crate::db::handoff_sent("k1", crate::ipc::CUE_HANDOFF, false, crate::ipc::AHEAD_SESSION);
        crate::db::handoff_continued("k1", "k2");
        // Ticked, but it's asking you something now: held as busy.
        let f = waiting("f1", hour - 60_000, 150_000, 3600);
        crate::db::ahead_watch("f1", true);
        hub.write(|st| st.sessions.mark(&f, "deciding", None));
        // Ticked, and you've a draft in Cue's box for it: held.
        waiting("g1", hour - 60_000, 150_000, 3600);
        crate::db::ahead_watch("g1", true);
        crate::db::set_draft("s:g1", "half a thought", &serde_json::json!([]));
        // Ticked, and put off for later (Need to decide): leaving it is when a note pays, so it's still due.
        waiting("l1", hour - 60_000, 150_000, 3600);
        crate::db::ahead_watch("l1", true);
        hub.set_later("l1", true);
        hub.cache_tick();
        assert_eq!(events("l1"), ["held:unseen"], "put off for later, still due");
        assert_eq!(events("a1"), ["held:unseen"], "Cue can't see its box: nothing typed");
        for sid in ["b1", "c1", "d1", "e1", "k1"] {
            assert!(events(sid).is_empty(), "{sid}: {:?}", events(sid));
        }
        assert_eq!(events("f1"), ["held:busy"]);
        assert_eq!(events("g1"), ["held:draft"]);
        hub.cache_tick();
        assert_eq!(events("a1"), ["held:unseen"], "once per idle stretch, not every tick");
        assert!(crate::db::handoff("a1").is_none() && crate::db::handoff("c1").is_none(), "nothing was asked for");
        assert_eq!(crate::db::handoff("k1").map(|k| k.next), Some("k2".to_string()), "where it continued is kept");
        let _ = (a, c);
        // It began working just as the ask went: Cue would keep it for when that turn ends, with you there.
        // Taken back instead: nothing kept, nothing recorded as asked.
        let w = waiting("w1", hour - 60_000, 300_000, 3600);
        hub.mark_state(&w, "working");
        assert_eq!(crate::ipc::handoff_cue_ahead(&hub, "w1", crate::ipc::AHEAD_SESSION), Err("busy".to_string()));
        assert!(hub.read(|st| st.sessions.held("w1")).is_none() && crate::db::handoff("w1").is_none());
        // You pressing Write hand-off now while it works: kept for when it's free, as anything you send.
        assert!(crate::ipc::handoff_cue_ahead(&hub, "w1", crate::ipc::AHEAD_YOU).unwrap().starts_with("kept in Cue"));
        assert_eq!(crate::db::handoff("w1").map(|k| k.ahead), Some(crate::ipc::AHEAD_YOU));

        // In a tmux pane showing Claude Code's box: empty, it's asked for its note (ahead, by its tick); with
        // something typed in it, held and nothing typed.
        if crate::focus::tmux_installed() {
            let pane = |sid: &str, box_line: &str| -> (String, String) {
                let name = format!("cue-ahead-{sid}-{}", std::process::id());
                let script = format!("printf '%s\\n' '────────' '{box_line}' '────────'; exec cat");
                let out = std::process::Command::new(crate::focus::tmux_bin()).args(["new-session", "-d", "-s", &name, "-x", "120", "-y", "20", "-P", "-F", "#{pane_id}", "sh", "-c", &script]).output().unwrap();
                let pane = String::from_utf8_lossy(&out.stdout).trim().to_string();
                std::thread::sleep(std::time::Duration::from_millis(400));
                (name, pane)
            };
            let in_pane = |sid: &str, pane: &str| {
                let path = dir.join(format!("{sid}.jsonl"));
                cached_log(&path, hour - 60_000, 150_000, 3600);
                let o = Origin { session_id: sid.into(), harness: "claude".into(), cwd: "/x/proj".into(), transcript_path: path.to_string_lossy().into(), tmux_pane: pane.into(), ..Default::default() };
                hub.event(o, "stopped", "Done.".into(), vec![], "");
                crate::db::ahead_watch(sid, true);
            };
            let (n1, empty) = pane("t1", "❯ ");
            let (n2, typed) = pane("t2", "❯ yes go ahead");
            in_pane("t1", &empty);
            in_pane("t2", &typed);
            hub.cache_tick();
            let screen = |p: &str| crate::focus::tmux_screen(p).unwrap_or_default();
            assert_eq!(events("t2"), ["held:typed"]);
            assert!(!screen(&typed).contains("Hand off:"), "nothing typed over yours");
            assert_eq!(events("t1"), ["shown:session"]);
            assert!(screen(&empty).contains("Hand off: write a short note") && screen(&empty).contains("HANDOFF-t1.md"), "{}", screen(&empty));
            let h = crate::db::handoff("t1").unwrap();
            assert_eq!((h.skill.as_str(), h.ahead), (crate::ipc::CUE_HANDOFF, crate::ipc::AHEAD_SESSION));
            hub.cache_tick();
            assert_eq!(events("t1"), ["shown:session"], "asked once");
            for n in [n1, n2] {
                let _ = std::process::Command::new(crate::focus::tmux_bin()).args(["kill-session", "-t", &n]).output();
            }
        }
        std::env::remove_var("CUE_HOME");
        crate::db::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
