//! At launch, the hosted sessions cue-host kept running while Cue was away are picked up off the window's thread:
//! with many it takes seconds (a round trip to cue-host and a read of each one's conversation), and while its
//! thread waits the window takes no click or key (#456). Until they are, anything that reaches the hosted port
//! from another thread waits: a message sent meanwhile would start a second Claude on a conversation cue-host
//! still runs.

use super::*;
use std::thread::ThreadId;

/// Whether the pickup runs, and on which thread (that one goes on through the port: what it picks up can
/// report, and be sent to, on its own thread).
#[derive(Default)]
pub(crate) struct PickingUp(Mutex<(bool, Option<ThreadId>)>);

impl PickingUp {
    /// Until the pickup is over, on any thread but its own.
    pub(crate) fn wait(&self) {
        let me = std::thread::current().id();
        loop {
            let now = {
                let g = crate::locking::lock(&self.0, "hosted pickup");
                *g
            };
            match now {
                (true, owner) if owner != Some(me) => crate::clock::sleep_ms(5),
                _ => return,
            }
        }
    }

    fn set(&self, to: (bool, Option<ThreadId>)) {
        let mut g = crate::locking::lock(&self.0, "hosted pickup");
        *g = to;
    }
}

/// The pickup over when it ends, however it ends (a panic in it must not leave every send waiting).
struct Over<'a>(&'a PickingUp);

impl Drop for Over<'_> {
    fn drop(&mut self) {
        self.0.set((false, None));
    }
}

impl Hub {
    /// Launch: the hosted sessions cue-host kept running picked up (`reattach_hosted`) on a thread of their own,
    /// then `then` on it (what must come after them). Returns at once; from now until they are picked up, a send
    /// to a hosted session waits.
    pub(crate) fn reattach_hosted_then(self: &Arc<Self>, then: impl FnOnce(&Arc<Hub>) + Send + 'static) -> std::thread::JoinHandle<()> {
        self.picking_up.set((true, None));
        let me = self.clone();
        std::thread::spawn(move || {
            {
                me.picking_up.set((true, Some(std::thread::current().id())));
                let _over = Over(&me.picking_up);
                me.reattach_hosted();
            }
            then(&me);
        })
    }
}
