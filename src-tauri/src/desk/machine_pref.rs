//! Where a Desk's new sessions start (#597): this Mac, or one of your machines, and the folder of the Desk's clone of
//! its repository on each machine it has used. Kept in Cue's settings as `project.machine` (Desk -> { machine,
//! folders }), never in the project. You set the machine (and its folder) on the Desk's settings screen; Cue adds a
//! folder each time a session of the Desk starts on a machine. A machine is the Desk's once it has a folder there: the
//! lead may then start items on it without asking you again (desk/hub/landing/remote_item.rs `route_on`).

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// A Desk's machine setting: the machine its new sessions start on ("" for this Mac), and its clone's folder on each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pref {
    pub machine: String,
    pub folders: BTreeMap<String, String>,
    /// The machines you ticked for the Desk (#661, "" for this Mac): its new items spread over them, each to the one
    /// with the fewest of its items at work. Empty: only `machine`.
    pub machines: Vec<String>,
    /// The most of the Desk's items at work on each machine at once (#711), as you set them; one not set has
    /// `AT_ONCE_DEFAULT`. This Mac isn't one: its items wait for a free copy there (desk/lands.rs Items at once).
    pub at_once: BTreeMap<String, u32>,
}

/// The most items a Desk may have at work on one machine at once (#711): the range you can set, and the number for a
/// machine you haven't set.
pub const AT_ONCE: (u32, u32) = (1, 9);
pub const AT_ONCE_DEFAULT: u32 = 3;

impl Pref {
    /// The folder of the Desk's clone on `machine`, if it has used it.
    pub fn folder(&self, machine: &str) -> Option<&str> {
        self.folders.get(machine).map(String::as_str)
    }

    /// Where the Desk's new sessions may start, in the order you ticked them: the ticked machines, else `machine`.
    pub fn spread(&self) -> Vec<String> {
        if self.machines.is_empty() { vec![self.machine.clone()] } else { self.machines.clone() }
    }

    /// The most of the Desk's items at work on `machine` at once (#711).
    pub fn at_once(&self, machine: &str) -> u32 {
        self.at_once.get(machine).copied().unwrap_or(AT_ONCE_DEFAULT)
    }
}

/// Desk `desk`'s setting in config `c`.
pub fn of_in(c: &Value, desk: &str) -> Pref {
    let Some(v) = c.pointer("/project/machine").and_then(|m| m.get(desk)) else { return Pref::default() };
    let machine = v.get("machine").and_then(Value::as_str).unwrap_or("").to_string();
    let folders = v.get("folders").and_then(Value::as_object).map(|m| m.iter().filter_map(|(k, f)| Some((k.clone(), f.as_str()?.to_string()))).collect()).unwrap_or_default();
    let machines = v.get("machines").and_then(Value::as_array).map(|a| a.iter().filter_map(|m| m.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let at_once = v.get("at_once").and_then(Value::as_object).map(|m| m.iter().filter_map(|(k, n)| Some((k.clone(), n.as_u64()?.clamp(AT_ONCE.0 as u64, AT_ONCE.1 as u64) as u32))).collect()).unwrap_or_default();
    Pref { machine, folders, machines, at_once }
}

/// Desk `desk`'s setting as saved.
pub fn of(desk: &str) -> Pref {
    of_in(&crate::config::load(), desk)
}

/// A folder on a machine as given: trimmed, no trailing slash; it has to be a full path there (`/…` or `~…`).
pub fn clean_folder(f: &str) -> Result<String, String> {
    let f = f.trim();
    let f = if f.len() > 1 { f.trim_end_matches('/') } else { f };
    if f.is_empty() {
        return Err("give the folder of this repository's clone on that machine".into());
    }
    if !f.starts_with('/') && !f.starts_with('~') {
        return Err(format!("the folder needs its full path on the machine ({f} isn't one)"));
    }
    if f.chars().any(|c| c.is_control()) || f.len() > 400 {
        return Err("that folder can't be used".into());
    }
    Ok(f.to_string())
}

/// The whole `project.machine` setting as it may be saved: Desk -> { machine: name, folders: { name: full path } }.
pub fn checked(value: &Value) -> Result<Value, String> {
    let map = value.as_object().ok_or("the machine setting is Desk: { machine, folders }")?;
    for v in map.values() {
        let o = v.as_object().ok_or("the machine setting is Desk: { machine, folders }")?;
        if o.get("machine").is_some_and(|m| !m.is_string()) {
            return Err("a Desk's machine is its name".into());
        }
        if o.get("machines").is_some_and(|m| !m.as_array().is_some_and(|a| a.iter().all(Value::is_string))) {
            return Err("a Desk's machines are their names".into());
        }
        if let Some(ns) = o.get("at_once") {
            let ns = ns.as_object().ok_or("a Desk's items at once are machine: number")?;
            if ns.values().any(|n| !n.as_u64().is_some_and(|n| (AT_ONCE.0 as u64..=AT_ONCE.1 as u64).contains(&n))) {
                return Err(format!("items at once on a machine is {} to {}", AT_ONCE.0, AT_ONCE.1));
            }
        }
        if let Some(fs) = o.get("folders") {
            let fs = fs.as_object().ok_or("a Desk's folders are machine: folder")?;
            for f in fs.values() {
                clean_folder(f.as_str().ok_or("a folder is a path")?)?;
            }
        }
    }
    Ok(value.clone())
}

fn write(desk: &str, f: impl FnOnce(&mut Pref)) -> Result<Pref, String> {
    let mut out = None;
    crate::config::update("project.machine", |c| {
        let mut p = of_in(c, desk);
        f(&mut p);
        let mut map = c.pointer("/project/machine").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        let folders: Map<String, Value> = p.folders.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        let mut v = json!({ "machine": p.machine, "folders": folders });
        if !p.machines.is_empty() {
            v["machines"] = json!(p.machines);
        }
        if !p.at_once.is_empty() {
            v["at_once"] = json!(p.at_once);
        }
        map[desk] = v;
        out = Some(p);
        Some(map)
    })?;
    out.ok_or_else(|| "Couldn't save where its sessions start".into())
}

/// Set where Desk `desk`'s new sessions start: `machine` ("" for this Mac), one of Cue's machines, with the folder of
/// the Desk's clone there (`folder`; needed when the Desk hasn't used that machine yet). Returns the setting as it holds.
pub fn save(desk: &str, machine: &str, folder: Option<&str>) -> Result<Pref, String> {
    let machine = machine.trim();
    if machine.is_empty() {
        return write(desk, |p| {
            p.machine.clear();
            p.machines.clear();
        });
    }
    if crate::machines::get(machine).is_none() {
        return Err(format!("Cue has no machine called {machine}"));
    }
    let given = folder.map(str::trim).filter(|f| !f.is_empty()).map(clean_folder).transpose()?;
    let known = of(desk).folder(machine).map(str::to_string);
    let Some(folder) = given.or(known) else {
        return Err(format!("give the folder of this repository's clone on {}", crate::machines::display_name(machine)));
    };
    write(desk, |p| {
        p.machine = machine.to_string();
        p.machines.clear();
        p.folders.insert(machine.to_string(), folder);
    })
}

/// Set the machines Desk `desk`'s new items spread over (#661): `list` in your order ("" for this Mac), each of Cue's
/// machines with the folder of the Desk's clone there (`folders`, needed for one the Desk hasn't used). One machine
/// is the same as `save`.
pub fn save_many(desk: &str, list: &[String], folders: &BTreeMap<String, String>) -> Result<Pref, String> {
    let mut seen: Vec<String> = vec![];
    for m in list.iter().map(|m| m.trim().to_string()) {
        if !seen.contains(&m) {
            seen.push(m);
        }
    }
    match seen.as_slice() {
        [] => return Err("tick at least one place for new sessions to start".into()),
        [one] => return save(desk, one, folders.get(one).map(String::as_str)),
        _ => {}
    }
    let known = of(desk);
    let mut found = BTreeMap::new();
    for m in seen.iter().filter(|m| !m.is_empty()) {
        if crate::machines::get(m).is_none() {
            return Err(format!("Cue has no machine called {m}"));
        }
        let given = folders.get(m).map(|f| f.trim()).filter(|f| !f.is_empty()).map(clean_folder).transpose()?;
        let Some(f) = given.or_else(|| known.folder(m).map(str::to_string)) else {
            return Err(format!("give the folder of this repository's clone on {}", crate::machines::display_name(m)));
        };
        found.insert(m.clone(), f);
    }
    write(desk, |p| {
        p.machine = seen[0].clone();
        p.machines = seen.clone();
        p.folders.extend(found);
    })
}

/// Set how many of Desk `desk`'s items may be at work on each machine at once (#711): `at_once` is machine -> number,
/// each a machine Cue has, `AT_ONCE` the range. Machines left out keep theirs.
pub fn save_at_once(desk: &str, at_once: &BTreeMap<String, u32>) -> Result<Pref, String> {
    for (m, n) in at_once {
        if crate::machines::get(m).is_none() {
            return Err(format!("Cue has no machine called {m}"));
        }
        if !(AT_ONCE.0..=AT_ONCE.1).contains(n) {
            return Err(format!("items at once on {} is {} to {}", crate::machines::display_name(m), AT_ONCE.0, AT_ONCE.1));
        }
    }
    write(desk, |p| p.at_once.extend(at_once.iter().map(|(k, v)| (k.clone(), *v))))
}

/// Remember the folder of Desk `desk`'s clone on `machine`, as a session of it started there.
pub fn remember(desk: &str, machine: &str, folder: &str) {
    if machine.is_empty() || of(desk).folder(machine) == Some(folder) {
        return;
    }
    let Ok(folder) = clean_folder(folder) else { return };
    if let Err(e) = write(desk, |p| { p.folders.insert(machine.to_string(), folder); }) {
        eprintln!("cue: desk: couldn't remember the folder on {machine}: {e}");
    }
}

/// What the window shows of Desk `desk`'s machines (`extra`: folders it knew before this setting, #500): where new
/// sessions start, and each of Cue's machines with the folder of the Desk's clone there ("" when it hasn't used it).
pub fn desk_view(desk: &str, extra: &BTreeMap<String, String>) -> Value {
    let p = of(desk);
    let machines: Vec<Value> = crate::machines::list()
        .iter()
        .map(|m| {
            let folder = p.folder(&m.name).or(extra.get(&m.name).map(String::as_str)).unwrap_or("");
            json!({ "name": m.name, "label": crate::machines::display_name(&m.name), "folder": folder, "at_once": p.at_once(&m.name) })
        })
        .collect();
    let known = |m: &str| m.is_empty() || machines.iter().any(|x| x["name"] == m);
    let picked: Vec<String> = p.spread().into_iter().filter(|m| known(m)).collect();
    let machine = if known(&p.machine) { p.machine } else { String::new() };
    json!({ "machine": machine, "picked": if picked.is_empty() { vec![String::new()] } else { picked }, "machines": machines, "at_once": { "min": AT_ONCE.0, "max": AT_ONCE.1 } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desk_with_no_setting_starts_on_this_mac_and_folders_are_full_paths() {
        assert_eq!(of_in(&json!({}), "cue"), Pref::default());
        let c = json!({ "project": { "machine": { "cue": { "machine": "box", "folders": { "box": "~/code/cue" } } } } });
        let p = of_in(&c, "cue");
        assert_eq!(p.machine, "box");
        assert_eq!(p.folder("box"), Some("~/code/cue"));
        assert_eq!(of_in(&c, "web"), Pref::default(), "another Desk's setting isn't this one's");
        assert_eq!(clean_folder(" /home/me/cue/ ").unwrap(), "/home/me/cue");
        assert!(clean_folder("code/cue").is_err(), "a folder there is a full path");
        assert!(clean_folder("").is_err());
        assert!(checked(&json!({ "cue": { "machine": "box", "folders": { "box": "cue" } } })).is_err());
        assert!(checked(&json!({ "cue": { "machine": "box", "folders": { "box": "/srv/cue" } } })).is_ok());
        // Several ticked (#661): spread over them in your order; a setting from before spreads over its one machine.
        assert_eq!(p.spread(), vec!["box".to_string()]);
        let c = json!({ "project": { "machine": { "cue": { "machine": "", "machines": ["", "box"], "folders": { "box": "/srv/cue" } } } } });
        assert_eq!(of_in(&c, "cue").spread(), vec![String::new(), "box".to_string()]);
        assert!(checked(&json!({ "cue": { "machine": "", "machines": ["", 3], "folders": {} } })).is_err());
        assert!(checked(&json!({ "cue": { "machine": "", "machines": ["", "box"], "folders": { "box": "/srv/cue" } } })).is_ok());
        // Items at once on each machine (#711): 3 when not set, kept as set, and only 1 to 9.
        assert_eq!(of_in(&c, "cue").at_once("box"), AT_ONCE_DEFAULT);
        let c = json!({ "project": { "machine": { "cue": { "machine": "box", "at_once": { "box": 1 }, "folders": { "box": "/srv/cue" } } } } });
        assert_eq!(of_in(&c, "cue").at_once("box"), 1);
        assert!(checked(&json!({ "cue": { "machine": "box", "at_once": { "box": 0 }, "folders": {} } })).is_err());
        assert!(checked(&json!({ "cue": { "machine": "box", "at_once": { "box": 2 }, "folders": {} } })).is_ok());
    }
}
