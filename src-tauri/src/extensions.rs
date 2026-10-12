//! Extensions that bring their own page. Each is a folder in <Cue's folder>/extensions/<name>/
//! with a manifest.json and its HTML, served at cueext://localhost/<name>/… and shown in a sandboxed
//! frame. The page talks to Cue only through messages to the window, which checks the manifest's
//! capabilities. The one thing an extension can have Cue do outside its page is read git (git.read).

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use crate::safe_git;
use tauri::http::{Request, Response};

/// What an extension page may load and do: its own files and Cue's helper, nothing from the network.
const PAGE_CSP: &str = "default-src 'none'; script-src cueext://localhost; style-src cueext://localhost 'unsafe-inline'; img-src cueext://localhost data:; font-src cueext://localhost; connect-src 'none'; form-action 'none'";
const SDK_JS: &str = include_str!("../../ui/ext-sdk/cue.js");
const SDK_CSS: &str = include_str!("../../ui/ext-sdk/cue.css");

/// The extensions that come with Cue: in the app (Contents/Resources/extensions), so they update with it.
/// A development build run from this checkout uses the checkout's extensions/. CUE_BUILTIN_EXTENSIONS
/// overrides it (empty: none; tests use that).
pub fn builtin_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CUE_BUILTIN_EXTENSIONS") {
        return Some(PathBuf::from(p)).filter(|p| !p.as_os_str().is_empty() && p.is_dir());
    }
    let exe = std::env::current_exe().ok()?;
    let bundled = exe.parent().map(|d| d.join("../Resources/extensions")).filter(|p| p.is_dir());
    #[cfg(debug_assertions)]
    let r = bundled.or_else(|| Some(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../extensions"))).filter(|p| p.is_dir()));
    #[cfg(not(debug_assertions))]
    let r = bundled;
    r
}

/// Where extension `name` lives: your extensions folder first (a copy there takes the place of one that
/// comes with Cue), else Cue's own. And whether it's Cue's own.
fn dir_of(name: &str) -> Option<(PathBuf, bool)> {
    if !valid_name(name) {
        return None;
    }
    let mine = root().join(name);
    if mine.join("manifest.json").is_file() {
        return Some((mine, false));
    }
    let b = builtin_root()?.join(name);
    b.join("manifest.json").is_file().then_some((b, true))
}

/// Changes whenever any of the extension's files does (their names, sizes and times): its page reloads then.
fn stamp(dir: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut todo = vec![dir.to_path_buf()];
    let mut seen = 0;
    while let Some(d) = todo.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&d).into_iter().flatten().flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if e.file_name().to_string_lossy().starts_with('.') || seen > 2000 {
                continue;
            }
            seen += 1;
            let Ok(md) = e.metadata() else { continue };
            if md.is_dir() {
                todo.push(e.path());
            } else {
                e.path().strip_prefix(dir).ok().hash(&mut h);
                md.len().hash(&mut h);
                md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|t| t.as_millis()).hash(&mut h);
            }
        }
    }
    format!("{:x}", h.finish())
}

pub fn root() -> PathBuf {
    crate::server::cue_dir().join("extensions")
}

fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn manifest(name: &str) -> Option<Value> {
    if !valid_name(name) {
        return None;
    }
    let text = std::fs::read_to_string(dir_of(name)?.0.join("manifest.json")).ok()?;
    let mut m: Value = serde_json::from_str(&text).ok()?;
    m["name"] = json!(name);
    if m["entry"].as_str().is_none() {
        m["entry"] = json!("index.html");
    }
    Some(m)
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect()
}

/// What you approved for `name`, kept in Cue's own settings (never in the extension's folder, so an
/// extension can't approve itself): {"on": bool, "capabilities": [...]}.
fn approval(name: &str) -> Value {
    crate::config::load()["extensions"][name].clone()
}

/// The manifest with where it stands: `on` (you switched it on), what it asks for that you haven't
/// approved yet (`pending_*`: an update that asks for more), and `enabled` (on, and nothing pending).
fn with_status(mut m: Value) -> Value {
    let name = m["name"].as_str().unwrap_or("").to_string();
    let a = approval(&name);
    let on = a["on"].as_bool().unwrap_or(false);
    let (caps, progs) = (strings(&a["capabilities"]), strings(&a["programs"]));
    let pending_caps: Vec<String> = strings(&m["capabilities"]).into_iter().filter(|c| !caps.contains(c)).collect();
    let pending_programs: Vec<String> = strings(&m["programs"]).into_iter().filter(|p| !progs.contains(p)).collect();
    m["on"] = json!(on);
    m["enabled"] = json!(on && pending_caps.is_empty() && pending_programs.is_empty());
    m["pending_capabilities"] = json!(pending_caps);
    m["pending_programs"] = json!(pending_programs);
    if let Some((dir, builtin)) = dir_of(&name) {
        m["folder"] = json!(dir.display().to_string());
        m["builtin"] = json!(builtin);
        // Yours, in place of one that comes with Cue.
        m["replaces_builtin"] = json!(!builtin && builtin_root().is_some_and(|b| b.join(&name).join("manifest.json").is_file()));
        m["stamp"] = json!(stamp(&dir));
    }
    m
}

pub fn list() -> Vec<Value> {
    // Yours and Cue's own, each name once (yours wins: dir_of looks there first).
    let mut names: Vec<String> = [Some(root()), builtin_root()]
        .into_iter()
        .flatten()
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    names.sort();
    names.dedup();
    let mut out: Vec<Value> = names.iter().filter_map(|n| manifest(n)).map(with_status).collect();
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    out
}

/// The manifest of an extension you've switched on and approved; None for any other.
fn enabled(name: &str) -> Option<Value> {
    let m = with_status(manifest(name)?);
    m["enabled"].as_bool().unwrap_or(false).then_some(m)
}

fn has(m: &Value, key: &str, item: &str) -> bool {
    m[key].as_array().is_some_and(|a| a.iter().any(|c| c == item))
}

fn decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "woff2" => "font/woff2",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn reply(status: u16, kind: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", kind)
        .header("Content-Security-Policy", PAGE_CSP)
        .header("Cache-Control", "no-store")
        .body(body)
        .unwrap()
}

/// The file at `rel` inside extension `name`'s folder, never outside it (no `..`, no dot-files,
/// and a symlink only if it stays within the folder).
fn file_in(name: &str, rel: &str) -> Option<PathBuf> {
    enabled(name)?;
    let dir = dir_of(name)?.0.canonicalize().ok()?;
    let mut p = dir.clone();
    for seg in rel.split('/') {
        if seg.is_empty() || seg.starts_with('.') {
            return None;
        }
        p.push(seg);
    }
    let p = p.canonicalize().ok()?;
    (p.starts_with(&dir) && p.is_file()).then_some(p)
}

/// cueext://localhost/<name>/<path>, and Cue's helper at cueext://localhost/_cue/cue.js and cue.css.
pub fn serve(req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let Some(path) = decode(req.uri().path().trim_start_matches('/')) else {
        return reply(400, "text/plain", b"bad path".to_vec());
    };
    let (name, rel) = match path.split_once('/') {
        Some((n, r)) => (n.to_string(), r.to_string()),
        None => (path.clone(), String::new()),
    };
    let (name, rel) = (name.as_str(), rel.as_str());
    if name == "_cue" {
        return match rel {
            "cue.js" => reply(200, "text/javascript; charset=utf-8", SDK_JS.as_bytes().to_vec()),
            "cue.css" => reply(200, "text/css; charset=utf-8", SDK_CSS.as_bytes().to_vec()),
            _ => reply(404, "text/plain", b"not found".to_vec()),
        };
    }
    match file_in(name, if rel.is_empty() { "index.html" } else { rel }) {
        Some(p) => match std::fs::read(&p) {
            Ok(b) => reply(200, mime(&p), b),
            Err(_) => reply(404, "text/plain", b"not found".to_vec()),
        },
        None => reply(404, "text/plain", b"not found".to_vec()),
    }
}

fn log(line: &str) {
    use std::io::Write;
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::server::cue_dir().join("extensions.log")) {
        let _ = writeln!(f, "{at} {line}");
    }
}

// ---------- git.read: git that Cue runs, read-only ----------
// An extension with `git.read` names an operation; Cue picks the folder (the open session's, from Cue's
// own record, never the page's word), builds the command, and checks every argument. Nothing it runs can
// write, and git's repo settings that could start another program are switched off.

/// A commit to diff against: HEAD, or a commit's hex name (as Cue's own snapshots are).
fn valid_rev(r: &str) -> bool {
    r == "HEAD" || ((7..=64).contains(&r.len()) && r.chars().all(|c| c.is_ascii_hexdigit()))
}
/// A file inside the repo, as git names it: relative, no `..`, never read as an option.
fn valid_rel(p: &str) -> bool {
    !p.is_empty() && !p.starts_with('-') && !p.starts_with('/') && !p.split('/').any(|s| s == ".." || s.is_empty()) && !p.contains('\0')
}

/// One file this session edited, as it is now: its uncommitted changes, against its own repo's HEAD (a
/// file git doesn't track yet: its whole content). `abs` is absolute.
fn edited_one(machine: &str, abs: &str, filters: &mut std::collections::HashMap<String, Vec<String>>) -> Value {
    let p = Path::new(abs);
    let (dir, base) = (p.parent().map(|d| d.display().to_string()).unwrap_or_default(), p.file_name().map(|b| b.to_string_lossy().to_string()).unwrap_or_default());
    if dir.is_empty() || base.is_empty() || base.starts_with('-') {
        return json!({ "path": abs, "note": "not a file path" });
    }
    let extra = filters.entry(dir.clone()).or_insert_with(|| safe_git::no_repo_filters(machine, &dir)).clone();
    let tracked = safe_git::run(machine, &dir, &extra, &["ls-files", "--error-unmatch", "--", &base]).map(|(_, c)| c == 0).unwrap_or(false);
    let rest: Vec<&str> = if tracked {
        [&["diff", "HEAD"][..], &safe_git::QUIET_DIFF[..], &["--", &base]].concat()
    } else {
        [&["diff", "--no-index"][..], &safe_git::QUIET_DIFF[..], &["--", "/dev/null", &base]].concat()
    };
    match safe_git::run(machine, &dir, &extra, &rest) {
        Ok((out, _)) if !out.trim().is_empty() => json!({ "path": abs, "new": !tracked, "diff": out }),
        Ok(_) => json!({ "path": abs, "note": if tracked { "no uncommitted changes" } else { "gone, or not in a git repository" } }),
        Err(e) => json!({ "path": abs, "note": e }),
    }
}

/// A git.read operation for extension `ext` in the session folder `folder` (here, or on `machine`):
///   toplevel             the repo's top folder
///   diff {against}       tracked changes against HEAD or a commit
///   untracked            files git doesn't track and isn't told to ignore
///   new_file {path}      one such file's whole content, as a diff
///   edited {transcript}  every file the session edited (from its log), each with its uncommitted changes
pub fn git_read(ext: &str, op: &str, folder: &str, machine: &str, against: Option<&str>, path: Option<&str>, transcript: Option<&str>) -> Result<Value, String> {
    let m = enabled(ext).ok_or_else(|| format!("{ext} isn't switched on"))?;
    if !has(&m, "capabilities", "git.read") {
        return Err(format!("{ext} didn't ask to read git"));
    }
    if folder.is_empty() || !folder.starts_with('/') {
        return Err("that session has no folder".into());
    }
    let start = Instant::now();
    let done = |what: &str, code: i32| log(&format!("{ext} [{}{folder}] git.read {what} -> {code}", if machine.is_empty() { String::new() } else { format!("{machine}:") }));
    let extra = safe_git::no_repo_filters(machine, folder);
    let plain = |rest: &[&str], what: &str| -> Result<Value, String> {
        let (out, code) = safe_git::run(machine, folder, &extra, rest)?;
        done(what, code);
        Ok(json!({ "code": code, "stdout": out, "ms": start.elapsed().as_millis() as u64 }))
    };
    match op {
        "toplevel" => plain(&["rev-parse", "--show-toplevel"], "toplevel"),
        "diff" => {
            let rev = against.unwrap_or("HEAD");
            if !valid_rev(rev) {
                return Err(format!("not a commit: {rev}"));
            }
            plain(&[&["diff", rev][..], &safe_git::QUIET_DIFF[..]].concat(), &format!("diff {rev}"))
        }
        "untracked" => plain(&["ls-files", "--others", "--exclude-standard", "--full-name"], "untracked"),
        "new_file" => {
            let p = path.unwrap_or("");
            if !valid_rel(p) {
                return Err(format!("not a file in the repo: {p}"));
            }
            plain(&[&["diff", "--no-index"][..], &safe_git::QUIET_DIFF[..], &["--", "/dev/null", p]].concat(), &format!("new_file {p}"))
        }
        "edited" => {
            let t = transcript.filter(|t| !t.is_empty()).ok_or("that session has no log to read its edits from")?;
            let files: Vec<String> = crate::logs::steps::edited_files(t)
                .into_iter()
                .map(|f| if f.starts_with('/') { f } else { format!("{}/{f}", folder.trim_end_matches('/')) })
                .take(200)
                .collect();
            let mut filters = std::collections::HashMap::new();
            let out: Vec<Value> = files.iter().map(|f| edited_one(machine, f, &mut filters)).collect();
            done(&format!("edited ({} files)", out.len()), 0);
            Ok(json!({ "files": out, "ms": start.elapsed().as_millis() as u64 }))
        }
        _ => Err(format!("no such git.read operation: {op}")),
    }
}

#[tauri::command]
pub async fn ext_git(ext: String, op: String, folder: String, machine: Option<String>, against: Option<String>, path: Option<String>, transcript: Option<String>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || git_read(&ext, &op, &folder, machine.as_deref().unwrap_or(""), against.as_deref(), path.as_deref(), transcript.as_deref())).await.map_err(|e| e.to_string())?
}

// ---------- programs.run: programs installed on this Mac ----------
// For extensions you trust with your own tools (a container tool, a sync tool, your company's CLI). Off
// until you allow it in Settings; then each extension runs only the programs its manifest lists and you
// approved, by name, as installed on this Mac. Never a program it brings with it, never a shell or a
// script interpreter, never through a shell. Every run is logged.

/// Programs that would run whatever text they're given: never run for an extension.
const NOT_PROGRAMS: [&str; 22] = ["sh", "bash", "zsh", "dash", "ksh", "fish", "csh", "tcsh", "env", "xargs", "python", "python3", "node", "deno", "bun", "ruby", "perl", "php", "osascript", "lua", "tclsh", "expect"];
/// The longest a run may take (making a container or a pod can take a while).
const PROGRAM_LIMIT: std::time::Duration = std::time::Duration::from_secs(600);

/// Whether you've let extensions run programs at all (Settings → Extensions).
pub fn programs_allowed() -> bool {
    crate::config::load()["extensions_allow"]["programs"].as_bool().unwrap_or(false)
}

/// A program's name as an extension may give it: a plain name, not a path, not one of NOT_PROGRAMS
/// (nor a versioned one, python3.12 say).
fn valid_program(p: &str) -> bool {
    let base = p.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    (1..=64).contains(&p.len())
        && p.chars().all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
        && !p.starts_with(['.', '-'])
        && !NOT_PROGRAMS.contains(&p)
        && !NOT_PROGRAMS.contains(&base)
}

/// Where an installed program is: the folders programs are installed in on a Mac, never anywhere an
/// extension lives.
fn installed(program: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let found = path
        .split(':')
        .chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"])
        .filter(|d| d.starts_with('/'))
        .map(|d| Path::new(d).join(program))
        .find(|p| p.is_file())?;
    let real = found.canonicalize().ok()?;
    let theirs: Vec<PathBuf> = [Some(root()), builtin_root()].into_iter().flatten().filter_map(|d| d.canonicalize().ok()).collect();
    (!theirs.iter().any(|d| real.starts_with(d))).then_some(found)
}

/// Run `program args…` for extension `ext` in `cwd` (a folder here), within every rule above.
pub fn run_program(ext: &str, program: &str, args: &[String], cwd: Option<&str>) -> Result<Value, String> {
    if !programs_allowed() {
        return Err("Extensions aren't allowed to run programs (Settings → Extensions)".into());
    }
    let m = enabled(ext).ok_or_else(|| format!("{ext} isn't switched on"))?;
    if !has(&m, "capabilities", "programs.run") {
        return Err(format!("{ext} didn't ask to run programs"));
    }
    if !valid_program(program) {
        return Err(format!("{program} can't be run by an extension"));
    }
    if !has(&m, "programs", program) {
        return Err(format!("{ext} didn't list {program}"));
    }
    let bin = installed(program).ok_or_else(|| format!("{program} isn't installed on this Mac"))?;
    let dir = cwd.filter(|d| d.starts_with('/') && Path::new(d).is_dir());
    let mut c = Command::new(&bin);
    c.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    if let Some(d) = dir {
        c.current_dir(d);
    }
    let start = Instant::now();
    let child = c.spawn().map_err(|e| e.to_string())?;
    let (tx, rx) = std::sync::mpsc::channel();
    let pid = child.id();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let out = match rx.recv_timeout(PROGRAM_LIMIT) {
        Ok(r) => r.map_err(|e| e.to_string())?,
        Err(_) => {
            let _ = Command::new("/bin/kill").arg(pid.to_string()).status();
            log(&format!("{ext} ran {program} {} -> stopped (took too long)", args.join(" ")));
            return Err(format!("{program} took too long"));
        }
    };
    let code = out.status.code().unwrap_or(-1);
    log(&format!("{ext} ran {program} {} -> {code}", args.join(" ")));
    Ok(json!({
        "code": code,
        "stdout": String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(4 << 20)]),
        "stderr": String::from_utf8_lossy(&out.stderr[..out.stderr.len().min(1 << 20)]),
        "ms": start.elapsed().as_millis() as u64,
    }))
}

#[tauri::command]
pub async fn ext_program(ext: String, program: String, args: Vec<String>, cwd: Option<String>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || run_program(&ext, &program, &args, cwd.as_deref())).await.map_err(|e| e.to_string())?
}

/// Settings → Extensions: let extensions run programs, or not.
#[tauri::command]
pub fn ext_allow_programs(on: bool) -> Result<(), String> {
    log(&format!("settings programs {}", if on { "allowed" } else { "not allowed" }));
    crate::config::remember("extensions_allow.programs", json!(on))
}

/// Where extensions go, and each one there with where it stands.
#[tauri::command]
pub fn ext_list() -> Value {
    json!({ "root": root().display().to_string(), "list": list(), "programs_allowed": programs_allowed() })
}

/// Switch an extension on (approving everything its manifest asks for now) or off (what you approved
/// stays noted, so switching it back on asks nothing new). From Settings only: pages can't call this.
#[tauri::command]
pub fn ext_enable(name: String, on: bool) -> Result<(), String> {
    let m = manifest(&name).ok_or("no such extension")?;
    let a = approval(&name);
    let v = if on {
        json!({ "on": true, "capabilities": m["capabilities"].clone(), "programs": m["programs"].clone(), "version": m["version"].clone() })
    } else {
        json!({ "on": false, "capabilities": a["capabilities"].clone(), "programs": a["programs"].clone(), "version": a["version"].clone() })
    };
    log(&format!("{name} switched {} in Settings", if on { "on" } else { "off" }));
    crate::config::remember(&format!("extensions.{name}"), v)
}

/// The last `n` lines extensions.log has about `name`, newest last.
#[tauri::command]
pub fn ext_activity(name: String, n: usize) -> Vec<String> {
    let text = std::fs::read_to_string(crate::server::cue_dir().join("extensions.log")).unwrap_or_default();
    let mine: Vec<&str> = text.lines().filter(|l| l.split(' ').nth(1) == Some(name.as_str())).collect();
    mine[mine.len().saturating_sub(n.min(50))..].iter().map(|l| l.to_string()).collect()
}

/// The extensions folder in Finder (made first if it isn't there yet).
#[tauri::command]
pub fn ext_reveal() -> Result<(), String> {
    let dir = root();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Command::new("/usr/bin/open").arg(&dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// A line from an extension's page, into extensions.log.
#[tauri::command]
pub fn ext_log(ext: String, text: String) -> Result<(), String> {
    enabled(&ext).ok_or("no such extension")?;
    log(&format!("{ext} says: {}", text.replace('\n', " ").chars().take(500).collect::<String>()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_refuses_escapes() {
        assert_eq!(decode("a%20b/c.html").as_deref(), Some("a b/c.html"));
        assert!(decode("bad%2").is_none());
    }

    #[test]
    fn names_are_plain() {
        assert!(valid_name("diff"));
        assert!(valid_name("dev-pod_2"));
        assert!(!valid_name("../x"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name(""));
    }

    #[test]
    fn off_until_switched_on_and_off_again_when_an_update_asks_for_more() {
        let _one = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-ext-test-{}", std::process::id()));
        let ext = dir.join("extensions/demo");
        std::fs::create_dir_all(&ext).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        std::env::set_var("CUE_BUILTIN_EXTENSIONS", "");
        let write = |caps: &str| std::fs::write(ext.join("manifest.json"), format!(r#"{{"title":"Demo","tab":"Demo","capabilities":[{caps}]}}"#)).unwrap();
        std::fs::write(ext.join("index.html"), "<p>hi</p>").unwrap();
        write(r#""sessions.read""#);
        let page = || serve(Request::builder().uri("cueext://localhost/demo/index.html").body(vec![]).unwrap()).status();
        let folder = std::env::temp_dir().display().to_string();
        let read = || git_read("demo", "toplevel", &folder, "", None, None, None);

        // New: off, its page isn't served and it can't read git.
        assert_eq!(list()[0]["enabled"], json!(false));
        assert_eq!(page(), 404);
        assert!(read().unwrap_err().contains("isn't switched on"));

        // Switched on: what it asked for is approved; its page is served. Still nothing it didn't ask for.
        ext_enable("demo".into(), true).unwrap();
        assert_eq!(list()[0]["enabled"], json!(true));
        assert_eq!(page(), 200);
        assert!(read().unwrap_err().contains("didn't ask to read git"));

        // An update that asks for more: off again until switched on again, the new asks listed.
        write(r#""sessions.read","git.read""#);
        let e = &list()[0];
        assert_eq!((e["on"].clone(), e["enabled"].clone()), (json!(true), json!(false)));
        assert_eq!(e["pending_capabilities"], json!(["git.read"]));
        assert_eq!(page(), 404);
        assert!(read().unwrap_err().contains("isn't switched on"));

        // Switched off: nothing served, nothing runs; its log names the switches.
        ext_enable("demo".into(), true).unwrap();
        ext_enable("demo".into(), false).unwrap();
        assert_eq!(page(), 404);
        assert!(ext_activity("demo".into(), 10).iter().any(|l| l.ends_with("switched off in Settings")));

        std::env::remove_var("CUE_HOME");
        std::env::remove_var("CUE_BUILTIN_EXTENSIONS");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn programs_run_only_when_allowed_listed_approved_and_installed() {
        let _one = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-ext-prog-{}", std::process::id()));
        let ext = dir.join("extensions/tools");
        std::fs::create_dir_all(&ext).unwrap();
        std::env::set_var("CUE_HOME", &dir);
        std::env::set_var("CUE_BUILTIN_EXTENSIONS", "");
        std::fs::write(ext.join("manifest.json"), r#"{"tab":"T","capabilities":["programs.run"],"programs":["git","bash","ls"]}"#).unwrap();
        // A program it brings along, named like an installed one would be.
        std::fs::write(ext.join("mytool"), "#!/bin/sh\necho brought\n").unwrap();
        let run = |p: &str, a: &[&str]| run_program("tools", p, &a.iter().map(|s| s.to_string()).collect::<Vec<_>>(), None);

        ext_enable("tools".into(), true).unwrap();
        assert!(run("git", &["--version"]).unwrap_err().contains("aren't allowed"), "off until allowed in Settings");
        ext_allow_programs(true).unwrap();
        let ok = run("git", &["--version"]).unwrap();
        assert_eq!(ok["code"], json!(0));
        assert!(ok["stdout"].as_str().unwrap().starts_with("git version"));
        assert!(run("bash", &["-c", "true"]).unwrap_err().contains("can't be run"), "no shells, even when listed");
        assert!(run("mytool", &[]).is_err(), "not a program it brings");
        assert!(run(&ext.join("mytool").display().to_string(), &[]).unwrap_err().contains("can't be run"), "no paths");
        assert!(run("ls", &[]).is_ok(), "listed and installed");
        assert!(run("cat", &["/etc/hosts"]).unwrap_err().contains("didn't list"), "only what it listed");
        assert!(ext_activity("tools".into(), 10).iter().any(|l| l.contains("ran git --version -> 0")), "logged");
        ext_allow_programs(false).unwrap();
        assert!(run("git", &["--version"]).is_err(), "off again");

        std::env::remove_var("CUE_HOME");
        std::env::remove_var("CUE_BUILTIN_EXTENSIONS");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cues_own_extensions_are_listed_and_a_copy_of_yours_takes_their_place() {
        let _one = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-ext-builtin-{}", std::process::id()));
        let (mine, own) = (dir.join("home/extensions"), dir.join("app/extensions"));
        for d in [own.join("tool"), own.join("other")] {
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("manifest.json"), r#"{"title":"Cue's","tab":"T"}"#).unwrap();
            std::fs::write(d.join("index.html"), "v1").unwrap();
        }
        std::env::set_var("CUE_HOME", dir.join("home"));
        std::env::set_var("CUE_BUILTIN_EXTENSIONS", &own);
        let get = |n: &str| list().into_iter().find(|e| e["name"] == n).unwrap();

        // Cue's own: listed, marked as such, off until switched on, served once it is.
        assert_eq!(list().len(), 2);
        assert_eq!((get("tool")["builtin"].clone(), get("tool")["enabled"].clone()), (json!(true), json!(false)));
        ext_enable("tool".into(), true).unwrap();
        assert_eq!(serve(Request::builder().uri("cueext://localhost/tool/index.html").body(vec![]).unwrap()).into_body(), b"v1");

        // A change to its files changes its stamp (its page reloads).
        let before = get("tool")["stamp"].clone();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(own.join("tool/index.html"), "v2, longer").unwrap();
        assert_ne!(get("tool")["stamp"], before);

        // Your copy of the same name takes its place, and says so.
        std::fs::create_dir_all(mine.join("tool")).unwrap();
        std::fs::write(mine.join("tool/manifest.json"), r#"{"title":"Mine","tab":"T"}"#).unwrap();
        std::fs::write(mine.join("tool/index.html"), "mine").unwrap();
        let t = get("tool");
        assert_eq!((t["title"].clone(), t["builtin"].clone(), t["replaces_builtin"].clone()), (json!("Mine"), json!(false), json!(true)));
        assert_eq!(list().len(), 2);
        assert_eq!(serve(Request::builder().uri("cueext://localhost/tool/index.html").body(vec![]).unwrap()).into_body(), b"mine");

        std::env::remove_var("CUE_HOME");
        std::env::remove_var("CUE_BUILTIN_EXTENSIONS");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn git_read_runs_only_fixed_read_only_git_and_checks_every_argument() {
        assert!(valid_rev("HEAD") && valid_rev("3f9a1c2") && !valid_rev("--output=/tmp/x") && !valid_rev("main~1") && !valid_rev("-c"));
        assert!(valid_rel("src/a.rs") && !valid_rel("../x") && !valid_rel("/etc/passwd") && !valid_rel("--output=x") && !valid_rel("a//b"));
        let _one = crate::db::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("cue-gitread-{}", std::process::id()));
        let (home, repo) = (dir.join("home"), dir.join("repo"));
        std::fs::create_dir_all(home.join("extensions/r")).unwrap();
        std::fs::write(home.join("extensions/r/manifest.json"), r#"{"tab":"R","capabilities":["git.read"]}"#).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let sh = |c: &str| assert!(Command::new("/bin/sh").arg("-c").arg(c).current_dir(&repo).status().unwrap().success());
        sh("git init -q && git config user.email t@t && git config user.name t && printf 'one\n' > a.txt && git add . && git commit -qm i");
        // A repo whose own config would run a program for a diff: Cue's git.read must not.
        sh("git config diff.external 'touch ran-external' && printf 'one\ntwo\n' > a.txt && printf 'new\n' > b.txt");
        // And a filter: .gitattributes picks it, the repo's own config says what it runs.
        sh("printf '*.txt filter=evil\n' > .gitattributes && git config filter.evil.clean 'touch ran-clean; cat' && git config filter.evil.process 'touch ran-process; false'");
        std::env::set_var("CUE_HOME", &home);
        std::env::set_var("CUE_BUILTIN_EXTENSIONS", "");
        let folder = repo.canonicalize().unwrap().display().to_string();
        let read = |op: &str, against: Option<&str>, path: Option<&str>, t: Option<&str>| git_read("r", op, &folder, "", against, path, t);

        // Off: nothing.
        assert!(read("toplevel", None, None, None).unwrap_err().contains("isn't switched on"));
        ext_enable("r".into(), true).unwrap();
        assert_eq!(read("toplevel", None, None, None).unwrap()["stdout"].as_str().unwrap().trim(), folder);
        let d = read("diff", Some("HEAD"), None, None).unwrap();
        assert!(d["stdout"].as_str().unwrap().contains("+two"));
        assert!(!repo.join("ran-external").exists(), "the repo's external diff program never ran");
        assert!(!repo.join("ran-clean").exists() && !repo.join("ran-process").exists(), "the repo's filters never ran");
        assert_eq!(read("untracked", None, None, None).unwrap()["stdout"].as_str().unwrap().trim(), ".gitattributes\nb.txt");
        assert!(read("new_file", None, Some("b.txt"), None).unwrap()["stdout"].as_str().unwrap().contains("+new"));
        // Anything that isn't a commit or a file in the repo is refused before git runs.
        assert!(read("diff", Some("--output=/tmp/x"), None, None).is_err());
        assert!(read("new_file", None, Some("../../etc/passwd"), None).is_err());
        assert!(read("rm", None, None, None).is_err());

        // edited: the files a session's log says it edited, each with its changes now.
        let log = dir.join("t.jsonl");
        std::fs::write(&log, format!(r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"t1","name":"Edit","input":{{"file_path":"{folder}/a.txt"}}}},{{"type":"tool_use","id":"t2","name":"Write","input":{{"file_path":"{folder}/b.txt"}}}}]}}}}"#)).unwrap();
        let e = read("edited", None, None, Some(log.to_str().unwrap())).unwrap();
        let files = e["files"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        assert!(files[0]["diff"].as_str().unwrap().contains("+two") && files[0]["new"] == json!(false));
        assert!(files[1]["diff"].as_str().unwrap().contains("+new") && files[1]["new"] == json!(true));
        assert!(!repo.join("ran-clean").exists() && !repo.join("ran-process").exists(), "nor while reading edits");

        std::env::remove_var("CUE_HOME");
        std::env::remove_var("CUE_BUILTIN_EXTENSIONS");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
