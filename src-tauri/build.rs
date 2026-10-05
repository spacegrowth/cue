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
    // The optional add-on (`ext/`, built in with the `ext` feature): rebuild when it changes.
    println!("cargo:rerun-if-changed=ext");
    tauri_build::build()
}
