//! Tests run with a home folder of their own, never yours. Much of Cue finds its files from $HOME (its data
//! folder when CUE_HOME isn't set, ~/.claude and ~/.codex settings, logs), and environment variables are shared
//! by every test thread, so one test unsetting CUE_HOME while another cleans up once deleted the socket of the
//! Cue you were using. The test binary moves $HOME before `main`, before any test thread runs (`SANDBOX`), and
//! keeps the real one only to recognise it (`real_home`).

use std::path::PathBuf;
use std::sync::OnceLock;

static REAL_HOME: OnceLock<PathBuf> = OnceLock::new();

/// Your home folder as it was when the tests started: for refusing it, never for using it.
pub fn real_home() -> &'static PathBuf {
    REAL_HOME.get_or_init(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()))
}

/// Run by the loader before `main`: $HOME becomes a folder of this test process's own.
extern "C" fn sandbox_home() {
    let _ = real_home();
    let home = std::env::temp_dir().join(format!("cue-test-home-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&home);
    std::env::set_var("HOME", &home);
}

#[used]
#[cfg_attr(target_os = "macos", link_section = "__DATA,__mod_init_func")]
#[cfg_attr(target_os = "linux", link_section = ".init_array")]
static SANDBOX: extern "C" fn() = sandbox_home;

#[cfg(test)]
mod tests {
    #[test]
    fn tests_run_with_a_home_of_their_own_from_the_start() {
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
        assert_ne!(&home, super::real_home(), "$HOME is still yours");
        assert!(home.ends_with(format!("cue-test-home-{}", std::process::id())));
        assert!(!crate::server::cue_dir().starts_with(super::real_home()), "Cue's folder resolves under your home");
    }
}
