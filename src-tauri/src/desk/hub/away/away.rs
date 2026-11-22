//! Away mode on a Desk: while the owner is away, its lead decides some things alone (the rules switched on) and
//! writes each as a DECIDED line the owner can undo. It asks the owner nothing (#471): an ASK it writes, or one still
//! open as Away comes into force, goes back to it to decide itself, and the call it writes next is shown with it. Kept per Desk in config.json
//! (`desk.away`: { <desk id>: { on, since, until, budget, base, ended, rules } }). It ends by itself at its end time
//! or once the Desk's sessions have spent its budget (checked as a Desk session's turn ends); the lead is told each
//! time it changes. Ask me before (guard.rs) still waits for the owner: while Away is in force, as a held step
//! (desk/hub/away/away_holds.rs) the session carries on past.

use crate::desk::hub::prelude::*;

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
    for s in crate::desk::hub::core::desk::routable(st, desk) {
        let sid = &s.origin.session_id;
        if st.desk.on_desk(desk, sid) && !out.contains(sid) {
            out.push(sid.clone());
        }
    }
    out
}

/// Whether session `sid` is Desk `desk`'s lead or one of its sessions.
pub(crate) fn on_desk(st: &Store, desk: &str, sid: &str) -> bool {
    desk_sids(st, desk).iter().any(|s| s == sid)
}

/// Whether Desk `desk`'s Away is on and in force at `now` (not past its end time or budget).
pub(crate) fn away_in_force(st: &Store, desk: &str, now: u64) -> bool {
    saved(desk).get("on").and_then(Value::as_bool).unwrap_or(false) && Away::read(st, desk, now).active()
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

/// What the lead's pass before Away goes by (#612): each of Desk `desk`'s items with its state, and the open asks from
/// its sessions.
fn pass_marks(st: &Store, desk: &str) -> Value {
    let sids = desk_sids(st, desk);
    let mut asks: Vec<&str> = st.items.iter().filter(|it| it.status == "pending" && it.kind != "waiting" && sids.contains(&it.origin.session_id)).map(|it| it.id.as_str()).collect();
    asks.sort_unstable();
    json!({ "items": crate::desk::hub::away::away_log::item_states(st, desk), "asks": asks })
}

/// Item states that need their lead before Away (#612): not routed yet, waiting on a question, stopped. Progress (working,
/// queued, to check, done) and parked items don't: going through them again would redo the same pass.
const SETTLE: [(&str, &str); 3] = [("new", "new, not routed yet"), ("needs_you", "waits on a question"), ("stopped", "stopped")];

/// What changed on Desk `desk` since the lead's last pass (`last`, its marks then) that it must settle (#612): an item
/// that came into a state needing it, and each new question from a session on the Desk, one line each. Empty: nothing.
fn pass_news(st: &Store, desk: &str, last: &Value) -> Vec<String> {
    let mut out: Vec<String> = st
        .desk
        .items
        .iter()
        .filter(|i| i.project == desk && last["items"].get(&i.id).and_then(Value::as_str) != Some(i.state.as_str()))
        .filter_map(|i| SETTLE.iter().find(|(k, _)| *k == i.state).map(|(_, w)| format!("#{} ({}): {w}", i.id, i.title)))
        .collect();
    let was: Vec<&str> = last["asks"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let sids = desk_sids(st, desk);
    for it in st.items.iter().filter(|it| it.status == "pending" && it.kind != "waiting" && sids.contains(&it.origin.session_id) && !was.contains(&it.id.as_str())) {
        let q = it.tool_input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or(&it.tool_name);
        let who = st.sessions.get(&it.origin.session_id).map(|s| s.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "a session".into());
        out.push(format!("a new question from {who}: \"{}\"", q.trim()));
    }
    out
}

/// The lead's pass before Away (#612): the first one goes through every open item; a later one only what changed since.
pub(crate) enum Pass {
    Full,
    Only(Vec<String>),
}

/// The marks of Desk `desk`'s lead's last pass (config.json `desk.away_pass`), Null when it never made one.
fn last_pass(desk: &str) -> Value {
    crate::config::load().pointer("/desk/away_pass").and_then(|m| m.get(desk)).cloned().unwrap_or(Value::Null)
}

/// Whether session `sid` is stopped by a usage limit (Cue carries it on after the reset).
fn limited(st: &Store, sid: &str) -> bool {
    st.sessions.get(sid).is_some_and(|s| s.state == "limited")
}

/// `state.desk.projects[i].away`.
pub(crate) fn away_view(st: &Store, desk: &str, now: u64) -> Value {
    let mut v = Away::read(st, desk, now).view();
    let lead = st.desk.lead.get(desk).map(|l| l.sid.clone()).unwrap_or_default();
    v["sweeping"] = json!(!lead.is_empty() && is_sweeping(&lead));
    v
}

/// What Desk `desk`'s lead is asked as the owner presses Away (before it's on): settle what it can under `rules`;
/// every open item the first time, only what changed since its last pass after that (#612).
pub(crate) fn sweep_words(rules: &Value, pass: &Pass) -> String {
    let rules = rules_in(rules, &Value::Null);
    let may: Vec<&str> = RULES.iter().filter(|(k, _, _)| rules.get(*k).and_then(Value::as_bool).unwrap_or(false)).map(|(_, _, w)| *w).collect();
    let list = if may.is_empty() { "nothing: every call waits for the owner".to_string() } else { may.iter().map(|w| format!("\n- {w}")).collect::<String>() };
    let what = match pass {
        Pass::Full => "Go through every open item (working, waiting, to check) and settle everything you can".to_string(),
        Pass::Only(news) => format!(
            "Since your last pass, only these changed:\n{}\nSettle just these, nothing else: the rest stays as you left it. Settle them",
            news.iter().map(|n| format!("- {n}")).collect::<Vec<_>>().join("\n")
        ),
    };
    let most = match pass { Pass::Only(n) => n.len().min(5), Pass::Full => 5 };
    format!("The owner is about to go away. {what} under these rules, writing a DECIDED line for each call (DECIDED: <what you chose> — <why>). You may decide alone:{list}\nAnswer or close any ASK you can settle yourself. Leave at most {most} {} the owner, as ASK lines, most important first: any still open as Away goes on comes back to you to decide. Then stop.", if most == 1 { "question that truly needs" } else { "questions that truly need" })
}

/// While Away is on: when it started (ms), so the Desk keeps showing the decisions made since.
pub(crate) fn away_since(desk: &str) -> Option<u64> {
    let v = saved(desk);
    v.get("on").and_then(Value::as_bool).unwrap_or(false).then(|| v.get("since").and_then(Value::as_u64).unwrap_or(0))
}

/// The "Away mode" part of Desk `desk`'s summary for its lead, or None while Away is off.
pub(crate) fn away_block(st: &Store, desk: &str, now: u64) -> Option<String> {
    let a = Away::read(st, desk, now);
    a.on.then(|| block(&a, now))
}

/// How long a stretch ran (`ms`), as the Away tab says it: "under a minute", "45m", "2h 10m".
fn away_long(ms: u64) -> String {
    let m = (ms + 30_000) / 60_000;
    match (m / 60, m % 60) {
        (0, 0) => "under a minute".into(),
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// What Cue tells the lead as Away goes off (#612): how long and what moved (`back`: "2h 10m, 4 items moved, 3 calls
/// made"), in one line its chat folds into a "Back" card; the owner sees the rest in the Away tab, so no recap.
fn back_words(back: &str) -> String {
    let after = if back.is_empty() { String::new() } else { format!(" ({back})") };
    format!("Away mode is off: the owner is back{after}. Cue shows them what happened while they were away; don't welcome them back or recap it. Ask and decide as usual.")
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
    out.push_str("Don't write ASK: lines while the owner is away: Cue doesn't show them to the owner, it sends each back to you to decide yourself. Decide it and write a DECIDED line for it; pick what's easiest to change later.\n");
    if !waits.is_empty() {
        out.push_str("These stay the owner's: don't do them while they're away. Leave that item as it is and write DECIDED: Leave #<id> until the owner is back — <why>:\n");
        for (_, _, words) in &waits {
            out.push_str(&format!("- {words}\n"));
        }
    }
    out.push_str("These wait for the owner by themselves (don't ASK about them):\n");
    for words in LOCKED {
        out.push_str(&format!("- {words}\n"));
    }
    out.push_str("Carry on with the other items meanwhile.\n");
    out.push_str("A session's Ask me before step (delete, push, release) is held for the owner: the session is told and carries on; the owner allows or refuses it on return. A session stopped on a permission it can't grant itself waits for the owner in the Away list. Never grant it yourself: don't allow it, don't change its permission settings, don't tell it to get round it. Move the other items along meanwhile.\n");
    if on("route") {
        out.push_str("Start at most 3 new sessions while the owner is away.\n");
    }
    out.push_str("A session stopped by a usage limit is carried on by Cue after the reset: don't ASK about it.\n");
    out
}

/// What a lead is sent with its ASKs (question, item) while its Desk's Away is in force (#471): decide them yourself.
pub(crate) fn sent_back_words(asks: &[(String, Option<String>)]) -> String {
    let one = asks.len() == 1;
    let list: String = asks.iter().map(|(q, item)| format!("\n- {}{q}", item.as_ref().map(|i| format!("#{i}: ")).unwrap_or_default())).collect();
    let (this, it) = if one { ("this ASK", "it") } else { ("these ASKs", "each one, in this order") };
    format!("The owner is away: Cue didn't show them {this}. Decide {it} yourself and write a DECIDED line (DECIDED: <what you chose> — <why>), with the item's #id when it has one; the owner can undo it when back. If it's one that stays the owner's, write DECIDED: Leave #<id> until the owner is back — <why>. Then carry on.{list}")
}

/// Lead `lsid`'s calls from `first` on (this turn's DECIDED lines) answer the ASKs Cue sent back to it earlier (#471):
/// each marked with the question it was made in place of, by item first, then in order. ASKs left over wait for its
/// next calls.
pub(crate) fn answered_by_calls(st: &mut Store, lsid: &str, first: usize, fx: &mut crate::desk::hub::core::desk::Effects) {
    let Some(pending) = st.desk.run.away_asked.remove(lsid) else { return };
    let mut free: Vec<usize> = (first..st.desk.decisions.len()).filter(|&k| st.desk.decisions[k].call && st.desk.decisions[k].sid == lsid && st.desk.decisions[k].asked.is_empty()).collect();
    let mut mark = |st: &mut Store, k: usize, q: &str| {
        let d = &mut st.desk.decisions[k];
        d.asked = q.to_string();
        if !fx.decisions.contains(&d.id) {
            fx.decisions.push(d.id.clone());
        }
    };
    let mut left = Vec::new();
    for (q, item) in pending {
        match item.as_ref().and_then(|i| free.iter().position(|&k| st.desk.decisions[k].item_id.as_ref() == Some(i))) {
            Some(n) => mark(st, free.remove(n), &q),
            None => left.push((q, item)),
        }
    }
    let mut rest = Vec::new();
    for (q, item) in left {
        if free.is_empty() {
            rest.push((q, item));
        } else {
            mark(st, free.remove(0), &q);
        }
    }
    if !rest.is_empty() {
        st.desk.run.away_asked.insert(lsid.to_string(), rest);
    }
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
        let (cost, pre, items_now) = self.read(|st| (cost_now(st, desk), Away::read(st, desk, now), crate::desk::hub::away::away_log::item_states(st, desk)));
        let (was_on, rules_changed, before) = crate::config::update_with("desk.away", |c| {
            let mut map = c.pointer("/desk/away").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
            let before = map.get(desk).cloned().unwrap_or(Value::Null);
            let was_on = before.get("on").and_then(Value::as_bool).unwrap_or(false);
            let old_rules = rules_in(&before["rules"], &Value::Null);
            let rules = rules_in(away.get("rules").unwrap_or(&Value::Null), &old_rules);
            // Set again with the same limits (a rule changed): an Away that already ran out stays so, its lead told once.
            let same = on && was_on && before.get("until").and_then(Value::as_u64) == until && before.get("budget").and_then(Value::as_f64) == budget;
            let ended = if same { before.get("ended").cloned().unwrap_or(json!("")) } else { json!("") };
            let ended_at = if same { before.get("ended_at").cloned().unwrap_or(Value::Null) } else { Value::Null };
            // The items as Away went on (#416): what moved is told from these as it goes off.
            let items = if on && was_on { before.get("items").cloned().unwrap_or_else(|| items_now.clone()) } else if on { items_now.clone() } else { Value::Null };
            let (since, base) = if on && was_on {
                (before.get("since").cloned().unwrap_or(json!(now)), before.get("base").cloned().unwrap_or(json!(cost)))
            } else if on {
                (json!(now), json!(cost))
            } else {
                (before.get("since").cloned().unwrap_or(json!(0)), before.get("base").cloned().unwrap_or(json!(0.0)))
            };
            // Set again with new limits (a later end time, a bigger budget): it holds again until a turn's end finds otherwise.
            map[desk] = json!({ "on": on, "since": since, "until": until, "budget": budget, "base": base, "ended": ended, "ended_at": ended_at, "items": items, "rules": rules });
            Ok((Some(map), (was_on, rules != old_rules, before)))
        })?;
        // Back (#416): what happened while away kept for the dispatcher's Away tab.
        let back = if was_on && !on { self.desk_away_back(desk, &before, &pre, now) } else { String::new() };
        if on != was_on || (on && rules_changed) {
            let words = self.read(|st| {
                let a = Away::read(st, desk, now);
                if a.on { block(&a, now) } else { back_words(&back) }
            });
            self.tell_lead_away(desk, &words);
        }
        self.desk_away_asks(desk, on, now);
        // Its end time is noticed then even if no Desk session's turn ends (an idle Desk overnight). A timer left from
        // an earlier end time finds the limits changed and does nothing; once ended, the saved `ended` keeps it to one telling.
        if let Some(u) = until.filter(|_| on) {
            let desk = desk.to_string();
            self.later(u.saturating_sub(now), move |h| h.desk_away_end(&desk, None));
        }
        self.changed();
        Ok(self.read(|st| away_view(st, desk, now)))
    }

    /// Back from Away on Desk `desk` (`before`: its settings as it went off; `pre`: as it stood): the stretch kept.
    fn desk_away_back(&self, desk: &str, before: &Value, pre: &Away, now: u64) -> String {
        let since = before.get("since").and_then(Value::as_u64).unwrap_or(0);
        let end = before.get("ended_at").and_then(Value::as_u64).or_else(|| before.get("until").and_then(Value::as_u64).filter(|u| pre.ended == "until" && *u < now)).unwrap_or(now);
        let start = before.get("items").cloned().unwrap_or_else(|| json!({}));
        let (items, calls) = self.read(|st| crate::desk::hub::away::away_log::stretch(st, desk, since, end, &start));
        let n = |v: &Value| v.as_array().map_or(0, Vec::len);
        let moved = match n(&items) { 0 => "no items moved".to_string(), 1 => "1 item moved".into(), k => format!("{k} items moved") };
        let made = match n(&calls) { 0 => "no calls made".to_string(), 1 => "1 call made".into(), k => format!("{k} calls made") };
        crate::desk::hub::away::away_log::keep(json!({ "id": format!("away-{since}"), "desk": desk, "since": since, "end": end, "until": before.get("until"), "budget": before.get("budget"),
            "spent": (pre.spent * 100.0).round() / 100.0, "ended": pre.ended, "items": items, "calls": calls }));
        format!("{}, {moved}, {made}", away_long(end.saturating_sub(since)))
    }

    /// Away on Desk `desk` was just set (`on`): in force now, its lead's ASKs still waiting on the owner go back to it
    /// (#471); off, what it was sent back is no longer waited on.
    fn desk_away_asks(&self, desk: &str, on: bool, now: u64) {
        let (in_force, lead) = self.read(|st| (away_in_force(st, desk, now), st.desk.lead.get(desk).map(|l| l.sid.clone())));
        if in_force {
            self.desk_away_send_back_open(desk);
        } else if !on {
            if let Some(lead) = lead {
                self.write(|st| st.desk.run.away_asked.remove(&lead));
            }
        }
    }

    /// Desk `desk`'s lead's ASKs still waiting on the owner as its Away comes into force (#471): their cards closed and
    /// the questions sent back to the lead to decide itself. Only its own ASK lines: not a route waiting on a yes, a
    /// change to how work lands, or Cue's own question in its chat.
    fn desk_away_send_back_open(&self, desk: &str) {
        let Some((lead, cards)) = self.read(|st| {
            let lead = st.desk.lead.get(desk)?.sid.clone();
            let cards: Vec<(String, String, Option<String>)> = st
                .items
                .iter()
                .filter(|i| i.desk && i.status == "pending" && i.origin.session_id == lead && i.tool_input["desk_id"].as_str() == Some(desk))
                .filter(|i| ["desk_use", "desk_lands", "desk_old", "desk_push"].iter().all(|k| i.tool_input.get(*k).is_none()))
                .map(|i| (i.id.clone(), i.tool_input.pointer("/questions/0/question").and_then(Value::as_str).unwrap_or("").to_string(), i.tool_input["desk_item"].as_str().map(str::to_string)))
                .collect();
            Some((lead, cards))
        }) else { return };
        if cards.is_empty() {
            return;
        }
        let mut asks = Vec::new();
        for (id, question, item) in cards {
            self.finish(&id, "answered", "sent back to the lead to decide: you're away");
            asks.push((question, item));
        }
        let words = sent_back_words(&asks);
        // A lead a usage limit stopped reads it in its next Desk block instead (it would bounce off the limit).
        let reach = self.write(|st| {
            st.desk.run.away_asked.entry(lead.clone()).or_default().extend(asks);
            let reach = st.sessions.get(&lead).is_some() && !limited(st, &lead);
            if !reach {
                let held = st.desk.run.routes_held.entry(desk.to_string()).or_default();
                held.push_str(&words);
                held.push('\n');
            }
            reach
        });
        if reach {
            self.later(0, move |h| {
                if let Err(e) = h.tell(&lead, &words) {
                    eprintln!("cue: couldn't send {lead} its ASKs back: {e}");
                }
            });
        }
        self.changed();
    }

    /// Session `sid`'s turn is over: each Desk it's on whose Away has run out (its end time, or its budget) is ended,
    /// and its lead told, once.
    pub(crate) fn desk_away_check(&self, sid: &str) {
        if self.desk_away_turn_over(sid, false) {
            let desks: Vec<String> = self.read(|st| st.desk.lead.iter().filter(|(_, l)| l.sid == sid).map(|(d, _)| d.clone()).collect());
            for desk in &desks {
                self.desk_away_pass_kept(desk);
            }
            self.changed();
        }
        let all = crate::config::load().pointer("/desk/away").and_then(Value::as_object).cloned().unwrap_or_default();
        for desk in all.keys() {
            self.desk_away_end(desk, Some(sid));
        }
    }

    /// Desk `desk`'s Away, if it has run out (its end time, or its budget) and isn't marked so yet: marked ended, and
    /// its lead told, once. `by`: only when that session is one of the Desk's (a turn's end); None for the end-time timer.
    fn desk_away_end(&self, desk: &str, by: Option<&str>) {
        let v = saved(desk);
        if !v.get("on").and_then(Value::as_bool).unwrap_or(false) || !v.get("ended").and_then(Value::as_str).unwrap_or("").is_empty() {
            return;
        }
        let now = now_ms();
        let found = self.read(|st| by.is_none_or(|sid| desk_sids(st, desk).iter().any(|s| s == sid)).then(|| Away::read(st, desk, now)));
        let Some(a) = found.filter(|a| !a.ended.is_empty()) else { return };
        let saved = crate::config::update_with("desk.away", |c| {
            let mut map = c.pointer("/desk/away").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
            let Some(d) = map.get_mut(desk).filter(|d| d.get("on").and_then(Value::as_bool).unwrap_or(false)) else { return Ok((None, false)) };
            if !d.get("ended").and_then(Value::as_str).unwrap_or("").is_empty() {
                return Ok((None, false));
            }
            d["ended"] = json!(a.ended);
            d["ended_at"] = json!(now);
            Ok((Some(map), true))
        });
        match saved {
            Ok(true) => {
                self.tell_lead_away(desk, &block(&a, now));
                self.changed();
            }
            Ok(false) => {}
            Err(e) => eprintln!("cue: couldn't end Away on {desk}: {e}"),
        }
    }

    /// Desk `desk`'s lead is asked to sort things out before Away: the window says so until its next turn ends. False
    /// (nothing asked) while a usage limit has stopped it: a message then would bounce off the limit and cancel Cue's
    /// carry-on; it reads Away's words in the Desk's summary once it's carried on.
    pub(crate) fn desk_away_sweeping(&self, desk: &str) -> bool {
        let Some((lead, busy)) = self.read(|st| st.desk.lead.get(desk).map(|l| (l.sid.clone(), st.sessions.get(&l.sid).is_some_and(|s| !matches!(s.state.as_str(), "" | "idle" | "waiting" | "stopped" | "ended"))))) else { return true };
        if self.read(|st| limited(st, &lead)) {
            return false;
        }
        {
            let mut held = crate::locking::lock(sweeping(), "leads sweeping for Away");
            held.insert(lead, u8::from(busy));
        }
        self.changed();
        true
    }

    /// The pass Desk `desk`'s lead should make before Away (#612): None when there's nothing new since its last one
    /// (or it's on one now); every item the first time; after that only what changed and needs it.
    pub(crate) fn desk_away_pass_due(&self, desk: &str) -> Option<Pass> {
        let last = last_pass(desk);
        let (lead, news) = self.read(|st| (st.desk.lead.get(desk).map(|l| l.sid.clone()), pass_news(st, desk, &last)));
        if lead.as_deref().is_some_and(is_sweeping) {
            return None;
        }
        if last.is_null() {
            return Some(Pass::Full);
        }
        (!news.is_empty()).then_some(Pass::Only(news))
    }

    /// Desk `desk`'s lead finished its pass: the Desk as it left it is kept, so the next Away goes by what changed since.
    fn desk_away_pass_kept(&self, desk: &str) {
        let marks = self.read(|st| pass_marks(st, desk));
        let r = crate::config::update_with("desk.away_pass", |c| {
            let mut map = c.pointer("/desk/away_pass").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
            map[desk] = marks;
            Ok((Some(map), ()))
        });
        if let Err(e) = r {
            eprintln!("cue: couldn't keep {desk}'s pass before Away: {e}");
        }
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

    /// `words` to Desk `desk`'s lead: a row on its Desk's change feed, so it hears it in order with the rest (the
    /// sender holds it while a usage limit has the lead stopped; the Desk's summary carries Away's words then too).
    fn tell_lead_away(&self, desk: &str, words: &str) {
        self.desk_feed_away(desk, words);
    }
}
