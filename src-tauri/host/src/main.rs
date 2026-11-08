//! cue-host on its own: next to Cue's program in Cue.app on this Mac, and on a machine Cue reaches over SSH.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let me = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "cue-host".into());
    let code = cue_host::main_with(&args, |home| vec![me.clone(), "serve".into(), "--home".into(), home.to_string_lossy().into_owned()]);
    std::process::exit(code);
}
