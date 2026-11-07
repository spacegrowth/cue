//! The World: one hub on a private CUE_HOME, driven as the socket drives it, with its terminals, its
//! follow-up work and its clock in the test's hands. One World at a time per process (CUE_HOME and the
//! agent index are process-wide): it holds the database's and the index's test locks for its life.

use super::{Agent, Deferred, FakeTerminal, Snap};
use crate::clock::fake::Fake;
use crate::hub::ports::{Later, Ports};
use crate::hub::Hub;
use crate::model::ClientMsg;
use crate::server::inbound::{self, Outcome};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

/// How many jobs one `settle` may run before it's taken for work that keeps handing in more of itself.
const SETTLE_MAX: usize = 1000;

/// What a test drives the hub with from any thread: messages as a socket client sends them.
#[derive(Clone)]
pub struct Client {
    pub hub: Arc<Hub>,
    clock: Fake,
}

impl Client {
    /// `msg` (the wire's JSON) applied as a connection applies its first line. A prompt whose sender the
    /// transcript says is settled at once, from the transcript as the test wrote it.
    pub fn drive(&self, msg: Value) -> Outcome {
        let parsed: ClientMsg = serde_json::from_value(msg.clone()).unwrap_or_else(|e| panic!("not a message Cue takes ({e}): {msg}"));
        match inbound::apply(&self.hub, parsed, None) {
            Outcome::Settle(p) => {
                let said = inbound::sender(&p);
                inbound::settle(&self.hub, p, said);
                Outcome::Quiet
            }
            o => o,
        }
    }

    /// `msg`, whose connection says one line back: that line.
    #[track_caller]
    pub fn ask(&self, msg: Value) -> Value {
        match self.drive(msg.clone()) {
            Outcome::Say(v) => v,
            _ => panic!("expected one line back for {msg}"),
        }
    }

    /// Send `text` to a live session (the "say something" box); `now`: ⌘↵.
    pub fn send_to(&self, sid: &str, text: &str, now: bool) -> Value {
        self.ask(json!({"type": "send_to", "session_id": sid, "text": text, "now": now}))
    }

    /// Run `f` on a thread of its own that reads this World's clock.
    pub fn spawn<R: Send + 'static>(&self, f: impl FnOnce(Client) -> R + Send + 'static) -> std::thread::JoinHandle<R> {
        let c = self.clone();
        std::thread::spawn(move || {
            c.clock.install();
            f(c)
        })
    }
}

pub struct World {
    client: Client,
    pub terminal: Arc<FakeTerminal>,
    later: Arc<Deferred>,
    home: PathBuf,
    // Dropped last, after `Drop` has put things back: the index's turn, then the database's.
    _index: std::sync::MutexGuard<'static, ()>,
    _db: std::sync::MutexGuard<'static, ()>,
}

impl World {
    /// A World on a clock that runs ahead of the wall's.
    pub fn new() -> Self {
        Self::with_clock(Fake::default())
    }

    /// A World whose clock starts at `start_ms` and moves only when it's moved: everything it builds comes
    /// out the same each time (the UI fixtures).
    pub fn at(start_ms: u64) -> Self {
        Self::with_clock(Fake::frozen(start_ms))
    }

    fn with_clock(clock: Fake) -> Self {
        let index = crate::leads::test_turn();
        let db = crate::db::test_turn();
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let home = std::env::temp_dir().join(format!("cue-world-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("CUE_HOME", &home);
        crate::db::reset();
        crate::live::set_claude(vec![]);
        crate::leads::install_index(Default::default());
        clock.install();
        let (terminal, later) = (Arc::new(FakeTerminal::default()), Arc::new(Deferred::default()));
        let hub = Arc::new(Hub::with_ports(None, Ports { keys: terminal.clone(), later: Later::Deferred(later.clone()) }));
        let w = Self { client: Client { hub, clock }, terminal, later, home, _index: index, _db: db };
        w.agents_take_what_is_typed();
        w
    }

    /// Typed into a session whose agent takes it, its prompt hook fires: an "active" event, as Claude
    /// Code's UserPromptSubmit sends it (no prompt id: the prompt is yours).
    fn agents_take_what_is_typed(&self) {
        let c = Arc::downgrade(&self.client.hub);
        let clock = self.client.clock.clone();
        self.terminal.on_prompt(move |origin, text| {
            let Some(hub) = c.upgrade() else { return };
            let mut msg = serde_json::to_value(origin).unwrap();
            msg["type"] = json!("event");
            msg["event"] = json!("active");
            msg["message"] = json!(text);
            Client { hub, clock: clock.clone() }.drive(msg);
        });
    }

    pub fn hub(&self) -> &Arc<Hub> {
        &self.client.hub
    }

    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// This World's own folder (its CUE_HOME).
    pub fn home(&self) -> &std::path::Path {
        &self.home
    }

    /// Session `sid`, a Claude Code session until said otherwise.
    pub fn session(&self, sid: &str) -> Agent<'_> {
        Agent::new(self, sid)
    }

    /// `msg` applied as a connection would, then whatever follow-up work is due now.
    pub fn drive(&self, msg: Value) -> Outcome {
        let o = self.client.drive(msg);
        self.settle();
        o
    }

    /// `msg` with its one line back, then the follow-up work due now.
    #[track_caller]
    pub fn ask(&self, msg: Value) -> Value {
        let v = self.client.ask(msg);
        self.settle();
        v
    }

    /// What the window shows, as a client's `list` gets it.
    pub fn snapshot(&self) -> Snap {
        Snap(self.ask(json!({"type": "list"})))
    }

    pub fn send_to(&self, sid: &str, text: &str) -> Value {
        let v = self.client.send_to(sid, text, false);
        self.settle();
        v
    }

    /// ⌘↵: stop its turn if it's working, then send.
    pub fn send_now(&self, sid: &str, text: &str) -> Value {
        let v = self.client.send_to(sid, text, true);
        self.settle();
        v
    }

    /// Reply to card `id` (a finished turn).
    pub fn reply(&self, id: &str, text: &str) -> Value {
        self.ask(json!({"type": "reply", "id": id, "text": text}))
    }

    pub fn interrupt(&self, sid: &str) -> Value {
        self.ask(json!({"type": "interrupt", "session_id": sid}))
    }

    /// Answer card `id` as the window does: `decision` is `{"behavior": …}` and the rest of a Decision.
    pub fn answer(&self, id: &str, decision: Value) -> Value {
        self.ask(json!({"type": "respond", "id": id, "decision": decision}))
    }

    /// Run the follow-up work that's due by the World's clock now, and what that hands in due now too.
    pub fn settle(&self) {
        for _ in 0..SETTLE_MAX {
            let Some(job) = self.later.take_due(crate::clock::now_ms()) else { return };
            job(&self.client.hub);
        }
        panic!("follow-up work keeps handing in more of itself, due at once");
    }

    /// Move the World's clock on by `ms`, running each piece of follow-up work as its time comes.
    pub fn advance(&self, ms: u64) {
        let until = crate::clock::now_ms() + ms;
        while let Some(due) = self.later.next_due().filter(|d| *d <= until) {
            self.client.clock.advance(due.saturating_sub(crate::clock::now_ms()));
            self.settle();
        }
        self.client.clock.advance(until.saturating_sub(crate::clock::now_ms()));
        self.settle();
    }

    /// One tick of the watcher (server::tick), as it runs every second; `usage` as on every tenth.
    pub fn tick(&self, usage: bool) {
        crate::server::tick(&self.client.hub, usage);
        self.settle();
    }

    /// How many pieces of follow-up work wait for their time.
    pub fn waiting_work(&self) -> usize {
        self.later.len()
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        Fake::uninstall();
        crate::db::reset();
        crate::live::set_claude(vec![]);
        crate::leads::install_index(Default::default());
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&self.home);
    }
}
