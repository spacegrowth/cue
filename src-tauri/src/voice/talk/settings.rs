//! Settings → Voice: the rows for talking to a session, from your picks (voice.json) and how cue-voice says it is.

use super::{prefs, status};
use serde_json::{json, Value};

/// Its rows in Settings → Voice, under Cue's own voice picker: the key, how the listening model is, reading out.
pub fn settings() -> Option<Value> {
    let p = prefs();
    let on = p["talk"] == true;
    let talk = json!({ "title": "Talk to sessions",
        "sub": if on { "Hold right ⇧ in any app and talk; let go to send it." } else { "Off. On: hold right ⇧ in any app, talk, and let go to send it to the chat open in Cue. The first time, Cue asks for the microphone. What you say is written down by Apple's recognizer on this Mac, or by NVIDIA's if you pick it (Listening)." },
        "pick": { "id": "talk", "value": if on { "on" } else { "off" }, "options": [["on", "On"], ["off", "Off"]] } });
    let st = status();
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
    let wake_on = p["wake"] == true;
    let wake = json!({ "title": "Listen for “Hey Cue”",
        "sub": if wake_on { "The mic stays on while the Mac is unlocked; nothing leaves it until you say “Hey Cue”. Off when the screen locks. Say “Hey Cue, ask four ninety-seven…” to pick a session by its name, else it goes to the chat open in Cue. A mark in the menu bar shows it's listening." } else { "Off. On: say “Hey Cue” and what you want, without touching the Mac. The mic stays on while the Mac is unlocked; nothing leaves it until you say “Hey Cue”." },
        "pick": { "id": "wake", "value": if wake_on { "on" } else { "off" }, "options": [["on", "On"], ["off", "Off"]] } });
    let mut rows = vec![talk, wake];
    if wake_on {
        rows.push(json!({ "title": "After an answer", "sub": "It's read out at once, then it listens 6 s for your reply: no “Hey Cue” needed. Nothing said: back to waiting for “Hey Cue”." }));
    }
    rows.extend(json!([
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
    ]).as_array().cloned().unwrap_or_default());
    Some(json!({ "group": "Talking to a session", "tab": "Voice", "rows": rows }))
}
