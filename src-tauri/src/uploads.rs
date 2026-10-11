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

/// An image's type from its file name ("" when it isn't one Cue takes).
pub fn mime_of(path: &str) -> &'static str {
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "",
    }
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

/// The images where a session on `machine` can open them (empty: this Mac, where they already are).
/// Each is copied over the machine's connection to ~/.cue/uploads there, and its path there is what's
/// typed. Copies older than a week are cleared there on the way, as they are here. `progress` hears
/// the bytes copied so far of them all, and the total (a large screenshot over a slow link takes a while).
pub fn on_machine(machine: &str, saved: &[Saved], progress: &dyn Fn(u64, u64)) -> Result<Vec<Saved>, String> {
    if machine.is_empty() || saved.is_empty() {
        return Ok(saved.to_vec());
    }
    let host = crate::machines::get(machine).ok_or(format!("{machine} isn't one of your machines any more"))?.host;
    let files = saved.iter().map(|s| std::fs::read(&s.path).map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()?;
    let total: u64 = files.iter().map(|b| b.len() as u64).sum();
    let mut before = 0u64;
    progress(0, total);
    let mut out = vec![];
    for (s, bytes) in saved.iter().zip(&files) {
        let name = std::path::Path::new(&s.path).file_name().and_then(|n| n.to_str()).ok_or("an image with no name")?;
        // Written under a temporary name, then renamed: a copy cut short is never typed as the image.
        let script = format!(
            "d=\"$HOME/.cue/uploads\"; mkdir -p \"$d\" && find \"$d\" -type f -mtime +{KEEP_DAYS} -delete 2>/dev/null; cat >\"$d\"/{n}.part && mv \"$d\"/{n}.part \"$d\"/{n} && echo \"$d\"/{n}",
            n = crate::focus::shq(name)
        );
        let at = crate::machines::run_with_bytes(&host, &script, bytes, &|n| progress(before + n, total))
            .map_err(|e| if e.starts_with("Can't connect") { e } else { format!("couldn't copy the image to {}: {e}", crate::machines::display_name(machine)) })?;
        before += bytes.len() as u64;
        let path = at.trim().to_string();
        if path.is_empty() {
            return Err(format!("couldn't copy the image to {}", crate::machines::display_name(machine)));
        }
        out.push(Saved { path, mime: s.mime.clone() });
    }
    Ok(out)
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

    /// On a real machine, by hand: CUE_HOME=<a Cue folder listing it> CUE_MACHINE=name
    /// cargo test -q images_reach_a_machine -- --ignored --nocapture. The copy there is byte for byte the image.
    #[test]
    #[ignore]
    fn images_reach_a_machine() {
        let Ok(m) = std::env::var("CUE_MACHINE") else { return };
        let bytes: Vec<u8> = (0..=255u8).cycle().take(300_000).collect();
        let local = std::env::temp_dir().join(format!("cue-img {}.png", std::process::id()));
        std::fs::write(&local, &bytes).unwrap();
        let saved = vec![Saved { path: local.to_string_lossy().into(), mime: "image/png".into() }];
        let there = on_machine(&m, &saved, &|_, _| {}).unwrap();
        println!("copied to {}", there[0].path);
        assert!(there[0].path.ends_with(&format!("/.cue/uploads/cue-img {}.png", std::process::id())));
        let host = crate::machines::get(&m).unwrap().host;
        let sum = |out: &str| out.split_whitespace().next().unwrap_or("").to_string();
        let remote = crate::machines::run(&host, &format!("cksum <{}", crate::focus::shq(&there[0].path))).unwrap();
        let mine = std::process::Command::new("cksum").stdin(std::fs::File::open(&local).unwrap()).output().unwrap();
        assert_eq!(sum(&remote), sum(&String::from_utf8_lossy(&mine.stdout)), "the same bytes");
        assert_eq!(on_machine("", &saved, &|_, _| {}).unwrap()[0].path, saved[0].path, "on this Mac: where it already is");
        let _ = crate::machines::run(&host, &format!("rm -f {}", crate::focus::shq(&there[0].path)));
        std::fs::remove_file(&local).ok();
    }
}
