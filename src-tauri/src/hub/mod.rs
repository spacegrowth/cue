//! The in-memory queue. Everything that changes state goes through here; a change is drawn soon after
//! (redraw.rs: the window's state, the dock badge), and new items notify.

use crate::model::*;
use crate::transcript;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

mod activity;
mod asks;
mod cache_tick;
mod events;
mod hand_off;
mod history;
pub(crate) mod ports;
mod projects;
mod redraw;
mod reviews;
mod saves;
mod send;
mod sessions;
mod shown;
mod store_access;
mod turn_end;
mod usage;

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
    /// When Cue last stopped a session's turn itself to send a message now (⌘↵): the interrupt that
    /// leaves in its transcript is Cue's, with your message on its way, not a turn you stopped.
    stopped_to_send: HashMap<String, u64>,
    /// When Cue last pressed Esc in a session for you (its Stop button): Claude Code hands the stopped prompt
    /// back to its box, so a message typed in soon after needs a blank line before it, or the two go as one.
    /// By the forward-only clock (clock::now_mono_ms).
    escaped: HashMap<String, u64>,
    /// Every live session and its recent activity (the Working column and the 30-minute bars).
    sessions: crate::sessions::Sessions,
    /// Sessions you deleted while Cue ran, and when: an answer of theirs from before then, still on its way to
    /// the database (saves.rs), isn't saved.
    purged: HashMap<String, u64>,
    /// Sessions with an ask answered while another of theirs still waited on you: back at work once the last
    /// of them closes, however it closes (asks.rs `finish`).
    answered_meanwhile: HashSet<String>,
    /// Answered cards whose save goes with the next draw's (saves.rs `save_answered_with_draw`).
    unsaved: Vec<history::Answered>,
    /// Sessions you parked (stopped, kept to resume), the latest parked first.
    parked: Vec<crate::sessions::Parked>,
    /// Cards with a notification in Notification Center — taken back once the card is gone. Only cards: other
    /// notifications (notify_about) are taken back by whoever posted them.
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
    /// Sessions told their cache is about to run out while waiting on you (cache_tick's heads-up, by the rule
    /// the card's countdown and the header's "going cold" use too): session → the expiry it was told
    /// about, once per expiry. (Answering in time is counted by the insights themselves: insight_events.)
    cold_warned: HashMap<String, u64>,
    /// Sessions a hand-off note was asked for (or held) ahead of their cache running out, by the expiry it
    /// was for: once per idle stretch.
    ahead_tried: HashMap<String, u64>,
    /// The board's cards on this Mac: their cache's expiry and their context, as of the last cache_tick
    /// (insights.js counts down from these; ipc::insight_states has the same shape).
    insight: HashMap<String, Value>,
    /// Reviews typed into a lead: executor → (its lead, when). "in review" until it's no longer done
    /// (15 minutes at most). A lead takes one review at a time: Review waits until it's free again.
    review_sent: HashMap<String, (String, u64)>,
    /// When a review was last typed into each lead: it takes a moment to show as working.
    review_last: HashMap<String, u64>,
    /// Reviews asked for while their lead was busy, oldest first: each goes when its lead is free,
    /// one at a time. Saved, so a restart doesn't drop them.
    review_queue: Vec<QueuedReview>,
    /// Packets you set aside on their Review card: (relay's executor id, packet). Saved.
    set_aside: HashSet<(String, u64)>,
    /// Claude sessions missing from Claude Code's own list of running sessions, and since when: gone
    /// for GONE_AFTER_MS, they're ended in Cue (see `sweep`).
    missing: HashMap<String, u64>,
    /// Sessions that just ended in Cue, and when: their agent takes a moment to exit after its
    /// SessionEnd, and until it has, it isn't shown again as a session Cue hasn't heard from.
    just_ended: HashMap<String, u64>,
    /// Sessions that ended while Cue ran, newest first, each its record as it was and when it ended: what
    /// the window shows for one from the moment its row goes (its name with it), before the saved copy is
    /// asked for. The saved copy (db) is what a relaunch shows; this is the same list, had sooner.
    ended: VecDeque<(u64, crate::sessions::Session)>,
    /// The same, as each ended, until the next save (redraw.rs save_live) has written it: whatever changed
    /// since the save before its end (a rename just as it ended) reaches Closed after a relaunch.
    unsaved_ends: Vec<(u64, crate::sessions::Session)>,
    /// Your machines Cue couldn't reach on its last try: their sessions say so, and × removes them.
    unreachable: HashSet<String>,
    /// Sessions Cue is restarting (stopped and started again: after an MCP change, or ⋯ → Restart session): see `Restart`.
    restarting: HashMap<String, Restart>,
    /// Each session's last few restarts, oldest first, for a line in its chat at the time it happened.
    /// SHORTCUT: kept while Cue runs, not saved: a relaunch drops the lines (the restart itself is
    /// done). Save them with the session (db) if they should outlive that.
    restarted: HashMap<String, Vec<Value>>,
    /// How many snapshots have been taken from the store (each takes the next number): their order (shown.rs).
    shown_seq: u64,
}

/// What `snapshot` reads files for, from the store: the session ids Cue knows, the live sessions it hasn't
/// heard from yet, each Claude session's transcript, and every folder in play.
type Sources = (HashSet<String>, Vec<crate::live::Quiet>, Vec<(String, String)>, Vec<String>);

/// A restart in progress. Underneath it's a park and a resume, but the window never sees those: the
/// session stays where it is (its record as it was before, `held`, while its agent is down), never on
/// Parked, and its pane is covered until it's back. `step`: "stop", "start", then "load" until its agent
/// is running again.
#[derive(Clone)]
struct Restart {
    step: &'static str,
    since_ms: u64,
    /// The MCP servers it restarts with, by name, when it's pinned to a set (None: its folder's).
    servers: Option<Vec<String>>,
    /// Restarted for its MCP servers (⋯ → MCP servers…), not a plain ⋯ → Restart session.
    mcp: bool,
    held: Option<crate::sessions::Session>,
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
    store: store_access::StoreCell,
    app: Option<AppHandle>,
    /// One gate per session for typing into it: a second message waits until the first has been typed and taken
    /// (or not), so two sent at once never mix in its box (pasted A, pasted B, Enter, Enter → one prompt "AB").
    typing: Mutex<HashMap<String, std::sync::Arc<Mutex<()>>>>,
    /// The window's state as last built, handed out without building it again (shown.rs).
    shown: shown::Shown,
    /// The terminals it types into and where its follow-up work runs (ports.rs).
    ports: ports::Ports,
    /// What keeps saves made after the store is let go in order (saves.rs).
    saves: saves::SaveLocks,
    /// Changes waiting to be drawn, and the worker that draws them (redraw.rs).
    redraws: Arc<redraw::Redraws>,
}

impl Hub {
    pub fn new(app: Option<AppHandle>) -> Self {
        let ports = ports::Ports::real(app.as_ref());
        Self::with_ports(app, ports)
    }

    pub(crate) fn with_ports(app: Option<AppHandle>, ports: ports::Ports) -> Self {
        let mut st = Store::default();
        st.history = crate::db::history(crate::config::history_keep());
        st.answers = crate::db::answers_since(now_ms().saturating_sub(crate::db::DAY_MS));
        let (sessions, waiting) = crate::db::live();
        st.sessions.restore(sessions);
        st.items = waiting;
        st.parked = crate::db::parked();
        st.review_queue = crate::db::review_queue().into_iter().map(|(exec, lead, text, at_ms)| QueuedReview { exec, lead, text, at_ms }).collect();
        st.set_aside = crate::db::set_aside().into_iter().collect();
        // Ids keep counting from the clock so they never collide with ids in saved history.
        st.next = now_ms();
        Self { store: store_access::StoreCell::new(st), app, typing: Mutex::new(HashMap::new()), shown: shown::Shown::default(), ports, saves: saves::SaveLocks::default(), redraws: Arc::default() }
    }

    /// The session terminals, to type into and look at.
    pub(crate) fn keys(&self) -> &dyn crate::terminal::keys::Keys {
        &*self.ports.keys
    }

    /// Where it starts a session's agent.
    pub(crate) fn launcher(&self) -> &dyn crate::terminal::launch::Launcher {
        &*self.ports.launcher
    }

    /// `job` after `after_ms`, off this thread, with the hub as the app holds it (ports::Later). A hub with no
    /// app (a test's) has nothing to hand it: dropped, unless the test holds it (testing::Deferred).
    fn later(&self, after_ms: u64, job: impl FnOnce(&Arc<Hub>) + Send + 'static) {
        match &self.ports.later {
            ports::Later::Threads => {
                let Some(me) = self.shared() else { return };
                std::thread::spawn(move || {
                    crate::clock::sleep_ms(after_ms);
                    job(&me);
                });
            }
            #[cfg(test)]
            ports::Later::Deferred(queue) => queue.hand_in(after_ms, Box::new(job)),
        }
    }

    /// The session's typing gate (see `typing`). Held for seconds by design (typing, then waiting for its hook):
    /// waited on with a plain lock, not `locking::lock`, whose 5 s limit is for locks nothing holds that long.
    fn typing_gate(&self, session_id: &str) -> std::sync::Arc<Mutex<()>> {
        let mut gates = crate::locking::lock(&self.typing, "typing gates");
        gates.entry(session_id.to_string()).or_default().clone()
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
            at_prompt: false, agent_id: String::new(), agent_name: String::new(),
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
        let n = self.write(|st| {
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
            Ok(n)
        })?;
        self.redraw();
        Ok((n, rx))
    }

    /// Every agent process Cue knows, by session: the sessions' own, and the ones only subscribed so far.
    pub fn agent_pids(&self) -> Vec<(String, i32)> {
        self.read(|st| {
            let mut v = st.sessions.pids();
            v.extend(st.subscribed.values().filter_map(|(o, _)| o.agent_pid.map(|p| (o.session_id.clone(), p))));
            v
        })
    }

    /// Drop a listener — only if it's still the same connection (a reconnect may have replaced it).
    pub fn unsubscribe(&self, session_id: &str, n: u64) {
        let dropped = self.write(|st| {
            let same = st.subscribers.get(session_id).is_some_and(|(m, _)| *m == n);
            if same {
                st.subscribers.remove(session_id);
                st.subscribed.remove(session_id);
            }
            same
        });
        if dropped {
            self.redraw();
        }
    }

    pub fn get(&self, id: &str) -> Option<Item> {
        self.read(|st| st.items.iter().chain(st.history.iter()).find(|i| i.id == id).cloned())
    }

    pub fn pending(&self) -> Vec<Item> {
        self.read(|st| st.items.clone())
    }

    /// (session id, agent pid) for every live session — the watcher drops the ones whose agent exited.
    pub fn session_pids(&self) -> Vec<(String, i32)> {
        self.read(|st| st.sessions.pids())
    }

    /// The window's state as last built, or built now if nothing is current (the first ask, or after a change
    /// with no window to send it to). Its clock reads now: the window sets its clock against it.
    pub fn state(&self) -> Value {
        self.kept_state().unwrap_or_else(|| Self::stamped(&self.snapshot()))
    }

    /// The window's state as last built, if one is current: no store, no file, nothing to wait on (one
    /// `RwLock` read to clone an `Arc`), so it can be answered from any thread.
    pub fn kept_state(&self) -> Option<Value> {
        self.shown.get().map(|s| Self::stamped(&s))
    }

    /// A snapshot with its clock set to now.
    fn stamped(snap: &Value) -> Value {
        let mut v = snap.clone();
        v["now_ms"] = json!(now_ms());
        v
    }

    /// The window's state built now, and kept as the last one (shown.rs) unless a newer one is in already.
    pub fn snapshot(&self) -> Arc<Value> {
        let (seq, snap, next) = self.build_snapshot();
        let snap = Arc::new(snap);
        if self.shown.put(seq, snap.clone()) {
            self.rebuild_when(&next);
        }
        snap
    }

    /// What the window shows changes with time alone at `next`: it's built again and sent then (shown.rs).
    /// Only with a window: without one (tests) every ask builds it.
    fn rebuild_when(&self, next: &shown::NextChange) {
        let Some(me) = self.app.as_ref().and_then(|a| a.try_state::<Arc<Hub>>()).map(|h| Arc::downgrade(h.inner())) else { return };
        self.shown.rebuild_at(next.when(), move || Box::new(move || if let Some(h) = me.upgrade() { h.redraw() }));
    }

    /// When the window's state changes with time alone: each rule the snapshot applies that compares the
    /// clock with a threshold, at the moment it flips.
    fn next_change(st: &Store, now: u64) -> shown::NextChange {
        let mut next = shown::NextChange::default();
        Self::review_changes(st, now, &mut next);
        // A session that just ended stays off the live list (snapshot_sources) until JUST_ENDED_MS after.
        for at in st.just_ended.values() {
            next.at(at + sessions::JUST_ENDED_MS, now);
        }
        next
    }

    /// The snapshot, its place in the order snapshots were taken from the store, and when it next changes
    /// with time alone.
    fn build_snapshot(&self) -> (u64, Value, shown::NextChange) {
        // What the files say (each Claude session's transcript tail, each folder's git HEAD) and what other
        // parts of Cue keep are read with the store let go: this runs on every change, and a slow disk mustn't
        // hold every hook and tick behind it.
        let (known, live, transcripts, cwds) = self.snapshot_sources();
        let ids: HashSet<String> = known.iter().cloned().chain(live.iter().map(|q| q.session_id.clone())).collect();
        let put = crate::config::folders_put();
        let mut outside = folders_view(live, transcripts, cwds, &put);
        outside.extend(apart_view(&ids));
        let (seq, next, mut snap) = self.write(Self::store_view);
        if let Some(snap) = snap.as_object_mut() {
            snap.extend(outside);
        }
        settle_projects(&mut snap, &put);
        (seq, snap, next)
    }

    /// The snapshot's part that is the store's, taken as the next in the order snapshots are taken, and when
    /// it next changes with time alone.
    fn store_view(st: &mut Store) -> (u64, shown::NextChange, Value) {
        st.shown_seq += 1;
        let seq = st.shown_seq;
        let reviews = Self::reviews_view(st);
        let next = Self::next_change(st, now_ms());
        // A session restarting stays in its place: its record from before while its agent is down, and
        // not on Parked on the way.
        let mut sessions = st.sessions.snapshot();
        for (sid, r) in &st.restarting {
            if let Some(s) = r.held.as_ref().filter(|_| !sessions.iter().any(|x| x.origin.session_id == *sid)) {
                sessions.push(s.clone());
            }
        }
        let parked: Vec<&crate::sessions::Parked> = st.parked.iter().filter(|p| !st.restarting.contains_key(&p.session.origin.session_id)).collect();
        let restarting: serde_json::Map<String, Value> = st.restarting.iter().map(|(sid, r)| (sid.clone(), json!({ "step": r.step, "since_ms": r.since_ms, "servers": r.servers.as_ref().map(Vec::len), "mcp": r.mcp }))).collect();
        let view = json!({
            "items": st.items,
            "history": st.history,
            // Every answer of the last day, in brief: today's numbers count these, not the History list.
            "answers": st.answers,
            "sessions": sessions,
            "parked": parked,
            "ended": Self::ended_view(st),
            "restarting": restarting,
            "restarted": st.restarted,
            "reviews": reviews,
            "review_queue": Self::review_queue_view(st),
            "unreachable": st.unreachable,
            // The board's cards' cache and context numbers (cache_tick), so a card says them as it lands.
            "insight": st.insight,
            "usage": st.rates,
        });
        (seq, next, view)
    }

    /// The sessions that ended while Cue ran, for the snapshot: not the ones live or parked again, each
    /// just what the window names a closed session by until it asks for the full record (closed_sessions):
    /// its id, name, folder (the name's fallback), project, agent and machine (the row's tags and the
    /// agent counts), and when it ended. Not its thread, bars or delivery records: the snapshot goes out on
    /// every change, and thirty full records would ride along each time.
    fn ended_view(st: &Store) -> Vec<Value> {
        st.ended
            .iter()
            .filter(|(_, s)| st.sessions.origin(&s.origin.session_id).is_none() && !st.parked.iter().any(|p| p.session.origin.session_id == s.origin.session_id))
            .map(|(at, s)| {
                json!({
                    "session_id": s.origin.session_id, "name": s.name, "cwd": s.origin.cwd, "project": s.project,
                    "harness": s.origin.harness, "machine": s.origin.machine, "ended_ms": at,
                })
            })
            .collect()
    }

    /// For `snapshot`, under one short lock: the session ids Cue knows, the live sessions it hasn't heard
    /// from yet (quiet Claude ones from its registry, Pi ones that only subscribed), each Claude session's
    /// transcript, and every folder in play.
    fn snapshot_sources(&self) -> Sources {
        self.read(Self::sources_in)
    }

    fn sources_in(st: &Store) -> Sources {
        // A restarting session's new agent shows up in Claude Code's list before Cue has its record back.
        let known: HashSet<String> = st.sessions.all().iter().map(|s| s.origin.session_id.clone()).chain(st.items.iter().map(|i| i.origin.session_id.clone())).chain(st.restarting.keys().cloned()).collect();
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
        let fresh = now_ms().saturating_sub(sessions::JUST_ENDED_MS);
        let live: Vec<crate::live::Quiet> = crate::live::claude().into_iter().chain(pi).filter(|q| !known.contains(&q.session_id) && !st.just_ended.get(&q.session_id).is_some_and(|at| *at > fresh)).collect();
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
    fn shared(&self) -> Option<Arc<Hub>> {
        self.app.as_ref().map(|a| a.state::<Arc<Hub>>().inner().clone())
    }

    /// The kept state is out of date (the store changed): the next ask for it builds it, until a draw keeps a
    /// newer one.
    fn unshown(&self) {
        let seq = self.read(|st| st.shown_seq);
        self.shown.forget(seq);
    }

    /// A card's notification (`id`: the card's), taken back once the card is gone (redraw.rs). `category`:
    /// "turn" puts Reply and the quick phrases on it; "turn-full" adds Compact.
    fn notify(&self, id: &str, title: &str, body: &str, category: &str) {
        self.notify_card(id, title, body, category, false, true);
    }

    /// A turn you asked to be told about ("Tell me"): shown even with Cue in front, with its own sound.
    fn notify_watched(&self, id: &str, title: &str, body: &str, category: &str) {
        self.notify_card(id, title, body, category, true, true);
    }

    /// Never muted by notify::quiet (a cache about to run out isn't something voice tells you).
    pub(super) fn notify_unmuted(&self, id: &str, title: &str, body: &str, category: &str) {
        self.notify_card(id, title, body, category, false, false);
    }

    /// A notification about something that isn't a card (a session gone quiet, a failed task, a usage limit),
    /// under `key`: never taken back with the cards; whoever posted it takes it back when that's over.
    pub(super) fn notify_about(&self, key: &str, title: &str, body: &str) {
        self.ports.notes.show(crate::notify::Note { id: key.to_string(), title: title.to_string(), body: body.to_string(), category: String::new(), watched: false });
    }

    /// Notification Center's part (is Cue in front, the post itself) runs on its own thread (notify::show):
    /// here only the checks against the store.
    fn notify_card(&self, id: &str, title: &str, body: &str, category: &str, watched: bool, mutable: bool) {
        // A finished turn of a session something else is telling you about already (notify::quiet): not twice.
        // Anything that needs you (a permission, a question) and a turn you asked to be told about still come.
        if mutable && !watched && category.starts_with("turn") {
            let sid = self.read(|st| st.items.iter().find(|i| i.id == id).map(|i| i.origin.session_id.clone()));
            if sid.is_some_and(|s| crate::notify::is_quiet(&s)) {
                return;
            }
        }
        let note = crate::notify::Note { id: id.to_string(), title: title.to_string(), body: body.to_string(), category: category.to_string(), watched };
        // Noted as notified with the post handed in, under one lock: a draw either sees neither, or sees it
        // and hands in its taking back after the post. (Taking back one that never went up costs nothing.)
        self.write(|st| {
            st.notified.insert(note.id.clone());
            self.ports.notes.show(note);
        });
    }
}

/// The snapshot's live sessions Cue hasn't heard from, and what their folders and transcripts say: each
/// folder's branch and project, and your latest request to each Claude session (from its transcript: no model).
fn folders_view(live: Vec<crate::live::Quiet>, transcripts: Vec<(String, String)>, cwds: Vec<String>, put: &HashMap<String, String>) -> serde_json::Map<String, Value> {
    let about: HashMap<String, Value> = transcripts.into_iter()
        .filter_map(|(sid, path)| { let prompt = crate::live::about(&path); (!prompt.is_empty()).then(|| (sid, json!({ "prompt": prompt }))) })
        .collect();
    let projects: HashMap<String, Value> = cwds.iter().map(|c| (c.clone(), folder_project(c, put))).collect();
    let branches: HashMap<String, String> = cwds.into_iter().map(|c| { let b = crate::logs::repos::branch(&c); (c, b) }).collect();
    let view = json!({ "live": live, "branches": branches, "projects": projects, "about": about });
    view.as_object().cloned().unwrap_or_default()
}

/// The snapshot's parts other parts of Cue keep, each behind its own lock (taken before the store's, never
/// under it) for the sessions `ids`: the crew and its hand-offs (relay's, and a hand-off skill's pending ones
/// that offer "Continue in a new session"), "Hand off if I don't come back", About and the status list's
/// costs, and settings (kept in memory: config.rs).
fn apart_view(ids: &HashSet<String>) -> serde_json::Map<String, Value> {
    let (crew, handoffs, skill_handoffs, ahead_watch) = (crate::leads::view(ids), crate::leads::handoffs(ids), crate::db::handoffs(), crate::db::ahead_watched());
    let (tasks, tasks_spent, status_spent) = (crate::tasks::view(), crate::tasks::spent(), crate::statuslist::spent());
    let (settings, connections, usage_file) = (crate::config::effective(), crate::config::connections(), crate::usage_file::read());
    let view = json!({
        "crew": crew, "handoffs": handoffs, "skill_handoffs": skill_handoffs, "ahead_watch": ahead_watch,
        "tasks": tasks, "tasks_spent": tasks_spent, "status_spent": status_spent,
        "window_ms": crate::sessions::WINDOW_MS, "now_ms": now_ms(),
        "settings": settings, "connections": connections, "usage_file": usage_file,
    });
    view.as_object().cloned().unwrap_or_default()
}

/// Every row in the snapshot under the project it shows in: a folder's (`put_projects`), then a session's own
/// move (`move_projects`). Moves a new session took over (after /clear) are saved, so they stay once the old
/// session's gone: added to the moves as they are when saved, not as read here, so one saved meanwhile stays.
fn settle_projects(snap: &mut Value, put: &HashMap<String, String>) {
    put_projects(snap, put);
    let taken = move_projects(snap, &crate::config::moved());
    if !taken.is_empty() {
        let _ = crate::config::update("project.moves", |c| Some(Value::Object(crate::config::moves_in(c).into_iter().chain(taken).map(|(k, v)| (k, json!(v))).collect())));
    }
}

/// A folder's project, for the snapshot (see `build_snapshot`).
fn folder_project(cwd: &str, put: &HashMap<String, String>) -> Value {
    let own = crate::model::own_project(cwd, "");
    let name = crate::model::project_said(cwd, "", put).unwrap_or_else(|| own.clone());
    let root = crate::logs::repos::repo_root(cwd).map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|| cwd.to_string());
    let top = crate::logs::repos::checkout_top(cwd).map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|| cwd.to_string());
    json!({ "name": name, "own": own, "root": root, "top": top, "key": crate::model::put_key(cwd, "") })
}

/// Every card, row, answer, parked and live entry in a folder you put in a project, or whose repository
/// says its project (model.rs project_said), says that project, and `folder_project` keeps the one it
/// was recorded with (its folder's own), so taking it back needs nothing rewritten.
fn put_projects(snap: &mut Value, put: &HashMap<String, String>) {
    let mut seen: HashMap<(String, String), Option<String>> = HashMap::new();
    for list in ["items", "history", "answers", "sessions", "parked", "ended", "live"] {
        let Some(rows) = snap.get_mut(list).and_then(Value::as_array_mut) else { continue };
        for row in rows {
            let Some(o) = row.as_object_mut() else { continue };
            let Some(cwd) = o.get("cwd").and_then(Value::as_str).filter(|c| !c.is_empty()).map(str::to_string) else { continue };
            let machine = o.get("machine").and_then(Value::as_str).unwrap_or("").to_string();
            let said = seen.entry((machine.clone(), cwd.clone())).or_insert_with(|| crate::model::project_said(&cwd, &machine, put)).clone();
            let Some(to) = said else { continue };
            let own = o.get("project").cloned().unwrap_or_else(|| json!(crate::model::own_project(&cwd, &machine)));
            o.insert("folder_project".into(), own);
            o.insert("project".into(), json!(to));
        }
    }
}

/// A session moved to another project in Cue (Settings' project.moves): its cards, row, answers, parked entry
/// and live entry say that project, and `folder_project` keeps its folder's, so it can be moved back.
/// A session new in the process of a moved one (`/clear` starts a new one there) takes its move too: those
/// come back, to be saved.
fn move_projects(snap: &mut Value, moved: &HashMap<String, String>) -> HashMap<String, String> {
    let pid_of = |row: &Value| row.get("agent_pid").or_else(|| row.get("pid")).and_then(Value::as_i64).filter(|p| *p > 0);
    let sid_of = |row: &Value| row.get("session_id").and_then(Value::as_str).map(str::to_string);
    // Same process and same folder: a process number alone is reused by an unrelated session later.
    let cwd_of = |row: &Value| row.get("cwd").and_then(Value::as_str).unwrap_or("").trim_end_matches('/').to_string();
    let mut by_pid: HashMap<i64, (String, String)> = HashMap::new();
    for list in ["sessions", "live"] {
        for row in snap.get(list).and_then(Value::as_array).into_iter().flatten() {
            if let (Some(pid), Some(to)) = (pid_of(row), sid_of(row).and_then(|sid| moved.get(&sid).cloned())) {
                by_pid.insert(pid, (to, cwd_of(row)));
            }
        }
    }
    let mut taken = HashMap::new();
    for list in ["sessions", "live"] {
        for row in snap.get(list).and_then(Value::as_array).into_iter().flatten() {
            if let (Some(pid), Some(sid)) = (pid_of(row), sid_of(row)) {
                if let Some((to, _)) = by_pid.get(&pid).filter(|(_, cwd)| !moved.contains_key(&sid) && *cwd == cwd_of(row)) {
                    taken.insert(sid, to.clone());
                }
            }
        }
    }
    if moved.is_empty() && taken.is_empty() {
        return taken;
    }
    for list in ["items", "history", "answers", "sessions", "parked", "ended", "live"] {
        let Some(rows) = snap.get_mut(list).and_then(Value::as_array_mut) else { continue };
        for row in rows {
            let Some(to) = sid_of(row).and_then(|sid| moved.get(&sid).or_else(|| taken.get(&sid)).cloned()) else { continue };
            if let Some(o) = row.as_object_mut() {
                let own = o.get("folder_project").or_else(|| o.get("project")).cloned().unwrap_or(Value::Null);
                o.insert("folder_project".into(), own);
                o.insert("project".into(), json!(to));
            }
        }
    }
    taken
}

#[cfg(test)]
mod move_tests {
    use super::*;

    #[test]
    fn a_folder_put_in_a_project_shows_under_it() {
        let mut snap = json!({ "items": [{ "session_id": "a", "cwd": "/srv/scratch/x", "machine": "box", "project": "x" }, { "session_id": "b", "cwd": "/srv/other", "machine": "box", "project": "other" }], "live": [] });
        put_projects(&mut snap, &HashMap::from([("box:/srv/scratch".to_string(), "api".to_string())]));
        assert_eq!(snap["items"][0]["project"], "api");
        assert_eq!(snap["items"][0]["folder_project"], "x");
        assert_eq!(snap["items"][1], json!({ "session_id": "b", "cwd": "/srv/other", "machine": "box", "project": "other" }));
        // A session moved on its own after that keeps its folder's own to go back to.
        move_projects(&mut snap, &HashMap::from([("a".to_string(), "web".to_string())]));
        assert_eq!(snap["items"][0]["project"], "web");
        assert_eq!(snap["items"][0]["folder_project"], "x");
    }

    #[test]
    fn a_moved_session_shows_under_its_new_project_and_keeps_its_folders() {
        let mut snap = json!({
            "items": [{ "session_id": "a", "project": "web" }, { "session_id": "b", "project": "web" }],
            "sessions": [{ "session_id": "a", "project": "web" }],
            "settings": { "project": "untouched" },
        });
        move_projects(&mut snap, &HashMap::from([("a".to_string(), "api".to_string())]));
        assert_eq!(snap["items"][0], json!({ "session_id": "a", "project": "api", "folder_project": "web" }));
        assert_eq!(snap["items"][1], json!({ "session_id": "b", "project": "web" }));
        assert_eq!(snap["sessions"][0]["project"], "api");
        assert_eq!(snap["settings"], json!({ "project": "untouched" }));
    }

    #[test]
    fn a_new_session_in_a_moved_ones_process_takes_its_move() {
        let mut snap = json!({
            "sessions": [{ "session_id": "old", "project": "web", "agent_pid": 7, "cwd": "/x/web" }, { "session_id": "new", "project": "web", "agent_pid": 7, "cwd": "/x/web/" }, { "session_id": "other", "project": "web", "agent_pid": 8, "cwd": "/x/web" }],
            "live": [{ "session_id": "quiet", "pid": 7, "cwd": "/x/web" }],
        });
        let taken = move_projects(&mut snap, &HashMap::from([("old".to_string(), "api".to_string())]));
        assert_eq!(taken, HashMap::from([("new".to_string(), "api".to_string()), ("quiet".to_string(), "api".to_string())]));
        assert_eq!(snap["sessions"][1]["project"], "api");
        assert_eq!(snap["sessions"][2]["project"], "web", "another process keeps its own");
        assert_eq!(snap["live"][0]["project"], "api", "a live entry says it too");
        // The same process number in another folder is another session (numbers get reused): it keeps its own.
        let mut snap = json!({ "sessions": [{ "session_id": "old", "agent_pid": 7, "cwd": "/x/web", "project": "web" }, { "session_id": "late", "agent_pid": 7, "cwd": "/x/cue", "project": "cue" }] });
        assert!(move_projects(&mut snap, &HashMap::from([("old".to_string(), "api".to_string())])).is_empty());
        assert_eq!(snap["sessions"][1]["project"], "cue");
        // Moved by you, it keeps yours.
        let mut snap = json!({ "sessions": [{ "session_id": "old", "agent_pid": 7, "project": "web" }, { "session_id": "new", "agent_pid": 7, "project": "web" }] });
        let taken = move_projects(&mut snap, &HashMap::from([("old".to_string(), "api".to_string()), ("new".to_string(), "web".to_string())]));
        assert!(taken.is_empty());
        assert_eq!(snap["sessions"][1]["project"], "web");
    }
}
