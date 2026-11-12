//! An extension's program that misbehaves (stability plan §4, Extensions / actions): one that prints far
//! more than an answer carries, and one that never exits.

use super::*;

/// Extension `tools` switched on, allowed `programs`, in a Cue folder of its own while this lives.
struct Tools {
    dir: PathBuf,
    _g: std::sync::MutexGuard<'static, ()>,
}

impl Tools {
    fn on(programs: &[&str]) -> Self {
        let g = crate::db::test_turn();
        let dir = std::env::temp_dir().join(format!("cue-ext-faults-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("extensions/tools")).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        let manifest = json!({"tab": "T", "capabilities": ["programs.run"], "programs": programs});
        std::fs::write(dir.join("extensions/tools/manifest.json"), manifest.to_string()).unwrap();
        enable("tools", true).unwrap();
        Tools { dir, _g: g }
    }

    fn run(&self, program: &str, args: &[&str]) -> Result<Value, String> {
        run_program("tools", program, &args.iter().map(|a| a.to_string()).collect::<Vec<_>>(), None)
    }
}

impl Drop for Tools {
    fn drop(&mut self) {
        std::env::remove_var("CUE_HOME");
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 10 MB printed: the page gets the first 4 MB and the exit code, not all of it.
#[test]
fn a_program_printing_10_mb_gives_back_its_first_4_mb() {
    let t = Tools::on(&["head"]);
    let out = t.run("head", &["-c", "10485760", "/dev/zero"]).unwrap();
    assert_eq!(out["code"], 0);
    assert_eq!(out["stdout"].as_str().unwrap().len(), 4 << 20);
}

/// A program that never exits is stopped at the limit and says so, in the answer and the log.
#[test]
fn a_program_that_never_exits_is_stopped_at_the_limit() {
    let t = Tools::on(&["sleep"]);
    let t0 = Instant::now();
    assert_eq!(t.run("sleep", &["600"]), Err("sleep took too long".into()));
    assert!(t0.elapsed() < PROGRAM_LIMIT + std::time::Duration::from_secs(2), "{:?}", t0.elapsed());
    assert!(activity("tools", 5).iter().any(|l| l.contains("ran sleep 600 -> stopped (took too long)")));
}
