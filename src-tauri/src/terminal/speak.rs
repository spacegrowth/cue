//! Listen's natural voices: cue-speak (speak/, the Swift helper shipped next to Cue's binary: Kokoro through
//! FluidAudio, on this Mac). The window sends it JSON lines (speak_send) and gets each of its lines back as a
//! "speak" event; it's started on the first line and again after it quits (it does every 100 sentences) or
//! crashes, which comes as {"gone": true}.

use crate::locking::lock;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// The running helper's stdin.
#[derive(Default)]
pub struct Speak(Mutex<Option<ChildStdin>>);

fn helper() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let p = exe.parent().ok_or("no app folder")?.join("cue-speak");
    if p.exists() { Ok(p) } else { Err("natural voices aren't in this build of Cue".into()) }
}

/// One line to cue-speak, starting it first when it isn't running.
pub fn send(app: AppHandle, s: &Speak, line: &Value) -> Result<(), String> {
    let mut slot = lock(&s.0, "speak stdin");
    if slot.is_none() {
        let mut child = Command::new(helper()?).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
        *slot = child.stdin.take();
        let out = child.stdout.take().ok_or("no output from the voices helper")?;
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    let _ = app.emit("speak", v);
                }
            }
            let _ = child.wait();
            lock(&app.state::<Speak>().0, "speak stdin").take();
            let _ = app.emit("speak", serde_json::json!({ "gone": true }));
        });
    }
    let to = slot.as_mut().ok_or("the voices helper isn't running")?;
    if writeln!(to, "{line}").and_then(|_| to.flush()).is_err() {
        slot.take();   // it went: the next line starts it again
        return Err("the voices helper stopped".into());
    }
    Ok(())
}
