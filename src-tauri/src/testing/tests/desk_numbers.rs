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
