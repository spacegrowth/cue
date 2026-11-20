//! What an item's work needs before it's to check (split from desk_finish.rs): only the window's files are served
//! in place with no install, work the app doesn't run (mocks, scratch files) needs none, and anything the app runs
//! waits for an install.

use super::desk_finish::*;
use super::desk::desk;
use super::desk_commit::{ends, item};
use super::team::{git, says, writes};
use crate::testing::World;
use serde_json::json;
use std::sync::Arc;

/// An item done in this Cue's source, the running Cue built from its commit before the item's work, which writes
/// `files` (paths in the repo): (fake, repo, project, id).
fn done_since_running(w: &World, files: &'static [&'static str]) -> (Arc<Fake>, std::path::PathBuf, String, String) {
    done_since_running_with(w, files, false, false)
}

/// `done_since_running`, with Cue's window check (scripts/check-window.mjs) committed in the repo when `checked`, its
/// window loading ui/desk.js, and Cue's node_modules linked in.
/// `ahead`: another change to the app (src-tauri/desk.rs) committed on the main line after the running Cue's commit,
/// not installed yet, before the item's work.
fn done_since_running_with(w: &World, files: &'static [&'static str], checked: bool, ahead: bool) -> (Arc<Fake>, std::path::PathBuf, String, String) {
    done_since_running_saying(w, files, checked, ahead, "Fixed.")
}

/// `done_since_running_with`, its holder's DONE turn saying `words` first.
fn done_since_running_saying(w: &World, files: &'static [&'static str], checked: bool, ahead: bool, words: &str) -> (Arc<Fake>, std::path::PathBuf, String, String) {
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, project, id) = super::desk_commit::done_saying(
        w,
        |_, _, dir| {
            as_this_cue(dir);
            // The test repo ignores everything but a few files: let in the window's, the tests' and the app's.
            let mut ignore = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
            ignore.push_str("!ui/\n!ui/**\n!tests/\n!tests/**\n!src-tauri/\n!src-tauri/desk.rs\n!work/\n!work/**\n!docs/\n!docs/**\n");
            std::fs::write(dir.join(".gitignore"), &ignore).unwrap();
            std::fs::create_dir_all(dir.join("ui")).unwrap();
            std::fs::write(dir.join("ui/index.html"), "<p>built in</p>\n").unwrap();
            if checked {
                let cue = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
                std::fs::write(dir.join(".gitignore"), format!("{ignore}!scripts/\n!scripts/**\n")).unwrap();
                std::fs::create_dir_all(dir.join("scripts")).unwrap();
                std::fs::copy(cue.join("scripts/check-window.mjs"), dir.join("scripts/check-window.mjs")).unwrap();
                std::os::unix::fs::symlink(cue.join("node_modules"), dir.join("node_modules")).unwrap();
                std::fs::write(dir.join("ui/index.html"), "<div id=\"app\"></div>\n<script src=\"desk.js\"></script>\n").unwrap();
            }
            git(dir, &["add", "-A"]);
            git(dir, &["commit", "-q", "-m", "the Cue that runs"]);
            let running = git(dir, &["rev-parse", "HEAD"]).trim().to_string();
            let install = crate::server::cue_dir().join("install");
            std::fs::create_dir_all(&install).unwrap();
            std::fs::write(install.join("running.json"), json!({ "commit": running }).to_string()).unwrap();
            if ahead {
                std::fs::write(dir.join("src-tauri/desk.rs"), "// another item's change\n").unwrap();
                git(dir, &["add", "-A"]);
                git(dir, &["commit", "-q", "-m", "another item, not installed yet"]);
            }
        },
        |w, h, dir| {
            let paths: Vec<String> = files.iter().map(|f| dir.join(f).to_string_lossy().to_string()).collect();
            for p in &paths {
                std::fs::create_dir_all(std::path::Path::new(p).parent().unwrap()).unwrap();
            }
            says(w, h, writes("toolu_A", "agentA", "fixer", &paths));
        },
        words,
    );
    (fake, dir, project, id)
}

#[test]
fn an_item_that_changed_only_the_windows_files_is_served_in_place_and_to_check_with_no_install() {
    let w = World::new();
    let (fake, dir, project, _) = done_since_running(&w, &["ui/desk.js", "tests/ui/specs/desk.spec.mjs"]);
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["commit"].as_str()), (Some("to_check"), Some("checked"), Some(head.as_str())), "{shown}");
    assert_eq!(w.window.heard("ui-reload"), vec![json!({ "commit": head })], "the window reloads");
    let served = crate::server::cue_dir().join("live").join(&head);
    assert_eq!(std::fs::read_to_string(served.join("desk.js")).unwrap(), "fixer wrote this\n", "from the item's commit");
    assert_eq!(std::fs::read_to_string(served.join("index.html")).unwrap(), "<p>built in</p>\n");
    w.advance(3 * 60_000);
    assert!(fake.started().is_empty(), "no build, no restart");
}

/// An item whose window doesn't load at its commit (here desk.js isn't even JavaScript): not served, no install
/// queued, and back to its holder saying why.
#[test]
fn an_item_whose_window_doesnt_load_is_not_served_and_goes_back_to_its_holder() {
    let w = World::new();
    let (fake, _, project, id) = done_since_running_with(&w, &["ui/desk.js"], true, false);
    let shown = item(&w, &project, "Header jumps");
    let broken = "The window doesn't load at this commit, so it wasn't installed or served";
    assert_eq!((shown["state"].as_str(), shown["latest"].as_str(), shown["finish"]["step"].as_str()), (Some("finishing"), Some(broken), Some("failed")), "{shown}");
    assert_eq!(shown["stopped"], true, "the Desk lead's to settle, not yours (#483): {shown}");
    assert!(shown["finish"]["log"].as_str().unwrap_or("").contains("doesn't parse: ui/desk.js"), "{shown}");
    let h = shown["holder"]["sid"].as_str().unwrap().to_string();
    let sent = sent_to(&w, &h);
    assert!(sent.iter().any(|t| t.contains(&format!("isn't done: {broken}")) && t.contains("doesn't parse: ui/desk.js") && t.contains(&format!("DONE: #{id}"))), "{sent:?}");
    assert!(w.window.heard("ui-reload").is_empty(), "nothing served");
    assert!(!crate::server::cue_dir().join("live/current.json").exists());
    w.advance(30 * 60_000);
    assert!(fake.started().is_empty(), "nor installed");
}

#[test]
fn an_item_that_changed_the_app_side_too_still_waits_for_an_install() {
    let w = World::new();
    let (fake, _, project, _) = done_since_running(&w, &["ui/desk.js", "src-tauri/desk.rs"]);
    assert_eq!(step(&w, &project), "queued");
    assert!(w.window.heard("ui-reload").is_empty());
    w.advance(30 * 60_000);
    assert_eq!(fake.started().len(), 1, "installed");
    assert!(!crate::server::cue_dir().join("live").exists(), "nothing served in place");
}

/// #408: mocks, canvas pages, notes and plans change nothing the app runs: to check at once, with the link to what
/// it made, even with another item's change to the app on the main line waiting for an install.
#[test]
fn an_item_whose_work_changed_nothing_the_app_runs_goes_to_check_with_no_install() {
    let w = World::new();
    let (fake, dir, project, id) = done_since_running_with(&w, &["work/header-mocks.html", "docs/header-plan.md"], false, true);
    let head = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str(), shown["finish"]["commit"].as_str()), (Some("to_check"), Some("checked"), Some(head.as_str())), "{shown}");
    assert_eq!(shown["finish"]["mocks"], true, "its line says mocks, not in the app (#436): {shown}");
    let h = shown["holder"]["sid"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("DESIGNS: #{id} https://claude.ai/artifact/abc"));
    assert_eq!(item(&w, &project, "Header jumps")["designs"], "https://claude.ai/artifact/abc", "its link, to check it");
    w.advance(30 * 60_000);
    assert!(fake.started().is_empty(), "nothing installed");
    assert!(w.window.heard("ui-reload").is_empty(), "nor served");
}

/// #408: the same, but its work changed the app: it waits for the install as before.
#[test]
fn an_item_whose_work_changed_the_app_waits_for_an_install_even_with_mocks_beside_it() {
    let w = World::new();
    let (fake, _, project, _) = done_since_running_with(&w, &["work/header-mocks.html", "src-tauri/desk.rs"], false, true);
    assert_eq!(step(&w, &project), "queued");
    w.advance(30 * 60_000);
    assert_eq!(fake.started().len(), 1, "installed");
    assert!(item(&w, &project, "Header jumps")["finish"]["mocks"].is_null(), "in the app, not mocks (#436)");
}

/// #432 (#423 lined up to install): a mocks item whose only files in the repository are ones it never committed (a
/// script it wrote in its scratch folder, read as the repository's; here one the repository ignores) made no commit
/// there: to check at once, no install, even with another item's change to the app waiting for one.
#[test]
fn a_mocks_item_with_no_commit_and_only_scratch_files_goes_to_check_with_no_install() {
    let w = World::new();
    let (fake, _, project, id) = done_since_running_with(&w, &["gen_rr.py"], false, true);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["step"].as_str()), (Some("to_check"), Some("checked")), "{shown}");
    assert!(shown["commit"].as_str().unwrap_or("").is_empty(), "no commit made: {shown}");
    assert_eq!(shown["finish"]["mocks"], true, "{shown}");
    let h = shown["holder"]["sid"].as_str().unwrap().to_string();
    ends(&w, &h, &format!("DESIGNS: #{id} https://claude.ai/artifact/abc"));
    w.advance(30 * 60_000);
    assert!(fake.started().is_empty(), "nothing installed");
    assert!(w.window.heard("ui-reload").is_empty(), "nor served");
}

#[test]
fn a_mocks_only_items_pick_closes_it_and_makes_its_build_item_for_the_lead_marking_the_pick_on_its_canvas() {
    let w = World::new();
    let words = "Two ways on the canvas: https://claude.ai/artifact/abc\nM1: a strip under the header\nM2: a card\nPick one: M1 or M2.";
    let (fake, _, project, id) = done_since_running_saying(&w, &["work/header-mocks.html"], false, true, words);
    let shown = item(&w, &project, "Header jumps");
    assert_eq!((shown["state"].as_str(), shown["finish"]["mocks"].as_bool(), shown["designs"].as_str()), (Some("to_check"), Some(true), None), "mocks only, no DESIGNS: line: {shown}");
    let h = shown["holder"]["sid"].as_str().unwrap().to_string();
    let lead = desk(&w, &project)["lead"]["sid"].as_str().unwrap().to_string();
    let told_lead = sent_to(&w, &lead).len();

    // M2, as you'd pick it (#445): its canvas is the link it made.
    let picked = w.hub().desk_item_act(&id, "pick", "m2").unwrap();
    assert_eq!((picked.state.as_str(), picked.picked.as_str()), ("done", "M2"));
    let b = item(&w, &project, "Build: Header jumps (option M2)");
    let n = b["id"].as_str().unwrap().to_string();
    assert_eq!((picked.built_as.as_str(), b["builds"].as_str(), b["state"].as_str()), (n.as_str(), Some(id.as_str()), Some("new")));
    assert!(b["why"].as_str().unwrap().contains("https://claude.ai/artifact/abc"), "{b}");
    assert!(b.to_string().contains("Picked: M2"), "its done when has the pick marked: {b}");
    // Its holder marks the pick on its canvas row and the Ideas page, and does no more on it.
    let told = sent_to(&w, &h).into_iter().find(|t| t.contains("option M2 was picked")).expect("its holder is told");
    assert!(told.contains(&format!("#{n} builds it")) && told.contains("Picked: M2") && told.contains("https://claude.ai/artifact/abc") && told.contains("Ideas page"), "{told}");
    // The lead routes it at once.
    w.advance(1000);
    let sent = sent_to(&w, &lead);
    assert_eq!(sent.len(), told_lead + 1, "{sent:?}");
    assert!(sent.last().unwrap().contains(&format!("New item #{n} (bug): Build: Header jumps (option M2)")) && sent.last().unwrap().contains("Route it now"), "{sent:?}");
    assert!(fake.started().is_empty(), "nothing installed for the mocks");
}
