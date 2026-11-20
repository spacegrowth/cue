//! Steps held while you're away (desk/hub/away/away_holds.rs): with Away on, a session's delete or push is held without stopping
//! it; the Desk lists each in plain words; once you answer, the session is sent them in the order it asked, and only
//! what you allowed is let through, once. Nothing held runs before that.

use super::desk::{desk, holder, new_lead, sent_to};
use crate::hub::HELD_WORDS;
use crate::testing::World;
use serde_json::{json, Value};

fn held(w: &World, project: &str) -> Vec<Value> {
    desk(w, project)["held"].as_array().cloned().unwrap_or_default()
}

#[test]
fn away_holds_deletes_and_pushes_and_runs_only_what_you_allow_in_order() {
    let w = World::new();
    let (h, project) = holder(&w);
    new_lead(&w, &project);
    w.hub().ask_before_set(&project, Some(&["main".into(), "release".into(), "data".into()])).unwrap();
    let s = w.session(&h).in_folder(w.home());
    w.hub().desk_away_set(&project, &json!({ "on": true, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();

    // Held, not asked: told at once to carry on; no card, the session not stopped.
    let rm = s.guard_bash("rm -rf build", "main").expect("checked");
    let told = rm.told().expect("answered at once");
    assert_eq!((told["behavior"].clone(), told["message"].clone()), (json!("deny"), json!(HELD_WORDS)), "{told}");
    let push = s.guard_bash("git push origin main", "main").expect("checked");
    assert_eq!(push.told().unwrap()["behavior"], "deny");
    let snap = w.snapshot();
    assert!(snap.cards_of(&h).iter().all(|c| c["one_way"] != json!(true)), "no card");
    assert_ne!(snap.session(&h)["state"], "deciding");
    // Asked again before you answer: still one entry.
    assert_eq!(s.guard_bash("rm -rf build", "main").unwrap().told().unwrap()["behavior"], "deny");
    let list = held(&w, &project);
    assert_eq!(list.iter().map(|x| (x["what"].as_str().unwrap(), x["command"].as_str().unwrap())).collect::<Vec<_>>(), vec![("delete files", "rm -rf build"), ("push", "git push origin main")], "{list:?}");
    assert!(sent_to(&w, &h).iter().all(|t| !t.contains("The owner answered")), "nothing runs while away");

    // You answer the later one first: it waits for the earlier one, so the order holds.
    let (rm_id, push_id) = (list[0]["id"].as_str().unwrap().to_string(), list[1]["id"].as_str().unwrap().to_string());
    w.hub().desk_held_answer(&push_id, true).unwrap();
    w.settle();
    assert!(sent_to(&w, &h).iter().all(|t| !t.contains("The owner answered")), "the earlier step isn't answered yet");
    assert_eq!(held(&w, &project).len(), 1);
    w.hub().desk_held_answer(&rm_id, false).unwrap();
    w.settle();
    let words = sent_to(&w, &h).into_iter().find(|t| t.contains("The owner answered")).expect("told once both are answered");
    let (a, b) = (words.find("rm -rf build").unwrap(), words.find("git push origin main").unwrap());
    assert!(a < b && words.contains("1. NO") && words.contains("2. ALLOWED"), "{words}");
    assert!(held(&w, &project).is_empty());
    assert!(w.hub().desk_held_answer(&rm_id, true).is_err(), "answered once");

    // Only what you allowed is let through, once; what you refused is held again.
    assert_eq!(s.guard_bash("git push origin main", "main").unwrap().told().unwrap()["behavior"], "allow");
    assert_eq!(s.guard_bash("rm -rf build", "main").unwrap().told().unwrap()["behavior"], "deny");
    assert_eq!(s.guard_bash("git push origin main", "main").unwrap().told().unwrap()["behavior"], "deny", "once");

    // Back from Away: Ask me before is a card again; what's still held stays listed.
    w.hub().desk_away_set(&project, &json!({ "on": false, "until": null, "budget": null, "rules": {} })).unwrap();
    w.settle();
    let ask = s.guard_bash("git push origin HEAD:main", "crew/x").expect("asked");
    assert!(ask.told().is_none(), "waits on a card");
    assert_eq!(held(&w, &project).len(), 2, "rm and push held earlier, not answered yet");
}
