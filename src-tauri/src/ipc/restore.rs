//! At launch (and when a machine is reached): sessions still running in Cue's old terminal (tmux) are moved
//! to hosted, each between turns; waiting cards come back.

use crate::model;
use crate::machines;
use crate::logs::live;
use crate::terminal::focus;
use crate::hub::Hub;
use std::collections::HashSet;
use std::sync::Arc;

/// A Claude Code session in a tmux pane, not hosted: how Cue's own terminal ran sessions before every one was hosted.
pub(crate) fn terminal_chat(o: &model::Origin) -> bool {
    o.harness == "claude" && !o.hosted && !o.tmux_pane.is_empty() && !o.cwd.is_empty()
}

/// Run `f` (blocking) at most `secs`: None when it hasn't finished (its thread keeps going).
pub(crate) fn with_timeout<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static, secs: u64) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(f()); });
    rx.recv_timeout(std::time::Duration::from_secs(secs)).ok()
}

/// This Mac's saved sessions to move at launch: in Cue's terminal (`is_cues` of its pane, name and terminal),
/// their agent still `running`. One whose agent ended needs nothing: resuming it starts it hosted.
pub(crate) fn launch_moves(saved: Vec<crate::sessions::Session>, running: &HashSet<String>, is_cues: impl Fn(&str, &str, &str) -> bool) -> Vec<String> {
    let label = |s: &crate::sessions::Session| if s.name.is_empty() { model::folder_of(&s.origin.cwd) } else { s.name.clone() };
    saved
        .into_iter()
        .filter(|s| s.origin.machine.is_empty() && terminal_chat(&s.origin) && running.contains(&s.origin.session_id) && is_cues(&s.origin.tmux_pane, &label(s), &s.origin.tty))
        .map(|s| s.origin.session_id)
        .collect()
}

/// Between turns: at its prompt (its turn ended, or it never started one), nothing it asked (a permission, a
/// question, a prompt in its terminal) open, and its pane agrees: what was typed into it while Cue was closed
/// started a turn Cue never heard of.
pub(crate) fn between_turns(hub: &Arc<Hub>, o: &model::Origin) -> bool {
    let sid = &o.session_id;
    let idle = hub.saved_sessions().iter().any(|s| &s.origin.session_id == sid && matches!(s.state.as_str(), "waiting" | "stopped"));
    idle && !hub.pending().iter().any(|i| &i.origin.session_id == sid && i.kind != "waiting") && at_rest(o)
}

/// Its pane at rest, from one read of it: no dialog up, no turn running ("esc to interrupt"). What's in its input
/// box isn't asked: a capture can't tell a dimmed suggestion from typed text, and a session held for that would
/// never move; a draft never sent is all that's left behind.
fn at_rest(o: &model::Origin) -> bool {
    let Some(screen) = pane_screen(o) else { return false };
    crate::terminal::dialog::parse(&screen).is_none() && !screen.contains("esc to interrupt")
}

/// What a session's tmux pane shows now, this Mac's or its machine's; None when it can't be read.
fn pane_screen(o: &model::Origin) -> Option<String> {
    if o.machine.is_empty() {
        let mut cmd = std::process::Command::new(focus::tmux_bin());
        let out = focus::capped(cmd.args(["capture-pane", "-p", "-J", "-t", &o.tmux_pane]), std::time::Duration::from_secs(10)).ok()?;
        return out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let m = machines::get(&o.machine)?;
    machines::tmux(&m.host, &["capture-pane", "-p", "-J", "-t", &o.tmux_pane]).ok()
}

/// One pass: each session between turns is moved to hosted (Move to Cue: its agent ended, resumed hosted from
/// the same conversation). Gives back the ones still mid-turn, to try again; one that ended or was moved
/// another way is dropped, and so is one whose move failed (it keeps running where it is).
pub(crate) fn move_between_turns(hub: &Arc<Hub>, sids: Vec<String>) -> Vec<String> {
    let mut left = Vec::new();
    for sid in sids {
        let Some(o) = hub.session_origin(&sid).filter(terminal_chat) else { continue };
        if !between_turns(hub, &o) {
            left.push(sid);
            continue;
        }
        if let Err(e) = super::move_session_to_cue(hub, &sid) {
            eprintln!("cue: moving {sid} from the terminal to hosted failed: {e}");
        }
    }
    left
}

/// How often a session mid-turn is looked at again, and for how long at most (then it's left in tmux: it
/// still runs, and resuming it later starts it hosted).
const RETRY_MS: u64 = 15_000;
const GIVE_UP_MS: u64 = 12 * 60 * 60 * 1000;

/// The sessions some waiter is waiting on to move.
static WAITING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Sessions one waiter holds: let go when it's dropped, however its thread ends (a panic too).
pub(crate) struct Waiting(pub(crate) Vec<String>);

impl Drop for Waiting {
    fn drop(&mut self) {
        crate::locking::lock(&WAITING, "WAITING").retain(|s| !self.0.contains(s));
    }
}

/// Of `sids`, the ones no waiter has yet, held for a new one.
pub(crate) fn wait_for(sids: Vec<String>) -> Waiting {
    let mut w = crate::locking::lock(&WAITING, "WAITING");
    let fresh: Vec<String> = sids.into_iter().filter(|s| !w.contains(s)).collect();
    w.extend(fresh.iter().cloned());
    Waiting(fresh)
}

/// Move `sids` in the background, each once it's between turns. A session already being waited on isn't
/// taken twice.
fn move_when_between_turns(hub: &Arc<Hub>, sids: Vec<String>) {
    let held = wait_for(sids);
    if held.0.is_empty() {
        return;
    }
    let h = hub.clone();
    std::thread::spawn(move || {
        let held = held;
        let mut left = move_between_turns(&h, held.0.clone());
        let mut waited = 0;
        while !left.is_empty() && waited < GIVE_UP_MS {
            crate::clock::sleep_ms(RETRY_MS);
            waited += RETRY_MS;
            left = move_between_turns(&h, left);
        }
    });
}

/// At launch: this Mac's sessions still running in Cue's terminal move to hosted, each between turns.
/// Skipped in a quiet test instance.
pub(crate) fn move_terminal_sessions(hub: &Arc<Hub>) {
    if std::env::var_os("CUE_QUIET").is_some() {
        return;
    }
    // Claude Code's own registry of the sessions running now, read fresh (the watcher hasn't yet).
    live::refresh();
    let running: HashSet<String> = live::claude().into_iter().map(|q| q.session_id).collect();
    let sids = launch_moves(hub.saved_sessions(), &running, focus::pane_is_cues);
    move_when_between_turns(hub, sids);
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

/// The script that says whether a session runs in Cue's terminal on a machine: its agent LIVE and its pane in
/// Cue's tmux session there, still on its terminal `tty` (MOVE), or not (STAY).
pub(crate) fn cues_terminal_script(session_id: &str, pane: &str, tty: &str) -> String {
    format!(
        "[ \"$({live})\" = LIVE ] && [ \"$(tmux display-message -p -t {pane} '#{{session_name}}|#{{pane_tty}}' 2>/dev/null)\" = {want} ] && echo MOVE || echo STAY",
        live = agent_running_script(session_id),
        pane = focus::shq(pane),
        want = focus::shq(&format!("{}|{tty}", focus::TMUX_SESSION))
    )
}

/// A machine reached (Cue started, or the machine came back): its sessions still running in Cue's tmux there
/// move to hosted on it, each between turns. One pass per machine at a time; each check time-boxed at 8 s.
pub(crate) fn move_machine_sessions(hub: &Arc<Hub>, machine: &str) {
    if std::env::var_os("CUE_QUIET").is_some() {
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
    let mut sids = Vec::new();
    // One with no terminal recorded can't be told from another pane with its id: left where it is.
    for s in hub.saved_sessions().into_iter().filter(|s| s.origin.machine == machine && terminal_chat(&s.origin) && !s.origin.tty.is_empty()) {
        let (mn, sid, pane, tty) = (machine.to_string(), s.origin.session_id.clone(), s.origin.tmux_pane.clone(), s.origin.tty.clone());
        let said = with_timeout(move || machines::get(&mn).map(|m| machines::run(&m.host, &cues_terminal_script(&sid, &pane, &tty))), 8);
        match said {
            Some(Some(Ok(out))) if out.trim() == "MOVE" => sids.push(s.origin.session_id),
            Some(Some(Err(e))) => machines::hooks::log_raw(machine, &format!("move {} skipped: {e}", s.origin.session_id)),
            None => machines::hooks::log_raw(machine, &format!("move {} check timed out", s.origin.session_id)),
            _ => {}
        }
    }
    move_when_between_turns(hub, sids);
    crate::locking::lock(&BUSY, "BUSY").retain(|m| m != machine);
}

/// Launch: every session saved as waiting comes back with its card, however it lost it. The card
/// is the turn it finished, and its saved thread still has it.
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

    /// The machine check, run here with a stand-in tmux on PATH and Claude Code's registry in a folder of its own.
    #[test]
    fn a_machine_session_moves_only_live_in_cues_tmux_on_its_own_terminal() {
        let home = std::env::temp_dir().join(format!("cue-cues-term-{}", std::process::id()));
        let (reg, bin) = (home.join("sessions"), home.join("bin"));
        std::fs::create_dir_all(&reg).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let tmux = bin.join("tmux");
        std::fs::write(&tmux, "#!/bin/sh\ncase \"$4\" in %1) echo 'cue|/dev/pts/3' ;; %2) echo 'mine|/dev/pts/4' ;; *) exit 1 ;; esac\n").unwrap();
        std::fs::set_permissions(&tmux, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        std::fs::write(reg.join(format!("{}.json", std::process::id())), r#"{"sessionId":"live-1"}"#).unwrap();
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
        let run = |sid: &str, pane: &str, tty: &str| {
            let out = std::process::Command::new("/bin/sh").env("CLAUDE_CONFIG_DIR", &home).env("PATH", &path).args(["-c", &cues_terminal_script(sid, pane, tty)]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(run("live-1", "%1", "/dev/pts/3"), "MOVE");
        assert_eq!(run("live-1", "%1", "/dev/pts/9"), "STAY", "its pane id now on another terminal");
        assert_eq!(run("live-1", "%2", "/dev/pts/4"), "STAY", "a tmux session of your own");
        assert_eq!(run("gone-1", "%1", "/dev/pts/3"), "STAY", "its agent ended");
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn terminal_chat_is_a_claude_session_in_a_pane_not_hosted() {
        assert!(terminal_chat(&model::Origin { harness: "claude".into(), tmux_pane: "%1".into(), cwd: "/tmp/x".into(), ..Default::default() }));
        assert!(!terminal_chat(&model::Origin { harness: "codex".into(), tmux_pane: "%1".into(), cwd: "/tmp/x".into(), ..Default::default() }), "only claude has --resume");
        assert!(!terminal_chat(&model::Origin { harness: "claude".into(), cwd: "/tmp/x".into(), ..Default::default() }), "the Apple Terminal one: no pane");
        assert!(!terminal_chat(&model::Origin { harness: "claude".into(), tmux_pane: "%1".into(), cwd: "/tmp/x".into(), hosted: true, ..Default::default() }), "hosted already");
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
