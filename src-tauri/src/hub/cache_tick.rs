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
/// so not due); 0: neither.
fn ahead_by(ticked: bool, all: bool, tokens: u64, start: u64, window: u64) -> u8 {
    let window = if window > 0 { window } else { AHEAD_WINDOW };
    let due = start > 0 && tokens as f64 >= window as f64 * AHEAD_SHARE && tokens as f64 >= start as f64 * AHEAD_GROWTH;
    if ticked { crate::ipc::AHEAD_SESSION } else if all && due { crate::ipc::AHEAD_ALL } else { 0 }
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
        let waiting: Vec<(Item, bool)> = self.read(|st| {
            st.items
                .iter()
                .filter(|i| i.status == "pending" && i.origin.machine.is_empty() && !i.origin.transcript_path.is_empty() && matches!(i.origin.harness.as_str(), "claude" | "codex"))
                .map(|i| (i.clone(), st.sessions.is_later(&i.origin.session_id)))
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
        st.ahead_tried.retain(|_, until| *until > now);
        let ahead: Vec<Ahead> = scan.ahead.into_iter().filter(|a| st.ahead_tried.insert(a.origin.session_id.clone(), a.until) != Some(a.until)).collect();
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

/// Each waiting card's log read (`later`: the card is put off for later): its numbers, and whether its cache
/// is due a heads-up or a note ahead.
fn scan_caches(waiting: Vec<(Item, bool)>, now: u64) -> CacheScan {
    let mut scan = CacheScan::default();
    let mut watched: Watched = None;
    for (it, later) in waiting {
        let Some(state) = crate::steps::insight_state(&it.origin.transcript_path) else { continue };
        scan.insight.insert(it.origin.session_id.clone(), serde_json::json!(state));
        // Codex logs don't say how long they cache: no heads-up.
        if state.until_ms == 0 {
            continue;
        }
        if let Some(by) = ahead_wanted(&it, &state, now, &mut watched) {
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
fn ahead_wanted(it: &Item, state: &InsightState, now: u64, watched: &mut Watched) -> Option<u8> {
    let sid = &it.origin.session_id;
    if it.kind != "waiting" || it.origin.harness != "claude" || !ahead_due(state.until_ms, state.last_ms, state.tokens, now) {
        return None;
    }
    let (ticked, all) = watched.get_or_insert_with(|| (crate::db::ahead_watched(), crate::config::flag("/insights/ahead")));
    let by = ahead_by(ticked.contains(sid), *all, state.tokens, state.start, state.window);
    // Not when a hand-off is already on its way or written (ahead, or one you sent this stretch), nor
    // once it has continued in a fresh session (a second note would write over that link), nor when
    // Claude Code compacts it itself before the cache runs out (config::idle_compacts).
    let handing = crate::db::handoff(sid).is_some_and(|k| !k.next.is_empty() || k.ahead > 0 || k.sent_ms >= state.last_ms);
    (by > 0 && !handing && !crate::config::idle_compacts(state.tokens)).then_some(by)
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
        assert_eq!(super::ahead_by(true, false, 60_000, 50_000, 1_000_000), crate::ipc::AHEAD_SESSION);
        assert_eq!(super::ahead_by(false, true, 650_000, 45_000, 1_000_000), crate::ipc::AHEAD_ALL);
        assert_eq!(super::ahead_by(false, true, 130_000, 40_000, 0), crate::ipc::AHEAD_ALL, "a 200k window when it doesn't say");
        assert_eq!(super::ahead_by(false, true, 173_000, 48_000, 1_000_000), 0, "17% of its window: started from a note an hour ago");
        assert_eq!(super::ahead_by(false, true, 700_000, 400_000, 1_000_000), 0, "hasn't grown 3× from its start");
        assert_eq!(super::ahead_by(false, true, 700_000, 0, 1_000_000), 0, "its start not known");
        assert_eq!(super::ahead_by(false, false, 900_000, 45_000, 1_000_000), 0, "off unless you turned it on");
    }
}
