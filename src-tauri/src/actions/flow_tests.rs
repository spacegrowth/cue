//! An action from allowed to run to stopped, and the ways it can't be (stability plan §4, Extensions /
//! actions): a project's file that isn't JSON, an action gone from it, one renamed, one never run.

use super::tests::Scratch;
use super::*;

fn allow_all(s: &Scratch) {
    let all = gather(&s.project, "").unwrap().actions;
    edit_record(&s.project, "", |rec| all.iter().for_each(|a| allow(rec, a)));
}

/// A file that isn't JSON says why where the actions would be, and nothing in it runs.
#[test]
fn a_project_file_that_isnt_json_says_why_and_runs_nothing() {
    let s = Scratch::new("broken");
    s.write(r#"{"actions": [{"name": "Install", "run": "npm ci"}"#);
    let v = listing(&s.project, "").unwrap();
    assert!(v["actions"].as_array().unwrap().is_empty());
    assert!(v["error"].as_str().is_some_and(|e| e.contains(&mine_path(&s.project, "").display().to_string())), "names the file: {v}");
    assert_eq!(run_one(&s.project, "", "local:Install").unwrap_err(), "That action isn't there any more");
}

/// An allowed action taken out of the file can't be run or stopped by its old id; renamed, it asks again.
#[test]
fn an_action_gone_or_renamed_can_no_longer_run_unasked() {
    let s = Scratch::new("gone");
    s.write(r#"{"actions": [{"name": "Test", "run": "npm test"}]}"#);
    allow_all(&s);
    s.write(r#"{"actions": [{"name": "Check", "run": "npm test"}]}"#);
    assert_eq!(run_one(&s.project, "", "local:Test").unwrap_err(), "That action isn't there any more");
    assert_eq!(stop_one(&s.project, "", "local:Test").unwrap_err(), "That action isn't there any more");
    assert_eq!(run_one(&s.project, "", "local:Check").unwrap_err(), "Allow it first", "a new name is a new action");
}

/// Stopping one that never ran (or has ended) is nothing: it says it isn't running.
#[test]
fn stopping_an_action_that_isnt_running_is_nothing() {
    let s = Scratch::new("idle");
    s.write(r#"{"actions": [{"name": "Serve", "run": "npm start", "keep": true}]}"#);
    allow_all(&s);
    let st = stop_one(&s.project, "", "local:Serve").unwrap();
    assert_eq!(st["running"], false, "{st}");
}
