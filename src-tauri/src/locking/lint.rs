//! Cue's source against the xtask analysis (src-tauri/xtask; docs/dev/inventory.md says what it reads and
//! what it can't see): every lock taken through `locking::lock` with a name of its own, no lock cycle, lock
//! orders and external calls under the hub's store only as docs/dev/lint-allow.toml allows, no sync command
//! but the AppKit ones, no tmux started past the test seam; and the inventory kept current.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// One analysis for both tests.
fn outcome() -> &'static xtask::Outcome {
    static OUTCOME: OnceLock<Result<xtask::Outcome, String>> = OnceLock::new();
    OUTCOME.get_or_init(|| xtask::run(&repo(), &repo().join(xtask::ALLOW))).as_ref().unwrap_or_else(|e| panic!("xtask: {e}"))
}

#[test]
fn the_source_passes_the_xtask_check() {
    let check = &outcome().check;
    assert!(check.failures.is_empty(), "{}", check.render());
}

#[test]
fn the_inventory_is_current() {
    for (path, generated) in [(xtask::INVENTORY, &outcome().inventory), (xtask::LOCK_GRAPH, &outcome().lock_graph)] {
        let on_disk = std::fs::read_to_string(repo().join(path)).unwrap_or_default();
        assert!(on_disk == *generated, "{path} is out of date: run `cargo run -p xtask -- inventory` in src-tauri and commit it");
    }
}
