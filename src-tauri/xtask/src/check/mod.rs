//! `xtask check`: the rules, over the model, the sizes and the allow-list. Every failure says what to do; an
//! allow-list entry nothing matches any more is a failure too (the list only shrinks).

mod locks;
mod sizes;
pub mod tmux;

use crate::allow::Allow;
use crate::graph::{Lines, Model};
use std::collections::BTreeSet;

pub struct CheckReport {
    pub failures: Vec<String>,
    /// How much of the code the graph sees.
    pub calls: crate::graph::CallStats,
    /// External calls under the store the allow-list accepts today, for Phase 2 to burn down.
    pub allowed_store_io: Vec<String>,
}

impl CheckReport {
    pub fn render(&self) -> String {
        let mut out = String::new();
        if !self.allowed_store_io.is_empty() {
            out += &format!("{} external call(s) under the store, allowed for now (docs/dev/lint-allow.toml [[store_io]]):\n", self.allowed_store_io.len());
            out += &self.allowed_store_io.iter().map(|s| format!("  {s}\n")).collect::<String>();
        }
        out += &format!("{}\n", self.calls);
        if self.failures.is_empty() {
            out += "xtask check: ok\n";
        } else {
            out += &format!("xtask check: {} failure(s)\n", self.failures.len());
            out += &self.failures.iter().map(|s| format!("- {s}\n")).collect::<String>();
        }
        out
    }
}

pub fn check(model: &Model, allow: &Allow, literal_tmux: Vec<String>, sizes: &crate::sizes::Sizes) -> CheckReport {
    let mut failures = vec![];
    failures.extend(cycles(model));
    failures.extend(lock_order(model, allow));
    let (io_failures, allowed_store_io) = store_io(model, allow);
    failures.extend(io_failures);
    failures.extend(sync_commands(model, allow));
    failures.extend(locks::bare(model, allow));
    failures.extend(locks::shared_names(model));
    failures.extend(literal_tmux.into_iter().map(|t| format!("tmux started by its literal name at {t}: go through terminal::focus::tmux_bin() (tests then use a private tmux server, never yours)")));
    failures.extend(sizes::sizes(sizes, &allow.size_exempt));
    CheckReport { failures, calls: model.call_stats(), allowed_store_io }
}

fn cycles(model: &Model) -> Vec<String> {
    model
        .cyclic
        .iter()
        .map(|(a, b)| {
            let what = if a == b { "taken again while this thread holds it (it would wait on itself for good)".to_string() } else { format!("taken while {a} is held, and {a} is reachable while {b} is held (two threads can deadlock)") };
            format!("lock cycle: {b} {what}: {}", model.lock_chain(&model.edges[&(a.clone(), b.clone())], b, Lines::Shown))
        })
        .collect()
}

fn lock_order(model: &Model, allow: &Allow) -> Vec<String> {
    let allowed: BTreeSet<(&str, &str)> = allow.lock_order.iter().map(|o| (o.from.as_str(), o.to.as_str())).collect();
    let mut out: Vec<String> = model
        .edges
        .iter()
        .filter(|(k, _)| !model.cyclic.contains(*k) && !allowed.contains(&(k.0.as_str(), k.1.as_str())))
        .map(|((a, b), w)| format!("new lock order {a} → {b}: {}. If it's meant, add it to [[lock_order]] in docs/dev/lint-allow.toml with a reason", model.lock_chain(w, b, Lines::Shown)))
        .collect();
    out.extend(allowed.iter().filter(|(a, b)| !model.edges.contains_key(&(a.to_string(), b.to_string()))).map(|(a, b)| format!("[[lock_order]] {a} → {b} no longer happens: remove it from docs/dev/lint-allow.toml")));
    out
}

fn store_io(model: &Model, allow: &Allow) -> (Vec<String>, Vec<String>) {
    let allowed: BTreeSet<(&str, &str, &str)> = allow.store_io.iter().map(|v| (v.holder.as_str(), v.effect.as_str(), v.site.as_str())).collect();
    let mut failures = vec![];
    let mut listed = vec![];
    for (k, (detail, w)) in &model.store_io {
        let line = format!("{} [{}] {} ({detail})", k.holder, k.effect.name(), k.site);
        if allowed.contains(&(k.holder.as_str(), k.effect.name(), k.site.as_str())) {
            listed.push(line);
        } else {
            let site = model.fns.iter().position(|f| f.path == k.site).expect("a site is a function");
            failures.push(format!("{} call while {} is held: {line}: {}", k.effect.name(), model.store_lock, model.effect_chain(w, k.effect, site, Lines::Shown)));
        }
    }
    let seen: BTreeSet<(&str, &str, &str)> = model.store_io.keys().map(|k| (k.holder.as_str(), k.effect.name(), k.site.as_str())).collect();
    failures.extend(allowed.difference(&seen).map(|(h, e, s)| format!("[[store_io]] {h} [{e}] {s} no longer happens: remove it from docs/dev/lint-allow.toml")));
    (failures, listed)
}

/// Commands that aren't `async` run on the main thread: only the ones that must (AppKit) may.
fn sync_commands(model: &Model, allow: &Allow) -> Vec<String> {
    let allowed: BTreeSet<&str> = allow.sync_command.iter().map(|s| s.name.as_str()).collect();
    let sync: BTreeSet<&str> = model.fns.iter().filter(|f| f.is_command && !f.is_async).map(|f| f.name.as_str()).collect();
    let mut out: Vec<String> = sync.difference(&allowed).map(|n| format!("command {n} is sync: Tauri runs it on the main thread. Make it async with its work in ipc::blocking/with_hub, or (AppKit only) add it to [[sync_command]]")).collect();
    out.extend(allowed.difference(&sync).map(|n| format!("[[sync_command]] {n} is no longer a sync command: remove it from docs/dev/lint-allow.toml")));
    out
}
