//! Away mode on a Desk: while the owner is away, its lead decides some things alone (the rules switched on) and
//! writes each as a DECIDED line the owner can undo; everything else waits as an ASK. Kept per Desk in config.json
//! (`desk.away`: { <desk id>: { on, since, until, budget, base, ended, rules } }). It ends by itself at its end time
//! or once the Desk's sessions have spent its budget (checked as a Desk session's turn ends); the lead is told each
//! time it changes. Ask me before (guard.rs) is untouched: those steps wait for the owner either way.

use super::*;

/// The rules: key, on unless the owner switched it, and what the lead may decide alone when it's on.
pub(crate) const RULES: &[(&str, bool, &str)] = &[
    ("route", true, "Route, split and merge items; wake or start sessions (up to 3 new)"),
    ("how", true, "'How' questions inside an item, when easy to change later"),
    ("stopped", true, "Send back a stopped item with what to do next"),
    ("order", true, "Which item goes first when two change the same files"),
    ("what", false, "'What' questions: scope, taste, what you want"),
    ("outside", false, "Pulling in a session that isn't on the Desk"),
];

/// What always waits for the owner, whatever the rules say.
const LOCKED: &[&str] = &["Ask me before: push to main, release, deleting files", "Picking a mock", "Looks right / Send back on a finished item"];

/// Desk `desk`'s saved Away settings (Null when never set).
fn saved(desk: &str) -> Value {
    crate::config::load().pointer("/desk/away").and_then(|m| m.get(desk)).cloned().unwrap_or(Value::Null)
}

/// The rules in `v` (a saved Away, or what the window sent), each key missing there as `before` has it.
fn rules_in(v: &Value, before: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for (k, default, _) in RULES {
        let was = before.get(*k).and_then(Value::as_bool).unwrap_or(*default);
        out.insert((*k).into(), json!(v.get(*k).and_then(Value::as_bool).unwrap_or(was)));
    }
    Value::Object(out)
}

/// Desk `desk`'s sessions: its lead and the sessions on it.
fn desk_sids(st: &Store, desk: &str) -> Vec<String> {
    let mut out: Vec<String> = st.desk.lead.get(desk).map(|l| l.sid.clone()).into_iter().collect();
    for s in super::desk::routable(st, desk) {
        let sid = &s.origin.session_id;
        if st.desk.on_desk(desk, sid) && !out.contains(sid) {
            out.push(sid.clone());
        }
    }
    out
}

/// What Desk `desk`'s sessions have cost so far, summed.
// SHORTCUT: spend is "this sum now minus the sum when Away turned on". A session that leaves the Desk (or ends and
// drops out of the store) lowers the sum, so `spent` can read low. Ceiling: fine while a Desk's sessions stay put
// overnight; past that, keep a per-session baseline (sid → cost at `since`) instead of one number.
fn cost_now(st: &Store, desk: &str) -> f64 {
    desk_sids(st, desk).iter().filter_map(|sid| st.costs.get(sid)).sum()
}

/// Away as Cue reads it at `now`: saved settings, spend, and whether (and why not) it's in force.
struct Away {
    on: bool,
    since: u64,
    until: Option<u64>,
    budget: Option<f64>,
    spent: f64,
    /// Why it's on but not in force: "until" | "budget" (saved once noticed, or seen live), else "".
    ended: String,
    rules: Value,
}

impl Away {
    fn read(st: &Store, desk: &str, now: u64) -> Away {
        let v = saved(desk);
        let on = v.get("on").and_then(Value::as_bool).unwrap_or(false);
        let until = v.get("until").and_then(Value::as_u64);
        let budget = v.get("budget").and_then(Value::as_f64);
        let base = v.get("base").and_then(Value::as_f64).unwrap_or(0.0);
        let spent = if on { (cost_now(st, desk) - base).max(0.0) } else { 0.0 };
        let mut ended = v.get("ended").and_then(Value::as_str).unwrap_or("").to_string();
        if !on {
            ended.clear();
        } else if ended.is_empty() {
            ended = ended_now(until, budget, spent, now).to_string();
        }
        Away { on, since: v.get("since").and_then(Value::as_u64).unwrap_or(0), until, budget, spent, ended, rules: rules_in(&v["rules"], &Value::Null) }
    }

    fn active(&self) -> bool {
        self.on && self.ended.is_empty()
    }

    fn view(&self) -> Value {
        json!({ "on": self.on, "active": self.active(), "ended": self.ended, "since": self.since, "until": self.until, "budget": self.budget, "spent": (self.spent * 100.0).round() / 100.0, "rules": self.rules })
    }
}

/// Why an Away with these limits is over at `now`: "until", "budget", or "" while it holds.
fn ended_now(until: Option<u64>, budget: Option<f64>, spent: f64, now: u64) -> &'static str {
    if until.is_some_and(|u| now >= u) {
        "until"
    } else if budget.is_some_and(|b| spent >= b) {
        "budget"
    } else {
        ""
    }
}

/// Leads (session ids) asked to sort their Desk out before Away goes on, until the turn that reads it ends: each with
/// the turns still to end before that one (1 for a lead mid-turn as it was asked: the ask waits for that turn's end).
/// By lead, not by Desk: a lead's id is its own, so a new lead never inherits it.
fn sweeping() -> &'static std::sync::Mutex<std::collections::HashMap<String, u8>> {
    static SWEEPING: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, u8>>> = std::sync::OnceLock::new();
    SWEEPING.get_or_init(Default::default)
}

fn is_sweeping(sid: &str) -> bool {
    let held = crate::locking::lock(sweeping(), "leads sweeping for Away");
    held.contains_key(sid)
}

/// `state.desk.projects[i].away`.
pub(super) fn away_view(st: &Store, desk: &str, now: u64) -> Value {
    let mut v = Away::read(st, desk, now).view();
    let lead = st.desk.lead.get(desk).map(|l| l.sid.clone()).unwrap_or_default();
    v["sweeping"] = json!(!lead.is_empty() && is_sweeping(&lead));
    v
}

/// What Desk `desk`'s lead is asked as the owner presses Away (before it's on): settle what it can under `rules`.
pub(crate) fn sweep_words(rules: &Value) -> String {
    let rules = rules_in(rules, &Value::Null);
    let may: Vec<&str> = RULES.iter().filter(|(k, _, _)| rules.get(*k).and_then(Value::as_bool).unwrap_or(false)).map(|(_, _, w)| *w).collect();
    let list = if may.is_empty() { "nothing: every call waits for the owner".to_string() } else { may.iter().map(|w| format!("\n- {w}")).collect::<String>() };
    format!("The owner is about to go away. Go through every open item (working, waiting, to check) and settle everything you can under these rules, writing a DECIDED line for each call (DECIDED: <what you chose> — <why>). You may decide alone:{list}\nAnswer or close any ASK you can settle yourself. Leave at most 5 questions that truly need the owner, as ASK lines, most important first. Then stop.")
}

/// While Away is on: when it started (ms), so the Desk keeps showing the decisions made since.
pub(super) fn away_since(desk: &str) -> Option<u64> {
    let v = saved(desk);
    v.get("on").and_then(Value::as_bool).unwrap_or(false).then(|| v.get("since").and_then(Value::as_u64).unwrap_or(0))
}

/// The "Away mode" part of Desk `desk`'s summary for its lead, or None while Away is off.
pub(super) fn away_block(st: &Store, desk: &str, now: u64) -> Option<String> {
    let a = Away::read(st, desk, now);
    a.on.then(|| block(&a, now))
}

fn block(a: &Away, now: u64) -> String {
    let mut out = String::from("Away mode:\n");
    if !a.active() {
        let why = if a.ended == "budget" { "its budget is spent" } else { "its end time has passed" };
        out.push_str(&format!("Away is paused: {why}. Decide nothing more alone: everything goes as an ASK: line and waits for the owner.\n"));
        return out;
    }
    let until = a.until.map(|u| format!("until {}", crate::usage::when(u, now))).unwrap_or_else(|| "until they're back".into());
    let budget = a.budget.map(|b| format!("with a ${b:.2} budget (${:.2} spent so far)", a.spent)).unwrap_or_else(|| "with no budget set".into());
    out.push_str(&format!("The owner is away {until}, {budget}.\n"));
    let on = |k: &str| a.rules.get(k).and_then(Value::as_bool).unwrap_or(false);
    let (may, waits): (Vec<_>, Vec<_>) = RULES.iter().partition(|(k, _, _)| on(k));
    if !may.is_empty() {
        out.push_str("You may decide these alone. Write each one as a line `DECIDED: <what you chose> — <why>` so the owner can undo it:\n");
        for (_, _, words) in &may {
            out.push_str(&format!("- {words}\n"));
        }
    }
    out.push_str("Everything else waits for the owner: write it as an ASK: line. Always ASK about:\n");
    for (_, _, words) in &waits {
        out.push_str(&format!("- {words}\n"));
    }
    for words in LOCKED {
        out.push_str(&format!("- {words}\n"));
    }
    out.push_str("Don't wait on an ASK: carry on with the other items.\n");
    if on("route") {
        out.push_str("Start at most 3 new sessions while the owner is away.\n");
    }
    out.push_str("A session stopped by a usage limit is carried on by Cue after the reset: don't ASK about it.\n");
    out
}

impl Hub {
    /// The window's Away settings for Desk `desk` (`away`: { on, until, budget, rules }): saved, its lead told when it
    /// changes. Turning it on starts it now, from what the Desk's sessions have spent so far. Returns the view.
    pub(crate) fn desk_away_set(&self, desk: &str, away: &Value) -> Result<Value, String> {
        if desk.trim().is_empty() {
            return Err("no Desk".into());
        }
        let on = away.get("on").and_then(Value::as_bool).ok_or("on must be true or false")?;
        let until = away.get("until").and_then(Value::as_u64);
        let budget = match away.get("budget") {
            None | Some(Value::Null) => None,
            // $0 is no budget: one that ended Away before it began would be no use.
            Some(b) => Some(b.as_f64().filter(|b| *b >= 0.0).ok_or("the budget must be a number of dollars")?).filter(|b| *b > 0.0),
        };
        let now = now_ms();
        let cost = self.read(|st| cost_now(st, desk));
        let (was_on, rules_changed) = crate::config::update_with("desk.away", |c| {
            let mut map = c.pointer("/desk/away").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
            let before = map.get(desk).cloned().unwrap_or(Value::Null);
            let was_on = before.get("on").and_then(Value::as_bool).unwrap_or(false);
            let old_rules = rules_in(&before["rules"], &Value::Null);
            let rules = rules_in(away.get("rules").unwrap_or(&Value::Null), &old_rules);
            // Set again with the same limits (a rule changed): an Away that already ran out stays so, its lead told once.
            let same = on && was_on && before.get("until").and_then(Value::as_u64) == until && before.get("budget").and_then(Value::as_f64) == budget;
            let ended = if same { before.get("ended").cloned().unwrap_or(json!("")) } else { json!("") };
            let (since, base) = if on && was_on {
                (before.get("since").cloned().unwrap_or(json!(now)), before.get("base").cloned().unwrap_or(json!(cost)))
            } else if on {
                (json!(now), json!(cost))
            } else {
                (before.get("since").cloned().unwrap_or(json!(0)), before.get("base").cloned().unwrap_or(json!(0.0)))
            };
            // Set again with new limits (a later end time, a bigger budget): it holds again until a turn's end finds otherwise.
            map[desk] = json!({ "on": on, "since": since, "until": until, "budget": budget, "base": base, "ended": ended, "rules": rules });
            Ok((Some(map), (was_on, rules != old_rules)))
        })?;
        if on != was_on || (on && rules_changed) {
            let words = self.read(|st| {
                let a = Away::read(st, desk, now);
                if a.on { block(&a, now) } else { "Away mode is off: the owner is back. Ask and decide as usual.".to_string() }
            });
            self.tell_lead_away(desk, &words);
        }
        self.changed();
        Ok(self.read(|st| away_view(st, desk, now)))
    }

    /// Session `sid`'s turn is over: each Desk it's on whose Away has run out (its end time, or its budget) is ended,
    /// and its lead told, once.
    pub(super) fn desk_away_check(&self, sid: &str) {
        if self.desk_away_turn_over(sid, false) {
            self.changed();
        }
        let all = crate::config::load().pointer("/desk/away").and_then(Value::as_object).cloned().unwrap_or_default();
        let now = now_ms();
        for (desk, v) in all {
            if !v.get("on").and_then(Value::as_bool).unwrap_or(false) || !v.get("ended").and_then(Value::as_str).unwrap_or("").is_empty() {
                continue;
            }
            let found = self.read(|st| desk_sids(st, &desk).iter().any(|s| s == sid).then(|| Away::read(st, &desk, now)));
            let Some(a) = found.filter(|a| !a.ended.is_empty()) else { continue };
            let saved = crate::config::update_with("desk.away", |c| {
                let mut map = c.pointer("/desk/away").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
                let Some(d) = map.get_mut(&desk).filter(|d| d.get("on").and_then(Value::as_bool).unwrap_or(false)) else { return Ok((None, false)) };
                if !d.get("ended").and_then(Value::as_str).unwrap_or("").is_empty() {
                    return Ok((None, false));
                }
                d["ended"] = json!(a.ended);
                Ok((Some(map), true))
            });
            match saved {
                Ok(true) => {
                    self.tell_lead_away(&desk, &block(&a, now));
                    self.changed();
                }
                Ok(false) => {}
                Err(e) => eprintln!("cue: couldn't end Away on {desk}: {e}"),
            }
        }
    }

    /// Desk `desk`'s lead was asked to sort things out before Away: the window says so until its next turn ends.
    pub(crate) fn desk_away_sweeping(&self, desk: &str) {
        let Some((lead, busy)) = self.read(|st| st.desk.lead.get(desk).map(|l| (l.sid.clone(), st.sessions.get(&l.sid).is_some_and(|s| !matches!(s.state.as_str(), "" | "idle" | "waiting" | "stopped" | "ended"))))) else { return };
        {
            let mut held = crate::locking::lock(sweeping(), "leads sweeping for Away");
            held.insert(lead, u8::from(busy));
        }
        self.changed();
    }

    /// The sweep didn't reach Desk `desk`'s lead: nothing to wait for.
    pub(crate) fn desk_away_sweep_failed(&self, desk: &str) {
        let Some(lead) = self.read(|st| st.desk.lead.get(desk).map(|l| l.sid.clone())) else { return };
        {
            let mut held = crate::locking::lock(sweeping(), "leads sweeping for Away");
            held.remove(&lead);
        }
        self.changed();
    }

    /// Session `sid`'s turn is over (`failed`: it ended in an error, so the sweep won't be read): whether a sweep it
    /// was on is now over. A turn it was already on as it was asked doesn't count.
    pub(crate) fn desk_away_turn_over(&self, sid: &str, failed: bool) -> bool {
        let mut held = crate::locking::lock(sweeping(), "leads sweeping for Away");
        match held.get_mut(sid) {
            Some(n) if *n > 0 && !failed => {
                *n -= 1;
                false
            }
            Some(_) => {
                held.remove(sid);
                true
            }
            None => false,
        }
    }

    /// `words` to Desk `desk`'s lead, if it's live. Sent on the hub's own thread (`later`), since a
    /// turn's end can arrive while a send holds the typing gate.
    fn tell_lead_away(&self, desk: &str, words: &str) {
        let lead = self.read(|st| st.desk.lead.get(desk).map(|l| l.sid.clone()).filter(|sid| st.sessions.get(sid).is_some()));
        if let Some(sid) = lead {
            let words = words.to_string();
            self.later(0, move |h| {
                if let Err(e) = h.send_to_session(&sid, &words, &[], false) {
                    eprintln!("cue: couldn't tell {sid} about Away mode: {e}");
                }
            });
        }
    }
}
