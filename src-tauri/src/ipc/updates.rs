//! Updates: a newer Cue from GitHub's releases, signed with Cue's update key, on the channel Settings picks.
//! Stable reads latest.json (tauri.conf.json's endpoint); beta ("Get beta builds") reads latest-beta.json, which
//! scripts/release.sh publishes beside it and keeps at least as new as stable (docs/dev/releasing.md).

use tauri::Url;

/// Which releases Cue is offered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Channel {
    Stable,
    Beta,
}

impl Channel {
    /// The channel Settings → Get beta builds says, from the settings in memory.
    pub(crate) fn chosen() -> Self {
        Self::from_beta(crate::config::beta_updates())
    }

    pub(crate) fn from_beta(beta: bool) -> Self {
        if beta { Channel::Beta } else { Channel::Stable }
    }
}

/// The beta channel's release file, published beside each stable latest.json.
pub(crate) const BETA_FILE: &str = "latest-beta.json";

/// The release files a check on `channel` asks: the stable ones as they are, or the beta file beside each.
pub(crate) fn endpoints_for(stable: &[Url], channel: Channel) -> Vec<Url> {
    match channel {
        Channel::Stable => stable.to_vec(),
        // (A relative join replaces the URL's last segment: …/download/latest.json → …/download/latest-beta.json.)
        Channel::Beta => stable.iter().filter_map(|u| u.join(BETA_FILE).ok()).collect(),
    }
}

/// The stable release files, as tauri.conf.json's updater section lists them.
fn configured_endpoints<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<Url> {
    let updater = app.config().plugins.0.get("updater");
    let listed = updater.and_then(|u| u.get("endpoints")).and_then(serde_json::Value::as_array);
    listed.into_iter().flatten().filter_map(|e| e.as_str()).filter_map(|e| Url::parse(e).ok()).collect()
}

/// Updates: is a newer Cue out on your channel? Always says which version this is.
#[tauri::command]
pub(crate) async fn update_check(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let current = app.package_info().version.to_string();
    Ok(match find_update(&app).await? {
        Some(u) => serde_json::json!({ "current": current, "version": u.version, "notes": u.body }),
        None => serde_json::json!({ "current": current }),
    })
}

/// Download the newer Cue, check its signature, put it in place of this one, and restart into it.
#[tauri::command]
pub(crate) async fn update_install(app: tauri::AppHandle) -> Result<(), String> {
    install_update(app).await
}

pub(crate) async fn install_update<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    let Some(mut u) = find_update(&app).await? else { return Err("Cue is already up to date".into()) };
    // The check's wait is for the release server's answer; the download takes as long as your line needs.
    u.timeout = None;
    u.download_and_install(|_, _| {}, || {}).await.map_err(|e| format!("Couldn't install the update: {e}"))?;
    app.restart()
}

/// A newer Cue was found (in the background): the window shows its card.
pub(crate) fn offer_update(app: &tauri::AppHandle, u: &tauri_plugin_updater::Update) {
    use tauri::Emitter;
    let _ = app.emit("update-ready", serde_json::json!({ "version": u.version, "notes": u.body }));
}

/// The longest a check for a newer Cue waits on the release server. Past it, it couldn't check: a network that
/// takes the connection and never answers would otherwise hold that check for good, and with it the window's
/// "Check for updates" and every later check in the background.
const CHECK_FOR: std::time::Duration = std::time::Duration::from_secs(30);

/// A Cue built with the add-on never offers a release: releases are built without it, so installing
/// one would drop the add-on. That Cue is updated by building it again.
pub(crate) async fn find_update<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<Option<tauri_plugin_updater::Update>, String> {
    check_within(app, CHECK_FOR, Channel::chosen()).await
}

/// `find_update` on `channel`, waiting `wait` at most for the release server.
pub(crate) async fn check_within<R: tauri::Runtime>(app: &tauri::AppHandle<R>, wait: std::time::Duration, channel: Channel) -> Result<Option<tauri_plugin_updater::Update>, String> {
    if cfg!(feature = "ext") {
        return Ok(None);
    }
    use tauri_plugin_updater::UpdaterExt;
    let endpoints = endpoints_for(&configured_endpoints(app), channel);
    let builder = app.updater_builder().endpoints(endpoints).map_err(|e| e.to_string())?;
    let updater = builder.timeout(wait).build().map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| format!("Couldn't check for updates: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const STABLE: &str = "https://github.com/spacegrowth/cue/releases/latest/download/latest.json";

    fn chosen_by(settings: serde_json::Value) -> Vec<String> {
        let channel = Channel::from_beta(crate::config::beta_updates_in(&settings));
        endpoints_for(&[Url::parse(STABLE).unwrap()], channel).iter().map(Url::to_string).collect()
    }

    #[test]
    fn stable_until_get_beta_builds_is_turned_on() {
        assert_eq!(chosen_by(json!({})), vec![STABLE], "off by default");
        assert_eq!(chosen_by(json!({ "updates": { "beta": false } })), vec![STABLE]);
        assert_eq!(chosen_by(json!({ "updates": { "beta": "yes" } })), vec![STABLE], "only a real true turns it on");
    }

    #[test]
    fn beta_reads_the_beta_file_beside_stables() {
        let beta = "https://github.com/spacegrowth/cue/releases/latest/download/latest-beta.json";
        assert_eq!(chosen_by(json!({ "updates": { "beta": true } })), vec![beta]);
    }

    #[test]
    fn the_shipped_config_names_the_stable_file() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        assert_eq!(conf["plugins"]["updater"]["endpoints"], json!([STABLE]), "release.sh publishes latest.json there");
    }
}
