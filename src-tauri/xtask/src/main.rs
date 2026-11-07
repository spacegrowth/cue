//! `cargo run -p xtask -- <inventory|check|sizes> [--repo <path>]`, or `soak-report <run.json>…` (in
//! src-tauri). See lib.rs.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = match args.iter().position(|a| a == "--repo") {
        Some(i) => match args.get(i + 1) {
            Some(p) => PathBuf::from(p),
            None => return usage(),
        },
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
    };
    let Some(command) = args.first() else { return usage() };
    if command == "soak-report" {
        return soak_report(&args[1..]);
    }
    let outcome = match xtask::run(&repo, &repo.join(xtask::ALLOW)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("xtask: {e}");
            return ExitCode::FAILURE;
        }
    };
    match command.as_str() {
        "inventory" => {
            println!("{}", outcome.check.calls);
            write(&repo, &outcome)
        }
        "check" => {
            print!("{}", outcome.check.render());
            if outcome.check.failures.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        "sizes" => {
            for f in &outcome.sizes.functions {
                println!("{:>5}  {}:{}  {}{}", f.lines, f.file, f.line, f.name, if f.test { " (test)" } else { "" });
            }
            for (path, lines) in &outcome.sizes.files {
                println!("{lines:>5}  {path}");
            }
            ExitCode::SUCCESS
        }
        _ => usage(),
    }
}

fn write(repo: &std::path::Path, outcome: &xtask::Outcome) -> ExitCode {
    for (path, text) in [(xtask::INVENTORY, &outcome.inventory), (xtask::LOCK_GRAPH, &outcome.lock_graph)] {
        if let Err(e) = std::fs::write(repo.join(path), text) {
            eprintln!("xtask: {path}: {e}");
            return ExitCode::FAILURE;
        }
        println!("wrote {path}");
    }
    ExitCode::SUCCESS
}

/// The soak's page for the runs measured in `files`; a failure if any detector failed.
fn soak_report(files: &[String]) -> ExitCode {
    let runs: Result<Vec<_>, _> = files.iter().map(|f| xtask::soak::Run::read(std::path::Path::new(f))).collect();
    let runs = match runs {
        Ok(r) if !r.is_empty() => r,
        Ok(_) => return usage(),
        Err(e) => {
            eprintln!("xtask: {e}");
            return ExitCode::FAILURE;
        }
    };
    print!("{}", xtask::soak::page(&runs));
    if runs.iter().all(|r| r.failures().is_empty()) { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn usage() -> ExitCode {
    eprintln!("usage: cargo run -p xtask -- <inventory|check|sizes> [--repo <path>] | soak-report <run.json>…");
    ExitCode::FAILURE
}
