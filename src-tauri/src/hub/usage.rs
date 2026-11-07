//! Usage: API errors and the plan's limits (hit, lifted, back), what each session costs, and the tick that
//! catches turns that died without a hook reaching Cue.

use super::*;

impl Hub {
    /// A turn ended on an API error. A usage limit leaves the session "limited" (no card: there's
    /// nothing to answer until it resets); any other error is a finished turn that needs you.
    pub fn failed(&self, origin: Origin, error_type: &str, details: &str) {
        let sid = origin.session_id.clone();
        if sid.is_empty() {
            return;
        }
        let now = now_ms();
        let Some(mut limit) = crate::usage::parse(error_type, details, now) else {
            let what = if details.trim().is_empty() { error_type.replace('_', " ") } else { details.trim().to_string() };
            let driver = crate::leads::resolve_driver(&sid, "");
            self.event(origin, "stopped", format!("Stopped on an error: {what}"), vec![], &driver);
            return;
        };
        let announce = self.write(|st| {
            // Already marked by the hook or the transcript check: once is enough.
            if st.sessions.all().iter().any(|s| s.origin.session_id == sid && s.state == "limited") {
                return None;
            }
            if limit.resets_ms.is_none() {
                limit.resets_ms = st.rates.as_ref().and_then(|r| r.models.iter().find(|m| m.name == limit.scope)).and_then(|m| m.resets_ms);
            }
            st.items.retain(|i| !(i.kind == "waiting" && i.origin.session_id == sid));
            st.sessions.mark(&origin, "limited", None);
            st.sessions.set_queued(&sid, None);
            let key = format!("{}|{}", limit.scope, limit.resets_ms.unwrap_or(0) / 60_000);
            st.sessions.set_limit(&sid, Some(limit.clone()));
            Some(st.told_out.insert(key))
        });
        let Some(announce) = announce else { return };
        self.changed();
        if announce {
            self.tell_limit_hit(&limit, now);
        }
    }

    /// A usage limit was hit: once per scope and reset.
    fn tell_limit_hit(&self, limit: &crate::usage::Limit, now: u64) {
        let when = limit.resets_ms.map(|r| format!(" until {}", crate::usage::when(r, now))).unwrap_or_default();
        let (title, body) = if limit.scope == "all" {
            (format!("Claude Code is out of usage{when}"), "No Claude Code session can run until then.".to_string())
        } else {
            (format!("{} is used up{when}", limit.scope), "Other models still work: switch with /model.".to_string())
        };
        self.notify_about("usage", &title, &body);
    }

    /// A Claude Code turn finished normally, so the plan's limit is behind us: every session that
    /// was waiting on it can run again. (A model's own limit only lifts for the session that hit it.)
    pub(super) fn limits_lifted(&self, sid: &str) {
        let now = now_ms();
        let lifted = self.write(|st| {
            let mut lifted = false;
            for s in st.sessions.all() {
                let Some(mut l) = s.limit.clone() else { continue };
                if s.state != "limited" || l.scope != "all" || l.resets_ms.is_some_and(|r| r <= now) || s.origin.session_id == sid {
                    continue;
                }
                l.resets_ms = Some(now);
                st.sessions.set_limit(&s.origin.session_id, Some(l));
                lifted = true;
            }
            lifted
        });
        if lifted {
            self.changed();
        }
    }

    /// The status line's usage. Only redraws when the numbers move.
    pub fn set_cost(&self, session_id: &str, cost: f64) {
        self.write(|st| st.costs.insert(session_id.to_string(), cost));
    }

    /// A Claude Code session's cost so far, for API-key and proxy users only. On a plan (Claude Code sends
    /// its limits only then) it's what the session would cost at API prices, not what you pay.
    pub fn session_cost(&self, session_id: &str) -> Option<f64> {
        self.read(|st| if st.rates.is_some() { None } else { st.costs.get(session_id).copied() })
    }

    pub fn set_rates(&self, rl: &Value) {
        let now = now_ms();
        let unchanged = self.read(|st| {
            let Some(old) = &st.rates else { return false };
            let same = |a: &Option<crate::usage::Window>, k: &str| {
                a.as_ref().map(|w| w.pct) == rl.pointer(&format!("/{k}/used_percentage")).and_then(Value::as_f64)
            };
            // No numbers in this one at all: keep what we have, nothing to redraw.
            let none = rl.pointer("/five_hour/used_percentage").is_none() && rl.pointer("/seven_day/used_percentage").is_none();
            // Re-reading ~/.claude.json for the per-model rows is the slow part: at most once a minute.
            none || (same(&old.five_hour, "five_hour") && same(&old.seven_day, "seven_day") && now.saturating_sub(old.at_ms) < 60_000)
        });
        if unchanged {
            return;
        }
        let mut rates = crate::usage::rates_from_statusline(rl, now);
        let changed = self.write(|st| {
            // A status line without a window (another agent's, or before Claude Code has numbers) keeps the
            // last one instead of wiping it: the meter shouldn't vanish between updates.
            if let Some(old) = &st.rates {
                let fresh = rates.five_hour.is_some() || rates.seven_day.is_some();
                if rates.five_hour.is_none() { rates.five_hour = old.five_hour.clone(); }
                if rates.seven_day.is_none() { rates.seven_day = old.seven_day.clone(); }
                if !fresh { rates.at_ms = old.at_ms; }   // "as of" stays when the numbers it carries are old
            }
            let changed = st.rates.as_ref().map(|r| (&r.five_hour, &r.seven_day, &r.models)) != Some((&rates.five_hour, &rates.seven_day, &rates.models));
            st.rates = Some(rates);
            changed
        });
        if changed {
            self.changed();
        }
    }


    /// Every ~10s: catch turns that died without a hook reaching Cue, and announce limits that lifted.
    pub fn usage_tick(&self) {
        // A lead that came free without a Stop reaching Cue still gets its queued reviews.
        self.drain_reviews();
        let now = now_ms();
        let (stuck, limited) = self.read(|st| {
            let all = st.sessions.all();
            let stuck: Vec<Origin> = all
                .iter()
                .filter(|s| s.state == "working" && s.origin.harness == "claude" && !s.origin.transcript_path.is_empty() && now.saturating_sub(s.since_ms) > 20_000)
                .map(|s| s.origin.clone())
                .collect();
            let limited: Vec<crate::usage::Limit> = all.iter().filter(|s| s.state == "limited").filter_map(|s| s.limit.clone()).collect();
            (stuck, limited)
        });
        self.note_retries();
        self.catch_dead_turns(stuck, now);
        self.cache_tick();
        self.tell_limits_back(&limited, now);
    }

    /// Can't reach the API: Claude Code tries again for minutes before it gives up, its terminal only
    /// spinning, so the chat says what failed and when the next try is.
    fn note_retries(&self) {
        let retrying: Vec<Origin> = self.read(|st| st.sessions.all().iter().filter(|s| s.origin.harness == "claude" && !s.origin.transcript_path.is_empty() && (s.state == "working" || s.retry.is_some())).map(|s| s.origin.clone()).collect());
        let mut retry_changed = false;
        for o in retrying {
            let r = crate::usage::retrying(&o.transcript_path);
            retry_changed |= self.write(|st| st.sessions.set_retry(&o.session_id, r));
        }
        if retry_changed {
            self.changed();
        }
    }

    /// Sessions working a while (`stuck`) whose turn died on an API error, or ended without its Stop reaching Cue.
    fn catch_dead_turns(&self, stuck: Vec<Origin>, now: u64) {
        for o in stuck {
            if let Some((kind, text)) = crate::usage::last_api_error(&o.transcript_path) {
                self.failed(o, &kind, &text);
            } else if let Some((at, reply, parts)) = transcript::turn_ended_parts(&o.transcript_path).filter(|(at, _, _)| now.saturating_sub(*at) > 5_000) {
                // Its turn ended but the Stop never reached Cue (Cue was restarting): finish it now.
                // Only a turn that ended after it started working, and seconds ago (the hook is faster).
                let since = self.read(|st| st.sessions.all().into_iter().find(|s| s.origin.session_id == o.session_id).map(|s| s.since_ms)).unwrap_or(u64::MAX);
                if at >= since {
                    let driver = crate::leads::resolve_driver(&o.session_id, "");
                    self.event(o, "stopped", reply, parts, &driver);
                }
            }
        }
    }

    /// A limit whose reset time passed: say so once, per scope.
    fn tell_limits_back(&self, limited: &[crate::usage::Limit], now: u64) {
        let back: Vec<(String, usize)> = self.write(|st| {
            let mut back = vec![];
            for l in limited {
                let Some(r) = l.resets_ms.filter(|r| *r <= now && now - *r < 3600 * 1000) else { continue };
                let key = format!("{}|{}", l.scope, r / 60_000);
                if st.told_back.insert(key) {
                    let n = limited.iter().filter(|x| x.scope == l.scope).count();
                    back.push((l.scope.clone(), n));
                }
            }
            back
        });
        if !back.is_empty() {
            self.changed();
        }
        for (scope, n) in back {
            let who = if scope == "all" { "Claude Code".to_string() } else { scope };
            let paused = if n == 1 { "1 session didn't run: resend it from Cue.".to_string() } else { format!("{n} sessions didn't run: resend them from Cue.") };
            self.notify_about("usage", &format!("{who} is back"), &paused);
        }
    }
}
