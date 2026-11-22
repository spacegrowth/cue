//! An item whose code lives in another repository on this Mac (#635, desk/hub/landing/other_repo.rs):
//! `ROUTE: #<id> -> new | repo: <folder> | why: …` starts its session in that repository, gives it a copy of that
//! repository, and at its DONE lands its work on that repository's main line, the Desk's own left as it was.

use super::desk_commit::{ends, item};
use super::team::{git, lead, main_repo};
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A second repository beside the Desk's (its main line `main`, one commit): its main checkout.
fn second_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("second-repo");
    std::fs::create_dir_all(repo.join("app")).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "t@example.com"]);
    git(&repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("app/lib.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "app/lib.rs"]);
    git(&repo, &["commit", "-q", "-m", "start"]);
    std::fs::canonicalize(&repo).unwrap()
}

/// A Desk (its repository the World's) whose lead adds "Login screen" and routes it with `route` (its `{repo}` the
/// second repository's folder): (the Desk's repository, the second, project, the Desk's first session, the lead).
fn desk_routing(w: &World, route: &str) -> (PathBuf, PathBuf, String, String, String) {
    let dir = main_repo(w);
    let second = second_repo(&dir);
    let h = lead(w, "hello");
    ends(w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(w, &d, "I run the Desk now.");
    w.advance(1000);
    let route = route.replace("{repo}", &second.to_string_lossy());
    ends(w, &d, &format!("ITEM: feature | Login screen | done when: it shows\n{route}"));
    w.advance(1000);
    w.settle();
    (dir, second, project, h, d)
}

fn copy_of(it: &Value) -> PathBuf {
    PathBuf::from(it["tree"]["path"].as_str().unwrap_or_else(|| panic!("no copy: {it}")))
}

fn told(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, _)| s == sid).map(|(_, t)| t).collect()
}

#[test]
fn the_routes_repo_is_read() {
    let got = lines("ROUTE: #12 -> new | repo: /src/app | why: its code is there");
    assert!(matches!(&got[..], [Line::Route { repo: Some(r), .. }] if r == "/src/app"), "{got:?}");
    let got = lines("ROUTE: #12 -> new | why: report the bug");
    assert!(matches!(&got[..], [Line::Route { repo: None, .. }]), "\"report\" isn't repo: {got:?}");
}

#[test]
fn a_new_session_for_an_item_in_another_repo_starts_there_and_its_done_lands_on_that_repos_main_line() {
    let w = World::new();
    let (dir, second, project, _, _) = desk_routing(&w, "ROUTE: #new -> new | repo: {repo} | why: its code is there");
    let it = item(&w, &project, "Login screen");
    let id = it["id"].as_str().unwrap().to_string();
    assert_eq!(it["in_repo"]["repo"], second.to_string_lossy().as_ref(), "{it}");
    let by = it["holder"]["sid"].as_str().unwrap_or_else(|| panic!("nobody took it: {it}")).to_string();
    let cwd = w.snapshot().session(&by)["cwd"].as_str().unwrap().to_string();
    assert_eq!(PathBuf::from(&cwd).canonicalize().unwrap(), second, "its session starts in the second repository");
    // Its copy is of the second repository, on a branch of its own from that repository's main line.
    let copy = copy_of(&it);
    assert!(copy.starts_with(&second), "a copy of the second repository: {}", copy.display());
    assert_eq!(git(&copy, &["branch", "--show-current"]).trim(), format!("desk/{id}"));
    assert!(told(&w, &by).iter().any(|t| t.contains(&copy.to_string_lossy().to_string())), "its session is told where its copy is");
    let desk_main = git(&dir, &["rev-parse", "HEAD"]);
    let second_main = git(&second, &["rev-parse", "main"]);
    std::fs::write(copy.join("app/login.rs"), "fn login() {}\n").unwrap();
    ends(&w, &by, &format!("Built.\nDONE: #{id}"));
    w.advance(1000);
    w.settle();
    // Landed on the second repository's main line, as one commit on what it was; the Desk's own untouched.
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), desk_main, "nothing on the Desk's own main line");
    assert_eq!(git(&second, &["rev-parse", "main^"]), second_main, "one commit on the second's main line");
    assert_eq!(git(&second, &["log", "-1", "--format=%s", "main"]).trim(), format!("Login screen (#{id})"));
    assert_eq!(std::fs::read_to_string(second.join("app/login.rs")).unwrap(), "fn login() {}\n", "its main checkout has it");
    let it = item(&w, &project, "Login screen");
    assert!(it["tree"].is_null() && !copy.exists(), "the copy is gone: {it}");
    assert_eq!(it["state"], "to_check", "{it}");
}

#[test]
fn a_session_given_an_item_in_another_repo_gets_a_copy_of_that_repo() {
    let w = World::new();
    let (_, second, project, h, _) = desk_routing(&w, "ROUTE: #new -> cue-teams | repo: {repo} | why: it knows logins");
    let it = item(&w, &project, "Login screen");
    assert_eq!(it["holder"]["sid"], h.as_str(), "{it}");
    let copy = copy_of(&it);
    assert!(copy.starts_with(&second), "a copy of the second repository, not the Desk's: {}", copy.display());
}

#[test]
fn a_repo_that_isnt_one_is_refused_and_the_lead_told_why() {
    let w = World::new();
    let (_, _, project, _, d) = desk_routing(&w, "ROUTE: #new -> new | repo: /no/such/folder | why: there");
    let it = item(&w, &project, "Login screen");
    assert!(it["holder"].is_null() && it["in_repo"].is_null(), "not routed: {it}");
    assert!(told(&w, &d).iter().any(|t| t.contains("Refused: repo: /no/such/folder") && t.contains("isn't a folder")), "{:?}", told(&w, &d));
}

#[test]
fn a_repo_named_by_a_project_cue_doesnt_know_is_refused() {
    let w = World::new();
    let (_, _, project, _, d) = desk_routing(&w, "ROUTE: #new -> new | repo: nothing-by-that-name | why: there");
    assert!(item(&w, &project, "Login screen")["holder"].is_null(), "not routed");
    assert!(told(&w, &d).iter().any(|t| t.contains("Cue knows no repository called nothing-by-that-name")), "{:?}", told(&w, &d));
}
