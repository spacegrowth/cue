//! What each working session is doing, read every second from the end of its transcript (no hook involved,
//! so it costs the agent nothing): stopped mid-turn, gone quiet, waiting on its background agents, a task
//! that failed, compacting, a screen of its own before Claude Code starts.

use super::*;
use super::turn_end::TAKEN_SETTLE_MS;

/// The last background agent reported: Claude Code starts the session's next turn within a moment.
/// Past this with no turn, its last reply is taken as your turn.
const HELPERS_SETTLE_SECS: u64 = 15;

/// No new output for this long while running a command, or while only thinking: stuck.
const STUCK_RUNNING_MS: u64 = 10 * 60_000;
const STUCK_THINKING_MS: u64 = 5 * 60_000;

/// After Cue stops a turn to send your message now, this long before the stop counts as yours: typing the
/// message into a machine's terminal takes a few round trips, and a failed send says so itself.
const STOPPED_TO_SEND_MS: u64 = 30_000;
/// How far a machine's log copy may lag behind its hook: a turn begun this recently isn't read as stopped.
const LOG_LAG_MS: u64 = 10_000;

/// What the transcripts said as a refresh began, put in the store as it ends.
struct Reads {
    /// What each working session is doing now, and since when.
    doing: Vec<(String, (String, u64))>,
    /// Each session's agents, as its chat lists them (its card says the same count).
    agents: Vec<(String, u64)>,
    /// Each working session's queued message: session, harness, log, the message.
    queued: Vec<(String, String, String, Exchange)>,
}

/// A working session that may have gone quiet: its log, when it began working, what it's doing, when it was
/// last said to be stuck (0: it isn't), and its name for the notification.
struct Quiet {
    sid: String,
    path: String,
    since: u64,
    doing: String,
    was: u64,
    name: String,
}

impl Hub {
    /// Every second: what each working Claude Code session is doing now, read from the end of its
    /// transcript (no hook involved, so it costs the agent nothing).
    pub fn refresh_activity(&self) {
        self.sync_agent_names();
        let reads = self.transcript_reads();
        let now = now_ms();
        self.catch_interrupted(now);
        self.catch_stuck(now);
        self.watch_helpers();
        self.note_task_failures();
        self.watch_compacting(now);
        self.read_setup_screens(now);
        self.watch_trust(now);
        self.put_reads(reads, now);
        self.read_about();
    }

    /// Your latest request to each Claude session the window lists, read again from its transcript here (the
    /// store let go), so building the window's state reads none (`folders_view` takes what's kept).
    fn read_about(&self) {
        let (_, _, transcripts, _) = self.snapshot_sources();
        let paths: Vec<String> = transcripts.into_iter().map(|(_, p)| p).collect();
        if crate::live::read_about(&paths) {
            self.changed();
        }
    }

    /// The working sessions' logs, read with the store let go. Agents are counted only while something reads a
    /// session's feed (its chat open), so idle ones cost nothing.
    fn transcript_reads(&self) -> Reads {
        let (working, agent_logs, queued) = self.read(|st| {
            let working = st.sessions.working_transcripts();
            let queued = working.iter().filter_map(|(sid, h, path)| st.sessions.queued(sid).map(|q| (sid.clone(), h.clone(), path.clone(), q))).collect();
            (working, st.sessions.agent_transcripts(), queued)
        });
        let doing = working.into_iter().filter_map(|(sid, harness, path)| crate::transcript::activity_of(&harness, &path).map(|a| (sid, a))).collect();
        let agents = agent_logs.into_iter().filter_map(|(sid, path, at_work)| crate::steps::agent_count(&path, at_work).map(|n| (sid, n as u64))).collect();
        Reads { doing, agents, queued }
    }

    /// Interrupted (Esc, on purpose or not) a few seconds ago with nothing since: no Stop comes for
    /// that, so it lands in Waiting as "interrupted", with Continue. A few seconds: ⌘Enter's Esc
    /// is followed at once by your message.
    /// Not when the stop was Cue's own, to send your message now: typing it into a machine's terminal
    /// can take longer than that. Nor when a message reached it since moments ago: a machine's log copy
    /// can lag behind its hook, still ending on the interrupt while the agent works on what came next.
    fn catch_interrupted(&self, now: u64) {
        let candidates: Vec<(Origin, u64)> = self.read(|st| {
            let recent = |m: &HashMap<String, u64>, sid: &str, ms: u64| m.get(sid).is_some_and(|t| now.saturating_sub(*t) < ms);
            st.sessions.all().into_iter().filter(|s| s.state == "working" && s.origin.harness == "claude" && !s.origin.transcript_path.is_empty())
                .filter(|s| !recent(&st.stopped_to_send, &s.origin.session_id, STOPPED_TO_SEND_MS) && !recent(&st.active_at, &s.origin.session_id, LOG_LAG_MS))
                .map(|s| (s.origin, s.since_ms)).collect()
        });
        let stopped = candidates.into_iter()
            .filter(|(o, since)| transcript::interrupted_at(&o.transcript_path).is_some_and(|at| at + 1500 >= *since && now.saturating_sub(at) > 3000))
            .map(|(o, _)| o);
        for o in stopped {
            let sid = o.session_id.clone();
            let driver = crate::leads::resolve_driver(&sid, "");
            self.event(o, "stopped", "Interrupted: it stopped mid-turn and is waiting for you. Say “continue” to pick up where it left off.".into(), vec![], &driver);
            self.write(|st| {
                if let Some(it) = st.items.iter_mut().find(|i| i.kind == "waiting" && i.origin.session_id == sid) {
                    it.interrupted = true;
                }
            });
            self.changed();
        }
    }

    /// Stuck: working, but nothing new in its transcript for 10 minutes while it runs a command (a long
    /// build is fine; one waiting for input in its terminal isn't), or 5 while it's only thinking. Not
    /// while a helper agent works (that writes to its own log). Said once per quiet spell.
    fn catch_stuck(&self, now: u64) {
        let working: Vec<Quiet> = self.read(|st| {
            st.sessions.all().into_iter().filter(|s| s.state == "working" && s.origin.harness != "pi" && !s.origin.transcript_path.is_empty())
                .map(|s| Quiet { sid: s.origin.session_id, path: s.origin.transcript_path, since: s.since_ms, doing: s.doing, was: s.stuck_ms, name: if s.name.is_empty() { crate::model::folder_of(&s.origin.cwd) } else { s.name } })
                .collect()
        });
        // When each log was last written: looked at with the store let go (a stat can be slow).
        for q in working {
            let Some(written) = written_ms(&q.path) else { continue };
            self.quiet_since(&q, written.max(q.since), now);
        }
    }

    /// A working session whose log was last written `at`: stuck once it's been quiet too long (said once per
    /// quiet spell), and no longer once it writes again.
    fn quiet_since(&self, q: &Quiet, at: u64, now: u64) {
        let limit = if q.doing.starts_with("Delegating") { u64::MAX } else if q.doing.starts_with("Running") || q.doing.starts_with("Using") { STUCK_RUNNING_MS } else { STUCK_THINKING_MS };
        let stuck = now.saturating_sub(at) >= limit;
        let sid = &q.sid;
        if stuck && q.was != at {
            self.write(|st| st.sessions.set_stuck(sid, at));
            self.changed();
            if crate::config::flag("/notify/decisions") {
                let what = if q.doing.is_empty() { "Thinking".to_string() } else { q.doing.trim_end_matches('…').to_string() };
                self.notify_about(&format!("stuck-{sid}"), &format!("{} · no new output for {}m", q.name, now.saturating_sub(at) / 60_000), &format!("{what}. If it's waiting for input, it's in its terminal."));
            }
        } else if !stuck && q.was != 0 {
            self.write(|st| st.sessions.set_stuck(sid, 0));
            self.ports.notes.take_back(vec![format!("stuck-{sid}")]);
            self.changed();
        }
    }

    /// At its prompt while its background agents work: how many are left. Once none is, Claude Code
    /// wakes it with their reports (a new turn); if that doesn't come, its last reply is your turn.
    fn watch_helpers(&self) {
        let with_helpers = self.read(|st| st.sessions.with_helpers());
        for o in with_helpers {
            let left = crate::steps::running_helpers(&o.transcript_path).len() as u64;
            let background = crate::steps::running_background(&o.transcript_path) as u64;
            let sid = o.session_id.clone();
            let changed = self.write(|st| st.sessions.set_helpers(&sid, left) | st.sessions.set_background(&sid, background));
            if changed || left == 0 {
                self.changed();
            }
            if left > 0 {
                continue;
            }
            self.later(HELPERS_SETTLE_SECS * 1000, move |me: &Arc<Hub>| {
                let still = me.read(|st| st.sessions.state(&sid).as_deref() == Some("agent"));
                if still {
                    if let Some((at, reply, parts)) = transcript::turn_ended_parts(&o.transcript_path) {
                        me.your_turn_since(o, reply, parts, at);
                        me.changed();
                    }
                }
            });
        }
    }

    /// A background task that failed after running a while (noted by its session's steps feed): on the
    /// session, so the window and the add-on can say so, and a notification like a stuck session's.
    fn note_task_failures(&self) {
        let failures = crate::steps::take_task_failures();
        if failures.is_empty() {
            return;
        }
        let told: Vec<(String, String, String, String)> = self.write(|st| {
            let mut told = vec![];
            for f in failures {
                let Some(s) = st.sessions.all().into_iter().find(|s| s.origin.transcript_path == f.path) else { continue };
                let sid = s.origin.session_id.clone();
                if st.sessions.set_task_failed(&sid, f.at_ms, &f.desc, &f.line) {
                    told.push((sid, s.project.clone(), f.desc, f.line));
                }
            }
            told
        });
        if told.is_empty() {
            return;
        }
        self.changed();
        if crate::config::flag("/notify/decisions") {
            for (sid, project, desc, line) in told {
                self.notify_about(&format!("task-failed-{sid}"), &format!("{project}: background task failed"), &(if line.is_empty() { desc } else { line }));
            }
        }
    }

    /// Compacting (from Cue, its terminal, or on its own) until its transcript says it's done; given up after 15 minutes.
    fn watch_compacting(&self, now: u64) {
        let compacting = self.read(|st| st.sessions.compacting());
        let compacted: Vec<(String, bool)> = compacting.into_iter()
            .filter_map(|(sid, path, at)| if transcript::compact_finished(&path, at) { Some((sid, true)) } else { (now.saturating_sub(at) > 15 * 60_000).then_some((sid, false)) })
            .collect();
        if compacted.is_empty() {
            return;
        }
        // A compact at its prompt (no turn to end): a message kept for it meanwhile goes now it's done.
        let send_kept: Vec<String> = self.write(|st| {
            let mut send_kept = vec![];
            for (sid, done) in &compacted {
                if *done {
                    st.sessions.set_compacted(sid, now);
                    if st.sessions.held(sid).is_some() && st.sessions.state(sid).as_deref() != Some("working") {
                        send_kept.push(sid.clone());
                    }
                } else {
                    st.sessions.set_compacting(sid, 0);
                }
            }
            send_kept
        });
        self.changed();
        for sid in send_kept {
            self.send_held_soon(&sid, 500);
        }
    }

    /// Before Claude Code starts (no hook from it yet), its terminal can sit on a screen of its own: the
    /// trust question, or one waiting for Enter. Read off its pane: this Mac's now, a machine's from
    /// its last listing (leads::refresh_remote, no SSH here). Shown as such, not as at work.
    fn read_setup_screens(&self, now: u64) {
        let before = self.read(|st| st.sessions.before_start());
        let mut seen: Vec<(String, Option<crate::dialog::Setup>)> = vec![];
        for o in before {
            let setup = if o.machine.is_empty() {
                match self.keys().screen(&o) {
                    Ok(screen) => crate::dialog::setup_on(&screen),
                    Err(_) => continue,
                }
            } else if crate::leads::remote_screens_known(&o.machine) {
                crate::leads::remote_setup(&o.machine, &o.tmux_pane)
            } else {
                continue;
            };
            seen.push((o.session_id, setup));
        }
        if seen.is_empty() {
            return;
        }
        if self.write(|st| seen.iter().fold(false, |any, (sid, setup)| st.sessions.set_setup(sid, *setup, now) || any)) {
            self.changed();
        }
    }

    /// A session Cue started in a folder Claude Code didn't trust yet, in a terminal Cue can't read:
    /// asking until ~/.claude.json says it's trusted (answered there). Given up after 30 minutes. Only
    /// on this Mac: a machine's folder is never looked up here.
    /// ~/.claude.json can be megabytes: read after the store is let go.
    fn watch_trust(&self, now: u64) {
        let asking: Vec<(String, String, u64)> = self.read(|st| {
            st.sessions.asking_trust().into_iter().filter(|(sid, _, _)| st.sessions.origin(sid).is_some_and(|o| o.machine.is_empty() && o.tmux_pane.is_empty())).collect()
        });
        let done: Vec<String> = asking.into_iter().filter(|(_, cwd, at)| now.saturating_sub(*at) > 30 * 60_000 || crate::focus::claude_trusts(cwd)).map(|(sid, _, _)| sid).collect();
        if done.is_empty() {
            return;
        }
        self.write(|st| {
            for sid in &done {
                st.sessions.set_trust(sid, 0);
            }
        });
        self.changed();
    }

    /// What the logs said as the refresh began, in the store: what each session is doing, its agents, and the
    /// queued messages it has taken since (into the running turn, or as the next prompt): no longer queued,
    /// so no "Queued" bubble pinned below the answer, and no Send now.
    fn put_reads(&self, reads: Reads, now: u64) {
        // Taken as the next turn's prompt only counts once the turn before it has had its say: the
        // queue is read the moment a turn ends, a hair before that turn's Stop, which moves it below the reply.
        let taken: Vec<String> = reads.queued.into_iter()
            .filter(|(_, h, path, q)| match transcript::read_state(h, path, &q.text, q.at_ms) {
                Some(transcript::Queued::Absorbed) => true,
                Some(transcript::Queued::Taken(at)) => at.map_or(true, |at| now.saturating_sub(at) > TAKEN_SETTLE_MS),
                _ => false,
            })
            .map(|(sid, _, _, _)| sid)
            .collect();
        let changed = self.write(|st| {
            for sid in &taken {
                st.sessions.set_queued(sid, None);
            }
            let any = reads.agents.iter().fold(!taken.is_empty(), |any, (sid, n)| st.sessions.set_agents(sid, *n) || any);
            reads.doing.iter().fold(any, |any, (sid, (a, at))| st.sessions.set_doing(sid, a, *at) || any)
        });
        if changed {
            self.changed();
        }
    }
}

/// When the file at `path` was last written (ms since 1970), if it's there.
pub(super) fn written_ms(path: &str) -> Option<u64> {
    let at = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    at.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::transcript::iso;

    #[test]
    fn a_turn_cue_stopped_to_send_now_isnt_called_interrupted_and_one_you_stopped_is() {
        let dir = std::env::temp_dir().join(format!("cue-int-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let interrupted = |sid: &str| -> Origin {
            let path = dir.join(format!("{sid}.jsonl"));
            let o = Origin { session_id: sid.into(), harness: "claude".into(), cwd: "/x/proj".into(), transcript_path: path.to_string_lossy().into(), ..Default::default() };
            hub.mark_state(&o, "working");
            // Stopped a second after it began: its log ends on the interrupt, nothing after it.
            let at = now_ms() - 1000;
            std::fs::write(&path, format!("{{\"type\":\"user\",\"timestamp\":\"{}\",\"message\":{{\"content\":\"on the pill\"}}}}\n{{\"type\":\"user\",\"timestamp\":\"{}\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"[Request interrupted by user]\"}}]}}}}\n", iso(at - 500), iso(at))).unwrap();
            o
        };
        let cue_stopped = interrupted("now1");
        hub.write(|st| st.stopped_to_send.insert("now1".into(), now_ms()));   // ⌘↵: Cue's own Esc, your message on its way
        let lagging = interrupted("lag1");
        let you_stopped = interrupted("esc1");
        std::thread::sleep(std::time::Duration::from_millis(2300));
        hub.write(|st| st.active_at.insert("lag1".into(), now_ms()));   // its next message reached Cue; the log copy hasn't caught up
        hub.refresh_activity();
        let state = |o: &Origin| hub.read(|st| st.sessions.state(&o.session_id));
        assert_eq!(state(&cue_stopped).as_deref(), Some("working"), "Cue stopped it to send yours: still at work, no Interrupted card");
        assert_eq!(state(&lagging).as_deref(), Some("working"), "a message just reached it: the log copy is behind, not stopped");
        assert_eq!(state(&you_stopped).as_deref(), Some("waiting"), "you stopped it: Interrupted, waiting for you");
        assert!(hub.pending().iter().any(|i| i.origin.session_id == "esc1" && i.interrupted));
        assert!(!hub.pending().iter().any(|i| i.origin.session_id != "esc1"));
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
