//! The window as a test hears it: every event the hub tells it beside its redraws, in order.

use crate::hub::ports::Window;
use serde_json::Value;
use std::sync::Mutex;

#[derive(Default)]
pub struct FakeWindow {
    heard: Mutex<Vec<(String, Value)>>,
}

impl FakeWindow {
    /// What it was told as `event`, in order.
    pub fn heard(&self, event: &str) -> Vec<Value> {
        crate::locking::lock(&self.heard, "fake window").iter().filter(|(e, _)| e == event).map(|(_, v)| v.clone()).collect()
    }
}

impl Window for FakeWindow {
    fn tell(&self, event: &str, payload: Value) {
        crate::locking::lock(&self.heard, "fake window").push((event.to_string(), payload));
    }
}
