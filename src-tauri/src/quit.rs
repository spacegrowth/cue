//! Quitting when asked by a signal (`kill`, logout, Ctrl+C in the terminal that started Cue): as Cue's own Quit
//! does, so what changed since the last draw is saved first (RunEvent::Exit). With Cue taking those signals
//! itself, one whose main thread is stuck would otherwise ignore them for good: a second signal, or a few
//! seconds with Cue still here, ends the process at once.

use std::time::Duration;
use tokio::sync::mpsc;

/// How long a quit may take before the process is ended anyway.
const GRACE: Duration = Duration::from_secs(3);

/// SIGTERM, SIGINT and SIGHUP, handled from now on.
pub fn on_signals(app: tauri::AppHandle) {
    let (tx, told) = mpsc::unbounded_channel();
    tauri::async_runtime::spawn(forward_signals(tx));
    tauri::async_runtime::spawn(quit_when_told(told, move || app.exit(0), || std::process::exit(1), GRACE));
}

/// Each quit signal, told on `tx`.
async fn forward_signals(tx: mpsc::UnboundedSender<()>) {
    use tokio::signal::unix::{signal, SignalKind};
    let (Ok(mut term), Ok(mut int), Ok(mut hup)) = (signal(SignalKind::terminate()), signal(SignalKind::interrupt()), signal(SignalKind::hangup())) else { return };
    loop {
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
            _ = hup.recv() => {}
        }
        if tx.send(()).is_err() {
            return;
        }
    }
}

/// At the first signal `quit`; then `force` at a second one, or once `grace` has passed.
async fn quit_when_told(mut told: mpsc::UnboundedReceiver<()>, quit: impl FnOnce(), force: impl FnOnce(), grace: Duration) {
    if told.recv().await.is_none() {
        return;
    }
    quit();
    let _ = tokio::time::timeout(grace, told.recv()).await;
    force();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    fn run(f: impl std::future::Future<Output = ()>) {
        tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap().block_on(f);
    }

    /// 1: quit asked; 2: ended anyway.
    fn steps() -> (Arc<AtomicU8>, impl FnOnce(), impl FnOnce()) {
        let step = Arc::new(AtomicU8::new(0));
        let (q, f) = (step.clone(), step.clone());
        (step, move || q.store(1, Ordering::SeqCst), move || f.store(2, Ordering::SeqCst))
    }

    #[test]
    fn a_quit_that_doesnt_end_the_process_is_ended_after_the_grace() {
        let (step, quit, force) = steps();
        let (tx, told) = mpsc::unbounded_channel();
        tx.send(()).unwrap();
        let start = Instant::now();
        run(quit_when_told(told, quit, force, Duration::from_millis(100)));
        assert_eq!(step.load(Ordering::SeqCst), 2, "asked to quit, then ended");
        assert!(start.elapsed() >= Duration::from_millis(100), "not before the grace was up");
        drop(tx);
    }

    #[test]
    fn a_second_signal_ends_it_at_once() {
        let (step, quit, force) = steps();
        let (tx, told) = mpsc::unbounded_channel();
        tx.send(()).unwrap();
        tx.send(()).unwrap();
        let start = Instant::now();
        run(quit_when_told(told, quit, force, Duration::from_secs(30)));
        assert_eq!(step.load(Ordering::SeqCst), 2);
        assert!(start.elapsed() < Duration::from_secs(1), "the second signal didn't wait for the grace");
    }
}
