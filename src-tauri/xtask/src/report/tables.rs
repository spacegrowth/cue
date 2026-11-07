//! The inventory's sections, one function each.

use crate::check::CheckReport;
use crate::graph::{Lines, Model};
use crate::sizes::Sizes;
use crate::walk::events::{each, Effect, Ev, Frame};
use std::collections::{BTreeMap, BTreeSet};

/// Markdown table cells can't hold a `|`.
fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

fn table(head: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> String {
    let mut out = format!("| {} |\n|{}\n", head.join(" | "), "---|".repeat(head.len()));
    for r in rows {
        out += &format!("| {} |\n", r.iter().map(|c| cell(c)).collect::<Vec<_>>().join(" | "));
    }
    out
}

/// Every event of every function, with the function's index.
fn all_events(model: &Model) -> Vec<(usize, Ev)> {
    let mut out = vec![];
    for (id, facts) in model.facts.iter().enumerate() {
        each(&facts.events, &mut |ev| out.push((id, ev.clone())));
    }
    out
}

pub fn summary(model: &Model) -> String {
    let events = all_events(model);
    let count = |pred: &dyn Fn(&Ev) -> bool| events.iter().filter(|(_, e)| pred(e)).count();
    let effect = |k: Effect| count(&|e| matches!(e, Ev::Effect { effect, .. } if *effect == k));
    let commands = model.fns.iter().filter(|f| f.is_command).count();
    let sync = model.fns.iter().filter(|f| f.is_command && !f.is_async).count();
    let rows = [
        ("`#[tauri::command]`", format!("{commands} ({sync} sync)")),
        ("Lock declarations (static or field)", model.locks.len().to_string()),
        ("Lock acquisitions", model.facts.iter().map(|f| f.lock_sites.len()).sum::<usize>().to_string()),
        ("Lock-graph edges", model.edges.len().to_string()),
        ("Spawn sites", count(&|e| matches!(e, Ev::Frame { frame: Frame::Spawn(_), .. })).to_string()),
        ("External calls: fs / db / process / notify / login / socket", Effect::ALL.map(|k| effect(k).to_string()).join(" / ")),
        ("External calls reachable under the hub's store", model.store_io.len().to_string()),
    ];
    let missing = if model.missing_modules.is_empty() { String::new() } else { format!("\nModules declared but not in this checkout (not analysed): {}.\n", model.missing_modules.join(", ")) };
    format!("## Summary\n\nHow many calls the analysis followed, and how many it couldn't that may be crate calls, `xtask check` prints with each run (they change with almost every edit, so they're not kept here).\n\n{}{missing}", table(&["What", "Count"], rows.into_iter().map(|(a, b)| vec![a.to_string(), b])))
}

pub fn commands(model: &Model) -> String {
    let rows = model.fns.iter().enumerate().filter(|(_, f)| f.is_command).map(|(id, f)| {
        let pool = work_handed_off(model, id);
        vec![format!("`{}`", f.name), (if f.is_async { "async" } else { "**sync**" }).to_string(), format!("`{}`", f.item), if pool.is_empty() { "—".into() } else { pool.join(", ") }]
    });
    format!("## Commands\n\nSync commands run on the main thread. \"Hands work to\": the threads or pools its body starts (directly or through a crate helper such as `ipc::with_hub`).\n\n{}", table(&["Command", "Kind", "Item", "Hands work to"], rows))
}

/// What a function's body hands off: spawns, and crate helpers that run a closure on a thread.
fn work_handed_off(model: &Model, id: usize) -> Vec<String> {
    let mut out = BTreeSet::new();
    each(&model.facts[id].events, &mut |ev| {
        let handed = match ev {
            Ev::Frame { frame: Frame::Spawn(label), .. } => Some(label.clone()),
            Ev::Frame { frame: Frame::ClosureArg { target, index }, .. } if model.sums[*target].params.get(index).is_some_and(|u| u.detached.is_some()) => Some(model.fns[*target].path.clone()),
            _ => None,
        };
        out.extend(handed);
    });
    out.into_iter().collect()
}

pub fn locks(model: &Model) -> String {
    let mut sites: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &model.facts {
        for s in &f.lock_sites {
            *sites.entry(s.name.as_str()).or_default() += 1;
        }
    }
    let rows = model.locks.iter().map(|l| {
        let names: Vec<String> = l.names.iter().map(|n| format!("\"{n}\" ({} sites)", sites.get(n.as_str()).unwrap_or(&0))).collect();
        vec![format!("`{}`", l.item), format!("`{}`", l.ty.replace(' ', "")), if names.is_empty() { "—".into() } else { names.join(", ") }]
    });
    let named: BTreeSet<&str> = model.locks.iter().flat_map(|l| l.names.iter().map(String::as_str)).collect();
    let loose: Vec<String> = sites.iter().filter(|(n, _)| !named.contains(**n)).map(|(n, c)| format!("\"{n}\" ({c} sites)")).collect();
    format!(
        "## Locks\n\nEach declaration with the names `locking::lock`/`read`/`write` give it (matched by the identifier in the call; \"—\": a once-cell, or a lock reached through a helper the match can't see). Sites include calls of a function that hands out its guard (`Hub::st`).\n\n{}\nLock names not matched to a declaration: {}.\n",
        table(&["Declared", "Type", "Locked as"], rows),
        if loose.is_empty() { "none".into() } else { loose.join(", ") }
    )
}

pub fn lock_graph(model: &Model) -> String {
    let rows = model.edges.iter().map(|((a, b), w)| vec![format!("\"{a}\""), format!("\"{b}\""), (if model.cyclic.contains(&(a.clone(), b.clone())) { "**cycle**" } else { "" }).to_string(), model.lock_chain(w, b, Lines::Left)]);
    format!("## Lock graph\n\nAn edge A → B: B is taken (here or in something called) while A is held. Also in `lock-graph.dot`.\n\n{}", table(&["Held", "Taken", "", "First seen"], rows))
}

pub fn store_io(model: &Model, check: &CheckReport) -> String {
    let rows = model.store_io.iter().map(|(k, (detail, w))| {
        let site = model.fns.iter().position(|f| f.path == k.site).expect("a site is a function");
        vec![format!("`{}`", k.holder), k.effect.name().to_string(), format!("`{}`", k.site), detail.clone(), model.effect_chain(w, k.effect, site, Lines::Left)]
    });
    format!(
        "## External calls under the hub's store\n\nWhat the store's holders reach while they hold it ({} allowed for now in `lint-allow.toml`, for Phase 2 to burn down).\n\n{}",
        check.allowed_store_io.len(),
        table(&["Holder", "Kind", "Made in", "Call", "Path"], rows)
    )
}

/// How many times each function does each thing (by its item, so no line numbers), sorted.
fn tally(model: &Model, what: impl Fn(&Ev) -> Option<(String, String)>) -> BTreeMap<(String, String, String), usize> {
    let mut out = BTreeMap::new();
    for (id, ev) in all_events(model) {
        if let Some((kind, detail)) = what(&ev) {
            *out.entry((kind, model.fns[id].item.clone(), detail)).or_default() += 1;
        }
    }
    out
}

pub fn spawns(model: &Model) -> String {
    let spawns = tally(model, |ev| match ev {
        Ev::Frame { frame: Frame::Spawn(label), .. } => Some((String::new(), label.clone())),
        _ => None,
    });
    let rows = spawns.into_iter().map(|((_, item, label), n)| vec![format!("`{item}`"), label, n.to_string()]);
    format!("## Spawn sites\n\n{}", table(&["In", "Spawn", "Times"], rows))
}

pub fn effects(model: &Model) -> String {
    let calls = tally(model, |ev| match ev {
        Ev::Effect { effect, detail, .. } => Some((effect.name().to_string(), detail.clone())),
        _ => None,
    });
    let mut out = "## External calls\n\n".to_string();
    for kind in Effect::ALL.map(Effect::name) {
        let rows: Vec<Vec<String>> = calls.iter().filter(|((k, _, _), _)| k == kind).map(|((_, item, detail), n)| vec![format!("`{item}`"), detail.clone(), n.to_string()]).collect();
        let total: usize = calls.iter().filter(|((k, _, _), _)| k == kind).map(|(_, n)| n).sum();
        if total > 0 {
            out += &format!("### {kind} ({total})\n\n{}\n", table(&["In", "Call", "Times"], rows));
        }
    }
    out
}

/// The functions and files over the limits, by name: their lengths change with every edit (`xtask sizes`
/// prints them), which ones are over changes only when one crosses a limit.
pub fn sizes(sizes: &Sizes) -> String {
    let mut fns: BTreeMap<(String, String, bool), usize> = BTreeMap::new();
    for f in &sizes.functions {
        *fns.entry((f.file.clone(), f.name.clone(), f.test)).or_default() += 1;
    }
    let fns = fns.into_iter().map(|((file, name, test), n)| vec![format!("`{file}::{name}`"), (if test { "test" } else { "" }).to_string(), if n > 1 { format!("{n} of them") } else { String::new() }]);
    let mut files: Vec<String> = sizes.files.iter().map(|(p, _)| format!("`{p}`")).collect();
    files.sort();
    format!(
        "## Sizes\n\nPaths from the repo root; `cargo run -p xtask -- sizes` prints the lengths. Functions over {} lines (Rust from the parse; `ui/*.js` by a brace-aware scan):\n\n{}\nFiles over {} lines: {}.\n",
        crate::sizes::LONG_FN,
        table(&["Function", "", ""], fns),
        crate::sizes::LONG_FILE,
        files.join(", ")
    )
}
