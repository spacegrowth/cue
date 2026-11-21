//! What builds this Cue from a commit and installs it, as the Desk sees it (split from desk/model.rs): the app's own
//! is desk/hub/landing/install.rs, a test's its own.

/// How an install is going, as the installer says: its tag and commit, its step ("building" | "installing" |
/// "starting" | "installed" | "failed" | "rolled_back"), its log's end, and when (ms).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InstallStatus {
    pub tag: String,
    pub commit: String,
    pub step: String,
    pub log: String,
    pub at: u64,
}

/// What builds this Cue from a commit and installs it (the app restarts into it), and says how that's going.
pub trait Installer: Send + Sync {
    /// Whether the running Cue was built from `commit` of `repo`, or from one that has it.
    fn has(&self, repo: &str, commit: &str) -> bool;
    /// An install of `commit` of `repo` begun, called `tag`.
    fn start(&self, repo: &str, commit: &str, tag: &str) -> Result<(), String>;
    /// The latest install's status, if there's been one.
    fn status(&self) -> Option<InstallStatus>;
    /// The check `commit` of `repo` has to pass before it lands on the main line: None when the repository has no
    /// check, else what it said (an error with the end of its output when it failed).
    fn check(&self, _repo: &str, _commit: &str) -> Option<Result<(), String>> {
        None
    }
    /// Whether `of` of `repo` has `commit`: it is that commit or one after it.
    fn contains(&self, repo: &str, commit: &str, of: &str) -> bool;
    /// Whether the running Cue was built with `commit` of its add-on (`repo`, the add-on's repository), or one after
    /// it.
    fn has_addon(&self, repo: &str, commit: &str) -> bool {
        self.has(repo, commit)
    }
}

/// The installer a Desk uses: None for the app's own.
#[derive(Clone, Default)]
pub struct InstallerSlot(pub Option<std::sync::Arc<dyn Installer>>);

impl std::fmt::Debug for InstallerSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() { "InstallerSlot(set)" } else { "InstallerSlot(app)" })
    }
}
