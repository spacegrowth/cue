//! Holds checked against what copies really changed (#540, docs/desk-coordination-plan.md option B, step 4): as a
//! hold is taken, both leads hear which open items on other Desks already changed a held file; at landing, an item
//! from another Desk whose diff touches a held file waits in the line, plainly and asking nobody, and lands by itself
//! once the hold lifts (UNHOLD, or the holder's item landing). Items not touching the area, and the holder's own
//! Desk's, land as ever.

use super::desk::sent_to;
use super::desk_commit::{ends, item, routed_with};
use super::team::git;
use crate::testing::World;
use std::path::PathBuf;

/// "Header jumps" (`id`) of Desk `a` at work in its own copy, its holder `h` having changed a.txt there; a second
/// Desk "Loops" (`b`, its lead `lb`) on the same repository: (repo, a, id, h, b, lb).
fn two_desks(w: &World) -> (PathBuf, String, String, String, String, String) {
    let (dir, a, id, h) = routed_with(w, |_, _, _| {}, |_, _, _| {});
    let path = PathBuf::from(item(w, &a, "Header jumps")["tree"]["path"].as_str().unwrap().to_string());
    std::fs::write(path.join("a.txt"), "a\n").unwrap();
    let r = crate::ipc::desk_start_named(w.hub(), &dir.to_string_lossy(), true, Some("Loops")).unwrap();
    let (b, lb) = (r["id"].as_str().unwrap().to_string(), r["sid"].as_str().unwrap().to_string());
    (dir, a, id, h, b, lb)
}

/// Loops' lead adds `title` and holds `area` for it: its id.
fn hold(w: &World, b: &str, lb: &str, title: &str, area: &str) -> String {
    ends(w, lb, &format!("ITEM: task | {title}\nHOLD: #new | area: {area} | till: it lands | why: the move"));
    item(w, b, title)["id"].as_str().unwrap().to_string()
}

fn name(w: &World, d: &str) -> String {
    w.hub().read(|st| st.desk.name_of(d).to_string())
}

/// What lead `lead` was last told back as Held.
fn held_answer(w: &World, lead: &str) -> String {
    sent_to(w, lead).into_iter().rev().find(|t| t.starts_with("Held:")).unwrap_or_default()
}

/// The bands Desk `to` was sent, in order.
fn bands_to(w: &World, to: &str) -> Vec<String> {
    w.hub().read(|st| st.desk.bands.iter().filter(|b| b.to == to).map(|b| b.text.clone()).collect())
}

fn subject(dir: &std::path::Path) -> String {
    git(dir, &["log", "-1", "--format=%s"]).trim().to_string()
}

#[test]
fn a_hold_lists_the_items_on_other_desks_that_already_changed_a_held_file_to_both_leads() {
    let w = World::new();
    let (_, a, id, _, b, lb) = two_desks(&w);
    let an = name(&w, &a);
    let x = hold(&w, &b, &lb, "Move the files", "a.txt, ui/desk/");
    let said = held_answer(&w, &lb);
    assert!(said.ends_with(&format!(" Told {an}. 1 item there already changed a held file: {an} #{id} a.txt.")), "{said}");
    let band = bands_to(&w, &a).pop().unwrap();
    assert_eq!(band, format!("Loops holds a.txt, ui/desk/ for #{x} till it lands (the move). Yours already changed a held file: #{id} a.txt."));
    // An item that changed nothing held isn't named.
    let y = hold(&w, &b, &lb, "Move the sources", "src/, *.md");
    let said = held_answer(&w, &lb);
    assert!(said.contains(&format!("#{y} holds src/, *.md")) && said.ends_with(&format!(" Told {an}. No item on another Desk has changed a held file yet.")), "{said}");
    let band = bands_to(&w, &a).pop().unwrap();
    assert_eq!(band, format!("Loops holds src/, *.md for #{y} till it lands (the move)"));
}

#[test]
fn an_item_on_another_desk_touching_a_held_file_waits_at_landing_and_lands_once_unheld() {
    let w = World::new();
    let (dir, a, id, h, b, lb) = two_desks(&w);
    let x = hold(&w, &b, &lb, "Move the files", "*.txt");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &a, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"]["step"].as_str()), (Some("finishing"), Some("line")), "{it}");
    assert_eq!(it["finish"]["wait"].as_str(), Some(format!("Waits for Loops #{x}'s hold on a.txt").as_str()));
    assert!(it["finish"]["question"].as_str().unwrap_or("").is_empty() && it["ask"].is_null(), "not a stop, nobody asked: {it}");
    assert_eq!(subject(&dir), "start", "nothing landed");
    super::desk_commit::tick(&w);
    assert_eq!(item(&w, &a, "Header jumps")["finish"]["step"], "line", "still waits on the tick");
    // Main moved meanwhile: it's brought up to it as it lands.
    std::fs::write(dir.join("shared.txt"), "meanwhile\n").unwrap();
    git(&dir, &["add", "shared.txt"]);
    git(&dir, &["commit", "-q", "-m", "meanwhile"]);
    ends(&w, &lb, &format!("UNHOLD: #{x}"));
    w.advance(1000);
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"), "{}", item(&w, &a, "Header jumps"));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s", "HEAD~1"]).trim(), "meanwhile");
    assert_eq!(item(&w, &a, "Header jumps")["state"], "to_check");
}

#[test]
fn a_waiting_item_lands_once_the_holders_item_lands() {
    let w = World::new();
    let (dir, a, id, h, b, lb) = two_desks(&w);
    ends(&w, &lb, "ITEM: task | Move the files\nROUTE: #new -> new | why: a fresh session");
    let x = item(&w, &b, "Move the files")["id"].as_str().unwrap().to_string();
    ends(&w, &lb, &format!("HOLD: #{x} | area: a.txt | till: it lands"));
    let moved = item(&w, &b, "Move the files");
    let (hx, px) = (moved["holder"]["sid"].as_str().unwrap().to_string(), PathBuf::from(moved["tree"]["path"].as_str().unwrap_or_else(|| panic!("no copy: {moved}"))));
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(item(&w, &a, "Header jumps")["finish"]["wait"].as_str(), Some(format!("Waits for Loops #{x}'s hold on a.txt").as_str()));
    std::fs::write(px.join("b.txt"), "b\n").unwrap();
    ends(&w, &hx, &format!("Moved.\nDONE: #{x}"));
    w.advance(1000);
    assert_eq!(git(&dir, &["log", "-2", "--format=%s"]).trim(), format!("Header jumps (#{id})\nMove the files (#{x})"), "the holder's first, then the one that waited");
    assert_eq!(item(&w, &a, "Header jumps")["state"], "to_check");
    assert!(w.hub().read(|st| crate::desk::holds::holds_on(st, &b).is_empty()), "the hold ended as its item landed");
}

#[test]
fn an_item_on_another_desk_not_touching_the_area_lands_and_the_holders_own_desk_isnt_held_back() {
    let w = World::new();
    let (dir, a, id, h, b, lb) = two_desks(&w);
    hold(&w, &b, &lb, "Move the files", "b.txt, ui/desk/");
    // Its own Desk holds a.txt too, for another of its items: that doesn't stop it.
    let lead_a = w.hub().read(|st| st.desk.lead.get(&a).map(|l| l.sid.clone())).unwrap();
    ends(&w, &lead_a, "ITEM: task | Rename the header\nHOLD: #new | area: a.txt | till: it lands");
    assert!(held_answer(&w, &lead_a).contains("No item on another Desk has changed a held file yet."), "{}", held_answer(&w, &lead_a));
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(subject(&dir), format!("Header jumps (#{id})"), "{}", item(&w, &a, "Header jumps"));
}
