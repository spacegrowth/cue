#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `cue hook <event> [harness]`: the agents' hook, handled before any window or menu bar code.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("hook") {
        cue_lib::hook_main(&args[1..]);
        std::process::exit(0);
    }
    // `cue host serve|connect …`: cue-host, which keeps hosted sessions running across Cue restarts (host/).
    if args.first().map(String::as_str) == Some("host") {
        let me = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        std::process::exit(cue_host::main_with(&args[1..], |home| vec![me.clone(), "host".into(), "serve".into(), "--home".into(), home.to_string_lossy().into_owned()]));
    }
    // `cue connect <claude|codex|pi>…`: what Settings → Connect does (install.sh uses it too).
    if args.first().map(String::as_str) == Some("connect") {
        std::process::exit(cue_lib::connect_main(&args[1..]));
    }
    cue_lib::run()
}
