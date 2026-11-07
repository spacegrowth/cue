//! The leads tests, by area; this file holds what they share.

mod crew;
mod edges;
mod ledger;
mod remote;
mod scan;
mod wake;

use super::*;
use super::{close::*, commands::*, files::PARSED, git::*, ledger::*, index::*, relay::*, remote::*, verify::*, wake::*};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// The tests that fill INDEX / REMOTE take turns.
use super::TEST_LOCK as GLOBALS;
