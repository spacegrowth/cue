//! Each Desk's change feed (docs/desk-change-feed-plan.md, option B): one numbered list of Desk changes for its lead,
//! saved in the same write as the change (db/desk.rs `desk_save`), with two saved marks: "sent" up to here and
//! "heard" up to here, each with the lead it belongs to. One sender gives the lead the rows past them in order
//! (desk/hub/lead/feed.rs). Rows are notices, not state: each is checked against its item before it goes.

use std::collections::HashMap;

/// What kind of change a row is. A fixed list, so which ones wake the lead is checked at build time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    /// A new item to route.
    NewItem,
    /// Its holder stopped on it with no DONE and nothing to ask you: the lead settles it.
    Stopped,
    /// Its finishing hasn't moved for `LANDING_STUCK_MS` (waiting its turn in the landing line doesn't count), or its
    /// check failed twice.
    StuckLanding,
    /// Its holder has sat on a permission prompt nobody answered.
    PermissionStop,
    /// You picked an option on its mocks: it's done, and the item that builds the pick is to route.
    Picked,
    /// A route of it went nowhere: no session by that name, or a new session for it couldn't start.
    RouteFailed,
    /// Its holder ended, or hit its usage limit, with it at work.
    HolderGone,
    /// Its holder asks you something: the lead hears it (at once while you're away, so it can answer for you).
    NeedsYou,
    /// Its work clashed with the main line as it landed: handed back to its holder to settle.
    Clash,
    /// Its holder said DONE, it landed, or it's ready for your check (installed when it needed it).
    Finished,
    /// Away mode turned on or off on the Desk (no item).
    Away,
    /// What you did on the Desk yourself (Send back, Restart, Drop, Put it before): never wakes the lead.
    YouDid,
    /// Your words on it (a Send back, a question) went to its holder, but it waits for a free copy (#567): the lead
    /// hears them at once, so they don't sit unseen in the line.
    WordsWait,
    /// Its work is on another machine (#500) and needs the user: main here isn't pushed for it to start from, or the
    /// machine hasn't answered for a while.
    Machine,
    /// The full run of Cue's own main line passed (no item): never wakes the lead.
    FullRunPassed,
    /// The full run of Cue's own main line failed, or couldn't be made (no item).
    FullRunFailed,
}

/// How a row reaches the lead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    /// Wakes it at once (a parked lead is resumed for it).
    AtOnce,
    /// Goes with the next wake, or alone once it has waited `GROUPED_ALONE_MS`.
    Grouped,
    /// Never wakes it: goes with the next message, so its picture is right.
    Silent,
}

/// What every message of rows ends with, before their numbers: how a kept message is known to carry them.
pub const NUMBERS: &str = "(Desk changes ";

/// How long a grouped row waits for a wake before it goes on its own.
pub const GROUPED_ALONE_MS: u64 = 5 * 60_000;

/// How long an item's finishing may sit on one step before its lead hears it's stuck.
pub const LANDING_STUCK_MS: u64 = 15 * 60_000;

impl ChangeKind {
    pub const ALL: [ChangeKind; 16] = [
        ChangeKind::NewItem,
        ChangeKind::Stopped,
        ChangeKind::StuckLanding,
        ChangeKind::PermissionStop,
        ChangeKind::Picked,
        ChangeKind::RouteFailed,
        ChangeKind::HolderGone,
        ChangeKind::NeedsYou,
        ChangeKind::Clash,
        ChangeKind::Finished,
        ChangeKind::Away,
        ChangeKind::YouDid,
        ChangeKind::WordsWait,
        ChangeKind::Machine,
        ChangeKind::FullRunPassed,
        ChangeKind::FullRunFailed,
    ];

    pub fn wake(self) -> Wake {
        match self {
            ChangeKind::NewItem
            | ChangeKind::Stopped
            | ChangeKind::StuckLanding
            | ChangeKind::PermissionStop
            | ChangeKind::Picked
            | ChangeKind::RouteFailed
            | ChangeKind::HolderGone
            | ChangeKind::WordsWait
            | ChangeKind::Machine
            | ChangeKind::FullRunFailed => Wake::AtOnce,
            ChangeKind::NeedsYou | ChangeKind::Clash | ChangeKind::Finished | ChangeKind::Away => Wake::Grouped,
            ChangeKind::YouDid | ChangeKind::FullRunPassed => Wake::Silent,
        }
    }

    /// How it wakes the lead with Away in force on its Desk (`away`) or not: a holder's ask wakes it at once under
    /// Away, since the lead answers for you then.
    pub fn wake_in(self, away: bool) -> Wake {
        if away && self == ChangeKind::NeedsYou { Wake::AtOnce } else { self.wake() }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::NewItem => "new",
            ChangeKind::Stopped => "stopped",
            ChangeKind::StuckLanding => "stuck_landing",
            ChangeKind::PermissionStop => "permission",
            ChangeKind::Picked => "picked",
            ChangeKind::RouteFailed => "route_failed",
            ChangeKind::HolderGone => "holder_gone",
            ChangeKind::NeedsYou => "needs_you",
            ChangeKind::Clash => "clash",
            ChangeKind::Finished => "finished",
            ChangeKind::Away => "away",
            ChangeKind::YouDid => "you",
            ChangeKind::WordsWait => "words_wait",
            ChangeKind::Machine => "machine",
            ChangeKind::FullRunPassed => "full_run_passed",
            ChangeKind::FullRunFailed => "full_run_failed",
        }
    }

    pub fn parse(s: &str) -> Option<ChangeKind> {
        ChangeKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// One change: row `n` of Desk `desk`, about item `item`. `key`: what makes a repeat of it the same row (a stop's
/// item `updated`), so a timer that sees it again adds nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub desk: String,
    pub n: u64,
    pub item: String,
    pub kind: ChangeKind,
    pub words: String,
    pub key: String,
    pub created_ms: u64,
}

/// A mark: up to row `n`, for lead `lead`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mark {
    pub n: u64,
    pub lead: String,
}

/// One Desk's feed as Cue holds it: its rows past "heard", the marks, and the next number.
#[derive(Clone, Debug, Default)]
pub struct Feed {
    pub rows: Vec<Change>,
    pub next: u64,
    pub sent: Mark,
    pub heard: Mark,
    /// How many of the lead's turns had ended (`Session::turn_ends`) when the sent rows reached it, and whether Cue
    /// kept them for the end of the lead's turn then and they haven't gone yet. Only a turn ended after that count,
    /// with them gone, heard them. Not saved: after a restart, sending starts again from "heard".
    pub sent_turn: u64,
    pub sent_kept: bool,
    /// A send to the lead is under way: another for this Desk waits for it (one at a time, so the marks move in
    /// order).
    pub sending: bool,
    /// Rows numbered but not saved yet (`desk_save` writes them with the items).
    pub unsaved: Vec<u64>,
}

impl Feed {
    /// The row past "heard" with this item, kind and key, if there is one.
    fn has(&self, item: &str, kind: ChangeKind, key: &str) -> bool {
        self.rows.iter().any(|r| r.item == item && r.kind == kind && r.key == key)
    }

    /// Where sending starts for lead `lead`: past "sent" when it went to that lead, else (a new lead, a restart)
    /// past "heard".
    pub fn from_for(&self, lead: &str) -> u64 {
        if self.sent.lead == lead && self.sent.n >= self.heard.n { self.sent.n } else { self.heard.n }
    }

    /// The lead `lead`'s turn ended cleanly, its `turn`th to end: what was sent to it before that turn is heard
    /// (a turn counted after the send, with a kept message gone by then). False when nothing moved.
    pub fn heard_by(&mut self, lead: &str, turn: u64) -> bool {
        if self.sent.lead != lead || self.sent.n <= self.heard.n || turn <= self.sent_turn || self.sent_kept {
            return false;
        }
        self.heard = self.sent.clone();
        self.rows.retain(|r| r.n > self.heard.n);
        true
    }

    /// The kept message with the sent rows went to the lead, whose turns ended so far are `turn`: the turn it starts
    /// is the one that hears them.
    pub fn kept_went(&mut self, turn: u64) {
        self.sent_kept = false;
        self.sent_turn = self.sent_turn.max(turn);
    }

    /// The kept message with the sent rows never went (taken back, its session gone): sent again from "heard".
    pub fn kept_dropped(&mut self) {
        self.sent = self.heard.clone();
        self.sent_kept = false;
    }
}

/// Every Desk's feed, by Desk.
pub type Feeds = HashMap<String, Feed>;

/// A new row on Desk `desk`: numbered now, in the same change as what it says, and saved with it. None when a row
/// with the same item, kind and key is still past "heard" (a timer seeing the same change again).
pub fn add(feeds: &mut Feeds, desk: &str, item: &str, kind: ChangeKind, words: String, key: &str, now: u64) -> Option<u64> {
    let f = feeds.entry(desk.to_string()).or_default();
    if f.has(item, kind, key) {
        return None;
    }
    f.next = f.next.max(f.heard.n).max(f.sent.n) + 1;
    let n = f.next;
    f.rows.push(Change { desk: desk.to_string(), n, item: item.to_string(), kind, words, key: key.to_string(), created_ms: now });
    f.unsaved.push(n);
    Some(n)
}

/// Of `rows` (in order), those that still apply (`applies`), two of the same item and kind folded into the latest.
pub fn still_apply<'a>(rows: &'a [Change], applies: impl Fn(&Change) -> bool) -> Vec<&'a Change> {
    let live: Vec<&Change> = rows.iter().filter(|r| applies(r)).collect();
    live.iter().enumerate().filter(|(i, r)| !live[i + 1..].iter().any(|l| l.item == r.item && l.kind == r.kind)).map(|(_, r)| *r).collect()
}

/// Whether `rows` (those that still apply) should go now: one wakes at once, or a grouped one has waited long enough.
/// `away`: Away is in force on their Desk.
pub fn wakes(rows: &[&Change], now: u64, away: bool) -> bool {
    rows.iter().any(|r| match r.kind.wake_in(away) {
        Wake::AtOnce => true,
        Wake::Grouped => now >= r.created_ms + GROUPED_ALONE_MS,
        Wake::Silent => false,
    })
}

/// The numbers of `rows` as the lead reads them: "41–44", or "41, 43–44" with a gap.
pub fn numbers(rows: &[&Change]) -> String {
    let mut runs: Vec<(u64, u64)> = Vec::new();
    for r in rows {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == r.n => *end = r.n,
            _ => runs.push((r.n, r.n)),
        }
    }
    runs.iter().map(|(a, b)| if a == b { a.to_string() } else { format!("{a}–{b}") }).collect::<Vec<_>>().join(", ")
}

/// The message for `rows`: each row's words in order, then `tail` (what's said once for them all, as "Route it now"
/// for new items), then which numbers they are (the lead skips numbers it has seen already). Rows only: the Desk's
/// summary goes only to a new or resumed lead.
pub fn message(rows: &[&Change], tail: &str) -> String {
    let mut words: Vec<&str> = rows.iter().map(|r| r.words.trim()).collect();
    if !tail.trim().is_empty() {
        words.push(tail.trim());
    }
    // Cue's words say "the owner": the lead hears your name (#574).
    crate::owner::in_prompt(&format!("{}\n\n{NUMBERS}{})", words.join("\n\n"), numbers(rows)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feeds_with(rows: &[(&str, ChangeKind, &str)]) -> Feeds {
        let mut f = Feeds::new();
        for (item, kind, key) in rows {
            add(&mut f, "d", item, *kind, format!("{item} {}", kind.as_str()), key, 0);
        }
        f
    }

    #[test]
    fn rows_are_numbered_in_order_and_a_repeat_from_a_timer_adds_nothing() {
        let mut f = feeds_with(&[("a", ChangeKind::NewItem, ""), ("b", ChangeKind::Stopped, "10")]);
        assert_eq!(add(&mut f, "d", "b", ChangeKind::Stopped, "again".into(), "10", 0), None, "same item, kind and key");
        assert_eq!(add(&mut f, "d", "b", ChangeKind::Stopped, "later".into(), "11", 0), Some(3), "the item changed since");
        assert_eq!(f["d"].rows.iter().map(|r| r.n).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(f["d"].unsaved, vec![1, 2, 3]);
    }

    #[test]
    fn rows_that_no_longer_apply_are_left_out_and_two_of_a_kind_fold_into_the_latest() {
        let f = feeds_with(&[("a", ChangeKind::Stopped, "1"), ("b", ChangeKind::NewItem, ""), ("a", ChangeKind::Stopped, "2"), ("c", ChangeKind::Stopped, "1")]);
        let rows = &f["d"].rows;
        let kept = still_apply(rows, |r| r.item != "c");
        assert_eq!(kept.iter().map(|r| r.n).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(numbers(&kept), "2–3");
        let m = message(&kept, "Route it now.");
        assert!(m.starts_with("b new\n\na stopped\n\nRoute it now.") && m.ends_with("(Desk changes 2–3)"), "{m}");
        assert_eq!(numbers(&[&rows[0], &rows[2], &rows[3]]), "1, 3–4");
    }

    #[test]
    fn heard_moves_only_on_a_turn_counted_after_the_send_to_the_lead_it_went_to() {
        let mut f = feeds_with(&[("a", ChangeKind::NewItem, ""), ("b", ChangeKind::NewItem, "")]);
        let d = f.get_mut("d").unwrap();
        assert_eq!(d.from_for("L"), 0);
        (d.sent, d.sent_turn) = (Mark { n: 2, lead: "L".into() }, 4);
        assert_eq!(d.from_for("L"), 2);
        assert_eq!(d.from_for("M"), 0, "a new lead starts from heard");
        assert!(!d.heard_by("M", 5), "another lead's turn");
        assert!(!d.heard_by("L", 4), "the turn that ended before the send");
        d.sent_kept = true;
        assert!(!d.heard_by("L", 5), "kept and not gone yet");
        d.kept_went(5);
        assert!(!d.heard_by("L", 5), "the turn it was kept through");
        assert!(d.heard_by("L", 6));
        assert_eq!((d.heard.n, d.rows.len()), (2, 0));
    }

    #[test]
    fn grouped_rows_wait_for_a_wake_or_five_minutes() {
        let r = Change { desk: "d".into(), n: 1, item: "a".into(), kind: ChangeKind::NewItem, words: String::new(), key: String::new(), created_ms: 0 };
        assert!(wakes(&[&r], 0, false));
        let g = Change { kind: ChangeKind::Finished, created_ms: 1_000, ..r.clone() };
        assert!(!wakes(&[&g], 1_000 + GROUPED_ALONE_MS - 1, false), "a grouped row alone waits");
        assert!(wakes(&[&g], 1_000 + GROUPED_ALONE_MS, false), "then goes on its own");
        let s = Change { kind: ChangeKind::YouDid, ..r.clone() };
        assert!(!wakes(&[&s], u64::MAX, false), "a silent row never wakes");
        let ask = Change { kind: ChangeKind::NeedsYou, ..g.clone() };
        assert!(!wakes(&[&ask], 1_000, false) && wakes(&[&ask], 1_000, true), "a holder's ask wakes at once under Away");
        assert!(ChangeKind::ALL.iter().all(|k| ChangeKind::parse(k.as_str()) == Some(*k)));
    }
}
