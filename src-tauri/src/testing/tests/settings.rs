//! Settings (stability plan §4, Settings): every key the window saves goes to config.json and comes back as
//! saved, a wrong value is refused without touching the file, and a config.json that's broken, read-only,
//! or changed under Cue never crashes it or loses your settings.

use crate::testing::World;
use serde_json::{json, Value};

/// Each setting the window may save: a value it sends, and what's saved for it.
fn round_trips() -> Vec<(&'static str, Value, Value)> {
    let same = |k: &'static str, v: Value| (k, v.clone(), v);
    vec![
        same("history.keep", json!(120)),
        ("history.keep", json!(3), json!(10)),
        same("notify.finished", json!(false)),
        same("notify.decisions", json!(false)),
        same("notify.cold", json!(false)),
        same("notify.sound", json!("glass")),
        same("pi.gate", json!("all")),
        same("appearance.mode", json!("dark")),
        same("mini.mode", json!("rail")),
        same("appearance.zoom", json!(1.15)),
        same("appearance.project", json!(true)),
        ("context.keep", json!(99), json!(50)),
        same("turn.mode", json!("last")),
        same("agents.show_driven", json!(true)),
        ("quick.phrases", json!(["  ship   it ", "", "ok"]), json!(["ship it", "ok"])),
        same("steps.mode", json!("all")),
        same("setup.done", json!(true)),
        ("usage.file", json!("  "), json!(crate::config::USAGE_FILE_DEFAULT)),
        same("usage.claude", json!(false)),
        same("sessions.tmux", json!(false)),
        same("project.colors", json!({"web": 3})),
        same("project.all", json!(4)),
        same("project.hue", json!({"h": 200, "k": 0.5})),
        ("project.pins", json!(["a", "a", "", "b"]), json!(["a", "b"])),
        ("project.names", json!({"web": "  The   web ", "api": "api"}), json!({"web": "The web"})),
        ("project.moves", json!({"s1": " web ", "s2": ""}), json!({"s1": "web"})),
        ("project.folders", json!({"/w/app/": "app", "box:/srv": "srv"}), json!({"/w/app": "app", "box:/srv": "srv"})),
        ("project.made", json!([" new ", "new"]), json!(["new"])),
        same("insights.on", json!(true)),
        same("insights.queue", json!(false)),
        same("insights.snooze", json!(1_800_000_000_000u64)),
        same("insights.skills", json!({"|claude": "handoff"})),
        same("tips.on", json!(false)),
        same("actions.prompt", json!("Set my actions up.")),
        same("voice.pick", json!("af_heart")),
        same("tasks.on", json!(true)),
        same("tasks.auto", json!(true)),
        same("status.on", json!(false)),
        same("status.sessions", json!(["s1"])),
        same("background.sessions", json!(["s2"])),
    ]
}

/// The settings that do more than save, saved and read elsewhere (login.open: macOS's switch).
const NOT_HERE: [&str; 1] = ["login.open"];

fn pointer(key: &str) -> String {
    format!("/{}", key.replace('.', "/"))
}

fn on_disk(w: &World) -> Value {
    serde_json::from_str(&std::fs::read_to_string(w.home().join("config.json")).unwrap_or_else(|_| "{}".into())).unwrap()
}

/// config.json holding `text`, read into memory as the tick would.
fn config_file(w: &World, text: &str) {
    std::fs::write(w.home().join("config.json"), text).unwrap();
    crate::config::refresh();
}

#[test]
fn every_setting_the_window_saves_comes_back_as_saved() {
    let w = World::new();
    let cases = round_trips();
    let listed: Vec<&str> = cases.iter().map(|(k, ..)| *k).chain(NOT_HERE).collect();
    let unlisted: Vec<&&str> = crate::config::ALLOWED.iter().filter(|k| !listed.contains(k)).collect();
    assert!(unlisted.is_empty(), "settings the window may save with no round trip here: {unlisted:?}");
    for (key, sent, saved) in cases {
        assert_eq!(crate::config::set(key, sent.clone()), Ok(()), "{key} = {sent}");
        let (memory, disk) = if key == "actions.prompt" {
            (json!(crate::actions::prompt()), json!(crate::actions::prompt()))
        } else {
            (crate::config::load().pointer(&pointer(key)).cloned().unwrap_or(Value::Null), on_disk(&w).pointer(&pointer(key)).cloned().unwrap_or(Value::Null))
        };
        assert_eq!(memory, saved, "{key}: in memory");
        assert_eq!(disk, saved, "{key}: in config.json");
        assert!(!crate::config::refresh(), "{key}: what's in memory is what the file says");
    }
}

/// A value of the wrong kind for its key is refused, and config.json isn't touched.
#[test]
fn a_wrong_value_is_refused_and_nothing_is_written() {
    let w = World::new();
    config_file(&w, r#"{"mine": {"kept": true}}"#);
    let before = std::fs::read_to_string(w.home().join("config.json")).unwrap();
    for (key, bad) in [
        ("history.keep", json!("lots")), ("pi.gate", json!("sometimes")), ("appearance.mode", json!(1)), ("mini.mode", json!("left")),
        ("appearance.zoom", json!(2.0)), ("notify.sound", json!("klaxon")), ("context.keep", json!(-1)),
        ("quick.phrases", json!(["a", "b", "c", "d", "e"])), ("project.colors", json!({"web": 12})),
        ("project.hue", json!({"h": 400, "k": 0.5})), ("project.folders", json!({"relative/dir": "x"})),
        ("insights.skills", json!({"no-bar": "handoff"})), ("status.sessions", json!("s1")),
        ("voice.pick", json!("Not A Voice")), ("tips.on", json!("yes")), ("made.up", json!(true)),
    ] {
        assert!(crate::config::set(key, bad.clone()).is_err(), "{key} = {bad} was taken");
    }
    assert_eq!(std::fs::read_to_string(w.home().join("config.json")).unwrap(), before);
}

/// The kept-aside copies of an unreadable config.json beside it.
fn kept_aside(w: &World) -> Vec<String> {
    std::fs::read_dir(w.home()).unwrap().filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("config.json.unreadable-")).collect()
}

/// A config.json cut short (or hand-edited into bad JSON): every setting reads its default, and the first
/// save keeps the broken file aside rather than writing over what was in it.
#[test]
fn a_broken_config_reads_as_defaults_and_is_kept_aside_by_the_next_save() {
    for broken in [r#"{"history": {"keep": 120}, "notify": {"fin"#, "[1, 2, 3]", ""] {
        let w = World::new();
        config_file(&w, broken);
        let all = crate::config::effective();
        assert_eq!(all["history"]["keep"], crate::config::HISTORY_DEFAULT, "{broken:?}");
        assert_eq!(all["notify"]["finished"], true);
        crate::config::set("tips.on", json!(false)).unwrap();
        assert_eq!(on_disk(&w), json!({"tips": {"on": false}}), "{broken:?}");
        let aside = kept_aside(&w);
        assert_eq!(aside.len(), 1, "{broken:?}: {aside:?}");
        assert_eq!(std::fs::read_to_string(w.home().join(&aside[0])).unwrap(), broken, "kept as it was");
        drop(w);
    }
}

/// Values of the wrong kind written by hand: each reads as its default, the rest of the file as written.
#[test]
fn hand_written_values_of_the_wrong_kind_read_as_their_defaults() {
    let w = World::new();
    config_file(&w, r#"{"history": {"keep": "lots"}, "notify": "yes", "appearance": {"zoom": "big", "mode": 7},
        "quick": {"phrases": "hi"}, "project": {"pins": {}, "colors": [1]}, "context": {"keep": -3}, "tips": {"on": false}}"#);
    let all = crate::config::effective();
    assert_eq!(all["history"]["keep"], crate::config::HISTORY_DEFAULT);
    assert_eq!(all["notify"]["finished"], true);
    assert_eq!((all["appearance"]["zoom"].clone(), all["appearance"]["mode"].clone()), (json!(1.0), json!("system")));
    assert_eq!(all["quick"]["phrases"], json!(["Commit", "Go ahead", "Think hard"]));
    assert_eq!((all["project"]["pins"].clone(), all["project"]["colors"].clone()), (json!([]), json!({})));
    assert_eq!(all["context"]["keep"], crate::config::context_keep());
    assert_eq!(all["tips"]["on"], false, "what was written right is read");
    assert!(kept_aside(&w).is_empty(), "the file is settings: nothing to keep aside");
}

/// A data folder Cue can't write into: a save says why and changes nothing, in memory or on disk.
#[test]
fn a_save_into_a_folder_cue_cant_write_says_so_and_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let w = World::new();
    config_file(&w, r#"{"tips": {"on": true}}"#);
    std::fs::set_permissions(w.home(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let r = crate::config::set("tips.on", json!(false));
    let leftovers: Vec<_> = std::fs::read_dir(w.home()).unwrap().filter_map(Result::ok).map(|e| e.file_name()).filter(|n| n.to_string_lossy().ends_with(".next")).collect();
    std::fs::set_permissions(w.home(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(r.is_err(), "{r:?}");
    assert_eq!(crate::config::load()["tips"]["on"], true, "memory as it was");
    assert_eq!(on_disk(&w)["tips"]["on"], true, "the file as it was");
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// A config.json made read-only (by you, or a dotfiles tool): saved all the same, and still read-only.
#[test]
fn a_read_only_config_is_saved_and_stays_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let w = World::new();
    let file = w.home().join("config.json");
    config_file(&w, "{}");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(crate::config::set("tips.on", json!(false)), Ok(()));
    let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(on_disk(&w)["tips"]["on"], false);
    assert_eq!(mode, 0o444);
}

/// An editor that saves by writing a new file and renaming it over the old one, same size and time: a new
/// file all the same (its inode), read by the next refresh.
#[test]
fn an_editors_save_by_rename_is_picked_up() {
    let w = World::new();
    let file = w.home().join("config.json");
    config_file(&w, r#"{"turn": {"mode": "answer"}}"#);
    let when = std::fs::metadata(&file).unwrap().modified().unwrap();
    let swap = w.home().join("config.json.swp");
    std::fs::write(&swap, r#"{"turn": {"mode": "last"}}  "#).unwrap();
    assert_eq!(std::fs::metadata(&swap).unwrap().len(), std::fs::metadata(&file).unwrap().len(), "the same size");
    std::fs::File::options().write(true).open(&swap).unwrap().set_modified(when).unwrap();
    std::fs::rename(&swap, &file).unwrap();
    assert!(crate::config::refresh(), "a new file");
    assert_eq!(crate::config::load()["turn"]["mode"], "last");
}

/// config.json deleted by hand: every setting back to its default at the next refresh, and the next save
/// makes the file again with just that setting.
#[test]
fn a_deleted_config_reads_as_defaults_and_a_save_makes_it_again() {
    let w = World::new();
    config_file(&w, r#"{"tips": {"on": false}, "history": {"keep": 77}}"#);
    std::fs::remove_file(w.home().join("config.json")).unwrap();
    assert!(crate::config::refresh());
    assert_eq!(crate::config::history_keep(), crate::config::HISTORY_DEFAULT as usize);
    crate::config::set("notify.cold", json!(false)).unwrap();
    assert_eq!(on_disk(&w), json!({"notify": {"cold": false}}));
}

/// "Hide for a day" holds off the hand-off asked ahead until the day is over, by Cue's clock: the World's,
/// run a day on, brings it back without anything else changing.
#[test]
fn insights_hidden_for_a_day_come_back_when_the_day_is_over() {
    let w = World::at(1_800_000_000_000);
    let ahead = || crate::config::effective()["insights"]["ahead"].clone();
    crate::config::set("insights.on", json!(true)).unwrap();
    crate::config::set("insights.snooze", json!(1_800_000_000_000u64 + 86_400_000)).unwrap();
    assert_eq!(ahead(), false, "hidden for the day");
    w.advance(86_400_000 - 1);
    assert_eq!(ahead(), false, "a moment before it's over");
    w.advance(1);
    assert_eq!(ahead(), true, "back once it is");
}
