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
