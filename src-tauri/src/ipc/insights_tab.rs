//! The Insights tab: how sessions went over the last days, summed from the turns logs/turns.rs keeps
//! (db.rs `turns`), and how hand-offs and insights went.

use crate::db;
use crate::logs::prices;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The last `days` days (7, 30 or 90): totals and per day, and three findings: answers that came after a
/// session's cache ran out, long sessions past their hand-off point, and turns your next prompt corrected.
#[tauri::command]
pub(crate) async fn insights_summary(days: Option<u32>) -> Result<Value, String> {
    let days = days.unwrap_or(30).clamp(1, 365);
    tauri::async_runtime::spawn_blocking(move || {
        let mut v = summary(days);
        // Still reading the logs' history (a new Mac, or years of logs): how far, for the tab's bar.
        if let Some((done, total)) = crate::logs::turns::catching_up() {
            v["reading"] = serde_json::json!({ "done": done, "total": total });
        }
        v
    })
    .await
    .map_err(|e| e.to_string())
}

fn median(v: &mut [u64]) -> u64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

/// Where a session's turn re-wrote its context after waiting on you: a wait this long (past a 5-minute cache)
/// and a first reply that wrote at least this much, more than it read from the cache.
const WAIT_MS: u64 = 5 * 60 * 1000;
const REWRITE_MIN: u64 = 20_000;
/// Past this many times its starting context, and this big, a session has passed its hand-off point with
/// its cache warm (insights.js HANDOFF_WARM; a cold one's cost is what the waiting finding counts).
const HANDOFF_GROWTH: u64 = 4;
const HANDOFF_CONTEXT: u64 = 300_000;

/// A turn that looked around this much itself, and grew the session's context this much doing it, could
/// have sent a background agent to look instead: the agent's own context fills, and the session gets back a
/// summary about this big. Set from 30 days of real sessions: about a hundred turns a month clear both.
const LOOKS: u64 = 30;
const LOOK_GROWTH: u64 = 80_000;
const SUMMARY: u64 = 5_000;
/// A session that passed its hand-off point while editing in this many areas of its project (logs/steps.rs
/// area_of) was several pieces of work in one context: each could have had a session (relay) or an agent of its own.
const AREAS: usize = 3;

/// What a session could have delegated. `search`: its costliest turn that looked around itself (its index in
/// the session's turns, reads and searches, how much its context grew, and what its later replies re-read
/// of that growth: tokens, dollars, replies). `split`: the areas it edited, when it passed its hand-off point
/// in three or more without spawning a relay executor. `agents`, `relay`: what it did delegate.
#[derive(Default, Debug, PartialEq)]
pub(crate) struct Delegate {
    pub search: Option<Search>,
    pub search_tokens: u64,
    pub search_usd: f64,
    pub searches: u64,
    pub split: Vec<String>,
    pub agents: u64,
    pub relay: u64,
}

#[derive(Default, Debug, PartialEq)]
pub(crate) struct Search {
    pub turn: usize,
    pub looks: u64,
    pub growth: u64,
    pub tokens: u64,
    pub usd: f64,
    pub later: u64,
}

/// One session's turns, oldest first, and the context it started at (for its hand-off point).
/// SHORTCUT: a relay executor's own session can be told "split" too: the turns history doesn't keep which
/// sessions were executors (leads.rs knows only live ones). Upgrade path: keep a session's relay role with its turns.
/// SHORTCUT: a turn's growth is its whole growth, what it wrote and ran too, not only what its looking read.
/// Upgrade path: keep each tool result's size with the turn.
pub(crate) fn delegate_of(turns: &[&db::StoredTurn], start: u64) -> Delegate {
    let mut d = Delegate::default();
    for (i, t) in turns.iter().enumerate() {
        (d.agents, d.relay) = (d.agents + t.agents, d.relay + t.relay);
        // From what its first reply carried (a relay executor's packet is often one long turn, the session's first).
        let growth = t.context.saturating_sub(t.usage.first_read + t.usage.first_write);
        if t.looks < LOOKS || t.agents > 0 || growth < LOOK_GROWTH {
            continue;
        }
        // What its later replies re-read of the growth, until the session compacted (its context fell back).
        let extra = growth - SUMMARY;
        let (mut tokens, mut usd, mut later) = (0u64, 0f64, 0u64);
        for l in turns[i + 1..].iter().filter(|l| l.context > 0).take_while(|l| l.context >= t.context.saturating_sub(extra)) {
            let n = (l.usage.read as f64 * extra.min(l.context) as f64 / l.context as f64) as u64;
            (tokens, later) = (tokens + n, later + 1);
            usd += prices::rates(&l.model).map_or(0.0, |r| n as f64 * r.read / 1e6);
        }
        (d.searches, d.search_tokens, d.search_usd) = (d.searches + 1, d.search_tokens + tokens, d.search_usd + usd);
        if d.search.as_ref().is_none_or(|s| tokens > s.tokens) {
            d.search = Some(Search { turn: i, looks: t.looks, growth, tokens, usd, later });
        }
    }
    let peak = turns.iter().map(|t| t.context).max().unwrap_or(0);
    if d.relay == 0 && start > 0 && peak >= HANDOFF_CONTEXT && peak >= start * HANDOFF_GROWTH {
        let mut areas: Vec<String> = turns.iter().flat_map(|t| t.areas.lines()).filter(|a| !a.is_empty()).map(str::to_string).collect();
        areas.sort();
        areas.dedup();
        if areas.len() >= AREAS {
            d.split = areas;
        }
    }
    d
}

/// Whether relay (the claude-relay plugin) is on in Claude Code on this Mac: its settings' enabledPlugins.
/// SHORTCUT: this Mac's settings only, for every machine's sessions. Upgrade path: each machine's own.
pub(crate) fn relay_on() -> bool {
    let path = std::path::Path::new(&std::env::var("HOME").unwrap_or_default()).join(".claude/settings.json");
    let v: Value = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    v.get("enabledPlugins").and_then(Value::as_object).is_some_and(|m| m.iter().any(|(k, on)| k.starts_with("relay@") && on.as_bool() == Some(true)))
}

/// For each session asked about (a waiting card's): what it could have delegated, from its turns so far.
/// Only those with something to say.
#[tauri::command]
pub(crate) async fn insight_delegate(session_ids: Vec<String>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || delegate_states(&session_ids)).await.map_err(|e| e.to_string())
}

/// What insight_delegate answers with (it reads the turns history: call it off the main thread).
pub(crate) fn delegate_states(session_ids: &[String]) -> Value {
    let mut out = serde_json::Map::new();
    for sid in session_ids.iter().take(50) {
        let turns = db::turns_of(sid);
        let start = turns.iter().map(|t| t.context).find(|&c| c > 0).unwrap_or(0);
        let d = delegate_of(&turns.iter().collect::<Vec<_>>(), start);
        if d.search.is_some() || !d.split.is_empty() {
            out.insert(sid.clone(), delegate_json(&d));
        }
    }
    json!({ "relay": relay_on(), "sessions": out })
}

fn delegate_json(d: &Delegate) -> Value {
    let usd2 = |x: f64| (x * 100.0).round() / 100.0;
    json!({
        "search": d.search.as_ref().map(|s| json!({ "looks": s.looks, "growth": s.growth, "tokens": s.tokens, "usd": usd2(s.usd), "later": s.later })),
        "searches": d.searches, "search_tokens": d.search_tokens, "search_usd": usd2(d.search_usd),
        "split": d.split, "agents": d.agents, "relay": d.relay,
    })
}

#[derive(Default)]
struct Sum {
    turns: u64,
    tokens: u64,
    usd: f64,
    errors: u64,
    interrupted: u64,
    corrected: u64,
    wait_usd: f64,
    wait_tokens: u64,
    long_usd: f64,
    sessions: HashSet<String>,
    contexts: Vec<u64>,
}

impl Sum {
    fn add(&mut self, t: &db::StoredTurn, usd: f64) {
        self.turns += 1;
        self.tokens += t.tokens;
        self.usd += usd;
        self.errors += t.errors;
        self.interrupted += u64::from(t.interrupted);
        self.corrected += u64::from(t.corrected);
        self.sessions.insert(t.session_id.clone());
        if t.context > 0 {
            self.contexts.push(t.context);
        }
    }
}

/// One session in a finding's list: where it ran, and what the finding counted in it.
#[derive(Default)]
struct Hit {
    usd: f64,
    tokens: u64,
    times: u64,
    longest_wait_ms: u64,
    peak_context: u64,
    last: Option<usize>,
}

/// What a late answer cost: its first reply wrote `first_write` tokens into the cache that reading
/// would have served, at the write rate it paid less the read rate. None when it wasn't one.
/// SHORTCUT: priced at the hour's write rate when any reply of the turn wrote for an hour, though the
/// first reply may have written for 5 minutes; the split clamps what that overstates. Upgrade path: keep
/// the first reply's own 5m/1h split in Usage.
fn rewrite_cost(t: &db::StoredTurn) -> Option<(u64, f64)> {
    let u = &t.usage;
    if t.waited_ms < WAIT_MS || u.first_write < REWRITE_MIN || u.first_write <= u.first_read {
        return None;
    }
    let r = prices::rates(&t.model);
    let write = r.map_or(0.0, |r| if u.write_1h > 0 { r.write_1h } else { r.write_5m });
    Some((u.first_write, r.map_or(0.0, |r| u.first_write as f64 * (write - r.read) / 1e6)))
}

/// What a turn past its session's hand-off point re-read above where the session started: the share of
/// its cache reads beyond the starting context, at the read rate. None before that point.
/// SHORTCUT: scales the turn's reads by its last reply's context, though its earlier calls carried a little
/// less; close for a turn's handful of calls. Upgrade path: keep each call's context with the turn.
fn long_cost(t: &db::StoredTurn, start: u64) -> Option<(u64, f64)> {
    if start == 0 || t.context < HANDOFF_CONTEXT || t.context < start * HANDOFF_GROWTH {
        return None;
    }
    let extra = (t.usage.read as f64 * (t.context - start) as f64 / t.context as f64) as u64;
    Some((extra, prices::rates(&t.model).map_or(0.0, |r| extra as f64 * r.read / 1e6)))
}

/// What a turn cost by kind, in dollars: fresh input, output, cache reads, cache writes. None unpriced.
fn kinds(t: &db::StoredTurn) -> Option<[f64; 4]> {
    let (r, u) = (prices::rates(&t.model)?, &t.usage);
    let m = |n: u64, rate: f64| n as f64 * rate / 1e6;
    Some([m(u.fresh, r.input), m(u.output, r.output), m(u.read, r.read), m(u.write_5m, r.write_5m) + m(u.write_1h, r.write_1h)])
}

/// A slot in the week's grid, Monday's midnight hour first: libc's weekday (Sunday 0) and hour.
fn slot_of(wday: i32, hour: i32) -> usize {
    (((wday + 6) % 7) * 24 + hour.clamp(0, 23)) as usize
}

/// The week's slot `ms` falls in, in local time.
fn local_slot(ms: u64) -> usize {
    let t = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    slot_of(tm.tm_wday, tm.tm_hour)
}

/// A session's context turn by turn, at most `max` points: each stretch of turns kept as its biggest
/// (turn number, context), so a climb's peaks survive.
fn climb(ctx: &[u64], max: usize) -> Vec<(usize, u64)> {
    if ctx.len() <= max {
        return ctx.iter().copied().enumerate().collect();
    }
    (0..max).filter_map(|b| {
        let (lo, hi) = (b * ctx.len() / max, ((b + 1) * ctx.len() / max).max(b * ctx.len() / max + 1));
        (lo..hi.min(ctx.len())).map(|i| (i, ctx[i])).max_by_key(|&(_, c)| c)
    }).collect()
}

/// The numbers insights_summary answers with (reads the database and folders: off the main thread).
fn summary(days: u32) -> Value {
    let now = crate::model::now_ms();
    let since = now.saturating_sub(u64::from(days) * 24 * 3600 * 1000);
    let turns = db::turns_since(since);
    let sids: Vec<&str> = turns.iter().map(|t| t.session_id.as_str()).collect::<HashSet<_>>().into_iter().collect();
    let starts = db::start_contexts(&sids);
    let mut all = Sum::default();
    let (mut priced, mut writes) = (0u64, 0u64);
    let mut by_day: BTreeMap<String, Sum> = BTreeMap::new();
    let (mut waiting, mut long): (HashMap<&str, Hit>, HashMap<&str, Hit>) = Default::default();
    let (mut wait_turns, mut wait_tokens, mut wait_usd, mut long_tokens, mut long_usd) = (0u64, 0u64, 0f64, 0u64, 0f64);
    let mut corrections: Vec<usize> = Vec::new();
    // Where the dollars went (fresh, output, reads, writes, re-writes after a wait, re-reads past a hand-off),
    // and by model; the late answers one by one, and by when in the week their session was left waiting.
    let mut split = [0f64; 6];
    let mut models: HashMap<&str, (f64, u64)> = HashMap::new();
    let mut answers: Vec<(usize, u64, f64)> = Vec::new();
    let (mut grid_usd, mut grid_n) = (vec![0f64; 168], vec![0u64; 168]);
    // Each session's context turn by turn, and the turn it passed its hand-off point.
    let mut contexts: HashMap<&str, Vec<u64>> = HashMap::new();
    let mut passed: HashMap<&str, usize> = HashMap::new();
    let mut corrected_by: HashMap<String, (u64, u64)> = HashMap::new();
    // A folder's project, looked up once: its repository on this Mac, its folder's name on a machine.
    let mut projects: HashMap<(String, String), String> = HashMap::new();
    let mut project = |t: &db::StoredTurn| projects.entry((t.machine.clone(), t.cwd.clone())).or_insert_with(|| if t.machine.is_empty() { crate::model::project_of(&t.cwd) } else { crate::model::folder_of(&t.cwd) }).clone();
    for (i, t) in turns.iter().enumerate() {
        let usd = prices::cost(&t.model, &t.usage);
        if usd.is_some() {
            priced += t.tokens;
        }
        let usd = usd.unwrap_or(0.0);
        writes += t.usage.write_5m + t.usage.write_1h;
        if let Some(k) = kinds(t) {
            for (s, v) in split.iter_mut().zip(k) {
                *s += v;
            }
        }
        let m = models.entry(t.model.as_str()).or_default();
        (m.0, m.1) = (m.0 + usd, m.1 + t.tokens);
        let ctx = contexts.entry(&t.session_id).or_default();
        if t.context > 0 {
            ctx.push(t.context);
        }
        let p = corrected_by.entry(project(t)).or_default();
        (p.0, p.1) = (p.0 + u64::from(t.corrected), p.1 + 1);
        all.add(t, usd);
        let day = by_day.entry(t.day.clone()).or_default();
        day.add(t, usd);
        if let Some((tokens, cost)) = rewrite_cost(t) {
            (wait_turns, wait_tokens, wait_usd) = (wait_turns + 1, wait_tokens + tokens, wait_usd + cost);
            (split[3], split[4]) = (split[3] - cost, split[4] + cost);
            answers.push((i, tokens, cost));
            let at = local_slot(t.at_ms.saturating_sub(t.waited_ms));
            (grid_usd[at], grid_n[at]) = (grid_usd[at] + cost, grid_n[at] + 1);
            (day.wait_tokens, day.wait_usd) = (day.wait_tokens + tokens, day.wait_usd + cost);
            let h = waiting.entry(&t.session_id).or_default();
            (h.usd, h.tokens, h.times, h.longest_wait_ms, h.last) = (h.usd + cost, h.tokens + tokens, h.times + 1, h.longest_wait_ms.max(t.waited_ms), Some(i));
        }
        if let Some((tokens, cost)) = long_cost(t, starts.get(&t.session_id).copied().unwrap_or(0)) {
            (long_tokens, long_usd) = (long_tokens + tokens, long_usd + cost);
            (split[2], split[5]) = (split[2] - cost, split[5] + cost);
            let n = contexts.get(t.session_id.as_str()).map_or(0, |c| c.len().saturating_sub(1));
            passed.entry(&t.session_id).or_insert(n);
            day.long_usd += cost;
            let h = long.entry(&t.session_id).or_default();
            (h.usd, h.tokens, h.times, h.peak_context, h.last) = (h.usd + cost, h.tokens + tokens, h.times + 1, h.peak_context.max(t.context), Some(i));
        }
        if t.corrected {
            corrections.push(i);
        }
    }
    // What each session could have delegated, from its turns in the range (its start from before it, when it began earlier).
    let mut of_session: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, t) in turns.iter().enumerate() {
        of_session.entry(&t.session_id).or_default().push(i);
    }
    let mut delegates: Vec<(usize, Delegate)> = of_session.iter().map(|(sid, ix)| {
        let ts: Vec<&db::StoredTurn> = ix.iter().map(|&i| &turns[i]).collect();
        (*ix.last().unwrap_or(&0), delegate_of(&ts, starts.get(*sid).copied().unwrap_or(0)), ix)
    }).map(|(last, mut d, ix)| {
        // A search turn's index in the session, as an index into all the range's turns.
        if let Some(s) = d.search.as_mut() {
            s.turn = ix[s.turn];
        }
        (last, d)
    }).collect();
    let usd2 = |x: f64| (x * 100.0).round() / 100.0;
    let at = |i: usize, project: &mut dyn FnMut(&db::StoredTurn) -> String| {
        let t = &turns[i];
        json!({ "session_id": t.session_id, "project": project(t), "cwd": t.cwd, "machine": t.machine, "harness": t.harness, "last_ms": t.at_ms })
    };
    // The 8 sessions a finding counted most in, by dollars (tokens where nothing was priced).
    let top = |m: HashMap<&str, Hit>, long: bool, project: &mut dyn FnMut(&db::StoredTurn) -> String| -> Vec<Value> {
        let mut rows: Vec<(&str, Hit)> = m.into_iter().collect();
        rows.sort_by(|a, b| b.1.usd.total_cmp(&a.1.usd).then(b.1.tokens.cmp(&a.1.tokens)));
        rows.into_iter().take(8).filter_map(|(sid, h)| {
            let mut v = at(h.last?, project);
            v["usd"] = json!(usd2(h.usd));
            v["tokens"] = json!(h.tokens);
            if long {
                v["start_context"] = json!(starts.get(sid).copied().unwrap_or(0));
                v["peak_context"] = json!(h.peak_context);
                v["turns"] = json!(h.times);
                // Its climb, for a small chart: (turn, context) points, and the turn it passed its hand-off point.
                let c = contexts.get(sid).map_or(&[][..], |c| c.as_slice());
                v["climb"] = json!(climb(c, 60).into_iter().map(|(i, x)| [i as u64, x]).collect::<Vec<_>>());
                v["climb_turns"] = json!(c.len());
                v["handoff_turn"] = json!(passed.get(sid));
            } else {
                v["times"] = json!(h.times);
                v["longest_wait_ms"] = json!(h.longest_wait_ms);
            }
            Some(v)
        }).collect()
    };
    let long_by: HashMap<String, f64> = long.iter().map(|(sid, h)| (sid.to_string(), h.usd)).collect();
    let waiting_top = top(waiting, false, &mut project);
    let long_top = top(long, true, &mut project);
    let examples: Vec<Value> = corrections.iter().rev().take(12).map(|&i| {
        let t = &turns[i];
        let mut v = at(i, &mut project);
        v["at_ms"] = json!(t.at_ms);
        v["prompt"] = json!(t.prompt);
        v["correction"] = json!(t.correction);
        v["usd"] = json!(usd2(prices::cost(&t.model, &t.usage).unwrap_or(0.0)));
        v
    }).collect();
    // The 400 costliest late answers, each a dot: how long it waited, what it re-wrote, what that cost.
    answers.sort_by(|a, b| b.2.total_cmp(&a.2));
    let answers: Vec<Value> = answers.iter().take(400).map(|&(i, tokens, cost)| {
        let mut v = at(i, &mut project);
        (v["at_ms"], v["waited_ms"], v["tokens"], v["usd"]) = (json!(turns[i].at_ms), json!(turns[i].waited_ms), json!(tokens), json!(usd2(cost)));
        v
    }).collect();
    let (mut d_tokens, mut d_usd, mut d_turns, mut d_agents, mut d_agent_sessions, mut d_relay) = (0u64, 0f64, 0u64, 0u64, 0u64, 0u64);
    for (_, d) in &delegates {
        (d_tokens, d_usd, d_turns) = (d_tokens + d.search_tokens, d_usd + d.search_usd, d_turns + d.searches);
        (d_agents, d_agent_sessions, d_relay) = (d_agents + d.agents, d_agent_sessions + u64::from(d.agents > 0), d_relay + d.relay);
    }
    delegates.sort_by(|a, b| b.1.search_usd.total_cmp(&a.1.search_usd).then(b.1.search_tokens.cmp(&a.1.search_tokens)));
    let searched: Vec<Value> = delegates.iter().filter_map(|(_, d)| d.search.as_ref().map(|s| (s, d))).take(8).map(|(s, d)| {
        let t = &turns[s.turn];
        let mut v = at(s.turn, &mut project);
        (v["at_ms"], v["prompt"]) = (json!(t.at_ms), json!(t.prompt));
        (v["looks"], v["growth"], v["later"], v["tokens"], v["usd"], v["turns"]) = (json!(s.looks), json!(s.growth), json!(s.later), json!(d.search_tokens), json!(usd2(d.search_usd)), json!(d.searches));
        v
    }).collect();
    // The sessions that ran long across several areas, by how many, then their re-reads past the hand-off point.
    let mut spread: Vec<&(usize, Delegate)> = delegates.iter().filter(|(_, d)| !d.split.is_empty()).collect();
    let long_usd_of = |sid: &str| long_by.get(sid).copied().unwrap_or(0.0);
    spread.sort_by(|a, b| b.1.split.len().cmp(&a.1.split.len()).then(long_usd_of(&turns[b.0].session_id).total_cmp(&long_usd_of(&turns[a.0].session_id))));
    let spread_n = spread.len();
    let spread: Vec<Value> = spread.into_iter().take(8).map(|(last, d)| {
        let mut v = at(*last, &mut project);
        let sid = turns[*last].session_id.as_str();
        (v["areas"], v["usd"], v["peak_context"]) = (json!(d.split), json!(usd2(long_usd_of(sid))), json!(contexts.get(sid).and_then(|c| c.iter().max()).copied().unwrap_or(0)));
        v
    }).collect();
    let mut models: Vec<(&str, (f64, u64))> = models.into_iter().collect();
    models.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0).then(b.1 .1.cmp(&a.1 .1)));
    // The projects whose turns were corrected most (at least 20 turns, so one slip isn't a rate).
    let mut corrected_by: Vec<(String, (u64, u64))> = corrected_by.into_iter().filter(|(_, (n, all))| *n > 0 && *all >= 20).collect();
    corrected_by.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    let (sent, continued) = db::handoff_counts(since);
    let insights: Vec<Value> = db::insight_counts(since).into_iter().map(|(rule, shown, acted, tokens)| json!({ "rule": rule, "shown": shown, "acted": acted, "tokens": tokens })).collect();
    let share = |a: u64, b: u64| if b == 0 { 0.0 } else { ((a as f64 / b as f64) * 1000.0).round() / 1000.0 };
    json!({
        "days": days,
        "since_ms": since,
        "totals": { "turns": all.turns, "sessions": all.sessions.len(), "tokens": all.tokens, "usd": usd2(all.usd), "priced": share(priced, all.tokens) },
        "split": { "fresh": usd2(split[0]), "output": usd2(split[1]), "read": usd2(split[2].max(0.0)), "write": usd2(split[3].max(0.0)), "rewrite": usd2(split[4]), "long": usd2(split[5]) },
        "models": models.iter().take(6).map(|(m, (usd, tokens))| json!({ "model": m, "usd": usd2(*usd), "tokens": tokens })).collect::<Vec<_>>(),
        "by_day": by_day.into_iter().map(|(day, mut s)| json!({
            "day": day, "turns": s.turns, "tokens": s.tokens, "usd": usd2(s.usd), "median_context": median(&mut s.contexts),
            "corrected": s.corrected, "interrupted": s.interrupted, "errors": s.errors,
            "wait_usd": usd2(s.wait_usd), "wait_tokens": s.wait_tokens, "long_usd": usd2(s.long_usd),
        })).collect::<Vec<_>>(),
        "waiting": { "usd": usd2(wait_usd), "tokens": wait_tokens, "turns": wait_turns, "share_of_writes": share(wait_tokens, writes), "sessions": waiting_top,
            "answers": answers, "grid_usd": grid_usd.iter().map(|&x| usd2(x)).collect::<Vec<_>>(), "grid_turns": grid_n },
        "long": { "usd": usd2(long_usd), "tokens": long_tokens, "sessions": long_top, "handoffs": { "sent": sent, "continued": continued } },
        "corrections": { "count": all.corrected, "share": share(all.corrected, all.turns), "examples": examples,
            "by_project": corrected_by.iter().take(6).map(|(p, (n, all))| json!({ "project": p, "count": n, "turns": all })).collect::<Vec<_>>() },
        "delegate": { "usd": usd2(d_usd), "tokens": d_tokens, "turns": d_turns, "sessions": searched, "spread": spread, "spread_count": spread_n,
            "agents": d_agents, "agent_sessions": d_agent_sessions, "relay_spawns": d_relay, "relay": relay_on(),
            "looks_min": LOOKS, "growth_min": LOOK_GROWTH, "areas_min": AREAS },
        "insights": insights,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_median_of_none_is_zero_and_of_some_is_the_middle() {
        assert_eq!(median(&mut []), 0);
        assert_eq!(median(&mut [5, 1, 9]), 5);
    }

    fn turn(waited_ms: u64, context: u64, usage: crate::logs::steps::Usage) -> db::StoredTurn {
        db::StoredTurn { day: String::new(), session_id: "s".into(), tokens: 0, context, model: "claude-opus-5-5".into(), cwd: String::new(), machine: String::new(),
            errors: 0, interrupted: false, corrected: false, at_ms: 0, harness: "claude".into(), usage, waited_ms, prompt: String::new(), correction: String::new(),
            looks: 0, agents: 0, relay: 0, areas: String::new() }
    }

    /// A turn at `context` that read `read` from the cache, looked around `looks` times and edited in `areas`.
    /// Its first reply carried `first` of it.
    fn work(context: u64, read: u64, looks: u64, areas: &str) -> db::StoredTurn {
        db::StoredTurn { looks, areas: areas.into(), ..turn(0, context, crate::logs::steps::Usage { read, ..Default::default() }) }
    }
    fn from(first: u64, t: db::StoredTurn) -> db::StoredTurn {
        db::StoredTurn { usage: crate::logs::steps::Usage { first_read: first, ..t.usage }, ..t }
    }

    #[test]
    fn a_turn_that_looked_around_itself_counts_what_its_later_replies_re_read() {
        // 40 looks grew it 100k → 200k; the next two replies re-read the 95k above a summary; then it compacted.
        let ts = [work(100_000, 0, 0, ""), from(100_000, work(200_000, 0, 40, "")), work(210_000, 420_000, 0, ""), work(220_000, 440_000, 0, ""), work(30_000, 30_000, 0, "")];
        let d = delegate_of(&ts.iter().collect::<Vec<_>>(), 100_000);
        let s = d.search.expect("a search turn");
        assert_eq!((s.turn, s.looks, s.growth, s.later), (1, 40, 100_000, 2));
        assert_eq!(s.tokens, 420_000 * 95 / 210 + 440_000 * 95 / 220, "each later reply's reads, the growth's share of them");
        assert!(s.usd > 0.0 && (s.usd - s.tokens as f64 * prices::rates("claude-opus-5-5").unwrap().read / 1e6).abs() < 1e-6, "at the read rate");
        assert_eq!(d.searches, 1);
        // A session's first turn (a relay executor's one packet): from its first reply, not from where the turn ended.
        let one = [from(20_000, work(200_000, 0, 40, "")), work(210_000, 420_000, 0, "")];
        let s = delegate_of(&one.iter().collect::<Vec<_>>(), 200_000).search.expect("its first turn");
        assert_eq!((s.turn, s.growth, s.later), (0, 180_000, 1));
    }

    #[test]
    fn looking_around_is_not_a_finding_when_it_was_small_or_delegated() {
        let few = [work(100_000, 0, 0, ""), from(100_000, work(200_000, 0, LOOKS - 1, ""))];
        assert!(delegate_of(&few.iter().collect::<Vec<_>>(), 100_000).search.is_none(), "too few looks");
        let small = [work(100_000, 0, 0, ""), from(100_000, work(150_000, 0, 40, ""))];
        assert!(delegate_of(&small.iter().collect::<Vec<_>>(), 100_000).search.is_none(), "grew too little");
        let sent = [work(100_000, 0, 0, ""), db::StoredTurn { agents: 1, ..from(100_000, work(200_000, 0, 40, "")) }];
        let d = delegate_of(&sent.iter().collect::<Vec<_>>(), 100_000);
        assert!(d.search.is_none(), "it sent an agent in that turn");
        assert_eq!(d.agents, 1);
    }

    #[test]
    fn a_long_session_across_several_areas_could_have_been_split_unless_relay_was() {
        let ts = [work(50_000, 0, 0, "ui"), work(150_000, 0, 0, "src-tauri\nui"), work(350_000, 0, 0, "tests")];
        let d = delegate_of(&ts.iter().collect::<Vec<_>>(), 50_000);
        assert_eq!(d.split, vec!["src-tauri", "tests", "ui"]);
        let two = [work(50_000, 0, 0, "ui"), work(350_000, 0, 0, "src-tauri")];
        assert!(delegate_of(&two.iter().collect::<Vec<_>>(), 50_000).split.is_empty(), "two areas");
        assert!(delegate_of(&ts.iter().collect::<Vec<_>>(), 100_000).split.is_empty(), "under 4× its start");
        let relayed = [db::StoredTurn { relay: 1, ..work(50_000, 0, 0, "ui") }, work(150_000, 0, 0, "src-tauri"), work(350_000, 0, 0, "tests")];
        let d = delegate_of(&relayed.iter().collect::<Vec<_>>(), 50_000);
        assert!(d.split.is_empty(), "it spawned a relay executor");
        assert_eq!(d.relay, 1);
    }

    #[test]
    fn a_late_answer_costs_its_rewrite_less_the_read_it_replaced() {
        let u = crate::logs::steps::Usage { first_write: 500_000, write_1h: 500_000, first_read: 2_000, ..Default::default() };
        // 0.5M at $8 (an hour's write) less $0.20 (a read)
        let (tokens, usd) = rewrite_cost(&turn(2 * 3600 * 1000, 500_000, u)).unwrap();
        assert_eq!(tokens, 500_000);
        assert!((usd - 3.9).abs() < 1e-9);
        // Answered while it was still cached, or it read its context: not a late answer.
        assert!(rewrite_cost(&turn(60_000, 500_000, u)).is_none());
        assert!(rewrite_cost(&turn(2 * 3600 * 1000, 500_000, crate::logs::steps::Usage { first_read: 600_000, ..u })).is_none());
    }

    #[test]
    fn a_long_session_counts_only_what_it_reads_above_its_start() {
        let u = crate::logs::steps::Usage { read: 6_000_000, ..Default::default() };
        // Past 4× a 100k start: 6M read at 600k context, 5/6 of it above the start, at $0.20.
        let (tokens, usd) = long_cost(&turn(0, 600_000, u), 100_000).unwrap();
        assert_eq!(tokens, 5_000_000);
        assert!((usd - 1.0).abs() < 1e-9);
        assert!(long_cost(&turn(0, 350_000, u), 100_000).is_none(), "under 4×");
        assert!(long_cost(&turn(0, 250_000, u), 50_000).is_none(), "under 300k");
    }

    #[test]
    fn a_turns_kinds_add_up_to_its_cost() {
        let u = crate::logs::steps::Usage { fresh: 1_000, output: 2_000, read: 300_000, write_5m: 40_000, write_1h: 50_000, ..Default::default() };
        let k = kinds(&turn(0, 0, u)).unwrap();
        assert!((k.iter().sum::<f64>() - prices::cost("claude-opus-5-5", &u).unwrap()).abs() < 1e-9);
        assert!((k[3] - (0.04 * 5.0 + 0.05 * 8.0)).abs() < 1e-9, "writes at their own rates");
        assert!(kinds(&db::StoredTurn { model: "gpt-5".into(), ..turn(0, 0, u) }).is_none());
    }

    #[test]
    fn the_week_starts_on_monday_at_midnight() {
        assert_eq!(slot_of(1, 0), 0);
        assert_eq!(slot_of(0, 23), 167, "Sunday's last hour");
        assert_eq!(slot_of(3, 14), 2 * 24 + 14);
    }

    #[test]
    fn a_climb_keeps_its_peaks_and_its_turn_numbers() {
        assert_eq!(climb(&[10, 20, 30], 60), vec![(0, 10), (1, 20), (2, 30)]);
        let ctx: Vec<u64> = (0..300).map(|i| if i == 157 { 9_999 } else { i }).collect();
        let c = climb(&ctx, 60);
        assert_eq!(c.len(), 60);
        assert!(c.contains(&(157, 9_999)), "the spike survives");
        assert_eq!(c.last(), Some(&(299, 299)));
        assert!(c.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
