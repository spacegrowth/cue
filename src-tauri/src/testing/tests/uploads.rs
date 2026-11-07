//! Images you attach (stability plan §4, uploads): saved here, copied to a session's machine with the copy's
//! progress, typed as paths. Big images, files gone or unreadable, a machine that drops mid-copy or says
//! nothing back, and two saves at the same moment.

use crate::model::Upload;
use crate::testing::{FakeSsh, World, RUNS_IT};
use crate::uploads::{self, Saved};
use base64::Engine;
use serde_json::json;
use std::cell::RefCell;

/// `bytes` as the window hands an image over: a data: URL.
fn upload(name: &str, bytes: &[u8]) -> Upload {
    Upload { name: name.into(), mime: "image/png".into(), data: format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)) }
}

/// `n` bytes that aren't all the same.
fn bytes(n: usize) -> Vec<u8> {
    (0..=255u8).cycle().take(n).collect()
}

/// A World with machine `host` that runs Cue's scripts for real.
fn machine(host: &str) -> (World, FakeSsh) {
    let w = World::new();
    let ssh = FakeSsh::new();
    ssh.host(host, RUNS_IT);
    crate::machines::add(host, "").unwrap();
    (w, ssh)
}

/// Copy `saved` to `machine`, with every progress report heard.
fn copy(machine: &str, saved: &[Saved]) -> (Result<Vec<Saved>, String>, Vec<(u64, u64)>) {
    let heard = RefCell::new(vec![]);
    let r = uploads::on_machine(machine, saved, &|sent, total| heard.borrow_mut().push((sent, total)));
    (r, heard.into_inner())
}

#[test]
fn images_reach_the_machine_whole_and_the_copy_says_how_far_along_it_is() {
    let (_w, ssh) = machine("img-box");
    let saved = uploads::save(&[upload("a.png", &bytes(200_000)), upload("b.png", &bytes(1_000))]).unwrap();
    let (there, heard) = copy("img-box", &saved);
    let there = there.unwrap();
    assert_eq!(heard.first(), Some(&(0, 201_000)), "told it's starting, and how much there is in all");
    assert_eq!(heard.last(), Some(&(201_000, 201_000)));
    assert!(heard.windows(2).all(|w| w[0].0 <= w[1].0), "never going back: {heard:?}");
    let home = ssh.home("img-box");
    for (here, at) in saved.iter().zip(&there) {
        assert!(at.path.starts_with(&home.join(".cue/uploads").to_string_lossy().into_owned()), "{}", at.path);
        assert_eq!(std::fs::read(&at.path).unwrap(), std::fs::read(&here.path).unwrap(), "the same bytes there");
        assert_eq!(at.mime, "image/png");
    }
    let parts = std::fs::read_dir(home.join(".cue/uploads")).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "part")).count();
    assert_eq!(parts, 0, "nothing left half-written");
}

#[test]
fn a_50_mb_image_is_copied_in_steps_and_arrives_whole() {
    let (_w, ssh) = machine("big-img");
    let big = bytes(50 << 20);
    let saved = uploads::save(&[upload("big.png", &big)]).unwrap();
    let (there, heard) = copy("big-img", &saved);
    let there = there.unwrap();
    assert!(heard.len() > 100, "heard as it goes, not once at the end: {}", heard.len());
    assert_eq!(heard.last(), Some(&(50 << 20, 50 << 20)));
    assert!(there[0].path.starts_with(&ssh.home("big-img").to_string_lossy().into_owned()));
    assert!(std::fs::read(&there[0].path).unwrap() == big, "50 MB, byte for byte");
}

#[test]
fn an_image_gone_or_unreadable_before_the_copy_stops_it_before_anything_is_sent() {
    let (_w, ssh) = machine("lost-img");
    let saved = uploads::save(&[upload("a.png", &bytes(10))]).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&saved[0].path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (r, heard) = copy("lost-img", &saved);
    assert!(r.unwrap_err().to_lowercase().contains("permission denied"));
    std::fs::remove_file(&saved[0].path).unwrap();
    let (r, _) = copy("lost-img", &saved);
    assert!(r.is_err(), "a file that's gone");
    assert!(heard.is_empty(), "no progress for a copy that never started");
    assert_eq!(ssh.calls().len(), 0, "the machine never asked");
}

#[test]
fn a_machine_that_drops_mid_copy_says_so_and_types_no_path() {
    let (_w, ssh) = machine("drop-img");
    ssh.host("drop-img", "head -c 70000 >/dev/null; echo 'Connection to drop-img closed by remote host.' >&2; exit 255");
    let saved = uploads::save(&[upload("a.png", &bytes(3 << 20))]).unwrap();
    let (r, heard) = copy("drop-img", &saved);
    let err = r.unwrap_err();
    assert!(err.contains("drop-img"), "names the machine: {err}");
    assert!(heard.last().unwrap().0 < 3 << 20, "the copy never said it was all there: {:?}", heard.last());
}

#[test]
fn a_machine_that_takes_the_image_and_says_nothing_back_isnt_taken_for_done() {
    let (_w, ssh) = machine("mute-img");
    ssh.host("mute-img", "cat >/dev/null; exit 0");
    let saved = uploads::save(&[upload("a.png", &bytes(10))]).unwrap();
    let err = copy("mute-img", &saved).0.unwrap_err();
    assert_eq!(err, "couldn't copy the image to mute-img");
}

#[test]
fn an_image_for_a_machine_since_removed_says_so() {
    let (_w, _ssh) = machine("old-img");
    crate::machines::remove("old-img").unwrap();
    let saved = uploads::save(&[upload("a.png", &bytes(10))]).unwrap();
    assert_eq!(copy("old-img", &saved).0.unwrap_err(), "old-img isn't one of your machines any more");
    assert_eq!(copy("", &saved).0.unwrap()[0].path, saved[0].path, "on this Mac: where it already is");
}

#[test]
fn an_image_that_isnt_one_or_doesnt_decode_is_refused_by_name() {
    let _w = World::new();
    let err = uploads::save(&[Upload { name: "notes.txt".into(), mime: "text/plain".into(), data: "aGk=".into() }]).unwrap_err();
    assert_eq!(err, "notes.txt isn't an image");
    let err = uploads::save(&[Upload { name: "shot.png".into(), mime: "image/png".into(), data: "data:image/png;base64,@@not base64@@".into() }]).unwrap_err();
    assert!(err.starts_with("shot.png: "), "{err}");
}

#[test]
fn two_saves_at_the_same_moment_keep_each_image() {
    let _w = World::at(1_800_000_000_000);
    let first = uploads::save(&[upload("a.png", b"first")]).unwrap();
    let second = uploads::save(&[upload("a.png", b"second")]).unwrap();
    assert_ne!(first[0].path, second[0].path);
    assert_eq!(std::fs::read(&first[0].path).unwrap(), b"first", "the first message's image is still its own");
    assert_eq!(std::fs::read(&second[0].path).unwrap(), b"second");
}

#[test]
fn a_message_with_an_image_to_a_machines_session_types_the_path_there_escaped() {
    let (w, ssh) = machine("typed-img");
    // A home there with a space in it: the path is typed as one word.
    ssh.host("typed-img", &format!("{}\n", RUNS_IT.replace("$FAKE_HOME", "$FAKE_HOME/my home")));
    w.session("m1").on_machine("typed-img").in_tmux("%1").prompt("hi").stop("Done.");
    let told = w.ask(json!({"type": "send_to", "session_id": "m1", "text": "look", "images": [{"name": "a.png", "mime": "image/png", "data": base64::engine::general_purpose::STANDARD.encode(bytes(10))}], "now": false}));
    assert_eq!(told["ok"], true, "{told}");
    let typed = w.terminal.typed("m1");
    let there = ssh.home("typed-img").join("my home/.cue/uploads").to_string_lossy().replace(' ', "\\ ");
    assert!(typed.last().is_some_and(|t| t.starts_with(&format!("look {there}/"))), "{typed:?}");
}
