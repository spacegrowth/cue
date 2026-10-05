#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `cue hook <event> [harness]`: the agents' hook, handled before any window or menu bar code.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("hook") {
        cue_lib::hook_main(&args[1..]);
        std::process::exit(0);
    }
    // `cue connect <claude|codex|pi>…`: what Settings → Connect does (install.sh uses it too).
    if args.first().map(String::as_str) == Some("connect") {
        std::process::exit(cue_lib::connect_main(&args[1..]));
    }
    cue_lib::run()
}
