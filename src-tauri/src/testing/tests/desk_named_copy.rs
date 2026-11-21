//! An item at work in the add-on's own repository, on a branch the lead names (#557, desk/hub/landing/named_copy.rs):
//! `COPY: #<id> | in: add-on | branch: loops-d` gives it a copy of that repository on that branch as it starts; its
//! holder's edits there pass; at its DONE its work is committed on that branch, which stays. In the add-on's
//! repository (#565) the branch is then merged into the add-on's main line and Cue installed with it, to check once
//! the running Cue has that add-on commit; in a repository named by its folder nothing goes to a main line.

use super::desk_commit::{ends, item, tick};
use super::desk_finish::{as_this_cue, sent_to, Fake};
use super::team::{git, lead, main_repo};
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// An add-on repository beside the World's (its main line `main`, one commit), linked from `src-tauri/ext` as Cue's
/// add-on is: its main checkout.
fn add_on(dir: &Path) -> PathBuf {
    let repo = dir.join("addon-repo");
    std::fs::create_dir_all(repo.join("mac")).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "t@example.com"]);
    git(&repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("mac/lib.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "mac/lib.rs"]);
    git(&repo, &["commit", "-q", "-m", "start"]);
    std::fs::create_dir_all(dir.join("src-tauri")).unwrap();
    std::os::unix::fs::symlink(repo.join("mac"), dir.join("src-tauri/ext")).unwrap();
    std::fs::canonicalize(&repo).unwrap()
}

/// How the lead's COPY line names the repository: "add-on", the add-on's folder, another repository's folder (one
/// beside it, not linked), or no COPY line at all.
#[derive(Clone, Copy, PartialEq)]
enum Named {
    AddOn,
    AddOnFolder,
    Other,
    NoCopy,
}

/// A Desk whose lead adds "Loops band", gives it a copy of the add-on on `loops-d` and routes it: (cue's repository,
/// the add-on's, project, item id, holder, the lead). `before` runs on the add-on before the lead's turn.
fn routed(w: &World, before: impl Fn(&Path)) -> (PathBuf, PathBuf, String, String, String, String) {
    routed_in(w, Named::AddOn, before)
}

/// `routed`, the COPY line naming its repository as `named` says. With `Named::Other` the second is that repository.
fn routed_in(w: &World, named: Named, before: impl Fn(&Path)) -> (PathBuf, PathBuf, String, String, String, String) {
    let dir = main_repo(w);
    let addon = add_on(&dir);
    let addon = if named == Named::Other { other_repo(&dir) } else { addon };
    before(&addon);
    let copy = match named {
        Named::AddOn => "COPY: #new | in: add-on | branch: loops-d | why: it's add-on work\n".to_string(),
        Named::AddOnFolder | Named::Other => format!("COPY: #new | in: {} | branch: loops-d | why: it's add-on work\n", addon.display()),
        Named::NoCopy => String::new(),
    };
    let h = lead(w, "hello");
    ends(w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(w, &d, "I run the Desk now.");
    w.advance(1000);
    ends(w, &d, &format!("ITEM: task | Loops band | done when: it shows\n{copy}ROUTE: #new -> cue-teams | why: it knows Loops"));
    w.advance(1000);
    let id = item(w, &project, "Loops band")["id"].as_str().unwrap().to_string();
    (dir, addon, project, id, h, d)
}

/// A repository beside the World's that isn't Cue's add-on (its main line `main`, one commit): its main checkout.
fn other_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("other-repo");
    std::fs::create_dir_all(repo.join("mac")).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "t@example.com"]);
    git(&repo, &["config", "user.name", "T"]);
    std::fs::write(repo.join("mac/lib.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "mac/lib.rs"]);
    git(&repo, &["commit", "-q", "-m", "start"]);
    std::fs::canonicalize(&repo).unwrap()
}

fn copy_of(it: &Value) -> PathBuf {
    PathBuf::from(it["tree"]["path"].as_str().unwrap_or_else(|| panic!("no copy: {it}")))
}

#[test]
fn the_copy_line_is_read() {
    let got = lines("COPY: #12 | in: add-on | branch: loops-d | why: Loops lives there");
    assert_eq!(got, [Line::Copy { id: "12".into(), repo: "add-on".into(), branch: "loops-d".into(), why: "Loops lives there".into() }]);
    assert!(lines("COPY: #12 | in: add-on | branch: -x").is_empty(), "not a branch name");
    assert!(lines("COPY: #12 | in: add-on | branch: a b").is_empty(), "not a branch name");
    assert!(lines("COPY: #12 | branch: loops-d").is_empty(), "no repository");
}

#[test]
fn an_item_given_a_copy_of_the_add_on_works_there_on_its_branch_and_isnt_stopped() {
    let w = World::new();
    let (dir, addon, project, id, h, _) = routed(&w, |_| {});
    let it = item(&w, &project, "Loops band");
    let copy = copy_of(&it);
    assert!(copy.starts_with(&addon), "a copy of the add-on's repository: {}", copy.display());
    assert_eq!(git(&copy, &["branch", "--show-current"]).trim(), "loops-d");
    assert_eq!(git(&copy, &["rev-parse", "HEAD"]), git(&addon, &["rev-parse", "main"]), "a new branch, from its main line");
    let told: Vec<String> = w.hosted.sent().into_iter().filter(|(s, t)| *s == h && t.contains(&format!("Desk item #{id}"))).map(|(_, t)| t).collect();
    assert!(told.iter().any(|t| t.contains(&copy.to_string_lossy().to_string()) && t.contains("on loops-d as one commit")), "{told:?}");
    let s = w.session(&h).in_folder(&dir);
    let c = copy.to_string_lossy().to_string();
    assert!(s.guard_tool("Edit", json!({ "file_path": format!("{c}/mac/lib.rs") })).is_none(), "an edit in its copy passes");
    assert!(s.guard_tool("Bash", json!({ "command": format!("cd {c} && git add -A && git commit -m wip") })).is_none(), "and git there");
    let told: Value = s.guard_tool("Edit", json!({ "file_path": addon.join("mac/lib.rs") })).expect("stopped").told().unwrap();
    assert_eq!(told["behavior"], "deny", "the add-on's main checkout isn't its copy: {told}");
    assert!(told["message"].as_str().unwrap().contains("commits it on its branch"), "{told}");
}

#[test]
fn done_in_a_repository_named_by_its_folder_commits_its_work_on_the_named_branch_and_nothing_on_main() {
    let w = World::new();
    let (dir, addon, project, id, h, _) = routed_in(&w, Named::Other, |_| {});
    let copy = copy_of(&item(&w, &project, "Loops band"));
    let cue_main = git(&dir, &["rev-parse", "HEAD"]);
    let addon_main = git(&addon, &["rev-parse", "main"]);
    std::fs::write(copy.join("mac/a.rs"), "a\n").unwrap();
    git(&copy, &["add", "mac/a.rs"]);
    git(&copy, &["commit", "-q", "-m", "wip"]);
    std::fs::write(copy.join("mac/b.rs"), "b\n").unwrap();
    ends(&w, &h, &format!("Built.\nDONE: #{id}"));
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), cue_main, "nothing on Cue's main line");
    assert_eq!(git(&addon, &["rev-parse", "main"]), addon_main, "nor the add-on's");
    assert_eq!(git(&addon, &["log", "-1", "--format=%s", "loops-d"]).trim(), format!("Loops band (#{id})"));
    assert_eq!(git(&addon, &["diff", "--name-only", "main", "loops-d"]).trim(), "mac/a.rs\nmac/b.rs");
    let it = item(&w, &project, "Loops band");
    assert_eq!(it["state"], "to_check", "{it}");
    assert!(it["tree"].is_null() && !copy.exists(), "the copy is gone");
    assert!(!git(&addon, &["branch", "--list", "loops-d"]).trim().is_empty(), "its branch stays");
    let commits: Vec<&str> = it["commit"].as_str().unwrap().split_whitespace().collect();
    assert_eq!(commits.len(), 2, "its own commit and the one for what wasn't committed: {it}");
    let diff = w.hub().desk_item_diff(&id).unwrap();
    let diff = diff["diff"].as_str().unwrap();
    assert!(diff.contains("mac/a.rs") && diff.contains("mac/b.rs"), "its Files changed show it: {diff}");
}

#[test]
fn a_branch_that_has_work_already_is_where_its_copy_starts() {
    let w = World::new();
    let (_, addon, project, _, _, _) = routed(&w, |addon| {
        git(addon, &["switch", "-q", "-c", "loops-d"]);
        std::fs::write(addon.join("mac/loops.rs"), "earlier\n").unwrap();
        git(addon, &["add", "mac/loops.rs"]);
        git(addon, &["commit", "-q", "-m", "earlier Loops work"]);
        git(addon, &["switch", "-q", "main"]);
    });
    let copy = copy_of(&item(&w, &project, "Loops band"));
    assert_eq!(git(&copy, &["rev-parse", "HEAD"]), git(&addon, &["rev-parse", "loops-d"]));
    assert_eq!(std::fs::read_to_string(copy.join("mac/loops.rs")).unwrap(), "earlier\n");
}

#[test]
fn an_item_that_has_a_copy_of_cue_already_is_told_to_give_it_back_first() {
    let w = World::new();
    let (_, _, project, id, _, d) = routed(&w, |_| {});
    // Its copy of the add-on asked again, the same: nothing changes.
    ends(&w, &d, &format!("COPY: #{id} | in: add-on | branch: loops-d | why: again"));
    w.advance(2000);
    w.settle();
    assert_eq!(item(&w, &project, "Loops band")["tree"]["branch"], "loops-d");
    // Another branch: refused, with what to do.
    ends(&w, &d, &format!("COPY: #{id} | in: add-on | branch: loops-e | why: other"));
    w.advance(2000);
    w.settle();
    let to_lead: Vec<String> = w.hosted.sent().into_iter().filter(|(s, _)| *s == d).map(|(_, t)| t).collect();
    assert!(to_lead.iter().any(|t| t.contains(&format!("#{id} got no copy: it has one already")) && t.contains("TAKE COPY")), "{to_lead:?}");
}

/// The World as Cue's own source with a fake installer, and the add-on item routed: `routed`'s, and the installer.
fn routed_in_cue(w: &World) -> (Arc<Fake>, PathBuf, PathBuf, String, String, String) {
    routed_in_cue_as(w, Named::AddOn)
}

/// `routed_in_cue`, the COPY line as `named` says.
fn routed_in_cue_as(w: &World, named: Named) -> (Arc<Fake>, PathBuf, PathBuf, String, String, String) {
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, addon, project, id, h, _) = routed_in(w, named, |_| {});
    as_this_cue(&dir);
    (fake, dir, addon, project, id, h)
}

#[test]
fn done_on_the_add_on_merges_its_branch_into_the_add_ons_main_line_and_installs_cue_with_it() {
    let w = World::new();
    let (fake, dir, addon, project, id, h) = routed_in_cue(&w);
    let copy = copy_of(&item(&w, &project, "Loops band"));
    let addon_main = git(&addon, &["rev-parse", "main"]);
    std::fs::write(copy.join("mac/loops.rs"), "band\n").unwrap();
    ends(&w, &h, &format!("Built.\nDONE: #{id}"));
    // On the add-on's main line, as a merge of its branch; its branch and its commit kept.
    assert_eq!(std::fs::read_to_string(addon.join("mac/loops.rs")).unwrap(), "band\n", "the add-on's main checkout has it");
    assert_eq!(git(&addon, &["rev-parse", "main^1"]), addon_main, "a merge onto the main line as it was");
    assert_eq!(git(&addon, &["rev-parse", "main^2"]), git(&addon, &["rev-parse", "loops-d"]));
    assert_eq!(git(&addon, &["log", "-1", "--format=%s", "loops-d"]).trim(), format!("Loops band (#{id})"));
    let it = item(&w, &project, "Loops band");
    assert!(it["tree"].is_null() && !copy.exists(), "the copy is gone: {it}");
    for c in it["commit"].as_str().unwrap().split_whitespace() {
        assert!(git(&addon, &["branch", "--contains", c, "--format=%(refname:short)"]).lines().any(|b| b.trim() == "main"), "{c} is on the add-on's main line");
    }
    // The landing check counts it as landed: nothing left off a main line.
    assert_eq!(w.hub().desk_unlanded(&id), 0);
    // Queued for an install of the Cue it's built into, the add-on's new tip what that install has to have.
    assert_eq!(it["state"], "finishing", "{it}");
    assert_eq!(it["finish"]["step"], "queued", "{it}");
    assert_eq!(PathBuf::from(it["finish"]["repo"].as_str().unwrap()).canonicalize().unwrap(), dir.canonicalize().unwrap());
    let addon_head = git(&addon, &["rev-parse", "main"]).trim().to_string();
    assert_eq!(it["finish"]["addon"], addon_head);
    w.hub().desk_item_act(&id, "install_now", "").unwrap();
    w.settle();
    let started = fake.started();
    assert_eq!(started.len(), 1, "{started:?}");
    assert_eq!(PathBuf::from(&started[0].0).canonicalize().unwrap(), dir.canonicalize().unwrap(), "Cue is what's built, the add-on in it");
    // Cue's commit alone isn't enough: the running Cue must have the add-on's too.
    fake.runs(git(&dir, &["rev-parse", "HEAD"]).trim());
    fake.goes("installed", "");
    tick(&w);
    assert_ne!(item(&w, &project, "Loops band")["state"], "to_check", "not until the running Cue has the add-on's commit");
    fake.runs(&addon_head);
    tick(&w);
    assert_eq!(item(&w, &project, "Loops band")["state"], "to_check");
}

#[test]
fn add_on_work_that_clashes_with_its_main_line_goes_back_to_its_holder_with_its_copy_kept() {
    let w = World::new();
    let (_fake, _dir, addon, project, id, h) = routed_in_cue(&w);
    let copy = copy_of(&item(&w, &project, "Loops band"));
    std::fs::write(addon.join("mac/lib.rs"), "fn main_line() {}\n").unwrap();
    git(&addon, &["commit", "-q", "-am", "main moved on"]);
    let addon_main = git(&addon, &["rev-parse", "main"]);
    std::fs::write(copy.join("mac/lib.rs"), "fn mine() {}\n").unwrap();
    ends(&w, &h, &format!("Built.\nDONE: #{id}"));
    assert_eq!(git(&addon, &["rev-parse", "main"]), addon_main, "nothing merged");
    let it = item(&w, &project, "Loops band");
    assert_eq!(it["state"], "working", "back to its holder: {it}");
    assert!(copy.exists() && !it["tree"].is_null(), "its copy stays to settle it in: {it}");
    let told = sent_to(&w, &h);
    assert!(told.iter().any(|t| t.contains(&format!("Desk item #{id}")) && t.contains("clashes") && t.contains("mac/lib.rs")), "{told:?}");
    // Settled in its copy and DONE again: it lands.
    git(&copy, &["merge", "-q", "-X", "ours", "main", "-m", "settled"]);
    ends(&w, &h, &format!("Settled.\nDONE: #{id}"));
    assert_eq!(item(&w, &project, "Loops band")["finish"]["step"], "queued");
    assert!(!copy.exists());
    assert_eq!(std::fs::read_to_string(addon.join("mac/lib.rs")).unwrap(), "fn mine() {}\n", "its settled work on the add-on's main line");
}

#[test]
fn add_on_work_outside_what_cue_builds_goes_to_check_with_no_install() {
    let w = World::new();
    let (fake, _dir, addon, project, id, h) = routed_in_cue(&w);
    let copy = copy_of(&item(&w, &project, "Loops band"));
    std::fs::create_dir_all(copy.join("mocks")).unwrap();
    std::fs::write(copy.join("mocks/band.html"), "<p>band</p>\n").unwrap();
    ends(&w, &h, &format!("Mocks.\nDONE: #{id}"));
    assert!(addon.join("mocks/band.html").exists(), "on the add-on's main line");
    assert_eq!(item(&w, &project, "Loops band")["state"], "to_check");
    assert!(fake.started().is_empty());
}

/// Its add-on work landed: merged into the add-on's main line, nothing left off it, and Cue queued to install with
/// the add-on's new tip.
fn landed_on_add_on(w: &World, addon: &Path, project: &str, id: &str, file: &str) {
    assert!(addon.join(file).exists(), "the add-on's main checkout has {file}");
    assert_eq!(w.hub().desk_unlanded(id), 0, "nothing left off a main line");
    let it = item(w, project, "Loops band");
    assert_eq!(it["finish"]["step"], "queued", "{it}");
    assert_eq!(it["finish"]["addon"], git(addon, &["rev-parse", "main"]).trim(), "{it}");
}

#[test]
fn the_add_on_named_by_its_folder_lands_as_the_add_on() {
    let w = World::new();
    let (_fake, _dir, addon, project, id, h) = routed_in_cue_as(&w, Named::AddOnFolder);
    let copy = copy_of(&item(&w, &project, "Loops band"));
    std::fs::write(copy.join("mac/loops.rs"), "band\n").unwrap();
    ends(&w, &h, &format!("Built.\nDONE: #{id}"));
    landed_on_add_on(&w, &addon, &project, &id, "mac/loops.rs");
}

#[test]
fn a_copy_of_the_add_on_made_by_a_cue_from_before_565_still_lands_on_its_main_line() {
    let w = World::new();
    let (_fake, _dir, addon, project, id, h) = routed_in_cue(&w);
    w.hub().desk_copy_unnoted(&id);
    let copy = copy_of(&item(&w, &project, "Loops band"));
    std::fs::write(copy.join("mac/loops.rs"), "band\n").unwrap();
    ends(&w, &h, &format!("Built.\nDONE: #{id}"));
    landed_on_add_on(&w, &addon, &project, &id, "mac/loops.rs");
}

/// #582: its add-on work committed on desk/<id> by an earlier DONE and its copy gone, it works on in Cue (a copy of
/// Cue, or its holder's folder) and says DONE: its add-on branch is merged then, not refused as "not landed".
#[test]
fn add_on_work_left_on_its_branch_by_an_earlier_done_is_merged_at_its_next_done_in_cue() {
    let w = World::new();
    let fake = Arc::new(Fake::default());
    w.hub().desk_set_installer(fake.clone());
    let (dir, addon, project, id, h, _) = routed_in(&w, Named::NoCopy, |_| {});
    as_this_cue(&dir);
    let branch = format!("desk/{id}");
    git(&addon, &["switch", "-q", "-c", &branch]);
    std::fs::write(addon.join("mac/loops.rs"), "from local main\n").unwrap();
    git(&addon, &["add", "mac/loops.rs"]);
    git(&addon, &["commit", "-q", "-m", "Loops band"]);
    let short = git(&addon, &["rev-parse", "--short", "HEAD"]).trim().to_string();
    git(&addon, &["switch", "-q", "main"]);
    w.hub().desk_add_commit_in(&id, &addon.to_string_lossy(), &short);
    assert_eq!(w.hub().desk_unlanded(&id), 1, "left on desk/{id}, as #582's was");
    ends(&w, &h, &format!("Done again.\nDONE: #{id}"));
    landed_on_add_on(&w, &addon, &project, &id, "mac/loops.rs");
    let told = sent_to(&w, &h);
    assert!(!told.iter().any(|t| t.contains("not landed yet")), "not sent back: {told:?}");
}
