fn main() {
    // cue-listen, the dictation helper (Swift: Apple's speech API has no Rust binding). Tauri ships it
    // inside Cue.app next to the main binary, from binaries/<name>-<target triple>.
    println!("cargo:rerun-if-changed=listen/main.swift");
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("apple-darwin") {
        std::fs::create_dir_all("binaries").expect("binaries/");
        let out = format!("binaries/cue-listen-{target}");
        let ok = std::process::Command::new("swiftc")
            .args(["-O", "-target", &target.replace("apple-darwin", "apple-macos13.0"), "-o", &out, "listen/main.swift"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "couldn't build the dictation helper (swiftc listen/main.swift)");
        // A universal build (Apple silicon + Intel) builds each half separately; once both exist, the
        // bundle needs them as one.
        let (arm, intel) = ("binaries/cue-listen-aarch64-apple-darwin", "binaries/cue-listen-x86_64-apple-darwin");
        if std::path::Path::new(arm).exists() && std::path::Path::new(intel).exists() {
            let _ = std::process::Command::new("lipo").args(["-create", "-output", "binaries/cue-listen-universal-apple-darwin", arm, intel]).status();
        }
    }
    speak_helper(&target);
    voice_helper(&target);
    // The optional add-on (`ext/`, built in with the `ext` feature): rebuild when it changes.
    println!("cargo:rerun-if-changed=ext");
    tauri_build::build()
}

/// cue-voice, talking to a session from anywhere on the Mac (voice/: NVIDIA's Parakeet through FluidAudio, a Swift
/// package), built as cue-speak is; on Intel a small stand-in that says it isn't there.
fn voice_helper(target: &str) {
    println!("cargo:rerun-if-changed=voice/Package.swift");
    println!("cargo:rerun-if-changed=voice/Sources");
    println!("cargo:rerun-if-changed=voice/intel.swift");
    if !target.contains("apple-darwin") {
        return;
    }
    swift_helper(target, "voice", "cue-voice");
}

/// cue-speak, Listen's natural voices (Kokoro through FluidAudio, a Swift package): built with SwiftPM on Apple
/// silicon; on Intel a small stand-in that says there are none (Kokoro needs Apple silicon).
fn speak_helper(target: &str) {
    println!("cargo:rerun-if-changed=speak/Package.swift");
    println!("cargo:rerun-if-changed=speak/Sources");
    println!("cargo:rerun-if-changed=speak/intel.swift");
    if !target.contains("apple-darwin") {
        return;
    }
    swift_helper(target, "speak", "cue-speak");
}

/// A Swift helper in `dir`: SwiftPM on Apple silicon, `dir`/intel.swift on Intel, into binaries/`name`-<target>.
/// Paths of this Mac are rewritten out of it, as release.sh checks: its sources' through prefix maps, and it's built
/// in the temp folder (SwiftPM writes the build folder into it, to find FluidAudio's resources by).
fn swift_helper(target: &str, dir: &str, name: &str) {
    let out = format!("binaries/{name}-{target}");
    let home = std::env::var("HOME").unwrap_or_default();
    let here = std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default();
    let ok = if target.starts_with("aarch64") {
        let map = |flag: &str, to: &str, from: &str| [flag.to_string(), format!("{from}={to}")];
        let scratch = std::env::temp_dir().join(format!("{name}-build")).display().to_string();
        let mut args: Vec<String> = ["build", "-c", "release", "--package-path", dir, "--scratch-path", &scratch].map(String::from).to_vec();
        for (from, to) in [(here.as_str(), "/cue"), (home.as_str(), "/home")] {
            for [f, v] in [map("-file-prefix-map", to, from), map("-debug-prefix-map", to, from)] {
                args.extend(["-Xswiftc".into(), f, "-Xswiftc".into(), v]);
            }
            args.extend(["-Xcc".into(), format!("-ffile-prefix-map={from}={to}")]);
        }
        std::process::Command::new("swift").args(&args).status().map(|s| s.success()).unwrap_or(false)
            && std::process::Command::new("strip").args(["-x", "-o", &out, &format!("{scratch}/release/{name}")]).status().map(|s| s.success()).unwrap_or(false)
    } else {
        std::process::Command::new("swiftc")
            .args(["-O", "-target", &target.replace("apple-darwin", "apple-macos13.0"), "-o", &out, &format!("{dir}/intel.swift")])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    assert!(ok, "couldn't build {name} ({dir}/)");
    let (arm, intel) = (format!("binaries/{name}-aarch64-apple-darwin"), format!("binaries/{name}-x86_64-apple-darwin"));
    if std::path::Path::new(&arm).exists() && std::path::Path::new(&intel).exists() {
        let _ = std::process::Command::new("lipo").args(["-create", "-output", &format!("binaries/{name}-universal-apple-darwin"), &arm, &intel]).status();
    }
}
