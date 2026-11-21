//! What you do to a Desk item from the window (desk/hub/core/desk.rs): add, give, act on it (looks right, reopen,
//! send back, start now, put before, pick an option, drop) and undo.

use crate::desk::hub::prelude::*;
use crate::desk::{DeskDecision, DeskItem, Holder};
use crate::desk::hub::core::members::join;
use crate::desk::hub::core::desk::{Effects, NewStart, ask_of, at_work, holder_of, rule_of, secs};
use crate::desk::hub::core::view::shown;
use crate::desk::hub::turn::route::{decide, give, hand_on, release, send_step, start_released};
use crate::desk::hub::turn::heard::new_item;

/// What you did to item `id` ("looks_right" | "reopen" | "send_back" | "start_now" | "put_before" | "hold" | "drop"), in the store.
fn act_on(st: &mut Store, id: &str, act: &str, text: &str, now: u64) -> Result<Effects, String> {
    // What you did, for the lead's picture: read first, as a drop removes the item.
    // Words on its page (a note) to check or done that ask something are a question (#475): said to the lead as such
    // (#567). Any other note is a note, never a Send back (#584).
    let said = match st.desk.item(id).filter(|i| act == "note" && i.holder.is_some()) {
        Some(i) if crate::desk::hub::turn::asked::is_question(text) && matches!(i.state.as_str(), "to_check" | "done") => "ask",
        _ => act,
    };
    let did = crate::desk::hub::lead::feed::you_did(st, id, said, text);
    let mut fx = acted_on(st, id, act, text, now)?;
    if let Some((project, words)) = did.filter(|(p, _)| st.desk.lead_sid(p).is_some()) {
        crate::desk::feed::add(&mut st.desk.feed, &project, id, crate::desk::feed::ChangeKind::YouDid, words, &now.to_string(), now);
    }
    // Anything you did to it but write to its lead answers what its holder asked you (#475).
    if !matches!(act, "retry" | "wrote_lead") && st.desk.item_mut(id).and_then(|i| i.asks.take()).is_some() {
        fx.items.push(id.to_string());
    }
    Ok(fx)
}

fn acted_on(st: &mut Store, id: &str, act: &str, text: &str, now: u64) -> Result<Effects, String> {
    if act == "pick" {
        return pick(st, id, text, now);
    }
    let mut fx = Effects::default();
    // A note from its page (#437): to whoever holds it, at any stage. To check, it stays to check (#584: "ok testing
    // now" isn't a Send back); only Send back reopens it. With nobody holding it, the window sends it to the lead.
    // A question there (#475) isn't a Send back: its holder answers it, and the item stays where it was unless that
    // answer changes its code.
    // A quick phrase clicked on its page (#488, "Go ahead", "Commit"): a note to its holder at any stage, never a Send
    // back, so the item stays to check.
    let phrase = act == "phrase";
    let act = if phrase { "note" } else { act };
    if act == "note" && crate::desk::hub::turn::asked::is_question(text) && st.desk.item(id).is_some_and(|i| matches!(i.state.as_str(), "to_check" | "done") && i.holder.is_some()) {
        return crate::desk::hub::turn::asked::ask(st, id, text, now);
    }
    if act == "retry" {
        return retry(st, id, text, now);
    }
    // Parked (#385, #391): Park puts an item nobody is at work on there; Start on a held one makes it new again,
    // for the lead to route as any new item.
    let held = st.desk.item(id).is_some_and(|i| i.state == "held");
    // "park" (#391); "hold", its first name, still works.
    let act = if act == "park" { "hold" } else { act };
    if act == "hold" || (act == "start_now" && held) {
        park(st, id, act, text, now, &mut fx)?;
        return Ok(fx);
    }
    let stamp = apply(st, id, act, text, now, &mut fx)?;
    if !stamp {
        return Ok(fx);
    }
    if let Some(it) = st.desk.item_mut(id) {
        it.updated = secs(now);
    }
    fx.items.push(id.to_string());
    Ok(fx)
}

/// Act `act` (as `acted_on` settled it) done on item `id`: whether the item is stamped as changed afterwards.
fn apply(st: &mut Store, id: &str, act: &str, text: &str, now: u64, fx: &mut Effects) -> Result<bool, String> {
    let busy = st.desk.item(id).and_then(|i| i.holder.as_ref()).filter(|h| at_work(st, h)).map(|h| h.name.clone());
    // Picked, but its build item dropped since: it can be undone, and picked again.
    let build_gone = st.desk.item(id).is_some_and(|i| !i.built_as.is_empty() && st.desk.item(&i.built_as).is_none());
    let pr_open = pr_open(st, id, act);
    let rule = st.desk.item(id).map(|i| rule_of(st, &i.project)).unwrap_or_default();
    let holder = st.desk.item(id).ok_or("no such item")?.holder.clone();
    // Whether the item is stamped as changed afterwards: the acts that answer at once say false.
    let stamp = match act {
        "looks_right" => looks_right(st, id, now, fx)?,
        "skip_install" => skip_install(st, id, holder, now, fx)?,
        "reopen" => reopen(st, id, build_gone, now, fx)?,
        "send_back" => send_back(st, id, holder, text, &pr_open, now, fx)?,
        "note" => note_to_holder(st, id, holder, text, now, fx)?,
        "start_now" => start_now(st, id, holder, busy, &rule, now, fx)?,
        "wrote_lead" | "wrote_holder" => wrote(st, id, act, holder, text, now, fx)?,
        "put_before" => {
            put_before(st, id, now, fx)?;
            true
        }
        "reorder" => {
            reorder(st, id, text, fx)?;
            false
        }
        "drop" => drop_item(st, id, &pr_open, now, fx),
        _ => return Err(format!("unknown act: {act}")),
    };
    Ok(stamp)
}

/// Item `id`, to change: an act's own step finds it again, as `acted_on` found it.
fn item_of<'a>(st: &'a mut Store, id: &str) -> Result<&'a mut DeskItem, String> {
    st.desk.item_mut(id).ok_or_else(|| "no such item".to_string())
}

/// Park (`hold`) item `id`, or Start on a parked one: new again, for the lead to route.
fn park(st: &mut Store, id: &str, act: &str, text: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let project = st.desk.item(id).ok_or("no such item")?.project.clone();
    let (words, whom) = if act == "hold" {
        crate::desk::hub::stall::unstick::hold(st, "You", id, text, now, fx).map_err(|e| e.split_once(": ").map_or(e.clone(), |(_, w)| w.to_string()))?;
        ("Parked it", "nothing starts it till you press Start")
    } else {
        crate::desk::hub::stall::unstick::unhold(st, id, now, fx)?;
        ("Start", "no longer parked: queued as a new item")
    };
    you_wrote(st, &project, id, words, whom, now, fx);
    Ok(())
}

/// In review: its pull request stays open on GitHub when it's sent back or dropped (Cue never closes it). The words
/// that say so ("" when it has none).
fn pr_open(st: &mut Store, id: &str, act: &str) -> String {
    let pr = st.desk.item(id).and_then(|i| i.finish.as_ref().filter(|f| i.state == "finishing" && f.step == "review" && !f.pr.is_empty())).map(|f| f.pr.clone());
    if pr.is_some() && matches!(act, "send_back" | "drop") {
        st.desk.run.pr_looked.remove(id);
        st.desk.run.pr_misses.remove(id);
    }
    pr.map(|u| format!(" Its pull request stays open on GitHub: {u}")).unwrap_or_default()
}

/// The items queued after `id` now (on a Desk that isn't paused): what its Looks right lets go.
fn waiting_on(st: &Store, id: &str) -> Vec<String> {
    st.desk.items.iter().filter(|i| i.after.as_deref() == Some(id) && i.state == "queued" && !crate::desk::hub::away::pause::is_paused(&i.project)).map(|i| i.id.clone()).collect()
}

/// Item `id` is no longer done (Undo, or Reopen): those its Looks right let go wait for it again, while none has
/// started (no holder yet, #576).
fn hold_released(st: &mut Store, id: &str, now: u64, fx: &mut Effects) {
    let Some(ids) = st.desk.item_mut(id).map(|i| std::mem::take(&mut i.released)) else { return };
    for w in ids {
        let Some(it) = st.desk.item_mut(&w).filter(|i| i.holder.is_none() && i.state == "queued" && i.after.is_none()) else { continue };
        it.after = Some(id.to_string());
        it.updated = secs(now);
        fx.items.push(w);
    }
}

fn looks_right(st: &mut Store, id: &str, now: u64, fx: &mut Effects) -> Result<bool, String> {
    let released = waiting_on(st, id);
    let it = item_of(st, id)?;
    it.released = released;
    it.state = "done".into();
    it.after = None;
    it.close_on_check = false;
    it.done_at = Some(secs(now));
    release(st, id, now, fx);
    Ok(true)
}

// Done, skip install (#490): landed on main and waiting for its install, it's done now with no install of its
// own. Its work stays on main, so the next install (whatever starts it) has it; its holder is told.
fn skip_install(st: &mut Store, id: &str, holder: Option<Holder>, now: u64, fx: &mut Effects) -> Result<bool, String> {
    let it = item_of(st, id)?;
    if !(it.state == "finishing" && it.finish.as_ref().is_some_and(|f| f.step == "queued")) {
        return Err("It isn't waiting for an install".into());
    }
    if let Some(f) = it.finish.as_mut() {
        (f.step, f.at, f.now, f.question, f.wait) = ("checked".into(), now, false, String::new(), String::new());
    }
    (it.state, it.after, it.close_on_check, it.done_at) = ("done".into(), None, false, Some(secs(now)));
    let (title, project) = (it.title.clone(), it.project.clone());
    let released = waiting_on(st, id);
    item_of(st, id)?.released = released;
    release(st, id, now, fx);
    you_wrote(st, &project, id, "Done, skip install", "closed without an install", now, fx);
    if let Some(h) = holder {
        let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
        let message = format!("{crew}Desk item #{id} ({title}) is done: closed without an install, none is needed for it. Its work stays on main and goes into the next install. Nothing more to do on it.");
        send_step(st, id, h.to(), message, now, fx);
    }
    Ok(true)
}

// Undo on a done item: back to waiting on you, nothing in the work touched (a commit made stays; one still
// waiting isn't made).
fn reopen(st: &mut Store, id: &str, build_gone: bool, now: u64, fx: &mut Effects) -> Result<bool, String> {
    let it = item_of(st, id)?;
    if it.state != "done" {
        return Err("It isn't done: nothing to undo".into());
    }
    // Done by your pick: what it filed carries it on.
    if !it.picked.is_empty() && !build_gone {
        return Err(format!("Option {} is picked: it's being built as #{}", it.picked, it.built_as));
    }
    (it.picked, it.built_as) = (String::new(), String::new());
    (it.state, it.done_at, it.commit_wait) = ("to_check".into(), None, None);
    hold_released(st, id, now, fx);
    Ok(true)
}

/// Where Send back, or Reopen on a done item, takes item `id` (#577): back to `h`, the session that held it, unless it's
/// closed. Err: why not, in words for its page and the fresh session ("cue-268 is closed"). A done item whose session
/// has moved on to another item queues for it (#576, `send_back`).
fn back_to(st: &Store, h: &Holder) -> Result<(), String> {
    match st.sessions.get(h.to()) {
        None => Err(format!("{} is closed", h.name)),
        Some(_) => Ok(()),
    }
}

fn send_back(st: &mut Store, id: &str, holder: Option<Holder>, text: &str, pr_open: &str, now: u64, fx: &mut Effects) -> Result<bool, String> {
    let h = holder.ok_or("nobody holds it yet")?;
    let words = text.trim().to_string();
    let done = item_of(st, id)?.state == "done";
    if words.is_empty() {
        let what = if done { "Reopen" } else { "Send back" };
        return Err(format!("Say what isn't right first: type it in the box, then {what}"));
    }
    // Done by your pick: what it filed carries it on, as for Undo (`reopen`).
    let built = st.desk.item(id).map(|i| i.built_as.clone()).filter(|b| st.desk.item(b).is_some());
    if let Some(b) = built.filter(|_| done) {
        return Err(format!("Option {} is picked: it's being built as #{b}", item_of(st, id)?.picked));
    }
    // With a copy of its own still (a landing that stopped), it stays with its holder: the copy is that session's.
    if done {
        hold_released(st, id, now, fx);
    }
    let gone = back_to(st, &h).err().filter(|_| item_of(st, id).is_ok_and(|i| i.tree.is_none()));
    // Done, and its session has moved on to another item (one item per session, #576): it waits for that session.
    let busy = crate::desk::hub::turn::queue::busy_with(st, &h, id).filter(|_| done && gone.is_none() && item_of(st, id).is_ok_and(|i| i.tree.is_none()));
    let it = item_of(st, id)?;
    if done {
        (it.picked, it.built_as, it.commit_wait) = (String::new(), String::new(), None);
    }
    (it.state, it.done_at, it.finish, it.close_on_check) = ("working".into(), None, None, false);
    it.check = None;
    it.mark_reopened();
    // Kept on the DONE it answers, so its card says why (the pictures, added by ipc desk_item_act, counted).
    if let Some(c) = it.checks.last_mut() {
        let (why, pics) = crate::desk::your_words(&words);
        (c.back, c.back_pics) = (why.trim().to_string(), pics.len());
    }
    let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
    let (title, project) = (it.title.clone(), it.project.clone());
    let did = if done { "reopened" } else { "sent back" };
    if let Some(x) = busy {
        // What it's told once it's free; a fresh session's hand-off if it stays busy 20 minutes.
        let message = format!("{crew}Desk item #{id} ({title}) isn't done (you said it looks right, and now it's reopened): {}\nIts fixes stay in #{id}; don't add new items for them. Say DONE: #{id} when it is.\n{}{pr_open}", crate::desk::with_your_pictures(&words), crate::desk::check_words(id));
        let hand = reopen_handoff(it, &h, &format!("{} is at work on #{x}", h.name), &words);
        it.remember_holder();
        (it.moved_from, it.reopen_note, it.back_message) = (Some(h.clone()), hand, message);
        (it.holder, it.state, it.after, it.next_for, it.queued_at) = (None, "queued".into(), None, Some(h.clone()), now);
        it.crew.clear();
        you_wrote(st, &project, id, crate::desk::your_words(&words).0.trim(), &format!("{did}: next for {} once it's free (at work on #{x})", h.name), now, fx);
        return Ok(true);
    }
    // Its session can't take it back: a fresh one does, with a hand-off (#577).
    if let Some(why) = gone {
        let hand = reopen_handoff(it, &h, &why, &words);
        it.remember_holder();
        (it.moved_from, it.reopen_note) = (Some(h.clone()), hand);
        (it.holder, it.state, it.after) = (None, "new".into(), None);
        it.crew.clear();
        you_wrote(st, &project, id, crate::desk::your_words(&words).0.trim(), &format!("{did} for a fresh session: {why}"), now, fx);
        let why = format!("{did} from the Desk; {why}, so a fresh session takes it");
        fx.starts.push(NewStart { project, id: id.into(), why, by: "You".into(), by_sid: String::new() });
        return Ok(true);
    }
    // Back to the same session after it moved on: a new stint, so its page shows this part of its chat too (#575).
    if done {
        it.held_by(&h.sid, now);
    }
    you_wrote(st, &project, id, crate::desk::your_words(&words).0.trim(), &format!("{did} to {}", h.name), now, fx);
    // Your pictures go as pictures (desk_send_out), not as paths in its words (#541).
    let words = crate::desk::with_your_pictures(&words);
    let was = if done { " (you said it looks right, and now it's reopened)" } else { "" };
    let message = format!("{crew}Desk item #{id} ({title}) isn't done{was}: {words}\nIts fixes stay in #{id}; don't add new items for them. Say DONE: #{id} when it is.\n{}{pr_open}", crate::desk::check_words(id));
    fx.backs.push((id.to_string(), message.clone()));
    send_step(st, id, h.to(), message, now, fx);
    Ok(true)
}

/// What a fresh session taking item `it` back from `h` (closed, or busy: `why`) is told first (#577): what the item
/// was, who did it and what's on main, its files, and your words. Its holder's note or last reply follow
/// (route.rs `handoff_words`).
fn reopen_handoff(it: &DeskItem, h: &Holder, why: &str, words: &str) -> String {
    let id = &it.id;
    let mut out = format!("Hand-off: #{id} was done by {} and is reopened; {why}, so it's yours now.", h.name);
    let commits: Vec<&str> = it.commit_in.iter().map(|(_, c)| c.as_str()).chain(it.commit.split_whitespace()).collect();
    let mut seen = Vec::new();
    for c in commits {
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
    if !seen.is_empty() {
        out.push_str(&format!(" Its work so far is on main: {} (git show them first).", seen.join(", ")));
    }
    let files = it.checks.last().map(|c| c.files.as_slice()).filter(|f| !f.is_empty()).unwrap_or(&it.files);
    if !files.is_empty() {
        let shown: Vec<&str> = files.iter().take(12).map(String::as_str).collect();
        out.push_str(&format!(" Its files: {}{}.", shown.join(", "), if files.len() > 12 { ", …" } else { "" }));
    }
    let said = crate::desk::with_your_pictures(words);
    out.push_str(&format!("\nWhat isn't right, in the user's words: {said}"));
    out
}

// A note from its page to whoever holds it (#437), at any stage (to check too, #584: it stays to check): noted on
// it first, then sent as about the item ("About #12 (…): …"), kept in Cue while that session works, and how
// it went kept on its step, so the page says when it didn't go and offers Retry.
fn note_to_holder(st: &mut Store, id: &str, holder: Option<Holder>, text: &str, now: u64, fx: &mut Effects) -> Result<bool, String> {
    let it = item_of(st, id)?;
    let h = holder.filter(|_| !matches!(it.state.as_str(), "done" | "dropped")).ok_or("nobody holds it: it goes to the lead")?;
    let words = text.trim().to_string();
    if words.is_empty() {
        return Err("nothing written".into());
    }
    // Its holder asked you something as its turn ended: the answer goes where that ask came from.
    let ask = it.ask.as_ref().and_then(|a| a.get("sid")).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string);
    let to = ask.unwrap_or_else(|| h.to().to_string());
    let mate = if h.kind == "crew" { format!(", for {}", h.name) } else { String::new() };
    let message = format!("About #{id} ({}){mate}: {}", it.title, crate::desk::with_your_pictures(&words));
    let project = it.project.clone();
    you_wrote(st, &project, id, crate::desk::your_words(&words).0.trim(), &format!("to {}", h.name), now, fx);
    send_step(st, id, &to, message, now, fx);
    fx.items.push(id.to_string());
    Ok(false)
}

fn start_now(st: &mut Store, id: &str, holder: Option<Holder>, busy: Option<String>, rule: &str, now: u64, fx: &mut Effects) -> Result<bool, String> {
    // Next for a session (#576): it starts there if that one's free, else in a fresh session with a hand-off.
    let project = item_of(st, id)?.project.clone();
    if holder.is_none() && crate::desk::hub::turn::queue::start_next_now(st, &project, id, "", ("You", ""), now, fx) {
        you_wrote(st, &project, id, "Start now", "without waiting", now, fx);
        return Ok(true);
    }
    let it = item_of(st, id)?;
    // Waiting for a free copy (#401), its holder's (sent back, or given out with all copies in use): it starts now
    // where its holder is, as the lead's START does, with its words and your Send back's.
    if it.state == "queued" && it.tree_wait && holder.is_some() {
        (it.after, it.tree_wait, it.no_copy_wait, it.state) = (None, false, true, "working".into());
        it.tree_wait_repo.clear();
        let words = crate::desk::handed_words(it, "", rule);
        let (to, project) = (holder.as_ref().map(|h| h.to().to_string()).unwrap_or_default(), it.project.clone());
        fx.sends.push((to, words));
        you_wrote(st, &project, id, "Start now", "without waiting for a free working folder", now, fx);
    // Nobody holds it yet: a new session of its project takes it (desk_start_new).
    } else if holder.is_none() {
        // Tried again: "couldn't start" goes until this try says how it went.
        it.start_failed.clear();
        it.after = None;
        let s = NewStart { project: it.project.clone(), id: id.into(), why: String::new(), by: "You".into(), by_sid: String::new() };
        fx.starts.push(s);
    // Queued after another item, for its holder: it starts now with that holder, handed the item as a
    // release would (it never began, so it isn't a Resume); the item it waited on is left as it is.
    } else if it.state == "queued" && holder.is_some() {
        (it.after, it.state) = (None, "working".into());
        let words = crate::desk::handed_words(it, "", rule);
        let (to, project) = (holder.as_ref().map(|h| h.to().to_string()).unwrap_or_default(), it.project.clone());
        fx.sends.push((to, words));
        you_wrote(st, &project, id, "Start now", "without waiting for the item ahead", now, fx);
    } else {
        resume(st, id, holder, busy, now, fx)?;
    }
    Ok(true)
}

// Resume (an item at work whose holder isn't on it: cut off by a crash, a quit, or a turn that ended short of
// DONE): its holder's own session is woken to pick up where it left off (#375), not handed the item afresh.
fn resume(st: &mut Store, id: &str, holder: Option<Holder>, busy: Option<String>, now: u64, fx: &mut Effects) -> Result<(), String> {
    let it = item_of(st, id)?;
    let h = holder.ok_or("nobody holds it yet")?;
    if let Some(name) = busy.filter(|_| it.state == "working") {
        return Err(format!("{name} is at work on it now"));
    }
    it.after = None;
    it.state = "working".into();
    it.restart = format!("Resume: waking {}", h.name);
    it.check = None;
    let crew = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
    let words = format!("{crew}Desk item #{id} ({}): you stopped before it was done. Pick up where you left off.\nSay DONE: #{id} on a line of its own when it's done.\n{}", it.title, crate::desk::check_words(id));
    let project = it.project.clone();
    you_wrote(st, &project, id, "Resume", &format!("to {}", h.name), now, fx);
    fx.restarts.push((id.to_string(), h.to().to_string(), h.name.clone()));
    fx.sends.push((h.to().to_string(), words));
    Ok(())
}

// What you wrote on its page, to its lead or its holder (sent by the window): one of its Steps, with whom it
// went to. Nothing else about it changes (nor `updated`: it isn't news to you).
fn wrote(st: &mut Store, id: &str, act: &str, holder: Option<Holder>, text: &str, now: u64, fx: &mut Effects) -> Result<bool, String> {
    let it = item_of(st, id)?;
    let words = text.trim().to_string();
    if words.is_empty() {
        return Err("nothing written".into());
    }
    let project = it.project.clone();
    // To its holder, about an item at work: the turn it ended on before your note is no longer what it waits
    // on (#419). A stall (a question, a stop) counts from here, so it leaves Waiting on you for at work while
    // your note waits to go in or is read; a turn that ends on it after that is new.
    if act == "wrote_holder" && it.state == "working" {
        if let Some(sid) = holder.as_ref().filter(|h| h.kind != "crew").map(|h| h.sid.clone()) {
            st.desk.run.sent_at.insert(id.to_string(), (sid, now));
        }
    }
    let to = if act == "wrote_lead" { st.desk.lead.get(&project).map(|l| l.name.clone()) } else { holder.map(|h| h.name) };
    you_wrote(st, &project, id, &words, &to.map_or(String::new(), |n| format!("to {n}")), now, fx);
    Ok(false)
}

/// Item `id` dropped by its lead as you asked (DROP, desk/hub/stall/unstick.rs, #544): as your Drop does it.
pub(crate) fn drop_as_asked(st: &mut Store, id: &str, now: u64, fx: &mut Effects) {
    let pr_open = pr_open(st, id, "drop");
    drop_item(st, id, &pr_open, now, fx);
}

fn drop_item(st: &mut Store, id: &str, pr_open: &str, now: u64, fx: &mut Effects) -> bool {
    let gone = st.desk.items.iter().position(|i| i.id == id).map(|p| st.desk.items.remove(p));
    if let Some(h) = gone.as_ref().and_then(|g| g.holder.as_ref().filter(|_| g.state == "working")) {
        fx.sends.push((h.to().to_string(), format!("Desk item #{id} was dropped: stop work on it.")));
    }
    // Queued for a session that hasn't started it: off its list, so it never does (#544).
    if let Some((h, title)) = gone.as_ref().and_then(|g| g.holder.as_ref().filter(|_| g.state == "queued").map(|h| (h, &g.title))) {
        let who = if h.kind == "crew" { format!("For your teammate {}: ", h.name) } else { String::new() };
        fx.sends.push((h.to().to_string(), format!("{who}Desk item #{id} ({title}) was dropped: it's off your list. Don't start it.")));
    }
    // What a pick filed: once none of it is left, the picked item is no longer picked, so it can be picked again.
    let built = gone.as_ref().map(|g| g.builds.clone()).filter(|b| !b.is_empty());
    let other = built.as_deref().and_then(|b| st.desk.items.iter().find(|i| i.builds == b).map(|i| i.id.clone()));
    if let (Some(o), Some(m)) = (other, built.as_deref().and_then(|b| st.desk.item_mut(b)).filter(|m| m.built_as == id)) {
        m.built_as = o;
    }
    if let Some(m) = built.as_deref().and_then(|b| st.desk.item_mut(b)).filter(|m| m.built_as == id) {
        let (project, mid, option) = (m.project.clone(), m.id.clone(), std::mem::take(&mut m.picked));
        (m.built_as, m.updated) = (String::new(), secs(now));
        fx.items.push(mid.clone());
        you_wrote(st, &project, &mid, &format!("Dropped #{id}, the build of option {option}"), "it can be picked again", now, fx);
    }
    if let Some(g) = gone.as_ref().filter(|_| !pr_open.is_empty()) {
        let project = g.project.clone();
        you_wrote(st, &project, id, &format!("Dropped #{id}.{pr_open}"), "", now, fx);
    }
    // What waited on it waits on what it waited on, or starts now.
    let ahead = gone.as_ref().and_then(|g| g.after.clone());
    fx.trees_gone.extend(gone.filter(|g| g.tree.is_some()));
    fx.dropped.push(id.to_string());
    hand_on(st, id, ahead.as_deref(), now, fx);
    false
}

/// Your words about item `id`, as one of its Steps (`whom`: where they went), by you: nothing to undo.
/// Retry on item `id`'s step `step` (#437), whose message didn't go (or Cue stopped before it went): sent again as it
/// was, to the same session.
fn retry(st: &mut Store, id: &str, step: &str, now: u64) -> Result<Effects, String> {
    let mut fx = Effects::default();
    let d = st.desk.decisions.iter_mut().find(|d| d.id == step && d.item_id.as_deref() == Some(id)).ok_or("that message isn't on this item any more")?;
    let del = d.delivery.as_mut().ok_or("nothing to send again")?;
    if !del.went.starts_with("failed") {
        return Err("it went already".into());
    }
    (del.went, del.at_ms) = ("sending".into(), now);
    fx.sends.push((del.session_id.clone(), del.text.clone()));
    fx.went.push((d.id.clone(), del.session_id.clone(), id.to_string()));
    fx.decisions.push(d.id.clone());
    Ok(fx)
}

pub(crate) fn you_wrote(st: &mut Store, project: &str, id: &str, words: &str, whom: &str, now: u64, fx: &mut Effects) {
    decide(st, project, Some(id.to_string()), "You", "", words, whom, now, fx);
    if let Some(d) = st.desk.decisions.last_mut() {
        d.undo = "none".into();
    }
}

/// The option you picked ("B", "option b" → "B"), or "" when there's none.
fn option_of(text: &str) -> String {
    if let Some(o) = letter_of(text.trim().trim_end_matches('.')) {
        return o;
    }
    let t = text.trim().trim_end_matches('.').trim();
    let t = t.strip_prefix("option ").or_else(|| t.strip_prefix("Option ")).unwrap_or(t).trim();
    if t.chars().count() == 1 { t.to_uppercase() } else { t.chars().take(40).collect::<String>().trim().to_string() }
}

/// What an item is about, from its title: its first words before a colon go ("Mocks: working items", "Plan: the
/// landing line" → "working items", "the landing line"), as does a closing "(…)" note.
fn topic_of(title: &str) -> String {
    let t = title.trim();
    let mut topic = t;
    if let Some((head, rest)) = t.split_once(':') {
        if head.split_whitespace().count() <= 2 && !rest.trim().is_empty() {
            topic = rest.trim();
        }
    }
    if topic.ends_with(')') {
        if let Some(at) = topic.rfind(" (") {
            topic = topic[..at].trim();
        }
    }
    if topic.is_empty() { t.to_string() } else { topic.to_string() }
}

/// The option an answer names, as one capital letter, or a letter and a number (M1, #445): "B", "b", "Option B",
/// "B: the compact list", "B) …", "M2". None for anything else ("A new tab" isn't option A).
pub(crate) fn letter_of(answer: &str) -> Option<String> {
    let a = answer.trim();
    let a = if a.len() > 7 && a[..7].eq_ignore_ascii_case("option ") { a[7..].trim_start() } else { a };
    let mut chars = a.chars();
    let c = chars.next().filter(char::is_ascii_alphabetic)?;
    let digits: String = chars.as_str().chars().take_while(char::is_ascii_digit).take(2).collect();
    let rest = chars.as_str()[digits.len()..].trim_start();
    if rest.starts_with(|d: char| d.is_ascii_digit()) {
        return None;
    }
    let single = rest.is_empty();
    let marked = c.is_ascii_uppercase() && rest.starts_with([':', ')', '.', '-', '\u{2014}', '\u{2013}', ',', '(']);
    (single || marked).then(|| format!("{}{digits}", c.to_ascii_uppercase()))
}

/// The options a pick line names ("Pick one for working items: A, B or C." gives A, B, C): its newest words' line
/// with "pick" or "choose" in it (the one starting so first), and the first run of two or more single capital letters
/// joined by ",", "/", "or" or "and" in it. As the Desk page reads it (deskPickOptions).
pub(crate) fn pick_letters(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.split('\n').flat_map(|l| l.split_inclusive(['.', '?', '!'])).map(str::trim).collect();
    let words = |l: &str| l.split(|c: char| !c.is_alphanumeric()).map(str::to_ascii_lowercase).collect::<Vec<_>>();
    let asks: Vec<&str> = lines.into_iter().filter(|l| words(l).iter().any(|w| w == "pick" || w == "choose")).collect();
    let starts = |l: &str| words(l).first().is_some_and(|w| matches!(w.as_str(), "pick" | "choose" | "which"));
    let Some(line) = asks.iter().find(|l| starts(l)).or(asks.first()) else { return vec![] };
    // Words, with "," and "/" as their own; anything else breaks a run.
    let mut toks: Vec<String> = vec![];
    let mut word = String::new();
    for c in line.chars().chain([' ']) {
        if c.is_alphanumeric() {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            toks.push(std::mem::take(&mut word));
        }
        if !c.is_whitespace() {
            toks.push(if c == ',' || c == '/' { c.to_string() } else { "|".into() });
        }
    }
    // A, B… or M1, M2… (#445).
    let letter = |t: &str| (1..=3).contains(&t.len()) && t.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && t.chars().skip(1).all(|c| c.is_ascii_digit());
    let joins = |t: &str| matches!(t, "," | "/" | "or" | "and");
    for start in 0..toks.len() {
        if !letter(&toks[start]) {
            continue;
        }
        let (mut run, mut i) = (vec![toks[start].clone()], start + 1);
        loop {
            let mut j = i;
            while j < toks.len() && joins(&toks[j]) {
                j += 1;
            }
            if j == i || j >= toks.len() || !letter(&toks[j]) {
                break;
            }
            run.push(toks[j].clone());
            i = j + 1;
        }
        if run.len() > 1 {
            run.dedup();
            return run;
        }
    }
    vec![]
}

/// You picked option `text` on item `id` (#281, #543): it's done, picked. The follow-ups its holder named for that
/// option (IF PICKED: lines, `DeskItem::next`) are filed, each linked back to it (`builds`), and routed at once to
/// whom each names: its holder ("me"), a new session, or a session by name; one Cue can't route goes to the lead.
/// Named none: an item with a canvas gets one "Build: <topic> (option X)" item for the lead to route; any other just
/// closes. Picked already: nothing new, and the answer says what it's being built as.
pub(crate) fn pick(st: &mut Store, id: &str, text: &str, now: u64) -> Result<Effects, String> {
    let mut fx = Effects::default();
    let id = st.desk.canon(id);
    let it = st.desk.item(&id).ok_or("no such item")?.clone();
    let option = pickable(st, &it, text)?;
    let canvas = canvas_of(&it);
    let filed = file_follow_ups(st, &it, &option, &canvas, now);
    fx.answers.extend(question_answered(st, &it, &option));
    // Done first, so its holder is free to take what follows.
    let first = filed.first().map(|(n, _)| n.clone()).unwrap_or_default();
    let m = st.desk.item_mut(&id).ok_or("no such item")?;
    (m.picked, m.built_as) = (option.clone(), first.clone());
    (m.state, m.after, m.close_on_check, m.done_at, m.updated) = ("done".into(), None, false, Some(secs(now)), secs(now));
    fx.items.push(id.clone());
    let list = filed.iter().map(|(n, _)| format!("#{n}")).collect::<Vec<_>>().join(", ");
    let whom = if filed.is_empty() { "nothing follows it".to_string() } else if filed.len() == 1 { format!("being built as #{first}") } else { format!("followed by {list}") };
    you_wrote(st, &it.project, &id, &format!("Picked option {option}"), &whom, now, &mut fx);
    release(st, &id, now, &mut fx);
    let (routed, heard) = route_follow_ups(st, &it, &option, &filed, now, &mut fx);
    // Something of it left for its lead to route: the lead hears the pick at once, ahead of the new item's row, so it
    // routes it now. All of it routed already: no lead step.
    if !heard.is_empty() {
        let words = format!("The owner picked option {option} on #{id} ({}): \"{}\". #{id} is done; {}.", it.title, text.trim(), heard.join("; "));
        crate::desk::hub::lead::feed::raise(st, &it.project, &id, crate::desk::feed::ChangeKind::Picked, words, &format!("{option}:{first}"), now);
    }
    // Its holder is told it's done and what follows, so it doesn't carry on there.
    if let Some(h) = it.holder.as_ref() {
        let next = if routed.is_empty() { "Nothing follows it.".to_string() } else { format!("What follows: {}.", routed.join("; ")) };
        let mark = if canvas.is_empty() { String::new() } else { format!(" Mark \"Picked: {option}\" where it shows its options ({canvas}), keeping the others.") };
        fx.sends.push((h.to().to_string(), format!("Desk item #{id} ({}): option {option} was picked, so it's done. {next}{mark} Do no other work on #{id}.", it.title)));
    }
    Ok(fx)
}

/// The option `text` picks on `it`, or why it can't be picked: picked already (and being built as an item still
/// there), dropped, or no option named.
fn pickable(st: &Store, it: &DeskItem, text: &str) -> Result<String, String> {
    if !it.built_as.is_empty() && st.desk.item(&it.built_as).is_some() {
        return Err(format!("Option {} is picked already: it's being built as #{}", it.picked, it.built_as));
    }
    if !it.picked.is_empty() && it.built_as.is_empty() {
        return Err(format!("Option {} is picked already", it.picked));
    }
    if it.state == "dropped" {
        return Err("It was dropped".into());
    }
    let option = option_of(text);
    if option.is_empty() {
        return Err("Say which option you picked".into());
    }
    Ok(option)
}

/// Its canvas: the DESIGNS: link its session gave, or, for an item whose work changed nothing the app runs (#408,
/// #436), the link it made. Empty when it has none.
fn canvas_of(it: &DeskItem) -> String {
    let no_code = it.finish.as_ref().is_some_and(|f| f.mocks);
    if !it.designs.trim().is_empty() {
        it.designs.trim().to_string()
    } else if no_code {
        it.made.iter().find(|m| m.starts_with("http://") || m.starts_with("https://")).cloned().unwrap_or_default()
    } else {
        String::new()
    }
}

/// Files what follows `option` picked on `it`: the follow-ups its holder named, or, with none named and a canvas,
/// one "Build: <topic> (option X)" item. Each: (its id, whom it names to take it; empty for the lead).
fn file_follow_ups(st: &mut Store, it: &DeskItem, option: &str, canvas: &str, now: u64) -> Vec<(String, String)> {
    let id = &it.id;
    let named: Vec<crate::desk::model::FollowUp> = it.next.iter().filter(|n| n.option == "any" || n.option.eq_ignore_ascii_case(option)).cloned().collect();
    let on = if canvas.is_empty() { String::new() } else { format!(": {canvas}") };
    let mut filed: Vec<(String, String)> = Vec::new();
    for n in &named {
        let mut b = new_item(st, &it.project, &n.kind, &n.title, &n.done_when, "you", now);
        b.see = n.see.trim().to_string();
        b.builds = id.clone();
        b.why = format!("You picked option {option} on #{id}{on}");
        filed.push((b.id.clone(), n.by.clone()));
        st.desk.items.push(b);
    }
    if named.is_empty() && !canvas.is_empty() {
        let kind = if it.kind.is_empty() { "feature" } else { it.kind.as_str() };
        let title = format!("Build: {} (option {option})", topic_of(&it.title));
        let done_when = format!("option {option} of #{id} is in the app and works as #{id} shows it; where #{id} shows its options ({canvas}), it says \"Picked: {option}\"");
        let mut b = new_item(st, &it.project, kind, &title, &done_when, "you", now);
        b.builds = id.clone();
        b.why = format!("You picked option {option} on #{id}{on}");
        filed.push((b.id.clone(), String::new()));
        st.desk.items.push(b);
    }
    filed
}

/// Its holder waiting on its question (one): the question answered with the option picked. An ask of several
/// questions isn't answered for you.
fn question_answered(st: &Store, it: &DeskItem, option: &str) -> Option<(String, String, String)> {
    let one = |a: &&Item| a.tool_input["questions"].as_array().is_some_and(|q| q.len() == 1);
    it.holder.as_ref().and_then(|h| ask_of(st, h)).filter(|a| a.kind == "question").filter(one).map(|a| {
        let q = a.tool_input.pointer("/questions/0");
        let question = q.and_then(|q| q["question"].as_str()).unwrap_or("").to_string();
        let labels = q.and_then(|q| q["options"].as_array()).map(|o| o.iter().filter_map(|x| x["label"].as_str()).collect::<Vec<_>>()).unwrap_or_default();
        let label = labels.into_iter().find(|l| letter_of(l).as_deref() == Some(option)).map_or_else(|| format!("Option {option}"), str::to_string);
        (a.id.clone(), question, label)
    })
}

/// Each filed follow-up routed as it says, with no lead step; one with no taker, or one Cue can't route, to the
/// lead. Returns where each went, in plain lines for its holder, and what's left to the lead, in lines for it.
fn route_follow_ups(st: &mut Store, it: &DeskItem, option: &str, filed: &[(String, String)], now: u64, fx: &mut Effects) -> (Vec<String>, Vec<String>) {
    let id = &it.id;
    let lead = st.desk.lead.get(&it.project).cloned();
    let (lead_name, lead_sid) = (lead.as_ref().map(|l| l.name.clone()).unwrap_or_else(|| "You".into()), st.desk.lead_sid(&it.project).unwrap_or_default().to_string());
    let held = it.holder.as_ref().filter(|h| h.kind != "crew").map(|h| h.name.clone());
    let mut routed: Vec<String> = Vec::new();
    let mut heard: Vec<String> = Vec::new();
    for (n, by) in filed {
        let title = st.desk.item(n).map(|b| b.title.clone()).unwrap_or_default();
        you_wrote(st, &it.project, n, &format!("Follows option {option}, picked on #{id}"), "", now, fx);
        fx.items.push(n.clone());
        let to = if by.eq_ignore_ascii_case("me") { held.clone() } else if by.is_empty() { None } else { Some(by.clone()) };
        let said = to.as_ref().map(|to| {
            let by = crate::desk::hub::turn::route::By { name: &lead_name, sid: &lead_sid };
            let why = format!("you picked option {option} on #{id}, and #{id} named this to follow it");
            crate::desk::hub::turn::route::route(st, &it.project, &by, n, to, None, &why, now, fx)
        });
        let taken = st.desk.item(n).is_some_and(|b| b.holder.is_some() || b.state != "new") || fx.starts.iter().any(|s| s.id == *n);
        let refused = said.flatten().is_some_and(|w| w.starts_with("Refused"));
        if taken && !refused {
            routed.push(format!("#{n} to {}", to.unwrap_or_default()));
        } else if lead_sid.is_empty() {
            routed.push(format!("#{n} waits on the Desk for someone to take it"));
        } else {
            fx.news.push((it.project.clone(), n.clone()));
            heard.push(format!("Cue filed #{n} {title} to follow it: route it now"));
            routed.push(format!("#{n} goes to the Desk lead to route"));
        }
    }
    (routed, heard)
}

/// Your drag in the Queued list (#387): `order`, the queued items' ids top first as the list now shows them (`id`, the
/// one you moved, among them). Each takes its place (`queue_order`), unless one would sit above the item it waits
/// on: then nothing moves, and why says so in plain words. Not news to anyone: `updated` stays.
fn reorder(st: &mut Store, id: &str, order: &str, fx: &mut Effects) -> Result<(), String> {
    let ids: Vec<&str> = order.split([' ', ',', '\n']).map(|x| x.trim().trim_start_matches('#')).filter(|x| !x.is_empty()).collect();
    let project = st.desk.item(id).ok_or("no such item")?.project.clone();
    if !ids.contains(&id) {
        return Err(format!("#{id} isn't in that order"));
    }
    for x in &ids {
        let it = st.desk.item(x).ok_or_else(|| format!("#{x} isn't an item any more"))?;
        if it.project != project {
            return Err(format!("#{x} is on another Desk"));
        }
        // Running ones keep their place: only what hasn't started moves.
        if !matches!(it.state.as_str(), "new" | "queued") {
            return Err(format!("#{x} has started, so it can't move"));
        }
    }
    for (k, x) in ids.iter().enumerate() {
        let Some(on) = st.desk.item(x).and_then(|i| i.after.clone()) else { continue };
        if ids[k + 1..].contains(&on.as_str()) {
            return Err(format!("#{x} waits on #{on}, so it can't go above it"));
        }
    }
    for (k, x) in ids.iter().enumerate() {
        if let Some(it) = st.desk.item_mut(x) {
            it.queue_order = Some(k as u32 + 1);
            fx.items.push(x.to_string());
        }
    }
    Ok(())
}

/// Where a queued item stands in its Desk's queue (#387): those you placed first, in your order, then the rest oldest
/// first.
pub(crate) fn queue_key(i: &DeskItem) -> (u32, u64) {
    (i.queue_order.unwrap_or(u32::MAX), i.created)
}

/// Item `id` put before the one it waits on, in the queue: it takes that one's place (starting now when that one
/// waited on nothing), that one waits on it, and what waited on `id` waits on that one instead.
fn put_before(st: &mut Store, id: &str, now: u64, fx: &mut Effects) -> Result<(), String> {
    let (other, why) = st.desk.item(id).and_then(|it| it.after.clone().map(|a| (a, it.after_why.clone()))).ok_or("it isn't waiting on another item")?;
    let rule = st.desk.item(id).map(|i| rule_of(st, &i.project)).unwrap_or_default();
    let lead = st.desk.item(id).and_then(|i| st.desk.lead.get(&i.project)).cloned();
    // What that one waits on, if it hasn't had Looks right yet (#576): this one waits on it in its place.
    let open = |a: &String| st.desk.item(a).is_some_and(|w| w.state != "done");
    let ahead = st.desk.item(&other).and_then(|o| o.after.clone()).filter(open);
    let o = st.desk.item_mut(&other).ok_or("the item it waited on is gone")?;
    if o.state == "working" && o.holder.is_some() {
        // Already at work: only stopping it would put this first.
        return Err("the item it waits on is already being worked on".into());
    }
    let mut place = None;
    if (o.holder.is_some() || o.state == "queued") && !matches!(o.state.as_str(), "finishing" | "to_check" | "done") {
        place = ahead.map(|a| (a, o.after_why.clone()));
        o.after = Some(id.to_string());
        o.after_why = why;
        o.state = "queued".into();
        o.updated = secs(now);
        // Those queued after `id` stay behind both.
        for w in st.desk.items.iter_mut().filter(|w| w.id != other && w.after.as_deref() == Some(id)) {
            w.after = Some(other.clone());
            w.updated = secs(now);
            fx.items.push(w.id.clone());
        }
    }
    fx.items.push(other);
    let it = st.desk.item_mut(id).ok_or("no such item")?;
    match place {
        Some((a, a_why)) => {
            it.after = Some(a);
            it.after_why = a_why;
            it.state = "queued".into();
        }
        None => {
            it.after = None;
            start_released(it, &rule, lead.as_ref(), now, fx);
        }
    }
    Ok(())
}

impl Hub {
    /// An item of yours on `project`'s Desk. Held by live session `holder` (of the project) when given: at work on
    /// it, and sent nothing (it's already its work). Else new, and the lead (if any) told of it to route, with
    /// others added within a moment in the same message.
    /// Pictures you attached are kept with the item (uploads.rs `save_kept`) and named to whoever takes it.
    pub(crate) fn desk_add_with(&self, project: &str, kind: &str, title: &str, see: &str, done_when: &str, holder: Option<&str>, images: &[crate::model::Upload]) -> Result<DeskItem, String> {
        if project.trim().is_empty() || title.trim().is_empty() {
            return Err("an item needs a project and a title".into());
        }
        let images: Vec<String> = crate::uploads::save_kept(images)?.into_iter().map(|s| s.path).collect();
        let it = self.write(|st| -> Result<(DeskItem, Effects), String> {
            if project.starts_with("desk-") && !st.desk.spaces.contains_key(project) { return Err("no such Desk".into()); }
            let mut it = new_item(st, project, kind, title, done_when, "you", now_ms());
            it.see = see.trim().into();
            it.images = images.clone();
            let mut fx = Effects { items: vec![it.id.clone()], ..Default::default() };
            match holder {
                Some(sid) => {
                    // Given by hand: you override one item per session (#576), so a busy session takes it as another item.
                    let h = holder_of(st, sid).ok_or("that session isn't open")?;
                    if st.sessions.get(sid).is_some_and(|s| s.project != st.desk.project_of(project)) {
                        return Err(format!("{} isn't a session of {}", h.name, st.desk.project_of(project)));
                    }
                    if st.desk.lead_of(sid).is_some_and(|id| id != project) {
                        return Err("that session runs another Desk".into());
                    }
                    it.holder = Some(h);
                    it.state = "working".into();
                    join(st, project, sid, &mut fx);
                }
                None if st.desk.lead_sid(project).is_some() => fx.news.push((project.to_string(), it.id.clone())),
                None => {}
            }
            st.desk.items.push(it.clone());
            Ok((it, fx))
        })?;
        self.desk_done(it.1);
        Ok(it.0)
    }

    /// The same with no pictures, as the tests add.
    #[cfg(test)]
    pub(crate) fn desk_add(&self, project: &str, kind: &str, title: &str, done_when: &str, holder: Option<&str>) -> Result<DeskItem, String> {
        self.desk_add_with(project, kind, title, "", done_when, holder, &[])
    }

    /// You gave item `id` to session `sid`: see `give`. The item as it is now.
    pub(crate) fn desk_give(&self, id: &str, sid: &str) -> Result<DeskItem, String> {
        let now = now_ms();
        let fx = self.write(|st| {
            let mut fx = Effects::default();
            give(st, id, sid, now, &mut fx).map(|_| fx)
        })?;
        // As shown once its sends are stamped (desk_done).
        self.desk_done(fx);
        let it = self.item_shown(id);
        it.ok_or_else(|| "no such item".into())
    }

    /// What you did to an item: "looks_right" (done), "send_back" (to its holder again, with `text`), "note" (your
    /// words to its holder, never a Send back, #584), "phrase" (a quick phrase: a note, #488), "start_now"
    /// (not waiting on another any more), "put_before" (ahead of the item it waited on), "install_now" (its install
    /// starts without waiting, desk/hub/landing/finish.rs), "skip_install" (waiting to install, done now with no install of its own,
    /// #490), "rerun" (its landing run again: a plain retry, offered only when Cue
    /// can name nothing in its way, desk/hub/landing/finish.rs), what clears a wait (desk/hub/core/waits.rs): "cut" (no longer waits on
    /// item `text`), "drop", and "remove_kept" (a dropped item's kept copy deleted, desk/hub/landing/trees.rs: `id` is that item's).
    pub(crate) fn desk_item_act(&self, id: &str, act: &str, text: &str) -> Result<DeskItem, String> {
        match act {
            // Not done while changes on its own branch aren't on main (#448).
            "looks_right" => {
                let n = self.desk_unlanded(id);
                if n > 0 {
                    return Err(format!("{}: they're still on its branch. Send it back so they land", crate::desk::hub::landing::unlanded::unlanded_words(n)));
                }
            }
            "install_now" => return self.desk_install_now(id),
            "rerun" => return self.desk_rerun_landing(id),
            "remove_kept" => {
                self.desk_kept_remove(id)?;
                return Ok(DeskItem { id: id.to_string(), state: "dropped".into(), ..Default::default() });
            }
            "cut" => {
                self.desk_cut(id, text)?;
                let it = self.item_shown(id);
                return it.ok_or_else(|| "no such item".into());
            }
            _ => {}
        }
        let now = now_ms();
        let fx = self.write(|st| act_on(st, id, act, text, now))?;
        // As shown once its sends are stamped (desk_done).
        self.desk_done(fx);
        let it = self.item_shown(id);
        Ok(it.unwrap_or_else(|| DeskItem { id: id.to_string(), state: "dropped".into(), ..Default::default() }))
    }

    /// Undo this: whoever made decision `id` is told to undo it (through the normal send path: kept in Cue while
    /// it's busy), and the decision is marked so. What Cue did.
    pub(crate) fn desk_undo(&self, id: &str) -> Result<String, String> {
        self.desk_undo_with(id, "")
    }

    /// Undo decision `id`, telling whoever made it the user's new `answer` (when given) to take instead.
    pub(crate) fn desk_undo_with(&self, id: &str, answer: &str) -> Result<String, String> {
        let d = self.write(|st| -> Result<DeskDecision, String> {
            let d = st.desk.decisions.iter_mut().find(|d| d.id == id).ok_or("no such decision")?;
            match d.undo.as_str() {
                "can" => {}
                "asked" => return Err("already asked to undo it".into()),
                _ => return Err("this one can't be undone from here".into()),
            }
            d.undo = "asked".into();
            Ok(d.clone())
        })?;
        self.desk_done(Effects { decisions: vec![id.to_string()], ..Default::default() });
        let sent = self.tell(&d.sid, &crate::desk::undo_words_with(&d, answer));
        if sent.is_ok() {
            // A keep undone: Cue's own rule holds again for that session.
            self.write(|st| st.desk.run.keep.retain(|_, k| k.1 != d.id));
        }
        match sent {
            Ok(said) if crate::hub::team::kept(&said) => Ok(format!("Cue tells {} to undo this when its turn ends", d.by)),
            Ok(_) => Ok(format!("Asked {} to undo this", d.by)),
            Err(e) => {
                self.write(|st| st.desk.decisions.iter_mut().filter(|x| x.id == id).for_each(|x| x.undo = "can".into()));
                self.desk_done(Effects { decisions: vec![id.to_string()], ..Default::default() });
                Err(e)
            }
        }
    }
}
