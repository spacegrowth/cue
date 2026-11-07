//! A built-in terminal's input at its edges (stability plan §4, Terminal, F): still stuck after Resume, an
//! error from the pty, a queue that fills behind a stuck write, and an empty write.

use super::tests::Stalled;
use super::*;
use std::sync::Mutex;

/// A pty that refuses every write.
struct Broken;

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("Input/output error"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn resumed_while_its_client_is_still_stuck_the_next_write_latches_it_again() {
    let (_go, release) = mpsc::channel();
    let input = Input::new("still-stuck", Box::new(Stalled { release, got: Arc::new(Mutex::new(vec![])) })).unwrap();
    let bound = Duration::from_millis(200);
    assert!(input.write_within(b"a", bound).await.is_err());
    input.resume();
    let t0 = Instant::now();
    assert!(input.write_within(b"b", bound).await.is_err(), "still stuck: latched again");
    assert!(t0.elapsed() < Duration::from_millis(50), "at once: the stuck write is still in progress: {:?}", t0.elapsed());
    assert!(input.write_within(b"c", bound).await.is_err());
}

#[tokio::test]
async fn an_error_from_the_pty_latches_its_input_until_resumed() {
    let input = Input::new("broken", Box::new(Broken)).unwrap();
    let err = input.write(b"ls\r").await.unwrap_err();
    assert!(err.contains("Input/output error"), "{err}");
    assert_eq!(input.write(b"x").await.unwrap_err(), NOT_RESPONDING);
    input.resume();
    assert!(input.write(b"y").await.unwrap_err().contains("Input/output error"), "resumed: tried again, and told why");
}

#[tokio::test]
async fn writes_piling_up_behind_a_stuck_one_are_refused_at_once_when_the_queue_is_full() {
    let (_go, release) = mpsc::channel();
    let input = Input::new("full", Box::new(Stalled { release, got: Arc::new(Mutex::new(vec![])) })).unwrap();
    let waiting: Vec<_> = (0..QUEUE + 1)
        .map(|_| {
            let i = input.clone();
            tokio::spawn(async move { i.write_within(b"k", Duration::from_secs(30)).await })
        })
        .collect();
    // One being written, QUEUE waiting: the next has nowhere to go.
    while input.jobs.try_send(Job { bytes: vec![], epoch: 0, done: oneshot::channel().0 }).is_ok() {
        tokio::task::yield_now().await;
    }
    let t0 = Instant::now();
    assert_eq!(input.write_within(b"one too many", Duration::from_secs(30)).await.unwrap_err(), NOT_RESPONDING);
    assert!(t0.elapsed() < Duration::from_millis(100), "{:?}", t0.elapsed());
    assert!(input.state.latched.load(Ordering::Acquire));
    for w in waiting {
        w.abort();
    }
}

#[tokio::test]
async fn an_empty_write_goes_and_changes_nothing() {
    let (go, release) = mpsc::channel();
    drop(go);
    let got = Arc::new(Mutex::new(vec![]));
    let input = Input::new("empty", Box::new(Stalled { release, got: got.clone() })).unwrap();
    input.write(b"").await.unwrap();
    input.write(b"a").await.unwrap();
    assert_eq!(*crate::locking::lock(&got, "got"), b"a");
}
