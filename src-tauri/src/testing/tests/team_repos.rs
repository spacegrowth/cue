//! A hosted lead's teammate whose files are in more than one git repository (or in none): its Diff, counts, Commit
//! and the tidy, each per repo, against real git folders.

use super::team::{git, lead, mate, repo, says, writes, writes_then_stops};
use crate::testing::World;

/// A second git repository beside the World's folder (`<home>-<name>`), one commit on main, switched to a branch of
/// its own when `branch`; removed when dropped. Its folder name is what its paths are shown under.
struct OtherRepo(std::path::PathBuf);

impl OtherRepo {
    fn new(w: &World, name: &str, branch: bool) -> Self {
        Self::at(beside(w, name), branch)
    }
    /// The same, at `dir`.
    fn at(dir: std::path::PathBuf, branch: bool) -> Self {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mac/ui")).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "t@example.com"]);
        git(&dir, &["config", "user.name", "T"]);
        std::fs::write(dir.join("mac/ui/tracked.js"), "old\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "start"]);
        if branch {
            git(&dir, &["switch", "-q", "-c", "other-work"]);
        }
        OtherRepo(dir)
    }
    fn label(&self) -> String {
        self.0.file_name().unwrap().to_string_lossy().to_string()
    }
    fn file(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().to_string()
    }
}

impl Drop for OtherRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A folder beside the World's (`<home>-<name>`), outside its repo.
fn beside(w: &World, name: &str) -> std::path::PathBuf {
    w.home().with_file_name(format!("{}-{name}", w.home().file_name().unwrap().to_string_lossy()))
}

fn plus_lines(diff: &str) -> Vec<&str> {
    diff.lines().filter(|l| l.starts_with("+++")).collect()
}

#[test]
fn a_teammate_with_files_in_two_repos_shows_both_diffs_and_counts_and_commit_commits_each_in_its_own_repo() {
    let w = World::new();
    let dir = repo(&w);
    let other = OtherRepo::new(&w, "other-repo", true);
    let sid = lead(&w, "work across two repos");
    let files = [dir.join("a.txt").to_string_lossy().to_string(), other.file("mac/ui/addon.js"), other.file("mac/ui/tracked.js")];
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &files));
    let label = other.label();
    let diff = w.hub().team_action(&sid, "agentA", "diff").unwrap();
    let mut seen = plus_lines(&diff);
    seen.sort();
    let mut want = ["+++ b/a.txt".to_string(), format!("+++ b/{label}/mac/ui/addon.js"), format!("+++ b/{label}/mac/ui/tracked.js")];
    want.sort();
    assert_eq!(seen, want, "each repo's diff, the other repo's paths under its folder name:\n{diff}");
    assert_eq!(mate(&w, &sid, "rows")["stat"], "3 files +3 −1", "counted across both repos");
    let r = w.hub().team_action(&sid, "agentA", "commit").unwrap();
    let home = dir.file_name().unwrap().to_string_lossy().to_string();
    assert!(r.contains(&format!("In {home}: Committed rows's 1 file")) && r.contains(&format!("In {label}: Committed rows's 2 files")), "{r}");
    assert_eq!(git(&dir, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "rows's part\n\na.txt");
    assert_eq!(git(&other.0, &["show", "--name-only", "--format=%s", "HEAD"]).trim(), "rows's part\n\nmac/ui/addon.js\nmac/ui/tracked.js", "the same message, in its own repo");
    assert_eq!(git(&other.0, &["status", "--porcelain"]).trim(), "");
    assert_eq!(mate(&w, &sid, "rows")["state"], "committed");
}

#[test]
fn a_commit_that_fails_in_one_repo_says_which_and_keeps_what_the_other_committed() {
    let w = World::new();
    let dir = repo(&w);
    let other = OtherRepo::new(&w, "other-repo", false);
    let sid = lead(&w, "work across two repos");
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string(), other.file("mac/ui/addon.js")]));
    let label = other.label();
    let home = dir.file_name().unwrap().to_string_lossy().to_string();
    let r = w.hub().team_action(&sid, "agentA", "commit").unwrap_err();
    assert!(r.contains(&format!("In {home}: Committed rows's 1 file")), "the one that went through, said so: {r}");
    assert!(r.contains(&format!("In {label}: Not committed: its folder is on main")) && r.contains("Make a branch and commit"), "{r}");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "2", "committed in the lead's repo");
    assert_eq!(git(&other.0, &["rev-list", "--count", "HEAD"]).trim(), "1", "nothing on the other's main");
    assert_ne!(mate(&w, &sid, "rows")["state"], "committed", "work still waits in one repo");
    // Make a branch and commit: only what's left, in the repo that's on main.
    let r = w.hub().team_action(&sid, "agentA", "branch").unwrap();
    assert!(r.starts_with("Committed 1 file to crew/rows") && r.ends_with("your folder stays on main."), "{r}");
    assert_eq!(git(&other.0, &["show", "--name-only", "--format=", "crew/rows"]).trim(), "mac/ui/addon.js");
    assert_eq!(mate(&w, &sid, "rows")["state"], "committed");
}

#[test]
fn a_file_in_no_repo_is_said_so_in_the_diff_instead_of_failing_it() {
    let w = World::new();
    let dir = repo(&w);
    let loose = beside(&w, "loose");
    std::fs::create_dir_all(&loose).unwrap();
    let outside = loose.join("notes.txt").to_string_lossy().to_string();
    let more = loose.join("more.txt").to_string_lossy().to_string();
    let sid = lead(&w, "one inside, one outside");
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string(), outside.clone()]));
    says(&w, &sid, writes("toolu_B", "agentB", "hub", &[more.clone()]));
    let diff = w.hub().team_action(&sid, "agentA", "diff").unwrap();
    assert_eq!(plus_lines(&diff), ["+++ b/a.txt"], "{diff}");
    assert!(diff.contains(&format!("{outside}: not in a git repository")), "{diff}");
    let only_outside = w.hub().team_action(&sid, "agentB", "diff").unwrap();
    assert_eq!(only_outside.trim(), format!("{more}: not in a git repository"), "not \"No changes.\"");
    let r = w.hub().team_action(&sid, "agentB", "commit");
    assert!(r.as_ref().is_err_and(|e| e.contains("nothing to commit") && e.contains("not in a git repository")), "{r:?}");
    // Commit says which of its files it left out, and the teammate is done: there's nothing more git can commit.
    let r = w.hub().team_action(&sid, "agentA", "commit").unwrap();
    assert!(r.starts_with("Committed rows's 1 file") && r.contains(&format!("{outside}: not in a git repository, not committed")), "{r}");
    assert_eq!(mate(&w, &sid, "rows")["state"], "committed");
    let _ = std::fs::remove_dir_all(&loose);
}

#[test]
fn a_repo_git_cant_read_is_said_not_committed_and_the_teammate_stays_until_it_is() {
    let w = World::new();
    let dir = repo(&w);
    let other = OtherRepo::new(&w, "other-repo", true);
    let sid = lead(&w, "work across two repos");
    let addon = other.file("mac/ui/addon.js");
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string(), addon.clone()]));
    // The other repo's index broken: git finds the repo, but can't read what changed in it.
    std::fs::write(other.0.join(".git/index"), "not an index").unwrap();
    let r = w.hub().team_action(&sid, "agentA", "commit").unwrap_err();
    assert!(r.contains("Committed rows's 1 file") && r.contains(&format!("{addon}: git couldn't read its repository, not committed")), "{r}");
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]).trim(), "2", "the readable repo committed");
    assert_ne!(mate(&w, &sid, "rows")["state"], "committed", "its work in the other repo still waits");
    std::fs::remove_file(other.0.join(".git/index")).unwrap();
    git(&other.0, &["reset", "-q"]);
    let r = w.hub().team_action(&sid, "agentA", "commit").unwrap();
    assert!(r.starts_with("Committed rows's 1 file"), "only what was left: {r}");
    assert_eq!(git(&other.0, &["show", "--name-only", "--format=", "HEAD"]).trim(), "mac/ui/addon.js");
    assert_eq!(mate(&w, &sid, "rows")["state"], "committed");
}

#[test]
fn two_other_repos_with_one_folder_name_are_told_apart_by_their_parent_folder() {
    let w = World::new();
    let dir = repo(&w);
    let (x, y) = (beside(&w, "x"), beside(&w, "y"));
    let (one, two) = (OtherRepo::at(x.join("app"), true), OtherRepo::at(y.join("app"), true));
    let sid = lead(&w, "work in two apps");
    says(&w, &sid, writes("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string(), one.file("mac/ui/addon.js"), two.file("mac/ui/addon.js")]));
    let diff = w.hub().team_action(&sid, "agentA", "diff").unwrap();
    let name = |p: &std::path::Path| p.file_name().unwrap().to_string_lossy().to_string();
    let mut seen = plus_lines(&diff);
    seen.sort();
    let mut want = ["+++ b/a.txt".to_string(), format!("+++ b/{}/app/mac/ui/addon.js", name(&x)), format!("+++ b/{}/app/mac/ui/addon.js", name(&y))];
    want.sort();
    assert_eq!(seen, want, "{diff}");
    let r = w.hub().team_action(&sid, "agentA", "commit").unwrap();
    assert!(r.contains(&format!("In {}/app: Committed", name(&x))) && r.contains(&format!("In {}/app: Committed", name(&y))), "{r}");
    drop((one, two));
    let _ = std::fs::remove_dir_all(&x);
    let _ = std::fs::remove_dir_all(&y);
}

#[test]
fn the_tidy_sees_a_teammates_files_in_another_repo_committed_there_as_committed() {
    let w = World::new();
    let dir = repo(&w);
    let other = OtherRepo::new(&w, "other-repo", true);
    let sid = lead(&w, "work across two repos");
    says(&w, &sid, writes_then_stops("toolu_A", "agentA", "rows", &[dir.join("a.txt").to_string_lossy().to_string(), other.file("mac/ui/addon.js")]));
    says(&w, &sid, writes_then_stops("toolu_B", "agentB", "hub", &[other.file("mac/ui/tracked.js")]));
    assert!(!w.hub().team_tidy());
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "rows by hand, here"]);
    assert!(!w.hub().team_tidy(), "rows's file in the other repo still uncommitted");
    git(&other.0, &["add", "mac/ui/addon.js", "mac/ui/tracked.js"]);
    git(&other.0, &["commit", "-q", "-m", "by hand, there"]);
    assert!(w.hub().team_tidy());
    let rows = mate(&w, &sid, "rows");
    assert_eq!((rows["state"].as_str(), rows["step"].as_str()), (Some("committed"), Some("Committed outside Cue")));
    let hub = mate(&w, &sid, "hub");
    let there = git(&other.0, &["rev-parse", "--short", "HEAD"]);
    assert_eq!((hub["state"].as_str(), hub["commit"].as_str()), (Some("committed"), Some(there.trim())), "all its files in the other repo, committed there");
}

#[test]
fn scratch_and_ignored_files_dont_keep_a_committed_member_and_commit_on_committed_work_lets_it_go() {
    let w = World::new();
    let dir = repo(&w);
    std::fs::write(dir.join(".gitignore"), "shots/\n").unwrap();
    git(&dir, &["add", ".gitignore"]);
    git(&dir, &["commit", "-q", "-m", "ignore shots"]);
    std::fs::create_dir_all(dir.join("shots")).unwrap();
    std::fs::write(dir.join("shots/look.png"), "png").unwrap();
    let scratch = beside(&w, "scratch-notes.txt");
    std::fs::write(&scratch, "notes\n").unwrap();
    let sid = lead(&w, "work and screenshots");
    let files = [dir.join("a.txt").to_string_lossy().to_string(), dir.join("shots/look.png").to_string_lossy().to_string(), scratch.to_string_lossy().to_string()];
    says(&w, &sid, writes_then_stops("toolu_A", "agentA", "rows", &files));
    says(&w, &sid, writes_then_stops("toolu_B", "agentB", "hub", &[dir.join("b.txt").to_string_lossy().to_string()]));
    assert!(!w.hub().team_tidy(), "a.txt not committed yet");
    // Committed by hand: its screenshot (ignored) and its notes (in no repo) don't keep it.
    git(&dir, &["add", "a.txt"]);
    git(&dir, &["commit", "-q", "-m", "rows by hand"]);
    assert!(w.hub().team_tidy());
    assert_eq!(mate(&w, &sid, "rows")["state"], "committed");
    // Commit on work already committed elsewhere: it leaves as committed, not "nothing to commit".
    git(&dir, &["add", "b.txt"]);
    git(&dir, &["commit", "-q", "-m", "hub by hand"]);
    let said = w.hub().team_action(&sid, "agentB", "commit").unwrap();
    assert!(said.contains("already committed"), "{said}");
    assert_eq!(mate(&w, &sid, "hub")["state"], "committed");
    let _ = std::fs::remove_file(&scratch);
}
