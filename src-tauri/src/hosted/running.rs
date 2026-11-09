//! The hosted sessions running now, by session id, behind the `Hosted` port the hub reaches them through
//! (hub/ports.rs; testing::FakeHosted stands in for it). Each session reports through the function it was
//! started with; a session closed on purpose doesn't report its exit, since whoever closed it already knows.

use super::{Answer, HostedEvent, Session, Spec};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// How long a request of Cue's to an agent waits for its answer (a sign-in link can take a few seconds to make).
pub const CONTROL_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// What a session reports to, from its own thread: each event with the conversation's id.
pub type Report = Box<dyn Fn(&str, HostedEvent) + Send + Sync>;

/// A session started: its process, and the conversation's id (the one it was started under, or the one the
/// agent picked: Codex's) and file, when the agent says where it keeps it.
#[derive(Clone, Debug)]
pub struct Started {
    pub pid: i32,
    pub session_id: String,
    pub transcript: Option<String>,
    /// Mid-turn (a session picked up after Cue started again, at work while Cue was away).
    pub in_turn: bool,
}

/// Starting, talking to and closing hosted sessions.
pub trait Hosted: Send + Sync {
    /// Start (or resume, by `spec.start`) a session reporting to `report`: its process id. One already
    /// running under that id is closed first: two processes on one conversation would both write its transcript.
    fn start(&self, spec: &Spec, report: Report) -> Result<Started, String>;
    fn send(&self, session_id: &str, text: &str) -> Result<(), String>;
    fn answer(&self, session_id: &str, request_id: &str, answer: &Answer) -> Result<(), String>;
    fn interrupt(&self, session_id: &str) -> Result<(), String>;
    /// A request of Cue's to its agent itself (Claude Code's control request: `{"subtype": "mcp_status"}`), and the
    /// agent's answer. Blocking, up to `CONTROL_WAIT`: call it off the main thread.
    fn control(&self, _session_id: &str, _body: serde_json::Value) -> Result<serde_json::Value, String> {
        Err("this session takes no requests of Cue's".into())
    }
    /// Close it and return once it's gone. Blocking: up to a few seconds for one that won't end.
    fn close(&self, session_id: &str);
    /// Whether its process runs.
    fn running(&self, session_id: &str) -> bool;
    /// Close every one (Cue quitting).
    fn close_all(&self);
    /// The processes hosted sessions ran as under a Cue that's gone, ended if they still run on their own (that
    /// Cue was killed before it could close them): before any is started again, so two never share a conversation.
    fn end_left(&self, pids: &[i32]);
    /// The sessions that end if Cue quits: those it runs as its own children (cue-host couldn't be reached when they
    /// started), by the conversation's id.
    fn in_cue(&self) -> Vec<String>;
    /// A session started under an earlier Cue that cue-host kept, picked up again. None: the host has nothing for it
    /// (the way back is a new start).
    fn reattach(&self, _spec: &Spec, _report: Report) -> Option<Reattached> {
        None
    }
    /// Every session Cue knows has been picked up (`reattach`): what's left over can be let go.
    fn settled(&self) {}
    /// Every host ended, and every agent it runs (Settings → Agents: Restart cue-host): what it did, or why not.
    fn restart_host(&self) -> Result<String, String> {
        Err("Cue doesn't run sessions under cue-host here".into())
    }
    /// The hosts, as Settings shows them (null: none).
    fn host_status(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
}

/// A session picked up from cue-host: its process when it still runs (`started`), and `go`, which reports the asks
/// it still waits on and then what it said while no Cue was there and says from now on, to the session's report.
/// Called once whoever picked it up has noted its process, so nothing reported finds it without one.
pub struct Reattached {
    pub started: Option<Started>,
    pub go: Box<dyn FnOnce() + Send>,
}

struct Entry {
    session: Session,
    /// Closed on purpose: its exit isn't reported.
    closing: Arc<AtomicBool>,
}

/// The real sessions: Claude Code processes on this Mac.
#[derive(Default)]
pub struct Running {
    sessions: Mutex<HashMap<String, Arc<Entry>>>,
}

impl Running {
    fn get(&self, session_id: &str) -> Result<Arc<Entry>, String> {
        let entry = crate::locking::lock(&self.sessions, "hosted sessions").get(session_id).cloned();
        entry.filter(|e| !e.session.gone()).ok_or_else(|| "it isn't running (send it a message to start it again)".to_string())
    }

    /// Taken out of the list, then closed, with the list let go: closing waits for the process.
    fn take_and_close(&self, session_id: &str) {
        let entry = crate::locking::lock(&self.sessions, "hosted sessions").remove(session_id);
        if let Some(e) = entry {
            e.closing.store(true, Ordering::SeqCst);
            e.session.close();
        }
    }
}

impl Hosted for Running {
    fn start(&self, spec: &Spec, report: Report) -> Result<Started, String> {
        self.take_and_close(spec.start.session_id());
        let closing = Arc::new(AtomicBool::new(false));
        let quiet = closing.clone();
        let session = Session::start(spec, move |id, e| {
            if !(matches!(e, HostedEvent::Exited { .. }) && quiet.load(Ordering::SeqCst)) {
                report(id, e);
            }
        })?;
        let started = Started { pid: session.pid(), session_id: session.session_id(), transcript: session.transcript(), in_turn: false };
        crate::locking::lock(&self.sessions, "hosted sessions").insert(started.session_id.clone(), Arc::new(Entry { session, closing }));
        Ok(started)
    }

    fn send(&self, session_id: &str, text: &str) -> Result<(), String> {
        self.get(session_id)?.session.send(text)
    }

    fn answer(&self, session_id: &str, request_id: &str, answer: &Answer) -> Result<(), String> {
        self.get(session_id)?.session.answer(request_id, answer)
    }

    fn interrupt(&self, session_id: &str) -> Result<(), String> {
        self.get(session_id)?.session.interrupt()
    }

    fn control(&self, session_id: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
        self.get(session_id)?.session.control(body, CONTROL_WAIT)
    }

    fn close(&self, session_id: &str) {
        self.take_and_close(session_id);
    }

    fn running(&self, session_id: &str) -> bool {
        self.get(session_id).is_ok()
    }

    fn in_cue(&self) -> Vec<String> {
        crate::locking::lock(&self.sessions, "hosted sessions").iter().filter(|(_, e)| !e.session.gone()).map(|(id, _)| id.clone()).collect()
    }

    fn end_left(&self, pids: &[i32]) {
        let Ok(out) = std::process::Command::new("/bin/ps").args(["-axww", "-o", "pid=,ppid=,comm="]).output() else { return };
        for pid in left_running(&String::from_utf8_lossy(&out.stdout), pids) {
            eprintln!("cue: ending hosted agent {pid}, left running by a Cue that's gone");
            // SAFETY: a plain signal to one process id.
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }

    fn close_all(&self) {
        let all: Vec<Arc<Entry>> = crate::locking::lock(&self.sessions, "hosted sessions").drain().map(|(_, e)| e).collect();
        // All at once: each may take a few seconds to end.
        let closing: Vec<_> = all
            .into_iter()
            .map(|e| {
                e.closing.store(true, Ordering::SeqCst);
                std::thread::spawn(move || e.session.close())
            })
            .collect();
        for c in closing {
            let _ = c.join();
        }
    }
}

/// Hosted Claudes a Cue that's gone left running: Cue was stopped before it could close them (killed, or it
/// crashed). Each goes on to the end of its turn, its input gone, and a message to its session would start a
/// second one on the same conversation. Ended as Cue starts, by exact process id, before any is resumed.
pub fn end_leftovers() {
    let Ok(out) = std::process::Command::new("/bin/ps").args(["-axwwE", "-o", "pid=,ppid=,command="]).output() else { return };
    for pid in leftovers(&String::from_utf8_lossy(&out.stdout)) {
        eprintln!("cue: ending hosted Claude {pid}, left running by a Cue that's gone");
        // SAFETY: a plain signal to one process id.
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

/// From `ps -axwwE -o pid=,ppid=,command=`: the hosted Claudes whose Cue is gone. Only Claude itself as Cue
/// starts it (headless, asking Cue over its input), marked by Cue, and adopted by launchd (its Cue ended):
/// whatever a hosted Claude ran inherits the mark, and a background job of one is never matched.
fn leftovers(ps: &str) -> Vec<i32> {
    ps.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let pid: i32 = w.next()?.parse().ok()?;
            let ppid: i32 = w.next()?.parse().ok()?;
            let words: Vec<&str> = w.collect();
            let claude = words.first().is_some_and(|c| *c == "claude" || c.ends_with("/claude"));
            let hosted = words.windows(2).any(|p| p == ["--permission-prompt-tool", "stdio"]) && words.windows(2).any(|p| p == ["--input-format", "stream-json"]);
            let marked = words.iter().any(|x| *x == format!("{}=1", super::HOSTED_ENV));
            (ppid == 1 && claude && hosted && marked).then_some(pid)
        })
        .collect()
}

/// From `ps -axww -o pid=,ppid=,comm=`: which of `pids` (hosted sessions' processes under the Cue before) still
/// run as an agent on their own: adopted by launchd (their Cue ended), and Claude Code or Pi by name, so a process
/// id since reused by something else is never matched. (Pi names its process "pi", with nothing else to tell it by.)
fn left_running(ps: &str, pids: &[i32]) -> Vec<i32> {
    ps.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let pid: i32 = w.next()?.parse().ok()?;
            let ppid: i32 = w.next()?.parse().ok()?;
            let name = w.next()?.rsplit('/').next()?;
            (pids.contains(&pid) && ppid == 1 && matches!(name, "claude" | "pi")).then_some(pid)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{left_running, leftovers};

    #[test]
    fn a_process_a_hosted_session_ran_as_is_ended_only_if_it_is_still_that_agent_on_its_own() {
        let ps = "  201     1 pi\n  202   900 pi\n  203     1 /opt/homebrew/bin/claude\n  204     1 node\n  205     1 pi";
        assert_eq!(left_running(ps, &[201, 202, 203, 204]), vec![201, 203], "202: its Cue still runs; 204: the id is something else's now; 205: not one of Cue's");
    }

    #[test]
    fn only_a_hosted_claude_whose_cue_is_gone_is_a_leftover() {
        let hosted = "claude -p --input-format stream-json --output-format stream-json --verbose --permission-prompts host --permission-prompt-tool stdio --resume abc";
        let ps = [
            format!("  101     1 {hosted} HOME=/u CUE_HOSTED=1 PATH=/bin"),
            // Its Cue still runs: Cue's own.
            format!("  102   900 {hosted} CUE_HOSTED=1"),
            // Not Cue's: an editor's Claude, headless the same way.
            format!("  103     1 {hosted} HOME=/u"),
            // Something a hosted Claude ran, left behind: inherits the mark, never matched.
            "  104     1 /bin/bash -c nohup ./install.sh --permission-prompt-tool stdio --input-format stream-json CUE_HOSTED=1".to_string(),
            // A Claude in a terminal.
            "  105     1 claude --resume abc CUE_HOSTED=1".to_string(),
            format!("  106     1 /opt/homebrew/bin/claude -p --input-format stream-json --permission-prompt-tool stdio CUE_HOSTED=1"),
        ]
        .join("\n");
        assert_eq!(leftovers(&ps), vec![101, 106]);
    }
}
