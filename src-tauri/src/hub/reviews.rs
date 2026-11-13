//! Teammates' reviews typed into their lead (hub/team.rs): sent when the lead is free, queued while it is busy.

use super::*;

/// After typing a review into a lead, wait this long before judging it free again.
pub(super) const REVIEW_SETTLE_MS: u64 = 8000;

/// A review typed in stops counting as running after this long, even if its lead never replied.
pub(super) const REVIEW_SHOWN_MS: u64 = 15 * 60_000;

/// What Review does with a review (`take_review`).
enum Taken {
    /// Refused, or nothing to do: what to say.
    Answer(Result<String, String>),
    /// Behind its busy lead (why it's busy), to be saved with the queue.
    Queued(String),
    /// Its lead is free: typed in now.
    Send,
}

impl Hub {
    /// Review: typed into its lead now if the lead is free, else queued (in order, saved) and sent
    /// the moment it is (see `drain_reviews`). Never refused for being busy. `exec`: the teammate's key.
    pub fn review(&self, lead: &str, exec: &str, text: &str) -> Result<String, String> {
        match self.write(|st| Self::take_review(st, lead, exec, text, now_ms())) {
            Taken::Answer(r) => r,
            Taken::Queued(why) => {
                self.save_queued_review(exec);
                self.changed();
                Ok(format!("Queued: its lead is {why}, Cue sends it when it's free"))
            }
            Taken::Send => {
                self.changed();
                self.send_review(lead, exec, text)
            }
        }
    }

    /// What Review does with `exec`'s review, decided and noted in the store: refused or already done, queued
    /// behind its busy lead, or sent now.
    fn take_review(st: &mut Store, lead: &str, exec: &str, text: &str, now: u64) -> Taken {
        // A crew member's review sent before is over once it isn't under review any more (its review came, it went
        // back to work and finished again): only one under review now refuses another.
        let mate = super::team::teammate_key(exec);
        let reviewing = mate.is_none_or(|(l, a)| st.sessions.team(l).iter().any(|t| t.agent_id == a && t.state == "reviewing"));
        if st.review_sent.contains_key(exec) && reviewing {
            return Taken::Answer(Err("its lead is already reviewing it".into()));
        }
        st.review_sent.remove(exec);
        if st.review_queue.iter().any(|q| q.exec == exec) {
            return Taken::Answer(Ok("Already queued: it goes when its lead is free".into()));
        }
        // Behind another queued review, or the lead is busy: queue it.
        let ahead = st.review_queue.iter().any(|q| q.lead == lead);
        if let Some(why) = Self::lead_busy(st, lead, now).or_else(|| ahead.then(|| "reviewing".to_string())) {
            let q = QueuedReview { exec: exec.into(), lead: lead.into(), text: text.into(), at_ms: now };
            st.review_queue.push(q);
            // A crew member's: marked queued in the same step, so it's never queued with no review in the queue.
            if let Some((lead, agent)) = super::team::teammate_key(exec) {
                st.sessions.with_teammate(lead, agent, |m| m.state = "queued".into());
            }
            return Taken::Queued(why);
        }
        st.review_sent.insert(exec.to_string(), (lead.to_string(), now));
        st.review_last.insert(lead.to_string(), now);
        Taken::Send
    }

    fn send_review(&self, lead: &str, exec: &str, text: &str) -> Result<String, String> {
        let sent = self.send_to_session(lead, text, &[], false);
        // Under review from here, or back to done.
        self.team_review_sent(exec, sent.is_ok());
        match sent {
            Ok(_) => Ok("Sent its review to its lead".into()),
            Err(e) => {
                self.write(|st| st.review_sent.remove(exec));
                self.changed();
                Err(e)
            }
        }
    }

    /// Take a queued review back (Skip on Cue's bubble in the lead's chat): its teammate is done again, with Review
    /// in its ⋯ for when you want it.
    pub fn unqueue_review(&self, exec: &str) {
        self.write(|st| st.review_queue.retain(|q| q.exec != exec));
        self.save_queued_review(exec);
        self.team_review_sent(exec, false);
        self.changed();
    }

    /// Send each lead its next queued review once it's free (its turn ended, nothing asked, not
    /// compacting, no message of yours waiting to go first). One per lead at a time: the next waits
    /// until that review is done. A review whose teammate is no longer waiting for one (back at work,
    /// set aside) is dropped. Blocking (typing takes a moment): call off the UI thread.
    /// A free lead's crew commit asks that wait go before its next review (team_tidy.rs `drain_commit_asks`), and
    /// that review waits for the lead to settle after them, as after a review.
    pub fn drain_reviews(&self) {
        self.drain_commit_asks();
        let (stale, due) = self.write(|st| Self::take_due_reviews(st, now_ms()));
        if due.is_empty() && stale.is_empty() {
            return;
        }
        for exec in stale.iter().chain(due.iter().map(|q| &q.exec)) {
            self.save_queued_review(exec);
        }
        self.changed();
        for q in due {
            let _ = self.send_review(&q.lead, &q.exec, &q.text);
        }
    }

    /// The queue's moot reviews dropped (their keys), and each free lead's next review taken off it to
    /// send now, noted as sent.
    fn take_due_reviews(st: &mut Store, now: u64) -> (Vec<String>, Vec<QueuedReview>) {
        // Moot once its teammate is no longer waiting in the queue (back at work, set aside); anything that
        // isn't a teammate's (a review saved by an older Cue) is moot as well.
        let moot = |q: &QueuedReview| match super::team::teammate_key(&q.exec) {
            Some((lead, agent)) => !st.sessions.team(lead).iter().any(|t| t.agent_id == agent && t.state == "queued"),
            None => true,
        };
        let stale: Vec<String> = st.review_queue.iter().filter(|q| moot(q)).map(|q| q.exec.clone()).collect();
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

    /// A queued or sent review's teammate, by name (its key when the team no longer has it).
    fn reviewed_name(st: &Store, exec: &str) -> String {
        super::team::teammate_key(exec).and_then(|(lead, agent)| st.sessions.teammate_name(lead, agent)).unwrap_or_else(|| exec.to_string())
    }

    /// Why a lead can't take a review right now, or None when it's free: working (a review or anything
    /// else), compacting, asking you something, or a review just typed in (it takes a moment to show as working).
    pub(super) fn lead_busy(st: &Store, lead: &str, now: u64) -> Option<String> {
        let reviewing = || st.review_sent.iter().filter(|(_, (l, _))| l == lead).max_by_key(|(_, (_, at))| *at).map(|(e, _)| Self::reviewed_name(st, e));
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

    /// Reviews typed in that are over: past REVIEW_SETTLE_MS, a review stays sent while its teammate is under
    /// review (until its lead replies), for REVIEW_SHOWN_MS at most.
    pub(super) fn prune_reviews(st: &mut Store, now: u64) {
        let reviewing = |s: &Store, exec: &str| super::team::teammate_key(exec).is_some_and(|(lead, agent)| s.sessions.team(lead).iter().any(|t| t.agent_id == agent && t.state == "reviewing"));
        let over: Vec<String> = st
            .review_sent
            .iter()
            .filter(|(exec, (_, at))| {
                let age = now.saturating_sub(*at);
                age >= REVIEW_SHOWN_MS || (age >= REVIEW_SETTLE_MS && !reviewing(st, exec))
            })
            .map(|(e, _)| e.clone())
            .collect();
        for e in over {
            st.review_sent.remove(&e);
        }
    }

    /// When the review rules above flip with time alone: a lead is no longer settling (REVIEW_SETTLE_MS after
    /// its last review), and a review stops counting as running (REVIEW_SHOWN_MS after it was sent).
    pub(super) fn review_changes(st: &Store, now: u64, next: &mut super::shown::NextChange) {
        for at in st.review_last.values() {
            next.at(at + REVIEW_SETTLE_MS, now);
        }
        for (_, at) in st.review_sent.values() {
            next.at(at + REVIEW_SHOWN_MS, now);
        }
    }

    /// For every screen: each lead's queued reviews, in order (shown in its chat, each with ×).
    pub(super) fn review_queue_view(st: &Store) -> Value {
        let mut out = serde_json::Map::new();
        for q in &st.review_queue {
            let row = json!({ "exec": q.exec, "name": Self::reviewed_name(st, &q.exec), "at_ms": q.at_ms });
            out.entry(q.lead.clone()).or_insert_with(|| json!([])).as_array_mut().unwrap().push(row);
        }
        Value::Object(out)
    }
}

#[cfg(test)]
impl Hub {
    /// Tests: the teammates whose reviews are queued, in order, as the store has them.
    pub(crate) fn queued_reviews(&self) -> Vec<String> {
        self.read(|st| st.review_queue.iter().map(|q| q.exec.clone()).collect())
    }

    /// Tests: teammate `agent` of `lead`, waiting in Review's queue as its Review leaves it; its review key.
    pub(crate) fn queued_teammate(&self, lead: &str, agent: &str) -> String {
        let t = crate::sessions::team::Teammate { agent_id: agent.into(), name: format!("{agent}-name"), state: "queued".into(), ..Default::default() };
        self.write(|st| st.sessions.add_teammate(lead, t));
        format!("team:{lead}:{agent}")
    }

    /// Tests: review `exec` gone from the queue, its member left as it was (a queue an older Cue lost).
    pub(crate) fn unqueue_review_only(&self, exec: &str) {
        self.write(|st| st.review_queue.retain(|q| q.exec != exec));
    }

    /// Tests: teammate `agent` of `lead` set to `state`.
    pub(crate) fn set_teammate_state(&self, lead: &str, agent: &str, state: &str) {
        self.write(|st| st.sessions.with_teammate(lead, agent, |t| t.state = state.into()));
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

    #[test]
    fn a_review_for_a_busy_lead_is_queued_kept_across_restarts_and_dropped_once_moot() {
        let dir = std::env::temp_dir().join(format!("cue-rq-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        // The lead was just handed a review (it takes a moment to show as working): busy.
        hub.write(|st| {
            st.review_sent.insert("team:L:a0".into(), ("L".into(), now_ms()));
            st.review_last.insert("L".into(), now_ms());
        });
        let r = hub.review("L", "team:L:a1", "review a1").unwrap();
        assert!(r.starts_with("Queued: its lead is reviewing"), "{r}");
        assert!(hub.review("L", "team:L:a1", "review a1").unwrap().starts_with("Already queued"), "twice doesn't queue it twice");
        hub.review("L", "team:L:a2", "review a2").unwrap();
        let q = hub.read(Hub::review_queue_view);
        let names: Vec<&str> = q["L"].as_array().unwrap().iter().map(|r| r["exec"].as_str().unwrap()).collect();
        assert_eq!(names, ["team:L:a1", "team:L:a2"], "in the order you asked");
        // Saved: a restart keeps them, in order.
        let again = Hub::new(None);
        assert_eq!(again.read(|st| st.review_queue.iter().map(|q| q.exec.clone()).collect::<Vec<_>>()), ["team:L:a1", "team:L:a2"]);
        // × takes one back, for good.
        again.unqueue_review("team:L:a1");
        assert_eq!(Hub::new(None).read(|st| st.review_queue.len()), 1);
        // A teammate the team no longer has waiting is dropped, not sent.
        again.drain_reviews();
        assert!(again.read(|st| st.review_queue.is_empty()));
        assert!(Hub::new(None).read(|st| st.review_queue.is_empty()));
        crate::db::reset();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A review typed in counts as running while it settles, then while its teammate is under review.
    #[test]
    fn a_sent_review_is_pruned_once_it_settled_and_nothing_is_under_review() {
        let mut st = Store::default();
        let t = 1_000_000;
        st.review_sent.insert("team:L:a1".into(), ("L".into(), t));
        Hub::prune_reviews(&mut st, t + 1000);
        assert!(st.review_sent.contains_key("team:L:a1"), "still settling");
        Hub::prune_reviews(&mut st, t + REVIEW_SETTLE_MS);
        assert!(st.review_sent.is_empty(), "settled, and no teammate under review");
    }
}
