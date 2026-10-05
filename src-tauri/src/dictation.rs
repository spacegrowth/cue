//! Dictation: the mic button in a text box. Runs cue-listen (the Swift helper shipped next to Cue's
//! binary, using Apple's on-device speech recognition) and forwards each update to the window as a
//! "dictation" event: {"text", "final"}, {"listening": true}, {"error"}, and {"done": true} at the end.

use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// The running helper's stdin: closing it tells cue-listen to stop and send its final text.
#[derive(Default)]
pub struct Dictation(Mutex<Option<ChildStdin>>);

fn helper() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let p = exe.parent().ok_or("no app folder")?.join("cue-listen");
    if p.exists() { Ok(p) } else { Err("dictation isn't installed with this build of Cue".into()) }
}

pub fn start(app: AppHandle, d: &Dictation) -> Result<(), String> {
    let mut slot = d.0.lock().unwrap();
    if slot.is_some() {
        return Ok(()); // already listening
    }
    let mut child: Child = Command::new(helper()?).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
    *slot = child.stdin.take();
    let out = child.stdout.take().ok_or("no output from the dictation helper")?;
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                let _ = app.emit("dictation", v);
            }
        }
        let _ = child.wait();
        app.state::<Dictation>().0.lock().unwrap().take(); // it may have stopped on its own
        let _ = app.emit("dictation", serde_json::json!({"done": true}));
    });
    Ok(())
}

/// Stop listening; the final text still arrives as an event, then {"done": true}.
pub fn stop(d: &Dictation) {
    d.0.lock().unwrap().take(); // dropping stdin closes it
}
