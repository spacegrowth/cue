//! The in-memory queue. Everything that changes state goes through here, and every change
//! pushes a fresh snapshot to the window, updates the dock badge, and (for new items) notifies.

use crate::model::*;
use crate::transcript;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::oneshot;

mod activity;
mod asks;
mod history;
mod reviews;
mod send;
mod sessions;

pub use history::{split_turn, summary};

/// What an open Ask connection is told.
pub enum Reply {
    Decision(Decision),
    /// Answered elsewhere / session gone: the client should exit without deciding.
    Cancel(String),
}

#[derive(Default)]
struct Store {
    /// Each Claude Code session's cost so far, from its status line (shown in the chat's tab).
    costs: HashMap<String, f64>,
    items: Vec<Item>,
    history: VecDeque<Item>,
    /// Every answer of the last day, newest first (History's today numbers; the list above is capped).
    answers: Vec<Answer>,
    waiters: HashMap<String, oneshot::Sender<Reply>>,
    next: u64,
    /// When each session last started a turn — how a typed reply is confirmed as submitted.
    active_at: HashMap<String, u64>,
    /// Every live session and its recent activity (the Working column and the 30-minute bars).
    sessions: crate::sessions::Sessions,
    /// Sessions you parked (stopped, kept to resume), the latest parked first.
    parked: Vec<crate::sessions::Parked>,
    /// Cards with a notification in Notification Center — taken back once the card is gone.
    notified: HashSet<String>,
    /// Sessions that take replies directly (Pi's extension): session id → (connection number, channel).
    subscribers: HashMap<String, (u64, tokio::sync::mpsc::UnboundedSender<Value>)>,
    next_sub: u64,
    /// The same sessions' origins and when they connected: a Pi session subscribes the moment it
    /// starts, so the Sessions view lists it before its first turn.
    subscribed: HashMap<String, (Origin, u64)>,
    /// The commands each subscribed session reported (its "/" menu), as Cue shows them.
    agent_commands: HashMap<String, Vec<crate::commands::Cmd>>,
    /// The plan's usage from the status line (None until a status line forwards it).
    rates: Option<crate::usage::Rates>,
    /// Limits already announced (scope + reset), so each one notifies once going out and once coming back.
    told_out: HashSet<String>,
    told_back: HashSet<String>,
    /// Reviews typed into a lead: executor → (its lead, when). "in review" until it's no longer done
    /// (15 minutes at most). A lead takes one review at a time: Review waits until it's free again.
    review_sent: HashMap<String, (String, u64)>,
    /// When a review was last typed into each lead: it takes a moment to show as working.
    review_last: HashMap<String, u64>,
    /// Reviews asked for while their lead was busy, oldest first: each goes when its lead is free,
    /// one at a time. Saved, so a restart doesn't drop them.
    review_queue: Vec<QueuedReview>,
    /// Claude sessions missing from Claude Code's own list of running sessions, and since when: gone
    /// for GONE_AFTER_MS, they're ended in Cue (see `sweep`).
    missing: HashMap<String, u64>,
    /// Your machines Cue couldn't reach on its last try: their sessions say so, and × removes them.
    unreachable: HashSet<String>,
}

/// A review waiting for its lead.
#[derive(Clone)]
struct QueuedReview {
    exec: String,
    lead: String,
    text: String,
    at_ms: u64,
}

pub struct Hub {
    store: Mutex<Store>,
    app: Option<AppHandle>,
}

impl Hub {
    pub fn new(app: Option<AppHandle>) -> Self {
        let mut st = Store::default();
        st.history = crate::db::history(crate::config::history_keep());
        st.answers = crate::db::answers_since(now_ms().saturating_sub(crate::db::DAY_MS));
        let (sessions, waiting) = crate::db::live();
        st.sessions.restore(sessions);
        st.items = waiting;
        st.parked = crate::db::parked();
        st.review_queue = crate::db::review_queue().into_iter().map(|(exec, lead, text, at_ms)| QueuedReview { exec, lead, text, at_ms }).collect();
        // Ids keep counting from the clock so they never collide with ids in saved history.
        st.next = now_ms();
        Self { store: Mutex::new(st), app }
    }

    fn new_item(st: &mut Store, kind: &str, origin: Origin) -> Item {
        st.next += 1;
        Item {
            id: format!("n{}", st.next),
            kind: kind.into(),
            project: crate::model::project_of_origin(&origin),
            origin,
            tool_name: String::new(),
            tool_input: Value::Null,
            suggestions: Value::Null,
            context: vec![],
            message: String::new(),
            followup: String::new(), interrupted: false, back_ms: None,
            images: vec![],
            created_ms: now_ms(),
            status: "pending".into(),
            outcome: String::new(),
            resolved_ms: None,
            thread: vec![],
            tool_use_id: None,
            scan_from: 0,
            at_prompt: false,
        }
    }

    /// A session starts listening for direct replies. Returns its connection number for unsubscribe.
    /// `peer`: the connecting process, when the socket can say. The session's own process (Pi's extension
    /// runs inside it) is the only one that may subscribe as it: a second subscriber would get every
    /// message you send the session, and the session nothing. So while a subscription is open, another
    /// process can't replace it, and a claimed `agent_pid` must be the peer itself.
    pub fn subscribe(&self, origin: &Origin, peer: Option<i32>) -> Result<(u64, tokio::sync::mpsc::UnboundedReceiver<Value>), String> {
        if let (Some(p), Some(claimed)) = (peer, origin.agent_pid) {
            if p != claimed {
                return Err(format!("subscribe refused: process {p} says it's {claimed}"));
            }
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let n = {
            let mut st = self.st();
            let open = st.subscribers.get(&origin.session_id).is_some_and(|(_, tx)| !tx.is_closed());
            if open {
                let was = st.subscribed.get(&origin.session_id).and_then(|(o, _)| o.agent_pid);
                if was.is_some() && was != peer {
                    return Err(format!("subscribe refused: another process already listens as session {}", origin.session_id));
                }
            }
            st.next_sub += 1;
            let n = st.next_sub;
            st.subscribers.insert(origin.session_id.clone(), (n, tx));
            st.subscribed.insert(origin.session_id.clone(), (origin.clone(), now_ms()));
            n
        };
        self.redraw();
        Ok((n, rx))
    }

    /// Every agent process Cue knows, by session: the sessions' own, and the ones only subscribed so far.
    pub fn agent_pids(&self) -> Vec<(String, i32)> {
        let st = self.st();
        let mut v = st.sessions.pids();
        v.extend(st.subscribed.values().filter_map(|(o, _)| o.agent_pid.map(|p| (o.session_id.clone(), p))));
        v
    }

    /// Drop a listener — only if it's still the same connection (a reconnect may have replaced it).
    pub fn unsubscribe(&self, session_id: &str, n: u64) {
        let mut st = self.st();
        if st.subscribers.get(session_id).is_some_and(|(m, _)| *m == n) {
            st.subscribers.remove(session_id);
            st.subscribed.remove(session_id);
            drop(st);
            self.redraw();
        }
    }

    pub fn get(&self, id: &str) -> Option<Item> {
        let st = self.st();
        st.items.iter().chain(st.history.iter()).find(|i| i.id == id).cloned()
    }

    pub fn pending(&self) -> Vec<Item> {
        self.st().items.clone()
    }

    /// (session id, agent pid) for every live session — the watcher drops the ones whose agent exited.
    pub fn session_pids(&self) -> Vec<(String, i32)> {
        self.st().sessions.pids()
    }

    pub fn snapshot(&self) -> Value {
        // What the files say (each Claude session's transcript tail, each folder's git HEAD) is read
        // with the store unlocked: this runs on the window's thread and on every change, and a slow disk
        // mustn't hold every hook and tick behind it.
        let (known, live, transcripts, cwds) = self.snapshot_sources();
        // Your latest request to each Claude session, from its transcript: no model.
        let about: HashMap<String, Value> = transcripts.into_iter()
            .filter_map(|(sid, path)| { let prompt = crate::live::about(&path); (!prompt.is_empty()).then(|| (sid, json!({ "prompt": prompt }))) })
            .collect();
        // Each folder's repository, where it's in one: its name (the project) and main checkout (where + New starts).
        let projects: HashMap<String, Value> = cwds.iter()
            .filter_map(|c| crate::live::repo_root(c).map(|r| (c.clone(), json!({ "name": crate::live::repo_name(&r), "root": r.to_string_lossy() }))))
            .collect();
        let branches: HashMap<String, String> = cwds.into_iter().map(|c| { let b = crate::live::branch(&c); (c, b) }).collect();
        let crew = crate::leads::view(&known.iter().cloned().chain(live.iter().map(|q| q.session_id.clone())).collect());
        let mut st = self.st();
        let reviews = Self::reviews_view(&mut st);
        json!({
            "items": st.items,
            "history": st.history,
            // Every answer of the last day, in brief: today's numbers count these, not the History list.
            "answers": st.answers,
            "sessions": st.sessions.snapshot(),
            "parked": st.parked,
            "reviews": reviews,
            "review_queue": Self::review_queue_view(&st),
            "unreachable": st.unreachable,
            "crew": crew,
            "live": live,
            "branches": branches,
            "projects": projects,
            "about": about,
            "window_ms": crate::sessions::WINDOW_MS,
            "now_ms": now_ms(),
            "settings": crate::config::effective(),
            "connections": crate::config::connections(),
            "usage": st.rates,
            "usage_file": crate::usage_file::read(),
        })
    }

    /// For `snapshot`, under one short lock: the session ids Cue knows, the live sessions it hasn't heard
    /// from yet (quiet Claude ones from its registry, Pi ones that only subscribed), each Claude session's
    /// transcript, and every folder in play.
    fn snapshot_sources(&self) -> (HashSet<String>, Vec<crate::live::Quiet>, Vec<(String, String)>, Vec<String>) {
        let st = self.st();
        let known: HashSet<String> = st.sessions.all().iter().map(|s| s.origin.session_id.clone()).chain(st.items.iter().map(|i| i.origin.session_id.clone())).collect();
        let pi = st.subscribed.values().map(|(o, at)| crate::live::Quiet {
            session_id: o.session_id.clone(),
            harness: o.harness.clone(),
            name: String::new(),
            cwd: o.cwd.clone(),
            pid: o.agent_pid.unwrap_or(0),
            status: "idle".into(),
            waiting_for: String::new(),
            since_ms: *at,
        });
        let live: Vec<crate::live::Quiet> = crate::live::claude().into_iter().chain(pi).filter(|q| !known.contains(&q.session_id)).collect();
        let transcripts: Vec<(String, String)> = st.sessions.all().iter().filter(|s| s.origin.harness == "claude").map(|s| (s.origin.session_id.clone(), s.origin.transcript_path.clone()))
            .chain(live.iter().filter(|q| q.harness == "claude").filter_map(|q| crate::live::claude_transcript(&q.cwd, &q.session_id).map(|p| (q.session_id.clone(), p))))
            .collect();
        // Folders on this Mac only. Another machine's folder read here would be a lookup of its path on
        // this disk: "/home/…" is macOS's automounter, which can stall a stat for good (it stalled a launch).
        let remote: HashSet<&str> = st.subscribed.values().filter(|(o, _)| !o.machine.is_empty()).map(|(o, _)| o.session_id.as_str()).collect();
        let cwds: Vec<String> = st.sessions.all().iter().filter(|s| s.origin.machine.is_empty()).map(|s| s.origin.cwd.clone())
            .chain(live.iter().filter(|q| !remote.contains(q.session_id.as_str())).map(|q| q.cwd.clone()))
            .filter(|c| !c.is_empty()).collect::<HashSet<_>>().into_iter().collect();
        (known, live, transcripts, cwds)
    }

    /// This hub as the app holds it, for work handed to another thread (none in tests: no app).
    /// The store, locked. A panic on another thread while it held the lock poisons it; what's in it is
    /// still whole (each change is made under one lock), so Cue goes on with it rather than panicking
    /// at every lock from then on, which would leave the window up and nothing behind it.
    fn st(&self) -> std::sync::MutexGuard<'_, Store> {
        crate::locking::lock(&self.store, "the hub's store")
    }

    fn shared(&self) -> Option<Arc<Hub>> {
        self.app.as_ref().map(|a| a.state::<Arc<Hub>>().inner().clone())
    }

    /// Something outside the store changed what the window shows (who leads whom): redraw only.
    pub fn redraw(&self) {
        if let Some(app) = &self.app {
            let _ = app.emit("state", self.snapshot());
        }
    }

    fn changed(&self) {
        let Some(app) = &self.app else { return };
        {
            let mut st = self.st();
            // A parked session that's live again (you resumed it yourself, in a terminal) isn't parked.
            let back: Vec<String> = st.parked.iter().map(|p| p.session.origin.session_id.clone()).filter(|sid| st.sessions.origin(sid).is_some()).collect();
            if !back.is_empty() {
                st.parked.retain(|p| !back.contains(&p.session.origin.session_id));
                for sid in &back {
                    crate::db::unpark(sid);
                }
            }
            // The write itself with the store unlocked: it's every session and thread, and on this
            // thread (often the window's) nothing else must wait for it.
            let sessions = st.sessions.all();
            let waiting: Vec<Item> = st.items.iter().filter(|i| i.kind == "waiting").cloned().collect();
            drop(st);
            crate::db::sync_live(&sessions, &waiting.iter().collect::<Vec<_>>());
        }
        let _ = app.emit("state", self.snapshot());
        let (decisions, items, answered) = {
            let mut st = self.st();
            let live: HashSet<String> = st.items.iter().map(|i| i.id.clone()).collect();
            // Answered anywhere (Cue, menu bar, the terminal): take the notification back.
            let answered: Vec<String> = st.notified.difference(&live).cloned().collect();
            for id in &answered {
                st.notified.remove(id);
            }
            (st.items.iter().filter(|i| i.kind != "waiting").count(), st.items.clone(), answered)
        };
        crate::notify::remove(&answered);
        if std::env::var_os("CUE_QUIET").is_none() {
            crate::tray::sync(app, &items);
        }
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_badge_count(if decisions > 0 { Some(decisions as i64) } else { None });
        }
    }

    /// `category`: "turn" puts Reply and the quick phrases on it; "turn-full" adds Compact.
    fn notify(&self, id: &str, title: &str, body: &str, category: &str) {
        let Some(app) = &self.app else { return };
        // Test instances run with CUE_QUIET=1 so they never ping you.
        if std::env::var_os("CUE_QUIET").is_some() {
            return;
        }
        // Cue is the window in front of you: the new item is already on screen in Waiting.
        if app.get_webview_window("main").is_some_and(|w| w.is_focused().unwrap_or(false) && w.is_visible().unwrap_or(false)) {
            return;
        }
        if crate::notify::post(id, title, body, category) {
            self.st().notified.insert(id.to_string());
        } else {
            // Unbundled dev binary: a plain notification Cue can't take back.
            let _ = app.notification().builder().title(title).body(body).show();
        }
    }
}
