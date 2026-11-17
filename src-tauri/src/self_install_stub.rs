//! A stand-in for self_install.rs, with its interface and none of its work, so the Desk's finishing builds before
//! that file lands. Delete this file once it has, and declare `mod self_install;` in lib.rs.
#![allow(dead_code)]

/// How the latest install is going.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub tag: String,
    pub commit: String,
    /// "building" | "installing" | "starting" | "installed" | "failed" | "rolled_back"
    pub step: String,
    pub log: String,
    pub at: u64,
}

pub fn running_commit() -> Option<String> {
    None
}

pub fn note_running() {}

pub fn has(_repo: &str, _commit: &str) -> bool {
    false
}

pub fn start(_repo: &str, _commit: &str, _tag: &str) -> Result<(), String> {
    Err("Cue can't install itself in this build".into())
}

pub fn status() -> Option<Status> {
    None
}
