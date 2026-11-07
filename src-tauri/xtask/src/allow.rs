//! docs/dev/lint-allow.toml: what the checks accept today, each entry with its reason. An entry nothing
//! matches any more is an error too, so the list only shrinks as the code gets there.

use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allow {
    pub store: Store,
    #[serde(default)]
    pub sync_command: Vec<SyncCommand>,
    #[serde(default)]
    pub bare_lock: Vec<BareLock>,
    #[serde(default)]
    pub lock_order: Vec<LockOrder>,
    #[serde(default)]
    pub store_io: Vec<StoreIo>,
    #[serde(default)]
    pub tmux_seam: Vec<TmuxSeam>,
}

/// The lock no external call may happen under.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    pub lock: String,
}

/// A command Tauri runs on the main thread (not `async`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncCommand {
    pub name: String,
    pub reason: String,
}

/// A lock taken another way than `locking::lock`: in `file` (under src/), on `receiver` as written,
/// known to the lock graph as `name`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BareLock {
    pub file: String,
    pub receiver: String,
    pub name: String,
    pub reason: String,
}

/// `to` may be taken while `from` is held.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockOrder {
    pub from: String,
    pub to: String,
    pub reason: String,
}

/// An external call of kind `effect`, made in `site`, reachable from `holder` while it holds the store.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreIo {
    pub holder: String,
    pub effect: String,
    pub site: String,
    pub reason: String,
}

/// A function that may start tmux by its literal name: the seam that points tests at a private server
/// (see check/tmux.rs). Configurable so the seam can move without touching the rule.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TmuxSeam {
    pub file: String,
    pub function: String,
    pub reason: String,
}

impl Allow {
    pub fn load(path: &Path) -> Result<Allow, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let allow: Allow = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        allow.check_reasons().map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(allow)
    }

    /// Every entry says why.
    fn check_reasons(&self) -> Result<(), String> {
        let reasons = self.sync_command.iter().map(|s| (&s.name, &s.reason))
            .chain(self.bare_lock.iter().map(|b| (&b.receiver, &b.reason)))
            .chain(self.lock_order.iter().map(|o| (&o.to, &o.reason)))
            .chain(self.store_io.iter().map(|v| (&v.site, &v.reason)))
            .chain(self.tmux_seam.iter().map(|t| (&t.function, &t.reason)));
        for (what, reason) in reasons {
            if reason.trim().is_empty() {
                return Err(format!("the entry for {what} has no reason"));
            }
        }
        Ok(())
    }

    pub fn bare_locks(&self) -> Vec<crate::walk::BareLock> {
        self.bare_lock.iter().map(|b| crate::walk::BareLock { file: b.file.clone(), receiver: b.receiver.clone(), name: b.name.clone() }).collect()
    }
}
