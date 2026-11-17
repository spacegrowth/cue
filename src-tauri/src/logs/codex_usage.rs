//! Account usage from Codex's local rollout events. Reads bounded tails on a background ticker.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Window {
    pub pct: f64,
    pub minutes: u64,
    pub resets_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Rates {
    pub windows: Vec<Window>,
    pub at_ms: u64,
}

#[derive(Default)]
pub struct Reader {
    files: HashMap<PathBuf, (SystemTime, u64, Option<Rates>)>,
}

impl Reader {
    pub fn read(&mut self, root: &Path, now: u64) -> Option<Rates> {
        let paths = candidates(root);
        self.files.retain(|p, _| paths.contains(p));
        for path in paths {
            let Ok(meta) = path.metadata() else { continue };
            let Ok(modified) = meta.modified() else { continue };
            if self.files.get(&path).is_some_and(|(m, len, _)| *m == modified && *len == meta.len()) {
                continue;
            }
            // An incomplete final line is ignored and tried again when the file grows.
            let Ok(mut file) = std::fs::File::open(&path) else { continue };
            if file.seek(SeekFrom::Start(meta.len().saturating_sub(512 * 1024))).is_err() {
                continue;
            }
            let mut bytes = Vec::new();
            if file.take(512 * 1024).read_to_end(&mut bytes).is_err() {
                continue;
            }
            let rates = String::from_utf8_lossy(&bytes).lines().rev()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .filter_map(|event| parse(&event))
                .max_by_key(|rates| rates.at_ms);
            // A long response can push the last usage event outside the tail. Keep the last
            // observed event until a newer event arrives or its windows expire.
            let previous = self.files.get(&path).and_then(|(_, _, rates)| rates.clone());
            self.files.insert(path, (modified, meta.len(), rates.or(previous)));
        }
        let mut rates = self.files.values().filter_map(|(_, _, rates)| rates.as_ref()).max_by_key(|r| r.at_ms)?.clone();
        rates.windows.retain(|w| w.resets_ms.map_or(true, |at| at > now));
        (!rates.windows.is_empty()).then_some(rates)
    }
}

/// Newest modified rollouts in the three newest day folders. Directory names are ISO date parts.
fn candidates(root: &Path) -> Vec<PathBuf> {
    let dirs = |path: &Path| {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(path).into_iter().flatten().flatten()
            .map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort_unstable_by(|a, b| b.cmp(a));
        dirs
    };
    let days: Vec<PathBuf> = dirs(root).into_iter().flat_map(|y| dirs(&y)).flat_map(|m| dirs(&m)).take(3).collect();
    let mut files: Vec<_> = days.iter().flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p))).collect();
    files.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(32).map(|(_, p)| p).collect()
}

fn parse(event: &Value) -> Option<Rates> {
    if event.get("type")?.as_str()? != "event_msg" || event.pointer("/payload/type")?.as_str()? != "token_count" {
        return None;
    }
    let limits = event.pointer("/payload/rate_limits")?;
    // Other named buckets belong to particular models, not the account meter.
    if limits.get("limit_id").and_then(Value::as_str).is_some_and(|id| id != "codex") {
        return None;
    }
    let windows: Vec<Window> = ["primary", "secondary"].into_iter().filter_map(|key| {
        let w = limits.get(key)?;
        let pct = w.get("used_percent")?.as_f64()?;
        let minutes = w.get("window_minutes")?.as_u64()?;
        if !pct.is_finite() || pct < 0.0 || minutes == 0 {
            return None;
        }
        Some(Window { pct, minutes, resets_ms: w.get("resets_at").and_then(Value::as_u64).and_then(|s| s.checked_mul(1000)) })
    }).collect();
    let at_ms = super::transcript::stamp(event);
    (!windows.is_empty() && at_ms > 0).then_some(Rates { windows, at_ms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reader_uses_event_time_caches_and_expires_windows() {
        let root = std::env::temp_dir().join(format!("cue-codex-usage-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let day = root.join("2026/11/17");
        std::fs::create_dir_all(&day).unwrap();
        let event = |stamp, pct| json!({"type":"event_msg", "timestamp":stamp, "payload": {
            "type":"token_count", "rate_limits": {
                "primary":{"used_percent":pct,"window_minutes":300,"resets_at":1794909030u64}
            }
        }}).to_string();
        let first = event("2026-11-17T05:22:22.466Z", 30);
        let second = event("2026-11-17T05:22:24.951Z", 31);
        let path = day.join("new.jsonl");
        std::fs::write(&path, format!("{first}\n{{\"partial\":" )).unwrap();
        std::fs::write(day.join("old.jsonl"), event("2026-11-16T05:22:22.466Z", 12)).unwrap();
        let mut reader = Reader::default();
        assert_eq!(reader.read(&root, 0).unwrap().windows[0].pct, 30.0);
        assert_eq!(reader.read(&root, 0).unwrap().windows[0].pct, 30.0);
        std::fs::write(&path, format!("{first}\n{second}\n")).unwrap();
        assert_eq!(reader.read(&root, 0).unwrap().windows[0].pct, 31.0);
        assert!(reader.read(&root, 1794909030000).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_account_windows_and_ignores_other_buckets() {
        let mut event = json!({"type":"event_msg", "timestamp":"2026-11-17T05:22:22.466Z", "payload": {
            "type":"token_count", "rate_limits":{"limit_id":"codex",
                "primary":{"used_percent":30.0,"window_minutes":300,"resets_at":1794909030u64},
                "secondary":{"used_percent":5.0,"window_minutes":10080,"resets_at":1795495830u64}}
        }});
        let rates = parse(&event).unwrap();
        assert_eq!(rates.windows[0], Window { pct: 30.0, minutes: 300, resets_ms: Some(1794909030000) });
        assert_eq!(rates.windows[1].minutes, 10080);
        event["payload"]["rate_limits"]["limit_id"] = json!("codex_other");
        assert!(parse(&event).is_none());
        event["payload"]["rate_limits"]["limit_id"] = Value::Null;
        event["payload"]["rate_limits"]["primary"] = Value::Null;
        assert_eq!(parse(&event).unwrap().windows.len(), 1);
        event["payload"]["rate_limits"]["secondary"]["used_percent"] = json!(-1);
        assert!(parse(&event).is_none());
    }
}
