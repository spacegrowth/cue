//! The item step keeps its recipient and exact message across a restart.

use crate::hosted::HostedEvent;
use crate::testing::World;

#[test]
fn a_queued_send_back_keeps_its_delivery_link_after_relaunch() {
    let mut w = World::new();
    let (sid, project) = super::desk::holder(&w);
    let item = w.hub().desk_add(&project, "bug", "Spacing", "", Some(&sid)).unwrap();
    w.hosted_says(&sid, HostedEvent::Taken { text: "Keep working".into() });
    assert_eq!(w.snapshot().session(&sid)["state"], "working");
    w.hub().desk_item_act(&item.id, "send_back", "Fix the spacing").unwrap();
    w.settle();
    let snapshot = w.snapshot();
    let held = snapshot.session(&sid)["held"]["text"].as_str().unwrap().to_string();
    let step = super::desk::desk(&w, &project)["decisions"].as_array().unwrap().iter()
        .find(|d| d["why"].as_str().unwrap_or("").starts_with("sent back")).unwrap().clone();
    assert_eq!(step["delivery"]["session_id"], sid);
    assert_eq!(step["delivery"]["text"], held);
    assert!(step["delivery"]["at_ms"].as_u64().unwrap() > 0);
    w.relaunch();
    let desk = super::desk::desk(&w, &project);
    let restored = desk["decisions"].as_array().unwrap().iter().find(|d| d["id"] == step["id"]).unwrap();
    assert_eq!(restored["delivery"], step["delivery"]);
    assert_eq!(w.snapshot().session(&sid)["held"]["text"], held);
}
