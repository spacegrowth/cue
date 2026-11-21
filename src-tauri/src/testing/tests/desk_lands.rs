//! How work lands, per project (lands.rs): the Desk shows the setting with its defaults, refuses what it can't do
//! yet, and works as it says: the shared folder (no copy), items at once (the pool), you commit (the Desk commits
//! nothing) and no check (landed, not checked).

use super::desk::desk;
use super::desk_commit::{commits, ends, item, log_edit, routed_with};
use super::team::git;
use crate::desk::{InstallStatus, Installer};
use crate::testing::World;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// An installer whose check passes, and that keeps what it was asked to check.
#[derive(Default)]
pub(super) struct Checker {
    pub(super) checked: Mutex<Vec<String>>,
}

impl Installer for Checker {
    fn has(&self, _repo: &str, _commit: &str) -> bool {
        false
    }

    fn start(&self, _repo: &str, _commit: &str, _tag: &str) -> Result<(), String> {
        Ok(())
    }

    fn status(&self) -> Option<InstallStatus> {
        None
    }

    fn contains(&self, repo: &str, commit: &str, of: &str) -> bool {
        crate::install::self_install::is_ancestor(repo, commit, of)
    }

    fn check(&self, _repo: &str, commit: &str) -> Option<Result<(), String>> {
        crate::locking::lock(&self.checked, "checker checked").push(commit.to_string());
        Some(Ok(()))
    }
}

/// Session `h`'s project set to `lands`, as the Desk's setting saves it.
pub(super) fn set(w: &World, h: &str, lands: Value) {
    let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
    crate::desk::lands::save(&project, crate::desk::lands::Place::Repo, &lands).unwrap();
}

/// The repository's main checkout (where its copies are made from).
fn top(w: &World) -> String {
    let out = String::from_utf8(std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(w.home()).args(["worktree", "list", "--porcelain"]).output().unwrap().stdout).unwrap();
    out.lines().next().and_then(|l| l.strip_prefix("worktree ")).unwrap().to_string()
}

/// The holder edits a.txt in the folder it works in.
fn edits(w: &World, h: &str, dir: &std::path::Path) {
    std::fs::write(dir.join("a.txt"), "its work\n").unwrap();
    log_edit(w, h, &dir.join("a.txt"));
}

#[test]
fn a_new_project_shows_the_defaults_and_what_cant_be_picked_yet_says_so() {
    let w = World::new();
    let (_, project, _, _) = routed_with(&w, |_, _, _| {}, |_, _, _| {});
    let l = &desk(&w, &project)["lands"];
    assert_eq!(l["set"], json!({ "where": "copy", "how": "squash", "check": "none", "after": "nothing", "push": "ask", "at_once": 3, "check_steps": [], "after_steps": [] }), "{l}");
    assert_eq!(l["summary"], "own copy, not checked, one commit per item");
    let choice = |part: &str, key: &str| l["parts"].as_array().unwrap().iter().find(|p| p["key"] == part).unwrap()["choices"].as_array().unwrap().iter().find(|c| c["key"] == key).cloned().unwrap();
    for (part, key) in [("where", "branch"), ("how", "merge")] {
        assert_eq!((choice(part, key)["ok"].as_bool(), choice(part, key)["why"].as_str()), (Some(false), Some("not yet")), "{part} {key}");
    }
    assert_eq!(choice("check", "cue")["why"], "only for Cue's own project");
    for (part, key) in [("how", "pr"), ("push", "after"), ("push", "never"), ("check", "command"), ("after", "command")] {
        assert_eq!(choice(part, key)["ok"], true, "{part} {key} can be picked");
    }
    let err = w.hub().desk_lands_set(&project, &json!({ "how": "merge" })).unwrap_err();
    assert!(err.contains("not yet"), "{err}");
    let saved = w.hub().desk_lands_set(&project, &json!({ "where": "shared", "at_once": 2 })).unwrap();
    assert_eq!((saved["set"]["where"].as_str(), saved["set"]["at_once"].as_u64(), saved["summary"].as_str()), (Some("shared"), Some(2), Some("shared folder, not checked, one commit per item")));
    assert_eq!(desk(&w, &project)["lands"]["set"]["where"], "shared", "kept in Cue's settings");
}

#[test]
fn a_folder_that_isnt_a_git_repository_works_in_place_and_says_why() {
    let _w = World::new();
    let dir = std::env::temp_dir().join(format!("cue-lands-plain-{}-{}", std::process::id(), crate::clock::now_ms()));
    std::fs::create_dir_all(&dir).unwrap();
    let folder = dir.to_string_lossy().to_string();
    crate::logs::repos::first_look(&folder);
    let v = crate::desk::lands::desk_view("plain", &folder);
    assert_eq!((v["place"].as_str(), v["summary"].as_str()), (Some("folder"), Some("shared folder, not checked, you commit")), "{v}");
    assert!(v["why"].as_str().unwrap().contains("isn't a git repository"), "{v}");
    let err = crate::desk::lands::save("plain", crate::desk::lands::place_of(&folder), &json!({ "where": "copy" })).unwrap_err();
    assert!(err.contains("needs a git repository"), "{err}");
    // The settings screen's save never takes steps that push: a push always asks first.
    let err = crate::desk::lands::save("plain", crate::desk::lands::place_of(&folder), &json!({ "after": "command", "after_steps": ["npm run build", "git push"] })).unwrap_err();
    assert!(err.ends_with("Steps can't push; a push always asks you first"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_the_shared_folder_an_item_gets_no_copy() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, json!({ "where": "shared" })), edits);
    let it = item(&w, &project, "Header jumps");
    assert!(it["tree"].is_null(), "no copy: {it}");
    assert!(!w.hosted.sent().iter().any(|(s, t)| *s == h && t.contains(".claude/worktrees/desk-")), "its holder isn't sent to a copy");
    let before = commits(&dir);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!(it["state"], "to_check", "{it}");
    assert_ne!(commits(&dir), before, "the Desk committed its files in the shared folder");
}

#[test]
fn items_at_once_is_the_pool_of_copies() {
    let w = World::new();
    let (_, project, id, _) = routed_with(
        &w,
        |w, h, _| {
            set(w, h, json!({ "at_once": 1 }));
            w.hub().desk_hold_copies(&top(w), &[1]);
        },
        |_, _, _| {},
    );
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"].is_null()), (Some("queued"), true), "one copy, held: {it}");
    let summary = w.hub().desk_summary(&project, "", false).0;
    let line = summary.lines().find(|l| l.starts_with(&format!("- #{id} "))).unwrap_or_else(|| panic!("{summary}"));
    assert!(line.contains("(1 of 1 held, by #held1)"), "{line}");
    w.hub().desk_let_go_copy_quietly(1);
    w.hub().desk_tick_now();
    w.settle();
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["tree"]["slot"].as_u64()), (Some("working"), Some(1)), "{it}");
}

#[test]
fn no_check_lands_without_running_one_and_says_not_checked() {
    let w = World::new();
    let checker = Arc::new(Checker::default());
    w.hub().desk_set_installer(checker.clone());
    let (dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, json!({ "check": "none" })), |_, _, _| {});
    let path = item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap().to_string();
    std::fs::write(std::path::Path::new(&path).join("a.txt"), "a\n").unwrap();
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"), "landed");
    assert!(crate::locking::lock(&checker.checked, "checker checked").is_empty(), "no check run");
    assert_eq!(item(&w, &project, "Header jumps")["finish"]["unchecked"], true, "landed, not checked");
}

#[test]
fn you_commit_means_the_desk_commits_nothing() {
    let w = World::new();
    let (dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, json!({ "where": "shared", "how": "owner" })), edits);
    assert!(item(&w, &project, "Header jumps")["tree"].is_null(), "works in the shared folder");
    let before = commits(&dir);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"].is_null()), (Some("to_check"), true), "{it}");
    w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!(commits(&dir), before, "nothing committed");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "its work\n", "its work left for you");
}

#[test]
fn a_folder_that_isnt_a_git_repository_with_nothing_saved_gets_no_copy_and_you_commit() {
    // As routed_with, with no repository: the session works in a plain folder.
    let w = World::new();
    let dir = w.home().to_path_buf();
    assert!(std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(&dir).args(["rev-parse", "--show-toplevel"]).output().is_ok_and(|o| !o.status.success()), "a plain folder");
    let h = super::team::lead(&w, "hello");
    ends(&w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, crate::hosted::HostedEvent::Taken { text: "hello".into() });
    ends(&w, &d, "I run the Desk now.");
    w.advance(1000);
    ends(&w, &d, "ITEM: bug | Header jumps | done when: it stays put\nROUTE: #new -> cue-teams | why: it made the header");
    w.advance(1000);
    edits(&w, &h, &dir);
    let id = item(&w, &project, "Header jumps")["id"].as_str().unwrap().to_string();
    assert!(item(&w, &project, "Header jumps")["tree"].is_null(), "no copy");
    assert_eq!(desk(&w, &project)["lands"]["summary"], "shared folder, not checked, you commit");
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    let it = item(&w, &project, "Header jumps");
    assert_eq!((it["state"].as_str(), it["finish"].is_null()), (Some("to_check"), true), "yours to commit: {it}");
    let r = w.hub().desk_looks_right(&id).unwrap();
    w.settle();
    assert_eq!((r["files"].as_u64(), r["commit"].as_str()), (Some(0), Some("")), "nothing committed: {r}");
    assert_eq!(item(&w, &project, "Header jumps")["state"], "done");
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "its work\n", "its work left for you");
}

/// A check of your own (#381): its steps run in order on main plus the item and stop at the first that fails; the
/// installer's check isn't run, and nothing reaches main.
#[test]
fn a_check_of_your_own_runs_in_order_and_stops_at_the_first_failure() {
    let w = World::new();
    let checker = Arc::new(Checker::default());
    w.hub().desk_set_installer(checker.clone());
    let ran = w.home().join("ran-steps").to_string_lossy().to_string();
    let steps = json!([format!("echo one >> {ran}"), format!("echo two >> {ran}; exit 1"), format!("echo three >> {ran}")]);
    let (dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, json!({ "check": "command", "check_steps": steps })), |_, _, _| {});
    let path = item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap().to_string();
    std::fs::write(std::path::Path::new(&path).join("a.txt"), "a\n").unwrap();
    let before = git(&dir, &["rev-parse", "HEAD"]);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(std::fs::read_to_string(&ran).unwrap_or_default(), "one\ntwo\n", "in order, stopped at the second");
    assert!(crate::locking::lock(&checker.checked, "checker checked").is_empty(), "not the installer's check");
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), before, "nothing reached main");
    let it = item(&w, &project, "Header jumps");
    assert!(it.to_string().contains("echo two"), "it says which step failed: {it}");
}

/// A check of your own reads the tree Cue's own check reads (#381): main moved after the item started, and the steps
/// see main's change with the item's.
#[test]
fn a_check_of_your_own_sees_main_moved_since_the_item_started() {
    let w = World::new();
    let checker = Arc::new(Checker::default());
    w.hub().desk_set_installer(checker.clone());
    let seen = w.home().join("seen").to_string_lossy().to_string();
    let steps = json!([format!("cat main.txt a.txt > {seen}")]);
    let (dir, project, id, h) = routed_with(&w, |w, h, _| set(w, h, json!({ "check": "command", "check_steps": steps })), |_, _, _| {});
    let path = item(&w, &project, "Header jumps")["tree"]["path"].as_str().unwrap().to_string();
    std::fs::write(std::path::Path::new(&path).join("a.txt"), "the item's\n").unwrap();
    // Main moves on after the item's copy was made.
    std::fs::write(dir.join("main.txt"), "main's\n").unwrap();
    git(&dir, &["add", "-f", "main.txt"]);
    git(&dir, &["commit", "-q", "-m", "main moved"]);
    ends(&w, &h, &format!("Fixed.\nDONE: #{id}"));
    assert_eq!(std::fs::read_to_string(&seen).unwrap_or_default(), "main's\nthe item's\n", "main plus the item");
    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), format!("Header jumps (#{id})"), "landed");
    assert!(!git(&dir, &["worktree", "list"]).contains("/checks/"), "its copy removed after");
}

/// The lead changes how work lands only through you (#381): its LANDS line is a Yes / No ask naming the change, nothing
/// is saved before your yes, a no leaves it, a yes saves it; and it can never turn off the push ask.
#[test]
fn the_leads_change_waits_for_your_yes() {
    let w = World::new();
    let (_h, project) = super::desk::holder(&w);
    let l = super::desk::new_lead(&w, &project);
    let now = |w: &World| super::desk::desk(w, &project)["lands"]["set"].clone();
    let block = w.hub().desk_summary(&project, &l, false).0;
    assert!(block.contains("How work lands now (LANDS: to propose a change): where: shared folder · how: you commit"), "the lead reads it in its Desk block: {block}");
    let card = |w: &World| {
        let snap = w.snapshot();
        snap.cards_of(&l).into_iter().find(|c| !c["tool_input"]["desk_lands"].is_null()).cloned()
    };
    let answer = |w: &World, id: &str, a: &str| {
        w.hub().respond(id, crate::model::Decision { behavior: "allow".into(), answers: Some(json!({ "q": a })), ..Default::default() });
        w.settle();
        w.advance(1000);
    };
    // No: nothing changes.
    ends(&w, &l, "Sure, I'll ask.\nLANDS: after -> command | steps: make deploy | why: the user asked");
    let ask = card(&w).expect("asked");
    assert!(ask.to_string().contains(&format!("Change how work lands on {project}: after landing → make deploy?")), "{ask}");
    assert_eq!(now(&w)["after"], "nothing", "nothing saved before your answer");
    answer(&w, ask["id"].as_str().unwrap(), "No");
    assert_eq!(now(&w)["after"], "nothing");
    assert!(super::desk::sent_to(&w, &l).iter().any(|t| t.contains("The user said no")));
    // Yes: saved, and the Desk shows it.
    ends(&w, &l, "LANDS: check -> command | steps: npm test | tail -5 ;; cd web; npm run lint | why: the user asked");
    let ask = card(&w).expect("asked");
    answer(&w, ask["id"].as_str().unwrap(), "Yes");
    assert_eq!((now(&w)["check"].clone(), now(&w)["check_steps"].clone()), (json!("command"), json!(["npm test | tail -5", "cd web; npm run lint"])));
    // Steps that push are never asked: a push always asks first.
    ends(&w, &l, "LANDS: after -> command | steps: npm run build ;; git push origin main | why: faster");
    assert!(card(&w).is_none(), "not asked");
    assert_eq!(now(&w)["after"], "nothing");
    assert!(super::desk::sent_to(&w, &l).iter().any(|t| t.starts_with("Not asked: LANDS: after -> command") && t.contains("Steps can't push; a push always asks you first")));
    assert!(super::desk::sent_to(&w, &l).iter().any(|t| t.contains("The user said yes")));
    // A push after each landing is never asked: the lead is told why.
    ends(&w, &l, "LANDS: push -> after | why: faster");
    assert!(card(&w).is_none(), "not asked");
    assert_eq!(now(&w)["push"], "ask");
    assert!(super::desk::sent_to(&w, &l).iter().any(|t| t.starts_with("Not asked: LANDS: push -> after") && t.contains("keeps asking")));
}

/// A git repository beside the test's home, named `name`; `cue` makes it Cue's own (its tauri.conf.json).
fn repo_beside(w: &World, name: &str, cue: bool) -> String {
    let home = w.home();
    let dir = home.with_file_name(format!("{}-{name}", home.file_name().unwrap().to_string_lossy()));
    std::fs::create_dir_all(dir.join("src-tauri")).unwrap();
    if cue {
        std::fs::write(dir.join("src-tauri/tauri.conf.json"), r#"{ "identifier": "dev.spacegrowth.cue" }"#).unwrap();
    }
    git(&dir, &["init", "-q", "-b", "main"]);
    dir.to_string_lossy().into_owned()
}

#[test]
fn a_desk_on_another_project_never_offers_cues_own_check_or_install_and_each_keeps_its_own() {
    // #460: two Desks in one workspace, one on Cue's own folder, one on another repository.
    let w = World::new();
    let (_h, project) = super::desk::holder(&w);
    let space = |id: &str, name: &str, folder: String| crate::desk::model::DeskSpace { id: id.into(), name: name.into(), project: project.clone(), folder, machine_folders: Default::default() };
    w.hub().desk_put_space(space("desk-cue", "Cue work", repo_beside(&w, "cue", true))).unwrap();
    w.hub().desk_put_space(space("desk-web", "Web app", repo_beside(&w, "web", false))).unwrap();
    let shown = |desk: &str, part: &str| -> Vec<String> {
        let d = desk_of(&w, desk);
        let p = d["lands"]["parts"].as_array().unwrap().iter().find(|p| p["key"] == part).cloned().unwrap();
        p["choices"].as_array().unwrap().iter().filter(|c| c["ok"] == true).map(|c| c["key"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(shown("desk-web", "check"), ["none", "command"], "another project: None or your own steps");
    assert_eq!(shown("desk-web", "after"), ["nothing", "command"], "another project: Nothing or your own steps");
    assert_eq!(shown("desk-cue", "check"), ["none", "cue", "command"], "Cue's own Desk keeps Cue's own check");
    assert_eq!(shown("desk-cue", "after"), ["nothing", "install", "command"], "and its install");
    assert_eq!(desk_of(&w, "desk-cue")["lands"]["set"]["check"], "cue");
    assert_eq!(desk_of(&w, "desk-cue")["lands"]["set"]["after"], "install");
    // Cue's own setting can't be saved on the other Desk.
    assert!(w.hub().desk_lands_set("desk-web", &json!({ "check": "cue" })).is_err());
    assert!(w.hub().desk_lands_set("desk-web", &json!({ "after": "install" })).is_err());
    // Each keeps its own: a change on one leaves the other as it was.
    w.hub().desk_lands_set("desk-web", &json!({ "check": "command", "check_steps": ["npm test"], "after": "command", "after_steps": ["npm run build"] })).unwrap();
    w.hub().desk_lands_set("desk-cue", &json!({ "check": "none" })).unwrap();
    let web = desk_of(&w, "desk-web")["lands"]["set"].clone();
    assert_eq!((web["check"].clone(), web["check_steps"].clone(), web["after"].clone()), (json!("command"), json!(["npm test"]), json!("command")));
    let cue = desk_of(&w, "desk-cue")["lands"]["set"].clone();
    assert_eq!((cue["check"].clone(), cue["after"].clone()), (json!("none"), json!("install")), "Cue's Desk keeps its install");
    // What its items land by is what its screen shows.
    let folder = desk_of(&w, "desk-web")["folder"].as_str().unwrap().to_string();
    assert_eq!(crate::desk::lands::lands_of("desk-web", &folder).check, "command");
    assert_eq!(crate::desk::lands::lands_of("desk-web", &folder).after, "command");
    // The workspace project's own setting (the legacy Desk) is untouched by either.
    assert_ne!(crate::desk::lands::lands_of(&project, &w.home().to_string_lossy()).check_steps, ["npm test"]);
    for dir in ["cue", "web"] {
        let home = w.home();
        let _ = std::fs::remove_dir_all(home.with_file_name(format!("{}-{dir}", home.file_name().unwrap().to_string_lossy())));
    }
}

fn desk_of(w: &World, id: &str) -> Value {
    let snap = w.snapshot();
    snap.0["desk"]["projects"].as_array().unwrap().iter().find(|p| p["id"] == id).cloned().unwrap_or_else(|| panic!("no Desk {id}"))
}
