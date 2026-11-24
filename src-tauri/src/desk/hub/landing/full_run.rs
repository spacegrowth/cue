//! All the tests run once, after landing (docs/faster-checks-plan.md): on Cue's own project, when its main line has
//! moved since the last full run, one run of every test is started at its head (full_run_kit.rs `FullRunner`), on this
//! machine or the one picked for it (`project.lands.<project>.full_run_on`), one at a time. Its end is one line for
//! the Desk's lead; a failure traced to an item's commit opens that item again with its holder and holds the install for four hours at most; a failure no item caused holds
//! nothing (install_pick.rs). What's kept between
//! launches is in Cue's own folder (full_run_kit.rs `FullRunState`).

use crate::desk::feed::ChangeKind;
use crate::desk::hub::landing::finish::Claim;
use crate::desk::hub::landing::full_run_kit::{self as kit, Culprit, FullRunAsk, FullRunEnd, FullRunResult, FullRunState, FullRunner, Running};
use crate::desk::hub::landing::git::git;
use crate::desk::hub::landing::install_pick::{pick, Pick};
use crate::desk::hub::prelude::*;
use crate::desk::DeskItem;
use std::sync::Arc;

/// The last commit of `repo`'s main line (its main checkout's folder) that passed a full run, if one has.
pub(crate) fn last_passed(repo: &str) -> Option<String> {
    kit::load_known(repo).map(|s| s.passed).filter(|p| !p.is_empty())
}

/// Which commit an install of `repo` builds now, its main line at `tip` (install_pick.rs). It reads a file and asks
/// git: never with the hub's store held. A World with no runner makes no full run, so its installs take the tip.
pub(crate) fn install_pick(repo: &str, tip: &str, now: u64) -> Pick {
    let Some(runner) = full_runner() else { return Pick::Tip };
    let state = kit::load_known(repo);
    let going = state.is_none() && runner.going(repo);
    pick(state.as_ref(), going, tip, |passed| crate::install::self_install::is_ancestor(repo, passed, tip), now)
}

/// What makes the runs: the app's own; a test's World its own, and none at all unless it gave one.
fn full_runner() -> Option<Arc<dyn FullRunner>> {
    #[cfg(test)]
    return kit::fake();
    #[cfg(not(test))]
    Some(crate::desk::hub::landing::full_run_script::runner())
}

/// What a run's end comes to, told once its state is kept.
struct Told {
    kind: ChangeKind,
    words: String,
    /// The item opened again, with what its holder reads: (id, what, more).
    reopen: Option<(String, String, String)>,
    /// An item already open: what its holder is told, and where (to, words).
    note: Option<(String, String)>,
    /// The same failure the lead was told of last: not told again (#820).
    quiet: bool,
}

impl Told {
    fn line(kind: ChangeKind, words: String) -> Told {
        Told { kind, words, reopen: None, note: None, quiet: false }
    }
}

/// How many of a failed run's last lines its row for the lead carries (#820): the rest is in its log.
const ROW_TAIL_LINES: usize = 6;

/// What a failed run's row for the lead ends with: its output's last few lines, each cut short, and where its log is
/// (on the machine it ran on). The holder of an item it's traced to reads the longer tail (`tail_words`).
fn row_tail(res: &FullRunResult, r: &Running) -> String {
    let mut out = String::new();
    if let Some(t) = res.tail.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        let lines: Vec<&str> = t.lines().collect();
        let last: Vec<String> = lines[lines.len().saturating_sub(ROW_TAIL_LINES)..].iter().map(|l| cut(l, 200)).collect();
        out.push_str(&format!("\nThe end of its output:\n{}", last.join("\n")));
    }
    let log = res.failed_log();
    if !log.is_empty() {
        let on = if r.on.is_empty() { String::new() } else { format!(" on {}", crate::machines::display_name(&r.on)) };
        out.push_str(&format!("\nThe whole log: {log}{on}"));
    }
    out
}

/// `line` cut to `n` characters at most, with "…" when cut.
fn cut(line: &str, n: usize) -> String {
    let mut c = line.chars();
    let mut s: String = c.by_ref().take(n).collect();
    if c.next().is_some() {
        s.push('…');
    }
    s
}

/// What makes two failed runs the same failure for the lead: what failed, and the commit blamed.
fn failure_key(res: &FullRunResult) -> String {
    let mut failed = res.failed.clone();
    failed.sort();
    format!("{}\n{}", res.culprit.as_ref().map_or("", |c| c.commit.as_str()), failed.join("\n"))
}

/// Run `r` failed with no item to blame (no commit found, or one that is no item's landed commit): nothing holds the
/// install, its commit is built as one that passed, and that its tests failed is kept apart.
/// `docs_only`: its culprit changed only documents, so it can't have broken anything.
fn failed_on_nobody(s: &mut FullRunState, r: &Running, res: &FullRunResult, docs_only: bool) -> Told {
    (s.good, s.failing, s.unrun_since) = (r.at.clone(), r.at.clone(), 0);
    (s.held_since, s.held_item) = (0, String::new());
    let why = match res.culprit.as_ref().filter(|c| !c.commit.is_empty()) {
        Some(c) if docs_only => format!("the search ended on commit {} ({}), which changed only documents, so what failed fails only sometimes", short(&c.commit), c.subject),
        Some(c) => format!("commit {} ({}) broke it, which is no item's landed commit on this Desk", short(&c.commit), c.subject),
        None => res.culprit_note.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("the commit that broke it wasn't found").to_string(),
    };
    let words = format!("Full run failed on main, on tests no item in this batch caused: {}. Why: {why}. No item was reopened and the app still updates.{}{}", kit::failed_words(&res.failed), instead_words(r), row_tail(res, r));
    Told::line(ChangeKind::FullRunFailed, words)
}

/// Whether item `it` has landed with commit `commit` and isn't at work again: checked or to check, or finishing with
/// that very commit (waiting for its install). At work, or landing something newer, it isn't.
fn landed_with(it: &DeskItem, commit: &str) -> bool {
    matches!(it.state.as_str(), "to_check" | "done") || (it.state == "finishing" && it.finish.as_ref().is_some_and(|f| f.commit == commit))
}

/// Run `r` failed and commit `c`, item `it`'s own, broke it: the install is held on the last commit that passed (four
/// hours at most from the first such failure), and the item is opened again, once per commit and only when it has
/// landed; one at work again has its holder told instead.
fn failed_on_item(s: &mut FullRunState, r: &Running, res: &FullRunResult, c: &Culprit, it: &DeskItem, now: u64) -> Told {
    if s.held_since == 0 {
        s.held_since = now;
    }
    s.held_item = it.id.clone();
    let (id, failed) = (&it.id, kit::failed_words(&res.failed));
    let twice = s.blamed.contains(&c.commit);
    if !twice {
        s.blamed.push(c.commit.clone());
        let extra = s.blamed.len().saturating_sub(50);
        s.blamed.drain(..extra);
    }
    let mut told = Told::line(ChangeKind::FullRunFailed, String::new());
    let did = if twice {
        format!("#{id} was already sent back for that commit: not reopened again")
    } else if landed_with(it, &c.commit) {
        told.reopen = Some((id.clone(), format!("the full run of main failed after it landed ({failed})"), tail_words(res)));
        format!("#{id} is open again with its holder; the other items stay landed")
    } else {
        let words = format!("Desk item #{id} ({}): the full run of main failed on its commit {} ({failed}). It is open with you already: fix that within #{id} too.{}", it.title, short(&c.commit), tail_words(res));
        told.note = it.holder.as_ref().map(|h| (h.to().to_string(), words));
        format!("#{id} is at work already, so it wasn't reopened; its holder was told")
    };
    told.words = format!("Full run failed on main: {failed}. Commit {} ({}) broke it: {did}. The app stays on the last version that passed, for 4 hours at most.{}", short(&c.commit), c.subject, instead_words(r));
    told
}

/// " It ran on this machine instead: <why>." when it did.
fn instead_words(r: &Running) -> String {
    if r.instead.is_empty() { String::new() } else { format!(" It ran on this machine instead: {}.", r.instead) }
}

/// "\nThe end of its output:\n<tail>" when there's one.
fn tail_words(res: &FullRunResult) -> String {
    res.tail.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(|t| format!("\nThe end of its output:\n{t}")).unwrap_or_default()
}

/// The items landed on `repo`'s main line by run `r`'s commits (after the last that passed; with none, its head's).
fn batch(repo: &str, r: &Running) -> Vec<String> {
    let range = format!("{}..{}", r.since, r.at);
    let said = if r.since.is_empty() { git("", repo, &[], &["log", "-1", "--format=%s", &r.at]) } else { git("", repo, &[], &["log", "--first-parent", "--reverse", "--format=%s", &range]) };
    kit::ids_in(said.unwrap_or_default().lines())
}

impl Hub {
    /// The look at Cue's own projects, after a landing and on the finishing tick: a run that ended is acted on, one
    /// due is started. One look at a time.
    pub(crate) fn full_run_tick(&self) {
        self.full_run_look(false);
    }

    /// The same look at once, whenever the last one was: a landing just moved the main line.
    pub(crate) fn full_run_now(&self) {
        self.full_run_look(true);
    }

    /// The look: `at_once` after a landing; else a project with no run going is looked at once a minute at most
    /// (each look reads files and asks git).
    fn full_run_look(&self, at_once: bool) {
        let Some(runner) = full_runner() else { return };
        let Some(_claim) = Claim::take(self, "full run") else { return };
        let desks: Vec<(String, String)> = self.read(|st| {
            let mut names: Vec<&String> = st.desk.spaces.keys().chain(st.desk.lead.keys()).collect();
            names.sort();
            names.dedup();
            names.into_iter().map(|d| (d.clone(), crate::desk::hub::core::view::desk_folder(st, d))).filter(|(_, f)| !f.is_empty()).collect()
        });
        let (mut seen, now): (Vec<String>, u64) = (Vec::new(), now_ms());
        for (desk, folder) in desks {
            if !at_once && kit::looked_lately(&folder, now) {
                continue;
            }
            let repo = Some(&folder).filter(|f| crate::hub::builds_this_cue(f)).and_then(|f| crate::desk::hub::landing::main_line::main_checkout(f)).map(|(repo, _)| repo);
            let running = match repo.filter(|r| !seen.contains(r) && crate::hub::builds_this_cue(r)) {
                Some(repo) => {
                    seen.push(repo.clone());
                    self.full_run_step(&desk, &repo, &*runner)
                }
                None => false,
            };
            kit::looked(&folder, now, running);
        }
    }

    /// One look at `repo` (Desk `desk`'s main checkout): its run's end taken, a run lost to a quit or to a machine
    /// that stopped answering set to be made again, the next one started; what changed kept before anything is told.
    /// Whether a run is going after it.
    fn full_run_step(&self, desk: &str, repo: &str, runner: &dyn FullRunner) -> bool {
        let was = kit::load(repo);
        let (mut s, now, mut told) = (was.clone(), now_ms(), None);
        if let Some(r) = s.running.clone() {
            match runner.take(repo) {
                Some(end) => {
                    s.running = None;
                    told = self.full_run_ended(desk, repo, &mut s, &r, end, now);
                }
                // Cue quit under it: asked for again, the same commits.
                None if !runner.going(repo) => self.full_run_same(desk, repo, &mut s, runner, &r, now),
                None if !r.on.is_empty() && now.saturating_sub(r.began) > kit::MACHINE_LIMIT_MS => {
                    runner.stop(repo);
                    (s.running, s.started) = (None, String::new());
                    s.instead = format!("{} gave no answer", crate::machines::display_name(&r.on));
                }
                None => {}
            }
        }
        if s.running.is_none() {
            self.full_run_next(desk, repo, &mut s, runner, now);
        }
        if s != was {
            kit::save(repo, &s);
        }
        self.full_run_shown(desk, repo, &s, runner);
        let running = s.running.is_some();
        let Some(t) = told else { return running };
        if let Some((id, what, more)) = &t.reopen {
            self.desk_open_again(id, what, "Fix it", more);
        }
        if let Some(note) = t.note.clone() {
            self.desk_done(crate::desk::hub::core::desk::Effects { sends: vec![note], ..Default::default() });
        }
        if !t.quiet {
            self.desk_feed_line(desk, t.kind, &t.words, &t.words);
        }
        running
    }

    /// The full run of `repo` going, kept for the items it carries (now_line.rs, each says it's being tested): the
    /// window drawn again when it changed. The run's items are asked of git once per run.
    fn full_run_shown(&self, desk: &str, repo: &str, s: &FullRunState, runner: &dyn FullRunner) {
        let line = s.running.as_ref().map(|r| {
            static ITEMS: std::sync::Mutex<Vec<(String, Vec<String>)>> = std::sync::Mutex::new(Vec::new());
            let key = format!("{repo}\n{}..{}", r.since, r.at);
            let known = crate::locking::lock(&ITEMS, "full run items").iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone());
            let items = match known {
                Some(v) => v,
                None => {
                    let v = batch(repo, r);
                    let mut all = crate::locking::lock(&ITEMS, "full run items");
                    all.retain(|(k, _)| !k.starts_with(&format!("{repo}\n")));
                    all.push((key, v.clone()));
                    v
                }
            };
            let on = if r.on.is_empty() { String::new() } else { crate::machines::display_name(&r.on) };
            let step = if on.is_empty() { runner.step(repo) } else { String::new() };
            (step, on, items)
        });
        let line = line.and_then(|(step, on, items)| crate::desk::hub::landing::now_line::full_line(s, &step, &on, items));
        if crate::desk::hub::landing::now_line::set_full(desk, line) {
            self.redraw();
        }
    }

    /// Run `r` of `repo` (Desk `desk`) ended with `end`: what's kept of it in `s`, and what to tell (nothing when its
    /// machine couldn't make it: it's made here next).
    fn full_run_ended(&self, desk: &str, repo: &str, s: &mut FullRunState, r: &Running, end: FullRunEnd, now: u64) -> Option<Told> {
        let res = match end {
            FullRunEnd::Result(res) => res,
            FullRunEnd::NotMade(why) if !r.on.is_empty() => {
                s.started = String::new();
                s.instead = format!("{} couldn't run it ({})", crate::machines::display_name(&r.on), why.trim());
                return None;
            }
            FullRunEnd::NotMade(why) => {
                return Some(Told::line(ChangeKind::FullRunFailed, format!("Full run couldn't be made on main at {}: {}. Nothing was reopened; it's tried again once main moves.", short(&r.at), why.trim())));
            }
        };
        // A run finished on it. The install's two hours start again only when it gave the install a newer commit to
        // build (it passed, or failed on no item): a failure traced to an item leaves that clock alone, so failing
        // runs one after another can't keep both it and the four hours from ever being up.
        s.judged = r.at.clone();
        if res.passed {
            s.told_failure.clear();
            (s.passed, s.good, s.failing, s.unrun_since) = (r.at.clone(), r.at.clone(), String::new(), 0);
            (s.held_since, s.held_item) = (0, String::new());
            return Some(Told::line(ChangeKind::FullRunPassed, format!("Full run passed on main{}.{}", kit::ids_words(&batch(repo, r)), instead_words(r))));
        }
        let culprit = res.culprit.as_ref().filter(|c| !c.commit.is_empty());
        // A commit that changed only documents broke nothing (the script says so too; one on another machine may be older).
        let docs_only = culprit.is_some_and(|c| only_documents(repo, &c.commit));
        let mut told = match culprit.filter(|_| !docs_only).and_then(|c| self.full_run_blamed(desk, c).map(|it| (c, it))) {
            Some((c, it)) => failed_on_item(s, r, &res, c, &it, now),
            None => failed_on_nobody(s, r, &res, docs_only),
        };
        // The same failure as the last the lead heard of (a run on the next commit failing the same way): said once.
        let key = failure_key(&res);
        told.quiet = s.told_failure == key;
        s.told_failure = key;
        Some(told)
    }

    /// The item of Desk `desk` that commit `c` is: the one its subject names, when that item's own landed commit is
    /// `c` (its finishing's commit, or one of those Cue made for it). Any other naming is no item's doing.
    fn full_run_blamed(&self, desk: &str, c: &Culprit) -> Option<DeskItem> {
        let id = kit::ids_in([c.subject.as_str()]).into_iter().next()?;
        let it = self.read(|st| st.desk.item(&id).cloned())?;
        let is = |short: &str| !short.is_empty() && c.commit.starts_with(short);
        let own = it.finish.as_ref().is_some_and(|f| f.commit == c.commit) || it.commit_in.iter().any(|(_, sha)| is(sha)) || it.commit.split_whitespace().any(is);
        (it.project == desk && own).then_some(it)
    }

    /// Run `r`, cut short when Cue quit, asked for again with the same commits on the same machine: the script makes
    /// no second run of them, it waits for the one left going and answers with its result (scripts/full-run.sh).
    fn full_run_same(&self, desk: &str, repo: &str, s: &mut FullRunState, runner: &dyn FullRunner, r: &Running, now: u64) {
        let mut ask = FullRunAsk { repo: repo.to_string(), at: r.at.clone(), since: r.since.clone(), ..Default::default() };
        let placed = if r.on.is_empty() { Ok(()) } else { self.full_run_place(desk, &r.on).map(|(host, folder)| (ask.machine, ask.host, ask.folder) = (r.on.clone(), host, folder)) };
        match placed.and_then(|()| runner.start(&ask)) {
            Ok(()) => s.running = Some(Running { began: now, ..r.clone() }),
            // It's made anew, from main's head.
            Err(_) => (s.running, s.started) = (None, String::new()),
        }
    }

    /// The next run of `repo`, when its main line's head is one no run was started on: on the machine picked, else
    /// here once nothing of the Desk is being built or installed (or that has taken `QUIET_WAIT_MS`).
    fn full_run_next(&self, desk: &str, repo: &str, s: &mut FullRunState, runner: &dyn FullRunner, now: u64) {
        let Some(head) = crate::desk::hub::landing::in_place::main_head(repo) else { return };
        // Since when main's head is a commit no run has finished on: what the install's two hours count from.
        if head == s.good() || head == s.judged {
            s.unrun_since = 0;
        } else if s.unrun_since == 0 {
            s.unrun_since = now;
        }
        if head == s.started || head == s.passed {
            s.wanted_since = 0;
            return;
        }
        // Since the last commit that passed, or that failed on tests no item caused: what fails after it is this batch's.
        let mut ask = FullRunAsk { repo: repo.to_string(), at: head.clone(), since: s.good().to_string(), ..Default::default() };
        let machine = if s.instead.is_empty() { crate::desk::lands::full_run_on(desk) } else { String::new() };
        if !machine.is_empty() {
            let there = self.full_run_place(desk, &machine).and_then(|(host, folder)| {
                let on = FullRunAsk { machine: machine.clone(), host, folder, ..ask.clone() };
                runner.start(&on).map_err(|e| format!("{} couldn't start it ({})", crate::machines::display_name(&machine), e.trim()))
            });
            match there {
                Ok(()) => {
                    (s.started, s.wanted_since) = (head.clone(), 0);
                    s.running = Some(Running { at: head, since: ask.since, on: machine, began: now, instead: String::new() });
                    return;
                }
                Err(why) => s.instead = why,
            }
        }
        let busy = self.read(|st| st.desk.items.iter().any(|i| i.project == desk && i.finish.as_ref().is_some_and(|f| crate::desk::hub::landing::install::installing(&f.step))));
        if busy {
            if s.wanted_since == 0 {
                s.wanted_since = now;
            }
            if now.saturating_sub(s.wanted_since) < kit::QUIET_WAIT_MS {
                return;
            }
        }
        ask.machine = String::new();
        match runner.start(&ask) {
            Ok(()) => {
                (s.started, s.wanted_since) = (head.clone(), 0);
                s.running = Some(Running { at: head, since: ask.since, on: String::new(), began: now, instead: std::mem::take(&mut s.instead) });
            }
            Err(e) => eprintln!("cue: desk: the full run couldn't start: {e}"),
        }
    }

    /// Where Desk `desk`'s run goes on `machine`: how it's reached and the project's clone there, as Cue has them
    /// (nothing on the machine is looked at). Err: why it can't go there.
    fn full_run_place(&self, desk: &str, machine: &str) -> Result<(String, String), String> {
        let m = crate::machines::get(machine).ok_or(format!("Cue knows no machine named {machine}"))?;
        // What you picked for the Desk first (a file of Cue's, read with the store let go), then its space's.
        let picked = crate::desk::machine_pref::of(desk).folder(machine).map(str::to_string);
        let folder = picked.or_else(|| self.read(|st| st.desk.spaces.get(desk).and_then(|d| d.machine_folders.get(machine).cloned())));
        let folder = folder.filter(|f| !f.trim().is_empty()).ok_or(format!("the project has no folder on {}", crate::machines::display_name(machine)))?;
        Ok((m.host, folder))
    }
}

/// Whether `commit` of `repo` changed only documents (Markdown, a licence) or nothing, against its first
/// parent: what no test or build reads (scripts/full-run.sh `only_documents`). Unknown to git: no.
pub(crate) fn only_documents(repo: &str, commit: &str) -> bool {
    let Some(files) = crate::install::live_ui::changed_in(repo, commit) else { return false };
    files.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).all(|f| {
        let name = f.rsplit('/').next().unwrap_or(f);
        [".md", ".markdown"].iter().any(|e| name.ends_with(e)) || name == "LICENSE"
    })
}

/// A commit's name, short.
fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}
