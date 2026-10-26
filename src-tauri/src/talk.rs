//! Talking to a session from anywhere on the Mac (Settings → Voice, off until turned on): hold right ⇧ (or
//! ⌥Space), talk, let go. cue-voice (voice/, shipped inside Cue.app) does the key, the island at the top of the
//! screen and the captions bar, the microphone, NVIDIA's model and reading out; this side tells it who you'd be
//! talking to (the chat open in Cue's window, else the session that moved last), sends what you said (your words,
//! then one line asking for a short answer: `message`) and gives it the answer once the turn is over. That
//! session's next replies for half an hour are read out too, and the turn under way survives a restart of Cue.
//! They talk in JSON lines over its stdin and stdout.

use crate::hub::Hub;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::Listener;
use tokio::sync::{mpsc, watch};

/// The line asking for a short answer. A session whose hook can add it (Claude Code on this Mac) gets it from the
/// hook, as context only the agent reads, so the chat shows just what you said; any other gets it after your words.
pub const ASK: &str = "(Voice: I'm listening, not reading. Answer in one or two short, plain sentences: no lists, code or file paths unless I ask for them. If there's a link I'll need, put it at the end: I'll see it on screen.)";

/// What goes to the session for `said`: your words, then (unless its hook adds it) the line asking for a short
/// answer. None when you said nothing.
pub fn message(said: &str, hooked: bool) -> Option<String> {
    let said = said.trim();
    (!said.is_empty()).then(|| if hooked { said.to_string() } else { format!("{said}\n\n{ASK}") })
}

/// What you said by voice, a file each in Cue's data folder (voice-said/<ms>.txt, `.used` once the hook added the
/// line): the hook, a process of its own, finds it there without asking Cue, and following a session can tell
/// your voice from your typing once the line isn't in the message. Kept an hour.
fn said_dir() -> std::path::PathBuf {
    crate::server::cue_dir().join("voice-said")
}
const SAID_KEEP_MS: u64 = 60 * 60 * 1000;
/// How long after it was typed in the hook still takes it as yours (it arrives within seconds).
const SAID_FRESH_MS: u64 = 10 * 60 * 1000;

/// Words compared as the agent's prompt has them: the same words, however the spaces and lines came out.
fn plain(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The voice messages in `dir`: when, what, and whether the hook has added its line yet.
fn said_in(dir: &std::path::Path) -> Vec<(u64, String, bool)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_str()?;
            if ext != "txt" && ext != "used" {
                return None;
            }
            let at = p.file_stem()?.to_str()?.parse::<u64>().ok()?;
            Some((at, plain(&std::fs::read_to_string(&p).ok()?), ext == "used"))
        })
        .collect()
}

fn remember_in(dir: &std::path::Path, text: &str, now: u64) {
    let _ = std::fs::create_dir_all(dir);
    for (at, _, used) in said_in(dir) {
        if at + SAID_KEEP_MS < now {
            let _ = std::fs::remove_file(dir.join(format!("{at}.{}", if used { "used" } else { "txt" })));
        }
    }
    let _ = std::fs::write(dir.join(format!("{now}.txt")), text);
}

/// The hook's part (UserPromptSubmit, Claude Code on this Mac): `prompt` is something you just said by voice, so
/// the line asking for a short answer goes with it. Each said once: it's marked used.
pub fn hook_context(prompt: &str) -> Option<&'static str> {
    hook_context_in(&said_dir(), prompt, crate::model::now_ms())
}
fn hook_context_in(dir: &std::path::Path, prompt: &str, now: u64) -> Option<&'static str> {
    let want = plain(prompt);
    let (at, _, _) = said_in(dir).into_iter().filter(|(at, t, used)| !used && *t == want && at + SAID_FRESH_MS >= now).max_by_key(|s| s.0)?;
    std::fs::rename(dir.join(format!("{at}.txt")), dir.join(format!("{at}.used"))).ok()?;
    Some(ASK)
}

/// The session's own hook adds the line: Claude Code (Codex's and Pi's don't take added context) on this Mac (a
/// machine's hook can't read Cue's data folder).
fn hooked(hub: &Hub, sid: &str) -> bool {
    hub.session_origin(sid).is_some_and(|o| o.harness == "claude" && o.machine.is_empty())
}

/// How it is, for Settings: whether ⌥Space could be had, and the listening model.
static STATUS: Mutex<Value> = Mutex::new(Value::Null);

/// When you last talked to Cue or had an answer read out to you (ms): you're at this Mac, hands off it or not.
static HEARD_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn heard() {
    HEARD_MS.store(crate::model::now_ms(), std::sync::atomic::Ordering::Relaxed);
}
#[cfg_attr(not(feature = "ext"), allow(dead_code))]
pub fn heard_ms() -> u64 {
    HEARD_MS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Your voice settings, which cue-voice reads as it needs them (its Prefs.swift): voice.json in Cue's data folder.
fn prefs_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("voice.json")
}
fn prefs() -> Value {
    std::fs::read_to_string(prefs_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}))
}

/// One of Settings → Voice's picks: `read` ("on" / "off"), `model` ("parakeet" / "apple"). The voice answers are
/// read in is Cue's own setting (voice.pick, its Voice tab), sent with each answer.
pub fn set(id: &str, value: &str) -> Result<(), String> {
    let mut p = prefs();
    if !p.is_object() {
        p = json!({});   // a voice.json that isn't an object: start it over rather than fail to set anything
    }
    p[id] = match id {
        "model" => json!(value),
        "read" => json!(value == "on"),
        "auto" if ["always", "front", "never"].contains(&value) => json!(value),
        "captions" | "talk" => json!(value == "on"),
        _ => return Err(format!("no setting {id}")),
    };
    std::fs::write(prefs_file(), p.to_string()).map_err(|e| e.to_string())?;
    // Turned on or off: cue-voice takes the keys, or gives them back, now. NVIDIA's listening model picked: it
    // gets it now (nothing is downloaded before it's picked).
    let retell = match id {
        "talk" => Some(json!({ "talk": value == "on" })),
        "model" => Some(json!({ "model": value })),
        _ => None,
    };
    if let (Some(line), Some(tx)) = (retell, crate::locking::lock(&RETELL, "talk retell").as_ref()) {
        let _ = tx.send(line);
    }
    Ok(())
}

/// A line for cue-voice from outside the loop (a setting changed), sent on by it.
static RETELL: Mutex<Option<mpsc::UnboundedSender<Value>>> = Mutex::new(None);

/// cue-voice in its dictation mode, for the mic in Cue's text boxes (the same lines as cue-listen's): once talking
/// is on, so NVIDIA's model is here.
pub fn dictation() -> Option<Command> {
    if prefs()["talk"] != true || cfg!(not(target_arch = "aarch64")) {
        return None;
    }
    let exe = helper()?;
    let mut c = Command::new(exe);
    c.arg("listen");
    Some(c)
}

fn helper() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("cue-voice"))).filter(|p| p.exists())
}

/// What you said, waiting on its answer: the session, and the newest message it had before (its answer is
/// the agent's message after that, once the turn is over).
struct Turn {
    sid: String,
    after: u64,
}

/// After a voice turn's answer, that session's next replies are read out too (a turn it started itself, a reply to
/// something typed): the session, its newest reply already told, and until when (half an hour after the last).
/// Every session you've talked to is followed this way, each on its own: talking to another doesn't end it.
struct Follow {
    sid: String,
    seen: u64,
    until: u64,
}
const FOLLOW_MS: u64 = 30 * 60 * 1000;

/// The turn under way and the sessions followed, kept in Cue's data folder so a restart of Cue (an update, an
/// install) doesn't forget them.
fn saved_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("voice-turn.json")
}
fn save(turn: &Option<Turn>, follows: &[Follow]) {
    let v = json!({
        "turn": turn.as_ref().map(|t| json!({ "sid": t.sid, "after": t.after })),
        "follows": follows.iter().map(|f| json!({ "sid": f.sid, "seen": f.seen, "until": f.until })).collect::<Vec<_>>(),
    });
    let _ = std::fs::write(saved_file(), v.to_string());
}
fn load() -> (Option<Turn>, Vec<Follow>) {
    let v = std::fs::read_to_string(saved_file()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null);
    let turn = v["turn"]["sid"].as_str().map(|sid| Turn { sid: sid.into(), after: v["turn"]["after"].as_u64().unwrap_or(0) });
    let one = |f: &Value| f["sid"].as_str().map(|sid| Follow { sid: sid.into(), seen: f["seen"].as_u64().unwrap_or(0), until: f["until"].as_u64().unwrap_or(0) });
    // Saved before more than one was followed: "follow", a single one.
    let follows = match v["follows"].as_array() {
        Some(all) => all.iter().filter_map(one).collect(),
        None => one(&v["follow"]).into_iter().collect(),
    };
    (turn, follows)
}

/// `sid` followed from now (its newest reply, `seen`, already told), in place of any earlier following of it.
fn follow_on(follows: &mut Vec<Follow>, sid: &str, seen: u64) {
    follows.retain(|f| f.sid != sid);
    follows.push(Follow { sid: sid.into(), seen, until: crate::model::now_ms() + FOLLOW_MS });
}

/// Start cue-voice, when this Cue has it.
pub fn start(app: &tauri::App, hub: Arc<Hub>) {
    // A quiet test instance has no keys, no microphone and no island.
    if std::env::var_os("CUE_QUIET").is_some() {
        return;
    }
    let Some(exe) = helper() else { return };
    // Each snapshot the window gets.
    let (tx, live) = watch::channel(Arc::new(String::new()));
    app.listen_any("state", move |e| {
        let _ = tx.send(Arc::new(e.payload().to_string()));
    });
    let child = Command::new(&exe).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let Ok(mut child) = child else { return };
    let (Some(to), Some(from)) = (child.stdin.take(), child.stdout.take()) else { return };
    let (lines, rx) = mpsc::unbounded_channel::<Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(from).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                let _ = lines.send(v);
            }
        }
        let _ = child.wait();
    });
    // The window says which chat is open, and the swatch it wears (app.js, when either changes).
    let (open_tx, open_rx) = watch::channel(None::<String>);
    app.listen_any("chat-open", move |e| {
        let v = serde_json::from_str::<Value>(e.payload()).unwrap_or_default();
        if let Some(i) = v["swatch"].as_u64() {
            colors::THEME.store(i as usize, std::sync::atomic::Ordering::Relaxed);
        }
        let _ = open_tx.send(v["sid"].as_str().map(str::to_string));
    });
    tauri::async_runtime::spawn(run(app.handle().clone(), hub, live, open_rx, rx, to));
}

/// Its rows in Settings → Voice, under Cue's own voice picker: the key, how the listening model is, reading out.
pub fn settings() -> Option<Value> {
    let p = prefs();
    let on = p["talk"] == true;
    let talk = json!({ "title": "Talk to sessions",
        "sub": if on { "Hold right ⇧ in any app and talk; let go to send it." } else { "Off. On: hold right ⇧ in any app, talk, and let go to send it to the chat open in Cue. The first time, Cue asks for the microphone. What you say is written down by Apple's recognizer on this Mac, or by NVIDIA's if you pick it (Listening)." },
        "pick": { "id": "talk", "value": if on { "on" } else { "off" }, "options": [["on", "On"], ["off", "Off"]] } });
    let st = crate::locking::lock(&STATUS, "talk status").clone();
    // Off, or on but cue-voice hasn't said how it is yet: only the switch.
    if !on || st.is_null() {
        return Some(json!({ "group": "Talking to a session", "tab": "Voice", "rows": [talk] }));
    }
    // Picked in Settings, else what cue-voice took (NVIDIA's when it was already here, else Apple's).
    let pick = p["model"].as_str().or(st["pick"].as_str()).unwrap_or("apple").to_string();
    let model = if pick != "parakeet" {
        "Apple's recognizer, on this Mac. NVIDIA's Parakeet writes down what you say more accurately: pick it to download it (about 450 MB, once, from Hugging Face).".to_string()
    } else {
        match st["model"].as_str().unwrap_or("") {
            "ready" => "NVIDIA's Parakeet, on this Mac: what's sent is its text. Nothing you say leaves the Mac.".to_string(),
            "downloading" | "off" => format!("Getting NVIDIA's listening model ({}% of about 450 MB). Until it's here, Apple's recognizer is used.", (st["progress"].as_f64().unwrap_or(0.0) * 100.0) as u32),
            "loading" => "Loading NVIDIA's listening model…".to_string(),
            m => format!("NVIDIA's listening model isn't working ({}); Apple's recognizer is used.", m.trim_start_matches("failed: ")),
        }
    };
    let key = if st["key"].as_bool() == Some(false) {
        "In any app: hold right ⇧, talk, let go to send it to the chat open in Cue (else the session that moved last). ⌥Space can't be used: another app has it."
    } else {
        "In any app: hold right ⇧ (or ⌥Space), talk, let go to send it to the chat open in Cue (else the session that moved last). While holding, esc cancels and left ⇧ pauses. While it works it's a chip at the top; when it answers, the pill comes back and waits: Listen (or tap right ⇧) reads it out."
    };
    let read = if p["read"] == false { "off" } else { "on" };
    Some(json!({ "group": "Talking to a session", "tab": "Voice", "rows": [
        talk,
        { "title": "Hold right ⇧ to talk", "sub": key },
        { "title": "Listening", "sub": model,
          "pick": { "id": "model", "value": pick, "options": [["apple", "Apple"], ["parakeet", "NVIDIA Parakeet"]] } },
        { "title": "Read answers out", "sub": "When you talked and it answers, Listen (or right ⇧, a tap or a hold) reads it out in the voice above. Off: no Listen, it's only shown.",
          "pick": { "id": "read", "value": read, "options": [["on", "On"], ["off", "Off"]] } },
        { "title": "Live captions", "sub": "A bar along the bottom of the screen: the session, your words as you say them, its answer as it's read; while it works, a dot breathes in the bottom right corner. The bar shows no keys (they're below). Off: it's all in the island at the top, the keys too.",
          "pick": { "id": "captions", "value": if p["captions"] == true { "on" } else { "off" }, "options": [["on", "On"], ["off", "Off"]] } },
        { "title": "Keys", "sub": "While you talk: let go sends it, esc cancels, left ⇧ pauses (hold right ⇧ again to go on), ↩ types it into the chat instead, → with right ⇧ asks it by the way. When its answer is back: a tap on right ⇧ listens. While it reads: a tap on right ⇧ pauses and goes on, holding it talks back, esc stops (or goes to the next answer waiting). A click on the dot opens the chat." },
        { "title": "Start reading by itself", "sub": "When its answer is back: read it out at once, always or only while Cue is the app in front. Never: it waits for Listen.",
          "pick": { "id": "auto", "value": p["auto"].as_str().unwrap_or("never"), "options": [["always", "Always"], ["front", "When Cue is in front"], ["never", "Never"]] } },
    ] }))
}

/// The conversation with cue-voice, and when it ends however it ends (cue-voice gone, Cue quitting), nothing it
/// set stays: the sessions' notifications come back.
async fn run(app: tauri::AppHandle, hub: Arc<Hub>, live: watch::Receiver<Arc<String>>, open: watch::Receiver<Option<String>>, from: mpsc::UnboundedReceiver<Value>, to: ChildStdin) {
    run_loop(app, hub, live, open, from, to).await;
    crate::notify::quiet(Vec::new());
}

async fn run_loop(app: tauri::AppHandle, hub: Arc<Hub>, mut live: watch::Receiver<Arc<String>>, mut open: watch::Receiver<Option<String>>, mut from: mpsc::UnboundedReceiver<Value>, mut to: ChildStdin) {
    let mut snap = Value::Null;
    let mut told = Value::Null;   // the last target sent
    let (mut turn, mut follows) = load();
    // A turn under way when Cue restarted: cue-voice is told once there's a snapshot to say whose.
    let mut resumed = turn.is_none();
    let mut told_doing = String::new();   // what the session was last said to be doing
    let mut quieted: Vec<String> = Vec::new();   // the sessions told to notify::quiet
    // "By the way" answers, asked off this loop (they take a few seconds), come back here to be told.
    let (btw_tx, mut btw_rx) = mpsc::unbounded_channel::<Value>();
    *crate::locking::lock(&RETELL, "talk retell") = Some(btw_tx.clone());   // a setting's line goes the same way
    // What you said, sent off this loop too (a remote session can take seconds): how it went comes back here.
    let (sent_tx, mut sent_rx) = mpsc::unbounded_channel::<(String, u64, Result<String, String>)>();
    loop {
        tokio::select! {
            changed = live.changed() => {
                if changed.is_err() { return; }
                let Ok(v) = serde_json::from_str::<Value>(&live.borrow_and_update()) else { continue };
                snap = v;
            }
            changed = open.changed() => { if changed.is_err() { return; } }
            Some(out) = btw_rx.recv() => {
                if tell(&mut to, &out).is_err() { return; }
            }
            Some((sid, after, reply)) = sent_rx.recv() => {
                let out = match reply {
                    Ok(_) => {
                        // A turn still under way with another session: its answer still comes, as a followed one.
                        if let Some(old) = turn.take().filter(|old| old.sid != sid) {
                            follow_on(&mut follows, &old.sid, old.after);
                        }
                        // This one's answer comes as the turn's, not twice.
                        follows.retain(|f| f.sid != sid);
                        turn = Some(Turn { sid: sid.clone(), after });
                        save(&turn, &follows);
                        json!({ "sent": true, "sid": sid })
                    }
                    Err(e) => json!({ "error": format!("Couldn't send it: {e}") }),
                };
                if tell(&mut to, &out).is_err() { return; }
            }
            line = from.recv() => {
                let Some(line) = line else { return };
                if let Some(st) = line.get("status") {
                    *crate::locking::lock(&STATUS, "talk status") = st.clone();
                } else {
                    heard();
                }
                // A click on the island: that session's chat (whether or not a card of it waits).
                if let Some(sid) = line["open"].as_str() {
                    let (a, key) = (app.clone(), format!("s:{sid}"));
                    let _ = app.run_on_main_thread(move || crate::show_main(&a, Some(key)));
                }
                // Typing instead of talking: the chat opens with what you said in its box, unsent.
                // Into the chat's own box (its draft "s:<session>"), whether or not a card waits: a card's box is the
                // board's, and a working session has none.
                if let (Some(text), Some(sid)) = (line["draft"].as_str(), line["sid"].as_str()) {
                    let (a, text, key) = (app.clone(), text.to_string(), format!("s:{sid}"));
                    let _ = app.run_on_main_thread(move || {
                        use tauri::Emitter;
                        crate::show_main(&a, Some(key.clone()));
                        if !text.is_empty() { let _ = a.emit_to("main", "fill", json!({ "id": key, "text": text })); }
                    });
                }
                // Right ⇧ and → held together: a side question about the session ("by the way"), asked of a copy of
                // its conversation; the session itself never sees it and keeps working. Its answer is read out.
                if let (Some(q), Some(sid)) = (line["btw"].as_str(), line["sid"].as_str()) {
                    let (h, sid, q, tx, voice, who) = (hub.clone(), sid.to_string(), message(q, false).unwrap_or_default(), btw_tx.clone(), snap["settings"]["voice"]["pick"].clone(), target(&snap, Some(sid)));
                    tokio::spawn(async move {
                        let asked = tokio::task::spawn_blocking(move || {
                            let origin = h.session_origin(&sid).ok_or("that session is gone")?;
                            crate::btw::ask_about(&origin, &q)
                        }).await;
                        let out = match asked {
                            Ok(Ok(a)) => json!({ "answer": a, "btw": true, "voice": voice, "who": who }),
                            Ok(Err(e)) => json!({ "error": format!("Couldn't ask it: {e}"), "btw": true }),
                            Err(e) => json!({ "error": format!("Couldn't ask it: {e}"), "btw": true }),
                        };
                        let _ = tx.send(out);
                    });
                }
                if let (Some(said), Some(sid)) = (line["said"].as_str(), line["sid"].as_str()) {
                    let after = newest(&snap, sid);
                    let (h, s, sid, said, tx) = (hub.clone(), snap.clone(), sid.to_string(), said.to_string(), sent_tx.clone());
                    tokio::spawn(async move {
                        let reply = send(&h, &s, &sid, &said).await;
                        let _ = tx.send((sid, after, reply));
                    });
                }
            }
        }
        // Who you'd be talking to now, when that changed.
        let now = target(&snap, open.borrow().as_deref());
        if now != told {
            if tell(&mut to, &json!({ "target": now })).is_err() { return; }
            told = now;
        }
        // The turn's session has gone (closed, stopped) before it answered: no answer will come, so it's over.
        if let Some(t) = &turn {
            if gone(&snap, &t.sid) {
                turn = None;
                save(&turn, &follows);
                if tell(&mut to, &json!({ "stopped": true })).is_err() { return; }
            }
        }
        if !resumed && !snap.is_null() {
            resumed = true;
            if let Some(t) = &turn {
                if tell(&mut to, &json!({ "resume": target(&snap, Some(&t.sid)) })).is_err() { return; }
            }
        }
        // In a voice conversation (a turn under way, or sessions followed): those sessions' notifications stay
        // quiet: their answers are read out to you.
        let talking: Vec<String> = turn.iter().map(|t| t.sid.clone()).chain(follows.iter().map(|f| f.sid.clone())).collect();
        if talking != quieted {
            crate::notify::quiet(talking.clone());
            quieted = talking;
        }
        // What the session you talked to is doing now, for the island by the notch while it works.
        if let Some(d) = turn.as_ref().and_then(|t| session(&snap, &t.sid)).map(doing) {
            if d != told_doing {
                if tell(&mut to, &json!({ "doing": d })).is_err() { return; }
                told_doing = d;
            }
        }
        // The turn under way: over yet? Its answer goes with the voice to read it in.
        if let Some(t) = &turn {
            if let Some(mut out) = outcome(&snap, t) {
                out["voice"] = snap["settings"]["voice"]["pick"].clone();
                if out.get("answer").is_some() {
                    follow_on(&mut follows, &t.sid, newest(&snap, &t.sid));
                }
                turn = None;
                save(&turn, &follows);
                if tell(&mut to, &out).is_err() { return; }
            }
        }
        // The sessions followed, each on its own (whether or not a turn with another is under way): one that said
        // more since is read out too, as from it. Expired, or you've since written to it yourself (typed, not
        // said): you're in its chat now, not voice.
        let before = follows.len();
        follows.retain(|f| crate::model::now_ms() <= f.until && !typed_since(&snap, f));
        let mut changed = follows.len() != before;
        for f in follows.iter_mut() {
            if let Some((at, text)) = followed(&snap, f) {
                f.seen = at;
                f.until = crate::model::now_ms() + FOLLOW_MS;
                changed = true;
                let out = json!({ "answer": text, "follow": true, "who": target(&snap, Some(&f.sid)), "voice": snap["settings"]["voice"]["pick"] });
                if tell(&mut to, &out).is_err() { return; }
            }
        }
        if changed {
            save(&turn, &follows);
        }
    }
}

fn tell(to: &mut ChildStdin, v: &Value) -> std::io::Result<()> {
    writeln!(to, "{v}")?;
    to.flush()
}

/// What you said goes to `sid`: as the reply to its finished turn when one waits, else as a new message.
async fn send(hub: &Arc<Hub>, snap: &Value, sid: &str, said: &str) -> Result<String, String> {
    let hooked = hooked(hub, sid);
    let msg = message(said, hooked).ok_or("nothing was said")?;
    // Known as yours before it's typed in, so the hook finds it as soon as the agent takes it.
    if hooked {
        remember_in(&said_dir(), &msg, crate::model::now_ms());
    }
    let waiting = items_of(snap, sid).find(|i| i["kind"] == "waiting").and_then(|i| i["id"].as_str()).map(str::to_string);
    let (h, sid) = (hub.clone(), sid.to_string());
    tokio::task::spawn_blocking(move || {
        let sent = match waiting {
            Some(id) => h.reply(&id, &msg, &[]),
            None => h.send_to_session(&sid, &msg, &[], false),
        };
        // Its bubble shows it was said, wherever it went: the chat, kept in Cue, or queued in the agent's box.
        if sent.is_ok() {
            h.said_by_voice(&sid, &msg);
        }
        sent
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The turn's end, once there is one: its answer, a question or permission it asks meanwhile, or that it was
/// stopped (by you, here or in its terminal) before it answered.
fn outcome(snap: &Value, t: &Turn) -> Option<Value> {
    let s = session(snap, &t.sid)?;
    if let Some(ask) = items_of(snap, &t.sid).find(|i| i["kind"] != "waiting") {
        return Some(json!({ "answer": format!("It's asking: {}", asking(ask)) }));
    }
    // Still at it, or at its prompt waiting on background agents it started (Claude Code wakes it when they
    // report; what it said meanwhile isn't the answer).
    if s["state"] == "working" || (s["state"] == "agent" && s["helpers"].as_u64().unwrap_or(0) > 0) {
        return None;
    }
    let thread = s["thread"].as_array()?;
    if let Some(reply) = thread.iter().rev().find(|e| e["role"] == "agent" && e["at_ms"].as_u64().unwrap_or(0) > t.after) {
        return Some(json!({ "answer": reply["text"] }));
    }
    let stopped = items_of(snap, &t.sid).any(|i| i["kind"] == "waiting" && i["interrupted"] == true && i["created_ms"].as_u64().unwrap_or(0) > t.after);
    stopped.then(|| json!({ "stopped": true }))
}

/// `sid` isn't running any more (Cue's record of it is gone, or it stopped), once Cue has a snapshot at all.
fn gone(snap: &Value, sid: &str) -> bool {
    !snap.is_null() && session(snap, sid).is_none_or(|s| s["state"] == "stopped")
}

/// The followed session's newest reply after the one already told, once its turn is over: when, and what.
fn followed(snap: &Value, f: &Follow) -> Option<(u64, String)> {
    let s = session(snap, &f.sid)?;
    if s["state"] == "working" {
        return None;
    }
    let e = s["thread"].as_array()?.iter().rev().find(|e| e["role"] == "agent")?;
    let at = e["at_ms"].as_u64().unwrap_or(0);
    (at > f.seen).then(|| (at, e["text"].as_str().unwrap_or("").to_string())).filter(|(_, t)| !t.trim().is_empty())
}

/// You wrote to the followed session yourself since its last reply was told: a message that isn't a voice one
/// (those end with the line asking for a short spoken answer, or, its hook adding that, are in voice-said), and
/// isn't how you answered a permission or a question (those can't be answered by voice).
fn typed_since(snap: &Value, f: &Follow) -> bool {
    typed_since_in(snap, f, &said_dir())
}
fn typed_since_in(snap: &Value, f: &Follow, dir: &std::path::Path) -> bool {
    let Some(thread) = session(snap, &f.sid).and_then(|s| s["thread"].as_array()) else { return false };
    let mut new = thread.iter().filter(|e| e["role"] == "you" && e["decision"] != true && e["at_ms"].as_u64().unwrap_or(0) > f.seen).map(|e| e["text"].as_str().unwrap_or("")).peekable();
    if new.peek().is_none() {
        return false;   // nothing since (the folder isn't read for that)
    }
    let said = said_in(dir);
    new.any(|t| !t.contains("(Voice: I'm listening") && !said.iter().any(|(_, s, _)| *s == plain(t)))
}

/// A session's step now, as its card says it: "Running: cargo test", "Compacting…", else "Thinking".
fn doing(s: &Value) -> String {
    if s["compacting_ms"].as_u64().unwrap_or(0) > 0 {
        return "Compacting…".into();
    }
    s["doing"].as_str().filter(|d| !d.is_empty()).unwrap_or("Thinking").to_string()
}

/// A question or permission in a few words.
fn asking(i: &Value) -> String {
    if i["kind"] == "question" {
        if let Some(q) = i["tool_input"]["questions"][0]["question"].as_str() {
            return q.to_string();
        }
    }
    let input = &i["tool_input"];
    match i["tool_name"].as_str().unwrap_or("") {
        "Bash" => format!("it wants to run {}", input["command"].as_str().unwrap_or("a command")),
        "" => i["message"].as_str().filter(|m| !m.is_empty()).unwrap_or("something in its terminal").to_string(),
        tool => format!("it wants to use {tool}"),
    }
}

/// Who you'd be talking to: the chat open in the window if its session runs, else the running session that
/// moved last. Its name and colors, as the pill shows them.
fn target(snap: &Value, open: Option<&str>) -> Value {
    let running = || snap["sessions"].as_array().into_iter().flatten().filter(|s| s["state"] != "stopped");
    let s = open.and_then(|sid| running().find(|s| s["session_id"] == sid)).or_else(|| running().max_by_key(|s| s["since_ms"].as_u64().unwrap_or(0)));
    let Some(s) = s else { return Value::Null };
    let project = s["project"].as_str().unwrap_or("");
    let name = s["name"].as_str().map(|n| n.trim_start_matches("[Exec] ").trim_start_matches("[Lead] ")).filter(|n| !n.is_empty())
        .or_else(|| s["cwd"].as_str().and_then(|c| c.rsplit('/').find(|p| !p.is_empty())))
        .unwrap_or(project);
    let look = colors::of(snap, project);
    json!({
        "sid": s["session_id"],
        "name": if name == project { name.to_string() } else { format!("{project} · {name}") },
        "agent": match s["harness"].as_str().unwrap_or("") { "codex" => "Codex", "pi" => "Pi", _ => "Claude Code" },
        "bg": look.0, "onBg": look.1, "sel": look.2, "onSel": look.3,
    })
}

fn session<'a>(snap: &'a Value, sid: &str) -> Option<&'a Value> {
    snap["sessions"].as_array()?.iter().find(|s| s["session_id"] == sid)
}
fn items_of<'a>(snap: &'a Value, sid: &'a str) -> impl Iterator<Item = &'a Value> {
    snap["items"].as_array().into_iter().flatten().filter(move |i| i["session_id"] == sid)
}
/// The time of `sid`'s newest message (not the count: Cue keeps only a session's last few).
fn newest(snap: &Value, sid: &str) -> u64 {
    session(snap, sid).and_then(|s| s["thread"].as_array()).and_then(|t| t.last()).and_then(|e| e["at_ms"].as_u64()).unwrap_or(0)
}

/// The window's colors (style.css .proj-theme): the same OKLCH lightness and chroma per
/// token, at its swatch's hue. Light mode: the pill is light.
mod colors {
    use serde_json::Value;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// The swatch the window wears (its header's project, or All projects'), as it last said; MAX until it has.
    pub static THEME: AtomicUsize = AtomicUsize::new(usize::MAX);

    /// Hue and strength per swatch (Moss, 0, is Cue's own).
    const HUE: [(f64, f64); 10] = [(0.0, 0.0), (107.0, 0.3), (301.0, 0.57), (87.0, 0.75), (67.0, 0.37), (157.0, 0.33), (194.0, 0.48), (276.0, 0.64), (353.0, 0.44), (271.0, 0.1)];

    /// (bg, onBg, sel, onSel): the window's, so the pill matches it; before the window has said, `project`'s.
    pub fn of(snap: &Value, project: &str) -> (String, String, String, String) {
        let i = match THEME.load(Ordering::Relaxed) {
            usize::MAX => swatch(snap, project),
            i => i,
        };
        if i == 0 || i >= HUE.len() {
            return ("#3F4A22".into(), "#FFFFFF".into(), "#D2DE96".into(), "#1E2410".into());
        }
        let (h, k) = HUE[i];
        (oklch(0.385, 0.072 * k, h), "#FFFFFF".into(), oklch(0.875, 0.098 * k, h), oklch(0.25, 0.04 * k, h))
    }

    /// The swatch you picked for it in Cue, else one from its name (as the window does).
    fn swatch(snap: &Value, project: &str) -> usize {
        if snap["settings"]["appearance"]["project"] == false {
            return 0;
        }
        if let Some(c) = snap["settings"]["project"]["colors"][project].as_u64() {
            return c as usize;
        }
        let mut h: u32 = 0;
        for ch in project.encode_utf16() {
            h = (h * 31 + ch as u32) % 9973;
        }
        (h % 10) as usize
    }

    /// OKLCH to #rrggbb, each channel clipped into sRGB.
    pub fn oklch(l: f64, c: f64, h: f64) -> String {
        let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
        let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
        let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
        let s_ = (l - 0.0894841775 * a - 1.291485548 * b).powi(3);
        let lin = [
            4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
            -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
            -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
        ];
        let ch = |v: f64| {
            let g = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
            (g.clamp(0.0, 1.0) * 255.0).round() as u8
        };
        format!("#{:02x}{:02x}{:02x}", ch(lin[0]), ch(lin[1]), ch(lin[2]))
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn moss_is_cues_own_and_a_swatch_gets_its_hue() {
            let snap = serde_json::json!({ "settings": { "project": { "colors": { "cue": 0, "billing": 6 } } } });
            assert_eq!(super::of(&snap, "cue").2, "#D2DE96");
            let teal = super::of(&snap, "billing");
            assert_ne!(teal.2, "#D2DE96");
            assert!(teal.0.starts_with('#') && teal.0.len() == 7, "{teal:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_you_said_goes_first_then_the_ask_for_a_short_answer() {
        let m = message("  make command T open a new tab \n", false).unwrap();
        assert!(m.starts_with("make command T open a new tab\n\n(Voice:"), "{m}");
        assert!(m.ends_with("I'll see it on screen.)"), "{m}");
        assert_eq!(message(" \n ", false), None, "nothing said, nothing sent");
        assert_eq!(message("  open a new tab \n", true).as_deref(), Some("open a new tab"), "its hook adds the ask: only your words");
    }

    fn said_dir_for(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cue-voice-said-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn the_hook_adds_the_ask_to_what_you_just_said_once_and_to_nothing_else() {
        let d = said_dir_for("hook");
        remember_in(&d, "open a new tab", 1_000);
        assert_eq!(hook_context_in(&d, "something typed", 2_000), None, "not something you said");
        assert_eq!(hook_context_in(&d, "open a new\n tab ", 2_000), Some(ASK), "the same words, however the spaces came out");
        assert_eq!(hook_context_in(&d, "open a new tab", 3_000), None, "once only: typed again, it's typing");
        remember_in(&d, "run the tests", 10_000);
        assert_eq!(hook_context_in(&d, "run the tests", 10_000 + SAID_FRESH_MS + 1), None, "too long ago to be this prompt");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn what_you_said_is_kept_an_hour() {
        let d = said_dir_for("keep");
        remember_in(&d, "old", 1_000);
        remember_in(&d, "new", 1_000 + SAID_KEEP_MS + 1);
        let kept: Vec<String> = said_in(&d).into_iter().map(|s| s.1).collect();
        assert_eq!(kept, vec!["new".to_string()]);
        let _ = std::fs::remove_dir_all(&d);
    }

    fn snap(state: &str, thread: Value, items: Value) -> Value {
        json!({ "sessions": [{ "session_id": "s1", "project": "cue", "state": state, "since_ms": 5, "thread": thread, "harness": "claude" }], "items": items })
    }

    #[test]
    fn a_followed_session_s_next_reply_is_told_once_its_turn_is_over_and_only_once() {
        let f = Follow { sid: "s1".into(), seen: 200, until: u64::MAX };
        let told = json!([{ "role": "agent", "text": "Done.", "at_ms": 200 }]);
        assert!(followed(&snap("waiting", told, json!([])), &f).is_none(), "already told");
        let more = json!([{ "role": "agent", "text": "Done.", "at_ms": 200 }, { "role": "agent", "text": "Also pushed it.", "at_ms": 300 }]);
        assert!(followed(&snap("working", more.clone(), json!([])), &f).is_none(), "still working");
        assert_eq!(followed(&snap("waiting", more, json!([])), &f), Some((300, "Also pushed it.".to_string())));
    }

    #[test]
    fn a_lead_waiting_on_its_background_agents_hasnt_answered_yet() {
        let t = Turn { sid: "s1".into(), after: 100 };
        let thread = json!([{ "role": "agent", "text": "Started two agents.", "at_ms": 200 }]);
        let mut s = snap("agent", thread.clone(), json!([]));
        s["sessions"][0]["helpers"] = json!(2);
        assert!(outcome(&s, &t).is_none(), "its agents are still at work");
        s["sessions"][0]["helpers"] = json!(0);
        assert_eq!(outcome(&s, &t), Some(json!({ "answer": "Started two agents." })), "another agent drives it (relay): its reply is the answer");
    }

    #[test]
    fn a_turn_whose_session_stopped_or_went_is_over_but_not_before_cue_has_a_snapshot() {
        assert!(!gone(&Value::Null, "s1"), "no snapshot yet: nothing's known");
        assert!(!gone(&snap("working", json!([]), json!([])), "s1"));
        assert!(gone(&snap("stopped", json!([]), json!([])), "s1"));
        assert!(gone(&snap("working", json!([]), json!([])), "other"), "no record of it");
    }

    #[test]
    fn typing_to_the_followed_session_ends_following_it_but_talking_doesnt() {
        let f = Follow { sid: "s1".into(), seen: 200, until: u64::MAX };
        let said = json!([{ "role": "you", "text": "hi\n\n(Voice: I'm listening, not reading.)", "at_ms": 250 }]);
        assert!(!typed_since(&snap("working", said, json!([])), &f));
        let typed = json!([{ "role": "you", "text": "fix the scroll", "at_ms": 250 }]);
        assert!(typed_since(&snap("working", typed, json!([])), &f));
        let before = json!([{ "role": "you", "text": "old typing", "at_ms": 150 }]);
        assert!(!typed_since(&snap("waiting", before, json!([])), &f), "only since its last told reply");
        // Its hook added the ask, so the message is your words alone: known as voice from what was said.
        let d = said_dir_for("follow");
        remember_in(&d, "open a new tab", 240);
        let bare = json!([{ "role": "you", "text": "open a new tab", "at_ms": 250 }]);
        assert!(!typed_since_in(&snap("working", bare.clone(), json!([])), &f, &d));
        assert!(typed_since_in(&snap("working", bare, json!([])), &f, &said_dir_for("empty")), "not said: typed");
        let _ = std::fs::remove_dir_all(&d);
        // Allowing a permission it asked for (it can't be done by voice) isn't typing to it.
        let allowed = json!([{ "role": "you", "text": "allowed", "at_ms": 250, "decision": true }]);
        assert!(!typed_since(&snap("working", allowed, json!([])), &f));
    }

    #[test]
    fn sessions_are_followed_each_on_its_own_and_once() {
        let mut follows = Vec::new();
        follow_on(&mut follows, "a", 100);
        follow_on(&mut follows, "b", 200);
        follow_on(&mut follows, "a", 300);
        let got: Vec<(&str, u64)> = follows.iter().map(|f| (f.sid.as_str(), f.seen)).collect();
        assert_eq!(got, vec![("b", 200), ("a", 300)], "talking to b keeps a; a again replaces its own");
    }

    #[test]
    fn its_answer_is_the_agents_message_after_what_you_said_once_the_turn_is_over() {
        let t = Turn { sid: "s1".into(), after: 100 };
        let old = json!([{ "role": "agent", "text": "before", "at_ms": 100 }]);
        assert!(outcome(&snap("working", old.clone(), json!([])), &t).is_none(), "still working");
        assert!(outcome(&snap("waiting", old, json!([])), &t).is_none(), "nothing new yet");
        let new = json!([{ "role": "agent", "text": "before", "at_ms": 100 }, { "role": "you", "text": "hi", "at_ms": 150 }, { "role": "agent", "text": "Done.", "at_ms": 200 }]);
        assert_eq!(outcome(&snap("waiting", new, json!([])), &t), Some(json!({ "answer": "Done." })));
    }

    #[test]
    fn a_question_meanwhile_is_said_and_a_stop_ends_it_with_nothing() {
        let t = Turn { sid: "s1".into(), after: 100 };
        let q = json!([{ "session_id": "s1", "kind": "question", "tool_input": { "questions": [{ "question": "Which branch?" }] } }]);
        assert_eq!(outcome(&snap("deciding", json!([]), q), &t), Some(json!({ "answer": "It's asking: Which branch?" })));
        let stop = json!([{ "session_id": "s1", "kind": "waiting", "interrupted": true, "created_ms": 150 }]);
        assert_eq!(outcome(&snap("waiting", json!([]), stop), &t), Some(json!({ "stopped": true })));
        let old_stop = json!([{ "session_id": "s1", "kind": "waiting", "interrupted": true, "created_ms": 50 }]);
        assert!(outcome(&snap("waiting", json!([]), old_stop), &t).is_none(), "a stop from before isn't this turn's");
    }

    #[test]
    fn it_talks_to_the_open_chat_else_the_session_that_moved_last() {
        let s = json!({ "sessions": [
            { "session_id": "a", "project": "cue", "state": "waiting", "since_ms": 10, "name": "voice" },
            { "session_id": "b", "project": "site", "state": "working", "since_ms": 20 },
            { "session_id": "c", "project": "old", "state": "stopped", "since_ms": 30 },
        ] });
        assert_eq!(target(&s, Some("a"))["sid"], "a");
        assert_eq!(target(&s, Some("a"))["name"], "cue · voice");
        assert_eq!(target(&s, None)["sid"], "b", "the latest running one, not the stopped one");
        assert_eq!(target(&json!({ "sessions": [] }), None), Value::Null);
    }
}
