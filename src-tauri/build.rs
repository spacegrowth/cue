fn main() {
    // cue-host for other machines, which Cue.app carries (tauri.conf.json resources): scripts/build-host.sh builds
    // it. The folder made empty when it hasn't run (a fresh checkout, the tests), so the app still builds; such a
    // build says it can't host sessions on machines (hosted/machine_host.rs).
    std::fs::create_dir_all("host-builds").expect("host-builds/");
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
    stamp_commit();
    tauri_build::build()
}

/// CUE_COMMIT: the commit this source was checked out at (`git rev-parse HEAD`), empty when it isn't a git tree. A
/// running Cue reads it to say whether a change is in it (self_install.rs). Rebuilt when HEAD moves: HEAD itself
/// (a detached checkout keeps the commit there), the branch it names, and packed-refs, where that branch can be.
/// Edits not committed yet aren't told apart: the self-install builds from a clean copy.
fn stamp_commit() {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let commit = git(&["rev-parse", "HEAD"]);
    println!("cargo:rustc-env=CUE_COMMIT={commit}");
    if commit.is_empty() {
        return;
    }
    let mut watched = vec![git(&["rev-parse", "--git-path", "HEAD"]), git(&["rev-parse", "--git-path", "packed-refs"])];
    let branch = git(&["symbolic-ref", "-q", "HEAD"]);
    if !branch.is_empty() {
        watched.push(git(&["rev-parse", "--git-path", &branch]));
    }
    // A path that doesn't exist would rerun this script on every build: only the ones there are watched.
    for path in watched.into_iter().filter(|p| !p.is_empty() && std::path::Path::new(p).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
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
/// Paths of this Mac are rewritten out of it, as release.sh checks: its sources' through prefix maps, and its
/// build folder is in the temp folder (SwiftPM writes that folder into it, to find FluidAudio's resources by).
fn swift_helper(target: &str, dir: &str, name: &str) {
    let out = format!("binaries/{name}-{target}");
    let ok = if target.starts_with("aarch64") {
        swift_helper_shared(dir, name, &out)
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

/// Helper `name` built with SwiftPM into `out`, through one folder per helper that every checkout of Cue shares
/// (`<temp>/cue-swift/<name>`: SwiftPM's build folder, 1–2 GB, and the helper last built), held locked while used so
/// two builds wait for each other instead of breaking the folder. A helper already built from the same sources is
/// copied, not built again: the path maps leave nothing of the checkout in it. Before, each checkout had a folder
/// of its own, and every copy of Cue a Desk item or a check made left 2–4 GB behind (#377).
fn swift_helper_shared(dir: &str, name: &str, out: &str) -> bool {
    let home = swift_home(name);
    prune_swift_scratch();
    if std::fs::create_dir_all(&home).is_err() {
        return false;
    }
    let Ok(lock) = std::fs::File::create(home.join("lock")) else { return false };
    if lock.lock().is_err() {
        return false;
    }
    let built = home.join(format!("{name}-{:016x}", sources_key(dir)));
    if !built.exists() {
        let scratch = home.join("build");
        let build = || swiftpm_build(dir, &scratch);
        // A build folder the temp folder's cleanup half-emptied fails the build (an xcframework without its
        // Info.plist, say): built again once from nothing before giving up.
        let ok = build() || {
            println!("cargo:warning=building {name} failed; building it again from a clean {}", scratch.display());
            let _ = std::fs::remove_dir_all(&scratch);
            build()
        };
        let part = home.join("part");
        let stripped = ok && std::process::Command::new("strip").arg("-x").arg("-o").arg(&part).arg(scratch.join("release").join(name)).status().is_ok_and(|s| s.success());
        if !stripped || std::fs::rename(&part, &built).is_err() {
            return false;
        }
        // Only the helper just built is kept.
        for e in std::fs::read_dir(&home).into_iter().flatten().flatten() {
            if e.path() != built && e.file_name().to_string_lossy().starts_with(&format!("{name}-")) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    std::fs::copy(&built, out).is_ok()
}

/// The folder helper `name` is built in, shared by every checkout. In the temp folder, not target/: the path ends up
/// in the helper, and target/ is under your home folder, whose path release.sh refuses to ship.
fn swift_home(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join("cue-swift").join(name)
}

/// What helper package `dir` is built from, as one number: its manifest, pinned versions and sources (names and
/// contents). The same number means the same helper.
fn sources_key(dir: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    fn files(at: &std::path::Path, into: &mut Vec<std::path::PathBuf>) {
        if at.is_dir() {
            for e in std::fs::read_dir(at).into_iter().flatten().flatten() {
                files(&e.path(), into);
            }
        } else if at.is_file() {
            into.push(at.to_path_buf());
        }
    }
    let root = std::path::Path::new(dir);
    let mut all = Vec::new();
    for part in ["Package.swift", "Package.resolved", "Sources"] {
        files(&root.join(part), &mut all);
    }
    all.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in all {
        f.strip_prefix(root).unwrap_or(&f).hash(&mut h);
        std::fs::read(&f).unwrap_or_default().hash(&mut h);
    }
    h.finish()
}

/// The build folders each checkout once had of its own (`<temp>/cue-swift-<checkout>`, 1–2 GB each) that no build
/// has written to in a day: removed, since nothing else removes them. One written to within the day may be an older
/// checkout's build under way. The temp folder's cleanup touches the folders as it empties them, so a folder's age
/// is its newest file's.
fn prune_swift_scratch() {
    let day = std::time::Duration::from_secs(24 * 60 * 60);
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_name().to_string_lossy().starts_with("cue-swift-") {
            continue;
        }
        let stale = match newest_file(&path) {
            Some(t) => t.elapsed().map_or(false, |age| age > day),
            None => true,
        };
        if stale {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// When the newest file under `dir` was written (links not followed); None for a folder with no files.
fn newest_file(dir: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::read_dir(dir).ok()?.flatten().filter_map(|e| {
        let kind = e.file_type().ok()?;
        if kind.is_dir() { newest_file(&e.path()) } else if kind.is_file() { e.metadata().ok()?.modified().ok() } else { None }
    }).max()
}

/// `swift build` of the package in `dir`, release, into `scratch`, with this Mac's paths mapped out of it.
fn swiftpm_build(dir: &str, scratch: &std::path::Path) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let here = std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default();
    let map = |flag: &str, to: &str, from: &str| [flag.to_string(), format!("{from}={to}")];
    let mut args: Vec<String> = ["build", "-c", "release", "--package-path", dir, "--scratch-path"].map(String::from).to_vec();
    args.push(scratch.display().to_string());
    for (from, to) in [(here.as_str(), "/cue"), (home.as_str(), "/home")] {
        for [f, v] in [map("-file-prefix-map", to, from), map("-debug-prefix-map", to, from)] {
            args.extend(["-Xswiftc".into(), f, "-Xswiftc".into(), v]);
        }
        args.extend(["-Xcc".into(), format!("-ffile-prefix-map={from}={to}")]);
    }
    std::process::Command::new("swift").args(&args).status().map(|s| s.success()).unwrap_or(false)
}
