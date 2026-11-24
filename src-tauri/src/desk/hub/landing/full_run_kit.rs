//! The full run's parts (docs/faster-checks-plan.md, docs/dev/test-speed.md "Full run"): what runs every test of Cue
//! at one commit (`FullRunner`: the app's is full_run_script.rs, a test's its own), what it's asked and what it
//! answers (scripts/full-run.sh's result file), and what Cue keeps of it between launches. No hub here: the Desk's
//! side is full_run.rs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How long a run on this machine waits for a moment when nothing of the project is being built or installed.
pub(crate) const QUIET_WAIT_MS: u64 = 30 * 60_000;

/// How long a run on another machine may take in all before Cue stops waiting and runs it here: an hour for the run
/// and its search for the commit that broke it, and 30 minutes more with no answer.
pub(crate) const MACHINE_LIMIT_MS: u64 = 90 * 60_000;

/// One run asked for: every test of `repo` (a main checkout on this machine) at commit `at`; `since`: the last commit
/// that passed ("" for none), so a failure is traced to a commit after it. `machine` "": on this machine; else that
/// machine (as Cue names it), reached as `host`, in the project's clone `folder` there.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FullRunAsk {
    pub repo: String,
    pub at: String,
    pub since: String,
    pub machine: String,
    pub host: String,
    pub folder: String,
}

/// The commit a failed run was traced to.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub(crate) struct Culprit {
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub subject: String,
}

/// What a run found, as its result file says (the fields Cue acts on).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub(crate) struct FullRunResult {
    /// The commit it ran at ("" when the file doesn't say).
    #[serde(default)]
    pub at: String,
    pub passed: bool,
    #[serde(default)]
    pub failed: Vec<String>,
    #[serde(default)]
    pub culprit: Option<Culprit>,
    #[serde(default)]
    pub culprit_note: Option<String>,
    #[serde(default)]
    pub tail: Option<String>,
}

/// A result file's text read: None when it isn't one.
pub(crate) fn parse(text: &str) -> Option<FullRunResult> {
    serde_json::from_str(text.trim()).ok()
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FullRunEnd {
    /// It ran: what it found.
    Result(FullRunResult),
    /// It couldn't be made (the machine didn't answer, the script refused): why.
    NotMade(String),
}

/// What runs every test of a repository at a commit, one run per repository at a time.
pub(crate) trait FullRunner: Send + Sync {
    /// A run begun; it goes on by itself. Err: why it can't begin.
    fn start(&self, ask: &FullRunAsk) -> Result<(), String>;
    /// Whether it knows of a run of `repo`, going or ended and not taken yet. A Cue that quit knows of none, though
    /// the run it left may still be going: asked for the same commits again, the script waits for that one and
    /// answers with its result (scripts/full-run.sh), so `start` with what was kept makes no second run.
    fn going(&self, repo: &str) -> bool;
    /// How `repo`'s run ended, once: None while it's going.
    fn take(&self, repo: &str) -> Option<FullRunEnd>;
    /// `repo`'s run given up on: stopped where it can be, its end never told.
    fn stop(&self, repo: &str);
    /// The step `repo`'s run is at now, as the script names it ("rust tests"), "" when it can't be told (a run on
    /// another machine says nothing till it ends).
    fn step(&self, _repo: &str) -> String {
        String::new()
    }
}

/// A run under way, as kept: the commits, where ("" this machine), when it began (ms), and why it runs on this
/// machine instead of the one picked ("" when it doesn't).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Running {
    pub at: String,
    #[serde(default)]
    pub since: String,
    #[serde(default)]
    pub on: String,
    #[serde(default)]
    pub began: u64,
    #[serde(default)]
    pub instead: String,
}

/// What Cue keeps of a repository's full runs between launches.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct FullRunState {
    /// The last commit a run was started on ("" when that run has to be made again).
    #[serde(default)]
    pub started: String,
    /// The last commit that passed.
    #[serde(default)]
    pub passed: String,
    #[serde(default)]
    pub running: Option<Running>,
    /// Since when a run on this machine waits for a quiet moment (ms; 0: none waits).
    #[serde(default)]
    pub wanted_since: u64,
    /// Why the next run goes on this machine instead of the one picked ("": it doesn't).
    #[serde(default)]
    pub instead: String,
    /// The last commit a run finished on, passed or failed ("" for none).
    #[serde(default)]
    pub judged: String,
    /// Since when the main line's head is a commit no run has finished on (ms; 0: one has). The install stops
    /// waiting for a run once this is two hours old (install_pick.rs).
    #[serde(default)]
    pub unrun_since: u64,
    /// The last commit an install may build as if it had passed: one that passed, or one whose run failed on tests
    /// no item of its batch caused ("": see `passed`).
    #[serde(default)]
    pub good: String,
    /// The commit whose run failed with no item to blame, while it is the one in `good` ("": that one passed).
    #[serde(default)]
    pub failing: String,
    /// Since when a failed run traced to an item holds the install on `good` (ms; 0: none does), and that item.
    /// The install stops waiting once this is four hours old (install_pick.rs); a run that passes clears it.
    #[serde(default)]
    pub held_since: u64,
    #[serde(default)]
    pub held_item: String,
    /// The commits a failed run was traced to and an item was told of: none is blamed twice.
    #[serde(default)]
    pub blamed: Vec<String>,
}

impl FullRunState {
    /// The commit an install may build as if it had passed ("" for none).
    pub(crate) fn good(&self) -> &str {
        if self.good.is_empty() { &self.passed } else { &self.good }
    }
}

/// How often the finishing tick looks at a repository nothing is running on.
pub(crate) const LOOK_EVERY_MS: u64 = 60_000;

/// When each Desk folder was last looked at (ms) and whether a run was going then, by Cue's folder and the Desk's.
static LOOKS: std::sync::Mutex<Vec<(String, u64, bool)>> = std::sync::Mutex::new(Vec::new());

fn look_key(folder: &str) -> String {
    format!("{}\n{folder}", crate::server::cue_dir().display())
}

/// Whether `folder` was looked at under a minute ago with nothing running: the tick leaves it till the minute is up.
pub(crate) fn looked_lately(folder: &str, now: u64) -> bool {
    let key = look_key(folder);
    let looks = crate::locking::lock(&LOOKS, "full run looks");
    looks.iter().any(|(k, at, running)| *k == key && !running && now.saturating_sub(*at) < LOOK_EVERY_MS)
}

/// `folder` looked at `now`, a run going on it or not.
pub(crate) fn looked(folder: &str, now: u64, running: bool) {
    let key = look_key(folder);
    let mut looks = crate::locking::lock(&LOOKS, "full run looks");
    looks.retain(|(k, _, _)| *k != key);
    looks.push((key, now, running));
}

/// Where the state is kept: one file in Cue's own folder, repository → its state.
fn state_file() -> std::path::PathBuf {
    crate::server::cue_dir().join("full-run.json")
}

fn load_all() -> HashMap<String, FullRunState> {
    std::fs::read_to_string(state_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// `repo`'s state as kept (never with the hub's store held: it reads a file).
pub(crate) fn load(repo: &str) -> FullRunState {
    load_all().remove(repo).unwrap_or_default()
}

/// What's kept of the repository in folder `repo`, however that folder is spelled: None when nothing is.
pub(crate) fn load_known(repo: &str) -> Option<FullRunState> {
    let all = load_all();
    let key = all.keys().find(|k| crate::desk::hub::landing::main_line::same_dir(k, repo)).cloned()?;
    all.get(&key).filter(|s| **s != FullRunState::default()).cloned()
}

/// `repo`'s state kept: written whole beside the file, then moved over it, so a quit mid-write loses nothing.
pub(crate) fn save(repo: &str, state: &FullRunState) {
    let mut all = load_all();
    all.insert(repo.to_string(), state.clone());
    let (file, tmp) = (state_file(), state_file().with_extension("json.tmp"));
    let done = serde_json::to_string_pretty(&all).map_err(|e| e.to_string()).and_then(|t| std::fs::write(&tmp, t).and_then(|()| std::fs::rename(&tmp, &file)).map_err(|e| e.to_string()));
    if let Err(e) = done {
        eprintln!("cue: desk: couldn't keep the full run's state: {e}");
    }
}

/// The items named by commit `subjects`, as a landing writes them ("… (#12)" at the end), in order, each once.
pub(crate) fn ids_in<'a>(subjects: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for s in subjects {
        let Some(id) = s.trim().strip_suffix(')').and_then(|s| s.rsplit_once("(#")).map(|(_, id)| id) else { continue };
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && !ids.iter().any(|i| i == id) {
            ids.push(id.to_string());
        }
    }
    ids
}

/// The names that failed, for a line: the first eight, and how many more.
pub(crate) fn failed_words(failed: &[String]) -> String {
    if failed.is_empty() {
        return "it didn't say what".into();
    }
    let shown = failed.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
    if failed.len() > 8 { format!("{shown} and {} more", failed.len() - 8) } else { shown }
}

/// " (#1, #2)" for the items a run covered; "" for none.
pub(crate) fn ids_words(ids: &[String]) -> String {
    if ids.is_empty() {
        return String::new();
    }
    format!(" ({})", ids.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(", "))
}

#[cfg(test)]
type Fakes = std::sync::Mutex<Vec<(std::path::PathBuf, std::sync::Arc<dyn FullRunner>)>>;

/// The runners tests gave, by the folder their World keeps Cue's data in.
#[cfg(test)]
static FAKES: Fakes = std::sync::Mutex::new(Vec::new());

/// `runner` in place of the app's own for the World whose folder is Cue's now (a test's; it stays across a relaunch).
#[cfg(test)]
pub(crate) fn set_fake(runner: std::sync::Arc<dyn FullRunner>) {
    let home = crate::server::cue_dir();
    let mut all = crate::locking::lock(&FAKES, "full run fakes");
    all.retain(|(h, _)| *h != home);
    all.push((home, runner));
}

/// The runner a test gave this World, if it gave one: a World with none makes no full run at all.
#[cfg(test)]
pub(crate) fn fake() -> Option<std::sync::Arc<dyn FullRunner>> {
    let home = crate::server::cue_dir();
    let all = crate::locking::lock(&FAKES, "full run fakes");
    all.iter().find(|(h, _)| *h == home).map(|(_, r)| r.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_file_is_read_and_anything_else_isnt_one() {
        let r = parse(r#"{"at":"a1","since":null,"passed":false,"started":1,"ended":2,"steps":[{"name":"build","ok":true,"secs":3,"log":"x"}],"failed":["a::b","tests/x.test.js"],"culprit":{"commit":"c3","subject":"Footer (#12)"},"culprit_note":null,"tail":"boom"}"#).unwrap();
        assert_eq!((r.passed, r.failed.len(), r.tail.as_deref()), (false, 2, Some("boom")));
        assert_eq!(r.culprit, Some(Culprit { commit: "c3".into(), subject: "Footer (#12)".into() }));
        let ok = parse(r#"{"passed":true,"failed":[],"culprit":null,"culprit_note":null,"tail":null}"#).unwrap();
        assert!(ok.passed && ok.culprit.is_none());
        assert_eq!(parse("ssh: connection refused"), None);
        assert_eq!(parse(r#"{"failed":[]}"#), None, "no word on whether it passed");
    }

    #[test]
    fn items_are_named_by_the_end_of_a_landings_subject() {
        let subjects = ["Header jumps (#12)", "Tidy the readme", "Footer (see #9) (#14)", "Header again (#12)", "Odd (#)", "Half (#1 2)"];
        assert_eq!(ids_in(subjects), vec!["12".to_string(), "14".to_string()]);
        assert_eq!(ids_words(&ids_in(subjects)), " (#12, #14)");
        assert_eq!(ids_words(&[]), "");
    }

    #[test]
    fn many_failed_names_are_cut_short() {
        let names: Vec<String> = (1..=10).map(|n| format!("t{n}")).collect();
        assert_eq!(failed_words(&names), "t1, t2, t3, t4, t5, t6, t7, t8 and 2 more");
        assert_eq!(failed_words(&names[..2]), "t1, t2");
    }
}
