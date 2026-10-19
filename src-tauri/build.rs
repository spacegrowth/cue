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
    // The optional add-on (`ext/`, built in with the `ext` feature): rebuild when it changes.
    println!("cargo:rerun-if-changed=ext");
    tauri_build::build()
}

/// cue-speak, Listen's natural voices (Kokoro through FluidAudio, a Swift package): built with SwiftPM on Apple
/// silicon; on Intel a small stand-in that says there are none (Kokoro needs Apple silicon). Paths of this Mac are
/// rewritten out of it, as release.sh checks: its sources' through prefix maps, and it's built in the temp folder
/// (SwiftPM writes the build folder into it, to find FluidAudio's resources by).
fn speak_helper(target: &str) {
    println!("cargo:rerun-if-changed=speak/Package.swift");
    println!("cargo:rerun-if-changed=speak/Sources");
    println!("cargo:rerun-if-changed=speak/intel.swift");
    if !target.contains("apple-darwin") {
        return;
    }
    let out = format!("binaries/cue-speak-{target}");
    let home = std::env::var("HOME").unwrap_or_default();
    let here = std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default();
    let ok = if target.starts_with("aarch64") {
        let map = |flag: &str, to: &str, from: &str| [flag.to_string(), format!("{from}={to}")];
        let scratch = std::env::temp_dir().join("cue-speak-build").display().to_string();
        let mut args: Vec<String> = ["build", "-c", "release", "--package-path", "speak", "--scratch-path", &scratch].map(String::from).to_vec();
        for (from, to) in [(here.as_str(), "/cue"), (home.as_str(), "/home")] {
            for [f, v] in [map("-file-prefix-map", to, from), map("-debug-prefix-map", to, from)] {
                args.extend(["-Xswiftc".into(), f, "-Xswiftc".into(), v]);
            }
            args.extend(["-Xcc".into(), format!("-ffile-prefix-map={from}={to}")]);
        }
        std::process::Command::new("swift").args(&args).status().map(|s| s.success()).unwrap_or(false)
            && std::process::Command::new("strip").args(["-x", "-o", &out, &format!("{scratch}/release/cue-speak")]).status().map(|s| s.success()).unwrap_or(false)
    } else {
        std::process::Command::new("swiftc")
            .args(["-O", "-target", &target.replace("apple-darwin", "apple-macos13.0"), "-o", &out, "speak/intel.swift"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    assert!(ok, "couldn't build the voices helper (speak/)");
    let (arm, intel) = ("binaries/cue-speak-aarch64-apple-darwin", "binaries/cue-speak-x86_64-apple-darwin");
    if std::path::Path::new(arm).exists() && std::path::Path::new(intel).exists() {
        let _ = std::process::Command::new("lipo").args(["-create", "-output", "binaries/cue-speak-universal-apple-darwin", arm, intel]).status();
    }
}
