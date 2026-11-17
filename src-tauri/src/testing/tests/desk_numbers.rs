//! Desk item ids (desk/model.rs new_id): plain numbers, #1, #2…, one sequence for every project's Desk.

use crate::testing::World;

#[test]
fn items_are_numbered_1_2_3_across_projects_and_a_dropped_number_isnt_given_again() {
    let mut w = World::new();
    let a = w.hub().desk_add("web", "bug", "Header jumps", "", None).unwrap();
    let b = w.hub().desk_add("api", "task", "Footer", "", None).unwrap();
    assert_eq!((a.id.as_str(), b.id.as_str()), ("1", "2"), "plain numbers, shared by every project's Desk");
    w.hub().desk_item_act(&b.id, "drop", "").unwrap();
    assert_eq!(w.hub().desk_add("web", "task", "Menu", "", None).unwrap().id, "3");
    w.relaunch();
    assert_eq!(w.hub().desk_add("web", "task", "Logo", "", None).unwrap().id, "4", "on from the highest kept");
    assert_eq!(crate::desk::next_number(["d7k2", "9", "x1"].into_iter(), 0), 10, "older random ids are passed over");
}

#[test]
fn items_from_before_numbering_are_numbered_oldest_first_and_keep_their_old_tag() {
    use crate::desk::DeskItem;
    let mut w = World::new();
    let item = |id: &str, created: u64, after: Option<&str>| DeskItem {
        id: id.into(), project: "web".into(), kind: "bug".into(), title: id.into(), state: "queued".into(),
        after: after.map(Into::into), created, updated: created, from: "you".into(), ..Default::default()
    };
    crate::db::desk_put_item(&item("zz9a", 300, Some("d7k2")));
    crate::db::desk_put_item(&item("d7k2", 100, None));
    crate::db::desk_put_item(&item("4", 200, None));
    w.relaunch();
    let got = |w: &mut World| {
        let d = super::desk::desk(w, "web");
        let s = |v: &serde_json::Value| v.as_str().map(str::to_string);
        let mut v: Vec<_> = d["items"].as_array().unwrap().iter().map(|i| (s(&i["id"]).unwrap(), s(&i["alias"]).unwrap_or_default(), s(&i["after"]))).collect();
        v.sort();
        v
    };
    let want = vec![
        ("4".to_string(), String::new(), None),
        ("5".to_string(), "d7k2".to_string(), None),
        ("6".to_string(), "zz9a".to_string(), Some("5".to_string())),
    ];
    assert_eq!(got(&mut w), want, "oldest first, after the highest number; what waited on an old tag waits on its number");
    let mut desk = crate::desk::Desk::default();
    desk.items = vec![item("d7k2", 100, None)];
    desk.number_old();
    assert_eq!((desk.item("d7k2").map(|i| i.id.as_str()), desk.canon("d7k2"), desk.canon("zz")), (Some("1"), "1".to_string(), "zz".to_string()), "the old tag still finds it");
    w.relaunch();
    assert_eq!(got(&mut w), want, "saved under the numbers, numbered once");
    assert_eq!(w.hub().desk_add("web", "task", "Next", "", None).unwrap().id, "7");
}

#[test]
fn done_items_page_newest_first_ten_at_a_time_and_only_that_projects() {
    use crate::desk::DeskItem;
    let w = World::new();
    let item = |id: u64, project: &str, state: &str| DeskItem {
        id: id.to_string(), project: project.into(), kind: "task".into(), title: format!("t{id}"), state: state.into(),
        created: id, updated: 1_000 + id, from: "you".into(), ..Default::default()
    };
    for n in 1..=25 { crate::db::desk_put_item(&item(n, "web", "done")); }
    crate::db::desk_put_item(&item(26, "web", "queued"));
    crate::db::desk_put_item(&item(27, "api", "done"));
    let ids = |v: &[DeskItem]| v.iter().map(|i| i.id.parse::<u64>().unwrap()).collect::<Vec<_>>();
    let first = w.hub().desk_done_page("web", None, 10);
    assert_eq!(ids(&first), (16..=25).rev().collect::<Vec<_>>(), "the newest ten, newest first; not open ones, not another project's");
    let last = first.last().unwrap();
    let next = w.hub().desk_done_page("web", Some((last.updated * 1000, last.id.as_str())), 10);
    assert_eq!(ids(&next), (6..=15).rev().collect::<Vec<_>>(), "then the ten before them");
    let end = next.last().unwrap();
    assert_eq!(ids(&w.hub().desk_done_page("web", Some((end.updated * 1000, end.id.as_str())), 10)), (1..=5).rev().collect::<Vec<_>>());
}
