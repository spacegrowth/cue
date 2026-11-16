//! The board's cards' cache and context numbers, read from the end of their logs every ~10 s and as a turn
//! ends: kept for the window, a heads-up before a waiting session's cache runs out, and a hand-off note asked
//! for ahead of it (hand_off.rs).

use super::*;
use super::asks::notify_title;
use crate::steps::InsightState;

/// How long before a waiting session's cache runs out Cue gives you a heads-up: an hour's cache in its last
/// 10 minutes (a 5-minute one gets none: it runs out while you read); and the least context worth one.
/// insights.js cacheWarn counts the card down by the same rule (CACHE_WARN_MS, CACHE_MIN_TOKENS).
const COLD_WARN_MS: u64 = 10 * 60_000;
const COLD_MIN_TOKENS: u64 = 50_000;

/// A hand-off note asked for before a cache runs out (a session's "Hand off if I don't come back", or Insights for
/// every session that's due one): this long before it does, an hour's cache only (a 5-minute one runs out while
/// you're still reading; its Write hand-off now is yours to press). Insights' every-session note goes only where a
/// fresh session is due anyway, as insights.js handoffDue offers one (its HANDOFF): most of its window used and
/// grown several times from its start; a session that started small from a note an hour ago isn't (one you ticked
/// goes from COLD_MIN_TOKENS).
const AHEAD_LEAD_MS: u64 = 90_000;
const AHEAD_TTL_MS: u64 = 30 * 60_000;
const AHEAD_SHARE: f64 = 0.6;
const AHEAD_GROWTH: f64 = 3.0;
const AHEAD_WINDOW: u64 = 200_000;

/// Whether a waiting session's cache (running out at `until`, its last reply at `last`, `tokens` of context)
/// is in the minute and a half a hand-off note goes ahead of: an hour's cache, big enough to matter.
fn ahead_due(until: u64, last: u64, tokens: u64, now: u64) -> bool {
    until > now && until - now <= AHEAD_LEAD_MS && until.saturating_sub(last) >= AHEAD_TTL_MS && tokens >= COLD_MIN_TOKENS
}

/// Who a due note is asked for by (ipc AHEAD_*): its session's own tick, else Insights for a session a fresh
/// session is due for (`tokens` of a `window`, 0: 200k; `start`: its context at its first reply, 0: not known,
/// so not due), else the Desk's rule for a session of a project whose Desk has a live lead (`desk`: its part
/// there; None: no such lead, or the lead said to keep its context: desk_due); 0: none of them.
fn ahead_by(ticked: bool, all: bool, desk: Option<&DeskRole>, tokens: u64, start: u64, window: u64) -> u8 {
    let window = if window > 0 { window } else { AHEAD_WINDOW };
    let due = start > 0 && tokens as f64 >= window as f64 * AHEAD_SHARE && tokens as f64 >= start as f64 * AHEAD_GROWTH;
    if ticked {
        crate::ipc::AHEAD_SESSION
    } else if all && due {
        crate::ipc::AHEAD_ALL
    } else if desk.is_some_and(|r| desk_due(r, tokens)) {
        crate::ipc::AHEAD_DESK
    } else {
        0
    }
}

/// The Desk's rule's bands: from DESK_LEAVE Claude Code compacts a waiting session itself (config::idle_compacts);
/// from DESK_HELD a note pays for a session holding an item not done, or the lead; below it only for one holding
/// an item at work; below COLD_MIN_TOKENS never.
const DESK_LEAVE: u64 = 200_000;
const DESK_HELD: u64 = 100_000;

/// A session's part on its project's Desk, which has a live lead (`desk_role`): the lead itself, holding an item
/// not done (`open`), one at work (`working`), and when the lead last said to keep its context (0: it didn't;
/// u64::MAX: said while it worked, so it holds through the idle stretch after that turn).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DeskRole {
    pub(super) lead: bool,
    pub(super) open: bool,
    pub(super) working: bool,
    pub(super) kept_at: u64,
}

impl DeskRole {
    /// Whether the lead's keep holds for the idle stretch since `last` (its latest reply): it said so since.
    pub(super) fn kept(&self, last: u64) -> bool {
        self.kept_at > 0 && self.kept_at >= last
    }
}

/// Whether the Desk's rule asks a session with `tokens` of context for a note ahead of its cache running out.
pub(super) fn desk_due(role: &DeskRole, tokens: u64) -> bool {
    match tokens {
        t if t >= DESK_LEAVE || t < COLD_MIN_TOKENS => false,
        t if t >= DESK_HELD => role.open || role.lead,
        _ => role.working,
    }
}

/// Session `sid`'s part on its project's Desk, when that Desk's lead is live (not parked, not ended); None else,
/// and then the Desk changes nothing.
pub(super) fn desk_role(st: &Store, sid: &str) -> Option<DeskRole> {
    let project = st.sessions.get(sid).map(|s| s.project.as_str()).filter(|p| !p.is_empty())?;
    let lead = st.desk.lead_sid(project)?;
    st.sessions.get(lead)?;
    let kept_at = st.desk.keep.get(sid).map_or(0, |k| if k.2 { u64::MAX } else { k.0 });
    let mut role = DeskRole { lead: lead == sid, kept_at, ..Default::default() };
    let held = |i: &&crate::desk::DeskItem| i.project == project && i.state != "done" && i.holder.as_ref().is_some_and(|h| h.is(sid));
    for it in st.desk.items.iter().filter(held) {
        role.open = true;
        role.working |= it.state == "working";
    }
    Some(role)
}

/// Whether Cue will ask session `sid` (its card's numbers: `insight`) for a note as its cache runs out, as things
/// stand (the lead's summary says it will): who the scan found would ask (its "ahead_by", by `ahead_would`, the
/// same test the ask itself goes by; 0 once asked), its cache still warm, and no keep said since for the Desk's rule.
pub(super) fn desk_will_hand_off(st: &Store, sid: &str, insight: &Value, now: u64) -> bool {
    let n = |k: &str| insight[k].as_u64().unwrap_or(0);
    let by = n("ahead_by");
    if by == 0 || n("until_ms") <= now {
        return false;
    }
    by != crate::ipc::AHEAD_DESK as u64 || desk_role(st, sid).is_some_and(|r| !r.kept(n("last_ms")))
}

/// A hand-off note due ahead of a cache running out at `until`, asked for `by` its tick or Insights.
pub(super) struct Ahead {
    pub(super) origin: Origin,
    until: u64,
    pub(super) tokens: u64,
    pub(super) by: u8,
}

/// What the waiting cards' logs say (see `Hub::cache_tick`): each card's numbers, the cards whose cache runs
/// out soon (and when), and the notes due ahead of one.
#[derive(Default)]
struct CacheScan {
    insight: HashMap<String, Value>,
    cold: Vec<(Item, u64)>,
    ahead: Vec<Ahead>,
}

/// Whether Insights' notes are on, and the sessions you ticked: asked only once one is due, so most ticks
/// don't read the database.
type Watched = Option<(Vec<String>, bool)>;

impl Hub {
    /// The board's cards' insight numbers (their cache's expiry and their context, from the end of their
    /// logs: steps::insight_state), kept for the state the window gets, and the heads-up before a waiting
    /// session's cache runs out. Off the main thread: every ~10 s and as a turn ends. Cards on this Mac
    /// only: another machine's log is never looked up from here (the window asks for those itself).
    pub(super) fn cache_tick(&self) {
        let now = now_ms();
        let waiting: Vec<Waiting> = self.read(|st| {
            st.items
                .iter()
                .filter(|i| i.status == "pending" && i.origin.machine.is_empty() && !i.origin.transcript_path.is_empty() && matches!(i.origin.harness.as_str(), "claude" | "codex"))
                .map(|i| (i.clone(), st.sessions.is_later(&i.origin.session_id), desk_role(st, &i.origin.session_id)))
                .collect()
        });
        let scan = scan_caches(waiting, now);
        let (cold, ahead) = self.write(|st| Self::not_told_yet(st, scan, now));
        self.hand_off_due(ahead);
        self.warn_cold(cold, now);
    }

    /// The scan kept for the window, and of what it found, what wasn't told already: a heads-up once per expiry,
    /// a note asked for once per idle stretch.
    fn not_told_yet(st: &mut Store, scan: CacheScan, now: u64) -> (Vec<(Item, u64)>, Vec<Ahead>) {
        st.insight = scan.insight;
        // A keep is for one idle stretch, and no cache outlasts an hour of it.
        st.desk.keep.retain(|_, k| now.saturating_sub(k.0) < 4 * AHEAD_TTL_MS);
        st.ahead_tried.retain(|_, until| *until > now);
        let ahead: Vec<Ahead> = scan.ahead.into_iter().filter(|a| st.ahead_tried.insert(a.origin.session_id.clone(), a.until) != Some(a.until)).collect();
        // Asked already this expiry (or held from it): no other ask comes before it runs out.
        for (sid, v) in st.insight.iter_mut() {
            if v["ahead_by"].as_u64().unwrap_or(0) > 0 && st.ahead_tried.get(sid).is_some_and(|u| Some(*u) == v["until_ms"].as_u64()) {
                v["ahead_by"] = 0.into();
            }
        }
        st.cold_warned.retain(|_, until| *until > now);
        let cold = scan.cold.into_iter().filter(|(it, until)| st.cold_warned.insert(it.origin.session_id.clone(), *until) != Some(*until)).collect();
        (cold, ahead)
    }

    /// Ask for each due note off this tick's thread (typing waits for the session to take it); a hub without
    /// the app (tests) does it here.
    fn hand_off_due(&self, ahead: Vec<Ahead>) {
        match self.shared() {
            Some(h) if !ahead.is_empty() => {
                std::thread::spawn(move || {
                    for a in ahead {
                        h.hand_off_ahead(&a.origin, a.tokens, a.by);
                    }
                });
            }
            Some(_) => {}
            None => {
                for a in ahead {
                    self.hand_off_ahead(&a.origin, a.tokens, a.by);
                }
            }
        }
    }

    /// The heads-up that a waiting card's cache runs out at `until`, in place of the card's own notification
    /// (Reply on it still goes to the session).
    fn warn_cold(&self, cold: Vec<(Item, u64)>, now: u64) {
        if !crate::config::flag("/notify/cold") {
            return;
        }
        for (it, until) in cold {
            let project = crate::config::moved().remove(&it.origin.session_id).unwrap_or_else(|| it.project.clone());
            let mins = (until - now).div_ceil(60_000);
            let (_, what) = notify_title(&it);
            self.notify_unmuted(&it.id, &format!("{project} · cache runs out in {mins}m"), &what, if it.kind == "waiting" { "turn" } else { "" });
        }
    }
}

/// A waiting card, whether it's put off for later, and its session's part on its project's Desk (desk_role).
type Waiting = (Item, bool, Option<DeskRole>);

/// Each waiting card's log read (`later`: the card is put off for later): its numbers, and whether its cache
/// is due a heads-up or a note ahead.
fn scan_caches(waiting: Vec<Waiting>, now: u64) -> CacheScan {
    let mut scan = CacheScan::default();
    let mut watched: Watched = None;
    for (it, later, desk) in waiting {
        let Some(state) = crate::steps::insight_state(&it.origin.transcript_path) else { continue };
        let mut numbers = serde_json::json!(state);
        // For its Desk's lead (desk_lead.rs): who would ask it for a note as its cache runs out, a keep aside (the
        // lead may take one back before then; desk_will_hand_off looks at it then).
        if let Some(r) = desk {
            numbers["ahead_by"] = ahead_would(&it, &state, Some(DeskRole { kept_at: 0, ..r }), now, &mut watched).unwrap_or(0).into();
        }
        scan.insight.insert(it.origin.session_id.clone(), numbers);
        // Codex logs don't say how long they cache: no heads-up.
        if state.until_ms == 0 {
            continue;
        }
        if let Some(by) = ahead_wanted(&it, &state, desk, now, &mut watched) {
            scan.ahead.push(Ahead { origin: it.origin.clone(), until: state.until_ms, tokens: state.tokens, by });
        }
        if cold_soon(&it.origin.session_id, &state, later, now) {
            scan.cold.push((it, state.until_ms));
        }
    }
    scan
}

/// Who a note ahead of this card's cache running out is due by, if one is. Put off for later too: leaving it for
/// a while is when a note pays.
fn ahead_wanted(it: &Item, state: &InsightState, desk: Option<DeskRole>, now: u64, watched: &mut Watched) -> Option<u8> {
    if !ahead_due(state.until_ms, state.last_ms, state.tokens, now) {
        return None;
    }
    ahead_would(it, state, desk, now, watched)
}

/// Who a note would be asked by as this card's cache runs out, when aside (ahead_due says when): a waiting Claude
/// Code card with an hour's cache still warm, big enough to matter, and none of the reasons not to.
fn ahead_would(it: &Item, state: &InsightState, desk: Option<DeskRole>, now: u64, watched: &mut Watched) -> Option<u8> {
    let sid = &it.origin.session_id;
    let (until, last) = (state.until_ms, state.last_ms);
    if it.kind != "waiting" || it.origin.harness != "claude" || until <= now || until.saturating_sub(last) < AHEAD_TTL_MS || state.tokens < COLD_MIN_TOKENS {
        return None;
    }
    let (ticked, all) = watched.get_or_insert_with(|| (crate::db::ahead_watched(), crate::config::flag("/insights/ahead")));
    // The lead said to keep this one's context for this idle stretch: the Desk's rule leaves it be.
    let desk = desk.filter(|r| !r.kept(last));
    let by = ahead_by(ticked.contains(sid), *all, desk.as_ref(), state.tokens, state.start, state.window);
    if by == 0 {
        return None;
    }
    // Not when a hand-off is already on its way or written (ahead, or one you sent this stretch), nor
    // once it has continued in a fresh session (a second note would write over that link), nor when
    // Claude Code compacts it itself before the cache runs out (config::idle_compacts).
    let handing = crate::db::handoff(sid).is_some_and(|k| !k.next.is_empty() || k.ahead > 0 || k.sent_ms >= last);
    (!handing && !crate::config::idle_compacts(state.tokens)).then_some(by)
}

/// Whether a card's cache is in its last minutes, big enough to warn about: an hour's cache (a 5-minute one
/// gets none), not put off for later, and no note written ahead waiting for you (its card says so, and its
/// renewed cache needs no heads-up).
fn cold_soon(sid: &str, state: &InsightState, later: bool, now: u64) -> bool {
    let (until, last) = (state.until_ms, state.last_ms);
    if until.saturating_sub(last) <= COLD_WARN_MS || until <= now || until - now > COLD_WARN_MS || state.tokens < COLD_MIN_TOKENS || later {
        return false;
    }
    !crate::db::handoff(sid).is_some_and(|k| k.ahead > 0 && k.next.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_note_goes_ahead_only_in_an_hour_caches_last_minute_and_a_half() {
        let (now, hour, min) = (10_000_000_000u64, 3_600_000u64, 60_000u64);
        // Its last reply an hour before its cache runs out (an hour's cache), 80 s left: due.
        assert!(super::ahead_due(now + 80_000, now + 80_000 - hour, 300_000, now));
        assert!(!super::ahead_due(now + 3 * min, now + 3 * min - hour, 300_000, now), "not yet: 3 minutes left");
        assert!(!super::ahead_due(now - 1, now - 1 - hour, 300_000, now), "already gone");
        assert!(!super::ahead_due(now + 80_000, now + 80_000 - 5 * min, 300_000, now), "a 5-minute cache: the button only");
        assert!(!super::ahead_due(now + 80_000, now + 80_000 - hour, 40_000, now), "too small to matter");
        // Its own tick goes from any size worth a note; Insights' every-session only where a fresh session is due.
        assert_eq!(super::ahead_by(true, false, None, 60_000, 50_000, 1_000_000), crate::ipc::AHEAD_SESSION);
        assert_eq!(super::ahead_by(false, true, None, 650_000, 45_000, 1_000_000), crate::ipc::AHEAD_ALL);
        assert_eq!(super::ahead_by(false, true, None, 130_000, 40_000, 0), crate::ipc::AHEAD_ALL, "a 200k window when it doesn't say");
        assert_eq!(super::ahead_by(false, true, None, 173_000, 48_000, 1_000_000), 0, "17% of its window: started from a note an hour ago");
        assert_eq!(super::ahead_by(false, true, None, 700_000, 400_000, 1_000_000), 0, "hasn't grown 3× from its start");
        assert_eq!(super::ahead_by(false, true, None, 700_000, 0, 1_000_000), 0, "its start not known");
        assert_eq!(super::ahead_by(false, false, None, 900_000, 45_000, 1_000_000), 0, "off unless you turned it on");
    }

    #[test]
    fn the_desks_rule_goes_by_context_and_what_the_session_holds() {
        use super::{ahead_by, DeskRole};
        use crate::ipc::{AHEAD_DESK, AHEAD_SESSION};
        let none = DeskRole::default();
        let open = DeskRole { open: true, ..none };
        let working = DeskRole { open: true, working: true, ..none };
        let lead = DeskRole { lead: true, ..none };
        let by = |r: &DeskRole, tokens: u64| ahead_by(false, false, Some(r), tokens, 0, 0);
        // 200k and up: Claude Code compacts it itself.
        for r in [&none, &open, &working, &lead] {
            assert_eq!(by(r, 200_000), 0);
            assert_eq!(by(r, 450_000), 0);
        }
        // 100k–200k: holding an item not done, or the lead.
        assert_eq!((by(&none, 150_000), by(&open, 150_000), by(&working, 100_000), by(&lead, 199_000)), (0, AHEAD_DESK, AHEAD_DESK, AHEAD_DESK));
        // 50k–100k: only holding one at work.
        assert_eq!((by(&none, 80_000), by(&open, 80_000), by(&lead, 80_000), by(&working, 50_000)), (0, 0, 0, AHEAD_DESK));
        // Under 50k: never.
        assert_eq!(by(&working, 49_000), 0);
        // No lead (None): as before; your tick still comes first.
        assert_eq!(ahead_by(false, false, None, 150_000, 0, 0), 0);
        assert_eq!(ahead_by(true, false, Some(&working), 80_000, 0, 0), AHEAD_SESSION);
        // A keep holds for the idle stretch it was said in: until the session's next reply.
        let kept = DeskRole { kept_at: 5_000, ..working };
        assert!(kept.kept(4_000) && kept.kept(5_000) && !kept.kept(6_000) && !working.kept(0));
    }
}
