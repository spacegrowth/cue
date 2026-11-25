//! Notifications (stability plan §4, Notifications), on the World's Notification Center (`FakeNotes`): a
//! card's notification is posted once and taken back once the card goes, however it goes; a session
//! something else is telling you about (notify::quiet) isn't notified twice for its finished turns; and a
//! notification that isn't a card's stays up through every draw.

use crate::testing::World;
use serde_json::json;

/// Sessions quieted (notify::quiet) for as long as this lives; none after.
struct Quiet;

impl Quiet {
    fn sessions(ids: &[&str]) -> Self {
        crate::notify::quiet(ids.iter().map(|s| s.to_string()).collect());
        Quiet
    }
}

impl Drop for Quiet {
    fn drop(&mut self) {
        crate::notify::quiet(Vec::new());
    }
}

fn count(ids: &[String], id: &str) -> usize {
    ids.iter().filter(|n| *n == id).count()
}

/// A finished turn's card: posted once, taken back once you reply, and never again by the draws after.
#[test]
fn a_cards_notification_is_posted_once_and_taken_back_once() {
    let w = World::new();
    w.session("a").prompt("fix it").stop("Fixed.");
    let id = w.snapshot().card("a")["id"].as_str().unwrap().to_string();
    assert_eq!(count(&w.notes.shown(), &id), 1);
    w.reply(&id, "thanks");
    w.session("b").prompt("more").stop("More done.");
    w.session("b").prompt("again");
    assert_eq!(count(&w.notes.taken_back(), &id), 1, "taken back once, the draws after notwithstanding");
}

/// However a card goes, its notification goes with it: answered in its own terminal, or its session ended.
#[test]
fn a_card_answered_elsewhere_or_whose_session_ended_is_taken_back() {
    let w = World::new();
    let ask = w.session("t").prompt("clean").ask_permission("Bash", json!({"command": "rm -rf build"}));
    ask.resolved_elsewhere(&w, "terminal", "allow");
    w.session("e").prompt("go").stop("Gone quiet.");
    let ended = w.snapshot().card("e")["id"].as_str().unwrap().to_string();
    w.session("e").ended();
    let back = w.notes.taken_back();
    assert_eq!((count(&back, &ask.id), count(&back, &ended)), (1, 1), "{back:?}");
}

/// A session something else reads its answers out for: its finished turns aren't notified, but what needs
/// you (a permission) still is; quiet no more, its next turn is notified again.
#[test]
fn a_quieted_sessions_turns_arent_notified_but_its_asks_are() {
    let w = World::new();
    let quiet = Quiet::sessions(&["v"]);
    w.session("v").prompt("read it to me").stop("Here it is.");
    let turn = w.snapshot().card("v")["id"].as_str().unwrap().to_string();
    let ask = w.session("v").prompt("now delete").ask_permission("Bash", json!({"command": "rm -rf tmp"}));
    w.session("other").prompt("x").stop("Done.");
    let other = w.snapshot().card("other")["id"].as_str().unwrap().to_string();
    let shown = w.notes.shown();
    assert_eq!((count(&shown, &turn), count(&shown, &ask.id), count(&shown, &other)), (0, 1, 1), "{shown:?}");
    drop(quiet);
    w.answer(&ask.id, json!({"behavior": "allow"}));
    w.session("v").stop("Deleted.");
    let next = w.snapshot().cards_of("v").iter().map(|c| c["id"].as_str().unwrap().to_string()).find(|id| *id != turn).expect("the new turn's card");
    assert_eq!(count(&w.notes.shown(), &next), 1, "notified again once it's not quiet");
}

/// A usage limit's notification isn't a card's: it stays up while cards come and go, each card's taken back.
#[test]
fn a_notification_that_isnt_a_cards_stays_up_while_cards_come_and_go() {
    let w = World::new();
    let a = w.session("a");
    a.prompt("do it all");
    a.failed("rate_limit", "You've hit your session limit · resets 4:10am (America/Los_Angeles)");
    let mut cards = vec![];
    for sid in ["b", "c", "d"] {
        let ask = w.session(sid).prompt("go").ask_permission("Bash", json!({"command": "make"}));
        w.answer(&ask.id, json!({"behavior": "allow"}));
        cards.push(ask.id);
    }
    assert_eq!(w.notes.taken_back(), cards, "the cards', in order, and never the limit's");
    assert_eq!(count(&w.notes.shown(), "usage"), 1);
}

/// The chats open in Cue's window for as long as this lives (#799); none, and Cue not in front, after. One at a
/// time: the window's chats and whether it's in front are Cue's, not a World's.
struct Chats(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

impl Chats {
    fn open(w: &World, chats: &[(&str, bool)], front: bool) -> Self {
        static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = ONE.lock().unwrap_or_else(|e| e.into_inner());
        let seen = crate::notify::open_chats("main", chats.iter().map(|(s, e)| (s.to_string(), *e)).collect(), Some(front));
        w.hub().chats_seen(&seen);
        Chats(guard)
    }
}

impl Drop for Chats {
    fn drop(&mut self) {
        crate::notify::open_chats("main", Vec::new(), Some(false));
    }
}

/// A reply you're reading as it comes (its chat open at its newest, Cue in front): never notified, even one you
/// asked to be told about; what's about another session still is.
#[test]
fn a_reply_seen_before_its_notification_is_never_notified() {
    let w = World::new();
    let _open = Chats::open(&w, &[("seen-799", true)], true);
    w.session("seen-799").prompt("fix it").stop("Fixed.");
    let seen = w.snapshot().card("seen-799")["id"].as_str().unwrap().to_string();
    w.session("else-799").prompt("go").stop("Done.");
    let other = w.snapshot().card("else-799")["id"].as_str().unwrap().to_string();
    let shown = w.notes.shown();
    assert_eq!((count(&shown, &seen), count(&shown, &other)), (0, 1), "{shown:?}");
}

/// A chat opened after its reply was notified: that notification, waiting out its minute or up already, is
/// taken back; another session's stays.
#[test]
fn opening_a_chat_takes_back_its_notifications() {
    let w = World::new();
    w.session("late-799").prompt("fix it").stop("Fixed.");
    let id = w.snapshot().card("late-799")["id"].as_str().unwrap().to_string();
    w.session("other-799").prompt("go").stop("Done.");
    let other = w.snapshot().card("other-799")["id"].as_str().unwrap().to_string();
    assert_eq!(count(&w.notes.shown(), &id), 1);
    let _open = Chats::open(&w, &[("late-799", false)], true);
    assert_eq!(count(&w.notes.taken_back(), &id), 0, "open but scrolled up: not read yet");
    let seen = crate::notify::open_chats("main", vec![("late-799".into(), true)], None);
    w.hub().chats_seen(&seen);
    let back = w.notes.taken_back();
    assert_eq!((count(&back, &id), count(&back, &other)), (1, 0), "{back:?}");
}

/// Not seen, so notified: its chat open but scrolled up, or open at its newest with Cue behind another app.
#[test]
fn a_reply_not_seen_is_still_notified() {
    let w = World::new();
    let open = Chats::open(&w, &[("up-799", false)], true);
    w.session("up-799").prompt("fix it").stop("Fixed.");
    let up = w.snapshot().card("up-799")["id"].as_str().unwrap().to_string();
    drop(open);
    let _open = Chats::open(&w, &[("behind-799", true)], false);
    w.session("behind-799").prompt("fix it").stop("Fixed.");
    let behind = w.snapshot().card("behind-799")["id"].as_str().unwrap().to_string();
    let shown = w.notes.shown();
    assert_eq!((count(&shown, &up), count(&shown, &behind)), (1, 1), "{shown:?}");
}
