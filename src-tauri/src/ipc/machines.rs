//! The Machines commands: list, add, rename, remove, set up, check, log in.

use crate::machines;

/// + New → Machine: the machines you added, to run sessions on over SSH.
#[tauri::command]
pub(crate) fn machines_list() -> Vec<machines::Machine> {
    machines::list()
}

#[tauri::command]
pub(crate) fn machine_add(host: String, name: String) -> Result<machines::Machine, String> {
    let m = machines::add(&host, &name)?;
    machines::hooks::watch(&m);
    Ok(m)
}

#[tauri::command]
pub(crate) fn machine_rename(name: String, label: String) -> Result<machines::Machine, String> {
    machines::rename(&name, &label)
}

#[tauri::command]
pub(crate) fn machine_remove(name: String) -> Result<(), String> {
    machines::hooks::unwatch(&name);
    machines::remove(&name)
}

/// Set up Cue's hooks there, for the agents its check found (never installing any): what it did.
#[tauri::command]
pub(crate) async fn machine_setup(name: String, tools: serde_json::Value) -> Result<Vec<String>, String> {
    let m = machines::get(&name).ok_or("that machine isn't added")?;
    tauri::async_runtime::spawn_blocking(move || machines::hooks::setup(&m, &tools)).await.map_err(|e| e.to_string())?
}

/// Can Cue reach it, and what's installed there (it connects, so it takes a moment).
#[tauri::command]
pub(crate) async fn machine_check(name: String) -> Result<serde_json::Value, String> {
    let m = machines::get(&name).ok_or("that machine isn't added")?;
    tauri::async_runtime::spawn_blocking(move || machines::check(&m)).await.map_err(|e| e.to_string())
}

/// Log in yourself, in a terminal tab; Cue uses the connection you open.
#[tauri::command]
pub(crate) async fn machine_login(name: String) -> Result<String, String> {
    let m = machines::get(&name).ok_or("that machine isn't added")?;
    tauri::async_runtime::spawn_blocking(move || machines::login(&m)).await.map_err(|e| e.to_string())?
}
