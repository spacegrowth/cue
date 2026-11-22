//! Two items' changes in one file of the shared folder, each made by an inline script (#658): the one that lands
//! commits only its own hunks, told by which session's commands hold their text (desk/hub/theirs/by_hunk.rs), and main
//! still builds; the other item's half-done hunk stays in the folder, uncommitted. e7361cace took the whole of
//! trees.rs into #650's commit; after #653 the file was left out whole, the landing item's own part with it.

use super::desk::desk;
use super::desk_commit::{done_saying, ends, item, log};
use super::team::git;
use crate::hosted::HostedEvent;
use crate::ipc::{start_session_with, StartOpts};
use crate::testing::World;
use serde_json::json;

const TREES: &str = "pub fn land() -> u32 {\n    1\n}\n\npub fn tidy() -> u32 {\n    2\n}\n";
const MAIN: &str = "mod trees;\n\nfn main() {\n    println!(\"{}\", trees::land() + trees::tidy());\n}\n";
/// Item A's: a function in trees.rs, and main.rs calling it.
const TREES_A: &str = "pub fn land() -> u32 {\n    1\n}\n\npub fn land_more() -> u32 {\n    3\n}\n\npub fn tidy() -> u32 {\n    2\n}\n";
const MAIN_A: &str = "mod trees;\n\nfn main() {\n    println!(\"{}\", trees::land() + trees::land_more() + trees::tidy());\n}\n";
/// Both items' trees.rs: item B's half-done call (to a function not written yet) in tidy.
const TREES_BOTH: &str = "pub fn land() -> u32 {\n    1\n}\n\npub fn land_more() -> u32 {\n    3\n}\n\npub fn tidy() -> u32 {\n    tidy_half_done()\n}\n";

fn commit_at_now(dir: &std::path::Path, msg: &str) {
    let at = format!("@{} +0000", crate::clock::now_ms() / 1000);
    let o = std::process::Command::new(crate::safe_git::git_bin()).arg("-C").arg(dir).args(["commit", "-q", "-m", msg])
        .env("GIT_COMMITTER_DATE", &at).env("GIT_AUTHOR_DATE", &at).output().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

/// An inline script run in `dir`, writing `file` with `from` replaced by `to`, the way most sessions edit.
fn script(dir: &std::path::Path, file: &str, from: &str, to: &str) -> String {
    format!("cd {} && python3 - <<'EOF'\np='{file}'\ns=open(p).read()\nopen(p,'w').write(s.replace('''{from}''', '''{to}'''))\nEOF", dir.display())
}

/// The folder's HEAD builds: its files, out of git, through rustc.
fn head_builds(dir: &std::path::Path) -> Result<(), String> {
    let out = std::env::temp_dir().join(format!("cue-hunks-build-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(out.join("src")).unwrap();
    for f in ["src/main.rs", "src/trees.rs"] {
        std::fs::write(out.join(f), git(dir, &["show", &format!("HEAD:{f}")])).unwrap();
    }
    let o = std::process::Command::new("rustc").args(["--edition", "2021", "--crate-name", "hunks", "-o"]).arg(out.join("hunks")).arg(out.join("src/main.rs")).output().unwrap();
    let _ = std::fs::remove_dir_all(&out);
    if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).into_owned()) }
}

#[test]
fn landing_one_item_commits_only_its_hunks_of_a_file_another_item_is_still_changing() {
    let w = World::new();
    let (dir, project, id) = done_saying(&w, |_, _, dir| {
        std::fs::write(dir.join(".gitignore"), "*\n!README\n!.gitignore\n!src/\n!src/*.rs\n").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/trees.rs"), TREES).unwrap();
        std::fs::write(dir.join("src/main.rs"), MAIN).unwrap();
        git(dir, &["add", ".gitignore", "src"]);
        commit_at_now(dir, "trees");
    }, |w, h, dir| {
        w.advance(1000);
        // Item A (this holder's): a function in trees.rs and its call in main.rs, by inline scripts.
        std::fs::write(dir.join("src/trees.rs"), TREES_A).unwrap();
        log(w, h).tool_use("toolu_a1", "Bash", json!({ "command": script(dir, "src/trees.rs", "pub fn tidy", "pub fn land_more() -> u32 {\n    3\n}\n\npub fn tidy") }));
        std::fs::write(dir.join("src/main.rs"), MAIN_A).unwrap();
        log(w, h).tool_use("toolu_a2", "Bash", json!({ "command": script(dir, "src/main.rs", "trees::land() +", "trees::land() + trees::land_more() +") }));
        w.advance(1000);
        // Item B, routed to another session at work in the same folder: its half-done change to tidy.
        let project = w.snapshot().session(h)["project"].as_str().unwrap().to_string();
        let opts = StartOpts { hosted: true, perm: "auto".into(), ..Default::default() };
        let r = start_session_with(w.hub(), "claude", &dir.to_string_lossy(), "hello", "tidier", &opts).unwrap();
        let b = r["session_id"].as_str().unwrap().to_string();
        w.hosted_says(&b, HostedEvent::Taken { text: "hello".into() });
        ends(w, &b, "Hi.");
        w.hub().desk_set_member(&project, &b, true).unwrap();
        let d = desk(w, &project)["lead"]["sid"].as_str().unwrap().to_string();
        ends(w, &d, "ITEM: bug | Footer jumps | done when: it stays put\nROUTE: #new -> tidier | why: it knows tidy");
        w.advance(1000);
        assert!(item(w, &project, "Footer jumps")["holder"].is_object(), "item B is at work");
        std::fs::write(dir.join("src/trees.rs"), TREES_BOTH).unwrap();
        log(w, &b).tool_use("toolu_b1", "Bash", json!({ "command": script(dir, "src/trees.rs", "    2\n", "    tidy_half_done()\n") }));
        ends(w, &b, "Halfway through tidy.");
        w.advance(1000);
    }, "Added land_more.");
    let shown = item(&w, &project, "Header jumps");
    assert_eq!(shown["state"], "to_check", "{shown}");
    assert_eq!(git(&dir, &["show", "HEAD:src/trees.rs"]), TREES_A, "only item A's hunk of trees.rs is committed (#{id})");
    assert_eq!(git(&dir, &["show", "HEAD:src/main.rs"]), MAIN_A);
    head_builds(&dir).expect("main builds");
    assert_eq!(std::fs::read_to_string(dir.join("src/trees.rs")).unwrap(), TREES_BOTH, "the folder's file stays as it is");
    assert_eq!(git(&dir, &["status", "--porcelain", "--", "src"]).trim(), "M src/trees.rs", "item B's hunk stays uncommitted");
    assert_eq!(item(&w, &project, "Footer jumps")["state"], "working", "item B is still at work");
}
