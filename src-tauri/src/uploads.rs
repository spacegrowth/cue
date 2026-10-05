//! Images you attach to a reply: saved under uploads/ in Cue's data folder so they can be handed to the agent —
//! as a file path typed into Claude/Codex (Claude Code attaches pasted image paths), or read back
//! and sent as real image content to Pi.

use crate::model::{now_ms, Upload};
use base64::Engine;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Saved {
    pub path: String,
    pub mime: String,
}

const KEEP_DAYS: u64 = 7;

fn dir() -> std::path::PathBuf {
    crate::server::cue_dir().join("uploads")
}

fn ext(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
}

/// Write each image to disk; returns where they landed. Also clears uploads older than a week.
pub fn save(images: &[Upload]) -> Result<Vec<Saved>, String> {
    if images.is_empty() {
        return Ok(vec![]);
    }
    let d = dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    prune(&d);
    let stamp = now_ms();
    images
        .iter()
        .enumerate()
        .map(|(n, im)| {
            if !im.mime.starts_with("image/") {
                return Err(format!("{} isn't an image", im.name));
            }
            let raw = im.data.split_once(',').map(|(_, b)| b).unwrap_or(&im.data); // accept data: URLs
            let bytes = base64::engine::general_purpose::STANDARD.decode(raw.trim()).map_err(|e| format!("{}: {e}", im.name))?;
            let path = d.join(format!("{stamp}-{n}.{}", ext(&im.mime)));
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            Ok(Saved { path: path.to_string_lossy().to_string(), mime: im.mime.clone() })
        })
        .collect()
}

fn prune(d: &std::path::Path) {
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(KEEP_DAYS * 86_400);
    if let Ok(entries) = std::fs::read_dir(d) {
        for e in entries.flatten() {
            if e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < cutoff) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// The image on the macOS clipboard, as a data: URL — for pastes the web view doesn't hand over.
/// Tries PNG, then TIFF (converted with sips). None when the clipboard holds no image.
pub fn clipboard_image() -> Option<String> {
    let tmp = std::env::temp_dir().join(format!("cue-clip-{}", now_ms()));
    let (png, tiff) = (tmp.with_extension("png"), tmp.with_extension("tiff"));
    let grab = |class: &str, out: &std::path::Path| {
        let script = format!(
            "set f to open for access (POSIX file \"{}\") with write permission\n\
             try\n  write (the clipboard as {class}) to f\n  close access f\non error\n  close access f\n  error \"no image\"\nend try",
            out.display()
        );
        std::process::Command::new("osascript").arg("-e").arg(script).output().map(|o| o.status.success()).unwrap_or(false)
    };
    let ok = grab("«class PNGf»", &png)
        || (grab("«class TIFF»", &tiff)
            && std::process::Command::new("sips").args(["-s", "format", "png"]).arg(&tiff).arg("--out").arg(&png).output().map(|o| o.status.success()).unwrap_or(false));
    let bytes = if ok { std::fs::read(&png).ok() } else { None };
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&tiff);
    bytes.filter(|b| !b.is_empty()).map(|b| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b)))
}

/// The text typed into a terminal: your words, then the image paths (space-separated).
pub fn with_paths(text: &str, saved: &[Saved]) -> String {
    // Spaces escaped the way a file dragged into a terminal is ("Application\ Support"), so the
    // agent reads one path, not two words.
    let paths: Vec<String> = saved.iter().map(|s| escape_path(&s.path)).collect();
    // A leading space before "/" is how you send a message that starts with a slash: without it the
    // agent runs it as a command ("/reomte …" → unknown command). Keep exactly one.
    let words = text.trim();
    let words = if words.starts_with('/') && text.starts_with(char::is_whitespace) { format!(" {words}") } else { words.to_string() };
    [words.as_str(), &paths.join(" ")].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ")
}

/// A path as typed into a terminal: spaces backslash-escaped.
pub fn escape_path(p: &str) -> String {
    p.replace(' ', "\\ ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_images_and_builds_the_typed_text() {
        let _g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-up-{}", std::process::id()));
        std::env::set_var("CUE_HOME", &dir);
        let png = base64::engine::general_purpose::STANDARD.encode([0x89u8, b'P', b'N', b'G']);
        let saved = save(&[Upload { name: "a.png".into(), mime: "image/png".into(), data: format!("data:image/png;base64,{png}") }]).unwrap();
        assert_eq!(std::fs::read(&saved[0].path).unwrap(), vec![0x89, b'P', b'N', b'G']);
        assert_eq!(with_paths("look at this", &saved), format!("look at this {}", saved[0].path));
        assert_eq!(with_paths("", &saved), saved[0].path);
        assert_eq!(with_paths("  /remote is it?  ", &[]), " /remote is it?"); // a message, not a command
        assert_eq!(with_paths("/rename x", &[]), "/rename x"); // a command stays a command
        assert_eq!(with_paths("  hi  ", &[]), "hi");
        assert_eq!(escape_path("/Users/v/Library/Application Support/x.png"), "/Users/v/Library/Application\\ Support/x.png");
        assert!(save(&[Upload { name: "x.txt".into(), mime: "text/plain".into(), data: png }]).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
