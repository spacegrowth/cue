//! Words Cue finds by itself for the check before every push (push_check.rs), on top of the ones you set: the Mac
//! account's full name, the signed-in Claude account's name, email and organization (not a generic one), and, for a
//! push of a public project, the names of your other projects that aren't public. Worked out when asked, kept a
//! short while so a push isn't slowed, and never written anywhere.
//!
//! A project is public when its repository's shared remote is a GitHub repository that GitHub says is public
//! (`gh repo view`, its answer kept per repository); with no shared remote, or when Cue can't tell, it's private.

// Under test only the words a test sets are used: nothing is read off this Mac or asked of GitHub.
#![cfg_attr(test, allow(dead_code))]

use serde_json::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// How often the words found are worked out again, on their own thread (`keep_warm`).
const REFRESH: Duration = Duration::from_secs(120);
/// How long GitHub's answer about a repository is kept; one Cue couldn't get, a shorter while.
const KEEP_SEEN: Duration = Duration::from_secs(3600);
const KEEP_UNSURE: Duration = Duration::from_secs(300);
/// At most this long for each `gh repo view` (they run side by side).
const GH_CAP: Duration = Duration::from_secs(8);

/// Names too short or too common to refuse a push over.
const COMMON: &[&str] = &[
    "web", "api", "app", "apps", "src", "test", "tests", "docs", "doc", "main", "server", "client", "site", "www", "code", "core", "lib", "libs",
    "tools", "tool", "utils", "util", "data", "home", "work", "notes", "scripts", "infra", "config", "backend", "frontend", "mobile", "desktop",
    "demo", "example", "examples", "sample", "temp", "tmp", "scratch", "sandbox", "playground", "project", "projects", "repo", "website", "blog",
    "service", "services", "common", "shared", "build", "deploy", "admin", "dashboard", "plugin", "plugins", "extension", "template", "static",
    "assets", "public", "private", "experiments", "spike", "draft", "next", "new", "old", "personal", "development", "dev", "staging", "prod",
];

/// A project Cue knows on this Mac: its name, the name it shows under, its repository's folder name, and whether
/// it's public.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Known {
    pub project: String,
    pub shown: String,
    pub folder: String,
    pub public: bool,
}

/// What Cue found: words for every push (your account names), and the projects it knows.
#[derive(Debug, Clone, Default)]
pub(crate) struct Found {
    pub mine: Vec<String>,
    pub projects: Vec<Known>,
}

impl Found {
    /// The words found for a push of `project`: yours, and the other projects' names that aren't public when
    /// `project` is.
    pub(crate) fn words_for(&self, project: &str) -> Vec<String> {
        let mut out = vec![];
        for w in self.mine.iter().cloned().chain(private_names(project, &self.projects)) {
            add(&mut out, w);
        }
        out
    }

    /// Every project name that would be kept out of a public project's push.
    pub(crate) fn private_names(&self) -> Vec<String> {
        private_names("", &self.projects)
    }
}

fn add(out: &mut Vec<String>, w: String) {
    let w = w.trim().to_string();
    if !w.is_empty() && !out.iter().any(|x: &String| x.eq_ignore_ascii_case(&w)) {
        out.push(w);
    }
}

/// The words Cue finds for a push of `project`.
pub(crate) fn words_for(project: &str) -> Vec<String> {
    found().words_for(project)
}

/// What Cue finds now, from what it kept when that's recent.
pub(crate) fn found() -> Found {
    #[cfg(test)]
    return TEST_FOUND.with(|f| f.borrow().clone());
    #[cfg(not(test))]
    found_now()
}

/// The words found last, worked out once here only when there are none yet; after that `keep_warm` renews them,
/// so a commit or push never waits on `gh` or git.
#[cfg(not(test))]
static KEPT: Mutex<Option<Found>> = Mutex::new(None);
/// Held while the words are worked out, so only one works them out at a time.
#[cfg(not(test))]
static WORKING: Mutex<()> = Mutex::new(());

/// `found`, off the Mac and the Claude account.
#[cfg(not(test))]
fn found_now() -> Found {
    let kept = crate::locking::lock(&KEPT, "found words");
    let last = kept.clone();
    drop(kept);
    if let Some(f) = last {
        return f;
    }
    work_out(false)
}

/// Work the words out and keep them. Unless `again`, words someone else just worked out are used instead.
#[cfg(not(test))]
fn work_out(again: bool) -> Found {
    let working = crate::locking::lock(&WORKING, "working out found words");
    if !again {
        let kept = crate::locking::lock(&KEPT, "found words");
        let last = kept.clone();
        drop(kept);
        if let Some(f) = last {
            drop(working);
            return f;
        }
    }
    let f = Found { mine: account_words(&claude_account(), &mac_full_name()), projects: known_projects() };
    let mut kept = crate::locking::lock(&KEPT, "found words");
    *kept = Some(f.clone());
    drop(kept);
    drop(working);
    f
}

/// Work the found words out on a thread of their own: soon after Cue starts (once its sessions' folders are known),
/// then every couple of minutes, so a commit or push uses them as they are and never waits on `gh`.
pub(crate) fn keep_warm() {
    #[cfg(not(test))]
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(15));
        loop {
            let _ = work_out(true);
            std::thread::sleep(REFRESH);
        }
    });
}

#[cfg(test)]
thread_local! {
    static TEST_FOUND: std::cell::RefCell<Found> = std::cell::RefCell::new(Found::default());
}

/// A test's found words, on its own thread, in place of the Mac's.
#[cfg(test)]
pub(crate) fn set_for_test(f: Found) {
    TEST_FOUND.with(|t| *t.borrow_mut() = f);
}

/// The signed-in Claude account (~/.claude.json's `oauthAccount`), `{}` when there's none.
fn claude_account() -> Value {
    let home = std::env::var("HOME").unwrap_or_default();
    std::fs::read_to_string(std::path::Path::new(&home).join(".claude.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("oauthAccount").cloned())
        .unwrap_or(Value::Null)
}

/// The Mac account's full name (`id -F`), "" when it can't be read.
fn mac_full_name() -> String {
    crate::terminal::focus::capped(std::process::Command::new("/usr/bin/id").arg("-F"), Duration::from_secs(2))
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Your words from the Claude account `account` (its `oauthAccount`) and the Mac account's full name `mac_name`:
/// the names, the email, and the organization unless it's a generic one.
pub(crate) fn account_words(account: &Value, mac_name: &str) -> Vec<String> {
    let mut out = vec![];
    add(&mut out, mac_name.to_string());
    let field = |k: &str| account.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    for k in ["fullName", "displayName", "emailAddress"] {
        add(&mut out, field(k));
    }
    let org = field("organizationName");
    if !generic_org(&org) {
        add(&mut out, org);
    }
    out
}

/// An organization name Claude makes up, not yours: "Personal", "Default", "…'s Organization", "…'s Workspace".
pub(crate) fn generic_org(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    n.is_empty()
        || ["personal", "default", "individual", "my organization", "organization"].contains(&n.as_str())
        || n.ends_with("'s organization")
        || n.ends_with("\u{2019}s organization")
        || n.ends_with("'s workspace")
        || n.ends_with("\u{2019}s workspace")
        || n.ends_with("'s individual org")
}

/// A name worth refusing a push over: 4 characters or more, and not a common word.
pub(crate) fn telling(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    n.chars().count() >= 4 && !COMMON.contains(&n.as_str())
}

/// The macOS word list: one word a line, lowercase entries are everyday words (capitalized ones are proper names).
const WORD_LIST: &str = "/usr/share/dict/words";

static EVERYDAY: LazyLock<std::collections::HashSet<String>> = LazyLock::new(|| {
    std::fs::read_to_string(WORD_LIST)
        .map(|all| all.lines().filter(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase())).map(str::to_string).collect())
        .unwrap_or_default()
});

/// An ordinary English word, alone or as a plural ("gadget", "gadgets", "libraries"): a project named so isn't a secret,
/// and refusing every push that says it would block ordinary commits.
pub(crate) fn everyday(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    let words = &*EVERYDAY;
    let plain = |w: &str| !w.is_empty() && words.contains(w);
    plain(&n)
        || n.strip_suffix('s').is_some_and(plain)
        || n.strip_suffix("es").is_some_and(plain)
        || n.strip_suffix("ies").is_some_and(|w| plain(&format!("{w}y")))
}

/// The names of the projects in `projects` that aren't public, when `pushed` is (none otherwise): each one's shown
/// name and folder name, the telling ones, never `pushed`'s own. `pushed` "" lists them all.
pub(crate) fn private_names(pushed: &str, projects: &[Known]) -> Vec<String> {
    let own: Vec<&Known> = projects.iter().filter(|k| !pushed.is_empty() && k.project == pushed).collect();
    if !pushed.is_empty() && !own.iter().any(|k| k.public) {
        return vec![];
    }
    let is_own = |w: &str| w.eq_ignore_ascii_case(pushed) || own.iter().any(|k| k.shown.eq_ignore_ascii_case(w) || k.folder.eq_ignore_ascii_case(w));
    let mut out = vec![];
    for k in projects.iter().filter(|k| !k.public && (pushed.is_empty() || k.project != pushed)) {
        for w in [&k.shown, &k.folder] {
            if telling(w) && !everyday(w) && !is_own(w) {
                add(&mut out, w.clone());
            }
        }
    }
    out
}

/// The projects Cue knows on this Mac: the repositories its sessions' folders are in, and the folders you put in a
/// project, each with whether it's public.
fn known_projects() -> Vec<Known> {
    let mut roots: Vec<String> = crate::logs::repos::known_roots().into_iter().map(|r| r.to_string_lossy().to_string()).collect();
    for folder in crate::config::folders_put().keys().filter(|k| k.starts_with('/')) {
        roots.push(crate::logs::repos::repo_root(folder).map(|r| r.to_string_lossy().to_string()).unwrap_or_else(|| folder.clone()));
    }
    roots.sort();
    roots.dedup();
    let names = crate::config::load().pointer("/project/names").cloned().unwrap_or(Value::Null);
    let repos: Vec<Option<String>> = roots.iter().map(|r| github_repo(r)).collect();
    let public = visibility(&repos.iter().flatten().cloned().collect::<Vec<_>>());
    roots
        .iter()
        .zip(repos)
        .map(|(root, repo)| {
            let project = crate::model::project_at(root, "");
            let shown = names.get(&project).and_then(Value::as_str).unwrap_or(&project).to_string();
            let folder = crate::logs::repos::repo_name(std::path::Path::new(root));
            Known { public: repo.is_some_and(|r| public.get(&r).copied().unwrap_or(false)), project, shown, folder }
        })
        .collect()
}

/// The GitHub repository ("owner/name") `root`'s shared remote is: origin, else its only remote. None with no
/// remote, or one that isn't on github.com.
fn github_repo(root: &str) -> Option<String> {
    let all = crate::desk::hub::landing::git::git("", root, &[], &["remote"])?;
    let all: Vec<&str> = all.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let name = if all.contains(&"origin") { "origin" } else if all.len() == 1 { all[0] } else { return None };
    let url = crate::desk::hub::landing::git::git("", root, &[], &["config", "--get", &format!("remote.{name}.url")])?;
    github_of(url.trim())
}

/// "owner/name" from a github.com remote's address (https, ssh, or git@github.com:owner/name.git).
pub(crate) fn github_of(url: &str) -> Option<String> {
    let rest = ["https://github.com/", "http://github.com/", "ssh://git@github.com/", "git@github.com:", "git://github.com/"].iter().find_map(|p| url.strip_prefix(p))?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    let (Some(o), Some(n), None) = (parts.next(), parts.next(), parts.next()) else { return None };
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (ok(o) && ok(n)).then(|| format!("{o}/{n}"))
}

/// Whether each of `repos` is public, as GitHub says (kept a while per repository); false when it can't tell.
fn visibility(repos: &[String]) -> HashMap<String, bool> {
    static SEEN: LazyLock<Mutex<HashMap<String, (Instant, Option<bool>)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
    let seen = crate::locking::lock(&SEEN, "repo visibility");
    let mut out: HashMap<String, bool> = HashMap::new();
    let mut ask: Vec<String> = vec![];
    for r in repos {
        match seen.get(r) {
            Some((at, Some(p))) if at.elapsed() < KEEP_SEEN => {
                out.insert(r.clone(), *p);
            }
            Some((at, None)) if at.elapsed() < KEEP_UNSURE => {
                out.insert(r.clone(), false);
            }
            _ if !ask.contains(r) => ask.push(r.clone()),
            _ => {}
        }
    }
    drop(seen);
    let asked: Vec<(String, Option<bool>)> = ask
        .into_iter()
        .map(|r| std::thread::spawn(move || { let p = gh_public(&r); (r, p) }))
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|h| h.join().ok())
        .collect();
    let mut seen = crate::locking::lock(&SEEN, "repo visibility");
    for (r, p) in asked {
        seen.insert(r.clone(), (Instant::now(), p));
        out.insert(r, p.unwrap_or(false));
    }
    drop(seen);
    out
}

/// GitHub's answer for `repo`: Some(true) public, Some(false) not, None when gh couldn't say.
fn gh_public(repo: &str) -> Option<bool> {
    let mut cmd = std::process::Command::new(crate::desk::hub::landing::pr::gh_bin());
    cmd.args(["repo", "view", repo, "--json", "visibility", "--jq", ".visibility"]).env("NO_COLOR", "1");
    let out = crate::terminal::focus::capped(&mut cmd, GH_CAP).ok().filter(|o| o.status.success())?;
    let said = String::from_utf8_lossy(&out.stdout).trim().to_uppercase();
    match said.as_str() {
        "PUBLIC" => Some(true),
        "PRIVATE" | "INTERNAL" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn k(project: &str, shown: &str, folder: &str, public: bool) -> Known {
        Known { project: project.into(), shown: shown.into(), folder: folder.into(), public }
    }

    #[test]
    fn the_account_fields_are_read_from_a_sample() {
        let acct = json!({ "fullName": "Pat Example", "displayName": "Pat", "emailAddress": "pat@example.com", "organizationName": "Acme Widgets Ltd", "accountUuid": "x" });
        assert_eq!(account_words(&acct, "Pat Q Example"), ["Pat Q Example", "Pat Example", "Pat", "pat@example.com", "Acme Widgets Ltd"]);
        assert_eq!(account_words(&Value::Null, ""), Vec::<String>::new(), "no account, no Mac name: nothing");
    }

    #[test]
    fn a_generic_organization_is_skipped() {
        for g in ["Personal", "pat@example.com's Organization", "Pat Example\u{2019}s Organization", "", "Default"] {
            assert!(generic_org(g), "{g}");
            let acct = json!({ "fullName": "Pat Example", "organizationName": g });
            assert_eq!(account_words(&acct, ""), ["Pat Example"], "{g}");
        }
        assert!(!generic_org("Acme Widgets Ltd"));
    }

    #[test]
    fn private_projects_are_added_and_public_ones_arent() {
        let all = [k("widget", "widget", "widget", true), k("acme-secret", "Acme Secret", "acme-secret", false), k("opentool", "opentool", "opentool", true)];
        assert_eq!(private_names("widget", &all), ["Acme Secret", "acme-secret"]);
        let f = Found { mine: vec!["Pat Example".into()], projects: all.to_vec() };
        assert_eq!(f.words_for("widget"), ["Pat Example", "Acme Secret", "acme-secret"]);
        assert_eq!(f.private_names(), ["Acme Secret", "acme-secret"]);
    }

    #[test]
    fn a_private_push_gets_no_project_names() {
        let all = [k("hidden", "hidden", "hidden", false), k("acme-secret", "acme-secret", "acme-secret", false)];
        assert_eq!(private_names("hidden", &all), Vec::<String>::new(), "a private project's push may name its siblings");
        assert_eq!(private_names("unknown-project", &all), Vec::<String>::new(), "a project Cue can't tell about counts as private");
    }

    #[test]
    fn short_and_common_names_are_skipped() {
        let all = [k("widget", "widget", "widget", true), k("web", "web", "web", false), k("api", "API", "api", false), k("xy", "xy", "xy", false), k("server", "Server", "server", false), k("acme-secret", "acme-secret", "acme-secret", false)];
        assert_eq!(private_names("widget", &all), ["acme-secret"]);
        assert!(!telling("abc") && telling("abcd") && !telling("Docs") && !telling("MAIN"));
    }

    #[test]
    fn a_project_named_with_an_everyday_word_isnt_a_secret() {
        // "gadgets" is a word anyone writes; a private project folder named so mustn't block pushes that say it.
        let all = [k("widget", "widget", "widget", true), k("gadgets", "gadgets", "gadgets", false), k("Library", "Library", "library", false), k("acme-secret", "Acme Secret", "acme-secret", false), k("quuxcorp", "quuxcorp", "quuxcorp", false)];
        assert_eq!(private_names("widget", &all), ["Acme Secret", "acme-secret", "quuxcorp"]);
        assert!(everyday("gadgets") && everyday("Gadget") && everyday("libraries") && everyday("branches"));
        assert!(!everyday("quuxcorp") && !everyday("acme-secret") && !everyday("Acme Secret"));
    }

    #[test]
    fn the_project_being_pushed_is_never_included() {
        // Its docs repository, private, goes with it: its names aren't the pushed project's secret.
        let all = [k("widget", "Widget Board", "widget", true), k("widget", "Widget Board", "widget-docs", false), k("acme-secret", "acme-secret", "acme-secret", false), k("other", "widget", "widget2", false)];
        let got = private_names("widget", &all);
        assert_eq!(got, ["acme-secret", "widget2"], "{got:?}");
    }

    #[test]
    fn github_addresses_are_read() {
        assert_eq!(github_of("git@github.com:pat-example/widget.git").as_deref(), Some("pat-example/widget"));
        assert_eq!(github_of("https://github.com/pat-example/widget").as_deref(), Some("pat-example/widget"));
        assert_eq!(github_of("ssh://git@github.com/pat-example/widget.git/").as_deref(), Some("pat-example/widget"));
        assert_eq!(github_of("https://git.example.com/pat/widget.git"), None, "not on GitHub: can't tell, so private");
        assert_eq!(github_of("/tmp/remote.git"), None);
    }
}
