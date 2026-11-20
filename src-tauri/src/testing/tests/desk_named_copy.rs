//! An item at work in the add-on's own repository, on a branch the lead names (#557, desk/hub/landing/named_copy.rs):
//! `COPY: #<id> | in: add-on | branch: loops-d` gives it a copy of that repository on that branch as it starts; its
//! holder's edits there pass; at its DONE its work is committed on that branch and stays there, nothing on Cue's main
//! line, and the item goes to check with that commit, its Files changed showing it.

use super::desk_commit::{ends, item};
use super::team::{git, lead, main_repo};
use crate::desk::parse::{lines, Line};
use crate::hosted::HostedEvent;
use crate::testing::World;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

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

/// A Desk whose lead adds "Loops band", gives it a copy of the add-on on `loops-d` and routes it: (cue's repository,
/// the add-on's, project, item id, holder, the lead). `before` runs on the add-on before the lead's turn.
fn routed(w: &World, before: impl Fn(&Path)) -> (PathBuf, PathBuf, String, String, String, String) {
    let dir = main_repo(w);
    let addon = add_on(&dir);
    before(&addon);
    let h = lead(w, "hello");
    ends(w, &h, "Hi.");
    let project = w.snapshot().session(&h)["project"].as_str().unwrap().to_string();
    w.hub().desk_set_member(&project, &h, true).unwrap();
    let r = crate::ipc::desk_set_lead_with(w.hub(), &project, None, true).unwrap();
    let d = r["sid"].as_str().unwrap().to_string();
    w.hosted_says(&d, HostedEvent::Taken { text: "hello".into() });
    ends(w, &d, "I run the Desk now.");
    w.advance(1000);
    ends(w, &d, "ITEM: task | Loops band | done when: it shows\nCOPY: #new | in: add-on | branch: loops-d | why: it's add-on work\nROUTE: #new -> cue-teams | why: it knows Loops");
    w.advance(1000);
    let id = item(w, &project, "Loops band")["id"].as_str().unwrap().to_string();
    (dir, addon, project, id, h, d)
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
fn done_commits_its_work_on_the_named_branch_and_nothing_on_main() {
    let w = World::new();
    let (dir, addon, project, id, h, _) = routed(&w, |_| {});
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
