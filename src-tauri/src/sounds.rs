//! The sound Cue's notifications play, picked under Settings → General.
//!
//! Cue's own three (Pluck, Chime, Rise), and Tada for a turn you asked to be told about, are drawn here, as a few decaying tones, and written once to
//! ~/Library/Sounds, where macOS looks for a notification's sound. Glass and Ping are macOS's own.
//! The sound goes out with the notification, so Focus and Do Not Disturb silence it as before.

/// Every choice, as stored in config.json.
pub const CHOICES: [&str; 6] = ["pluck", "chime", "rise", "glass", "ping", "none"];

/// What a turn you asked to be told about ("Tell me") plays: not one to pick, so it stands apart.
pub const TADA: &str = "tada";

/// The sound you picked; Pluck until you pick one.
pub fn choice() -> String {
    crate::config::load().pointer("/notify/sound").and_then(serde_json::Value::as_str).filter(|c| CHOICES.contains(c)).unwrap_or("pluck").to_string()
}

/// The name a notification asks macOS for: a file in ~/Library/Sounds or one of macOS's own sounds.
/// None: no sound.
pub fn notification_name(choice: &str) -> Option<String> {
    match choice {
        "glass" => Some("Glass".into()),
        "ping" => Some("Ping".into()),
        c if ours(c) => Some(file_name(c)),
        _ => None,
    }
}

fn ours(choice: &str) -> bool {
    matches!(choice, "pluck" | "chime" | "rise" | "tada")
}

/// The file a sound of Cue's is written to: its name and a fingerprint of how it sounds now, "Cue Tada 3f2a91.wav".
/// macOS remembers a notification's sound by its file name and keeps what it first found there (a file changed
/// under the same name then plays as nothing at all), so a sound drawn differently gets a name of its own.
fn file_name(choice: &str) -> String {
    named(choice, &wav(&render(choice)))
}

fn named(choice: &str, wav: &[u8]) -> String {
    format!("{} {:06x}.wav", stem(choice), fingerprint(wav))
}

/// "Cue Tada", for "tada".
fn stem(choice: &str) -> String {
    let mut name = choice.to_string();
    name[..1].make_ascii_uppercase();
    format!("Cue {name}")
}

/// FNV-1a over the file, kept to 24 bits: six hex digits in its name.
fn fingerprint(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5u32, |h, b| (h ^ u32::from(*b)).wrapping_mul(0x0100_0193)) & 0xff_ffff
}

/// An earlier drawing of this sound of Cue's: its name before fingerprints, or with another fingerprint.
/// Any other file in ~/Library/Sounds is never one.
fn stale(file: &str, choice: &str, current: &str) -> bool {
    let stem = stem(choice);
    file != current
        && (file == format!("{stem}.wav")
            || file.strip_prefix(&format!("{stem} ")).and_then(|r| r.strip_suffix(".wav")).is_some_and(|f| f.len() == 6 && f.chars().all(|c| c.is_ascii_hexdigit())))
}

fn sounds_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Sounds"))
}

/// Write Cue's sounds to ~/Library/Sounds, unless they're there already as drawn now.
pub fn install() {
    if let Some(dir) = sounds_dir() {
        install_in(&dir);
    }
}

/// Each sound of Cue's as drawn now, and its earlier drawings gone, so only the one in use is there.
fn install_in(dir: &std::path::Path) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let files: Vec<String> = std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
    for c in CHOICES.into_iter().chain([TADA]).filter(|c| ours(c)) {
        let wav = wav(&render(c));
        let name = named(c, &wav);
        let path = dir.join(&name);
        if std::fs::read(&path).ok().as_deref() != Some(&wav[..]) {
            let _ = std::fs::write(&path, &wav);
        }
        for old in files.iter().filter(|f| stale(f, c, &name)) {
            let _ = std::fs::remove_file(dir.join(old));
        }
    }
}

/// Play a choice once, as you pick it in Settings (afplay, in the background).
pub fn preview(choice: &str) {
    let path = match choice {
        "glass" => std::path::PathBuf::from("/System/Library/Sounds/Glass.aiff"),
        "ping" => std::path::PathBuf::from("/System/Library/Sounds/Ping.aiff"),
        c if ours(c) => {
            install();
            match sounds_dir() {
                Some(d) => d.join(file_name(c)),
                None => return,
            }
        }
        _ => return,
    };
    // One at a time: a new pick stops the one still playing.
    let mut playing = crate::locking::lock(&PLAYING, "PLAYING");
    if let Some(mut old) = playing.take() {
        let _ = old.kill();
        let _ = old.wait();
    }
    *playing = std::process::Command::new("/usr/bin/afplay").arg(&path).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok();
}

/// The preview playing now (afplay), if any.
static PLAYING: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

const RATE: u32 = 44_100;

/// One tone: its pitch (Hz), when it starts and how long it rings (seconds), how loud, and whether it's
/// a triangle wave (warmer, a plucked string) or a sine (a bell).
struct Tone {
    hz: f64,
    at: f64,
    dur: f64,
    gain: f64,
    triangle: bool,
}

fn tones(choice: &str) -> Vec<Tone> {
    let t = |hz, at, dur, gain, triangle| Tone { hz, at, dur, gain, triangle };
    let c5 = 523.25;
    match choice {
        "pluck" => vec![t(c5, 0.0, 0.6, 0.32, true), t(c5 * 2.0, 0.0, 0.25, 0.08, false)],
        "chime" => vec![t(c5 * 2.0, 0.0, 1.4, 0.16, false), t(c5 * 3.0, 0.0, 0.9, 0.06, false), t(c5 * 5.4, 0.0, 0.4, 0.02, false)],
        "rise" => vec![t(c5, 0.0, 0.28, 0.28, true), t(c5 * 1.5, 0.13, 0.5, 0.3, true)],
        // A soft marimba run up the major chord (C E G C), ringing on the top note; each note with a faint
        // high overtone, struck as it starts.
        "tada" => {
            let run = [(c5, 0.0, 0.9, 0.22), (659.25, 0.09, 0.9, 0.22), (783.99, 0.18, 0.9, 0.22), (c5 * 2.0, 0.27, 1.25, 0.26)];
            run.iter().flat_map(|&(hz, at, dur, gain)| [t(hz, at, dur, gain, false), t(hz * 4.0, at, 0.12, 0.03, false)]).collect()
        }
        _ => vec![],
    }
}

/// The samples: each tone rises in 5 ms, then dies away to silence over its length; the whole is
/// scaled so its loudest moment is at 70%.
fn render(choice: &str) -> Vec<i16> {
    let tones = tones(choice);
    let end = tones.iter().map(|t| t.at + t.dur).fold(0.0, f64::max) + 0.05;
    let n = (end * RATE as f64) as usize;
    let mut out = vec![0f64; n];
    for t in &tones {
        let (start, len) = ((t.at * RATE as f64) as usize, (t.dur * RATE as f64) as usize);
        let attack = 0.005 * RATE as f64;
        for i in 0..len.min(n.saturating_sub(start)) {
            let x = i as f64;
            let env = if x < attack { x / attack } else { 0.0001f64.powf((x - attack) / (len as f64 - attack)) };
            let phase = (t.hz * x / RATE as f64).fract();
            let wave = if t.triangle { 1.0 - 4.0 * (phase - 0.5).abs() } else { (phase * std::f64::consts::TAU).sin() };
            out[start + i] += wave * env * t.gain;
        }
    }
    let peak = out.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-9);
    out.iter().map(|v| (v / peak * 0.7 * i16::MAX as f64) as i16).collect()
}

/// 16-bit mono PCM in a WAV file.
fn wav(samples: &[i16]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_of_cues_sounds_is_a_short_wav_that_starts_and_ends_quietly() {
        for c in ["pluck", "chime", "rise", "tada"] {
            let s = render(c);
            let secs = s.len() as f64 / RATE as f64;
            assert!(secs > 0.3 && secs < 1.6, "{c}: {secs}s");
            let peak = s.iter().map(|v| v.unsigned_abs()).max().unwrap();
            assert!(peak > 20_000, "{c} is loud enough");
            assert!(s[0].unsigned_abs() < 500 && s[s.len() - 1].unsigned_abs() < 500, "{c} has no click at either end");
            let w = wav(&s);
            assert_eq!(&w[..4], b"RIFF");
            assert_eq!(w.len(), 44 + s.len() * 2);
        }
    }

    #[test]
    fn a_choice_names_the_sound_macos_plays() {
        let pluck = notification_name("pluck").unwrap();
        assert!(pluck.starts_with("Cue Pluck ") && pluck.ends_with(".wav") && pluck.len() == "Cue Pluck 123456.wav".len(), "{pluck}");
        assert_ne!(file_name("pluck"), file_name("tada"), "each sound has a name of its own");
        assert_eq!(notification_name("glass").as_deref(), Some("Glass"));
        assert_eq!(notification_name("none"), None);
    }

    #[test]
    fn installing_leaves_only_the_sound_as_drawn_now_and_never_touches_other_files() {
        let dir = std::env::temp_dir().join(format!("cue-sounds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for old in ["Cue Tada.wav", "Cue Tada 000000.wav", "Cue Tada notes.wav", "Tada.wav", "Cue Tadas 123456.wav"] {
            std::fs::write(dir.join(old), b"old").unwrap();
        }
        install_in(&dir);
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        left.sort();
        let mut want = vec!["Cue Tada notes.wav".to_string(), "Tada.wav".to_string(), "Cue Tadas 123456.wav".to_string()];
        want.extend(["pluck", "chime", "rise", "tada"].map(file_name));
        want.sort();
        assert_eq!(left, want);
        assert_eq!(std::fs::read(dir.join(file_name("tada"))).unwrap(), wav(&render("tada")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
