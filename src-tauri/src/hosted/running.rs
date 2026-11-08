//! The hosted sessions running now, by session id, behind the `Hosted` port the hub reaches them through
//! (hub/ports.rs; testing::FakeHosted stands in for it). Each session reports through the function it was
//! started with; a session closed on purpose doesn't report its exit, since whoever closed it already knows.

use super::{Answer, HostedEvent, Session, Spec};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// What a session reports to, from its own thread.
pub type Report = Box<dyn Fn(HostedEvent) + Send + Sync>;

/// Starting, talking to and closing hosted sessions.
pub trait Hosted: Send + Sync {
    /// Start (or resume, by `spec.start`) a session reporting to `report`: its process id. One already
    /// running under that id is closed first: two processes on one conversation would both write its transcript.
    fn start(&self, spec: &Spec, report: Report) -> Result<i32, String>;
    fn send(&self, session_id: &str, text: &str) -> Result<(), String>;
    fn answer(&self, session_id: &str, request_id: &str, answer: &Answer) -> Result<(), String>;
    fn interrupt(&self, session_id: &str) -> Result<(), String>;
    /// Close it and return once it's gone. Blocking: up to a few seconds for one that won't end.
    fn close(&self, session_id: &str);
    /// Whether its process runs.
    fn running(&self, session_id: &str) -> bool;
    /// Close every one (Cue quitting).
    fn close_all(&self);
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
    fn start(&self, spec: &Spec, report: Report) -> Result<i32, String> {
        let id = spec.start.session_id().to_string();
        self.take_and_close(&id);
        let closing = Arc::new(AtomicBool::new(false));
        let quiet = closing.clone();
        let session = Session::start(spec, move |e| {
            if !(matches!(e, HostedEvent::Exited { .. }) && quiet.load(Ordering::SeqCst)) {
                report(e);
            }
        })?;
        let pid = session.pid();
        crate::locking::lock(&self.sessions, "hosted sessions").insert(id, Arc::new(Entry { session, closing }));
        Ok(pid)
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

    fn close(&self, session_id: &str) {
        self.take_and_close(session_id);
    }

    fn running(&self, session_id: &str) -> bool {
        self.get(session_id).is_ok()
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
