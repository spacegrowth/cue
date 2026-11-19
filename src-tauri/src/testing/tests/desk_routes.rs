//! Every route gets an answer (#248): a ROUTE the Desk lead writes that Cue can't make is answered, all in one
//! message after its turn: "Refused: …" with why and what it can do instead; a line that looks like a route but
//! doesn't read as one is said back. A route made isn't answered (each answer costs the lead a turn).
//! Finishing items: desk_finish.rs.

use super::desk::{desk, ends, holder, item, new_lead, sent_to};
use crate::desk::parse::{lines, route_fix, route_like, unread_routes, Line};
use crate::testing::World;

#[test]
fn a_line_meant_as_a_route_but_not_in_its_form_is_found() {
    let text = "Routing these now.\nROUTE #12 -> web | why: it's theirs\nROUTE: #13 — web | why: x\nroute: #14 to web\nROUTE:\n\
                ROUTE: #15 -> web | why: fine\nROUTE: #16 → web | why: arrow\n`ROUTE: #17 -> web`\nROUTE: #<id> -> <session>\n\
                Route the rest later.\nRoute #3 is blocked on the API.\n- Route #4 handled by web already.\n\
                ```\nROUTE #18 web\n```\n> ROUTE #19 web";
    assert_eq!(unread_routes(text), vec!["ROUTE #12 -> web | why: it's theirs", "ROUTE: #13 — web | why: x", "route: #14 to web", "ROUTE:"]);
    assert!(route_like("- ROUTE: #2 web"));
    assert!(route_like("route #2 to web"));
    assert!(!route_like("Route the rest later."), "plain words aren't a route");
    assert!(!route_like("Route #3 is blocked"), "prose naming an item isn't either");
    assert!(!route_like("Routes: none"));
    assert!(!route_like("ROUTE: <id> -> <name>"), "the form, with its blanks");
    assert_eq!(route_fix("ROUTE #12 -> web"), "put a colon after ROUTE");
    assert_eq!(route_fix("ROUTE: #13 — web"), "put -> between the item and the session");
    assert_eq!(route_fix("route: #14 -> web"), "write ROUTE in capitals");
    assert_eq!(route_fix("1. **Route:** #14 -> web"), "write ROUTE in capitals");
}

#[test]
fn a_route_in_a_numbered_list_is_read_and_one_not_in_capitals_is_said_back() {
    let ids = |text: &str| lines(text).into_iter().filter_map(|l| if let Line::Route { id, to, .. } = l { Some(format!("{id}:{to}")) } else { None }).collect::<Vec<_>>();
    assert_eq!(ids("1. ROUTE: #12 -> web | why: a\n2) ROUTE: #13 -> api\n3. **ROUTE:** #14 -> ios\nRoute: #15 -> web\nroute: #16 -> web"), vec!["12:web", "13:api", "14:ios"]);
    assert_eq!(unread_routes("1. ROUTE: #12 -> web\nRoute: #15 -> web"), vec!["Route: #15 -> web"]);
    assert_eq!(unread_routes("1. ROUTE #12 web"), Vec::<String>::new(), "no colon and no arrow: not taken for a route");
    assert_eq!(unread_routes("1. ROUTE #12 -> web"), vec!["1. ROUTE #12 -> web"]);
}

#[test]
fn routes_from_one_turn_are_answered_in_one_message_only_those_not_made() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let before = sent_to(&w, &d).len();
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header\nROUTE: #99 -> cue-teams | why: old one\nROUTE: #98 -> cue-teams");
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "working");
    let sent = sent_to(&w, &d);
    assert_eq!(sent.len(), before + 1, "one message: {sent:?}");
    let told = sent.last().unwrap();
    assert!(!told.contains("Taken"), "a route made isn't answered: {told}");
    assert!(told.starts_with("Refused: #99 isn't an open item on this Desk. You can") && told.contains("\nRefused: #98 "), "{told}");
}

#[test]
fn a_route_made_sends_the_lead_nothing() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    let before = sent_to(&w, &d).len();
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "working");
    assert_eq!(sent_to(&w, &d).len(), before, "{:?}", sent_to(&w, &d));
}

#[test]
fn a_route_to_a_done_item_is_refused_and_says_what_the_lead_can_do() {
    let w = World::new();
    let (h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    let id = item(&desk(&w, &project), "Header jumps")["id"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | why: one more thing"));
    let told = sent_to(&w, &d).last().cloned().unwrap();
    assert!(told.contains(&format!("Refused: #{id} is done. You can ask the user to reopen it")), "{told}");
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "done");
}

#[test]
fn a_line_that_looks_like_a_route_but_doesnt_read_is_said_back_each_time() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put");
    let id = item(&desk(&w, &project), "Header jumps")["id"].as_str().unwrap().to_string();
    let before = sent_to(&w, &d).len();
    let line = format!("ROUTE #{id} -> cue-teams | why: it made the header");
    let back = format!("Couldn't read this line as a route: {line}\nWrite it as ROUTE: #<id> -> <session> | why: <reason> (put a colon after ROUTE).");
    ends(&w, &d, &format!("Sending it on.\n{line}"));
    assert_eq!(sent_to(&w, &d)[before..], [back.clone()]);
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "new", "nothing routed");
    // Said again in its next turn: answered again, as it's still not made.
    ends(&w, &d, &line);
    assert_eq!(sent_to(&w, &d)[before..], [back.clone(), back]);
    // Written right, it's made, and nothing's said.
    ends(&w, &d, &format!("ROUTE: #{id} -> cue-teams | why: it made the header"));
    assert_eq!(sent_to(&w, &d).len(), before + 2);
    assert_eq!(item(&desk(&w, &project), "Header jumps")["state"], "working");
}

#[test]
fn the_same_refused_route_said_again_is_answered_again() {
    let w = World::new();
    let (_h, project) = holder(&w);
    let d = new_lead(&w, &project);
    for n in 1..=2 {
        ends(&w, &d, "ROUTE: #99 -> cue-teams | why: old one\nROUTE: #new -> cue-teams | why: no item");
        let sent = sent_to(&w, &d);
        let told = sent.last().unwrap();
        assert!(told.starts_with("Refused: #99") && told.contains("\nRefused: ROUTE #new has no ITEM line"), "turn {n}: {told}");
    }
}
