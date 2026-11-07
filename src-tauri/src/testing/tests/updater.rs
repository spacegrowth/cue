//! Updates (stability plan §4, updater): find_update against a release server of the test's own on 127.0.0.1
//! serving latest.json (and, for the beta channel, latest-beta.json) as GitHub does, with Cue's real update key.
//! A mock app (tauri::test) is version 0.1.0.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How the release server answers latest.json.
#[derive(Clone)]
enum Answer {
    /// This release, `after` a while.
    Release { version: String, signature: String, after: Duration },
    /// This status, with this body.
    Status(u16, String),
    /// Takes the request and never answers.
    Never,
}

/// What the release server answers for each channel's file: latest.json, and latest-beta.json (a 404 when None).
#[derive(Clone)]
struct Answers {
    stable: Answer,
    beta: Option<Answer>,
}

/// The release server: what it answers, and the paths it was asked for.
struct Server {
    base: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Server {
    /// A server with only a stable release file.
    fn new(answer: Answer) -> Self {
        Self::with(Answers { stable: answer, beta: None })
    }

    fn with(answers: Answers) -> Self {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(vec![]));
        let (b, a) = (base.clone(), asked.clone());
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let (answers, base, asked) = (answers.clone(), b.clone(), a.clone());
                std::thread::spawn(move || serve(s, &answers, &base, &asked));
            }
        });
        Server { base, asked }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

fn serve(mut s: TcpStream, answers: &Answers, base: &str, asked: &Mutex<Vec<String>>) {
    let mut head = String::new();
    let mut r = BufReader::new(s.try_clone().unwrap());
    while r.read_line(&mut head).is_ok_and(|n| n > 0) && !head.ends_with("\r\n\r\n") {}
    let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
    asked.lock().unwrap().push(path.clone());
    let not_found = Answer::Status(404, "Not Found".into());
    let answer = match path.as_str() {
        "/latest-beta.json" => answers.beta.as_ref().unwrap_or(&not_found),
        _ => &answers.stable,
    };
    let (status, body) = match (path.as_str(), answer) {
        ("/cue.app.tar.gz", _) => (200, "not really an app".to_string()),
        (_, Answer::Never) => return std::thread::sleep(Duration::from_secs(120)),
        (_, Answer::Status(code, body)) => (*code, body.clone()),
        (_, Answer::Release { version, signature, after }) => {
            std::thread::sleep(*after);
            let platform = json!({"signature": signature, "url": format!("{base}/cue.app.tar.gz")});
            (200, json!({"version": version, "notes": "What's new", "pub_date": "2026-11-01T00:00:00Z", "platforms": {"darwin-aarch64": platform, "darwin-x86_64": platform}}).to_string())
        }
    };
    let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
}

/// Cue's own update key, as the app has it.
fn cue_pubkey() -> String {
    let conf: Value = serde_json::from_str(include_str!("../../../tauri.conf.json")).unwrap();
    conf["plugins"]["updater"]["pubkey"].as_str().unwrap().to_string()
}

/// A signature in minisign's form, made with a key that isn't Cue's.
fn signed_by_another_key() -> String {
    use base64::Engine;
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    let mut sig = b"Ed".to_vec();
    sig.extend([7u8; 8]);
    sig.extend([1u8; 64]);
    let text = format!("untrusted comment: signature from another key\n{}\ntrusted comment: timestamp:1700000000\tfile:cue.app.tar.gz\n{}\n", b64(&sig), b64(&[2u8; 64]));
    b64(text.as_bytes())
}

/// A Cue (version 0.1.0) whose updater asks `server`.
fn app(server: &Server) -> tauri::App<tauri::test::MockRuntime> {
    let mut ctx = tauri::test::mock_context(tauri::test::noop_assets());
    let updater = json!({"pubkey": cue_pubkey(), "endpoints": [format!("{}/latest.json", server.base)], "dangerousInsecureTransportProtocol": true});
    ctx.config_mut().plugins.0.insert("updater".into(), updater);
    tauri::test::mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(ctx).unwrap()
}

fn release(version: &str) -> Answer {
    Answer::Release { version: version.into(), signature: signed_by_another_key(), after: Duration::ZERO }
}

fn on(server: &Server, channel: crate::ipc::Channel) -> Result<Option<String>, String> {
    let app = app(server);
    tauri::async_runtime::block_on(crate::ipc::check_within(app.handle(), Duration::from_secs(5), channel)).map(|u| u.map(|u| u.version))
}

fn found(answer: Answer) -> Result<Option<String>, String> {
    let server = Server::new(answer);
    let app = app(&server);
    tauri::async_runtime::block_on(crate::ipc::find_update(app.handle())).map(|u| u.map(|u| u.version))
}

#[test]
fn a_newer_release_is_offered_with_its_notes() {
    let server = Server::new(release("0.2.0"));
    let app = app(&server);
    let u = tauri::async_runtime::block_on(crate::ipc::find_update(app.handle())).unwrap().expect("0.2.0 is newer than 0.1.0");
    assert_eq!((u.version.as_str(), u.body.as_deref()), ("0.2.0", Some("What's new")));
    assert_eq!(server.asked(), vec!["/latest.json"]);
}

#[test]
fn the_same_release_or_an_older_one_isnt_offered() {
    assert_eq!(found(release("0.1.0")), Ok(None));
    assert_eq!(found(release("0.0.9")), Ok(None));
}

#[test]
fn no_release_there_or_a_page_that_isnt_one_says_it_couldnt_check() {
    for answer in [Answer::Status(404, "Not Found".into()), Answer::Status(200, "<html>captive portal</html>".into())] {
        let err = found(answer).unwrap_err();
        assert!(err.starts_with("Couldn't check for updates: "), "{err}");
    }
}

#[test]
fn a_release_not_signed_with_cues_key_is_never_installed() {
    let server = Server::new(release("0.2.0"));
    let app = app(&server);
    let err = tauri::async_runtime::block_on(crate::ipc::install_update(app.handle().clone())).unwrap_err();
    assert!(err.starts_with("Couldn't install the update: "), "{err}");
    assert_eq!(server.asked(), vec!["/latest.json", "/cue.app.tar.gz"], "downloaded, checked against Cue's key, refused");
}

#[test]
fn a_slow_release_server_is_waited_for() {
    let answer = Answer::Release { version: "0.2.0".into(), signature: signed_by_another_key(), after: Duration::from_millis(1500) };
    assert_eq!(found(answer), Ok(Some("0.2.0".into())));
}

#[test]
fn a_release_server_that_never_answers_is_given_up_on() {
    let server = Server::new(Answer::Never);
    let app = app(&server);
    let start = Instant::now();
    let check = crate::ipc::check_within(app.handle(), Duration::from_secs(1), crate::ipc::Channel::Stable);
    let r = tauri::async_runtime::block_on(async { tokio::time::timeout(Duration::from_secs(20), check).await });
    let Err(err) = r.expect("the check waited on and on") else { panic!("a server that never answered offered a release") };
    assert!(err.starts_with("Couldn't check for updates: "), "{err}");
    assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
}

#[test]
fn the_beta_channel_reads_the_beta_file_and_offers_its_release() {
    let server = Server::with(Answers { stable: release("0.2.0"), beta: Some(release("0.3.0")) });
    assert_eq!(on(&server, crate::ipc::Channel::Beta), Ok(Some("0.3.0".into())));
    assert_eq!(server.asked(), vec!["/latest-beta.json"], "never the stable file");
}

#[test]
fn the_stable_channel_never_sees_a_beta() {
    let server = Server::with(Answers { stable: release("0.2.0"), beta: Some(release("0.3.0")) });
    assert_eq!(on(&server, crate::ipc::Channel::Stable), Ok(Some("0.2.0".into())));
    assert_eq!(server.asked(), vec!["/latest.json"]);
}

#[test]
fn a_beta_release_not_signed_with_cues_key_is_never_installed() {
    let server = Server::with(Answers { stable: release("0.1.0"), beta: Some(release("0.3.0")) });
    let app = app(&server);
    let found = tauri::async_runtime::block_on(crate::ipc::check_within(app.handle(), Duration::from_secs(5), crate::ipc::Channel::Beta));
    let u = found.unwrap().expect("0.3.0 is on beta");
    assert!(tauri::async_runtime::block_on(u.download_and_install(|_, _| {}, || {})).is_err(), "installed a release signed with another key");
    assert_eq!(server.asked(), vec!["/latest-beta.json", "/cue.app.tar.gz"], "downloaded from beta, checked against Cue's key, refused");
}

#[test]
fn no_beta_file_on_the_server_says_it_couldnt_check() {
    let server = Server::new(release("0.2.0"));
    let err = on(&server, crate::ipc::Channel::Beta).unwrap_err();
    assert!(err.starts_with("Couldn't check for updates: "), "{err}");
}
