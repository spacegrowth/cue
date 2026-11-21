//! Waits and LAND FIRST across Desks on the same code (#540, docs/desk-coordination-plan.md option B, step 3): an item
//! may be queued `after:` another Desk's item on the same repository, and starts when that one is done; LAND FIRST may
//! move up another Desk's item in the landing line. Both are refused for a Desk on other code. The waits go in the one
//! waits list, and its circle check covers a circle across two Desks.

use super::desk::{desk, ends, holder, item, new_lead, sent_to, summary};
use crate::desk::parse::{lines, Line};
use crate::testing::World;

/// A new independent Desk named `name` on folder `folder`, with its lead: (Desk, lead).
fn start_in(w: &World, folder: &std::path::Path, name: &str) -> (String, String) {
    let r = crate::ipc::desk_start_named(w.hub(), &folder.to_string_lossy(), true, Some(name)).unwrap();
    (r["id"].as_str().unwrap().into(), r["sid"].as_str().unwrap().into())
}

fn start(w: &World, name: &str) -> (String, String) {
    start_in(w, w.home(), name)
}

/// A Desk on another repository (its own, though its folder is under the home folder, a repository in some tests):
/// (Desk, lead).
fn elsewhere(w: &World) -> (String, String) {
    let other = w.home().join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    super::team::git(&other, &["init", "-q", "-b", "main"]);
    start_in(w, &other, "Elsewhere")
}

fn id_of(w: &World, d: &str, title: &str) -> String {
    item(&desk(w, d), title)["id"].as_str().unwrap().to_string()
}

/// What lead `lead` was last sent.
fn last(w: &World, lead: &str) -> String {
    sent_to(w, lead).last().cloned().unwrap_or_default()
}

fn decisions(w: &World, d: &str) -> Vec<String> {
    desk(w, d)["decisions"].as_array().unwrap().iter().map(|x| x["text"].as_str().unwrap_or("").to_string()).collect()
}

/// A holder's Desk (cue-teams at work on "Header", a) and an independent Desk "Loops" on the same folder, whose lead
/// queues "Footer" (b) for a new session after a: (holder, Desk A, Desk B, lead B, a, b).
fn queued_across(w: &World, after: impl Fn(&str) -> String) -> (String, String, String, String, String, String) {
    let (h, pa) = holder(w);
    let la = new_lead(w, &pa);
    ends(w, &la, "ITEM: Header\nROUTE: #new -> cue-teams | why: it knows the header");
    let a = id_of(w, &pa, "Header");
    ends(w, &h, "On it.");
    let (pb, lb) = start(w, "Loops");
    ends(w, &lb, &format!("ITEM: Footer\nROUTE: #new -> new | after: {} | why: same file", after(&a)));
    let b = id_of(w, &pb, "Footer");
    let it = item(&desk(w, &pb), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "{it} / {}", last(w, &lb));
    (h, pa, pb, lb, a, b)
}

#[test]
fn after_and_land_first_read_another_desks_item() {
    let after = |l: &str| match lines(l).as_slice() {
        [Line::Route { after, .. }] => after.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(after("ROUTE: #12 -> new | after: cue#509 | why: x").as_deref(), Some("cue#509"));
    assert_eq!(after("ROUTE: #12 -> new | after: Loops #509").as_deref(), Some("loops#509"));
    assert_eq!(after("ROUTE: #12 -> new | after: #509").as_deref(), Some("509"));
    assert_eq!(lines("LAND FIRST: loops#533 | why: 20 min, before the move"), [Line::Unstick { what: "land_first".into(), id: "loops#533".into(), commit: String::new(), why: "20 min, before the move".into() }]);
    assert!(lines("START: loops#533 | why: x").is_empty(), "only LAND FIRST names another Desk's item");
}

/// An item queued after another Desk's item on the same code starts when that one is done (Looks right, #576), as
/// within one Desk; the waits list of both Desks shows the wait, naming the other Desk.
#[test]
fn an_item_queued_after_another_desks_item_starts_when_it_is_done() {
    let w = World::new();
    let (h, pa, pb, _, a, b) = queued_across(&w, |a| format!("#{a}"));
    let wb = &desk(&w, &pb)["waits"][0];
    assert_eq!((wb["item"].as_str(), wb["on"].as_str(), wb["kind"].as_str(), wb["item_desk"].as_str()), (Some(b.as_str()), Some(a.as_str()), Some("queued"), Some("")), "{wb}");
    let a_name = desk(&w, &pa)["name"].as_str().map(str::to_string).unwrap_or_else(|| w.hub().read(|st| st.desk.name_of(&pa).to_string()));
    assert_eq!(wb["on_desk"].as_str(), Some(a_name.as_str()), "{wb}");
    let wa = desk(&w, &pa)["waits"].as_array().unwrap().iter().find(|x| x["item"] == b.as_str()).cloned().unwrap_or_default();
    assert_eq!((wa["on"].as_str(), wa["item_desk"].as_str()), (Some(a.as_str()), Some("Loops")), "the Desk waited on sees it too: {}", desk(&w, &pa)["waits"]);
    ends(&w, &h, &format!("DONE: #{a}"));
    assert_eq!(item(&desk(&w, &pb), "Footer")["after"], a.as_str(), "DONE isn't enough");
    w.hub().desk_item_act(&a, "looks_right", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &pb), "Footer").clone();
    assert!(it["state"] != "queued" && it["after"].is_null(), "{it}");
}

/// Refused for a Desk on another repository, and for a Desk's name that isn't the item's: plainly, with what to do.
#[test]
fn after_another_desks_item_is_refused_on_other_code_or_a_wrong_desk_name() {
    let w = World::new();
    let (pa, la) = start(&w, "Feature A");
    ends(&w, &la, "ITEM: Header");
    let a = id_of(&w, &pa, "Header");
    let (pc, lc) = elsewhere(&w);
    ends(&w, &lc, &format!("ITEM: Footer\nROUTE: #new -> new | after: #{a} | why: same file"));
    let c = id_of(&w, &pc, "Footer");
    assert_eq!(last(&w, &lc), format!("Refused: #{c} can't wait after #{a}: #{a} is on Feature A's Desk, which works on other code. You can route #{c} without after:, or ask the user (ASK: #{c} | …)."));
    let it = item(&desk(&w, &pc), "Footer").clone();
    assert!(it["state"] == "new" && it["after"].is_null(), "{it}");
    let (pb, lb) = start(&w, "Loops");
    ends(&w, &lb, &format!("ITEM: Logo\nROUTE: #new -> new | after: elsewhere#{a}"));
    let l = id_of(&w, &pb, "Logo");
    assert_eq!(last(&w, &lb), format!("Refused: #{l} can't wait after #{a}: #{a} is on Feature A's Desk, not elsewhere's. You can route #{l} without after:, or ask the user (ASK: #{l} | …)."));
    ends(&w, &lb, &format!("ITEM: Menu\nROUTE: #new -> new | after: Feature A#{a}"));
    assert_eq!(item(&desk(&w, &pb), "Menu")["after"], a.as_str(), "its own Desk's name, as written: {}", last(&w, &lb));
}

/// A circle across two Desks: a after b on one, b after a on the other. The second `after` is refused as within one
/// Desk ("waits on it"); one that's there already (saved before the check) is found by the circle check, shown on both
/// Desks, and both leads are told.
#[test]
fn a_circle_across_two_desks_is_refused_and_found() {
    let w = World::new();
    let (pa, la) = start(&w, "Feature A");
    let (pb, lb) = start(&w, "Feature B");
    ends(&w, &la, "ITEM: One");
    ends(&w, &lb, "ITEM: Two");
    let (x, y) = (id_of(&w, &pa, "One"), id_of(&w, &pb, "Two"));
    ends(&w, &la, &format!("ROUTE: #{x} -> new | after: #{y} | why: same files"));
    assert_eq!(item(&desk(&w, &pa), "One")["after"], y.as_str());
    ends(&w, &lb, &format!("ROUTE: #{y} -> new | after: #{x} | why: same files"));
    assert!(item(&desk(&w, &pb), "Two")["after"].is_null());
    assert!(summary(&w, &pb).contains(&format!("Cue couldn't queue #{y} after #{x}, which waits on it")), "{}", summary(&w, &pb));
    // One saved before the check: the waits list's circle check finds it.
    w.hub().desk_put_in_state(&y, "new");
    w.hub().desk_queue_after_unchecked(&y, &x);
    w.hub().circle_check();
    w.settle();
    for (d, lead) in [(&pa, &la), (&pb, &lb)] {
        let c = &desk(&w, d)["circles"];
        assert_eq!(c.as_array().map(Vec::len), Some(1), "{c}");
        assert_eq!(c[0]["items"].as_array().unwrap().len(), 2, "{c}");
        assert!(sent_to(&w, lead).iter().any(|t| t.contains("Circle on the Desk")), "lead told: {:?}", sent_to(&w, lead));
    }
}

/// LAND FIRST on another Desk's item on the same code moves it up in the landing line, as within one Desk, noted on
/// both Desks; on other code it's refused.
#[test]
fn land_first_moves_up_another_desks_item_on_the_same_code() {
    let w = World::new();
    let (dir, pa, a, h) = super::desk_commit::routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let copy = std::path::PathBuf::from(item(&desk(&w, &pa), "Header jumps")["tree"]["path"].as_str().unwrap().to_string());
    std::fs::write(copy.join("a.txt"), "a\n").unwrap();
    // The main checkout mid-merge: it waits in the landing line.
    std::fs::write(dir.join(".git/MERGE_HEAD"), super::team::git(&dir, &["rev-parse", "HEAD"])).unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{a}"));
    w.advance(1000);
    assert_eq!(item(&desk(&w, &pa), "Header jumps")["finish"]["step"], "line", "{}", item(&desk(&w, &pa), "Header jumps"));
    let (_, lc) = elsewhere(&w);
    ends(&w, &lc, &format!("LAND FIRST: #{a} | why: mine first"));
    let a_name = w.hub().read(|st| st.desk.name_of(&pa).to_string());
    let want = format!("Refused: LAND FIRST #{a}: #{a} is on {a_name}'s Desk, which works on other code. You can move up an item on this repository, or ask the user (ASK: #{a} | …).");
    assert!(sent_to(&w, &lc).contains(&want), "{:?}", sent_to(&w, &lc));
    let (pb, lb) = start_in(&w, &dir, "Loops");
    ends(&w, &lb, &format!("LAND FIRST: {a_name}#{a} | why: 20 min, before the move"));
    let first = w.hub().read(|st| st.desk.item(&a).and_then(|i| i.finish.as_ref().map(|f| f.first)));
    assert!(first.unwrap_or(0) > 0, "moved up: {first:?} {}", last(&w, &lb));
    assert!(decisions(&w, &pa).contains(&format!("Loops's lead moved #{a} up in the landing line")), "{:?}", decisions(&w, &pa));
    assert!(decisions(&w, &pb).contains(&format!("Moved {a_name} #{a} up in the landing line")), "{:?}", decisions(&w, &pb));
    std::fs::remove_file(dir.join(".git/MERGE_HEAD")).unwrap();
}

/// A wait across Desks is kept over a relaunch, and the item still starts when the one it waits on is gone.
#[test]
fn a_wait_across_desks_survives_a_relaunch() {
    let mut w = World::new();
    let (_, _, pb, _, a, b) = queued_across(&w, |a| format!("#{a}"));
    w.relaunch();
    let it = item(&desk(&w, &pb), "Footer").clone();
    assert_eq!((it["state"].as_str(), it["after"].as_str()), (Some("queued"), Some(a.as_str())), "{it}");
    assert_eq!(desk(&w, &pb)["waits"][0]["item"], b.as_str());
    w.hub().desk_item_act(&a, "drop", "").unwrap();
    w.settle();
    let it = item(&desk(&w, &pb), "Footer").clone();
    assert!(it["state"] != "queued" && it["after"].is_null(), "{it}");
}
