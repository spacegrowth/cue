//! Actions: a project's buttons for its developer commands (install, test, clean a cache, start a dev
//! server), usually two or three. They belong to the project (a folder, and the machine it's on for a
//! remote session), not to a session: they run any time, an agent working or not. Written down in a
//! file of yours per project in Cue's own folder, actions/mine/ (files.rs): nothing in the project, so
//! nothing is shared with its repo. Cue runs them in tmux (run.rs), so a server it started is never lost.
//!
//! Nothing from the file runs before you allow it (an agent may have written it), and a changed command
//! asks again: what you allowed is kept apart from it (actions/projects.json), per project, with what
//! each one ran, so the trust sheet can show the old command beside the new. Actions you save in Cue's
//! edit sheet are allowed by being saved. Each action's last run is kept there too. Every run is logged to actions.log in Cue's folder.
//!
//! The window hears "actions" ({folder, machine}) whenever a project's actions or their states change:
//! states are looked at every second while something runs (a machine's every 3), and the files of the
//! projects the window listed lately every 2 s.

pub mod files;
pub mod run;

pub use run::end_plain;

use files::Action;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Cue's actions prompt, when you haven't written your own (Settings → Actions).
/// `{file}` is the project's file (`mine_path`), put in when the prompt goes into the box.
pub const PROMPT: &str = "Look at this project and set up my Cue actions for it in {file} (Cue's own file, outside the repo). Read the \"about\" notes at the top of the file first and follow them. Keep the actions already there unless they're wrong. Then tell me what you added, changed or removed, and why.";

/// The actions prompt in use: yours, else Cue's.
pub fn prompt() -> String {
    crate::config::load().pointer("/actions/prompt").and_then(Value::as_str).filter(|p| !p.trim().is_empty()).unwrap_or(PROMPT).to_string()
}

/// Yours from now on ("" goes back to Cue's). The prompt now in use.
pub fn set_prompt(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.chars().count() > 4000 {
        return Err("Keep the prompt under 4000 characters".into());
    }
    crate::config::remember("actions.prompt", json!(if t == PROMPT { "" } else { t }))?;
    Ok(prompt())
}

// ---------- Cue's own record of each project ----------

/// A project's key: its machine ("" this Mac) and folder; on this Mac a repository's main checkout for
/// every worktree of it, so what you allowed in one is allowed in all.
fn key(folder: &str, machine: &str) -> String {
    format!("{machine}|{}", mine_dir(folder, machine))
}

/// The folder an action's commands are given: the top of its checkout on this Mac (a session in
/// `web-app/ui` has web-app's actions, run in web-app; a worktree's are its own, run there). On
/// another machine, the folder as given.
fn top(folder: String, machine: &Option<String>) -> String {
    if machine.as_deref().is_some_and(|m| !m.is_empty()) {
        return folder;
    }
    crate::logs::live::checkout_top(&folder).map(|t| t.to_string_lossy().to_string()).unwrap_or(folder)
}

/// The folder your actions are kept for: on this Mac a worktree's are its main checkout's (they're
/// yours for the whole project); otherwise the folder.
fn mine_dir(folder: &str, machine: &str) -> String {
    if !machine.is_empty() {
        return folder.to_string();
    }
    match crate::logs::live::repo_root(folder) {
        Some(r) if r.join(".git").is_dir() => r.to_string_lossy().to_string(),
        _ => folder.to_string(),
    }
}

/// The project's file of actions, in Cue's folder: actions/mine/<folder's name>[-<machine>]-<hash>.json.
fn mine_path(folder: &str, machine: &str) -> std::path::PathBuf {
    let dir = mine_dir(folder, machine);
    let base = dir.rsplit('/').find(|p| !p.is_empty()).unwrap_or("project");
    let tidy = |t: &str| t.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' }).collect::<String>();
    let mut name = tidy(base).trim_start_matches('.').to_string();
    if !machine.is_empty() {
        name = format!("{name}-{}", tidy(machine));
    }
    crate::server::cue_dir().join("actions").join("mine").join(format!("{name}-{:08x}.json", run::fnv(&key(folder, machine)) as u32))
}

fn store_path() -> std::path::PathBuf {
    crate::server::cue_dir().join("actions").join("projects.json")
}

/// Held while the record is read and written back, so two changes never lose one.
static STORE: Mutex<()> = Mutex::new(());

/// Every project's record: {"<machine>|<folder>": {allowed: {id: what it ran}, last: {id: {exit, secs, at}}}}.
fn store() -> Value {
    std::fs::read_to_string(store_path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

/// One project's record.
fn record(folder: &str, machine: &str) -> Value {
    let _one = crate::locking::lock(&STORE, "actions STORE");
    store()[key(folder, machine)].clone()
}

/// Change one project's record and keep it.
fn edit_record(folder: &str, machine: &str, f: impl FnOnce(&mut Value)) {
    let _one = crate::locking::lock(&STORE, "actions STORE");
    let mut all = store();
    let k = key(folder, machine);
    if !all[&k].is_object() {
        all[&k] = json!({});
    }
    f(&mut all[&k]);
    let path = store_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, serde_json::to_string_pretty(&all).unwrap_or_default() + "\n").is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

fn allow(rec: &mut Value, a: &Action) {
    if !rec["allowed"].is_object() {
        rec["allowed"] = json!({});
    }
    rec["allowed"][a.id()] = json!(a.trusted());
}

// ---------- actions.log ----------

fn log(folder: &str, machine: &str, name: &str, what: &str) {
    use std::io::Write;
    static ONE: Mutex<()> = Mutex::new(());
    let _one = crate::locking::lock(&ONE, "actions log");
    let at = crate::model::now_ms() / 1000;
    let place = if machine.is_empty() { folder.to_string() } else { format!("{machine}:{folder}") };
    let name = name.replace('\n', " ");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::server::cue_dir().join("actions.log")) {
        let _ = writeln!(f, "{at} [{place}] {name}: {}", what.replace('\n', " "));
    }
}

// ---------- reading a project ----------

/// The project's file, as text (None: not there yet). Its folder on this Mac must be there; a machine's
/// is never looked at from here (its file is on this Mac anyway).
fn read_mine(folder: &str, machine: &str) -> Result<Option<String>, String> {
    if !folder.starts_with('/') && !folder.starts_with('~') {
        return Err("that isn't a project folder".into());
    }
    if machine.is_empty() && !std::path::Path::new(folder).is_dir() {
        return Err(format!("{folder} isn't a folder"));
    }
    match std::fs::read(mine_path(folder, machine)) {
        Ok(b) => Ok(Some(String::from_utf8_lossy(&b[..b.len().min(files::FILE_MAX)]).to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// A project's actions, in the file's order, what Cue read, and the file's error (it isn't JSON).
struct Gathered {
    actions: Vec<Action>,
    text: Option<String>,
    error: Option<String>,
}

fn gather(folder: &str, machine: &str) -> Result<Gathered, String> {
    let text = read_mine(folder, machine)?;
    let (actions, error) = match text.as_deref().map(|t| files::parse(t, "local")) {
        Some(Ok(a)) => (a, None),
        Some(Err(e)) => (vec![], Some(format!("{} {e}", mine_path(folder, machine).display()))),
        None => (vec![], None),
    };
    Ok(Gathered { actions, text, error })
}

fn find(g: &Gathered, id: &str) -> Result<Action, String> {
    g.actions.iter().find(|a| a.id() == id).cloned().ok_or_else(|| "That action isn't there any more".into())
}

fn allowed(rec: &Value, a: &Action) -> bool {
    rec["allowed"][a.id()].as_str() == Some(a.trusted().as_str())
}

// ---------- states ----------

/// Sessions being stopped or restarted: their state says `busy` meanwhile.
static BUSY: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn busy(name: &str, on: bool) {
    let mut b = crate::locking::lock(&BUSY, "actions BUSY");
    let set = b.get_or_insert_with(HashSet::new);
    if on {
        set.insert(name.to_string());
    } else {
        set.remove(name);
    }
}

fn is_busy(name: &str) -> bool {
    let b = crate::locking::lock(&BUSY, "actions BUSY");
    b.as_ref().is_some_and(|s| s.contains(name))
}

/// An action's state: from its session when there is one, else its last run as Cue kept it.
fn state_of(a: &Action, folder: &str, machine: &str, seen: &[run::Seen], rec: &Value) -> Value {
    let name = run::session_name(folder, machine, &a.id(), &a.name);
    let mut s = match seen.iter().find(|s| s.session == name) {
        Some(s) if s.running || s.exit.is_some() || s.end.is_some() => s.state(),
        _ => {
            let last = &rec["last"][a.id()];
            if last.is_object() {
                json!({ "running": false, "exit": last["exit"], "secs": last["secs"], "at": last["at"] })
            } else {
                json!({ "running": false })
            }
        }
    };
    if is_busy(&name) {
        s["busy"] = json!(true);
    }
    s
}

/// An action's run that ended: kept as its last run (once), and logged.
fn note_ended(s: &run::Seen) {
    if s.running || s.id.is_empty() {
        return;
    }
    let secs = match (s.t0, s.end) {
        (Some(a), Some(b)) => Some(b.saturating_sub(a)),
        _ => None,
    };
    let at = s.end.map(|e| e * 1000).unwrap_or_else(crate::model::now_ms);
    let mut new = false;
    edit_record(&s.folder, &s.machine, |rec| {
        let had = &rec["last"][&s.id];
        if had["at"].as_u64() == Some(at) && had["exit"] == json!(s.exit) || (s.end.is_none() && had.is_object() && had["exit"] == json!(s.exit)) {
            return;
        }
        new = true;
        if !rec["last"].is_object() {
            rec["last"] = json!({});
        }
        rec["last"][&s.id] = json!({ "exit": s.exit, "secs": secs, "at": at });
    });
    if new {
        let how = if s.stopped { "stopped".to_string() } else { s.exit.map(|e| format!("exit {e}")).unwrap_or_else(|| "ended".into()) };
        log(&s.folder, &s.machine, &s.name, &format!("{how}{}", secs.map(|n| format!(" after {n}s")).unwrap_or_default()));
    }
}

/// The project's actions for the window: {actions, file: {path, there}, error, pending, prompt}.
fn listing(folder: &str, machine: &str) -> Result<Value, String> {
    let g = gather(folder, machine)?;
    let tag = run::project_tag(folder, machine);
    let seen: Vec<run::Seen> = run::list(machine).into_iter().filter(|s| s.session.starts_with(&tag)).collect();
    for s in seen.iter().filter(|s| !s.running) {
        note_ended(s);
    }
    let rec = record(folder, machine);
    let mut actions = vec![];
    let mut pending = vec![];
    for a in &g.actions {
        let id = a.id();
        let ok = allowed(&rec, a);
        actions.push(json!({
            "id": id, "name": a.name, "run": a.run, "source": a.source, "keep": a.keep, "ask": a.ask,
            "allowed": ok, "state": state_of(a, folder, machine, &seen, &rec),
        }));
        if !ok {
            let was = rec["allowed"][&id].as_str();
            let mut p = json!({ "id": id, "name": a.name, "run": a.trusted(), "why": if was.is_some() { "changed" } else { "new" }, "source": a.source });
            if let Some(w) = was {
                p["was"] = json!(w);
            }
            pending.push(p);
        }
    }
    watch(folder, machine, &seen);
    Ok(json!({
        "actions": actions,
        "file": { "path": mine_path(folder, machine).to_string_lossy(), "there": g.text.is_some() },
        "error": g.error,
        "pending": pending,
        "prompt": prompt(),
    }))
}

// ---------- watching ----------

static APP: OnceLock<AppHandle> = OnceLock::new();

/// Tell the window a project's actions or their states changed.
fn changed(folder: &str, machine: &str) {
    if let Some(app) = APP.get() {
        let _ = app.emit("actions", json!({ "folder": folder, "machine": machine }));
    }
}

/// A project Cue keeps an eye on: listed lately (its file), or with something running.
struct Watched {
    folder: String,
    machine: String,
    listed: Instant,
    stamp: u64,
    running: HashMap<String, run::Seen>,
}

static WATCH: Mutex<Option<HashMap<String, Watched>>> = Mutex::new(None);

/// How long after the window last listed a project its files are still watched.
const LISTED_FOR: Duration = Duration::from_secs(600);

/// The project was just listed (with these sessions): watch its files, and what runs in it.
fn watch(folder: &str, machine: &str, seen: &[run::Seen]) {
    let stamp = stamp(folder, machine);
    let mut w = crate::locking::lock(&WATCH, "actions WATCH");
    let e = w.get_or_insert_with(HashMap::new).entry(key(folder, machine)).or_insert_with(|| Watched { folder: folder.into(), machine: machine.into(), listed: Instant::now(), stamp, running: HashMap::new() });
    e.listed = Instant::now();
    e.stamp = stamp;
    e.running = seen.iter().filter(|s| s.running).map(|s| (s.session.clone(), s.clone())).collect();
}

/// Something of this project's runs now (just started, or found running).
fn track(s: &run::Seen) {
    let mut w = crate::locking::lock(&WATCH, "actions WATCH");
    let e = w.get_or_insert_with(HashMap::new).entry(key(&s.folder, &s.machine)).or_insert_with(|| Watched { folder: s.folder.clone(), machine: s.machine.clone(), listed: Instant::now() - LISTED_FOR, stamp: 0, running: HashMap::new() });
    e.running.insert(s.session.clone(), s.clone());
}

/// The project's file's size and time, as one number: it changes when the file does.
fn stamp(folder: &str, machine: &str) -> u64 {
    match std::fs::metadata(mine_path(folder, machine)) {
        Ok(m) => {
            let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis()).unwrap_or(0);
            run::fnv(&format!("{}:{t}", m.len()))
        }
        Err(_) => 0,
    }
}

/// One look: the running actions' states (this Mac's every time, a machine's every third), and the
/// watched projects' files (every second time). Changes go to the window.
fn tick(n: u64) {
    let (machines, files): (HashSet<String>, Vec<(String, String, String, u64)>) = {
        let w = crate::locking::lock(&WATCH, "actions WATCH");
        let all = w.as_ref().map(|m| m.values().collect::<Vec<_>>()).unwrap_or_default();
        let machines = all.iter().filter(|e| !e.running.is_empty()).map(|e| e.machine.clone()).filter(|m| m.is_empty() || n % 3 == 0).collect();
        let files = if n % 2 == 0 { all.iter().filter(|e| e.listed.elapsed() < LISTED_FOR).map(|e| (key(&e.folder, &e.machine), e.folder.clone(), e.machine.clone(), e.stamp)).collect() } else { vec![] };
        (machines, files)
    };
    let mut events: Vec<(String, String)> = vec![];
    for m in machines {
        let seen = run::list(&m);
        let mut ended: Vec<run::Seen> = vec![];
        let mut ports: Vec<(String, String)> = vec![];
        {
            let mut w = crate::locking::lock(&WATCH, "actions WATCH");
            for e in w.as_mut().map(|x| x.values_mut()).into_iter().flatten().filter(|e| e.machine == m && !e.running.is_empty()) {
                let mut moved = false;
                let tag = run::project_tag(&e.folder, &e.machine);
                for (name, before) in e.running.clone() {
                    match seen.iter().find(|s| s.session == name) {
                        Some(now) if now.running => {
                            if now.port.is_none() && before.port.is_none() {
                                ports.push((m.clone(), name.clone()));
                            }
                            if now.port != before.port {
                                moved = true;
                                e.running.insert(name, now.clone());
                            }
                        }
                        Some(now) => {
                            ended.push(now.clone());
                            e.running.remove(&name);
                            moved = true;
                        }
                        None => {
                            ended.push(run::Seen { running: false, ..before });
                            e.running.remove(&name);
                            moved = true;
                        }
                    }
                }
                // Started elsewhere meanwhile (another window's click, a Cue before this one).
                let new: Vec<&run::Seen> = seen.iter().filter(|s| s.running && s.session.starts_with(&tag) && !e.running.contains_key(&s.session)).collect();
                for s in new {
                    e.running.insert(s.session.clone(), s.clone());
                    moved = true;
                }
                if moved {
                    events.push((e.folder.clone(), e.machine.clone()));
                }
            }
        }
        for s in &ended {
            note_ended(s);
        }
        for (m, name) in ports {
            if run::find_port(&m, &name).is_some() {
                let s = seen.iter().find(|s| s.session == name);
                if let Some(s) = s {
                    events.push((s.folder.clone(), s.machine.clone()));
                }
            }
        }
    }
    for (k, folder, machine, before) in files {
        let now = stamp(&folder, &machine);
        if now != before {
            let mut w = crate::locking::lock(&WATCH, "actions WATCH");
            if let Some(e) = w.as_mut().and_then(|x| x.get_mut(&k)) {
                e.stamp = now;
            }
            drop(w);
            events.push((folder, machine));
        }
    }
    events.sort();
    events.dedup();
    for (f, m) in events {
        changed(&f, &m);
    }
}

/// From Cue's start: the window hears about actions, and the ones still running from before are watched.
pub fn start(app: AppHandle) {
    if APP.set(app).is_err() {
        return;
    }
    std::thread::spawn(|| {
        for s in run::list("").into_iter().filter(|s| s.running && !s.id.is_empty()) {
            track(&s);
        }
        let mut n = 0u64;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            n += 1;
            tick(n);
        }
    });
}

// ---------- what the window calls ----------

/// Run (or restart) action `id`, allowed or refused.
fn run_one(folder: &str, machine: &str, id: &str) -> Result<Value, String> {
    let g = gather(folder, machine)?;
    let a = find(&g, id)?;
    if !allowed(&record(folder, machine), &a) {
        return Err("Allow it first".into());
    }
    let name = run::session_name(folder, machine, id, &a.name);
    busy(&name, true);
    changed(folder, machine);
    let started = run::start(&run::Spec { folder, machine, id, name: &a.name, run: &a.run, keep: a.keep });
    busy(&name, false);
    let s = started?;
    log(folder, machine, &a.name, &format!("run {}", a.run));
    track(&s);
    changed(folder, machine);
    Ok(s.state())
}

fn stop_one(folder: &str, machine: &str, id: &str) -> Result<Value, String> {
    let g = gather(folder, machine)?;
    let a = find(&g, id)?;
    let name = run::session_name(folder, machine, id, &a.name);
    busy(&name, true);
    changed(folder, machine);
    let r = run::stop(folder, machine, &name);
    busy(&name, false);
    r?;
    let seen = run::list(machine);
    if let Some(s) = seen.iter().find(|s| s.session == name) {
        note_ended(s);
    }
    changed(folder, machine);
    Ok(state_of(&a, folder, machine, &seen, &record(folder, machine)))
}

/// Write the project's file (whole, put in place at once).
fn write_mine(folder: &str, machine: &str, text: &str) -> Result<(), String> {
    let path = mine_path(folder, machine);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.cue-tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// The project's file, made (with Cue's notes) when it isn't there yet, for an agent to fill: its path.
fn ensure_mine(folder: &str, machine: &str) -> Result<String, String> {
    if read_mine(folder, machine)?.is_none() {
        write_mine(folder, machine, &files::fresh(folder))?;
    }
    Ok(mine_path(folder, machine).to_string_lossy().to_string())
}

/// An action as the edit sheet sends it.
#[derive(serde::Deserialize, Debug, Default)]
pub struct Edited {
    pub name: String,
    pub run: String,
    #[serde(default)]
    pub keep: bool,
    #[serde(default)]
    pub ask: bool,
}

/// Save an action from Cue's edit sheet into the project's file, in place of `old_id`. Saved is allowed.
fn save(folder: &str, machine: &str, e: Edited, old_id: Option<&str>) -> Result<Value, String> {
    let name = e.name.split_whitespace().collect::<Vec<_>>().join(" ");
    let run = e.run.trim().to_string();
    if name.is_empty() || name.chars().count() > 60 {
        return Err("Give it a name (up to 60 characters)".into());
    }
    if run.is_empty() || run.contains('\n') || run.len() > 2000 {
        return Err("The command is one line".into());
    }
    let a = Action { source: "local".into(), name, run, keep: e.keep, ask: e.ask };
    let g = gather(folder, machine)?;
    let old = old_id.and_then(|o| o.split_once(':')).map(|(_, n)| n);
    let text = files::with_action(g.text.as_deref(), folder, old, &a)?;
    write_mine(folder, machine, &text)?;
    edit_record(folder, machine, |rec| {
        if let Some(was) = old_id.filter(|w| *w != a.id()) {
            if let Some(m) = rec["allowed"].as_object_mut() {
                m.remove(was);
            }
        }
        allow(rec, &a);
    });
    log(folder, machine, &a.name, &format!("saved: {}", a.run));
    changed(folder, machine);
    listing(folder, machine)
}

/// Delete an action from the file.
fn delete(folder: &str, machine: &str, id: &str) -> Result<Value, String> {
    let g = gather(folder, machine)?;
    let a = find(&g, id)?;
    let t = files::without_action(g.text.as_deref(), folder, &a.name)?;
    write_mine(folder, machine, &t)?;
    edit_record(folder, machine, |rec| {
        if let Some(m) = rec["allowed"].as_object_mut() {
            m.remove(id);
        }
    });
    log(folder, machine, &a.name, "deleted");
    changed(folder, machine);
    listing(folder, machine)
}

/// Every running action, across projects: this Mac's, and those on machines Cue has seen actions on.
fn running() -> Vec<Value> {
    let machines: HashSet<String> = {
        let w = crate::locking::lock(&WATCH, "actions WATCH");
        w.as_ref().map(|m| m.values().map(|e| e.machine.clone()).filter(|m| !m.is_empty()).collect()).unwrap_or_default()
    };
    let mut out = vec![];
    for m in std::iter::once(String::new()).chain(machines) {
        for s in run::list(&m).into_iter().filter(|s| s.running && !s.id.is_empty()) {
            track(&s);
            // Its project by the folder it runs in (model.rs project_at), never by a session: it's on its
            // project's chip with every session closed. And which checkout, for two of a repository's
            // worktrees running at once: its branch (this Mac's) and folder's name.
            let project = crate::model::project_at(&s.folder, &s.machine);
            let branch = if s.machine.is_empty() { crate::logs::live::branch(&s.folder) } else { String::new() };
            out.push(json!({ "folder": s.folder, "machine": s.machine, "project": project, "branch": branch, "checkout": crate::model::folder_of(&s.folder), "id": s.id, "name": s.name, "port": s.port, "since": s.start_ms }));
        }
    }
    out
}

fn mach(m: Option<String>) -> String {
    m.unwrap_or_default()
}

/// Blocking work off the window's thread.
async fn off<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// The project's actions: {actions: [Action], file: {path, there}, error, pending, prompt}.
#[tauri::command]
pub async fn actions_list(folder: String, machine: Option<String>) -> Result<Value, String> {
    let folder = top(folder, &machine);
    off(move || listing(&folder, &mach(machine))).await
}

/// Allow these (as they are now), from the trust sheet. The list again.
#[tauri::command]
pub async fn actions_allow(folder: String, machine: Option<String>, ids: Vec<String>) -> Result<Value, String> {
    let folder = top(folder, &machine);
    off(move || {
        let m = mach(machine);
        let g = gather(&folder, &m)?;
        let chosen: Vec<&Action> = g.actions.iter().filter(|a| ids.contains(&a.id())).collect();
        edit_record(&folder, &m, |rec| chosen.iter().for_each(|a| allow(rec, a)));
        for a in &chosen {
            log(&folder, &m, &a.name, &format!("allowed: {}", a.trusted()));
        }
        changed(&folder, &m);
        listing(&folder, &m)
    })
    .await
}

/// Run it (a running one restarts). Its state.
#[tauri::command]
pub async fn actions_run(folder: String, machine: Option<String>, id: String) -> Result<Value, String> {
    let folder = top(folder, &machine);
    off(move || run_one(&folder, &mach(machine), &id)).await
}

#[tauri::command]
pub async fn actions_restart(folder: String, machine: Option<String>, id: String) -> Result<Value, String> {
    let folder = top(folder, &machine);
    off(move || run_one(&folder, &mach(machine), &id)).await
}

#[tauri::command]
pub async fn actions_stop(folder: String, machine: Option<String>, id: String) -> Result<Value, String> {
    let folder = top(folder, &machine);
    off(move || stop_one(&folder, &mach(machine), &id)).await
}

/// Its output's last `lines` lines (200 when not said), plain text.
#[tauri::command]
pub async fn actions_log(folder: String, machine: Option<String>, id: String, lines: Option<usize>) -> Result<String, String> {
    let folder = top(folder, &machine);
    off(move || {
        let m = mach(machine);
        let a = find(&gather(&folder, &m)?, &id)?;
        run::log(&m, &run::session_name(&folder, &m, &id, &a.name), lines.unwrap_or(200).min(5000))
    })
    .await
}

/// Its tmux session, to open its output as a terminal: {tmux, machine}.
#[tauri::command]
pub async fn actions_attach_id(folder: String, machine: Option<String>, id: String) -> Result<Value, String> {
    let folder = top(folder, &machine);
    off(move || {
        let m = mach(machine);
        let a = find(&gather(&folder, &m)?, &id)?;
        Ok(json!({ "tmux": run::session_name(&folder, &m, &id, &a.name), "machine": m }))
    })
    .await
}

/// Its output as a terminal tab (⌘J): a tmux client attached to its session, streamed as "term" events
/// under `sid` (the window's, "shell:act-…"), written to, resized and closed with term_write /
/// term_resize / term_close as a shell's tab is. Closing the tab ends only the client: the action and its
/// session carry on (`sid` is never one of the shells Cue keeps, so term_close doesn't end a session).
/// Its output's generation.
#[tauri::command]
pub async fn actions_term_open(app: AppHandle, folder: String, machine: Option<String>, id: String, sid: String, cols: u16, rows: u16) -> Result<u64, String> {
    let folder = top(folder, &machine);
    off(move || {
        let plain = sid.strip_prefix("shell:act-").is_some_and(|n| !n.is_empty() && n.len() <= 80 && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        if !plain {
            return Err(format!("“{sid}” isn't an action's tab"));
        }
        let m = mach(machine);
        let a = find(&gather(&folder, &m)?, &id)?;
        let name = run::session_name(&folder, &m, &id, &a.name);
        if !run::list(&m).iter().any(|s| s.session == name && !s.no_tmux) {
            return Err("It hasn't run yet".into());
        }
        if m.is_empty() {
            crate::terminal::term::attach_local(&sid, &name, cols, rows, move |v| {
                let _ = app.emit("term", v);
            })
        } else {
            crate::terminal::term::open_attached(app, &sid, &m, &name, cols, rows)
        }
    })
    .await
}

/// Save one from the edit sheet, in place of `old_id`. The list again.
#[tauri::command]
pub async fn actions_save(folder: String, machine: Option<String>, action: Edited, old_id: Option<String>) -> Result<Value, String> {
    let folder = top(folder, &machine);
    off(move || save(&folder, &mach(machine), action, old_id.as_deref())).await
}

/// The project's file for an agent to fill (made, with Cue's notes, when it isn't there): its path.
#[tauri::command]
pub async fn actions_file(folder: String, machine: Option<String>) -> Result<String, String> {
    let folder = top(folder, &machine);
    off(move || ensure_mine(&folder, &mach(machine))).await
}

#[tauri::command]
pub async fn actions_delete(folder: String, machine: Option<String>, id: String) -> Result<Value, String> {
    let folder = top(folder, &machine);
    off(move || delete(&folder, &mach(machine), &id)).await
}

/// Every running action: [{folder, machine, project, branch, checkout, id, name, port, since}].
#[tauri::command]
pub async fn actions_running() -> Result<Vec<Value>, String> {
    off(|| Ok(running())).await
}

/// The project a folder belongs to (+ New's Project line): {name, own, how: "put"|"repo-says"|"repo"|
/// "folder", key (where putting it in a project keeps it)}. On another machine, by the folders you put in alone.
#[tauri::command]
pub async fn project_for_folder(folder: String, machine: Option<String>) -> Result<Value, String> {
    off(move || {
        let m = mach(machine);
        let folder = folder.trim().trim_end_matches('/').to_string();
        let folder = match (m.is_empty(), folder.strip_prefix('~'), std::env::var("HOME")) {
            (true, Some(rest), Ok(home)) => format!("{home}{rest}"),
            _ => folder,
        };
        if !folder.starts_with('/') {
            return Err("that isn't a folder".into());
        }
        let (name, how) = crate::model::project_how(&folder, &m);
        Ok(json!({ "name": name, "own": crate::model::own_project(&folder, &m), "how": how, "key": crate::model::put_key(&folder, &m) }))
    })
    .await
}

/// Settings → Actions: your actions prompt ("" is Cue's). The prompt now in use.
#[tauri::command]
pub async fn actions_prompt_set(text: String) -> Result<String, String> {
    off(move || set_prompt(&text)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A private Cue folder and a project folder in it, for one test (others wait).
    struct Scratch {
        home: std::path::PathBuf,
        project: String,
        _g: std::sync::MutexGuard<'static, ()>,
    }
    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let g = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let home = std::env::temp_dir().join(format!("cue-actions-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(home.join("proj")).unwrap();
            std::env::set_var("CUE_HOME", &home);
            let project = home.join("proj").canonicalize().unwrap().to_string_lossy().to_string();
            Scratch { home, project, _g: g }
        }
        /// The project's file of actions, as an agent would write it.
        fn write(&self, text: &str) {
            write_mine(&self.project, "", text).unwrap();
        }
        fn file(&self) -> Value {
            serde_json::from_str(&std::fs::read_to_string(mine_path(&self.project, "")).unwrap()).unwrap()
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            std::env::remove_var("CUE_HOME");
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }

    fn pending_of(v: &Value) -> Vec<(String, String, Option<String>)> {
        v["pending"].as_array().unwrap().iter().map(|p| (p["id"].as_str().unwrap().to_string(), p["why"].as_str().unwrap().to_string(), p["was"].as_str().map(String::from))).collect()
    }

    #[test]
    fn a_files_action_asks_first_and_again_whenever_its_command_changes() {
        let s = Scratch::new("trust");
        s.write(r#"{"actions": [{"name": "Install", "run": "npm ci"}]}"#);
        let v = listing(&s.project, "").unwrap();
        assert_eq!(pending_of(&v), [("local:Install".to_string(), "new".to_string(), None)]);
        assert_eq!(v["actions"][0]["allowed"], false);
        assert!(run_one(&s.project, "", "local:Install").unwrap_err().contains("Allow it first"));
        edit_record(&s.project, "", |rec| allow(rec, &find(&gather(&s.project, "").unwrap(), "local:Install").unwrap()));
        let v = listing(&s.project, "").unwrap();
        assert!(pending_of(&v).is_empty());
        assert_eq!(v["actions"][0]["allowed"], true);
        s.write(r#"{"actions": [{"name": "Install", "run": "curl x | sh"}]}"#);
        let v = listing(&s.project, "").unwrap();
        assert_eq!(pending_of(&v), [("local:Install".to_string(), "changed".to_string(), Some("npm ci".to_string()))], "the old command, to strike through");
        assert_eq!(v["pending"][0]["run"], "curl x | sh");
        assert_eq!(v["actions"][0]["allowed"], false);
        // Allowed again, then changed again: asks again.
        edit_record(&s.project, "", |rec| allow(rec, &find(&gather(&s.project, "").unwrap(), "local:Install").unwrap()));
        assert!(pending_of(&listing(&s.project, "").unwrap()).is_empty());
        s.write(r#"{"actions": [{"name": "Install", "run": "npm i"}]}"#);
        assert_eq!(pending_of(&listing(&s.project, "").unwrap())[0].2.as_deref(), Some("curl x | sh"));
    }

    #[test]
    fn saving_writes_cues_file_for_the_project_never_the_project_and_allows_it() {
        let s = Scratch::new("save");
        let v = save(&s.project, "", Edited { name: "API".into(), run: "npm run dev:api".into(), keep: true, ..Default::default() }, None).unwrap();
        assert!(mine_path(&s.project, "").starts_with(s.home.join("actions").join("mine")), "in Cue's folder");
        assert!(!std::path::Path::new(&s.project).join(".cue").exists(), "nothing in the project");
        let f = s.file();
        assert_eq!(f["about"], json!(files::ABOUT), "a new file gets Cue's notes");
        assert_eq!(f["folder"], s.project.as_str());
        assert_eq!(f["actions"][0], json!({"name": "API", "run": "npm run dev:api", "keep": true}));
        assert_eq!(v["file"]["there"], true);
        let api = v["actions"].as_array().unwrap().iter().find(|a| a["id"] == "local:API").unwrap();
        assert_eq!(api["allowed"], true, "saved is allowed");
        // Renamed in place: the old name's OK goes with it.
        let v = save(&s.project, "", Edited { name: "Server".into(), run: "npm run dev:api".into(), keep: true, ..Default::default() }, Some("local:API")).unwrap();
        assert_eq!(v["actions"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect::<Vec<_>>(), ["local:Server"]);
        assert!(record(&s.project, "")["allowed"]["local:API"].is_null());
        assert!(save(&s.project, "", Edited { name: "x".into(), run: "a\nb".into(), ..Default::default() }, None).is_err());
        let v = delete(&s.project, "", "local:Server").unwrap();
        assert!(v["actions"].as_array().unwrap().is_empty());
        // Another project (a machine's folder of the same path too) has its own file.
        assert_ne!(mine_path(&s.project, ""), mine_path(&s.project, "box"));
        assert_ne!(mine_path(&s.project, ""), mine_path(&format!("{}/x", s.project), ""));
    }

    #[test]
    fn asking_the_agent_makes_the_file_with_cues_notes_once() {
        let s = Scratch::new("ensure");
        let v = listing(&s.project, "").unwrap();
        assert_eq!(v["file"]["there"], false);
        let path = ensure_mine(&s.project, "").unwrap();
        assert_eq!(path, v["file"]["path"].as_str().unwrap());
        assert_eq!(s.file()["actions"], json!([]));
        s.write(r#"{"actions": [{"name": "T", "run": "t"}]}"#);
        ensure_mine(&s.project, "").unwrap();
        assert_eq!(s.file()["actions"][0]["name"], "T", "never written over");
        assert!(PROMPT.contains("{file}"));
    }

    #[test]
    fn the_prompt_is_cues_until_you_write_your_own() {
        let _s = Scratch::new("prompt");
        assert_eq!(prompt(), PROMPT);
        assert_eq!(set_prompt("  Set up actions.  ").unwrap(), "Set up actions.");
        assert_eq!(crate::config::effective()["actions"]["prompt"], "Set up actions.");
        assert_eq!(set_prompt("").unwrap(), PROMPT);
    }

    fn tmux_here(args: &[&str]) -> String {
        std::process::Command::new(crate::terminal::focus::tmux_bin()).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
    }

    /// A real run through tmux in a scratch folder (skipped without tmux): a one-off ends with its exit
    /// code and its output readable; a server keeps running until it's stopped.
    #[test]
    fn an_action_runs_in_tmux_and_is_stopped() {
        if !crate::terminal::focus::tmux_installed() {
            eprintln!("no tmux: skipped");
            return;
        }
        let s = Scratch::new("tmux");
        s.write(r#"{"actions": [{"name": "Once", "run": "echo hi; exit 3"}, {"name": "Wait", "run": "echo 'Listening on 127.0.0.1:4321'; sleep 30", "keep": true}]}"#);
        let all: Vec<Action> = gather(&s.project, "").unwrap().actions;
        edit_record(&s.project, "", |rec| all.iter().for_each(|a| allow(rec, a)));
        let names: Vec<String> = all.iter().map(|a| run::session_name(&s.project, "", &a.id(), &a.name)).collect();
        let result = std::panic::catch_unwind(|| {
            let st = run_one(&s.project, "", "local:Once").unwrap();
            assert_eq!(st["running"], true);
            let mut done = Value::Null;
            for _ in 0..150 {
                let v = listing(&s.project, "").unwrap();
                let a = v["actions"][0]["state"].clone();
                if a["running"] == false && a["exit"].is_i64() {
                    done = a;
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            assert_eq!(done["exit"], 3, "{done}");
            assert!(done["at"].as_u64().is_some());
            let log = run::log("", &names[0], 50).unwrap();
            assert!(log.lines().any(|l| l == "hi"), "{log}");
            assert!(!log.contains("Pane is dead"));
            // Kept as its last run, and logged.
            assert_eq!(record(&s.project, "")["last"]["local:Once"]["exit"], 3);
            let logged = std::fs::read_to_string(s.home.join("actions.log")).unwrap_or_default();
            assert!(logged.contains("Once: run echo hi; exit 3") && logged.contains("Once: exit 3"), "{logged}");
            // Run again: a fresh run in the same session.
            run_one(&s.project, "", "local:Once").unwrap();

            let st = run_one(&s.project, "", "local:Wait").unwrap();
            assert_eq!(st["running"], true);
            let mut port = None;
            for _ in 0..100 {
                port = run::find_port("", &names[1]);
                if port.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            assert_eq!(port, Some(4321));
            let v = listing(&s.project, "").unwrap();
            assert_eq!(v["actions"][1]["state"]["running"], true);
            assert_eq!(v["actions"][1]["state"]["port"], 4321);
            assert!(running().iter().any(|r| r["id"] == "local:Wait" && r["folder"] == s.project.as_str() && r["port"] == 4321));
            let st = stop_one(&s.project, "", "local:Wait").unwrap();
            assert_eq!(st["running"], false, "{st}");
            assert!(!running().iter().any(|r| r["id"] == "local:Wait"));
        });
        for n in &names {
            tmux_here(&["kill-session", "-t", &format!("={n}")]);
        }
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
