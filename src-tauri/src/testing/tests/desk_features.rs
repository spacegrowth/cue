//! Big features (#282): the lead's FEATURE: line puts items under one feature, the Desk lists them under its name
//! with how many are done, and the feature is done once all of its pieces are.

use super::desk::{desk, ends, holder, item, new_lead, sent_to, summary};
use crate::desk::parse::{lines, Line};
use crate::testing::World;

#[test]
fn feature_lines_are_read() {
    assert_eq!(
        lines("FEATURE: How work lands | #246 #247, #248 #246\nFEATURE: #12 | #13 #new #12\nFEATURE: no pieces"),
        vec![
            Line::Feature { id: None, name: "How work lands".into(), pieces: vec!["246".into(), "247".into(), "248".into()] },
            Line::Feature { id: Some("12".into()), name: String::new(), pieces: vec!["13".into(), "new".into()] },
        ]
    );
}

#[test]
fn a_features_pieces_show_under_it_and_it_is_done_when_they_all_are() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let ids: Vec<String> = ["Step one", "Step two", "Step three"].iter().map(|t| w.hub().desk_add_with(&project, "task", t, "", "", None, &[]).unwrap().id).collect();
    w.hub().desk_item_act(&ids[0], "looks_right", "").unwrap();
    ends(&w, &l, &format!("FEATURE: How work lands | #{} #{} #{} #nope", ids[0], ids[1], ids[2]));
    let d = desk(&w, &project);
    let f = item(&d, "How work lands");
    let fid = f["id"].as_str().unwrap().to_string();
    assert_eq!((f["kind"].as_str(), f["state"].as_str()), (Some("feature"), Some("queued")));
    let pieces: Vec<&str> = f["pieces"].as_array().unwrap().iter().map(|p| p["title"].as_str().unwrap()).collect();
    assert_eq!(pieces, ["Step one", "Step two", "Step three"]);
    for t in ["Step one", "Step two", "Step three"] {
        assert_eq!(item(&d, t)["part_of"], fid.as_str());
    }
    // The piece it couldn't find is told back; the lead's Desk block says the feature and its pieces.
    assert!(sent_to(&w, &l).iter().any(|t| t.contains("#nope (no such item)")), "{:?}", sent_to(&w, &l));
    let s = summary(&w, &project);
    assert!(s.contains("feature: 1 of 3 done") && s.contains(&format!("part of #{fid}")), "{s}");
    // Said again with the same name: the same feature, no second one.
    ends(&w, &l, &format!("FEATURE: How work lands | #{}", ids[2]));
    assert_eq!(desk(&w, &project)["items"].as_array().unwrap().iter().filter(|i| i["kind"] == "feature").count(), 1);
    // Done once all its pieces are, and open again when one is.
    w.hub().desk_item_act(&ids[1], "looks_right", "").unwrap();
    assert_eq!(item(&desk(&w, &project), "How work lands")["state"], "queued");
    w.hub().desk_item_act(&ids[2], "looks_right", "").unwrap();
    assert_eq!(item(&desk(&w, &project), "How work lands")["state"], "done");
    w.hub().desk_item_act(&ids[2], "reopen", "").unwrap();
    assert_eq!(item(&desk(&w, &project), "How work lands")["state"], "queued");
}

#[test]
fn a_feature_cant_be_a_piece_and_a_piece_moves_to_its_new_feature() {
    let w = World::new();
    let (_, project) = holder(&w);
    let l = new_lead(&w, &project);
    let a = w.hub().desk_add_with(&project, "feature", "Big A", "", "", None, &[]).unwrap().id;
    let b = w.hub().desk_add_with(&project, "feature", "Big B", "", "", None, &[]).unwrap().id;
    let p = w.hub().desk_add_with(&project, "task", "Piece", "", "", None, &[]).unwrap().id;
    ends(&w, &l, &format!("FEATURE: #{a} | #{p}"));
    assert_eq!(item(&desk(&w, &project), "Piece")["part_of"], a.as_str());
    ends(&w, &l, &format!("FEATURE: #{b} | #{p} #{a}"));
    let d = desk(&w, &project);
    assert_eq!(item(&d, "Piece")["part_of"], b.as_str());
    assert!(item(&d, "Big A")["part_of"].is_null());
    assert!(item(&d, "Big A")["pieces"].is_null());
    assert!(sent_to(&w, &l).iter().any(|t| t.contains("it's a feature itself")));
}
