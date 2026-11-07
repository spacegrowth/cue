//! The World's hub on a socket of its own (cue.sock in its CUE_HOME), served as the app serves it
//! (server::serve): for what reaches Cue the way a hook does, by connecting, rather than through `drive`.

use super::World;
use std::os::unix::net::UnixStream;

/// The socket, open until this is dropped.
pub struct Served {
    runtime: Option<tokio::runtime::Runtime>,
    /// The socket this World served, fixed when it started: CUE_HOME is process-wide and other tests change it,
    /// so the path is never worked out again at the end.
    sock: std::path::PathBuf,
}

impl World {
    pub fn serve(&self) -> Served {
        let clock = self.clock();
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().on_thread_start(move || clock.install()).build().unwrap();
        let hub = self.hub().clone();
        runtime.spawn(async move { crate::server::serve(hub).await });
        let sock = crate::server::socket_path();
        super::until("the World's socket to take connections", || UnixStream::connect(&sock).is_ok());
        Served { runtime: Some(runtime), sock }
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        if let Some(r) = self.runtime.take() {
            r.shutdown_background();
        }
        let _ = std::fs::remove_file(&self.sock);
    }
}
