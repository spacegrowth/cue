//! How Cue runs the programs that reach a terminal (tmux, osascript, kitten, wezterm, ps): found where an app
//! opened from Finder can't see them (`bin`), and each given at most a few seconds (`capped`).

use std::process::Command;

/// Find a CLI tool. An app opened from Finder gets a bare PATH (/usr/bin:/bin:…), so Homebrew's
/// tmux and friends must be looked up explicitly. In tests, tmux only ever on the tests' own server, or
/// the stand-in a test put on its thread for any program (terminal::test_tmux).
pub(super) fn bin(name: &str) -> String {
    #[cfg(test)]
    {
        if let Some(stand_in) = crate::terminal::test_tmux::standing_in(name) {
            return stand_in;
        }
        if name == "tmux" {
            crate::terminal::test_tmux::check();
        }
    }
    find_bin(name)
}

pub(crate) fn find_bin(name: &str) -> String {
    let candidates = [
        format!("/opt/homebrew/bin/{name}"),
        format!("/usr/local/bin/{name}"),
        format!("/Applications/WezTerm.app/Contents/MacOS/{name}"),
        format!("/Applications/kitty.app/Contents/MacOS/{name}"),
    ];
    candidates.into_iter().find(|p| std::path::Path::new(p).exists()).unwrap_or_else(|| name.to_string())
}

/// A command's output, given at most `cap`: past that it's killed and this is an error. What Cue runs
/// to type into a terminal or read it (osascript, tmux, kitten, wezterm) answers in well under a second;
/// one that doesn't (iTerm busy, a tmux server not answering) would otherwise hold a send for good.
pub(crate) fn capped(cmd: &mut Command, cap: std::time::Duration) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    if cmd.get_program().to_string_lossy().ends_with("tmux") {
        crate::chaos::pause(crate::chaos::Knob::TmuxDelayMs);
    }
    let mut child = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()?;
    // Read as it runs: a full pipe would stop it before it could exit.
    let drain = |p: Option<Box<dyn Read + Send>>| std::thread::spawn(move || { let mut b = vec![]; if let Some(mut p) = p { let _ = p.read_to_end(&mut b); } b });
    let out = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let until = std::time::Instant::now() + cap;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if std::time::Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return Err(timed_out(cmd, cap));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    Ok(std::process::Output { status, stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() })
}

/// "tmux didn't answer in 10s": what `capped` says of a command it cut off.
fn timed_out(cmd: &Command, cap: std::time::Duration) -> std::io::Error {
    let name = cmd.get_program().to_string_lossy().rsplit('/').next().unwrap_or("").to_string();
    let took = if cap.subsec_millis() == 0 { format!("{}s", cap.as_secs()) } else { format!("{}ms", cap.as_millis()) };
    std::io::Error::new(std::io::ErrorKind::TimedOut, format!("{name} didn't answer in {took}"))
}

/// Whether program `cmd` ran and succeeded, within its cap.
pub(super) fn run(cmd: &str, args: &[&str]) -> bool {
    let cap = if cmd == "tmux" { tmux_cap() } else { CAP };
    capped(Command::new(bin(cmd)).args(args), cap).map(|o| o.status.success()).unwrap_or(false)
}

/// How long a command Cue runs to type into a terminal or read it may take before it's cut off.
pub(super) const CAP: std::time::Duration = std::time::Duration::from_secs(10);

/// tmux's cap: CAP, or in a test the one it put on its thread (test_tmux::cap_tmux).
pub(super) fn tmux_cap() -> std::time::Duration {
    #[cfg(test)]
    if let Some(cap) = crate::terminal::test_tmux::tmux_cap() {
        return cap;
    }
    CAP
}

/// `s` inside an AppleScript string literal.
pub(super) fn osa(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "")
}

pub(super) fn run_osascript(script: &str) -> Result<String, String> {
    let out = capped(Command::new("osascript").arg("-e").arg(script), 2 * CAP).map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub(super) fn app_running(name: &str) -> bool {
    Command::new("pgrep").arg("-xq").arg(name).status().map(|s| s.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_that_hangs_is_cut_off() {
        let t = std::time::Instant::now();
        let e = capped(Command::new("/bin/sleep").arg("30"), std::time::Duration::from_secs(1)).unwrap_err();
        assert!(t.elapsed() < std::time::Duration::from_secs(3), "{:?}", t.elapsed());
        assert_eq!(e.kind(), std::io::ErrorKind::TimedOut);
        assert!(e.to_string().contains("sleep didn't answer in 1s"), "{e}");
        // One that answers: all of its output, more than a pipe holds, and its status.
        let o = capped(Command::new("/bin/sh").args(["-c", "head -c 200000 /dev/zero | tr '\\0' x; exit 3"]), CAP).unwrap();
        assert_eq!(o.stdout.len(), 200_000);
        assert_eq!(o.status.code(), Some(3));
    }
}
