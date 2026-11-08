//! The host's build: a fingerprint of its own sources (`CUE_HOST_BUILD`), so Cue knows whether a running host has
//! the code it was built with. Only the host's code counts: a change elsewhere in Cue doesn't make a new host build.

fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir("src").map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    files.push("Cargo.toml".into());
    files.sort();
    // FNV-1a over each file's name and bytes: stable across builds and Rust versions (std's hasher isn't promised to be).
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for f in &files {
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        eat(f.to_string_lossy().as_bytes());
        eat(&std::fs::read(f).unwrap_or_default());
    }
    println!("cargo:rustc-env=CUE_HOST_BUILD={h:016x}");
}
