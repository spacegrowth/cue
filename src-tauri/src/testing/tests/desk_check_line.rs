//! What you check on a done item (#526): its holder's `CHECK: #<id> | Try: …` or `CHECK: #<id> | Nothing to see: …`
//! line next to its DONE is kept on the item (desk/hub/turn/heard.rs `checks`), shown while it waits for your check,
//! and cleared when it's sent back, so an old one never shows.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::desk::parse::{lines, Line};
use crate::testing::World;
use serde_json::{json, Value};

fn check(id: &str, kind: &str, text: &str) -> Line {
    Line::Check { id: id.into(), kind: kind.into(), text: text.into(), steps: vec![] }
}

fn steps(id: &str, at: &str, act: &str, expect: &str) -> Line {
    Line::Check { id: id.into(), kind: "try".into(), text: act.into(), steps: vec![at.into(), act.into(), expect.into()] }
}

/// A holder at work on a routed item "Header jumps": (world, holder, lead, project, item id).
fn at_work() -> (World, String, String, String, String) {
    let w = World::new();
    let (h, project) = holder(&w);
    let l = new_lead(&w, &project);
    ends(&w, &l, "ITEM: bug | Header jumps | you'll see: it stays put\nROUTE: #new -> cue-teams | why: it built it");
    let id = item(&desk(&w, &project), "Header jumps")["id"].as_str().unwrap().to_string();
    (w, h, l, project, id)
}

fn kept(w: &World, project: &str) -> Value {
    item(&desk(w, project), "Header jumps").get("check").cloned().unwrap_or(Value::Null)
}

#[test]
fn check_lines_read_as_try_or_nothing_to_see_and_malformed_ones_as_nothing() {
    assert_eq!(lines("CHECK: #12 | Try: scroll the board, the header stays put"), vec![check("12", "try", "scroll the board, the header stays put")]);
    assert_eq!(lines("CHECK: #12 | Nothing to see: tests passed, close it"), vec![check("12", "nothing", "tests passed, close it")]);
    // Labels in any case, a list mark or bold around it, words holding a "|".
    assert_eq!(lines("- **CHECK:** #A7 | try : open Settings | Extensions"), vec![check("a7", "try", "open Settings | Extensions")]);
    assert_eq!(lines("1. CHECK: #3 | NOTHING TO SEE: a change nobody can see"), vec![check("3", "nothing", "a change nobody can see")]);
    for bad in [
        "CHECK: 12 | Try: scroll",
        "CHECK: | Try: scroll",
        "CHECK: #12 Try: scroll",
        "CHECK: #12 | Look: scroll",
        "CHECK: #12 | Try scroll",
        "CHECK: #12 | Trying: scroll",
        "CHECK: #12 | Try:",
        "CHECK: #12 | Nothing to see:   ",
        "CHECK: #<id> | Try: <one thing>",
        "`CHECK: #12 | Try: scroll`",
        "```\nCHECK: #12 | Try: scroll\n```",
    ] {
        assert_eq!(lines(bad), vec![], "{bad}");
    }
}

#[test]
fn done_with_a_try_line_keeps_it_on_the_item() {
    let (w, h, _, project, id) = at_work();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}\nCHECK: #{id} | Try: scroll the board; the header stays put"));
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "to_check");
    assert_eq!(kept(&w, &project), json!({ "kind": "try", "text": "scroll the board; the header stays put" }));
}

#[test]
fn done_with_nothing_to_see_keeps_it_and_the_last_one_said_wins() {
    let (w, h, _, project, id) = at_work();
    ends(&w, &h, &format!("CHECK: #{id} | Try: scroll the board\nDone.\nDONE: #{id}\nCHECK: #{id} | Nothing to see: tests passed, close it"));
    assert_eq!(kept(&w, &project), json!({ "kind": "nothing", "text": "tests passed, close it" }));
    // Said in a later turn, after its DONE: it replaces the one before.
    ends(&w, &h, &format!("One more thing.\nCHECK: #{id} | Try: open the board and scroll"));
    assert_eq!(kept(&w, &project), json!({ "kind": "try", "text": "open the board and scroll" }));
}

#[test]
fn done_without_a_check_line_keeps_none_and_a_check_before_done_or_from_another_session_does_nothing() {
    let (w, h, l, project, id) = at_work();
    // At work still: no DONE yet.
    ends(&w, &h, &format!("Halfway.\nCHECK: #{id} | Try: scroll the board"));
    assert_eq!(kept(&w, &project), Value::Null);
    ends(&w, &h, &format!("Done.\nDONE: #{id}"));
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "to_check");
    assert_eq!(kept(&w, &project), Value::Null);
    // The lead doesn't hold it.
    ends(&w, &l, &format!("CHECK: #{id} | Try: something else"));
    assert_eq!(kept(&w, &project), Value::Null);
}

#[test]
fn send_back_clears_it_and_the_holder_is_told_to_write_it_again() {
    let (w, h, _, project, id) = at_work();
    assert!(sent_to(&w, &h).iter().any(|t| t.contains(&format!("Say DONE: #{id}")) && t.contains(&format!("CHECK: #{id} | Where:")) && t.contains(&format!("CHECK: #{id} | Nothing to try:"))));
    ends(&w, &h, &format!("Done.\nDONE: #{id}\nCHECK: #{id} | Try: scroll the board"));
    assert_eq!(kept(&w, &project)["kind"], "try");
    let it = w.hub().desk_item_act(&id, "send_back", "It still jumps on scroll").unwrap();
    w.settle();
    assert!(it.check.is_none());
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "working");
    assert_eq!(kept(&w, &project), Value::Null);
    assert!(sent_to(&w, &h).iter().any(|t| t.contains("isn't done: It still jumps on scroll") && t.contains(&format!("CHECK: #{id} | Where:"))));
}

#[test]
fn the_brief_asks_for_the_check_line_even_without_a_youll_see() {
    let it = crate::desk::DeskItem { id: "7".into(), kind: "task".into(), title: "Tidy the parser".into(), ..Default::default() };
    let told = crate::desk::handed_words(&it, "", "");
    assert!(told.contains("Say DONE: #7 on a line of its own when it's done.\nWith your DONE, on a line of its own, write what whoever checks it does"), "{told}");
    assert!(told.contains("CHECK: #7 | Where: <the screen") && told.contains("| Do: <what to click") && told.contains("| Expect: <what they") && told.contains("CHECK: #7 | Nothing to try: <why"), "{told}");
}

#[test]
fn a_reopen_by_the_lead_and_a_bare_done_after_it_drop_last_rounds_line() {
    // Review of #526: the lead routing a to-check item back to work keeps it on its holder; its next DONE without a
    // CHECK must not show the line from the round before.
    let (w, h, l, project, id) = at_work();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}\nCHECK: #{id} | Try: scroll the board"));
    assert_eq!(kept(&w, &project)["kind"], "try");
    ends(&w, &l, &format!("ROUTE: #{id} -> cue-teams | why: the header still jumps"));
    ends(&w, &h, &format!("Fixed again.\nDONE: #{id}"));
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "to_check");
    assert_eq!(kept(&w, &project), Value::Null);
}

// #668: three steps (Where, Do, Expect) someone new can follow, and Cue sends a line back once when they're missing or
// not plain.

#[test]
fn three_step_check_lines_read_with_each_step_and_a_missing_step_reads_as_nothing() {
    assert_eq!(
        lines("CHECK: #12 | Where: the Desk, Settings | Do: pick a machine, close and reopen | Expect: it's still picked"),
        vec![steps("12", "the Desk, Settings", "pick a machine, close and reopen", "it's still picked")]
    );
    // Any case, bold, a step holding a "|", and "Nothing to try:" as the nothing label.
    assert_eq!(lines("- **CHECK:** #A7 | where : Settings | Extensions | DO: turn on Diff | expect: a Diff tab"), vec![steps("a7", "Settings | Extensions", "turn on Diff", "a Diff tab")]);
    assert_eq!(lines("CHECK: #3 | Nothing to try: a change nobody can see"), vec![check("3", "nothing", "a change nobody can see")]);
    for bad in ["CHECK: #12 | Where: the Desk | Do: click", "CHECK: #12 | Where: | Do: click | Expect: x", "CHECK: #12 | Where: a | Expect: b", "CHECK: #12 | Nothing to try:"] {
        assert_eq!(lines(bad), vec![], "{bad}");
    }
}

#[test]
fn done_with_three_plain_steps_keeps_them_and_sends_nothing_back() {
    let (w, h, _, project, id) = at_work();
    let before = sent_to(&w, &h).len();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}\nCHECK: #{id} | Where: the Desk | Do: scroll the list of items | Expect: the header stays at the top"));
    assert_eq!(kept(&w, &project), json!({ "kind": "try", "text": "scroll the list of items", "where": "the Desk", "do": "scroll the list of items", "expect": "the header stays at the top" }));
    assert_eq!(sent_to(&w, &h).len(), before, "{:?}", sent_to(&w, &h));
}

#[test]
fn done_with_no_steps_gets_one_line_back_and_its_steps_after_are_kept() {
    let (w, h, _, project, id) = at_work();
    ends(&w, &h, &format!("Done.\nDONE: #{id}"));
    let back = sent_to(&w, &h).into_iter().filter(|t| t.contains("check line needs another go")).collect::<Vec<_>>();
    assert_eq!(back.len(), 1, "{back:?}");
    assert!(back[0].contains("it has no check line") && back[0].contains(&format!("CHECK: #{id} | Where:")), "{}", back[0]);
    ends(&w, &h, &format!("CHECK: #{id} | Where: the Desk | Do: scroll | Expect: the header stays put"));
    assert_eq!(kept(&w, &project)["where"], "the Desk");
}

#[test]
fn a_one_line_try_or_desk_words_or_code_names_come_back_once_only() {
    let (w, h, _, project, id) = at_work();
    ends(&w, &h, &format!("Done.\nDONE: #{id}\nCHECK: #{id} | Try: scroll the board"));
    let back = |w: &World| sent_to(w, &h).into_iter().filter(|t| t.contains("check line needs another go")).collect::<Vec<_>>();
    assert_eq!(back(&w).len(), 1);
    assert!(back(&w)[0].contains("one line, not the three steps"), "{}", back(&w)[0]);
    // Its next try still uses Desk words: kept as it is, nothing more sent (one send back per DONE).
    ends(&w, &h, &format!("CHECK: #{id} | Where: the landing | Do: check the commit | Expect: it's on the branch"));
    assert_eq!(kept(&w, &project)["where"], "the landing");
    assert_eq!(back(&w).len(), 1);
}

#[test]
fn the_plain_words_check_finds_desk_words_and_code_names_but_not_everyday_words() {
    use crate::desk::model::Check;
    let c = |at: &str, act: &str, expect: &str| Check { kind: "try".into(), text: act.into(), at: at.into(), act: act.into(), expect: expect.into() };
    let words = crate::desk::check_back_words("9", Some(&c("the Desk", "open the item's branch", "its commit shows"))).unwrap();
    assert!(words.contains("\"branch\"") && words.contains("\"commit\"") && !words.contains("names code"), "{words}");
    for code in [c("deskItemSide", "click", "x"), c("Settings", "run `cargo test`", "x"), c("list.js", "click", "x"), c("Settings", "set check_back", "x"), c("a", "b", "desk::mod")] {
        assert!(crate::desk::check_back_words("9", Some(&code)).is_some_and(|w| w.contains("names code")), "{code:?}");
    }
    for plain in [
        c("the Desk, on eBay or in macOS", "click Looks right, then Send back", "the item goes back to In progress (it's 3:04, e.g. in Settings)"),
        c("Settings → Extensions", "copying text works; turn on Diff", "a Diff tab shows; the hooked-up button works"),
    ] {
        assert_eq!(crate::desk::check_back_words("9", Some(&plain)), None, "{plain:?}");
    }
    assert_eq!(crate::desk::check_back_words("9", Some(&Check { kind: "nothing".into(), text: "a change nobody can see".into(), ..Default::default() })), None);
}
