//! A hosted lead's teammate whose files are in more than one git repository (or in none): its Diff, counts, Commit
//! and the tidy, each per repo, against real git folders; and one whose files Cue never saw, committed by its lead.

use super::team::{git, lead, mate, repo, says, writes, writes_then_stops};
use crate::hosted::HostedEvent;
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

/// The lead's turn ends with `text`, a reply to something of yours.
fn replies(w: &World, sid: &str, text: &str) {
    w.hosted_says(sid, HostedEvent::TurnEnded { text: text.into(), failed: false, cost_usd: 0.0, unprompted: false });
    w.settle();
}

/// Skip on member `agent`'s queued review (its row's Skip): Commit is what these cases are about.
fn skip_review(w: &World, sid: &str, agent: &str) {
    w.hub().unqueue_review(&format!("team:{sid}:{agent}"));
}

/// What lead `sid` was sent asking it to commit a member's work.
fn commit_asks(w: &World, sid: &str) -> Vec<String> {
    w.hosted.sent().into_iter().filter(|(s, t)| s == sid && t.starts_with("Commit your teammate ")).map(|(_, t)| t).collect()
}

#[test]
fn commit_on_a_member_whose_files_cue_never_saw_asks_its_lead_and_its_committed_line_marks_it_committed() {
    let w = World::new();
    let _dir = repo(&w);
    let sid = lead(&w, "work done with shell commands elsewhere");
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    skip_review(&w, &sid, "agentA");
    let said = w.hub().team_action(&sid, "agentA", "commit").unwrap();
    assert_eq!(said, "Cue asks its lead to commit board's work once it's free");
    let t = mate(&w, &sid, "board");
    assert!(t["commit_wait_ms"].as_u64().is_some_and(|ms| ms > 0) && t["step"] == "Cue asks its lead to commit it when it's free", "{t}");
    assert!(w.hub().team_action(&sid, "agentA", "commit").is_err(), "asked once");
    assert!(!w.hub().team_tidy(), "the tidy leaves it alone while its lead commits it");
    // The lead was busy: the ask waits on its row until the lead is free, so that turn's reply isn't the answer.
    replies(&w, &sid, "started board");
    assert!(mate(&w, &sid, "board")["commit_asked_ms"].is_null(), "not asked yet");
    w.advance(2000);
    let t = mate(&w, &sid, "board");
    assert!(t["commit_asked_ms"].as_u64().is_some_and(|ms| ms > 0) && t["step"] == "Asked its lead to commit it", "{t}");
    let asks = commit_asks(&w, &sid);
    assert_eq!(asks.len(), 1, "sent once the lead was free: {:?}", w.hosted.sent());
    assert!(asks[0].starts_with("Commit your teammate board's work now: the files it changed") && asks[0].contains("make a branch crew/board first") && asks[0].ends_with("End your reply with a line `Committed board: <short sha>` for each commit, or `Committed board: nothing` if there was nothing to commit."), "{}", asks[0]);
    w.hosted_says(&sid, HostedEvent::Taken { text: asks[0].clone() });
    replies(&w, &sid, "Committed the board files on crew/board.\nCommitted board: abc1234\nCommitted board: def5678");
    let t = mate(&w, &sid, "board");
    assert_eq!((t["state"].as_str(), t["commit"].as_str(), t["step"].as_str()), (Some("committed"), Some("abc1234"), Some("Committed by its lead")), "{t}");
    assert!(t["commit_asked_ms"].is_null());
}

#[test]
fn committed_nothing_sets_it_aside_a_reply_without_either_leaves_it_to_you_and_a_review_meanwhile_is_still_a_review() {
    let w = World::new();
    let dir = repo(&w);
    let sid = lead(&w, "three parts");
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    says(&w, &sid, writes("toolu_C", "agentC", "rows", &[dir.join("a.txt").to_string_lossy().to_string()]));
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    says(&w, &sid, writes("toolu_B", "agentB", "notes", &[]));
    skip_review(&w, &sid, "agentA");
    skip_review(&w, &sid, "agentB");
    // Asked while the lead works: it goes once the lead is free (before rows's queued review), and the lead's next
    // reply is its answer.
    w.hub().team_action(&sid, "agentA", "commit").unwrap();
    replies(&w, &sid, "all three finished");
    w.advance(2000);
    assert_eq!(commit_asks(&w, &sid).len(), 1);
    assert_eq!(mate(&w, &sid, "rows")["state"], "queued", "its review goes after, once the lead settles");
    replies(&w, &sid, "Nothing of board's was left to commit.\nCommitted board: nothing");
    let t = mate(&w, &sid, "board");
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("aside"), Some("Nothing to commit")), "{t}");
    // Asked while the lead is free: it goes at once.
    w.advance(10_000);
    w.hub().team_action(&sid, "agentB", "commit").unwrap();
    assert_eq!(commit_asks(&w, &sid).len(), 2);
    replies(&w, &sid, "I couldn't find notes's files.\nThey may be gone.");
    let t = mate(&w, &sid, "notes");
    assert_eq!(t["step"], "Its lead didn't say it committed: I couldn't find notes's files.", "{t}");
    assert!(t["commit_asked_ms"].is_null(), "no longer waiting on its lead: yours");
    assert_eq!(t["needs"], "Its lead didn't say it committed it", "waiting on you, in the data: {t}");
    // A review under way, and notes asked again meanwhile: the review's reply files as rows's review, and notes's ask
    // waits for the lead.
    w.advance(60_000);
    w.hub().drain_reviews();
    w.settle();
    assert_eq!(mate(&w, &sid, "rows")["state"], "reviewing");
    w.hub().team_action(&sid, "agentB", "commit").unwrap();
    assert!(mate(&w, &sid, "notes")["needs"].is_null(), "asked again: its lead's again, not yours");
    replies(&w, &sid, "1. a.txt:1 — should-fix — say more\nRecommendation: send fixes");
    let rows = mate(&w, &sid, "rows");
    assert_eq!((rows["state"].as_str(), rows["recommendation"].as_str()), (Some("reviewed"), Some("send fixes")), "{rows}");
    assert!(mate(&w, &sid, "notes")["commit_wait_ms"].as_u64().is_some_and(|ms| ms > 0), "the review wasn't its answer: its ask still waits");
    // Its ask goes now; the lead again says nothing of it: yours, until something settles it.
    w.advance(10_000);
    w.hub().drain_reviews();
    w.settle();
    assert_eq!(commit_asks(&w, &sid).len(), 3);
    replies(&w, &sid, "Still can't find them.");
    assert_eq!(mate(&w, &sid, "notes")["needs"], "Its lead didn't say it committed it");
    w.hub().team_action(&sid, "agentB", "aside").unwrap();
    let t = mate(&w, &sid, "notes");
    assert_eq!((t["state"].as_str(), t["needs"].is_null()), (Some("aside"), true), "set aside, nothing waits on you: {t}");
}

#[test]
fn handled_by_cue_a_clean_review_of_a_member_whose_files_cue_never_saw_asks_its_lead_to_commit_it() {
    let w = World::new();
    let _dir = repo(&w);
    let sid = lead(&w, "one part, done with shell commands elsewhere");
    says(&w, &sid, writes("toolu_A", "agentA", "board", &[]));
    replies(&w, &sid, "started board");
    w.hub().drain_reviews();
    w.settle();
    assert_eq!(mate(&w, &sid, "board")["state"], "reviewing");
    w.advance(30_000); // the review takes a while
    replies(&w, &sid, "Recommendation: commit");
    let asks = commit_asks(&w, &sid);
    assert_eq!(asks.len(), 1, "asked by itself: {:?}", w.hosted.sent());
    let t = mate(&w, &sid, "board");
    assert_eq!((t["state"].as_str(), t["step"].as_str()), (Some("reviewed"), Some("Asked its lead to commit it")), "{t}");
    replies(&w, &sid, "Committed: 0a1b2c3");
    assert_eq!(mate(&w, &sid, "board")["state"], "committed");
}

#[test]
fn two_asks_joined_for_a_busy_lead_are_each_settled_by_the_line_naming_it_and_a_review_reply_answers_too() {
    let w = World::new();
    let _dir = repo(&w);
    let sid = lead(&w, "three parts, done with shell commands elsewhere");
    w.hub().set_crew_commit(&sid, "wait").unwrap();
    for (call, id, name) in [("toolu_A", "agentA", "board"), ("toolu_B", "agentB", "notes"), ("toolu_C", "agentC", "rows")] {
        says(&w, &sid, writes(call, id, name, &[]));
        skip_review(&w, &sid, id);
    }
    // Both asked while the lead works: they wait, and go joined into one message once it's free.
    w.hub().team_action(&sid, "agentA", "commit").unwrap();
    w.hub().team_action(&sid, "agentB", "commit").unwrap();
    replies(&w, &sid, "all three finished");
    w.advance(2000);
    assert_eq!(commit_asks(&w, &sid).len(), 1);
    let sent = w.hosted.sent().last().unwrap().1.clone();
    assert!(sent.contains("Commit your teammate board's") && sent.contains("Commit your teammate notes's"), "one message: {sent}");
    w.hosted_says(&sid, HostedEvent::Taken { text: sent });
    replies(&w, &sid, "Done.\nCommitted board: abc1234\nCommitted notes: def5678");
    let (board, notes) = (mate(&w, &sid, "board"), mate(&w, &sid, "notes"));
    assert_eq!((board["state"].as_str(), board["commit"].as_str()), (Some("committed"), Some("abc1234")), "{board}");
    assert_eq!((notes["state"].as_str(), notes["commit"].as_str()), (Some("committed"), Some("def5678")), "{notes}");
    // A reply with a Recommendation line: its Committed line answers the ask; saying nothing of it leaves it asked.
    w.advance(10_000);
    w.hub().team_action(&sid, "agentC", "commit").unwrap();
    assert_eq!(commit_asks(&w, &sid).len(), 2, "the lead free: at once");
    w.hosted_says(&sid, HostedEvent::Taken { text: commit_asks(&w, &sid).last().unwrap().clone() });
    replies(&w, &sid, "1. x.rs:1 — note — fine\nRecommendation: commit");
    assert!(mate(&w, &sid, "rows")["commit_asked_ms"].as_u64().is_some_and(|ms| ms > 0), "a review saying nothing of it is no answer");
    assert!(mate(&w, &sid, "rows")["needs"].is_null());
    replies(&w, &sid, "Recommendation: commit\nCommitted rows: 0a1b2c3");
    let rows = mate(&w, &sid, "rows");
    assert_eq!((rows["state"].as_str(), rows["commit"].as_str()), (Some("committed"), Some("0a1b2c3")), "{rows}");
}

#[test]
fn a_leads_committed_lines_are_read_by_name_with_their_markdown_and_anything_else_is_neither() {
    use crate::sessions::team::read_committed;
    let both = "Done.\n**Committed board:** `abc1234`\n- Committed Board: DEF5678.\nCommitted notes: 1234567";
    assert_eq!(read_committed(both, "board", false), Some(vec!["abc1234".to_string(), "def5678".to_string()]));
    assert_eq!(read_committed(both, "notes", false), Some(vec!["1234567".to_string()]));
    assert_eq!(read_committed(both, "rows", false), None, "another's lines aren't its");
    assert_eq!(read_committed("Committed notes: nothing", "notes", false), Some(vec![]));
    assert_eq!(read_committed("Committed: abc1234", "board", true), Some(vec!["abc1234".to_string()]), "a bare line, when it's the only one asked");
    assert_eq!(read_committed("Committed: abc1234", "board", false), None, "a bare line, with two asked");
    assert_eq!(read_committed("Committed board: the files, on crew/board", "board", true), None, "no sha");
    assert_eq!(read_committed("I committed it as abc1234", "board", true), None);
}
