//! Which landing step a Desk's items are in right now (#783, #814, desk/hub/landing/now_line.rs): Cue's own project
//! says its full run going and the items it carries, a project with steps of its own says which check step runs on
//! which item, and once nothing runs there's nothing (no line over the Desk says what ended: an item's row does).

use super::desk_commit::{ends, item, routed_with};
use super::desk_full_run::{failed, in_cue_passed, lands, looks};
use super::desk_lands::{set, Checker};
use crate::hub::Hub;
use crate::testing::World;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Desk `project`'s step under way as the window gets it.
fn line(hub: &Hub, project: &str) -> Value {
    let v = hub.read(|st| Hub::desk_view(st, 0));
    v["projects"].as_array().unwrap().iter().find(|p| p["project"] == project).map(|p| p["landing_now"].clone()).unwrap_or(Value::Null)
}

#[test]
fn cues_own_desk_says_its_full_run_going_and_the_items_it_carries() {
    let w = World::new();
    let (runner, dir, project, id, h, _start) = in_cue_passed(&w);
    assert_eq!(line(w.hub(), &project), Value::Null, "nothing going");
    let at = lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    looks(&w);
    let l = line(w.hub(), &project);
    assert_eq!((l["state"].as_str(), l["what"].as_str()), (Some("run"), Some(format!("Testing main at {}", &at[..7]).as_str())), "{l}");
    assert_eq!(l["items"], json!([id]), "the items it carries");
    runner.ends_with(failed(None, Some("it failed before this batch")));
    looks(&w);
    assert_eq!(line(w.hub(), &project), Value::Null, "it ended: nothing going");
    // Its build: the item's row says it of itself.
    w.hub().write(|st| {
        let it = st.desk.item_mut(&id).unwrap();
        it.state = "finishing".into();
        it.finish = Some(crate::desk::Finish { step: "building".into(), at: 7, ..Default::default() });
    });
    assert_eq!(line(w.hub(), &project), Value::Null);
}

#[test]
fn a_desk_with_steps_of_its_own_says_which_runs_on_which_item() {
    let w = World::new();
    w.hub().desk_set_installer(Arc::new(Checker::default()));
    let mark = w.home().join("check-began").to_string_lossy().to_string();
    // The check waits till the test has seen it running (or 10 s), so it's read mid-step.
    let go = w.home().join("check-go").to_string_lossy().to_string();
    let wait = format!("touch {mark}; for i in $(seq 1 200); do [ -f {go} ] && break; sleep 0.05; done");
    let lands = json!({ "check": "command", "check_steps": ["true", wait], "after": "command", "after_steps": ["true"] });
    let (_dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, lands.clone()), |_, _, _| {});
    assert_eq!(line(w.hub(), &project), Value::Null, "nothing runs yet");
    let path = item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap().to_string();
    std::fs::write(std::path::Path::new(&path).join("a.txt"), "a\n").unwrap();
    // Read from beside the landing while its steps run.
    let (hub, seen, stop) = (w.hub().clone(), Arc::new(Mutex::new(Value::Null)), Arc::new(AtomicBool::new(false)));
    let watcher = {
        let (seen, stop, project, mark, go) = (seen.clone(), stop.clone(), project.clone(), mark.clone(), go.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                if std::path::Path::new(&mark).exists() {
                    *crate::locking::lock(&seen, "seen line") = line(&hub, &project);
                    std::fs::write(&go, "").unwrap();
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        })
    };
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    stop.store(true, Ordering::SeqCst);
    watcher.join().unwrap();
    let mid = crate::locking::lock(&seen, "seen line").clone();
    assert_eq!((mid["state"].as_str(), mid["what"].as_str(), mid["step"].as_str()), (Some("run"), Some(format!("Checking #{id} before it lands").as_str()), Some(wait.as_str())), "{mid}");
    assert_eq!(mid["items"], json!([id]));
    assert_eq!(line(w.hub(), &project), Value::Null, "all ended: nothing runs");
}

#[test]
fn a_desk_with_no_check_and_nothing_after_landing_has_nothing() {
    let w = World::new();
    let (_dir, project, _id, _h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    assert_eq!(line(w.hub(), &project), Value::Null);
}
