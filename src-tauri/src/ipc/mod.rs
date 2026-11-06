//! The window's commands (`#[tauri::command]`) and the helpers they share, one file per area; lib.rs
//! lists them in `generate_handler!`.

mod chat;
mod insights;
mod insights_tab;
mod machines;
mod restore;
mod sessions;
mod settings;
mod state;
mod terminal;

pub(crate) use chat::*;
pub(crate) use insights::*;
pub(crate) use insights_tab::*;
pub(crate) use machines::*;
pub(crate) use restore::*;
pub(crate) use sessions::*;
pub(crate) use settings::*;
pub(crate) use state::*;
pub(crate) use terminal::*;

/// `f` on the blocking pool: what a command does that can wait (a lock, the db, a file, a process, the
/// network) runs there, never on the window's thread (where Tauri runs a command that isn't async) nor on
/// an async worker. A panic in it comes back as the command's error.
pub(crate) async fn blocking<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> Result<R, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}
