mod chaos;
mod clock;
mod config;
mod actions;
mod awake;
mod btw;
mod db;
mod hook;
// SHORTCUT: only its tests start hosted sessions until the hub's Hosted port does (docs/dev/cue-hosted-sessions.md,
// M2); this allow goes with that wiring.
#[cfg_attr(not(test), allow(dead_code, unused_imports))]
mod hosted;
mod changes;
mod safe_git;
mod commands;
mod hub;
mod ipc;
mod leads;
mod locking;
mod logs;
mod model;
mod notify;
mod quit;
mod sounds;
mod statuslist;
mod talk;
mod tasks;
// An optional add-on, compiled in only with the `ext` feature: Rust in `src-tauri/ext/` (not part of
// this repo). Without it there's nothing here.
#[cfg(feature = "ext")]
#[path = "../ext/mod.rs"]
mod ext;
mod extensions;
mod server;
mod sessions;
mod startup;
mod uploads;
mod terminal;
#[cfg(test)]
mod testing;
mod machines;
mod mini;
mod remote_poll;

// The old crate-root paths of the moved modules, kept for what still names them there: the hub (its own
// packet) and the optional add-on.
pub(crate) use logs::{about, live, steps, transcript, usage, usage_file};
pub(crate) use terminal::{dialog, focus};
#[cfg(feature = "ext")]
pub(crate) use terminal::shell;

use tauri::Manager;

/// Bring up the full window, optionally on one item (the menu bar, a notification) or, as "s:<session>", on a
/// session's chat.
pub(crate) fn show_main(app: &tauri::AppHandle, id: Option<String>) {
    use tauri::Emitter;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    activate_app();
    if let Some(id) = id {
        let _ = app.emit_to("main", "select", id);
    }
}

/// Bring Cue in front of the app you're in. macOS refuses in-process activation for a click on a
/// menu bar item ("cooperative activation"); opening our own bundle via LaunchServices is honoured
/// (measured in a Tauri menu bar app). A bare dev binary has no bundle: it falls back
/// to the in-process call, which works only once Cue is already active.
fn activate_app() {
    let bundle = std::env::current_exe().ok().and_then(|exe| exe.ancestors().nth(3).map(|p| p.to_path_buf())).filter(|p| p.extension().is_some_and(|e| e == "app"));
    match bundle {
        Some(b) => {
            let _ = std::process::Command::new("open").arg(&b).status();
        }
        None => {}
    }
}

/// The agents' hook (see hook.rs): runs and returns; the caller exits.
pub fn hook_main(args: &[String]) {
    // Settings into memory once, as the app does at launch (config.rs load reads only memory).
    config::refresh();
    hook::main(args)
}

/// Text size: the whole window zooms (text, cards and buttons together), as a browser's ⌘+ does.
pub(crate) fn apply_zoom(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_zoom(config::zoom());
    }
}

/// `cue connect <claude|codex|pi>…` from a terminal: the same as Settings → Connect. Prints what it
/// did; exit code 1 if any failed. Pi's extension comes from the app bundle, else this checkout.
pub fn connect_main(agents: &[String]) -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    let bundled = exe.parent().map(|d| d.join("../Resources/pi/cue.ts")).filter(|p| p.exists());
    // A development build run from this checkout uses the checkout's copy; a release build only its bundle
    // (so the build machine's paths never end up in a released app).
    #[cfg(debug_assertions)]
    let pi_ext = bundled.unwrap_or_else(|| std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi/cue.ts")));
    #[cfg(not(debug_assertions))]
    let pi_ext = bundled.unwrap_or_else(|| exe.with_file_name("pi/cue.ts"));
    if agents.is_empty() {
        eprintln!("usage: cue connect <claude|codex|pi>…");
        return 1;
    }
    let mut code = 0;
    for a in agents {
        match config::connect(a, &pi_ext) {
            Ok(m) => println!("{m}"),
            Err(e) => {
                eprintln!("{a}: {e}");
                code = 1;
            }
        }
    }
    code
}

/// Whether these locale settings (LC_ALL, LC_CTYPE, LANG, in the order that wins) say UTF-8.
fn utf8_locale(get: impl Fn(&str) -> Option<String>) -> bool {
    ["LC_ALL", "LC_CTYPE", "LANG"].iter().find_map(|k| get(k).filter(|v| !v.is_empty())).is_some_and(|v| {
        let v = v.to_ascii_uppercase();
        v.contains("UTF-8") || v.contains("UTF8")
    })
}

/// A setting Cue drops from its own environment at start, so tmux and the sessions it opens don't inherit
/// it: the marks of the Claude Code session Cue may have been opened from (`CLAUDE_CODE_*`, `CLAUDECODE`),
/// and, when that shell pointed Claude Code at another provider (`ANTHROPIC_BASE_URL` set), all of that
/// setup (`ANTHROPIC_*`: its key must not go to Anthropic either). Your own `ANTHROPIC_API_KEY` alone stays.
fn inherited_agent_var(k: &str, other_provider: bool) -> bool {
    (other_provider && k.starts_with("ANTHROPIC_")) || k.starts_with("CLAUDE_CODE_") || k == "CLAUDECODE" || k == "CLAUDE_PID"
        || terminal::shells::NOT_HERE.contains(&k)
}

/// Open Cue: settings read and a clean environment, then the app built and run (startup/).
pub fn run() {
    // Settings into memory before anything reads one (the window's size and zoom, below).
    config::refresh();
    // Opened from the Finder or the Dock, an app gets no locale, and tmux without UTF-8 rewrites what it
    // prints (a tab in its answers as "_", "❯" as "_" in the terminal it draws). Cue and everything it
    // starts (tmux, agents, shells) get UTF-8, as they would from a terminal. Set before any thread starts.
    if !utf8_locale(|k| std::env::var(k).ok()) {
        std::env::set_var("LC_CTYPE", "UTF-8");
    }
    // Opened from a shell set up for another provider, or from inside a Claude Code session, Cue would hand
    // that setup to tmux, and tmux to every session it opens. Cue starts them clean, as a new terminal would.
    // Nor is the terminal it was opened from Cue's: started inside tmux (`open` passes the shell's TMUX on),
    // tmux would refuse every terminal Cue opens as nested ("sessions should be nested with care").
    let other_provider = std::env::var_os("ANTHROPIC_BASE_URL").is_some_and(|v| !v.is_empty());
    for (k, _) in std::env::vars_os() {
        if k.to_str().is_some_and(|k| inherited_agent_var(k, other_provider)) {
            std::env::remove_var(&k);
        }
    }
    awake::stay_responsive();
    startup::build().run(startup::window::on_run_event);
}

#[cfg(test)]
mod tests {
    /// The window's scripts (ui/index.html, in order) share one global scope: a top-level name declared
    /// in two of them is a SyntaxError that stops the second script before it runs, and the window
    /// shows its background and nothing else (that reached the installed app once). Each name once.
    #[test]
    fn the_windows_scripts_declare_each_top_level_name_once() {
        let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
        let index = std::fs::read_to_string(ui.join("index.html")).unwrap();
        let scripts: Vec<&str> = index.split("<script src=\"").skip(1).filter_map(|r| r.split('"').next()).filter(|s| !s.starts_with("vendor/")).collect();
        assert!(scripts.len() >= 2, "{scripts:?}");
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut twice = vec![];
        for script in &scripts {
            let text = std::fs::read_to_string(ui.join(script)).unwrap();
            for (i, line) in text.lines().enumerate() {
                let mut words = line.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'));
                let (Some(kw), Some(name)) = (words.next(), words.next()) else { continue };
                if !matches!(kw, "const" | "let" | "var" | "function" | "class") || name.is_empty() || line.starts_with(' ') || line.starts_with('\t') {
                    continue;
                }
                let at = format!("{script}:{}", i + 1);
                match seen.insert(name.to_string(), at.clone()) {
                    Some(first) if !first.starts_with(&format!("{script}:")) => twice.push(format!("{name}: {first} and {at}")),
                    _ => {}
                }
            }
        }
        assert!(twice.is_empty(), "declared in two of the window's scripts:\n{}", twice.join("\n"));
    }

    use super::*;

    #[test]
    fn another_providers_setup_and_a_parent_claude_session_are_dropped() {
        for k in ["ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY", "ANTHROPIC_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL", "CLAUDE_CODE_SESSION_ID", "CLAUDECODE"] {
            assert!(inherited_agent_var(k, true), "{k}");
        }
        for k in ["PATH", "HOME", "LC_CTYPE", "CUE_DRIVEN_BY", "CLAUDE_CONFIG_DIR", "ANTHROPIC"] {
            assert!(!inherited_agent_var(k, true), "{k} stays");
        }
        // No other provider: your own key and settings stay; a parent session's marks still go.
        assert!(!inherited_agent_var("ANTHROPIC_API_KEY", false) && !inherited_agent_var("ANTHROPIC_MODEL", false));
        assert!(inherited_agent_var("CLAUDECODE", false) && inherited_agent_var("CLAUDE_CODE_SESSION_ID", false));
        // The terminal it was opened from: tmux's above all (every terminal Cue opened would quit as nested).
        for k in ["TMUX", "TMUX_PANE", "TERM_PROGRAM", "ITERM_SESSION_ID", "CLAUDE_PID"] {
            assert!(inherited_agent_var(k, false), "{k}");
        }
        assert!(!inherited_agent_var("TMUX_TMPDIR", false) && !inherited_agent_var("TERM", false), "where tmux keeps its servers, and the terminal type, stay");
    }

    #[test]
    fn a_utf8_locale_is_recognised_whichever_setting_says_so() {
        let env = |pairs: &'static [(&'static str, &'static str)]| move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string());
        assert!(!utf8_locale(env(&[])), "none at all (opened from the Finder)");
        assert!(utf8_locale(env(&[("LANG", "en_US.UTF-8")])));
        assert!(utf8_locale(env(&[("LC_CTYPE", "C.UTF-8"), ("LANG", "C")])), "LC_CTYPE wins over LANG");
        assert!(!utf8_locale(env(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")])), "LC_ALL wins over both");
        assert!(utf8_locale(env(&[("LC_ALL", ""), ("LANG", "de_DE.utf8")])), "an empty one doesn't count");
    }
}
