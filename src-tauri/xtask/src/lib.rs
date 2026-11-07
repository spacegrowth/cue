//! Cue's source inventory and lints: `xtask inventory` writes docs/dev/inventory.md and
//! docs/dev/lock-graph.dot, `xtask check` runs the rules in docs/dev/lint-allow.toml, `xtask sizes`
//! prints the long functions and files, `xtask soak-report` judges soak runs (soak/). `cargo test` in src-tauri runs the check and the inventory's
//! drift test (src/locking/lint.rs).
//!
//! parse (files → module tree → index → resolution) → walk (each function's events, with the locks it
//! holds) → graph (summaries to a fixpoint, lock edges, external calls under the store) → check / report.

pub mod allow;
pub mod check;
pub mod graph;
pub mod parse;
pub mod report;
pub mod sizes;
pub mod soak;
pub mod walk;

use std::path::Path;

/// Where things live, from the repo root.
pub const INVENTORY: &str = "docs/dev/inventory.md";
pub const LOCK_GRAPH: &str = "docs/dev/lock-graph.dot";
pub const ALLOW: &str = "docs/dev/lint-allow.toml";
const SRC: &str = "src-tauri/src";

/// Everything one run produces.
pub struct Outcome {
    pub check: check::CheckReport,
    pub inventory: String,
    pub lock_graph: String,
    pub sizes: sizes::Sizes,
}

/// The analysis of the repo at `repo`, with the allow-list at `allow`.
pub fn run(repo: &Path, allow: &Path) -> Result<Outcome, String> {
    let allow = allow::Allow::load(allow)?;
    let src = repo.join(SRC);
    let rust = parse::files::parse_rust(&src)?;
    let krate = parse::Crate::load(&src, &rust)?;
    let ix = parse::index::Index::build(&krate);
    let model = graph::analyze(&ix, &krate.missing, &allow.bare_locks(), &allow.store.lock);
    let tmux = check::tmux::literal_tmux(&rust, &allow.tmux_seam);
    let sizes = sizes::measure(repo, &rust)?;
    let check = check::check(&model, &allow, tmux, &sizes);
    let inventory = report::inventory(&model, &sizes, &check);
    let lock_graph = report::lock_graph_dot(&model);
    Ok(Outcome { check, inventory, lock_graph, sizes })
}
