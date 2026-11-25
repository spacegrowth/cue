//! The install the Desk shows (#867): the latest one, whoever started it (Cue's install queue, the lead's INSTALL: line,
//! or ./install.sh), while it builds, installs and starts, and how it ended. `state.desk.install`:
//! `{ tag, commit, step, at, began, line, log, items, by, failed_step, project }`, refreshed on the Desk's tick and pushed when it changes.
//! Also the lead's INSTALL: line itself.

use crate::desk::hub::landing::install::{installing, stopped_install, tail};
use crate::desk::hub::prelude::*;
use crate::desk::InstallStatus;
use crate::install::self_install::BY_HAND;

/// The tag an install the Desk lead asked for goes by (`lead-<ms>`).
pub(crate) const LEAD_TAG: &str = "lead-";

/// Who started the install tagged `tag`: "you" (./install.sh), "lead" (the lead's INSTALL: line) or "desk" (Cue's queue).
pub(crate) fn by(tag: &str) -> &'static str {
    if tag == BY_HAND {
        "you"
    } else if tag.starts_with(LEAD_TAG) {
        "lead"
    } else {
        "desk"
    }
}

/// The last line of `log` that says something, trimmed and cut to 200 characters: what the build is at now
/// ("Compiling cue (lib)").
pub(crate) fn last_line(log: &str) -> String {
    log.lines().rev().map(str::trim).find(|l| !l.is_empty()).map(|l| l.chars().take(200).collect()).unwrap_or_default()
}

/// The install `s` as the Desk shows it: `log` the end of its whole output, `items` the Desk items in it, `began` when it
/// started. Its last line while it runs, the end of its log once it failed or was rolled back. `last_step`: the last
/// step under way seen of it, for what a failure failed at (`failed_step`: "building" | "installing" | "starting"; a
/// roll-back is always at the start; "" unless it stopped). `project`: the Desk it's for ("" for none in particular).
pub(crate) fn shown(s: &InstallStatus, log: &str, items: Vec<String>, began: u64, last_step: &str, project: &str) -> Value {
    let line = if installing(&s.step) { last_line(log) } else { String::new() };
    let log = if stopped_install(&s.step) { tail(if s.log.trim().is_empty() { log } else { &s.log }) } else { String::new() };
    let failed_step = match s.step.as_str() {
        "rolled_back" => "starting",
        "failed" if installing(last_step) => last_step,
        "failed" => "building",
        _ => "",
    };
    json!({ "tag": s.tag, "commit": s.commit, "step": s.step, "at": s.at, "began": began, "line": line, "log": log, "items": items, "by": by(&s.tag), "failed_step": failed_step, "project": project })
}

/// When the install tagged `tag` began, first seen at `step` at `at` (ms): its step's time when that's the build's
/// start, else the time in its tag (`desk-<ms>`, `lead-<ms>`), else `at`.
fn began_of(tag: &str, step: &str, at: u64) -> u64 {
    if step == "building" {
        return at;
    }
    // Only a time in ms (a tag's own number, e.g. an item's in "lead-install-854", isn't one).
    tag.rsplit('-').next().and_then(|t| t.parse::<u64>().ok()).filter(|t| *t > 1_000_000_000_000 && *t <= at).unwrap_or(at)
}

impl Hub {
    /// The install the Desk shows, read again (the Desk's tick): pushed to the window when it changed.
    pub(crate) fn desk_install_watch(&self) {
        let inst = self.installer();
        let status = inst.status();
        let log = match &status {
            Some(s) if installing(&s.step) || (stopped_install(&s.step) && s.log.trim().is_empty()) => inst.log(),
            _ => String::new(),
        };
        let changed = self.write(|st| {
            let v = status.map(|s| {
                // Keyed on the tag, and started again when it builds anew after that: every ./install.sh run has
                // the same tag, and a later run's start and steps aren't the one before's.
                let anew = s.step == "building" && st.desk.run.install_began.as_ref().is_some_and(|(tag, at)| *tag == s.tag && s.at > *at);
                if anew {
                    st.desk.run.install_last_step = None;
                }
                let began = match &st.desk.run.install_began {
                    Some((tag, at)) if *tag == s.tag && !anew => *at,
                    _ => {
                        let at = began_of(&s.tag, &s.step, s.at);
                        st.desk.run.install_began = Some((s.tag.clone(), at));
                        at
                    }
                };
                if installing(&s.step) {
                    st.desk.run.install_last_step = Some((s.tag.clone(), s.step.clone()));
                }
                let last_step = st.desk.run.install_last_step.as_ref().filter(|(t, _)| *t == s.tag).map(|(_, step)| step.clone()).unwrap_or_default();
                let mine: Vec<&crate::desk::DeskItem> = st.desk.items.iter().filter(|i| !s.tag.is_empty() && i.finish.as_ref().is_some_and(|f| f.tag == s.tag)).collect();
                // The Desk it's for: its items' (when all are one Desk's), else the lead's that asked for it.
                let project = match mine.first() {
                    Some(f) if mine.iter().all(|i| i.project == f.project) => f.project.clone(),
                    Some(_) => String::new(),
                    None => st.desk.run.install_lead_desk.as_ref().filter(|(t, _)| *t == s.tag).map(|(_, d)| d.clone()).unwrap_or_default(),
                };
                let items = mine.iter().map(|i| i.id.clone()).collect();
                shown(&s, &log, items, began, &last_step, &project)
            });
            let changed = st.desk.run.install_shown != v;
            st.desk.run.install_shown = v;
            changed
        });
        if changed {
            self.changed();
        }
        self.desk_install_ended();
    }

    /// An install that ended (installed, failed, rolled back) leaves the Desk's top for its Activity: one move a Desk,
    /// said once (a restart reads the same status again), on the Desk it was for, or every Desk on Cue's own folder
    /// for one Cue didn't start (./install.sh). One that ended over a day ago is old news and isn't added.
    fn desk_install_ended(&self) {
        let Some(v) = self.read(|st| st.desk.run.install_shown.clone()) else { return };
        let (step, at) = (v["step"].as_str().unwrap_or_default().to_string(), v["at"].as_u64().unwrap_or(0));
        let now = now_ms();
        if !(step == "installed" || stopped_install(&step)) || at == 0 || now.saturating_sub(at) > 24 * 3_600_000 {
            return;
        }
        let project = v["project"].as_str().unwrap_or_default().to_string();
        let projects: Vec<String> = if project.is_empty() {
            // Read off the store: the folder check reads files.
            let spaces: Vec<(String, String)> = self.read(|st| st.desk.spaces.values().map(|d| (d.project.clone(), d.folder.clone())).collect());
            spaces.into_iter().filter(|(_, f)| crate::desk::hub::lead::checks::builds_this_cue(f)).map(|(p, _)| p).collect()
        } else {
            vec![project]
        };
        let items: Vec<String> = v["items"].as_array().map(|a| a.iter().filter_map(|i| i.as_str()).map(|i| format!("#{i}")).collect()).unwrap_or_default();
        let with = if items.is_empty() { String::new() } else { format!(" with {}", items.join(", ")) };
        let commit: String = v["commit"].as_str().unwrap_or_default().chars().take(7).collect();
        let took = at.saturating_sub(v["began"].as_u64().unwrap_or(at)) / 60_000;
        let (text, why) = if step == "installed" {
            (format!("Cue updated to {commit}{with}"), if took > 0 { format!("took {took}m") } else { String::new() })
        } else {
            let at_step = match v["failed_step"].as_str().unwrap_or_default() {
                "installing" => " at install",
                "starting" => " at restart",
                "building" => " at build",
                _ => "",
            };
            let last = v["log"].as_str().unwrap_or_default().trim_end().lines().last().unwrap_or_default().to_string();
            (format!("Cue update{with} failed{at_step}"), last)
        };
        let secs_at = crate::desk::hub::core::desk::secs(at);
        let fx = self.write(|st| {
            let mut fx = crate::desk::hub::core::desk::Effects::default();
            for p in &projects {
                if st.desk.decisions.iter().any(|d| d.act == "install" && d.project == *p && d.at == secs_at) {
                    continue;
                }
                crate::desk::hub::turn::route::decide(st, p, None, "Cue", "", &text, &why, at, &mut fx);
                if let Some(d) = st.desk.decisions.last_mut() {
                    (d.act, d.undo) = ("install".into(), "none".into());
                }
            }
            fx
        });
        if !fx.decisions.is_empty() {
            self.desk_done(fx);
        }
    }

    /// The Desk lead's INSTALL: main on Desk `desk`: Cue-repository items waiting for an install go now, as Install
    /// now does; with none, main's tip is installed as the lead's. What the lead is told: Ok when one started, Err when
    /// not (one under way, nothing to install from).
    pub(crate) fn desk_lead_install(&self, desk: &str) -> Result<String, String> {
        let inst = self.installer();
        let now = now_ms();
        if let Some(s) = inst.status().filter(|s| installing(&s.step)) {
            let whose = match by(&s.tag) {
                "you" => "./install.sh's",
                "lead" => "yours",
                _ => "the Desk's",
            };
            return Err(format!("An install is already under way ({whose}, {}): the Desk shows it. Nothing else started.", s.step));
        }
        let (ours, queued) = self.read(|st| {
            let finishing = || st.desk.items.iter().filter(|i| i.state == "finishing").filter_map(|i| i.finish.as_ref().map(|f| (i, f)));
            let ours = finishing().any(|(_, f)| installing(&f.step));
            let queued: Vec<String> = finishing().filter(|(_, f)| f.step == "queued" && crate::desk::hub::lead::checks::builds_this_cue(&f.repo)).map(|(i, _)| i.id.clone()).collect();
            (ours, queued)
        });
        if ours {
            return Err("An install is already under way (the Desk's): the Desk shows it. Nothing else started.".into());
        }
        if !queued.is_empty() {
            for id in &queued {
                self.finish_set(id, |i| {
                    if let Some(f) = i.finish.as_mut() {
                        f.now = true;
                    }
                });
            }
            self.install_now_start(now);
            let ids = queued.iter().map(|id| format!("#{id}")).collect::<Vec<_>>().join(", ");
            // Said only once it's so: the start can be turned away (another start holds it) or fail.
            let (started, after, failed) = self.read(|st| {
                let fins: Vec<&crate::desk::Finish> = queued.iter().filter_map(|id| st.desk.item(id)).filter_map(|i| i.finish.as_ref()).collect();
                (
                    fins.iter().any(|f| installing(&f.step)),
                    fins.iter().any(|f| f.step == "queued" && f.wait == crate::desk::hub::landing::install::AFTER_THIS_INSTALL),
                    fins.iter().any(|f| stopped_install(&f.step)),
                )
            });
            if started {
                return Ok(format!("Installing main now through Cue, with {ids} that waited for an install: the Desk shows it."));
            }
            if after {
                return Ok(format!("{ids} go in the install that starts the moment the one under way ends: the Desk shows it."));
            }
            if failed {
                return Err(format!("The install of main with {ids} didn't start: each item says why on the Desk."));
            }
            return Err(format!("Cue couldn't start the install just now: {ids} go with the next install, which starts as soon as Cue can."));
        }
        let folder = self.read(|st| crate::desk::hub::core::view::desk_folder(st, desk));
        if !crate::desk::hub::lead::checks::builds_this_cue(&folder) {
            return Err("This Desk's folder isn't Cue's repository: there's nothing for Cue to install.".into());
        }
        let repo = match crate::safe_git::run("", &folder, &[], &["rev-parse", "--show-toplevel"]) {
            Ok((top, 0)) if !top.trim().is_empty() => top.trim().to_string(),
            _ => folder.clone(),
        };
        let Some(tip) = crate::desk::hub::landing::in_place::main_head(&repo) else {
            return Err(format!("Cue couldn't read main's latest commit in {repo}: nothing started."));
        };
        if inst.has(&repo, &tip) {
            return Err(format!("The running Cue already has main ({}): nothing to install.", &tip[..tip.len().min(7)]));
        }
        let tag = format!("{LEAD_TAG}{now}");
        inst.start(&repo, &tip, &tag).map_err(|e| format!("The install didn't start: {e}"))?;
        let desk = desk.to_string();
        self.write(|st| st.desk.run.install_lead_desk = Some((tag, desk)));
        Ok(format!("Installing main ({}) through Cue now: the Desk shows it.", &tip[..tip.len().min(7)]))
    }
}

/// The Desk lead's INSTALL: line on Desk `desk`, a moment after its turn: done, the Desk's install shown at once, and the
/// lead told how it went.
pub(crate) fn lead_install(h: &Arc<Hub>, desk: &str) {
    let said = h.desk_lead_install(desk);
    h.desk_install_watch();
    let words = match said {
        Ok(w) | Err(w) => format!("INSTALL: {w}"),
    };
    if let Err(e) = crate::desk::hub::lead::desk_send_cue(h, desk, &words, crate::sessions::Kind::Answer) {
        eprintln!("cue: desk: the answer to {desk}'s INSTALL didn't reach its lead: {e}");
    }
}
