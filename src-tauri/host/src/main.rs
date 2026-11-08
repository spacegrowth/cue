//! cue-host on its own: for a machine Cue reaches over SSH (on this Mac, Cue runs it as `cue host …`).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let me = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "cue-host".into());
    let code = cue_host::main_with(&args, |home| vec![me.clone(), "serve".into(), "--home".into(), home.to_string_lossy().into_owned()]);
    std::process::exit(code);
}
