//! How many of a Desk's items work on each machine at once (#711, Desk settings → Machines): a machine at its number
//! takes no more; a new item goes to another place ticked, or waits and starts by itself once there's room. This
//! Mac's number is its folders for items of their own. Played with a machine ("box") whose git runs in place.

use super::desk_commit::{ends, item};
use super::desk_remote::{routed_there, with_remote};
use crate::testing::World;

// How many items at once on each machine (#711): a machine with its number at work takes no more; a new item goes to
// another place ticked, or waits and starts there by itself once one is done.

#[test]
fn a_machine_set_to_one_at_once_takes_one_item_and_the_next_waits_then_starts_there() {
    let w = World::new();
    let ((_, _, _, project, id, _), _ssh) = routed_there(&w);
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    w.hub().desk_lands_set(&project, &serde_json::json!({ "machines": ["box"], "machine_folders": { "box": "~/code/app" }, "machine_at_once": { "box": 1 } })).unwrap();
    let c: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(crate::server::cue_dir().join("config.json")).unwrap()).unwrap();
    assert_eq!(c.pointer(&format!("/project/machine/{project}/at_once/box")), Some(&serde_json::json!(1)), "{c}");
    let d = super::desk::desk(&w, &project);
    assert_eq!(d["machines"]["machines"][0]["at_once"], 1, "{}", d["machines"]);
    // Out of range is refused.
    assert!(w.hub().desk_lands_set(&project, &serde_json::json!({ "machine_at_once": { "box": 0 } })).is_err());
    // Box has one at work already (Header jumps): both new ones wait for room there, and the lead is told why.
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | why: next\nITEM: bug | Sidebar jumps | done when: it stays put\nROUTE: #new -> new | why: next");
    w.advance(1000);
    for t in ["Footer jumps", "Sidebar jumps"] {
        let it = item(&w, &project, t);
        assert!(it["holder"].is_null(), "box is full: {it}");
        assert_eq!((it["state"].as_str(), it["machine_wait"].as_str()), (Some("queued"), Some("box")), "{it}");
    }
    let summary = w.hub().desk_summary(&project, "", false).0;
    assert!(summary.contains("waiting for room on box"), "the lead is told why it waits: {summary}");
    // Started again meanwhile: it still waits.
    w.hub().desk_queue_tick();
    w.advance(1000);
    assert!(item(&w, &project, "Footer jumps")["holder"].is_null());
    let done = |id: &str| {
        let id = id.to_string();
        w.hub().write(|st| st.desk.item_mut(&id).unwrap().state = "done".into());
        w.hub().desk_queue_tick();
        w.advance(1000);
    };
    let started = |t: &str| item(&w, &project, t)["holder"]["sid"].as_str().is_some();
    // Header jumps is done: one of them starts on box by itself, the first waiting; the other still waits.
    done(&id);
    let (first, second) = if started("Footer jumps") { ("Footer jumps", "Sidebar jumps") } else { ("Sidebar jumps", "Footer jumps") };
    let now = item(&w, &project, first);
    assert!(started(first), "started once there was room: {now}");
    assert_eq!(now["machine"].as_str(), Some("box"), "{now}");
    assert!(now["machine_wait"].is_null(), "{now}");
    assert!(!started(second), "only one at once on box: {}", item(&w, &project, second));
    // That one is done too: the other starts.
    done(now["id"].as_str().unwrap());
    assert!(started(second), "{}", item(&w, &project, second));
}

#[test]
fn a_full_machine_sends_the_next_item_to_another_place_ticked() {
    let w = World::new();
    let ((_, _, _, project, id, _), _ssh) = routed_there(&w);
    // Box free: the item it had is done.
    w.hub().write(|st| st.desk.item_mut(&id).unwrap().state = "done".into());
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    w.hub().desk_lands_set(&project, &serde_json::json!({ "machines": ["box", ""], "machine_folders": { "box": "~/code/app" }, "machine_at_once": { "box": 1 } })).unwrap();
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | why: next\nITEM: bug | Sidebar jumps | done when: it stays put\nROUTE: #new -> new | why: next\nITEM: bug | Menu jumps | done when: it stays put\nROUTE: #new -> new | why: next");
    w.advance(1000);
    let on: Vec<String> = ["Footer jumps", "Sidebar jumps", "Menu jumps"]
        .iter()
        .map(|t| {
            let it = item(&w, &project, t);
            assert!(it["holder"]["sid"].as_str().is_some(), "started: {it}");
            it["machine"].as_str().unwrap_or("").to_string()
        })
        .collect();
    assert_eq!(on.iter().filter(|m| *m == "box").count(), 1, "one on box, the rest here: {on:?}");
}

// This Mac's slots are its folders for items of their own (#711): an item given to a session here waits in line for
// one; with a machine ticked for the Desk that has room, it goes to a new session there instead.

#[test]
fn an_item_waiting_for_a_folder_on_this_mac_moves_to_a_ticked_machine_with_room() {
    let w = World::new();
    let ssh = crate::testing::FakeSsh::new();
    ssh.host("box", crate::testing::RUNS_IT);
    crate::machines::add("box", "").unwrap();
    let top = |w: &World| {
        let out = String::from_utf8(std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
        out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
    };
    // This Mac's 3 folders all held: Header jumps, given to cue-teams here, waits in line for one.
    let (_, project, id, h) = super::desk_commit::routed_with(&w, |w, _, dir| {
        with_remote(dir);
        w.hub().desk_hold_copies(&top(w), &[1, 2, 3]);
    }, |_, _, _| {});
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree_wait"].as_bool()), (Some("queued"), Some(true)), "{it}");
    let summary = w.hub().desk_summary(&project, "", false).0;
    assert!(summary.contains("waiting for a free copy of its repository on this Mac (3 of 3 held"), "the line names the machine: {summary}");
    // box ticked, one at once there: Header jumps moves to a new session on box, cue-teams told to leave it.
    w.hub().desk_lands_set(&project, &serde_json::json!({ "machines": ["", "box"], "machine_folders": { "box": "~/code/app" }, "machine_at_once": { "box": 1 } })).unwrap();
    w.hub().desk_queue_tick();
    w.advance(1000);
    let it = item(&w, &project, "Header jumps");
    assert!(it["holder"]["sid"].as_str().is_some_and(|s| s != h), "a new session took it: {it}");
    assert_eq!(it["machine"].as_str(), Some("box"), "{it}");
    assert!(it["tree_wait"].is_null() || it["tree_wait"] == false, "{it}");
    assert!(w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(&format!("Desk item #{id}")) && t.contains("moved to a new session on box")), "cue-teams told");
    // Both full now (this Mac's folders, box's one): the next new item waits for room, and starts nowhere.
    let lead = w.hub().desk_lead_sid(&project).unwrap();
    ends(&w, &lead, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> new | why: next");
    w.advance(1000);
    let it = item(&w, &project, "Footer jumps");
    assert!(it["holder"].is_null(), "no room anywhere: {it}");
    assert_eq!(it["state"].as_str(), Some("queued"), "{it}");
    assert!(!it["machine_wait"].as_str().unwrap_or("").is_empty(), "{it}");
    // A folder here is let go of: Footer jumps starts on this Mac.
    w.hub().desk_let_go_copy_quietly(2);
    w.hub().desk_queue_tick();
    w.advance(1000);
    let it = item(&w, &project, "Footer jumps");
    assert!(it["holder"]["sid"].as_str().is_some(), "started once this Mac had room: {it}");
    assert!(it["machine"].as_str().unwrap_or("").is_empty() || it["machine"] == "This Mac", "{it}");
}
