//! Voice (stability plan §4, Voice / speak): which hook adds the ask for a short answer and how often, what
//! Settings → Voice says from cue-voice's status lines, its picks, and a stand-in cue-voice that prints
//! scripted lines. The real microphone and speaker are the manual checks (docs/dev/manual-checks.md).

use super::*;
use std::path::PathBuf;

static HELPER: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The cue-voice tests stand in, if any (talk::helper).
pub(super) fn helper() -> Option<PathBuf> {
    crate::locking::lock(&HELPER, "the voice helper's stand-in").clone().filter(|p| p.exists())
}

/// A Cue folder of the test's own with `voice.json` holding `prefs`, no status from cue-voice yet and no
/// stand-in; all put back when it goes.
struct Voice {
    home: PathBuf,
    _g: std::sync::MutexGuard<'static, ()>,
}

impl Voice {
    fn with(prefs: Value) -> Self {
        let g = crate::db::test_turn();
        let home = std::env::temp_dir().join(format!("cue-voice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("CUE_HOME", &home);
        std::fs::write(home.join("voice.json"), prefs.to_string()).unwrap();
        *crate::locking::lock(&STATUS, "talk status") = Value::Null;
        Voice { home, _g: g }
    }

    /// A stand-in cue-voice: a script that prints `lines` and exits.
    fn helper_printing(&self, lines: &[&str]) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = self.home.join("cue-voice");
        let body: String = lines.iter().map(|l| format!("printf '%s\\n' {}\n", crate::terminal::focus::shq(l))).collect();
        std::fs::write(&script, format!("#!/bin/sh\n[ \"$1\" = listen ] || exit 9\n{body}")).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        *crate::locking::lock(&HELPER, "the voice helper's stand-in") = Some(script.clone());
        script
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        *crate::locking::lock(&HELPER, "the voice helper's stand-in") = None;
        *crate::locking::lock(&STATUS, "talk status") = Value::Null;
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// Settings → Voice's row titled `title`: what it says under it.
fn row(title: &str) -> String {
    let s = settings().unwrap();
    s["rows"].as_array().unwrap().iter().find(|r| r["title"] == title).map(|r| r["sub"].as_str().unwrap_or("").to_string()).unwrap_or_else(|| panic!("no row {title}: {s}"))
}

#[test]
fn the_same_words_said_twice_are_each_taken_once_newest_first() {
    let d = std::env::temp_dir().join(format!("cue-voice-twice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    remember_in(&d, "yes go ahead", 1_000);
    remember_in(&d, "yes go ahead", 2_000);
    assert_eq!(hook_context_in(&d, "yes go ahead", 3_000), Some(ASK));
    assert!(d.join("2000.used").exists() && d.join("1000.txt").exists(), "the newest taken first");
    assert_eq!(hook_context_in(&d, "yes go ahead", 3_000), Some(ASK), "the older one next");
    assert_eq!(hook_context_in(&d, "yes go ahead", 3_000), None, "said twice, added twice");
    let _ = std::fs::remove_dir_all(&d);
}

/// Only Claude Code's hook on this Mac adds the ask itself; any other session gets it in the message.
#[test]
fn only_a_claude_session_here_has_its_hook_add_the_ask() {
    let w = crate::testing::World::new();
    w.session("c").prompt("hi");
    w.session("x").codex().prompt("hi");
    w.session("p").pi().prompt("hi");
    let hub = w.hub();
    assert_eq!([hooked(hub, "c"), hooked(hub, "x"), hooked(hub, "p"), hooked(hub, "gone")], [true, false, false, false]);
}

/// Off, or on before cue-voice has said how it is: Settings shows the switch alone.
#[test]
fn settings_show_only_the_switch_until_cue_voice_says_how_it_is() {
    let v = Voice::with(json!({"talk": true}));
    let rows = || settings().unwrap()["rows"].as_array().unwrap().len();
    assert_eq!(rows(), 1, "on, nothing said yet");
    assert!(!noted_status(&json!({"said": "hi", "sid": "s"})), "not a status line");
    assert_eq!(rows(), 1);
    assert!(noted_status(&json!({"status": {"model": "ready"}})));
    assert!(rows() > 1, "on, and it has said how it is");
    std::fs::write(v.home.join("voice.json"), json!({"talk": false}).to_string()).unwrap();
    assert_eq!(rows(), 1, "off");
}

/// What cue-voice says of NVIDIA's model and the keys, as Settings words it.
#[test]
fn settings_say_what_cue_voices_status_lines_say() {
    let _v = Voice::with(json!({"talk": true, "model": "parakeet"}));
    for (status, says) in [
        (json!({"model": "downloading", "progress": 0.42}), "42% of about 450 MB"),
        (json!({"model": "loading"}), "Loading NVIDIA's listening model"),
        (json!({"model": "ready"}), "Nothing you say leaves the Mac"),
        (json!({"model": "failed: no Neural Engine"}), "isn't working (no Neural Engine)"),
    ] {
        noted_status(&json!({ "status": status }));
        assert!(row("Listening").contains(says), "{status}: {}", row("Listening"));
    }
    noted_status(&json!({"status": {"model": "ready", "key": false}}));
    assert!(row("Hold right ⇧ to talk").contains("⌥Space can't be used"));
}

/// Settings → Voice's picks go to voice.json as cue-voice reads them; what isn't one is refused.
#[test]
fn voice_picks_are_saved_as_cue_voice_reads_them() {
    let v = Voice::with(json!("not an object"));
    set("read", "off").unwrap();
    set("auto", "front").unwrap();
    set("model", "parakeet").unwrap();
    assert_eq!(set("auto", "sometimes"), Err("no setting auto".into()));
    assert_eq!(set("volume", "11"), Err("no setting volume".into()));
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(v.home.join("voice.json")).unwrap()).unwrap();
    assert_eq!(saved, json!({"read": false, "auto": "front", "model": "parakeet"}), "a voice.json that wasn't an object started over");
}

/// Dictation through cue-voice: only once talking is on and cue-voice is there; then it runs `cue-voice
/// listen`, whose lines are dictation's, a status line among them noted for Settings, a line that isn't JSON
/// passed over.
#[test]
fn a_stand_in_cue_voice_is_run_for_dictation_and_its_lines_read() {
    let v = Voice::with(json!({"talk": false}));
    v.helper_printing(&[r#"{"status": {"model": "ready", "pick": "parakeet"}}"#, "not json", r#"{"text": "open a new", "final": false}"#, r#"{"text": "open a new tab", "final": true}"#]);
    assert!(dictation().is_none(), "talking off: Apple's own dictation instead");
    std::fs::write(v.home.join("voice.json"), json!({"talk": true}).to_string()).unwrap();
    let Some(mut cmd) = dictation() else {
        assert!(cfg!(not(target_arch = "aarch64")), "cue-voice is there and talking on");
        return;
    };
    let out = cmd.stdout(Stdio::piped()).output().unwrap();
    assert!(out.status.success(), "run as `cue-voice listen`");
    let lines: Vec<Value> = String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let dictated: Vec<&Value> = lines.iter().filter(|l| !noted_status(l)).collect();
    assert_eq!(dictated.last().map(|l| l["text"].clone()), Some(json!("open a new tab")));
    assert_eq!(dictated.len(), 2, "the line that isn't JSON passed over");
    assert!(row("Listening").contains("Nothing you say leaves the Mac"), "its status noted");
}

/// No cue-voice in this build: no dictation through it.
#[test]
fn without_cue_voice_there_is_no_dictation_through_it() {
    let v = Voice::with(json!({"talk": true}));
    let script = v.helper_printing(&[]);
    std::fs::remove_file(script).unwrap();
    assert!(dictation().is_none());
}
