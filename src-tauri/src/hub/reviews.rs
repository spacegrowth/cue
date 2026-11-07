//! Reviews typed into a lead for an executor: sent when the lead is free, queued while it is busy.

use super::*;

/// After typing a review into a lead, wait this long before judging it free again.
pub(super) const REVIEW_SETTLE_MS: u64 = 8000;

/// A review's row shows this many lines of findings.
const FINDING_LINES: usize = 10;

/// "in review" stops showing after this long even if the executor still reads as done.
const REVIEW_SHOWN_MS: u64 = 15 * 60_000;

/// What Review does with a review (`take_review`).
enum Taken {
    /// Refused, or nothing to do: what to say.
    Answer(Result<String, String>),
    /// Behind its busy lead (why it's busy), to be saved with the queue.
    Queued(QueuedReview, String),
    /// Its lead is free: typed in now.
    Send,
}

impl Hub {
    /// Review: typed into its lead now if the lead is free, else queued (in order, saved) and sent
    /// the moment it is (see `drain_reviews`). Never refused for being busy.
    pub fn review(&self, lead: &str, exec: &str, text: &str) -> Result<String, String> {
        match self.write(|st| Self::take_review(st, lead, exec, text, now_ms())) {
            Taken::Answer(r) => r,
            Taken::Queued(q, why) => {
                crate::db::queue_review(&q.exec, &q.lead, &q.text, q.at_ms);
                self.changed();
                Ok(format!("Queued: its lead is {why}, Cue sends it when it's free"))
            }
            Taken::Send => {
                self.changed(); // "in review" shows now; typing it in takes a moment
                self.send_review(lead, exec, text)
            }
        }
    }

    /// What Review does with `exec`'s review, decided and noted in the store: refused or already done, queued
    /// behind its busy lead, or sent now.
    fn take_review(st: &mut Store, lead: &str, exec: &str, text: &str, now: u64) -> Taken {
        if st.review_sent.contains_key(exec) {
            return Taken::Answer(Err("its lead is already reviewing it".into()));
        }
        if st.review_queue.iter().any(|q| q.exec == exec) {
            return Taken::Answer(Ok("Already queued: it goes when its lead is free".into()));
        }
        // A review of this packet already ran (relay saved it or ledgered it): a second one is the same work twice.
        if crate::leads::is_reviewed(exec) {
            return Taken::Answer(Ok("Its lead already reviewed this report".into()));
        }
        // Behind another queued review, or the lead is busy: queue it.
        let ahead = st.review_queue.iter().any(|q| q.lead == lead);
        if let Some(why) = Self::lead_busy(st, lead, now).or_else(|| ahead.then(|| "reviewing".to_string())) {
            let q = QueuedReview { exec: exec.into(), lead: lead.into(), text: text.into(), at_ms: now };
            st.review_queue.push(q.clone());
            return Taken::Queued(q, why);
        }
        st.review_sent.insert(exec.to_string(), (lead.to_string(), now));
        st.review_last.insert(lead.to_string(), now);
        Taken::Send
    }

    fn send_review(&self, lead: &str, exec: &str, text: &str) -> Result<String, String> {
        match self.send_to_session(lead, text, &[], false) {
            Ok(_) => Ok(format!("Sent {text}")),
            Err(e) => {
                self.write(|st| st.review_sent.remove(exec));
                self.changed();
                Err(e)
            }
        }
    }

    /// Take a queued review back (× on its line in the lead's chat).
    pub fn unqueue_review(&self, exec: &str) {
        self.write(|st| st.review_queue.retain(|q| q.exec != exec));
        crate::db::unqueue_review(exec);
        self.changed();
    }

    /// Send each lead its next queued review once it's free (its turn ended, nothing asked, not
    /// compacting, no message of yours waiting to go first). One per lead at a time: the next waits
    /// until that review is done. A review whose executor is no longer waiting for one (closed, or
    /// back at work) is dropped. Blocking (typing takes a moment): call off the UI thread.
    pub fn drain_reviews(&self) {
        let (stale, due) = self.write(|st| Self::take_due_reviews(st, now_ms()));
        if due.is_empty() && stale.is_empty() {
            return;
        }
        for exec in stale.iter().chain(due.iter().map(|q| &q.exec)) {
            crate::db::unqueue_review(exec);
        }
        self.changed();
        for q in due {
            let _ = self.send_review(&q.lead, &q.exec, &q.text);
        }
    }

    /// The queue's moot reviews dropped (their executors), and each free lead's next review taken off it to
    /// send now, noted as sent.
    fn take_due_reviews(st: &mut Store, now: u64) -> (Vec<String>, Vec<QueuedReview>) {
        // Moot: no longer waiting for a review, or a review of it already ran.
        let stale: Vec<String> = st.review_queue.iter().filter(|q| !crate::leads::is_done(&q.exec) || crate::leads::is_reviewed(&q.exec)).map(|q| q.exec.clone()).collect();
        st.review_queue.retain(|q| !stale.contains(&q.exec));
        let mut due: Vec<QueuedReview> = vec![];
        let leads: Vec<String> = st.review_queue.iter().map(|q| q.lead.clone()).collect();
        for lead in leads {
            if due.iter().any(|d| d.lead == lead) || st.sessions.held(&lead).is_some() || Self::lead_busy(st, &lead, now).is_some() {
                continue;
            }
            let Some(i) = st.review_queue.iter().position(|q| q.lead == lead) else { continue };
            let q = st.review_queue.remove(i);
            st.review_sent.insert(q.exec.clone(), (lead.clone(), now));
            st.review_last.insert(lead, now);
            due.push(q);
        }
        (stale, due)
    }

    /// Why a lead can't take a review right now, or None when it's free: working (a review or anything
    /// else), compacting, asking you something, or a review just typed in (it takes a moment to show as working).
    fn lead_busy(st: &Store, lead: &str, now: u64) -> Option<String> {
        let reviewing = || st.review_sent.iter().filter(|(_, (l, _))| l == lead).max_by_key(|(_, (_, at))| *at).map(|(e, _)| crate::leads::name_of(e));
        let settling = now.saturating_sub(st.review_last.get(lead).copied().unwrap_or(0)) < REVIEW_SETTLE_MS;
        if settling || st.sessions.state(lead).as_deref() == Some("working") {
            return Some(reviewing().map_or_else(|| "busy".to_string(), |n| format!("reviewing {n}")));
        }
        if st.sessions.compacting_since(lead) > 0 {
            return Some("compacting".into());
        }
        if st.items.iter().any(|i| i.origin.session_id == lead && i.kind != "waiting") {
            return Some("asking you something".into());
        }
        None
    }

    /// When the review rules above flip with time alone: a lead is no longer settling (REVIEW_SETTLE_MS after
    /// its last review), and "in review" stops showing (REVIEW_SHOWN_MS after it was sent).
    pub(super) fn review_changes(st: &Store, now: u64, next: &mut super::shown::NextChange) {
        for at in st.review_last.values() {
            next.at(at + REVIEW_SETTLE_MS, now);
        }
        for (_, at) in st.review_sent.values() {
            next.at(at + REVIEW_SHOWN_MS, now);
        }
    }

    /// Set aside (in a Review row's ⋯ menu): this executor's current packet stops waiting on you. Cue's own
    /// note, saved; relay isn't told, and the executor's next packet shows a card again.
    pub fn set_aside(&self, exec: &str) -> Result<String, String> {
        let (id, packet) = crate::leads::packet_of(exec).ok_or("Cue doesn't know this executor's packet")?;
        crate::db::add_set_aside(&id, packet);
        self.write(|st| st.set_aside.insert((id, packet)));
        self.changed();
        Ok("Set aside".into())
    }

    /// For every screen: where each packet that waits on you stands in its review, by executor, in one
    /// place. The apps show it as it is. Per packet: `state` (`waiting`: nothing has reviewed it yet;
    /// `auto`: its lead is on auto mode and reviews it on its own; `queued`: your Review waits for its lead;
    /// `reviewing`: your Review is typed in and running; `reviewed`: a review ran, it waits for you to
    /// commit or send fixes), `missed` (relay's wake-up never reached its lead), and what a card shows:
    /// executor, lead, packet, goal, outcome, the diff's size (`stat`), the review's findings and what it recommends.
    pub(super) fn reviews_view(st: &mut Store) -> Value {
        let now = now_ms();
        // "in review" until its lead has finished its turn after a review ran (or 15 minutes).
        let working = |s: &Store, lead: &str| s.sessions.state(lead).as_deref() == Some("working");
        let done: Vec<String> = st.review_sent.iter().filter(|(e, (lead, _))| crate::leads::is_reviewed(e) && !working(st, lead)).map(|(e, _)| e.clone()).collect();
        st.review_sent.retain(|exec, (_, at)| now.saturating_sub(*at) < REVIEW_SHOWN_MS && crate::leads::is_done(exec) && !done.contains(exec));
        let free = |s: &Store, lead: &str| Self::lead_busy(s, lead, now).is_none();
        let missed: HashSet<String> = crate::leads::missed(|lead| free(st, lead)).into_iter().collect();
        let mut out = serde_json::Map::new();
        for (sid, m, lead_auto) in crate::leads::needing_you() {
            if m.packet.is_some_and(|n| st.set_aside.contains(&(m.id.clone(), n))) {
                continue;
            }
            let missed = missed.contains(&sid);
            let state = if st.review_queue.iter().any(|q| q.exec == sid) {
                "queued"
            } else if st.review_sent.contains_key(&sid) {
                "reviewing"
            } else if m.reviewed {
                "reviewed"
            } else if lead_auto && !missed {
                "auto"
            } else {
                "waiting"
            };
            let (findings, recommendation) = if state == "reviewed" { Self::findings(st, &m) } else { (vec![], String::new()) };
            out.insert(sid, json!({
                "state": state, "missed": missed, "name": m.name, "lead": m.lead, "packet": m.packet, "goal": m.goal, "outcome": m.outcome,
                "stat": Self::stat_line(&m), "findings": findings, "recommendation": recommendation,
            }));
        }
        Value::Object(out)
    }

    /// "3 files · +94 −2": the size of what the packet staged, when relay recorded it.
    fn stat_line(m: &crate::leads::Member) -> String {
        m.diff_size.filter(|(files, ..)| *files > 0).map(|(files, added, removed)| format!("{files} file{} · +{added} −{removed}", if files == 1 { "" } else { "s" })).unwrap_or_default()
    }

    /// A review's findings (a few lines) and what it recommends ("commit", "fix-list", "" when it doesn't say):
    /// relay's saved review, else what the lead said last (an inline review has no file: its reply is the review).
    fn findings(st: &Store, m: &crate::leads::Member) -> (Vec<String>, String) {
        let reply = || st.items.iter().find(|i| i.kind == "waiting" && i.origin.session_id == m.lead).map(|i| if i.message.is_empty() { i.context.last().map(|c| c.text.clone()).unwrap_or_default() } else { i.message.clone() });
        let text = if m.review.is_empty() { reply().unwrap_or_default() } else { m.review.clone() };
        let said = |l: &str| l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case("recommendation")).map(|(_, v)| v.split(['.', '(', ',', ';']).next().unwrap_or("").trim().to_string());
        let lines = || text.lines().map(str::trim).filter(|l| !l.is_empty());
        let recommendation = lines().find_map(said).unwrap_or_default();
        let findings = lines().filter(|l| said(l).is_none() && !l.trim_end_matches(':').eq_ignore_ascii_case("findings")).take(FINDING_LINES).map(String::from).collect();
        (findings, recommendation)
    }

    /// For every screen: each lead's queued reviews, in order (shown in its chat, each with ×).
    pub(super) fn review_queue_view(st: &Store) -> Value {
        let mut out = serde_json::Map::new();
        for q in &st.review_queue {
            let row = json!({ "exec": q.exec, "name": crate::leads::name_of(&q.exec), "at_ms": q.at_ms });
            out.entry(q.lead.clone()).or_insert_with(|| json!([])).as_array_mut().unwrap().push(row);
        }
        Value::Object(out)
    }
}

#[cfg(test)]
impl Hub {
    /// Tests: the executors whose reviews are queued, in order, as the store has them.
    pub(crate) fn queued_reviews(&self) -> Vec<String> {
        self.read(|st| st.review_queue.iter().map(|q| q.exec.clone()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lead_just_handed_a_review_is_busy_until_it_settles() {
        let mut st = Store::default();
        let t = 1_000_000;
        assert_eq!(Hub::lead_busy(&st, "L", t), None);
        st.review_sent.insert("E1".into(), ("L".into(), t));
        st.review_last.insert("L".into(), t);
        assert_eq!(Hub::lead_busy(&st, "L", t + 1000), Some("reviewing E1".into()));
        assert_eq!(Hub::lead_busy(&st, "L", t + REVIEW_SETTLE_MS + 1), None);
        assert_eq!(Hub::lead_busy(&st, "OTHER", t + 1000), None);
    }

    /// The snapshot is built again when the lead stops settling and when "in review" stops showing: the
    /// times those rules flip, and none once they're past.
    #[test]
    fn a_review_says_when_its_card_changes_with_time_alone() {
        let mut st = Store::default();
        let t = 1_000_000;
        st.review_sent.insert("E1".into(), ("L".into(), t));
        st.review_last.insert("L".into(), t);
        let next = |now| { let mut n = crate::hub::shown::NextChange::default(); Hub::review_changes(&st, now, &mut n); n.when() };
        assert_eq!(next(t + 1000), Some(t + REVIEW_SETTLE_MS), "the settle ends first");
        assert_eq!(Hub::lead_busy(&st, "L", t + REVIEW_SETTLE_MS - 1).is_some(), true, "busy until then");
        assert_eq!(Hub::lead_busy(&st, "L", t + REVIEW_SETTLE_MS), None, "free from then");
        assert_eq!(next(t + REVIEW_SETTLE_MS), Some(t + REVIEW_SHOWN_MS), "then in review stops showing");
        assert_eq!(next(t + REVIEW_SHOWN_MS), None);
    }

    /// One state per packet that waits on you, from every fact: a woken lead is nothing yet (waiting), a lead on auto reviews on
    /// its own, a review waiting for its lead is queued, one Cue sent is reviewing, one that ran is reviewed, set aside is gone.
    #[test]
    fn each_packet_waiting_on_you_has_one_state() {
        let _g = crate::leads::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _d = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-rs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        let exec = |woken: bool, reviewed: bool, auto: bool| {
            let lead = crate::leads::Member { role: "lead".into(), plugin: "relay".into(), auto: Some(auto), ..Default::default() };
            let e = crate::leads::Member { woken, reviewed, ..crate::leads::reported_exec() };
            crate::leads::install_index([("L".to_string(), lead), ("E".to_string(), e)].into());
        };
        let state = |hub: &Hub| hub.write(Hub::reviews_view)["E"]["state"].clone();
        exec(true, false, false);
        assert_eq!(state(&hub), "waiting", "woken but unreviewed: it waits for you");
        exec(true, false, true);
        assert_eq!(state(&hub), "auto", "its lead is on auto mode and woken");
        hub.write(|st| st.review_queue.push(QueuedReview { exec: "E".into(), lead: "L".into(), text: "r".into(), at_ms: 1 }));
        assert_eq!(state(&hub), "queued");
        hub.write(|st| st.review_queue.clear());
        hub.write(|st| st.review_sent.insert("E".into(), ("L".into(), now_ms())));
        assert_eq!(state(&hub), "reviewing", "Review sent");
        exec(true, true, true);
        assert_eq!(state(&hub), "reviewed", "the review ran (a saved review or relay's ledger): the in-review note is dropped");
        assert!(hub.read(|st| st.review_sent.is_empty()));
        hub.set_aside("E").unwrap();
        assert!(hub.write(Hub::reviews_view).get("E").is_none(), "set aside: no longer waiting on you");
        assert!(Hub::new(None).read(|st| st.set_aside.contains(&("ex".to_string(), 1))), "kept across a restart");
        crate::db::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A lead whose wake-up never arrived is waiting for you, never "reviewing on its own", even on auto mode; the
    /// findings of a review are its saved text, else the lead's last reply.
    #[test]
    fn a_missed_wake_shows_as_waiting_and_a_reviews_findings_come_from_its_file_or_the_leads_reply() {
        let _g = crate::leads::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut st = Store::default();
        let lead = crate::leads::Member { role: "lead".into(), plugin: "relay".into(), auto: Some(true), ..Default::default() };
        let late = crate::leads::Member { ready_late: true, ..crate::leads::reported_exec() };
        crate::leads::install_index([("L".to_string(), lead), ("E".to_string(), late)].into());
        let v = Hub::reviews_view(&mut st);
        assert_eq!((v["E"]["state"].as_str(), v["E"]["missed"].as_bool()), (Some("waiting"), Some(true)));
        let m = crate::leads::Member { review: "Findings:\n1. one\n\n2. two\nRecommendation: fix-list (finding 1).\n".into(), ..crate::leads::reported_exec() };
        assert_eq!(Hub::findings(&st, &m), (vec!["1. one".to_string(), "2. two".to_string()], "fix-list".to_string()), "the heading and the recommendation aren't findings");
        let mut said = Hub::new_item(&mut st, "waiting", Origin { session_id: "L".into(), ..Default::default() });
        said.message = "looks right\n1. only thing".into();
        st.items.push(said);
        assert_eq!(Hub::findings(&st, &crate::leads::reported_exec()).0, ["looks right", "1. only thing"], "an inline review has no file: the reply is the review");
    }

    #[test]
    fn a_review_for_a_busy_lead_is_queued_kept_across_restarts_and_dropped_once_moot() {
        let dir = std::env::temp_dir().join(format!("cue-rq-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _l = crate::leads::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        // The lead was just handed a review (it takes a moment to show as working): busy.
        hub.write(|st| {
            st.review_sent.insert("E0".into(), ("L".into(), now_ms()));
            st.review_last.insert("L".into(), now_ms());
        });
        let r = hub.review("L", "E1", "/relay:review e1").unwrap();
        assert!(r.starts_with("Queued: its lead is reviewing"), "{r}");
        assert!(hub.review("L", "E1", "/relay:review e1").unwrap().starts_with("Already queued"), "twice doesn't queue it twice");
        hub.review("L", "E2", "/relay:review e2").unwrap();
        let q = hub.read(Hub::review_queue_view);
        let names: Vec<&str> = q["L"].as_array().unwrap().iter().map(|r| r["exec"].as_str().unwrap()).collect();
        assert_eq!(names, ["E1", "E2"], "in the order you asked");
        crate::leads::install_index(["E1", "E2"].map(|e| (e.to_string(), crate::leads::reported_exec())).into());
        assert_eq!(hub.write(Hub::reviews_view)["E2"]["state"], "queued");
        // Saved: a restart keeps them, in order.
        let again = Hub::new(None);
        assert_eq!(again.read(|st| st.review_queue.iter().map(|q| q.exec.clone()).collect::<Vec<_>>()), ["E1", "E2"]);
        // × takes one back, for good.
        again.unqueue_review("E1");
        assert_eq!(Hub::new(None).read(|st| st.review_queue.len()), 1);
        // An executor no longer waiting for a review (closed, unknown to relay) is dropped, not sent.
        crate::leads::install_index(HashMap::new());
        again.drain_reviews();
        assert!(again.read(|st| st.review_queue.is_empty()));
        assert!(Hub::new(None).read(|st| st.review_queue.is_empty()));
    }
}
