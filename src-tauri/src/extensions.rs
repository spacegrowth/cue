//! Extensions that bring their own page. Each is a folder in <Cue's folder>/extensions/<name>/
//! with a manifest.json and its HTML, served at cueext://localhost/<name>/… and shown in a sandboxed
//! frame. The page talks to Cue only through messages to the window, which checks the manifest's
//! capabilities; running a program is checked again here, against the manifest's `programs`.

use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tauri::http::{Request, Response};

/// What an extension page may load and do: its own files and Cue's helper, nothing from the network.
const PAGE_CSP: &str = "default-src 'none'; script-src cueext://localhost; style-src cueext://localhost 'unsafe-inline'; img-src cueext://localhost data:; font-src cueext://localhost; connect-src 'none'; form-action 'none'";
const SDK_JS: &str = include_str!("../../ui/ext-sdk/cue.js");
const SDK_CSS: &str = include_str!("../../ui/ext-sdk/cue.css");
const RUN_LIMIT: Duration = Duration::from_secs(20);
const OUT_LIMIT: usize = 4 << 20;

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
/// extension can't approve itself): {"on": bool, "capabilities": [...], "programs": [...]}.
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

/// Where a program is: the first match on a PATH that also has the usual Homebrew folders (an app
/// started from Finder gets a short PATH).
fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        return None;
    }
    let path = std::env::var("PATH").unwrap_or_default();
    let found = path.split(':')
        .chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"])
        .map(|d| Path::new(d).join(program))
        .find(|p| p.is_file());
    found
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

fn shown(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().map(|a| if a.is_empty() || a.contains(|c: char| c.is_whitespace() || "'\"$`\\".contains(c)) { format!("'{}'", a.replace('\'', "'\\''")) } else { a.clone() }))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run `program` for extension `ext`: only with the `cmd.run` capability, only a program its manifest
/// lists, never through a shell, at most RUN_LIMIT. Every run goes in extensions.log as a command line.
pub fn run(ext: &str, program: &str, args: Vec<String>, cwd: Option<String>, machine: Option<String>) -> Result<Value, String> {
    let m = enabled(ext).ok_or_else(|| format!("{ext} isn't switched on"))?;
    if !has(&m, "programs", program) {
        return Err(format!("{ext} didn't list {program} among its programs"));
    }
    if let Some(name) = machine.filter(|n| !n.is_empty()) {
        if !has(&m, "capabilities", "machines.run") {
            return Err(format!("{ext} didn't ask to run programs on other machines"));
        }
        return run_on(ext, &name, program, &args, cwd.as_deref());
    }
    if !has(&m, "capabilities", "cmd.run") {
        return Err(format!("{ext} didn't ask to run programs"));
    }
    let bin = which(program).ok_or_else(|| format!("{program} isn't installed"))?;
    let line = shown(program, &args);
    let dir = cwd.filter(|d| Path::new(d).is_dir());
    let mut c = Command::new(&bin);
    c.args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(d) = &dir {
        c.current_dir(d);
    }
    let mut child = c.spawn().map_err(|e| e.to_string())?;
    let (mut so, mut se) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let ro = std::thread::spawn(move || { let mut b = Vec::new(); let _ = (&mut so).take(OUT_LIMIT as u64).read_to_end(&mut b); b });
    let re = std::thread::spawn(move || { let mut b = Vec::new(); let _ = (&mut se).take(OUT_LIMIT as u64).read_to_end(&mut b); b });
    let start = Instant::now();
    let code = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s.code();
        }
        if start.elapsed() > RUN_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (out, err) = (ro.join().unwrap_or_default(), re.join().unwrap_or_default());
    log(&format!("{ext} [{}] {line} -> {}", dir.as_deref().unwrap_or("~"), code.map_or("stopped".into(), |c| c.to_string())));
    Ok(json!({
        "line": line,
        "code": code,
        "stdout": String::from_utf8_lossy(&out),
        "stderr": String::from_utf8_lossy(&err),
        "ms": start.elapsed().as_millis() as u64,
    }))
}

// ---------- git.read: git that Cue runs, read-only ----------
// An extension with `git.read` names an operation; Cue picks the folder (the open session's, from Cue's
// own record, never the page's word), builds the command, and checks every argument. Nothing it runs can
// write, and git's repo settings that could start another program are switched off.

/// The git every git.read operation runs: no pager, no locks, no file-system monitor, no external diff or
/// text conversion (both are set in a repo's own config and run a program).
fn git_args<'a>(dir: &'a str, rest: &[&'a str]) -> Vec<&'a str> {
    let mut a = vec!["-c", "core.fsmonitor=false", "-c", "core.pager=cat", "--no-optional-locks", "-C", dir];
    a.extend_from_slice(rest);
    a
}
const QUIET_DIFF: [&str; 3] = ["--no-color", "--no-ext-diff", "--no-textconv"];

/// A commit to diff against: HEAD, or a commit's hex name (as Cue's own snapshots are).
fn valid_rev(r: &str) -> bool {
    r == "HEAD" || ((7..=64).contains(&r.len()) && r.chars().all(|c| c.is_ascii_hexdigit()))
}
/// A file inside the repo, as git names it: relative, no `..`, never read as an option.
fn valid_rel(p: &str) -> bool {
    !p.is_empty() && !p.starts_with('-') && !p.starts_with('/') && !p.split('/').any(|s| s == ".." || s.is_empty()) && !p.contains('\0')
}

/// Filters the repo's own config defines (`filter.<name>.clean`, `.smudge`, `.process`: programs git runs
/// on files when it diffs them, picked by .gitattributes), each switched off: clean and smudge pass the
/// file through as it is, and no process runs. Yours, from your own git config (Git LFS, say), stay.
/// Reading config runs nothing.
fn no_repo_filters(machine: &str, dir: &str) -> Vec<String> {
    let mut names: Vec<String> = vec![];
    for scope in ["--local", "--worktree"] {
        let Ok((out, _)) = git_in(machine, dir, &[], &["config", scope, "--name-only", "--get-regexp", "^filter\\."]) else { continue };
        for key in out.lines() {
            if let Some(name) = key.strip_prefix("filter.").and_then(|k| k.rsplit_once('.')).map(|(n, _)| n.to_string()) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names.iter().flat_map(|n| ["-c".to_string(), format!("filter.{n}.clean=cat"), "-c".into(), format!("filter.{n}.smudge=cat"), "-c".into(), format!("filter.{n}.process=")]).collect()
}

/// Run git (the words after `git`) in `dir`, here or on `machine`, with `extra` settings first (from
/// no_repo_filters): stdout, and the exit code.
fn git_in(machine: &str, dir: &str, extra: &[String], rest: &[&str]) -> Result<(String, i32), String> {
    let extra: Vec<&str> = extra.iter().map(String::as_str).collect();
    let words = git_args(dir, &[&extra[..], rest].concat());
    if machine.is_empty() {
        let bin = which("git").ok_or("git isn't installed")?;
        let out = Command::new(bin).args(&words).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
        return Ok((String::from_utf8_lossy(&out.stdout[..out.stdout.len().min(OUT_LIMIT)]).to_string(), out.status.code().unwrap_or(1)));
    }
    let host = crate::machines::get(machine).ok_or_else(|| format!("Cue doesn't know a machine called {machine}"))?.host;
    let mut all = vec!["git"];
    all.extend(words);
    let script = format!("{}; printf '\\n@@cue-exit %s\\n' $?", sh_words(&all));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(crate::machines::run(&host, &script)); });
    let o = rx.recv_timeout(RUN_LIMIT).unwrap_or_else(|_| Err("it took too long".into()))?;
    Ok(match o.rfind("\n@@cue-exit ") {
        Some(i) => (o[..i].to_string(), o[i + 12..].trim().parse().unwrap_or(1)),
        None => (o, 1),
    })
}

/// One file this session edited, as it is now: its uncommitted changes, against its own repo's HEAD (a
/// file git doesn't track yet: its whole content). `abs` is absolute.
fn edited_one(machine: &str, abs: &str, filters: &mut std::collections::HashMap<String, Vec<String>>) -> Value {
    let p = Path::new(abs);
    let (dir, base) = (p.parent().map(|d| d.display().to_string()).unwrap_or_default(), p.file_name().map(|b| b.to_string_lossy().to_string()).unwrap_or_default());
    if dir.is_empty() || base.is_empty() || base.starts_with('-') {
        return json!({ "path": abs, "note": "not a file path" });
    }
    let extra = filters.entry(dir.clone()).or_insert_with(|| no_repo_filters(machine, &dir)).clone();
    let tracked = git_in(machine, &dir, &extra, &["ls-files", "--error-unmatch", "--", &base]).map(|(_, c)| c == 0).unwrap_or(false);
    let rest: Vec<&str> = if tracked {
        [&["diff", "HEAD"][..], &QUIET_DIFF[..], &["--", &base]].concat()
    } else {
        [&["diff", "--no-index"][..], &QUIET_DIFF[..], &["--", "/dev/null", &base]].concat()
    };
    match git_in(machine, &dir, &extra, &rest) {
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
    let extra = no_repo_filters(machine, folder);
    let plain = |rest: &[&str], what: &str| -> Result<Value, String> {
        let (out, code) = git_in(machine, folder, &extra, rest)?;
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
            plain(&[&["diff", rev][..], &QUIET_DIFF[..]].concat(), &format!("diff {rev}"))
        }
        "untracked" => plain(&["ls-files", "--others", "--exclude-standard", "--full-name"], "untracked"),
        "new_file" => {
            let p = path.unwrap_or("");
            if !valid_rel(p) {
                return Err(format!("not a file in the repo: {p}"));
            }
            plain(&[&["diff", "--no-index"][..], &QUIET_DIFF[..], &["--", "/dev/null", p]].concat(), &format!("new_file {p}"))
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

/// Where extensions go, and each one there with where it stands.
#[tauri::command]
pub fn ext_list() -> Value {
    json!({ "root": root().display().to_string(), "list": list() })
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

/// Every word quoted for a POSIX shell, so nothing in it is read as more shell.
fn sh_words(words: &[&str]) -> String {
    words.iter().map(|w| format!("'{}'", w.replace('\'', "'\\''"))).collect::<Vec<_>>().join(" ")
}

/// The same, on one of Cue's machines, over Cue's own SSH connection. The folder is only ever looked at
/// there, never on this Mac. At most RUN_LIMIT; every run goes in extensions.log.
fn run_on(ext: &str, name: &str, program: &str, args: &[String], cwd: Option<&str>) -> Result<Value, String> {
    let host = crate::machines::get(name).ok_or_else(|| format!("Cue doesn't know a machine called {name}"))?.host;
    let mut words = vec![program];
    words.extend(args.iter().map(String::as_str));
    let cmd = sh_words(&words);
    // The command's own exit code comes back on a last line (ssh would otherwise turn any non-zero exit,
    // like `git diff --no-index` finding a difference, into a failure and drop what it printed).
    let script = match cwd {
        Some(d) if !d.is_empty() => format!("cd {} && {cmd}; printf '\\n@@cue-exit %s\\n' $?", sh_words(&[d])),
        _ => format!("{cmd}; printf '\\n@@cue-exit %s\\n' $?"),
    };
    let line = shown(program, args);
    let start = Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    let (h, sc) = (host.clone(), script.clone());
    std::thread::spawn(move || { let _ = tx.send(crate::machines::run(&h, &sc)); });
    let got = rx.recv_timeout(RUN_LIMIT).unwrap_or_else(|_| Err("it took too long".into()));
    let (out, err, code) = match got {
        Ok(o) => match o.rfind("\n@@cue-exit ") {
            Some(i) => (o[..i].to_string(), String::new(), o[i + 12..].trim().parse::<i32>().unwrap_or(1)),
            None => (o, String::new(), 1),
        },
        Err(e) => (String::new(), e, 255),
    };
    log(&format!("{ext} [{name}:{}] {line} -> {code}", cwd.unwrap_or("~")));
    Ok(json!({ "line": format!("{line}  (on {name})"), "code": code, "stdout": out, "stderr": err, "ms": start.elapsed().as_millis() as u64 }))
}

#[tauri::command]
pub async fn ext_run(ext: String, program: String, args: Vec<String>, cwd: Option<String>, machine: Option<String>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || run(&ext, &program, args, cwd, machine)).await.map_err(|e| e.to_string())?
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
        let write = |caps: &str, progs: &str| std::fs::write(ext.join("manifest.json"), format!(r#"{{"title":"Demo","tab":"Demo","capabilities":[{caps}],"programs":[{progs}]}}"#)).unwrap();
        std::fs::write(ext.join("index.html"), "<p>hi</p>").unwrap();
        write(r#""cmd.run""#, r#""git""#);
        let page = || serve(Request::builder().uri("cueext://localhost/demo/index.html").body(vec![]).unwrap()).status();

        // New: off, its page isn't served and it can't run anything.
        assert_eq!(list()[0]["enabled"], json!(false));
        assert_eq!(page(), 404);
        assert!(run("demo", "git", vec!["--version".into()], None, None).unwrap_err().contains("isn't switched on"));

        // Switched on: what it asked for is approved; its page is served and git runs.
        ext_enable("demo".into(), true).unwrap();
        assert_eq!(list()[0]["enabled"], json!(true));
        assert_eq!(page(), 200);
        assert_eq!(run("demo", "git", vec!["--version".into()], None, None).unwrap()["code"], json!(0));
        // Still only what it listed.
        assert!(run("demo", "ls", vec![], None, None).unwrap_err().contains("didn't list ls"));

        // An update that asks for more: off again until switched on again, the new asks listed.
        write(r#""cmd.run","machines.run""#, r#""git","rsync""#);
        let e = &list()[0];
        assert_eq!((e["on"].clone(), e["enabled"].clone()), (json!(true), json!(false)));
        assert_eq!(e["pending_capabilities"], json!(["machines.run"]));
        assert_eq!(e["pending_programs"], json!(["rsync"]));
        assert_eq!(page(), 404);

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

    #[test]
    fn remote_words_are_all_quoted() {
        assert_eq!(sh_words(&["git", "-C", "/a b", "it's"]), "'git' '-C' '/a b' 'it'\\''s'");
    }

    #[test]
    fn command_lines_are_quoted() {
        assert_eq!(shown("git", &["-C".into(), "/a b".into(), "diff".into()]), "git -C '/a b' diff");
    }
}
