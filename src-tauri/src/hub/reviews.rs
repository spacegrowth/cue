//! Reviews typed into a lead for an executor: sent when the lead is free, queued while it is busy.

use super::*;

/// After typing a review into a lead, wait this long before judging it free again.
const REVIEW_SETTLE_MS: u64 = 8000;

/// "in review" stops showing after this long even if the executor still reads as done.
const REVIEW_SHOWN_MS: u64 = 15 * 60_000;

impl Hub {
    /// Review, tapped: type `text` into the lead now. Only one review at a time per lead (two typed into
    /// a busy lead can run together as one message), so while it's busy this refuses and says with what;
    /// you tap Review again once it's free. Cue never types a review you didn't just ask for.
    /// Review: typed into its lead now if the lead is free, else queued (in order, saved) and sent
    /// the moment it is (see `drain_reviews`). Never refused for being busy.
    pub fn review(&self, lead: &str, exec: &str, text: &str) -> Result<String, String> {
        {
            let mut st = self.st();
            if st.review_sent.contains_key(exec) {
                return Err("its lead is already reviewing it".into());
            }
            if st.review_queue.iter().any(|q| q.exec == exec) {
                return Ok("Already queued: it goes when its lead is free".into());
            }
            // relay woke the lead for it already (or you reviewed it): a second review is the same work twice.
            if crate::leads::is_reviewed(exec) {
                return Ok("Its lead already reviewed this report".into());
            }
            // Behind another queued review, or the lead is busy: queue it.
            let ahead = st.review_queue.iter().any(|q| q.lead == lead);
            if let Some(why) = Self::lead_busy(&st, lead, now_ms()).or_else(|| ahead.then(|| "reviewing".to_string())) {
                let q = QueuedReview { exec: exec.into(), lead: lead.into(), text: text.into(), at_ms: now_ms() };
                crate::db::queue_review(&q.exec, &q.lead, &q.text, q.at_ms);
                st.review_queue.push(q);
                drop(st);
                self.changed();
                return Ok(format!("Queued: its lead is {why}, Cue sends it when it's free"));
            }
            st.review_sent.insert(exec.to_string(), (lead.to_string(), now_ms()));
            st.review_last.insert(lead.to_string(), now_ms());
        }
        self.changed(); // "in review" shows now; typing it in takes a moment
        self.send_review(lead, exec, text)
    }

    fn send_review(&self, lead: &str, exec: &str, text: &str) -> Result<String, String> {
        match self.send_to_session(lead, text, &[], false) {
            Ok(_) => Ok(format!("Sent {text}")),
            Err(e) => {
                self.st().review_sent.remove(exec);
                self.changed();
                Err(e)
            }
        }
    }

    /// Take a queued review back (× on its line in the lead's chat).
    pub fn unqueue_review(&self, exec: &str) {
        self.st().review_queue.retain(|q| q.exec != exec);
        crate::db::unqueue_review(exec);
        self.changed();
    }

    /// Send each lead its next queued review once it's free (its turn ended, nothing asked, not
    /// compacting, no message of yours waiting to go first). One per lead at a time: the next waits
    /// until that review is done. A review whose executor is no longer waiting for one (closed, or
    /// back at work) is dropped. Blocking (typing takes a moment): call off the UI thread.
    pub fn drain_reviews(&self) {
        let due: Vec<QueuedReview> = {
            let mut st = self.st();
            // Moot: no longer waiting for a review, or its lead got to it already (relay wakes it).
            let stale: Vec<String> = st.review_queue.iter().filter(|q| !crate::leads::is_done(&q.exec) || crate::leads::is_reviewed(&q.exec)).map(|q| q.exec.clone()).collect();
            for e in &stale {
                crate::db::unqueue_review(e);
            }
            st.review_queue.retain(|q| !stale.contains(&q.exec));
            let now = now_ms();
            let mut due = vec![];
            let leads: Vec<String> = st.review_queue.iter().map(|q| q.lead.clone()).collect();
            for lead in leads {
                if due.iter().any(|d: &QueuedReview| d.lead == lead) || st.sessions.held(&lead).is_some() || Self::lead_busy(&st, &lead, now).is_some() {
                    continue;
                }
                let Some(i) = st.review_queue.iter().position(|q| q.lead == lead) else { continue };
                let q = st.review_queue.remove(i);
                crate::db::unqueue_review(&q.exec);
                st.review_sent.insert(q.exec.clone(), (lead.clone(), now));
                st.review_last.insert(lead.clone(), now);
                due.push(q);
            }
            if due.is_empty() && stale.is_empty() {
                return;
            }
            due
        };
        self.changed();
        for q in due {
            let _ = self.send_review(&q.lead, &q.exec, &q.text);
        }
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

    /// For every screen: each executor whose review you sent, executor → "in review", or that waits
    /// for its lead, → "review queued"; or whose report its lead never picked up, → "not picked up".
    pub(super) fn reviews_view(st: &mut Store) -> Value {
        let now = now_ms();
        // "in review" until its lead has seen the report and finished its turn (or 15 minutes).
        let working = |s: &Store, lead: &str| s.sessions.state(lead).as_deref() == Some("working");
        let done: Vec<String> = st.review_sent.iter().filter(|(e, (lead, _))| crate::leads::is_reviewed(e) && !working(st, lead)).map(|(e, _)| e.clone()).collect();
        st.review_sent.retain(|exec, (_, at)| now.saturating_sub(*at) < REVIEW_SHOWN_MS && crate::leads::is_done(exec) && !done.contains(exec));
        // Later words win: not picked up, reviewed, then in review, then queued.
        // "not picked up": its report sat unseen though its lead is listening and free (relay's wake-up never came).
        let free = |s: &Store, lead: &str| Self::lead_busy(s, lead, now).is_none();
        let missed = crate::leads::missed(|lead| free(st, lead)).into_iter().map(|e| (e, json!("not picked up")));
        let reviewed = crate::leads::reviewed().into_iter().map(|(e, _)| (e, json!("reviewed")));
        let sent = st.review_sent.keys().map(|e| (e.clone(), json!("in review"))).collect::<Vec<_>>();
        let queued = st.review_queue.iter().map(|q| (q.exec.clone(), json!("review queued")));
        Value::Object(missed.chain(reviewed).chain(sent).chain(queued).collect())
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

    #[test]
    fn a_review_for_a_busy_lead_is_queued_kept_across_restarts_and_dropped_once_moot() {
        let dir = std::env::temp_dir().join(format!("cue-rq-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CUE_HOME", &dir);
        crate::db::reset();
        let hub = Hub::new(None);
        // The lead was just handed a review (it takes a moment to show as working): busy.
        {
            let mut st = hub.st();
            st.review_sent.insert("E0".into(), ("L".into(), now_ms()));
            st.review_last.insert("L".into(), now_ms());
        }
        let r = hub.review("L", "E1", "/relay:review e1").unwrap();
        assert!(r.starts_with("Queued: its lead is reviewing"), "{r}");
        assert!(hub.review("L", "E1", "/relay:review e1").unwrap().starts_with("Already queued"), "twice doesn't queue it twice");
        hub.review("L", "E2", "/relay:review e2").unwrap();
        {
            let mut st = hub.st();
            let q = Hub::review_queue_view(&st);
            let names: Vec<&str> = q["L"].as_array().unwrap().iter().map(|r| r["exec"].as_str().unwrap()).collect();
            assert_eq!(names, ["E1", "E2"], "in the order you asked");
            assert_eq!(Hub::reviews_view(&mut st)["E2"], "review queued");
        }
        // Saved: a restart keeps them, in order.
        let again = Hub::new(None);
        assert_eq!(again.store.lock().unwrap().review_queue.iter().map(|q| q.exec.as_str()).collect::<Vec<_>>(), ["E1", "E2"]);
        // × takes one back, for good.
        again.unqueue_review("E1");
        assert_eq!(Hub::new(None).store.lock().unwrap().review_queue.len(), 1);
        // An executor no longer waiting for a review (closed, unknown to relay) is dropped, not sent.
        again.drain_reviews();
        assert!(again.store.lock().unwrap().review_queue.is_empty());
        assert!(Hub::new(None).store.lock().unwrap().review_queue.is_empty());
    }
}
