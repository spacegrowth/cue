//! A stand-in for a session's agent process: a child of the test (`sleep`), so ending it (Park, Close)
//! signals nothing but the test's own process. Reaped as it exits, so it reads as gone the moment it is;
//! taken down with the test if nothing ended it.

use std::sync::{Arc, Mutex};

pub struct AgentProcess {
    pub pid: i32,
    child: Arc<Mutex<std::process::Child>>,
}

impl AgentProcess {
    pub fn start() -> Self {
        let child = std::process::Command::new("/bin/sleep").arg("60").spawn().expect("sleep starts");
        let pid = child.id() as i32;
        let child = Arc::new(Mutex::new(child));
        let reaper = child.clone();
        std::thread::spawn(move || loop {
            if crate::locking::lock(&reaper, "agent process").try_wait().map_or(true, |s| s.is_some()) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        });
        Self { pid, child }
    }

    /// Whether it still runs.
    pub fn running(&self) -> bool {
        crate::server::alive(self.pid)
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        let _ = crate::locking::lock(&self.child, "agent process").kill();
    }
}
