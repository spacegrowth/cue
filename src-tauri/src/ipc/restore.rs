//! Bringing sessions back at launch (and when a machine is reached): rebuilding panes, restoring cards.

use crate::config;
use crate::leads;
use crate::model;
use crate::machines;
use crate::logs::live;
use crate::terminal::focus;
use crate::hub::Hub;
use std::sync::Arc;

/// Would a reboot have left this session without its pane, so Cue rebuilds it? Claude only:
/// it's the only harness with `--resume`. A session without a pane (a plain Terminal one) is left alone.
pub(crate) fn rebuildable(o: &model::Origin) -> bool {
    o.harness == "claude" && !o.tmux_pane.is_empty() && !o.cwd.is_empty()
}

/// Does Cue bring this session back by itself (at launch, or when its machine is reached again)? A
/// rebuildable one, unless it's a relay or pi-lead executor (`executor`, from their listings): that one
/// belongs to its lead, which resumes, replaces or closes it; a `claude --resume` of Cue's own would be a
/// second terminal on it, one its lead doesn't know about.
pub(crate) fn auto_resumable(o: &model::Origin, executor: impl Fn(&str) -> bool) -> bool {
    rebuildable(o) && !executor(&o.session_id)
}

/// Run `f` (blocking) at most `secs`: None when it hasn't finished (its thread keeps going).
pub(crate) fn with_timeout<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static, secs: u64) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(f()); });
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

/// One session again in tmux, as Resume does: `claude --resume` typed into its new pane,
/// the new pane recorded (its old one died with the reboot). Failures skip it, quietly.
pub(crate) fn rebuild(hub: &Arc<Hub>, o: &model::Origin, label: &str) {
    let Ok(line) = focus::resume_line_local(&o.cwd, &o.session_id) else { return };
    let Ok(tab) = focus::open_in_tmux(&line, &o.cwd, label) else { return };
    let fresh = model::Origin {
        session_id: o.session_id.clone(),
        harness: "claude".into(),
        cwd: o.cwd.clone(),
        transcript_path: o.transcript_path.clone(),
        term_program: tab.term_program,
        tty: tab.tty,
        tmux_pane: tab.tmux_pane,
        ..Default::default()
    };
    hub.mark_state(&fresh, "waiting");
    // Its card comes back too: the turn it finished is still the turn to answer.
    hub.waiting_card(&fresh);
}

/// Machine sessions, each call time-boxed at 8s. A failure goes in machines.log; a host that can't be
/// reached is skipped (Cue can't tell whether its pane survived) until it's reached again.
pub(crate) fn remote_restores(hub: &Arc<Hub>, sessions: Vec<(String, model::Origin, String)>) {
    for (machine, o, name) in sessions {
        let sid = o.session_id.clone();
        let mn = machine.clone();
        let h = hub.clone();
        let done = with_timeout(move || {
            let Some(m) = machines::get(&machine) else { return };
            // Its agent still runs there (Claude Code's registry: <pid>.json naming the session, that
            // pid alive), in Cue's tmux or your own: leave it, a second one would share its conversation.
            match machines::run(&m.host, &agent_running_script(&o.session_id)) {
                Ok(s) if s.trim() == "GONE" => {}
                Ok(_) => return,
                Err(e) => { machines::hooks::log_raw(&machine, &format!("rebuild {} skipped: {e}", o.session_id)); return; }
            }
            let Ok(line) = focus::resume_line(&o.cwd, &o.session_id) else { return };
            let label = if !name.is_empty() { name.clone() } else { model::folder_of(&o.cwd) };
            let (pane, tty, dir) = match machines::open_in_tmux(&m, &line, &o.cwd, &label) {
                Ok(t) => t,
                Err(e) => { machines::hooks::log_raw(&machine, &format!("rebuild {} failed: {e}", o.session_id)); return; }
            };
            let fresh = model::Origin {
                session_id: o.session_id.clone(),
                harness: "claude".into(),
                cwd: dir,
                transcript_path: o.transcript_path.clone(),
                term_program: "tmux".into(),
                tty,
                tmux_pane: pane,
                machine: m.name.clone(),
                ..Default::default()
            };
            h.mark_state(&fresh, "waiting");
            // Its card comes back too: the turn it finished is still the turn to answer.
            h.waiting_card(&fresh);
        }, 8);
        if done.is_none() {
            machines::hooks::log_raw(&mn, &format!("rebuild {sid} timed out"));
        }
    }
}

/// The script that says whether a Claude session's agent runs on a machine: LIVE or GONE.
pub(crate) fn agent_running_script(session_id: &str) -> String {
    format!(
        r#"for f in "${{CLAUDE_CONFIG_DIR:-$HOME/.claude}}"/sessions/*.json; do
  [ -f "$f" ] && grep -qF {sid} "$f" && kill -0 "$(basename "$f" .json)" 2>/dev/null && {{ echo LIVE; exit 0; }}
done
echo GONE"#,
        sid = focus::shq(session_id)
    )
}

/// At launch: a reboot killed every tmux pane, so live sessions' panes are gone. Open each
/// again (its own tmux session, `claude --resume` typed in), like Resume does for one.
/// Skipped in a quiet test instance, or when Settings says sessions don't run in tmux.
pub(crate) fn restore_sessions(hub: &Arc<Hub>) {
    if std::env::var_os("CUE_QUIET").is_some() || !config::tmux_sessions() {
        return;
    }
    // Claude Code's own registry of the sessions running now, read fresh (the watcher hasn't yet), and
    // relay's and pi-lead's crews here (their executors are left to their leads).
    live::refresh();
    leads::refresh();
    let running: std::collections::HashSet<String> = live::claude().into_iter().map(|q| q.session_id).collect();
    for s in hub.saved_sessions() {
        // A machine's sessions come back when Cue reaches the machine (restore_machine), not here.
        if !auto_resumable(&s.origin, leads::is_executor) || !s.origin.machine.is_empty() {
            continue;
        }
        let label = if !s.name.is_empty() { s.name.clone() } else { model::folder_of(&s.origin.cwd) };
        // Its agent still runs (Cue restarted, the Mac didn't): wherever it runs, in Cue's tmux, your
        // own, or a renamed session, a second `claude --resume` would be two agents on one conversation.
        if running.contains(&s.origin.session_id) || focus::pane_in_its_session(&s.origin.tmux_pane, &label) {
            continue;
        }
        rebuild(hub, &s.origin, &label);
    }
}

/// A machine reached (Cue started, or the machine came back from a reboot or a dropped line): its live
/// sessions whose agent no longer runs there are opened again. One pass per machine at a time.
pub(crate) fn restore_machine(hub: &Arc<Hub>, machine: &str) {
    if std::env::var_os("CUE_QUIET").is_some() || !config::tmux_sessions() {
        return;
    }
    static BUSY: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    {
        let mut busy = crate::locking::lock(&BUSY, "BUSY");
        if busy.iter().any(|m| m == machine) {
            return;
        }
        busy.push(machine.to_string());
    }
    restore_machine_now(hub, machine);
    crate::locking::lock(&BUSY, "BUSY").retain(|m| m != machine);
}

pub(crate) fn restore_machine_now(hub: &Arc<Hub>, machine: &str) {
    if !hub.saved_sessions().iter().any(|s| s.origin.machine == machine && rebuildable(&s.origin)) {
        return;
    }
    // Which of them are relay / pi-lead executors: read from the machine first if Cue hasn't yet (it
    // was just reached, the remote poll may not have got to it). If that read fails, the sessions not
    // known as executors (from an earlier read, or from Cue's own record of who drives them) still come
    // back now; the remote poll reads the machine's crews again on its next turn.
    if !leads::remote_known(machine) {
        let Some(m) = machines::get(machine) else { return };
        if leads::refresh_remote(&m.name, &m.host).1.is_none() {
            machines::hooks::log_raw(machine, "couldn't read its crews: rebuilding the sessions not known as executors");
        }
    }
    // Read after the machine's queued events were handled (an "ended" one has forgotten its session).
    let sessions = machine_restores(hub.saved_sessions(), machine, leads::is_executor);
    if !sessions.is_empty() {
        remote_restores(hub, sessions);
    }
}

/// A machine's saved sessions Cue brings back itself: rebuildable ones, minus every relay / pi-lead
/// executor it knows of, whether from the crews it read (`executor`) or from its own record of the
/// session (its hook said its lead drives it).
pub(crate) fn machine_restores(saved: Vec<crate::sessions::Session>, machine: &str, executor: impl Fn(&str) -> bool) -> Vec<(String, model::Origin, String)> {
    saved
        .into_iter()
        .filter(|s| s.origin.machine == machine && !recorded_executor(&s.driven_by) && auto_resumable(&s.origin, &executor))
        .map(|s| (s.origin.machine.clone(), s.origin, s.name))
        .collect()
}

/// Whether Cue's record of a session says a relay or pi-lead lead drives it ("its relay lead (…)",
/// "its pi-lead lead (…)", as the hook and `leads::resolve_driver` write it).
pub(crate) fn recorded_executor(driven_by: &str) -> bool {
    driven_by.starts_with("its relay lead") || driven_by.starts_with("its pi-lead lead")
}

/// Launch: every session saved as waiting comes back with its card, however it lost it. The card
/// is the turn it finished, and its saved thread still has it. (A rebuild makes one right away;
/// anything else that's waiting with no card gets it here.)
pub(crate) fn bring_back_cards(hub: &Arc<Hub>) {
    let pending = hub.pending();
    for s in hub.saved_sessions() {
        if s.state != "waiting" {
            continue;
        }
        if pending.iter().any(|i| i.kind == "waiting" && i.origin.session_id == s.origin.session_id) {
            continue;
        }
        // The saved pid is stale at best (it died with whatever ended its agent); the card
        // must not carry it, or the watcher would take it for the session's end.
        let mut o = s.origin.clone();
        o.agent_pid = None;
        hub.waiting_card(&o);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_and_pilead_executors_are_never_resumed_by_cue_itself() {
        let o = |sid: &str| model::Origin { session_id: sid.into(), harness: "claude".into(), cwd: "/w".into(), tmux_pane: "%3".into(), machine: "box".into(), ..Default::default() };
        let crews = |sid: &str| matches!(sid, "relay-exec" | "pilead-exec");
        assert!(auto_resumable(&o("mine"), crews), "a session of yours comes back");
        assert!(!auto_resumable(&o("relay-exec"), crews), "a relay executor is its lead's");
        assert!(!auto_resumable(&o("pilead-exec"), crews), "so is a pi-lead one");
        assert!(!auto_resumable(&model::Origin { tmux_pane: String::new(), ..o("mine") }, crews), "no pane: nothing to rebuild");
    }

    #[test]
    fn a_machine_whose_crews_cant_be_read_still_gets_its_own_sessions_back() {
        let saved = |sid: &str, machine: &str, driven_by: &str| -> crate::sessions::Session {
            serde_json::from_value(serde_json::json!({
                "session_id": sid, "harness": "claude", "cwd": "/w", "tmux_pane": "%3", "machine": machine, "project": "w",
                "state": "working", "driven_by": driven_by, "queued": null, "since_ms": 1, "prompt": "", "segments": [], "thread": []
            }))
            .unwrap()
        };
        let all = || {
            vec![
                saved("mine", "box", ""),
                saved("helped", "box", "2 background agents"),
                saved("read-exec", "box", ""),
                saved("relay-rec", "box", "its relay lead"),
                saved("pilead-rec", "box", "its pi-lead lead (shop)"),
                saved("elsewhere", "other", ""),
            ]
        };
        let ids = |v: Vec<(String, model::Origin, String)>| v.into_iter().map(|(_, o, _)| o.session_id).collect::<Vec<_>>();
        // The crew read failed and none ever succeeded: Cue's own records still keep its leads'
        // executors out, and every other session on that machine comes back.
        let mut got = ids(machine_restores(all(), "box", |_| false));
        got.sort();
        assert_eq!(got, vec!["helped", "mine", "read-exec"]);
        // An earlier read knew "read-exec" as an executor: that one stays its lead's too.
        let mut got = ids(machine_restores(all(), "box", |sid| sid == "read-exec"));
        got.sort();
        assert_eq!(got, vec!["helped", "mine"]);
    }

    #[test]
    fn a_machine_session_whose_agent_runs_is_left_alone() {
        let home = std::env::temp_dir().join(format!("cue-running-{}", std::process::id()));
        let dir = home.join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        let run = |sid: &str| {
            let out = std::process::Command::new("/bin/sh").env("CLAUDE_CONFIG_DIR", &home).args(["-c", &agent_running_script(sid)]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(run("abc-1"), "GONE", "no registry entry");
        std::fs::write(dir.join(format!("{}.json", std::process::id())), r#"{"pid":1,"sessionId":"abc-1"}"#).unwrap();
        assert_eq!(run("abc-1"), "LIVE", "its pid is alive");
        assert_eq!(run("other"), "GONE", "another session's entry");
        std::fs::write(dir.join("999999.json"), r#"{"sessionId":"dead-1"}"#).unwrap();
        assert_eq!(run("dead-1"), "GONE", "its pid is gone");
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn rebuildable_takes_claude_sessions_with_a_pane_and_a_folder() {
        assert!(rebuildable(&model::Origin { harness: "claude".into(), tmux_pane: "%1".into(), cwd: "/tmp/x".into(), ..Default::default() }));
        assert!(!rebuildable(&model::Origin { harness: "codex".into(), tmux_pane: "%1".into(), cwd: "/tmp/x".into(), ..Default::default() }), "only claude has --resume");
        assert!(!rebuildable(&model::Origin { harness: "claude".into(), cwd: "/tmp/x".into(), ..Default::default() }), "the Apple Terminal one: no pane");
    }

    #[test]
    fn a_waiting_session_comes_back_with_its_card_at_launch() {
        let dir = std::env::temp_dir().join(format!("cue-back-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = model::Origin { session_id: "s1".into(), harness: "claude".into(), cwd: "/x/proj".into(), ..Default::default() };
        let hub = Arc::new(Hub::new(None));
        hub.event(origin.clone(), "stopped", "Here's the fix.".into(), vec![], "");
        let id = hub.pending()[0].id.clone();
        hub.remove_waiting(&id); // its card is gone (however it went)
        bring_back_cards(&hub);
        let items = hub.pending();
        assert_eq!(items.len(), 1, "the waiting session's card is back, from its saved thread");
        assert_eq!(items[0].message, "Here's the fix.");
        assert_eq!(items[0].origin.agent_pid, None, "no stale pid for the watcher to take for its end");
        bring_back_cards(&hub);
        assert_eq!(hub.pending().len(), 1, "twice doesn't stack a second card");
    }
}
