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
    let text = std::fs::read_to_string(root().join(name).join("manifest.json")).ok()?;
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
    m["folder"] = json!(root().join(&name).display().to_string());
    m
}

pub fn list() -> Vec<Value> {
    let mut out: Vec<Value> = std::fs::read_dir(root())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| manifest(e.file_name().to_str()?))
        .map(with_status)
        .collect();
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
    let dir = root().join(name).canonicalize().ok()?;
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
