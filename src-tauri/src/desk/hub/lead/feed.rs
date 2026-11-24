//! The one sender of each Desk's change feed (desk/feed.rs) to its lead. It runs on Cue's own thread (`later`), after
//! a change that added rows, as the lead's turn ends, and on the finishing tick as a backstop. It never sends while
//! the lead is mid-turn, asking you something, or stopped by a usage limit (a message would bounce off the limit and
//! cancel Cue's carry-on). Each message carries only its rows, numbered; a lead that's new to the feed (a new lead)
//! gets the Desk's summary first, and a parked one is resumed with it, only for a row that wakes at once.

use crate::desk::feed::{self, Change, ChangeKind, Mark, Wake};
use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;

/// A new item `id` on Desk `project`: a row for its lead to route it, while it's still new and held by nobody.
pub(crate) fn new_item(st: &mut Store, project: &str, id: &str, now: u64) {
    let Some(it) = st.desk.item(id).filter(|i| i.project == project && i.state == "new" && i.holder.is_none()) else { return };
    let words = crate::desk::new_item_words(it);
    feed::add(&mut st.desk.feed, project, id, ChangeKind::NewItem, words, "", now);
}

/// A row of a kind raised once per change (by a timer, or a check that runs again): the key of the last one is kept
/// on the item (`fed`), so the same change seen again adds nothing, even after its lead heard it. True when it was
/// added: the item changed, so the caller saves it (its `Effects::items`) with the row.
pub(crate) fn raise(st: &mut Store, project: &str, id: &str, kind: ChangeKind, words: String, key: &str, now: u64) -> bool {
    let Some(it) = st.desk.item_mut(id) else { return false };
    if it.fed.get(kind.as_str()).is_some_and(|k| k == key) {
        return false;
    }
    it.fed.insert(kind.as_str().to_string(), key.to_string());
    feed::add(&mut st.desk.feed, project, id, kind, words, key, now);
    true
}

/// `raise`, on a Desk with a lead only: with no lead, nobody's there to hear it, and a new lead's summary says how
/// things stand.
pub(crate) fn note(st: &mut Store, project: &str, id: &str, kind: ChangeKind, words: String, key: &str, now: u64) -> bool {
    st.desk.lead_sid(project).is_some() && raise(st, project, id, kind, words, key, now)
}

/// Item `id`'s holder asks you `q` (asked at `asked`): a row for its lead, which answers it for you while you're away.
pub(crate) fn needs_you(st: &mut Store, id: &str, q: &str, asked: u64) -> bool {
    let Some(it) = st.desk.item(id) else { return false };
    let who = it.holder.as_ref().map(|h| format!(" {}", h.name)).unwrap_or_default();
    let words = format!("#{id} ({})'s holder{who} asks the owner: \"{}\". It waits on the answer, shown on the item; while Away is on, answer it for the owner.", it.title, q.trim());
    let project = it.project.clone();
    note(st, &project, id, ChangeKind::NeedsYou, words, &asked.to_string(), asked)
}

/// Item `id`'s work clashed with the main line as it landed, and went back to its holder to settle it (#371).
pub(crate) fn clash(st: &mut Store, id: &str, question: &str, now: u64) -> bool {
    let Some(it) = st.desk.item(id) else { return false };
    let Some(h) = it.holder.clone() else { return false };
    let words = format!("#{id} ({}) clashed with the main line as it landed: {question}. It's back with {} to settle; nothing for you unless it can't.", it.title, h.name);
    let (project, key) = (it.project.clone(), format!("{}:{}", h.sid, it.checks.len()));
    note(st, &project, id, ChangeKind::Clash, words, &key, now)
}

/// How far item `id` has finished: "said" (its holder said DONE), "landed", "check" (ready for your check, installed
/// when it needed it). A row for its lead, once per round of each.
pub(crate) fn finished(st: &mut Store, id: &str, how: &str, now: u64) -> bool {
    let Some(it) = st.desk.item(id) else { return false };
    let what = match how {
        "said" => "its holder said DONE; Cue lands it now",
        "landed" => "it landed on the main line",
        _ => "it's ready for the owner's check",
    };
    let words = format!("#{id} ({}) finished: {what}. Nothing to do unless it gets stuck.", it.title);
    let (project, key) = (it.project.clone(), format!("{how}:{}", it.checks.len()));
    note(st, &project, id, ChangeKind::Finished, words, &key, now)
}

/// What you did on item `id` yourself (`act`, from the Desk): a silent row, so its lead's picture is right when it
/// next hears from the Desk. Read before the act (a drop removes the item). A Send back carries your words (`text`, #567).
pub(crate) fn you_did(st: &Store, id: &str, act: &str, text: &str) -> Option<(String, String)> {
    let it = st.desk.item(id)?;
    let t = format!("#{id} ({})", it.title);
    let words = match act {
        "send_back" => format!("The owner sent {t} back to its holder: \"{}\".", crate::desk::your_words(text).0.trim()),
        "ask" => format!("The owner asked {t}'s holder: \"{}\".", crate::desk::your_words(text).0.trim()),
        "start_now" => format!("The owner restarted {t}."),
        "drop" => format!("The owner dropped {t}."),
        "put_before" => format!("The owner put {t} before #{}, which now waits on it.", it.after.clone().unwrap_or_default()),
        _ => return None,
    };
    Some((it.project.clone(), words))
}

/// What the finishing tick raises as time passes, each item's at most once per change (`raise`): finishing stuck on
/// one step, a holder gone or at its usage limit with an item at work. The items raised, to be saved.
pub(crate) fn watch(st: &mut Store, now: u64) -> Vec<String> {
    let found: Vec<(String, String, ChangeKind, String, String)> = st
        .desk
        .items
        .iter()
        .filter(|it| st.desk.lead_sid(&it.project).is_some())
        .filter_map(|it| {
            let (kind, key, words) = landing_stuck(it, now).map(|(k, w)| (ChangeKind::StuckLanding, k, w)).or_else(|| holder_gone(st, it).map(|(k, w)| (ChangeKind::HolderGone, k, w)))?;
            Some((it.project.clone(), it.id.clone(), kind, key, words))
        })
        .collect();
    found.into_iter().filter(|(project, id, kind, key, words)| raise(st, project, id, *kind, words.clone(), key, now)).map(|(_, id, ..)| id).collect()
}

/// Item `it`'s finishing stuck on one step for `LANDING_STUCK_MS` with nothing asked: (its key "step:at", the row's
/// words). Waiting its turn in the landing line (step "line" with no wait), queued for an install, or in review is
/// not stuck.
fn landing_stuck(it: &DeskItem, now: u64) -> Option<(String, String)> {
    let f = it.finish.as_ref().filter(|_| it.state == "finishing")?;
    let moving = matches!(f.step.as_str(), "commit" | "checking" | "landing" | "building" | "installing" | "starting") || (f.step == "line" && !f.wait.is_empty());
    if !moving || !f.question.is_empty() || now.saturating_sub(f.at) < feed::LANDING_STUCK_MS {
        return None;
    }
    let wait = if f.wait.is_empty() { String::new() } else { format!(" ({})", f.wait) };
    let mins = now.saturating_sub(f.at) / 60_000;
    let words = format!("#{} ({}) is stuck finishing: its {} step hasn't moved for {mins} minutes{wait}. Find what holds it and clear it, or ask the owner (ASK: #{} | …).", it.id, it.title, f.step, it.id);
    Some((format!("{}:{}", f.step, f.at), words))
}

/// Item `it` at work with a holder of its own (not crew, not a lead) that ended or hit its usage limit: (its key
/// "sid:why:updated", the row's words).
fn holder_gone(st: &Store, it: &DeskItem) -> Option<(String, String)> {
    let h = it.holder.as_ref().filter(|h| it.state == "working" && crate::desk::hub::core::desk::crew_parts(h).is_none() && h.lead_sid.is_empty())?;
    if st.desk.lead_of(&h.sid).is_some() {
        return None;
    }
    let (why, words) = match st.sessions.get(&h.sid) {
        Some(s) if s.state == "limited" => ("limited", "hit its usage limit with it at work. It carries on once the limit resets; ROUTE it to another session if it can't wait."),
        None if st.ended.iter().any(|(_, s)| s.origin.session_id == h.sid) => ("ended", "ended with it at work. ROUTE it to a session, or to new."),
        _ => return None,
    };
    Some((format!("{}:{why}:{}", h.sid, it.updated), format!("#{} ({})'s holder {} {words}", it.id, it.title, h.name)))
}

/// The rows numbered since the last save, every Desk's, in order: saved with the items they came with.
pub(crate) fn unsaved(st: &mut Store) -> Vec<Change> {
    let mut out = Vec::new();
    for f in st.desk.feed.values_mut() {
        for n in std::mem::take(&mut f.unsaved) {
            if let Some(r) = f.rows.iter().find(|r| r.n == n) {
                out.push(r.clone());
            }
        }
    }
    out
}

/// Whether row `r` still says something true of its item: a stop not settled since, a new item not routed yet.
fn applies(st: &Store, r: &Change) -> bool {
    // Said once and true still: what you did, Away turned on or off (a later one folds it), your pick (its mocks item
    // may have left memory since: it's done), how a full run of the main line ended.
    if matches!(r.kind, ChangeKind::YouDid | ChangeKind::Away | ChangeKind::Picked | ChangeKind::FullRunPassed | ChangeKind::FullRunFailed | ChangeKind::Installed) {
        return true;
    }
    let Some(it) = st.desk.item(&r.item) else { return false };
    match r.kind {
        ChangeKind::NewItem => it.state == "new" && it.holder.is_none(),
        // Not once it's in line for a free copy (#633): it waits for that, it isn't stopped.
        ChangeKind::Stopped => it.state == "working" && !crate::desk::hub::landing::trees::in_line(it) && it.stop.is_some_and(|(u, _)| u.to_string() == r.key),
        ChangeKind::StuckLanding => match r.key.strip_prefix("checks:") {
            Some(n) => it.checks.len().to_string() == n,
            None => it.state == "finishing" && it.finish.as_ref().is_some_and(|f| format!("{}:{}", f.step, f.at) == r.key),
        },
        ChangeKind::PermissionStop => st.items.iter().any(|c| c.id == r.key && c.status == "pending"),
        ChangeKind::RouteFailed => match r.key.strip_prefix("route:") {
            Some(to) => it.route_failed == to,
            None => !it.start_failed.is_empty(),
        },
        ChangeKind::HolderGone => holder_gone(st, it).is_some_and(|(k, _)| k == r.key),
        // Not answered since.
        ChangeKind::NeedsYou => it.asks.as_ref().is_some_and(|(_, at)| at.to_string() == r.key),
        // Still at work with the holder it went back to.
        ChangeKind::Clash => it.state == "working" && it.holder.as_ref().is_some_and(|h| r.key.rsplit_once(':').is_some_and(|(sid, _)| sid == h.sid)),
        // Still waiting for its copy (once it has one, its holder is at work with the words).
        ChangeKind::WordsWait => it.tree_wait,
        // Still on its machine, not landed yet.
        ChangeKind::Machine => it.remote.is_some() && matches!(it.state.as_str(), "working" | "finishing"),
        // Not sent back to work since.
        ChangeKind::Finished => matches!(it.state.as_str(), "finishing" | "to_check" | "done"),
        ChangeKind::YouDid | ChangeKind::Away | ChangeKind::Picked | ChangeKind::FullRunPassed | ChangeKind::FullRunFailed | ChangeKind::Installed => true,
    }
}

/// Where Desk `desk`'s lead stands for the feed.
enum Lead {
    /// Free: at its prompt (its "your turn" card doesn't count), nothing asked of you, no usage limit.
    Free(String),
    /// Parked (its session gone): resumed only for a row that wakes at once.
    Parked(String),
    /// Mid-turn, asking you something, limited, being started, or stopped by you (it isn't woken: rows wait until
    /// it's live again): rows wait for its next turn's end or the tick.
    Busy,
}

fn lead_of(st: &Store, desk: &str) -> Option<Lead> {
    let sid = st.desk.lead_sid(desk)?.to_string();
    if st.desk.run.starting.contains_key(desk) {
        return Some(Lead::Busy);
    }
    let Some(s) = st.sessions.get(&sid) else { return Some(Lead::Parked(sid)) };
    let asking = s.state == "deciding" || st.items.iter().any(|c| c.status == "pending" && c.kind != "waiting" && c.origin.session_id == sid);
    let free = matches!(s.state.as_str(), "" | "idle" | "waiting") && s.held.is_none() && s.queued.is_none();
    Some(if s.state == "limited" || asking || !free { Lead::Busy } else { Lead::Free(sid) })
}

/// Desk `desk`'s sent rows were kept for its lead's turn end, and that kept message is gone without going (taken back,
/// its session ended or resumed afresh): sent again from "heard". A kept message that goes leaves in the same write
/// as `kept_went`, so one gone here never went.
fn kept_check(st: &mut Store, desk: &str) {
    let Some(f) = st.desk.feed.get(desk).filter(|f| f.sent_kept) else { return };
    let still = st.sessions.held(&f.sent.lead).is_some_and(|h| h.text.contains(feed::NUMBERS));
    if !still {
        if let Some(f) = st.desk.feed.get_mut(desk) {
            f.kept_dropped();
        }
    }
}

/// The message kept for session `sid` (`text`) went to it, in this write: on each Desk it leads where rows sent were
/// kept in it, the turn it starts is the one that hears them.
pub(crate) fn kept_went(st: &mut Store, sid: &str, text: &str) {
    if !text.contains(feed::NUMBERS) {
        return;
    }
    let turn = st.sessions.turn_ends(sid);
    for f in st.desk.feed.values_mut().filter(|f| f.sent.lead == sid && f.sent.n > f.heard.n) {
        f.kept_went(turn);
    }
}

/// What the sender hands over once the store is let go.
struct Go {
    lead: String,
    parked: bool,
    /// A new lead (the feed went to another before): the Desk's summary first.
    summary: bool,
    text: String,
    told: Vec<DeskItem>,
    marks: (Mark, Mark),
}

impl Hub {
    /// Each Desk with rows past its marks: its lead sent them, as it's free to hear them.
    pub(crate) fn desk_feed_send(self: &Arc<Self>) {
        let desks: Vec<String> = self.read(|st| st.desk.feed.iter().filter(|(_, f)| !f.rows.is_empty()).map(|(d, _)| d.clone()).collect());
        for desk in desks {
            self.desk_feed_send_to(&desk);
        }
    }

    /// Rows numbered but not saved yet (none, once every change goes through `desk_done`): saved now, in order.
    fn desk_feed_flush(&self) {
        self.desk_saving(|| {
            let rows = self.write(unsaved);
            if let Err(e) = crate::db::desk_save(&[], &[], &rows) {
                eprintln!("cue: desk: couldn't save {} change(s): {e}", rows.len());
            }
        });
    }

    /// Desk `desk`'s rows its lead is free to hear, sent. One send at a time per Desk (`Feed::sending`): one that
    /// finds another under way leaves its rows to it, and it looks again once done.
    fn desk_feed_send_to(self: &Arc<Self>, desk: &str) {
        self.desk_feed_flush();
        let now = now_ms();
        let go = self.write(|st| feed_go(st, desk, now));
        let Some(go) = go else { return };
        self.desk_feed_save(desk, &go.told, &go.marks);
        // Only the marks moved (nothing left that applies).
        if go.text.is_empty() {
            return;
        }
        let sent = self.desk_feed_tell(desk, &go);
        let (marks, again) = self.write(|st| sent_marks(st, desk, &go.lead, &sent));
        if let Err(e) = &sent {
            eprintln!("cue: desk: {desk}'s lead wasn't sent its Desk changes: {e}");
        }
        if let Some(m) = marks {
            self.desk_feed_save(desk, &[], &m);
        }
        if again {
            self.later(0, |h| h.desk_feed_send());
        }
    }

    /// The message of `go` to Desk `desk`'s lead: what sending it said.
    fn desk_feed_tell(self: &Arc<Self>, desk: &str, go: &Go) -> Result<String, String> {
        if go.parked {
            // Resumed with the Desk's summary, then its rows.
            return super::send::desk_send_cue(self, desk, &go.text, crate::sessions::Kind::Desk);
        }
        // A lead that took over from one the feed went to hears the Desk's summary first, then the rows that still
        // apply. A Desk's first lead was given the summary as it started.
        let text = if go.summary {
            let (summary, failed) = self.desk_block(desk, &go.lead, &go.text, false, false);
            self.desk_told(&failed);
            super::send::desk_message(&summary, &go.text)
        } else {
            go.text.clone()
        };
        self.tell(&go.lead, &text, crate::sessions::Kind::Desk)
    }

    fn desk_feed_save(&self, desk: &str, items: &[DeskItem], (sent, heard): &(Mark, Mark)) {
        self.desk_saving(|| {
            if let Err(e) = crate::db::desk_save(items, &[], &[]) {
                eprintln!("cue: desk: couldn't save the items {desk}'s lead was told of: {e}");
            }
            if let Err(e) = crate::db::desk_feed_marks(desk, sent, heard) {
                eprintln!("cue: desk: couldn't save {desk}'s feed marks: {e}");
            }
        });
    }

    /// Session `sid`'s turn ended cleanly: on each Desk it leads, what was sent to it before that turn is heard, and
    /// rows that came meanwhile go now that it's free.
    pub(crate) fn desk_feed_turn_ended(&self, sid: &str) {
        let (heard, any): (Vec<(String, Mark, Mark)>, bool) = self.write(|st| {
            let desks: Vec<String> = st.desk.lead.iter().filter(|(_, l)| l.sid == sid).map(|(d, _)| d.clone()).collect();
            let mut heard = Vec::new();
            let mut any = false;
            let turn = st.sessions.turn_ends(sid);
            for d in desks {
                kept_check(st, &d);
                let Some(f) = st.desk.feed.get_mut(&d) else { continue };
                if f.heard_by(sid, turn) {
                    heard.push((d, f.sent.clone(), f.heard.clone()));
                }
                any |= !f.rows.is_empty();
            }
            (heard, any)
        });
        for (d, sent, heard) in heard {
            self.desk_feed_save(&d, &[], &(sent, heard));
        }
        if any {
            self.later(0, |h| h.desk_feed_send());
        }
    }

    /// How far item `id` has finished (`finished`), raised outside a change: saved with the item.
    pub(crate) fn desk_finished(&self, id: &str, how: &str) {
        let now = now_ms();
        if self.write(|st| finished(st, id, how, now)) {
            self.desk_done(crate::desk::hub::core::desk::Effects { items: vec![id.to_string()], ..Default::default() });
        }
        if how == "landed" {
            // Cue's own main line moved: its full run is due (desk/hub/landing/full_run.rs).
            self.later(0, |h| h.full_run_now());
        }
    }

    /// Away turned on or off on Desk `desk` (`words`, what its lead is to know): a row of the Desk's own, in order with
    /// the rest, while it has a lead.
    pub(crate) fn desk_feed_away(&self, desk: &str, words: &str) {
        self.desk_feed_line(desk, ChangeKind::Away, words, &now_ms().to_string());
    }

    /// A row of Desk `desk`'s own (about no item), in order with the rest, while it has a lead; `key`: what makes a
    /// repeat of it the same row.
    pub(crate) fn desk_feed_line(&self, desk: &str, kind: ChangeKind, words: &str, key: &str) {
        let now = now_ms();
        let added = self.write(|st| st.desk.lead_sid(desk).is_some() && feed::add(&mut st.desk.feed, desk, "", kind, words.to_string(), key, now).is_some());
        if added {
            self.desk_feed_flush();
            self.later(0, |h| h.desk_feed_send());
        }
    }

    /// The finishing tick's look for what time raises (`watch`): its rows saved with their items.
    pub(crate) fn desk_feed_watch(&self) {
        let now = now_ms();
        let items = self.write(|st| watch(st, now));
        if !items.is_empty() {
            self.desk_done(crate::desk::hub::core::desk::Effects { items, ..Default::default() });
        }
    }

    /// The finishing tick's backstop: rows past "sent" anywhere, so a missed wake-up costs at most a tick.
    pub(crate) fn desk_feed_tick(&self) {
        let due = self.read(|st| st.desk.feed.iter().any(|(d, f)| st.desk.lead_sid(d).is_some_and(|l| f.rows.iter().any(|r| r.n > f.from_for(l)))));
        if due {
            self.later(0, |h| h.desk_feed_send());
        }
    }
}

/// Under the store's lock: Desk `desk`'s rows its lead is to be sent now, its marks moved to them and the stops in
/// them marked told (no text: only the marks moved, past rows that no longer apply); None when nothing goes (no rows, the lead not free, nothing that wakes it yet).
fn feed_go(st: &mut Store, desk: &str, now: u64) -> Option<Go> {
    let (lead, parked) = match lead_of(st, desk)? {
        Lead::Free(sid) => (sid, false),
        Lead::Parked(sid) => (sid, true),
        Lead::Busy => return None,
    };
    kept_check(st, desk);
    let f = st.desk.feed.get(desk).filter(|f| !f.sending)?;
    let from = f.from_for(&lead);
    let pending: Vec<Change> = f.rows.iter().filter(|r| r.n > from).cloned().collect();
    let last = pending.last()?.n;
    let live: Vec<&Change> = feed::still_apply(&pending, |r| applies(st, r));
    // Nothing left that applies: the marks move past them, with nothing to send or hear.
    if live.is_empty() {
        let f = st.desk.feed.get_mut(desk)?;
        let nothing_out = f.sent.n <= f.heard.n || f.sent.lead != lead;
        f.sent = Mark { n: last, lead: lead.clone() };
        if nothing_out {
            f.heard = f.sent.clone();
            f.rows.retain(|r| r.n > last);
        }
        return Some(Go { lead, parked, summary: false, text: String::new(), told: Vec::new(), marks: (f.sent.clone(), f.heard.clone()) });
    }
    let away = crate::desk::hub::away::away::away_in_force(st, desk);
    if !feed::wakes(&live, now, away) || (parked && !live.iter().any(|r| r.kind.wake_in(away) == Wake::AtOnce)) {
        return None;
    }
    // What to do with the new items among them is said once, after them all.
    let news: Vec<&DeskItem> = live.iter().filter(|r| r.kind == ChangeKind::NewItem).filter_map(|r| st.desk.item(&r.item)).collect();
    let tail = if news.is_empty() { String::new() } else { crate::desk::new_items_tail(&news) };
    let text = feed::message(&live, &tail);
    let stops: Vec<(String, u64)> = live.iter().filter(|r| r.kind == ChangeKind::Stopped).filter_map(|r| Some((r.item.clone(), r.key.parse().ok()?))).collect();
    let mut told = Vec::new();
    for (id, updated) in stops {
        if let Some(it) = st.desk.item_mut(&id) {
            it.told = Some(updated);
            told.push(it.clone());
        }
    }
    let f = st.desk.feed.get_mut(desk)?;
    let summary = [&f.sent.lead, &f.heard.lead].iter().any(|l| !l.is_empty()) && f.sent.lead != lead && f.heard.lead != lead;
    f.sent = Mark { n: last, lead: lead.clone() };
    (f.sent_turn, f.sent_kept, f.sending) = (st.sessions.turn_ends(&lead), false, true);
    Some(Go { lead, parked, summary, text, told, marks: (f.sent.clone(), f.heard.clone()) })
}

/// Under the store's lock, once Desk `desk`'s send to `lead` said `sent`: its send is over, and its marks follow what
/// happened (kept in Cue for the lead's turn end while still kept, or not sent: tried again from "heard"). The marks
/// to save, if they moved, and whether rows came meanwhile (they go now, once this one went).
fn sent_marks(st: &mut Store, desk: &str, lead: &str, sent: &Result<String, String>) -> (Option<(Mark, Mark)>, bool) {
    let held = st.sessions.held(lead).is_some_and(|h| h.text.contains(feed::NUMBERS));
    let Some(f) = st.desk.feed.get_mut(desk) else { return (None, false) };
    f.sending = false;
    let again = f.rows.iter().any(|r| r.n > f.sent.n);
    if f.sent.lead != lead {
        return (None, again);
    }
    match sent {
        // Kept, and not gone already (it left in the same write as `kept_went`).
        Ok(said) => {
            f.sent_kept = crate::hub::team::kept(said) && held;
            (None, again)
        }
        // Not tried again at once (the tick, the lead's next turn end): it may fail the same way.
        Err(_) => {
            f.sent = f.heard.clone();
            (Some((f.sent.clone(), f.heard.clone())), false)
        }
    }
}
