//! The Desk's work in the hub: what Cue does for a Desk, by area. Each area adds to `Hub` or works with it; the
//! window's commands stay in ipc/desk.rs and call in here.

pub(crate) mod away;
pub(crate) mod core;
pub(crate) mod landing;
pub(crate) mod lead;
pub(crate) mod stall;
pub(crate) mod theirs;
pub(crate) mod turn;

/// What the area files had in hand when they lived in hub/: the hub's names and its imports.
pub(crate) mod prelude {
    pub(crate) use crate::hub::*;
    pub(crate) use crate::model::*;
    pub(crate) use serde_json::{json, Value};
    pub(crate) use std::collections::HashMap;
    pub(crate) use std::sync::Arc;
    pub(crate) use tokio::sync::oneshot;
}
