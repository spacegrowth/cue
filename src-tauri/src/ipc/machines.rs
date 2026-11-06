//! The Machines commands: list, add, rename, remove, set up, check, log in.

use super::blocking;
use crate::machines;

/// + New → Machine: the machines you added, to run sessions on over SSH.
#[tauri::command]
pub(crate) async fn machines_list() -> Result<Vec<machines::Machine>, String> {
    blocking(machines::list).await
}

#[tauri::command]
pub(crate) async fn machine_add(host: String, name: String) -> Result<machines::Machine, String> {
    blocking(move || {
        let m = machines::add(&host, &name)?;
        machines::hooks::watch(&m);
        Ok(m)
    })
    .await?
}

#[tauri::command]
pub(crate) async fn machine_rename(name: String, label: String) -> Result<machines::Machine, String> {
    blocking(move || machines::rename(&name, &label)).await?
}

#[tauri::command]
pub(crate) async fn machine_remove(name: String) -> Result<(), String> {
    blocking(move || {
        machines::hooks::unwatch(&name);
        machines::remove(&name)
    })
    .await?
}

/// Set up Cue's hooks there, for the agents its check found (never installing any): what it did.
#[tauri::command]
pub(crate) async fn machine_setup(name: String, tools: serde_json::Value) -> Result<Vec<String>, String> {
    let m = machines::get(&name).ok_or("that machine isn't added")?;
    blocking(move || machines::hooks::setup(&m, &tools)).await?
}

/// Can Cue reach it, and what's installed there (it connects, so it takes a moment).
#[tauri::command]
pub(crate) async fn machine_check(name: String) -> Result<serde_json::Value, String> {
    let m = machines::get(&name).ok_or("that machine isn't added")?;
    blocking(move || machines::check(&m)).await
}

/// Log in yourself, in a terminal tab; Cue uses the connection you open.
#[tauri::command]
pub(crate) async fn machine_login(name: String) -> Result<String, String> {
    let m = machines::get(&name).ok_or("that machine isn't added")?;
    blocking(move || machines::login(&m)).await?
}
