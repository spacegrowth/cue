//! The window's commands (`#[tauri::command]`) and the helpers they share, one file per area; lib.rs
//! lists them in `generate_handler!`.

use crate::hub::Hub;
use std::sync::Arc;

mod chat;
mod docs;
mod hosted;
mod insights;
mod insights_tab;
mod machines;
mod restore;
mod sessions;
mod settings;
mod state;
mod terminal;
mod updates;

pub(crate) use chat::*;
pub(crate) use docs::*;
pub(crate) use hosted::*;
pub(crate) use insights::*;
pub(crate) use insights_tab::*;
pub(crate) use machines::*;
pub(crate) use restore::*;
pub(crate) use sessions::*;
pub(crate) use settings::*;
pub(crate) use state::*;
pub(crate) use terminal::*;
pub(crate) use updates::*;

/// `f` on the blocking pool: what a command does that can wait (a lock, the db, a file, a process, the
/// network) runs there, never on the window's thread (where Tauri runs a command that isn't async) nor on
/// an async worker. A panic in it comes back as the command's error.
pub(crate) async fn blocking<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> Result<R, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

/// `blocking`, with the hub: nearly every command's work is a hub call, and each takes the store's lock.
/// (A command's `State<Arc<Hub>>` passes as `&hub`.)
pub(crate) async fn with_hub<R: Send + 'static>(hub: &Arc<Hub>, f: impl FnOnce(&Arc<Hub>) -> R + Send + 'static) -> Result<R, String> {
    let h = hub.clone();
    blocking(move || f(&h)).await
}
