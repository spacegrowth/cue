//! The sessions themselves: started, resumed, adopted, parked, swept, renamed, closed, forgotten.

use super::*;
use super::asks::waiting_words;

/// A Claude session missing this long from Claude Code's running list (here, or on its machine),
/// with nothing written to its conversation meanwhile, is gone.
const GONE_AFTER_MS: u64 = 45_000;
/// How long a session that just ended is kept out of the list of sessions Cue hasn't heard from: its
/// agent exits a moment after its SessionEnd (up to a second or two).
pub(super) const JUST_ENDED_MS: u64 = 10_000;

/// A rename typed into Claude Code shows in its list of sessions within a few seconds; past this,
/// it didn't take it.
const RENAME_WITHIN_MS: u64 = 15_000;

impl Hub {
    /// Where a live session's terminal is — for "Go to tab" on a working session.
    pub fn session_origin(&self, session_id: &str) -> Option<Origin> {
        self.st().sessions.origin(session_id)
    }

    /// "Catch me up" on a session, for a window or an add-on that last saw its chat at `seen_ms`: from the
    /// messages after then (Cue's log, with the live thread for what isn't saved yet). Any agent, any machine.
    pub fn catch_up(&self, session_id: &str, seen_ms: u64) -> Result<crate::btw::CatchUp, String> {
        let mut log: Vec<Exchange> = crate::db::session_log_page(session_id, i64::MAX as u64, 200).into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect();
        for e in self.st().sessions.thread(session_id) {
            if !log.iter().any(|x| x.at_ms == e.at_ms && x.role == e.role) {
                log.push(e);
            }
        }
        log.sort_by_key(|e| e.at_ms);
        let (before, since): (Vec<Exchange>, Vec<Exchange>) = log.into_iter().partition(|e| e.at_ms <= seen_ms);
        if !since.iter().any(|e| e.role == "agent") {
            return Err("nothing new since you looked".into());
        }
        let seen = before.iter().rev().find(|e| e.role == "agent").map(|e| e.text.as_str());
        crate::btw::catch_up(seen, &since)
    }

    /// Every live session as it was when Cue last ran (each with its name): what a reboot may
    /// have left without a tmux pane.
    pub fn saved_sessions(&self) -> Vec<crate::sessions::Session> {
        self.st().sessions.all()
    }

    /// A session's state set by hand (a moved one: "working" while it loads, "waiting" once its prompt is up).
    pub fn mark_state(&self, origin: &Origin, state: &str) {
        self.st().sessions.mark(origin, state, None);
        self.changed();
    }

    /// When a session was starred; 0 if it isn't.
    pub fn session_star(&self, session_id: &str) -> u64 {
        self.st().sessions.all().iter().find(|s| s.origin.session_id == session_id).map(|s| s.starred_ms).unwrap_or(0)
    }

    pub fn restore_star(&self, session_id: &str, starred_ms: u64) {
        self.st().sessions.restore_star(session_id, starred_ms);
        self.changed();
    }

    /// The name a session shows under (yours, or the one it gave itself); "" if it has none.
    pub fn session_name(&self, session_id: &str) -> String {
        self.st().sessions.all().into_iter().find(|s| s.origin.session_id == session_id).map(|s| s.name).unwrap_or_default()
    }

    /// Every name a session shows under, for picking one no session has (a fork's).
    pub fn session_names(&self) -> Vec<String> {
        self.st().sessions.all().into_iter().map(|s| s.name).filter(|n| !n.is_empty()).collect()
    }

    /// A session Cue just started ("+ New session"): known from the start, so it opens in Active and
    /// takes what you type before it has said anything. With a first message it's already working.
    /// The trust question was answered (or is gone): its banner goes, and the next look at its screen
    /// says what's there now.
    pub fn trust_answered(&self, session_id: &str) {
        self.st().sessions.set_trust(session_id, 0);
        self.changed();
    }

    /// Past a screen that waited for Enter before Claude Code started: its banner goes until another shows.
    pub fn enter_pressed(&self, session_id: &str) {
        self.st().sessions.set_setup(session_id, None, now_ms());
        self.changed();
    }

    pub fn started(&self, origin: Origin, message: &str, images: &[String], asks_trust: bool) {
        {
            let mut st = self.st();
            let sid = origin.session_id.clone();
            let msg = message.trim();
            let sent = !msg.is_empty() || !images.is_empty();
            st.sessions.mark(&origin, if sent { "working" } else { "waiting" }, (!msg.is_empty()).then_some(msg));
            if asks_trust {
                st.sessions.set_trust(&sid, now_ms());
            }
            st.sessions.note_with(&sid, "you", msg, images, crate::config::context_keep());
        }
        self.changed();
    }

    /// An older session Cue just resumed (or one it takes in): known at once, waiting for you (or working),
    /// with its name and the last few messages of its conversation so Active shows where it left off.
    pub fn resumed(&self, origin: Origin, name: &str, recent: &[Ctx], working: bool) {
        {
            let mut st = self.st();
            let sid = origin.session_id.clone();
            st.sessions.mark(&origin, if working { "working" } else { "waiting" }, None);
            if !name.is_empty() {
                st.sessions.set_name(&sid, name);
            }
            for c in recent {
                st.sessions.note(&sid, if c.role == "user" { "you" } else { "agent" }, &c.text, crate::config::context_keep());
            }
        }
        self.changed();
    }

    /// Open a live Claude session Cue hasn't heard from yet (one you resumed in a terminal after a restart,
    /// say) like any other: its terminal from its process, its last messages from its transcript. Its
    /// hooks take over from the next thing it does.
    pub fn adopt(&self, session_id: &str) -> Result<(), String> {
        if self.st().sessions.origin(session_id).is_some() {
            return Ok(());
        }
        let q = crate::live::claude().into_iter().find(|q| q.session_id == session_id).ok_or("that session is gone")?;
        let origin = crate::live::origin_of(&q);
        let path = origin.transcript_path.clone();
        let recent = if path.is_empty() { vec![] } else { crate::transcript::recent_context(&path, 6) };
        // Its name comes from Claude Code's registry, as for every session.
        self.resumed(origin.clone(), "", &recent, q.status == "busy");
        // In Waiting as if Cue had been there all along (no notification: it isn't new). A prompt open in its
        // terminal (one Cue missed): a card that sends you there.
        if q.status == "waiting" {
            self.terminal_ask(origin, waiting_words(&q.waiting_for), false);
        } else if q.status == "idle" && crate::leads::resolve_driver(session_id, "").is_empty() {
            // Its last turn ended with its reply and nothing from you since: your turn. (One a lead drives
            // is the lead's to pick up.)
            if let Some((at, reply, parts)) = (!path.is_empty()).then(|| transcript::turn_ended_parts(&path)).flatten() {
                self.your_turn_since(origin, reply, parts, at);
            }
        }
        Ok(())
    }

    /// A finished turn Cue missed, as a "your turn" card dated when it ended.
    pub(super) fn your_turn_since(&self, origin: Origin, reply: String, parts: Vec<TurnPart>, at: u64) {
        let sid = origin.session_id.clone();
        // As the Stop hook would have split it: the answer is the card, a hook's follow-up folds under it.
        let (reply, followup) = split_turn(reply, &parts, &crate::config::turn_mode());
        {
            let mut st = self.st();
            if st.items.iter().any(|i| i.origin.session_id == sid) {
                return;
            }
            st.sessions.mark(&origin, "waiting", None);
            let thread = st.sessions.thread(&sid);
            let mut it = Self::new_item(&mut st, "waiting", origin);
            it.thread = thread;
            it.context = vec![Ctx { role: "assistant".into(), text: reply.clone() }];
            it.message = reply;
            it.followup = followup;
            it.created_ms = at;
            st.items.push(it);
        }
        self.changed();
        // A message you kept for it (Cue restarted before its turn ended): its turn is over, so it goes now.
        let kept = self.st().sessions.held(&sid).is_some();
        if kept {
            self.send_held_soon(&sid, 0);
        }
    }

    /// Every few seconds, from Claude Code's list of live sessions:
    /// - take in any session Cue doesn't know yet (resumed after a restart, started while Cue was closed),
    ///   so it shows on the Board with its chat from the start, not only once it has sent Cue something;
    /// - a prompt open in a session's terminal that Cue has no card for (an MCP server asking for input,
    ///   a teammate's permission prompt, a dialog: they don't come through Cue's permission hook) gets an
    ///   "asking in its terminal" card, once it has been open a few seconds;
    /// - that card goes once Claude Code says the session stopped waiting (you answered it there).
    pub fn sync_live(&self) {
        let live = crate::live::claude();
        let now = now_ms();
        let (fresh, answered, asking) = {
            let st = self.st();
            let fresh: Vec<String> = live.iter().map(|q| q.session_id.clone()).filter(|sid| st.sessions.origin(sid).is_none()).collect();
            let answered: Vec<String> = live.iter().filter(|q| q.status != "waiting")
                .filter(|q| st.items.iter().any(|i| i.kind == "terminal" && i.origin.session_id == q.session_id && q.since_ms > i.created_ms))
                .map(|q| q.session_id.clone()).collect();
            // Not one Cue has a card for, and not one you just answered in Cue (the list can lag a moment).
            let answered_here = |sid: &str| st.history.iter().any(|i| i.origin.session_id == sid && i.resolved_ms.is_some_and(|r| now.saturating_sub(r) < 6_000));
            let asking: Vec<(Origin, String)> = live.iter()
                .filter(|q| q.status == "waiting" && now.saturating_sub(q.since_ms) > WAITING_GRACE_MS)
                .filter(|q| !st.items.iter().any(|i| i.origin.session_id == q.session_id && i.kind != "waiting") && !answered_here(&q.session_id))
                .filter_map(|q| st.sessions.origin(&q.session_id).map(|o| (o, waiting_words(&q.waiting_for))))
                .collect();
            (fresh, answered, asking)
        };
        for sid in answered {
            self.clear_terminal_asks(&sid);
        }
        for (origin, what) in asking {
            self.terminal_ask(origin, what, true);
        }
        for sid in fresh {
            let _ = self.adopt(&sid);   // gone meanwhile: nothing to take in
        }
    }

    /// Whether a session can be closed from Cue without losing anything: not while it's working
    /// (its turn would be cut off) or asking you something (the question would go unanswered).
    pub fn closable(&self, session_id: &str) -> Result<(), String> {
        // Its machine can't be reached: nothing there to wait for, Close just takes it out of Cue.
        if self.unreachable(session_id) {
            return Ok(());
        }
        let st = self.st();
        match st.sessions.state(session_id).as_deref() {
            Some("working") => return Err("it's working: stop it first, or let it finish".into()),
            Some("deciding") => return Err("it's asking you something: answer that first".into()),
            _ => {}
        }
        if st.items.iter().any(|i| i.origin.session_id == session_id && i.kind != "waiting") {
            return Err("it's asking you something: answer that first".into());
        }
        drop(st);
        if crate::live::claude().iter().any(|q| q.session_id == session_id && q.status == "busy") {
            return Err("it's working: stop it first, or let it finish".into());
        }
        Ok(())
    }

    /// Where a live session runs, for "Go to tab" in the Sessions view: one Cue has heard from, a
    /// Pi session that only subscribed, or a quiet Claude session (its terminal, from its process).
    pub fn live_origin(&self, session_id: &str) -> Option<Origin> {
        let st = self.st();
        if let Some(o) = st.sessions.origin(session_id).or_else(|| st.subscribed.get(session_id).map(|(o, _)| o.clone())) {
            return Some(o);
        }
        drop(st);
        let q = crate::live::claude().into_iter().find(|q| q.session_id == session_id)?;
        Some(Origin { session_id: q.session_id, harness: q.harness, cwd: q.cwd, tty: crate::live::tty_of(q.pid), agent_pid: Some(q.pid), ..Default::default() })
    }

    /// A live session's whole record (what Park keeps).
    pub fn session_record(&self, session_id: &str) -> Option<crate::sessions::Session> {
        self.st().sessions.all().into_iter().find(|s| s.origin.session_id == session_id)
    }

    /// Park: keep the record of a session whose agent was just ended, under Parked (saved), and take
    /// it off the live lists at once (its end may still be on its way).
    pub fn add_parked(&self, session: crate::sessions::Session) {
        let sid = session.origin.session_id.clone();
        let p = crate::sessions::Parked { parked_ms: now_ms(), session };
        crate::db::park(&p);
        {
            let mut st = self.st();
            st.sessions.remove(&sid);
            st.parked.retain(|x| x.session.origin.session_id != sid);
            st.parked.insert(0, p);
        }
        self.changed();
    }

    /// Sessions that ended within `window_ms` (at most `max`), newest first: not parked, not live again.
    /// Each its record as it was, with when it ended (`ended_ms`).
    pub fn closed(&self, window_ms: u64, max: usize) -> Vec<Value> {
        // The ones that ended while Cue ran come first, as Cue holds them (the saved copy may be a
        // moment behind): then the saved ones, each once.
        let (skip, mut out): (HashSet<String>, Vec<(u64, crate::sessions::Session)>) = {
            let st = self.st();
            let skip = st.parked.iter().map(|p| p.session.origin.session_id.clone()).chain(st.sessions.all().into_iter().map(|s| s.origin.session_id)).collect();
            (skip, st.ended.iter().cloned().collect())
        };
        let since = now_ms().saturating_sub(window_ms);
        out.retain(|(at, _)| *at > since);
        for (at, s) in crate::db::closed(since, max + skip.len()) {
            if !out.iter().any(|(_, e)| e.origin.session_id == s.origin.session_id) {
                out.push((at, s));
            }
        }
        out.sort_by(|a, b| b.0.cmp(&a.0));
        // Under the project you moved it to, as live rows are (the snapshot's remap).
        let moved = crate::config::moved();
        out.into_iter()
            .filter(|(_, s)| !skip.contains(&s.origin.session_id))
            .take(max)
            .filter_map(|(at, s)| {
                let mut v = serde_json::to_value(&s).ok()?;
                v["ended_ms"] = json!(at);
                if let Some(p) = moved.get(&s.origin.session_id) {
                    v["project"] = json!(p);
                }
                Some(v)
            })
            .collect()
    }

    pub fn parked(&self, session_id: &str) -> Option<crate::sessions::Parked> {
        self.st().parked.iter().find(|p| p.session.origin.session_id == session_id).cloned()
    }

    /// Off the Parked list (resumed, or unparked: forgotten, its conversation still in History).
    pub fn take_parked(&self, session_id: &str) -> Option<crate::sessions::Parked> {
        let p = {
            let mut st = self.st();
            let at = st.parked.iter().position(|p| p.session.origin.session_id == session_id)?;
            st.parked.remove(at)
        };
        crate::db::unpark(session_id);
        self.changed();
        Some(p)
    }

    /// A parked session running again on `origin`: its record back as it was, at its prompt, or
    /// working on `message` when it was resumed with one.
    /// A restart begins (see `Restart`): its record kept to show while its agent is down.
    pub fn restart_begin(&self, session_id: &str, servers: Option<Vec<String>>, mcp: bool) {
        let held = self.session_record(session_id);
        {
            let mut st = self.st();
            st.restarting.insert(session_id.to_string(), super::Restart { step: "stop", since_ms: now_ms(), servers, mcp, held });
        }
        self.changed();
    }

    /// The restart's next step ("start", "load").
    pub fn restart_step(&self, session_id: &str, step: &'static str) {
        {
            let mut st = self.st();
            if let Some(r) = st.restarting.get_mut(session_id) {
                r.step = step;
            }
        }
        self.changed();
    }

    /// The restart is over: back (`error` None), or it failed (then it's wherever park and resume left
    /// it). Either way its chat gets a line saying so.
    pub fn restart_end(&self, session_id: &str, error: Option<&str>) {
        {
            let mut st = self.st();
            let Some(r) = st.restarting.remove(session_id) else { return };
            let now = now_ms();
            let line = json!({ "at_ms": now, "secs": now.saturating_sub(r.since_ms).div_ceil(1000), "servers": r.servers, "error": error });
            let list = st.restarted.entry(session_id.to_string()).or_default();
            list.push(line);
            let extra = list.len().saturating_sub(5);
            list.drain(..extra);
        }
        self.changed();
    }

    pub fn unparked(&self, p: crate::sessions::Parked, origin: Origin, message: &str) {
        let s = p.into_session(origin.clone());
        {
            let mut st = self.st();
            st.parked.retain(|x| x.session.origin.session_id != s.origin.session_id);
            st.sessions.restore(vec![s.clone()]);
            if !message.is_empty() {
                st.sessions.mark(&origin, "working", Some(message));
                st.sessions.note(&s.origin.session_id, "you", message, crate::config::context_keep());
            }
        }
        crate::db::unpark(&s.origin.session_id);
        self.changed();
    }

    /// End Claude sessions whose agent isn't running any more, by Claude Code's own list of running
    /// sessions (`live`) on this Mac (`machine` "") or on one of your machines. Missing for
    /// GONE_AFTER_MS, with its conversation untouched that long, a session is ended in Cue, cards and
    /// all: a tab closed while Cue wasn't looking, a machine restarted, a session Cue lost track of.
    /// `live` None: the machine couldn't be reached; its sessions are marked so, not ended.
    pub fn sweep(&self, machine: &str, live: Option<&HashSet<String>>) {
        let now = now_ms();
        let (gone, reach_changed) = {
            let mut st = self.st();
            let reach_changed = match live {
                None => st.unreachable.insert(machine.to_string()),
                Some(_) => st.unreachable.remove(machine),
            };
            let mine: Vec<(String, String)> = st.sessions.all().into_iter().filter(|s| s.origin.machine == machine && s.origin.harness == "claude").map(|s| (s.origin.session_id, s.origin.transcript_path)).collect();
            let mut gone = vec![];
            for (sid, transcript) in mine {
                let written = std::fs::metadata(&transcript).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|e| (e.as_millis() as u64) < GONE_AFTER_MS);
                if live.is_some_and(|l| !l.contains(&sid)) && !written {
                    let since = *st.missing.entry(sid.clone()).or_insert(now);
                    if now.saturating_sub(since) > GONE_AFTER_MS {
                        gone.push(sid);
                    }
                } else {
                    st.missing.remove(&sid);
                }
            }
            for sid in &gone {
                st.missing.remove(sid);
            }
            (gone, reach_changed)
        };
        for sid in gone {
            self.forget(&sid);
        }
        if reach_changed {
            self.changed();
        }
    }

    /// A session Cue can't reach or that's gone, taken out of Cue: its cards finish as gone, its row goes.
    pub fn forget(&self, session_id: &str) {
        let ids: Vec<String> = self.st().items.iter().filter(|i| i.origin.session_id == session_id).map(|i| i.id.clone()).collect();
        for id in ids {
            self.finish(&id, "gone", "session ended");
        }
        self.drop_session(session_id);
    }

    /// Whether this session's machine couldn't be reached on Cue's last try.
    pub fn unreachable(&self, session_id: &str) -> bool {
        let st = self.st();
        st.sessions.all().iter().find(|s| s.origin.session_id == session_id).is_some_and(|s| !s.origin.machine.is_empty() && st.unreachable.contains(&s.origin.machine))
    }

    /// A session is gone: its row goes, and its record (name, folder, thread) stays as it was, under
    /// Closed, from this moment on. The window never has to fall back to the folder's name for it.
    pub fn drop_session(&self, session_id: &str) {
        let removed = {
            let mut st = self.st();
            let gone = st.sessions.take(session_id);
            // Its "missing since" goes with it: resumed later (same id), an old one would count it gone at once.
            st.missing.remove(session_id);
            if let Some(s) = gone.as_ref() {
                let now = now_ms();
                st.just_ended.retain(|_, at| now.saturating_sub(*at) < JUST_ENDED_MS);
                st.just_ended.insert(session_id.to_string(), now);
                st.ended.retain(|(_, e)| e.origin.session_id != session_id);
                st.ended.push_front((now, s.clone()));
                st.ended.truncate(crate::ipc::CLOSED_MAX);
            }
            gone.is_some()
        };
        if removed {
            self.changed();
        }
    }

    /// Delete: a session (already ended) out of Cue for good: its cards, its History entries, Parked,
    /// and everything cue.db kept of it.
    pub fn purge(&self, session_id: &str) {
        self.forget(session_id);
        {
            let mut st = self.st();
            st.history.retain(|i| i.origin.session_id != session_id);
            st.answers.retain(|a| a.session_id != session_id);
            st.parked.retain(|p| p.session.origin.session_id != session_id);
            st.missing.remove(session_id);
        }
        crate::db::delete_session(session_id);
        self.changed();
    }

    /// Change one setting. History size applies right away (memory and file are trimmed).
    pub fn set_setting(&self, key: &str, value: Value) -> Result<(), String> {
        // A sound you just picked plays once, so you hear what you chose.
        let sound = (key == "notify.sound").then(|| value.as_str().map(str::to_string)).flatten();
        crate::config::set(key, value)?;
        if let Some(c) = sound {
            crate::sounds::preview(&c);
        }
        if key == "quick.phrases" {
            crate::notify::set_phrases(&crate::config::quick_phrases());
        }
        if key == "history.keep" {
            let history = crate::db::history(crate::config::history_keep());
            self.st().history = history;
        }
        if key == "appearance.zoom" {
            if let Some(app) = &self.app {
                crate::apply_zoom(app);
            }
        }
        self.changed();
        Ok(())
    }

    /// Rename a session: Cue shows the name at once, and the agent is told in its own way. Claude Code:
    /// "/rename <name>" typed at its prompt (deferred to the end of the turn if it's busy). Pi: through
    /// Cue's extension, nothing typed. Codex: Cue only. Returns what happened, for a toast. Blocking.
    /// Rename a session: Cue shows the new name at once, then tells the agent. If the agent doesn't take
    /// it (Pi isn't connected, typing into Claude Code failed, or Claude Code's list never shows it), Cue
    /// keeps your name and the chat says so (`rename_error`), with Try again. Codex has no names: Cue's only.
    pub fn rename(&self, session_id: &str, name: &str) -> Result<String, String> {
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().filter(|c| !c.is_control()).take(60).collect();
        let (harness, state, origin) = self.st().sessions.set_name(session_id, &name).ok_or("that session is gone")?;
        self.changed();
        if name.is_empty() {
            return Ok("Name cleared in Cue".into());
        }
        match harness.as_str() {
            "pi" => {
                let sent = self.st().subscribers.get(session_id).map(|(_, tx)| tx.send(json!({ "type": "rename", "name": name })).is_ok()).unwrap_or(false);
                if sent { Ok("Renamed".into()) } else { Err(self.rename_failed(session_id, "Pi isn't connected to Cue, so it didn't get the new name")) }
            }
            "claude" if state == "working" => {
                self.st().sessions.defer_rename(session_id);
                Ok("Renamed · Claude Code gets it when this turn ends".into())
            }
            "claude" => match crate::focus::type_into(&origin, &format!("/rename {name}")) {
                Ok(_) => {
                    // Claude Code's list shows a rename within a few seconds: give it 15.
                    if origin.machine.is_empty() {
                        self.st().sessions.expect_rename(session_id, &name, now_ms() + RENAME_WITHIN_MS);
                    }
                    Ok("Renamed".into())
                }
                Err(e) => Err(self.rename_failed(session_id, &format!("Couldn't give Claude Code the new name: {e}"))),
            },
            "codex" => Ok("Named in Cue (Codex has no names of its own)".into()),
            _ => Ok("Named in Cue".into()),
        }
    }

    /// The agent didn't get the name: Cue keeps it, and the chat says why. Returns `why`.
    fn rename_failed(&self, session_id: &str, why: &str) -> String {
        self.st().sessions.rename_failed(session_id, why);
        self.changed();
        why.to_string()
    }

    /// × on the chat's rename notice.
    pub fn dismiss_rename_error(&self, session_id: &str) {
        if self.st().sessions.dismiss_rename_error(session_id) {
            self.changed();
        }
    }

    /// A session + New just started with a name: Cue shows it from the start (the agent got it on its
    /// command line where it takes one).
    pub fn name_new(&self, session_id: &str, name: &str) {
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
        if !name.is_empty() && self.st().sessions.set_name(session_id, &name).is_some() {
            self.changed();
        }
    }

    /// Every second: Claude Code's own names for its sessions here, to settle Cue's renames and follow
    /// the ones made in its terminal.
    pub(super) fn sync_agent_names(&self) {
        let listed: HashMap<String, String> = crate::live::claude().into_iter().map(|q| (q.session_id, q.name)).collect();
        if self.st().sessions.sync_agent_names(&listed, now_ms()) {
            self.changed();
        }
    }

    pub(super) fn deliver_deferred_rename(&self, session_id: &str) {
        let Some((name, origin)) = self.st().sessions.take_rename(session_id) else { return };
        if origin.machine.is_empty() {
            self.st().sessions.expect_rename(session_id, &name, now_ms() + 2_000 + RENAME_WITHIN_MS);
        }
        let (me, sid) = (self.shared(), session_id.to_string());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if let (Err(e), Some(me)) = (crate::focus::type_into(&origin, &format!("/rename {name}")), me) {
                me.rename_failed(&sid, &format!("Couldn't give Claude Code the new name: {e}"));
            }
        });
    }
}

/// A prompt open this long in a terminal before Cue makes a card for it: its own permission hook (which
/// asks Cue at once) gets there first.
const WAITING_GRACE_MS: u64 = 4_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_gone_from_claude_codes_running_list_is_ended_and_an_unreachable_machine_marked() {
        let dir = std::env::temp_dir().join(format!("cue-sweep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let here = Origin { session_id: "h1".into(), harness: "claude".into(), cwd: "/x".into(), ..Default::default() };
        let there = Origin { session_id: "m1".into(), harness: "claude".into(), cwd: "/x".into(), machine: "box".into(), ..Default::default() };
        let pi = Origin { session_id: "p1".into(), harness: "pi".into(), cwd: "/x".into(), ..Default::default() };
        for o in [&here, &there, &pi] {
            hub.event(o.clone(), "stopped", "done".into(), vec![], "");
        }
        let has = |sid: &str| hub.saved_sessions().iter().any(|s| s.origin.session_id == sid);
        let none: HashSet<String> = HashSet::new();
        // Missing a moment: kept (it may be starting up).
        hub.sweep("", Some(&none));
        assert!(has("h1"));
        // Missing long enough: ended, its card with it. Pi has no such list: left alone.
        hub.st().missing.insert("h1".into(), now_ms() - GONE_AFTER_MS - 1);
        hub.sweep("", Some(&none));
        assert!(!has("h1") && !hub.pending().iter().any(|i| i.origin.session_id == "h1"), "gone, card and all");
        assert!(has("p1"), "only Claude sessions are judged by Claude Code's list");
        assert!(has("m1"), "this Mac's list says nothing about a machine's sessions");
        // Its machine can't be reached: marked, Close allowed whatever it showed, and forget removes it.
        hub.sweep("box", None);
        assert!(hub.unreachable("m1") && hub.closable("m1").is_ok());
        hub.sweep("box", Some(&["m1".to_string()].into_iter().collect()));
        assert!(!hub.unreachable("m1") && has("m1"), "reached again, and still running there");
        hub.sweep("box", None);
        hub.forget("m1");
        assert!(!has("m1"));
    }

    #[test]
    fn renaming_shows_at_once_for_every_agent_and_says_when_the_agent_didnt_get_it() {
        let dir = std::env::temp_dir().join(format!("cue-rename-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        // No terminal Cue can type into (so nothing is typed anywhere real).
        let o = |sid: &str, h: &str| Origin { session_id: sid.into(), harness: h.into(), cwd: "/x/cue".into(), ..Default::default() };
        hub.event(o("cx", "codex"), "stopped", "done".into(), vec![], "");
        hub.event(o("pi", "pi"), "stopped", "done".into(), vec![], "");
        hub.event(o("cl", "claude"), "stopped", "done".into(), vec![], "");
        hub.event(o("busy", "claude"), "active", "working on it".into(), vec![], "");
        let shown = |sid: &str| hub.snapshot()["sessions"].as_array().unwrap().iter().find(|s| s["session_id"] == sid).cloned().unwrap();

        // Codex: Cue's name only, nothing to fail.
        assert_eq!(hub.rename("cx", "  tidy   up ").unwrap(), "Named in Cue (Codex has no names of its own)");
        assert_eq!(shown("cx")["name"], "tidy up");
        // Pi not connected: the name shows, and so does why Pi didn't get it.
        let e = hub.rename("pi", "pi-name").unwrap_err();
        assert_eq!(shown("pi")["name"], "pi-name");
        assert_eq!(shown("pi")["rename_error"], e);
        // Claude, idle, but Cue can't type into its terminal: the name shows, with the reason.
        let e = hub.rename("cl", "claude-name").unwrap_err();
        assert!(e.starts_with("Couldn't give Claude Code the new name"), "{e}");
        assert_eq!((shown("cl")["name"].as_str(), shown("cl")["rename_error"].as_str()), (Some("claude-name"), Some(e.as_str())));
        hub.dismiss_rename_error("cl");
        assert!(shown("cl").get("rename_error").is_none());
        // Claude, mid-turn: shown now, given to Claude Code when the turn ends; its old name doesn't win meanwhile.
        crate::live::set_claude(vec![crate::live::Quiet { session_id: "busy".into(), harness: "claude".into(), name: "old".into(), cwd: "/x/cue".into(), pid: 1, status: "busy".into(), waiting_for: String::new(), since_ms: 0 }]);
        hub.sync_agent_names();
        assert_eq!(shown("busy")["name"], "old", "Claude Code's name, Cue having none");
        hub.rename("busy", "later-name").unwrap();
        hub.sync_agent_names();
        assert_eq!(shown("busy")["name"], "later-name");
        assert!(shown("busy").get("rename_error").is_none());
        // Claude Code renamed in its own terminal: Cue follows.
        hub.st().sessions.take_rename("busy");
        crate::live::set_claude(vec![crate::live::Quiet { session_id: "busy".into(), harness: "claude".into(), name: "renamed-there".into(), cwd: "/x/cue".into(), pid: 1, status: "idle".into(), waiting_for: String::new(), since_ms: 0 }]);
        hub.sync_agent_names();
        assert_eq!(shown("busy")["name"], "renamed-there");
        // Gone: an error, nothing changed.
        assert!(hub.rename("nope", "x").is_err());
        crate::live::set_claude(vec![]);
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// End to end against a real Claude Code in tmux: `CUE_RENAME_PANE=%12 CUE_RENAME_SID=<id> cargo test
    /// -- --ignored renames_a_real_claude_code_session`. Renames that session (use a throwaway one).
    #[test]
    #[ignore]
    fn renames_a_real_claude_code_session() {
        let (Ok(pane), Ok(sid)) = (std::env::var("CUE_RENAME_PANE"), std::env::var("CUE_RENAME_SID")) else { return };
        let dir = std::env::temp_dir().join(format!("cue-rename-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = std::env::var("HOME").unwrap();
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let origin = Origin { session_id: sid.clone(), harness: "claude".into(), cwd: dir.to_string_lossy().into(), term_program: "tmux".into(), tmux_pane: pane, ..Default::default() };
        hub.event(origin, "stopped", "ready".into(), vec![], "");
        let wait_for = |want: &dyn Fn(&Value) -> bool| {
            for _ in 0..40 {
                std::env::set_var("HOME", &home);
                crate::live::refresh();
                hub.sync_agent_names();
                let s = hub.snapshot()["sessions"].as_array().unwrap().iter().find(|s| s["session_id"] == sid.as_str()).cloned().unwrap();
                if want(&s) {
                    return s;
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            panic!("timed out");
        };
        let name = format!("cue e2e “{}” #{}", "quoted", now_ms() % 1000);
        hub.rename(&sid, &name).unwrap();
        let s = hub.snapshot()["sessions"].as_array().unwrap().iter().find(|s| s["session_id"] == sid.as_str()).cloned().unwrap();
        assert_eq!(s["name"], name.as_str(), "shown at once");
        let s = wait_for(&|s| s["claude_name"] == name.as_str());
        assert!(s.get("rename_error").is_none(), "{s}");
        // Mid-turn: shown at once, typed only when the turn ends, then settled the same way.
        hub.event(Origin { session_id: sid.clone(), harness: "claude".into(), ..Default::default() }, "active", "a turn".into(), vec![], "");
        let name2 = format!("{name} mid-turn");
        hub.rename(&sid, &name2).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(3));
        crate::live::refresh();
        hub.sync_agent_names();
        let s = hub.snapshot()["sessions"].as_array().unwrap().iter().find(|s| s["session_id"] == sid.as_str()).cloned().unwrap();
        assert_eq!((s["name"].as_str(), s["claude_name"].as_str()), (Some(name2.as_str()), Some(name.as_str())), "not typed during the turn");
        hub.event(Origin { session_id: sid.clone(), harness: "claude".into(), ..Default::default() }, "stopped", "done".into(), vec![], "");
        let s = wait_for(&|s| s["claude_name"] == name2.as_str());
        assert!(s.get("rename_error").is_none(), "{s}");
        let name = name2;
        // A deadline already passed with Claude Code still on the old name: the notice.
        let old = name.clone();
        hub.st().sessions.set_name(&sid, "never-typed");
        hub.st().sessions.expect_rename(&sid, "never-typed", now_ms());
        let s = wait_for(&|s| s.get("rename_error").is_some());
        assert_eq!(s["name"], "never-typed");
        assert!(s["rename_error"].as_str().unwrap().contains(&old), "{s}");
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_prompt_open_in_its_terminal_gets_a_card_until_it_stops_waiting() {
        let dir = std::env::temp_dir().join(format!("cue-waiting-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = Origin { session_id: "s4".into(), harness: "claude".into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.event(origin, "active", "use the docs server".into(), vec![], "");
        let q = |status: &str, since_ms: u64| crate::live::Quiet { session_id: "s4".into(), harness: "claude".into(), name: String::new(), cwd: String::new(), pid: std::process::id() as i32, status: status.into(), waiting_for: "input needed".into(), since_ms };
        // Just opened: its own permission hook may still be on the way. No card yet.
        crate::live::set_claude(vec![q("waiting", now_ms())]);
        hub.sync_live();
        assert!(hub.pending().is_empty());
        // Open a while, and Cue has nothing for it: a card that sends you to the terminal.
        crate::live::set_claude(vec![q("waiting", now_ms() - 10_000)]);
        hub.sync_live();
        let items = hub.pending();
        assert_eq!((items.len(), items[0].kind.as_str(), items[0].message.as_str()), (1, "terminal", "Waiting in its terminal: input needed"));
        hub.sync_live();
        assert_eq!(hub.pending().len(), 1, "one card, however often it's seen");
        // Answered in the terminal: Claude Code says it's busy again, after the card came.
        crate::live::set_claude(vec![q("busy", now_ms() + 1)]);
        hub.sync_live();
        assert!(hub.pending().iter().all(|i| i.kind != "terminal"));
        crate::live::set_claude(vec![]);
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_turn_cue_missed_waits_from_when_it_ended_once() {
        let dir = std::env::temp_dir().join(format!("cue-missed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = Origin { session_id: "s3".into(), harness: "claude".into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.your_turn_since(origin.clone(), "Done: header moved.".into(), vec![], 1_000);
        hub.your_turn_since(origin, "Done: header moved.".into(), vec![], 1_000);
        let items = hub.pending();
        assert_eq!(items.len(), 1, "one card, however often it's seen");
        assert_eq!((items[0].kind.as_str(), items[0].created_ms, items[0].message.as_str()), ("waiting", 1_000, "Done: header moved."));
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_parked_session_is_kept_through_a_restart_and_comes_back_whole() {
        let dir = std::env::temp_dir().join(format!("cue-parked-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let origin = Origin { session_id: "p1".into(), harness: "codex".into(), cwd: "/x/infra".into(), tmux_pane: "%4".into(), ..Default::default() };
        let hub = Hub::new(None);
        hub.event(origin.clone(), "stopped", "Three resources drifted.".into(), vec![], "");
        hub.st().sessions.set_name("p1", "drift");
        hub.st().sessions.set_starred("p1", true);
        let rec = hub.session_record("p1").unwrap();
        hub.add_parked(rec);
        assert!(hub.session_origin("p1").is_none(), "off the live lists at once");
        assert_eq!(hub.snapshot()["parked"][0]["session_id"], "p1");

        // Cue restarts: still parked, with what it said.
        crate::db::reset();
        let hub = Hub::new(None);
        hub.set_starred("p1", false);
        assert_eq!(crate::db::parked()[0].session.starred_ms, 0, "unstarred while parked, and saved");
        hub.set_starred("p1", true);
        let p = hub.parked("p1").expect("parked survives a restart");
        assert!(p.session.thread.iter().any(|e| e.text.contains("Three resources drifted")));

        // Resumed in a new pane: its name, star and thread come back, at its prompt.
        let fresh = Origin { tmux_pane: "%9".into(), ..origin };
        hub.unparked(p, fresh, "");
        assert!(hub.parked("p1").is_none());
        let s = hub.session_record("p1").unwrap();
        assert_eq!((s.name.as_str(), s.state.as_str(), s.origin.tmux_pane.as_str()), ("drift", "waiting", "%9"));
        assert!(s.starred_ms > 0 && s.thread.iter().any(|e| e.text.contains("Three resources drifted")));
        crate::db::reset();
        assert!(Hub::new(None).parked("p1").is_none(), "resumed is no longer parked, after a restart too");

        // Closed by name: a session you named keeps that one name the whole way, from the moment its
        // row goes (the state has its record under `ended` at once, before anything is saved or asked
        // for), through a late hook from its exiting agent, into Closed, and after a relaunch.
        let hub = Hub::new(None);
        let named = Origin { session_id: "n1".into(), harness: "claude".into(), cwd: "/x/cue".into(), agent_pid: Some(424242), ..Default::default() };
        hub.event(named.clone(), "stopped", "Done.".into(), vec![], "");
        hub.st().sessions.set_name("n1", "cue-missing-messages");
        crate::db::sync_live(&hub.saved_sessions(), &[]); // as every change does in the app
        let name_in = |list: &Value| list.as_array().unwrap().iter().find(|v| v["session_id"] == "n1").map(|v| v["name"].as_str().unwrap_or("").to_string());
        let snap = hub.snapshot();
        assert_eq!(name_in(&snap["sessions"]).as_deref(), Some("cue-missing-messages"));
        assert_eq!(name_in(&snap["ended"]), None, "live: not under ended");
        hub.drop_session("n1");
        let snap = hub.snapshot();
        assert_eq!(name_in(&snap["sessions"]), None, "its row is gone");
        assert_eq!(name_in(&snap["ended"]).as_deref(), Some("cue-missing-messages"), "its record is there the same instant, name and all");
        let e = &snap["ended"][0];
        assert!(e["ended_ms"].as_u64().unwrap_or(0) > 0);
        assert_eq!((e["cwd"].as_str(), e["project"].as_str(), e["harness"].as_str()), (Some("/x/cue"), Some("cue"), Some("claude")));
        assert!(e.get("thread").is_none() && e.get("segments").is_none(), "slim: the snapshot carries what names it, not the whole record: {e}");
        assert!(hub.closed(24 * 3600 * 1000, 30)[0]["thread"].as_array().is_some_and(|t| !t.is_empty()), "Closed itself has the whole record");
        assert_eq!(name_in(&json!(hub.closed(24 * 3600 * 1000, 30))).as_deref(), Some("cue-missing-messages"), "Closed has it before the saved copy does");
        // Its agent's SessionEnd comes in after, as it does when Close ends the process: nothing comes back.
        hub.event(named.clone(), "ended", String::new(), vec![], "");
        let snap = hub.snapshot();
        assert_eq!(name_in(&snap["sessions"]), None, "a late end doesn't bring a nameless row back");
        assert_eq!(name_in(&snap["ended"]).as_deref(), Some("cue-missing-messages"));
        // Saved, and after a relaunch: the same record, the same name.
        crate::db::sync_live(&hub.saved_sessions(), &[]);
        let again = Hub::new(None);
        assert_eq!(name_in(&json!(again.closed(24 * 3600 * 1000, 30))).as_deref(), Some("cue-missing-messages"));
        assert!(again.snapshot()["ended"].as_array().unwrap().is_empty(), "the state's list is this run's; the saved one carries it on");
        // Live again (resumed): off ended, on sessions, as itself.
        hub.event(named, "stopped", "Back.".into(), vec![], "");
        let snap = hub.snapshot();
        assert_eq!(name_in(&snap["ended"]), None);
        crate::db::reset();

        // Ended (not parked): under Closed, with when it ended; parked or live ones aren't.
        let hub = Hub::new(None);
        hub.event(Origin { session_id: "c1".into(), harness: "claude".into(), cwd: "/x/lost".into(), ..Default::default() }, "stopped", "Halfway.".into(), vec![], "");
        crate::db::sync_live(&hub.saved_sessions(), &[]);
        hub.drop_session("c1");
        crate::db::sync_live(&hub.saved_sessions(), &[]);
        let closed = hub.closed(24 * 3600 * 1000, 30);
        assert!(closed.iter().any(|v| v["session_id"] == "c1" && v["ended_ms"].as_u64().unwrap_or(0) > 0), "an ended session is under Closed");
        assert!(!closed.iter().any(|v| v["session_id"] == "p1"), "a resumed one isn't");
        let (_, rec) = crate::db::ended_session("c1").unwrap();
        hub.add_parked(rec);
        assert!(!hub.closed(24 * 3600 * 1000, 30).iter().any(|v| v["session_id"] == "c1"), "parked, it leaves Closed");

        // Unparked: gone from the list, nothing started.
        let hub = Hub::new(None);
        hub.event(Origin { session_id: "p2".into(), harness: "pi".into(), cwd: "/x".into(), ..Default::default() }, "stopped", "Done.".into(), vec![], "");
        hub.add_parked(hub.session_record("p2").unwrap());
        assert!(hub.take_parked("p2").is_some());
        assert!(hub.parked("p2").is_none() && hub.session_origin("p2").is_none());
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn another_machines_folder_is_not_looked_up_on_this_mac() {
        let dir = std::env::temp_dir().join(format!("cue-remote-folder-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        // This repository, as a session's folder here; and a folder on another machine, under a path
        // that is macOS's automounter here (its lookup stalled a launch once).
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_str().unwrap().to_string();
        let there = "/home/someone/cue";
        hub.event(Origin { session_id: "here".into(), harness: "claude".into(), cwd: here.clone(), ..Default::default() }, "stopped", "done".into(), vec![], "");
        hub.event(Origin { session_id: "there".into(), harness: "claude".into(), cwd: there.into(), machine: "box".into(), ..Default::default() }, "stopped", "done".into(), vec![], "");
        let snap = hub.snapshot();
        assert_eq!(snap["projects"][&here]["name"], "cue", "a folder here is looked up");
        assert!(snap["branches"].get(&here).is_some());
        assert!(snap["projects"].get(there).is_none() && snap["branches"].get(there).is_none(), "{}", snap["branches"]);
    }
}
