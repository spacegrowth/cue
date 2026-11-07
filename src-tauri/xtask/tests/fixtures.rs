//! The analysis on small crates written for each case: what it must catch, and what it must not flag.

use std::path::PathBuf;

/// The check's failures and the inventory of `outcome`.
fn run(name: &str, files: &[(&str, &str)], allow: &str) -> (Vec<String>, String) {
    let out = outcome(name, files, allow);
    (out.check.failures, out.inventory)
}

/// A repo in a fresh temporary folder with `files` under `src-tauri/src` (plus the `locking` module every
/// case uses) and `allow` as its lint-allow.toml, analysed.
fn outcome(name: &str, files: &[(&str, &str)], allow: &str) -> xtask::Outcome {
    outcome_with_ui(name, files, &[], allow)
}

/// `outcome`, with `ui` written under `ui/` too.
fn outcome_with_ui(name: &str, files: &[(&str, &str)], ui: &[(&str, &str)], allow: &str) -> xtask::Outcome {
    let repo = std::env::temp_dir().join(format!("xtask-fixture-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo);
    let src = repo.join("src-tauri/src");
    std::fs::create_dir_all(src.join("locking")).unwrap();
    std::fs::create_dir_all(repo.join("ui")).unwrap();
    std::fs::write(src.join("locking/mod.rs"), LOCKING).unwrap();
    for (path, text) in files {
        let p: PathBuf = src.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    for (path, text) in ui {
        std::fs::write(repo.join("ui").join(path), text).unwrap();
    }
    let allow_path = repo.join("lint-allow.toml");
    std::fs::write(&allow_path, format!("[store]\nlock = \"the hub's store\"\n{allow}")).unwrap();
    let out = xtask::run(&repo, &allow_path).unwrap();
    let _ = std::fs::remove_dir_all(&repo);
    out
}

const LOCKING: &str = r#"
use std::sync::Mutex;
pub struct Guard<'a, T>(std::sync::MutexGuard<'a, T>);
pub fn lock<'a, T>(m: &'a Mutex<T>, what: &'static str) -> Guard<'a, T> { Guard(m.lock().unwrap()) }
"#;

const HUB: &str = r#"
use std::sync::Mutex;
pub struct Store { pub n: u32 }
pub struct Hub { store: Mutex<Store> }
impl Hub {
    fn st(&self) -> crate::locking::Guard<'_, Store> {
        crate::locking::lock(&self.store, "the hub's store")
    }
    pub fn snapshot(&self) {
        let mut st = self.st();
        Self::reviews_view(&mut st);
    }
    fn lead_busy(st: &Store, lead: &str) -> Option<String> {
        let reviewing = || crate::leads::name_of(lead);
        (st.n > 0).then(reviewing)
    }
    fn reviews_view(st: &mut Store) {
        let free = |s: &Store, lead: &str| Self::lead_busy(s, lead).is_none();
        let _missed = crate::leads::missed(|lead| free(st, lead));
    }
}
"#;

const LIB: &str = "mod hub;\nmod leads;\nmod locking;\n";

/// leads.rs as it was before 2e9abc0: `missed` held INDEX while it called `lead_free`, and the hub's
/// `lead_free` reads a lead's name from INDEX (through two closures and a helper).
const LEADS_BEFORE: &str = r#"
use std::sync::Mutex;
static INDEX: Mutex<Option<Vec<(String, String)>>> = Mutex::new(None);
pub fn name_of(session_id: &str) -> String {
    crate::locking::lock(&INDEX, "INDEX").as_ref().and_then(|i| i.iter().find(|m| m.0 == session_id)).map(|m| m.1.clone()).unwrap_or_default()
}
pub fn missed(lead_free: impl Fn(&str) -> bool) -> Vec<String> {
    let cur = crate::locking::lock(&INDEX, "INDEX");
    cur.as_ref().map(|i| i.iter().filter(|m| lead_free(&m.1)).map(|m| m.0.clone()).collect()).unwrap_or_default()
}
"#;

/// The fix: the index is let go before `lead_free` runs.
const LEADS_AFTER: &str = r#"
use std::sync::Mutex;
static INDEX: Mutex<Option<Vec<(String, String)>>> = Mutex::new(None);
pub fn name_of(session_id: &str) -> String {
    crate::locking::lock(&INDEX, "INDEX").as_ref().and_then(|i| i.iter().find(|m| m.0 == session_id)).map(|m| m.1.clone()).unwrap_or_default()
}
pub fn missed(lead_free: impl Fn(&str) -> bool) -> Vec<String> {
    let waiting: Vec<(String, String)> = {
        let cur = crate::locking::lock(&INDEX, "INDEX");
        cur.as_ref().map(|i| i.clone()).unwrap_or_default()
    };
    waiting.into_iter().filter(|m| lead_free(&m.1)).map(|m| m.0).collect()
}
"#;

const STORE_TO_INDEX: &str = "[[lock_order]]\nfrom = \"the hub's store\"\nto = \"INDEX\"\nreason = \"fixture\"\n";

#[test]
fn the_index_waiting_on_itself_through_a_closure_parameter_is_a_cycle() {
    let (failures, _) = run("index-before", &[("lib.rs", LIB), ("hub.rs", HUB), ("leads.rs", LEADS_BEFORE)], STORE_TO_INDEX);
    let cycle = failures.iter().find(|f| f.starts_with("lock cycle: INDEX taken again")).unwrap_or_else(|| panic!("{failures:#?}"));
    assert!(cycle.contains("in a closure leads::missed runs holding the lock") && cycle.contains("hub::Hub::lead_busy") && cycle.contains("leads::name_of"), "{cycle}");
}

#[test]
fn the_fixed_index_is_no_cycle() {
    let (failures, _) = run("index-after", &[("lib.rs", LIB), ("hub.rs", HUB), ("leads.rs", LEADS_AFTER)], STORE_TO_INDEX);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Phase 2's scoped access: what a `write` closure does runs under the store, as `write` holds it.
#[test]
fn io_inside_a_scoped_write_closure_is_io_under_the_store() {
    let hub = r#"
use std::sync::Mutex;
pub struct Store { pub n: u32 }
pub struct Hub { store: Mutex<Store> }
impl Hub {
    fn write<R>(&self, f: impl FnOnce(&mut Store) -> R) -> R {
        f(&mut crate::locking::lock(&self.store, "the hub's store"))
    }
    pub fn save(&self) {
        let n = self.write(|st| { st.n += 1; st.n });
        std::fs::write("/tmp/n", n.to_string()).ok();
    }
    pub fn save_badly(&self) {
        self.write(|st| std::fs::write("/tmp/n", st.n.to_string()).ok());
    }
}
"#;
    let (failures, _) = run("scoped", &[("lib.rs", "mod hub;\nmod locking;\n"), ("hub.rs", hub)], "");
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(failures[0].starts_with("fs call while the hub's store is held: hub::Hub::save_badly [fs]"), "{}", failures[0]);
}

/// A guard let go (`drop`, the end of its block, a plain `if` condition's temporary) isn't held after;
/// one bound with `let` is held through the rest of its block; a thread doesn't inherit it.
#[test]
fn guards_are_held_as_long_as_rust_keeps_them() {
    let a = r#"
use std::sync::Mutex;
static A: Mutex<u32> = Mutex::new(0);
static B: Mutex<u32> = Mutex::new(0);
fn b() -> u32 { *crate::locking::lock(&B, "B") }
pub fn dropped() { let g = crate::locking::lock(&A, "A"); drop(g); b(); }
pub fn condition() { if *crate::locking::lock(&A, "A") > 0 { b(); } }
pub fn block() { { let _g = crate::locking::lock(&A, "A"); } b(); }
pub fn thread() { let _g = crate::locking::lock(&A, "A"); std::thread::spawn(|| b()); }
pub fn held() { let _g = crate::locking::lock(&A, "A"); let x = 1; if x > 0 { b(); } }
"#;
    let (failures, _) = run("lifetimes", &[("lib.rs", "mod a;\nmod locking;\n"), ("a.rs", a)], "");
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(failures[0].starts_with("new lock order A → B: a::held"), "{}", failures[0]);
}

#[test]
fn a_bare_lock_and_a_new_sync_command_fail_the_check() {
    let a = r#"
use std::sync::Mutex;
static M: Mutex<u32> = Mutex::new(0);
pub fn bare() -> u32 { *M.lock().unwrap() }
#[tauri::command]
pub fn new_one() {}
#[tauri::command]
pub async fn fine() {}
#[cfg(test)]
mod tests { fn t() { let _ = super::M.lock(); } }
"#;
    let (failures, inventory) = run("drift", &[("lib.rs", "mod a;\nmod locking;\n"), ("a.rs", a)], "");
    assert_eq!(failures.len(), 2, "{failures:#?}");
    assert!(failures[0].starts_with("command new_one is sync"), "{}", failures[0]);
    assert!(failures[1].starts_with("a.rs:4: `M` locked another way"), "{}", failures[1]);
    assert!(inventory.contains("| `new_one` | **sync** | `a.rs::new_one` |"), "the inventory lists it too, by item, not line");
}

#[test]
fn an_allow_list_entry_nothing_matches_is_stale() {
    let allow = "[[sync_command]]\nname = \"gone\"\nreason = \"was AppKit\"\n";
    let (failures, _) = run("stale", &[("lib.rs", "mod locking;\n")], allow);
    assert_eq!(failures, ["[[sync_command]] gone is no longer a sync command: remove it from docs/dev/lint-allow.toml"]);
}

#[test]
fn one_lock_name_in_two_files_fails_the_check() {
    let a = "static X: std::sync::Mutex<u8> = std::sync::Mutex::new(0);\npub fn a() { crate::locking::lock(&X, \"X\"); }\n";
    let b = "static X: std::sync::Mutex<u8> = std::sync::Mutex::new(0);\npub fn b() { crate::locking::lock(&X, \"X\"); }\n";
    let (failures, _) = run("names", &[("lib.rs", "mod a;\nmod b;\nmod locking;\n"), ("a.rs", a), ("b.rs", b)], "");
    assert_eq!(failures, ["\"X\" names a lock in a.rs and b.rs"]);
}

/// Not followed and counted: a method whose name a crate type has, on a receiver of unknown type. Not
/// counted: standard-library-only names on such receivers, and calls followed.
#[test]
fn only_calls_that_may_be_crate_calls_count_as_not_followed() {
    let a = r#"
pub struct One;
pub struct Two;
impl One { pub fn refresh(&self) {} pub fn go(&self) { self.refresh(); } }
impl Two { pub fn refresh(&self) {} }
pub fn calls(x: Vec<u8>, y: &dyn std::any::Any) -> usize {
    let o = One;
    o.go();
    let unknown = make();
    unknown.refresh();
    x.iter().map(|b| *b as usize).sum::<usize>() + x.len()
}
fn make() -> One { One }
"#;
    let out = outcome("unresolved", &[("lib.rs", "mod a;\nmod locking;\n"), ("a.rs", a)], "");
    // Followed: self.refresh (One's), make(), and o.go() (`go` is One's alone). Not followed: unknown.refresh
    // (One's or Two's). iter/map/sum/len: standard-library names, not counted. Outside: the locking
    // fixture's `Guard(..)` constructor.
    let calls = out.check.calls;
    assert_eq!((calls.followed, calls.not_followed, calls.outside), (3, 1, 1), "{}", out.check.render());
}

/// A call inside `json!({..})` counts: a lock taken in a value, a nested one, or a parenthesized key is an
/// edge under the lock held around it (it was dropped once, until the call was moved out of the macro).
#[test]
fn calls_inside_json_are_followed() {
    let a = r#"
use std::sync::Mutex;
static A: Mutex<u32> = Mutex::new(0);
static B: Mutex<u32> = Mutex::new(0);
static C: Mutex<u32> = Mutex::new(0);
static D: Mutex<u32> = Mutex::new(0);
fn b() -> u32 { *crate::locking::lock(&B, "B") }
fn c() -> u32 { *crate::locking::lock(&C, "C") }
fn d() -> String { crate::locking::lock(&D, "D").to_string() }
pub fn view() -> serde_json::Value {
    let _a = crate::locking::lock(&A, "A");
    serde_json::json!({ "b": b(), "rows": [1, { "c": c() }], (d()): std::string::String::new(), "none": null })
}
"#;
    let (failures, _) = run("json", &[("lib.rs", "mod a;\nmod locking;\n"), ("a.rs", a)], "");
    let edges: Vec<&str> = failures.iter().filter_map(|f| f.strip_prefix("new lock order A → ")).map(|f| &f[..1]).collect();
    assert_eq!(edges, ["B", "C", "D"], "{failures:#?}");
}

/// A standard-library call is never taken for a crate method of the same name: `OpenOptions::new()…open(p)`
/// and `File::open(p)` beside a crate type with an `open` that takes a lock are no edge and no lock site.
#[test]
fn std_calls_are_never_a_same_named_crate_method() {
    let a = r#"
use std::sync::Mutex;
static A: Mutex<u32> = Mutex::new(0);
static FEEDS: Mutex<u32> = Mutex::new(0);
pub struct Feed;
impl Feed { pub fn open(_p: &str) -> Feed { let _g = crate::locking::lock(&FEEDS, "FEEDS"); Feed } }
pub fn write_log(p: &str) {
    let _a = crate::locking::lock(&A, "A");
    let _f = std::fs::OpenOptions::new().create_new(true).append(true).open(p);
    let _g = std::fs::File::open(p);
    let _h = (std::fs::OpenOptions::new()).write(true).open(p).map(|_| ());
}
pub fn feed(p: &str) -> Feed {
    let _a = crate::locking::lock(&A, "A");
    Feed::open(p)
}
"#;
    let (failures, _) = run("std-open", &[("lib.rs", "mod a;\nmod locking;\n"), ("a.rs", a)], "");
    assert_eq!(failures.len(), 1, "{failures:#?}");
    assert!(failures[0].starts_with("new lock order A → FEEDS: a::feed"), "only the crate's own Feed::open: {}", failures[0]);
}

/// `n` statement lines.
fn body(n: usize) -> String {
    "    x();\n".repeat(n)
}

#[test]
fn a_long_function_or_file_fails_the_check_and_test_code_is_exempt() {
    let long = format!("fn x() {{}}\npub fn long() {{\n{}}}\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn long_test() {{\n{}    }}\n}}\n", body(59), body(80));
    let file = format!("fn x() {{}}\n{}", (0..18).map(|i| format!("pub fn short_{i}() {{\n{}}}\n", body(56))).collect::<String>());
    let (failures, _) = run("sizes", &[("lib.rs", "mod a;\nmod big;\nmod locking;\n"), ("a.rs", &long), ("big.rs", &file)], "");
    assert_eq!(failures.len(), 2, "{failures:#?}");
    assert_eq!(failures[0], "src-tauri/src/a.rs:2 long is 61 lines, over 60: split it into a short function that reads as steps and helpers of their own");
    assert_eq!(failures[1], "src-tauri/src/big.rs is 1045 lines, over 1000: split it into modules along its seams");
}

#[test]
fn a_listed_file_is_exempt_until_it_fits_and_then_its_entry_is_stale() {
    let long_js = format!("function bind() {{\n{}}}\n", body(70));
    let short_js = "function ok() {\n  x();\n}\n";
    let ui = [("app.js", long_js.as_str()), ("chat.js", long_js.as_str()), ("core.js", short_js)];
    let allow = "[[size_exempt]]\nfile = \"ui/app.js\"\nreason = \"split by the UI track\"\n[[size_exempt]]\nfile = \"ui/core.js\"\nreason = \"split by the UI track\"\n";
    let failures = outcome_with_ui("js-sizes", &[("lib.rs", "mod locking;\n")], &ui, allow).check.failures;
    assert_eq!(failures.len(), 2, "{failures:#?}");
    assert!(failures[0].starts_with("ui/chat.js:1 bind is "), "{}", failures[0]);
    assert_eq!(failures[1], "[[size_exempt]] ui/core.js is within the size limits now: remove it from docs/dev/lint-allow.toml");
}
