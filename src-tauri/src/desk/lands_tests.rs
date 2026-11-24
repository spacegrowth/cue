//! The tests of how work lands (desk/lands.rs): its parts read, defaulted, checked and saved.

use super::*;

#[test]
fn an_older_builds_working_at_once_is_read_as_items_at_once() {
    let c = json!({ "project": { "lands": { "p": { "working": "2" } } } });
    assert_eq!(of_in(&c, "p", Place::Repo).at_once, 2);
    let c = json!({ "project": { "lands": { "p": { "working": "off", "at_once": 4 } } } });
    assert_eq!(of_in(&c, "p", Place::Repo).at_once, 4, "a saved number wins; no limit isn't a number");
    assert!(!of_in(&c, "p", Place::Repo).to_json().as_object().unwrap().contains_key("working"), "never saved again");
}

#[test]
fn a_new_project_gets_the_defaults_and_a_folder_with_no_repository_works_in_place() {
    let none = json!({});
    assert_eq!(of_in(&none, "p", Place::Repo), Lands { at: "copy", how: "squash", check: "none", after: "nothing", push: "ask", tests: "all", waits: "landed", at_once: 3, check_steps: vec![], after_steps: vec![] });
    assert_eq!(of_in(&none, "cue", Place::Cue), Lands { at: "copy", how: "squash", check: "cue", after: "command", push: "ask", tests: "changed", waits: "landed", at_once: 3, check_steps: vec![], after_steps: app_steps() });
    let c = json!({ "project": { "lands": { "p": { "where": "copy", "how": "squash" } } } });
    assert_eq!(of_in(&c, "p", Place::NoRepo), Lands { at: "shared", how: "owner", check: "none", after: "nothing", push: "ask", tests: "all", waits: "landed", at_once: 3, check_steps: vec![], after_steps: vec![] }, "a copy needs a repository");
}

#[test]
fn what_cant_be_picked_says_why_in_plain_words() {
    assert_eq!(allowed("where", "branch", Place::Repo), Ok("branch"));
    assert_eq!(allowed("where", "branch", Place::NoRepo), Err("needs a git repository"));
    assert_eq!(allowed("how", "merge", Place::Cue), Ok("merge"));
    assert_eq!(allowed("how", "merge", Place::NoRepo), Err("needs a git repository"));
    assert_eq!(allowed("how", "pr", Place::Repo), Ok("pr"));
    assert_eq!(allowed("how", "pr", Place::NoRepo), Err("needs a git repository"));
    assert_eq!(allowed("push", "after", Place::Cue), Ok("after"));
    assert_eq!(allowed("check", "command", Place::Cue), Ok("command"));
    assert_eq!(allowed("after", "command", Place::Repo), Ok("command"));
    assert_eq!(allowed("check", "cue", Place::Repo), Err("only for Cue's own project"));
    assert_eq!(allowed("where", "copy", Place::NoRepo), Err("needs a git repository"));
    assert_eq!(allowed("how", "owner", Place::Repo), Ok("owner"));
}

#[test]
fn a_saved_setting_holds_and_you_commit_means_the_shared_folder() {
    let c = json!({ "project": { "lands": { "p": { "where": "branch", "how": "owner", "check": "command", "at_once": 5, "push": "after" } } } });
    let l = of_in(&c, "p", Place::Repo);
    assert_eq!((l.at, l.how, l.check, l.at_once, l.push), ("shared", "owner", "none", 5, "after"), "you commit: the shared folder; your own steps with none are never in effect");
    let c = json!({ "project": { "lands": { "p": { "where": "branch", "how": "squash" } } } });
    assert_eq!((of_in(&c, "p", Place::Repo).at, of_in(&c, "p", Place::NoRepo).at), ("branch", "shared"), "a branch, in a repository only");
    let c = json!({ "project": { "lands": { "p": { "where": "branch", "how": "pr" } } } });
    assert_eq!(of_in(&c, "p", Place::Repo).how, "squash", "a pull request needs a copy");
    let c = json!({ "project": { "lands": { "p": { "where": "branch", "how": "merge" } } } });
    assert_eq!(of_in(&c, "p", Place::Repo).how, "merge", "merge works on a branch");
    let c = json!({ "project": { "lands": { "p": { "where": "shared", "how": "merge" } } } });
    assert_eq!(of_in(&c, "p", Place::Repo).how, "squash", "merge needs a branch of the item's own");
    let c = json!({ "project": { "lands": { "p": { "where": "shared", "how": "pr" } } } });
    assert_eq!(of_in(&c, "p", Place::Repo).how, "squash", "a pull request needs a copy");
    assert!(checked(&json!({ "p": { "where": "nowhere" } })).is_err());
    assert!(checked(&json!({ "p": { "at_once": 0 } })).is_err());
    assert!(checked(&json!({ "p": { "working": "2" } })).is_ok(), "an older build's Working at once is kept (#798)");
    assert!(checked(&json!({ "p": { "where": "shared", "at_once": 2 } })).is_ok());
}

#[test]
fn cues_own_project_keeps_its_check_and_install_and_a_new_project_gets_neither() {
    let cue = of_in(&json!({}), "cue", Place::Cue);
    assert_eq!((cue.check, cue.after, cue.push), ("cue", "command", "ask"), "Cue's own setup, nothing saved");
    assert_eq!(cue.after_steps, ["[Build] app:build", "[Install] app:install", "[Restart] app:restart"]);
    assert!(cue.updates_app() && cue.plain_after().is_empty());
    let new = of_in(&json!({}), "web", Place::Repo);
    assert_eq!((new.check, new.after, new.push), ("none", "nothing", "ask"), "no check, nothing after landing");
    assert!(new.check_steps.is_empty() && new.after_steps.is_empty());
    assert_eq!(allowed("after", "install", Place::Repo), Err("not a choice"));
    assert_eq!(allowed("after", "install", Place::Cue), Err("not a choice"), "no choice of its own: steps (#796)");
}

#[test]
fn your_own_steps_are_kept_in_cue_and_need_at_least_one() {
    let c = json!({ "project": { "lands": { "web": { "check": "command", "check_steps": ["npm test", " ", "npm run lint"], "after": "command" } } } });
    let l = of_in(&c, "web", Place::Repo);
    assert_eq!((l.check, l.check_steps.clone()), ("command", vec!["npm test".to_string(), "npm run lint".to_string()]));
    assert_eq!(l.after, "nothing", "after landing of your own with no steps is as if not picked");
    assert_eq!(steps_in(&json!("a\n\n b ")), Some(vec!["a".to_string(), "b".to_string()]));
    assert!(checked(&json!({ "web": { "check_steps": ["a"], "after_steps": "b" } })).is_ok());
    assert!(checked(&json!({ "web": { "check_steps": 3 } })).is_err());
}

#[test]
fn steps_run_in_order_and_stop_at_the_first_that_fails() {
    let dir = std::env::temp_dir().join(format!("cue-lands-steps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    let steps: Vec<String> = ["echo one >> ran", "echo two >> ran; echo it broke; exit 3", "echo three >> ran"].map(String::from).to_vec();
    let e = run_steps(&d, &steps).unwrap_err();
    assert_eq!((e.step.as_str(), e.why.as_str()), (steps[1].as_str(), "failed"));
    assert!(e.said.contains("it broke"), "{}", e.said);
    assert_eq!(std::fs::read_to_string(dir.join("ran")).unwrap(), "one\ntwo\n", "the third never ran");
    assert!(run_steps(&d, &steps[..1]).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_step_that_hangs_is_stopped_with_all_it_started() {
    let dir = std::env::temp_dir().join(format!("cue-lands-hang-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    TEST_STEP_CAP.with(|c| c.set(Some(std::time::Duration::from_secs(1))));
    let t = std::time::Instant::now();
    // A child of the step's shell that holds its output: stopped with it, and the next step never runs.
    let steps: Vec<String> = ["(sleep 2; echo late >> ran) & sleep 30", "echo next >> ran"].map(String::from).to_vec();
    let e = run_steps(&d, &steps).unwrap_err();
    TEST_STEP_CAP.with(|c| c.set(None));
    assert!(t.elapsed() < std::time::Duration::from_secs(5), "{:?}", t.elapsed());
    assert_eq!((e.step.as_str(), e.why.as_str()), (steps[0].as_str(), "took over 1s and was stopped"));
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert!(!dir.join("ran").exists(), "nothing it started is left running, and the next step didn't run");
    assert_eq!(cap_words(STEP_CAP), "15 minutes");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn all_the_steps_together_are_stopped_past_the_total_time() {
    let dir = std::env::temp_dir().join(format!("cue-lands-total-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    TEST_TOTAL_CAP.with(|c| c.set(Some(std::time::Duration::from_millis(1500))));
    // Each step is well under its own cap; a step left running in the background counts too.
    let steps: Vec<String> = ["sleep 1", "(sleep 5) & echo started", "echo never >> ran"].map(String::from).to_vec();
    let t = std::time::Instant::now();
    let e = run_steps(&d, &steps).unwrap_err();
    TEST_TOTAL_CAP.with(|c| c.set(None));
    assert!(t.elapsed() < std::time::Duration::from_secs(4), "{:?}", t.elapsed());
    assert_eq!((e.step.as_str(), e.why.as_str()), (steps[1].as_str(), "was stopped: your steps took over 1500ms in all"));
    assert!(!dir.join("ran").exists());
    assert_eq!(cap_words(TOTAL_CAP), "30 minutes");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn steps_run_with_your_login_shell() {
    let dir = std::env::temp_dir().join(format!("cue-lands-shell-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    // A pipe and a `;` in one step, as a shell reads them.
    assert!(run_steps(&d, &["echo one; echo two | tr a-z A-Z > said".to_string()]).is_ok());
    assert_eq!(std::fs::read_to_string(dir.join("said")).unwrap(), "TWO\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn steps_that_push_are_refused_everywhere_they_could_go_in() {
    for s in ["git push", "npm test && git push origin main", "git -C web push", "make; /usr/bin/git  push --force"] {
        assert_eq!(refused_steps(&[s.to_string()]).unwrap_err(), "Steps can't push; a push always asks you first", "{s}");
    }
    for s in ["git pull", "npm test", "echo push", "git status; echo git"] {
        assert!(refused_steps(&[s.to_string()]).is_ok(), "{s}");
    }
    let c = json!({});
    assert_eq!(propose(&c, "web", Place::Repo, "after", "command", &["make".into(), "git push".into()]).unwrap_err(), "Steps can't push; a push always asks you first");
    assert!(checked(&json!({ "web": { "after": "command", "after_steps": ["git push"] } })).is_err(), "nor through config");
}

#[test]
fn one_way_decides_the_check() {
    let c = json!({ "project": { "lands": { "web": { "check": "command", "check_steps": ["npm test"] }, "off": { "check": "none" } } } });
    assert_eq!(check_in(&c, "web", Place::Repo), Check::Steps(vec!["npm test".into()]));
    assert_eq!(check_in(&c, "off", Place::Cue), Check::Off, "None picked: off, Cue's own project too");
    assert_eq!(check_in(&c, "new", Place::Cue), Check::Repo, "not picked: the repository's own");
    assert_eq!(check_in(&json!({ "project": { "lands": { "p": { "check": "command" } } } }), "p", Place::Repo), Check::Repo, "no steps: as if not picked");
}

#[test]
fn the_leads_proposal_says_the_change_and_never_turns_off_the_push_ask() {
    let c = json!({ "project": { "lands": { "cue": { "at_once": 4 } } } });
    let (set, words) = propose(&c, "cue", Place::Cue, "check", "none", &[]).unwrap();
    assert_eq!(words, "check → none");
    assert_eq!(set, json!({ "at_once": 4, "check": "none" }), "what you saved stays, with the change");
    let (set, words) = propose(&c, "web", Place::Repo, "after", "command", &["make deploy".into()]).unwrap();
    assert_eq!(words, "after landing → make deploy");
    assert_eq!(set["after_steps"], json!(["make deploy"]));
    assert!(propose(&c, "cue", Place::Cue, "push", "after", &[]).unwrap_err().contains("keeps asking"));
    assert!(propose(&c, "cue", Place::Cue, "check", "command", &[]).is_err(), "your own steps need steps");
    assert!(propose(&c, "web", Place::Repo, "after", "install", &[]).is_err(), "not a choice");
    assert!(propose(&c, "web", Place::Repo, "after", "command", &app_steps()).is_err(), "the app's own steps, on its own source only");
    let (set, words) = propose(&c, "cue", Place::Cue, "after", "command", &["[Docs] make docs".into(), "[Build] app:build".into(), "[Install] app:install".into(), "[Restart] app:restart".into()]).unwrap();
    assert_eq!(words, "after landing → Docs, then Build, then Install, then Restart");
    assert_eq!(set["after_steps"][0], "[Docs] make docs");
    assert_eq!(propose(&c, "cue", Place::Cue, "at once", "2", &[]).unwrap().1, "items at once → 2");
}

#[test]
fn a_step_is_named_by_its_brackets_or_by_its_command() {
    assert_eq!((step_name("[Build] npm run build"), step_run("[Build] npm run build")), ("Build", "npm run build"));
    assert_eq!((step_name("[Put in place]  ./go.sh"), step_run("[Put in place]  ./go.sh")), ("Put in place", "./go.sh"));
    assert_eq!((step_name("npm test"), step_run("npm test")), ("npm test", "npm test"));
    assert_eq!((step_name("[ -f x ] && make"), step_run("[ -f x ] && make")), ("[ -f x ] && make", "[ -f x ] && make"), "a shell test is a command");
    assert_eq!(step_name("[Empty]"), "[Empty]", "a name with nothing to run is a command");
}

#[test]
fn a_desk_saved_with_the_old_install_choice_reads_as_the_apps_steps() {
    let c = json!({ "project": { "lands": { "cue": { "after": "install", "check": "cue" }, "web": { "after": "install" } } } });
    let l = of_in(&c, "cue", Place::Cue);
    assert_eq!((l.after, l.after_steps.clone()), ("command", app_steps()));
    assert_eq!((l.app_step_name("building"), l.app_step_name("starting")), (Some("Build".into()), Some("Restart".into())));
    assert_eq!(of_in(&c, "web", Place::Repo).after, "nothing", "elsewhere it was never allowed");
    assert!(checked(&json!({ "cue": { "after": "install" } })).is_ok(), "an old setting still saves");
}

#[test]
fn the_apps_steps_go_together_last_and_only_on_its_own_source() {
    let named = vec!["[Docs] make docs".to_string(), "[Compile] app:build".into(), "[Swap] app:install".into(), "[Relaunch] app:restart".into()];
    let c = json!({ "project": { "lands": { "cue": { "after": "command", "after_steps": named } } } });
    let l = of_in(&c, "cue", Place::Cue);
    assert!(l.updates_app());
    assert_eq!(l.plain_after(), ["[Docs] make docs"], "your own steps run as it lands; the app's go with the next update");
    assert_eq!(l.app_step_name("installing"), Some("Swap".into()));
    assert_eq!(l.to_json()["after_names"], json!({ "building": "Compile", "installing": "Swap", "starting": "Relaunch" }));
    // Elsewhere they're left out.
    let web = of_in(&json!({ "project": { "lands": { "web": c["project"]["lands"]["cue"].clone() } } }), "web", Place::Repo);
    assert!(!web.updates_app());
    assert_eq!(web.after_steps, ["[Docs] make docs"]);
    // Not all three, out of order, or not last: refused.
    for bad in [vec!["app:build", "app:install"], vec!["app:install", "app:build", "app:restart"], vec!["app:build", "app:install", "app:restart", "make docs"]] {
        let bad: Vec<String> = bad.into_iter().map(String::from).collect();
        assert!(refused_app_steps(&bad, Place::Cue).is_err(), "{bad:?}");
    }
    assert!(refused_app_steps(&app_steps(), Place::Repo).is_err());
    assert!(refused_app_steps(&app_steps(), Place::Cue).is_ok());
}

#[test]
fn a_named_step_runs_its_command_and_fails_by_its_name() {
    let dir = std::env::temp_dir().join(format!("cue-lands-named-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    let e = run_steps(&d, &["[Make it] echo made > out".to_string(), "[Break] exit 2".to_string()]).unwrap_err();
    assert_eq!(e.step, "Break");
    assert_eq!(std::fs::read_to_string(dir.join("out")).unwrap(), "made\n");
    let _ = std::fs::remove_dir_all(&dir);
}
