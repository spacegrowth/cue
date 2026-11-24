//! The line at the top of each Desk (#783, desk/hub/landing/now_line.rs): what Cue is doing with landed work, in that
//! Desk's own landing steps. Cue's own project says its full run (the commit, then whether it failed and on what),
//! a project with steps of its own says which runs and how the last ended, and one with none says nothing.

use super::desk_commit::{ends, item, routed_with};
use super::desk_full_run::{failed, in_cue_passed, lands, looks};
use super::desk_lands::{set, Checker};
use crate::hub::Hub;
use crate::testing::World;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Desk `project`'s top line as the window gets it.
fn line(hub: &Hub, project: &str) -> Value {
    let v = hub.read(|st| Hub::desk_view(st, 0));
    v["projects"].as_array().unwrap().iter().find(|p| p["project"] == project).map(|p| p["landing_now"].clone()).unwrap_or(Value::Null)
}

#[test]
fn cues_own_desk_says_its_full_run_the_items_it_carries_and_its_failure() {
    let w = World::new();
    let (runner, dir, project, id, h, start) = in_cue_passed(&w);
    let l = line(w.hub(), &project);
    assert_eq!((l["state"].as_str(), l["what"].as_str()), (Some("ok"), Some(format!("Tests passed on main at {}", &start[..7]).as_str())), "{l}");
    let at = lands(&w, &dir, &project, "Header jumps", &id, &h, "a.txt");
    looks(&w);
    let l = line(w.hub(), &project);
    assert_eq!(l["what"], format!("Testing main at {}", &at[..7]), "{l}");
    assert_eq!(l["items"], json!([id]), "the items it carries");
    assert!(l["since"].as_u64().unwrap() > 0, "when it started: {l}");
    runner.ends_with(failed(None, Some("it failed before this batch")));
    looks(&w);
    let l = line(w.hub(), &project);
    assert_eq!((l["state"].as_str(), l["what"].as_str()), (Some("fail"), Some("Failed: hub::tests::header_stays and 1 more")), "{l}");
    assert!(l["note"].as_str().unwrap().contains(&format!("on {}", &at[..7])), "{l}");
    assert!(l["note"].as_str().unwrap().contains("no item caused it"), "{l}");
    assert!(!l["note"].as_str().unwrap().contains("older commit"), "main is still there: {l}");
    // Its build, then its install, with the items in it; the full run's failure behind them.
    w.hub().write(|st| {
        let it = st.desk.item_mut(&id).unwrap();
        it.state = "finishing".into();
        it.finish = Some(crate::desk::Finish { step: "building".into(), at: 7, ..Default::default() });
    });
    let l = line(w.hub(), &project);
    assert_eq!((l["state"].as_str(), l["what"].as_str(), l["since"].as_u64()), (Some("run"), Some("Building Cue"), Some(7)), "{l}");
    assert_eq!(l["items"], json!([id]));
    w.hub().write(|st| st.desk.item_mut(&id).unwrap().finish.as_mut().unwrap().step = "installing".into());
    assert_eq!(line(w.hub(), &project)["what"], "Installing Cue");
}

#[test]
fn a_desk_with_steps_of_its_own_says_which_runs_and_how_the_last_ended() {
    let w = World::new();
    w.hub().desk_set_installer(Arc::new(Checker::default()));
    let mark = w.home().join("check-began").to_string_lossy().to_string();
    // The check waits till the test has seen it running (or 10 s), so the line is read mid-step.
    let go = w.home().join("check-go").to_string_lossy().to_string();
    let wait = format!("touch {mark}; for i in $(seq 1 200); do [ -f {go} ] && break; sleep 0.05; done");
    let lands = json!({ "check": "command", "check_steps": ["true", wait], "after": "command", "after_steps": ["true"] });
    let (_dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, lands.clone()), |_, _, _| {});
    let idle = line(w.hub(), &project);
    assert_eq!((idle["state"].as_str(), idle["what"].as_str()), (Some("idle"), Some(format!("Check before landing: true, then {wait} · After landing: true").as_str())), "{idle}");
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
    let after = line(w.hub(), &project);
    assert_eq!((after["state"].as_str(), after["what"].as_str()), (Some("ok"), Some(format!("After landing passed on #{id}").as_str())), "{after}");
}

#[test]
fn a_desk_with_no_check_and_nothing_after_landing_has_no_line() {
    let w = World::new();
    let (_dir, project, _id, _h) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    assert_eq!(line(w.hub(), &project), Value::Null);
}
