//! Machines over SSH (stability plan §4): each call Cue makes to a machine, against machines that refuse, ask
//! for a login, answer with garbage, hang, or answer well, played by `FakeSsh`; and when each is read
//! (remote_poll::Schedule). Hosts are named per test: what Cue keeps about a host lives as long as the process.

use crate::machines::{self, Machine};
use crate::remote_poll::Schedule;
use crate::testing::{FakeSsh, World, RUNS_IT};
use std::time::{Duration, Instant};

/// A World with machine `host` added (named after it), and the ssh that plays it.
fn with_machine(host: &str) -> (World, FakeSsh, Machine) {
    let w = World::new();
    let ssh = FakeSsh::new();
    let m = machines::add(host, "").unwrap();
    (w, ssh, m)
}

/// A machine's answer to Cue's tool check: these lines for `uname` and the `command -v` loop.
fn answers_check(lines: &str) -> String {
    format!("case \"$1\" in *uname*) printf '{lines}' ;; esac")
}

#[test]
fn a_machine_that_refuses_the_connection_is_tried_once_more_then_says_why() {
    let (_w, ssh, m) = with_machine("refuses");
    let err = machines::run(&m.host, "echo reached").unwrap_err();
    assert!(err.contains("Connection refused"), "ssh's own words: {err}");
    assert_eq!(ssh.calls_with("refuses", "echo reached"), 2, "a trip the machine never took is tried once more");
}

#[test]
fn a_machine_that_wants_a_login_isnt_retried_and_its_check_says_to_log_in() {
    let (_w, ssh, m) = with_machine("locked");
    ssh.host("locked", "echo 'locked: Permission denied (publickey).' >&2; exit 255");
    let c = machines::check(&m);
    assert_eq!(c["ok"], false);
    assert_eq!(c["needs_login"], true, "{c}");
    assert_eq!(c["error"], "locked: Permission denied (publickey).");
    assert_eq!(ssh.calls_with("locked", "uname"), 1, "a refused login isn't one a moment later fixes");
}

#[test]
fn a_check_reads_whats_installed_and_says_how_to_install_the_rest() {
    let (_w, ssh, m) = with_machine("tooled");
    ssh.host("tooled", &answers_check("Linux\\nhas claude\\nhas tmux\\n"));
    let c = machines::check(&m);
    assert_eq!((c["ok"].clone(), c["os"].clone()), (true.into(), "Linux".into()), "{c}");
    assert_eq!(c["tools"]["claude"], true);
    assert_eq!(c["tools"]["tmux"], true);
    assert_eq!(c["tools"]["codex"], false);
    assert_eq!(c["help"]["codex"], machines::how_to_install("codex"));
    assert!(c["help"].get("claude").is_none(), "nothing to say about what's there: {c}");
}

#[test]
fn a_machine_answering_garbage_counts_as_having_nothing_installed() {
    let (_w, ssh, m) = with_machine("noisy");
    ssh.host("noisy", &answers_check("\\001\\377Welcome!\\nhas claudex\\nhasclaude\\n has codex extra\\n"));
    let c = machines::check(&m);
    assert_eq!(c["ok"], true, "it was reached: {c}");
    assert!(machines::TOOLS.iter().all(|t| c["tools"][*t] == false), "only a whole `has <tool>` line counts: {c}");
}

#[test]
fn a_machine_that_drops_tmux_calls_says_it_cant_be_reached_by_the_name_you_gave_it() {
    let (_w, ssh, m) = with_machine("dropper");
    machines::rename(&m.name, "Build box").unwrap();
    ssh.host("dropper", "echo 'Connection closed by 10.0.0.9 port 22' >&2; exit 255");
    let err = machines::tmux(&m.host, &["list-panes"]).unwrap_err();
    assert_eq!(err, machines::cant_connect(&m.host));
    assert!(err.starts_with("Can't connect to Build box."), "{err}");
}

#[test]
fn tmux_runs_from_its_path_once_known_and_its_asked_again_once_the_machine_is_reached_again() {
    let (_w, ssh, m) = with_machine("tmuxed");
    ssh.host("tmuxed", "case \"$1\" in *'command -v tmux'*) echo /opt/bin/tmux ;; *list-panes*) echo %3 ;; esac");
    assert_eq!(machines::tmux(&m.host, &["list-panes"]).unwrap().trim(), "%3");
    assert_eq!(machines::tmux(&m.host, &["list-panes"]).unwrap().trim(), "%3");
    assert_eq!(ssh.calls_with("tmuxed", "command -v tmux"), 1, "asked once while Cue runs");
    let direct = ssh.calls().into_iter().filter(|(h, c)| h == "tmuxed" && c.contains("list-panes")).collect::<Vec<_>>();
    assert!(direct.iter().all(|(_, c)| c.contains("/opt/bin/tmux") && !c.contains("-lic")), "straight from its path, no login shell: {direct:?}");
    machines::forget(&m.host);
    machines::tmux(&m.host, &["list-panes"]).unwrap();
    assert_eq!(ssh.calls_with("tmuxed", "command -v tmux"), 2, "a machine reached again may have changed");
}

#[test]
fn the_machines_environment_is_written_there_once_and_later_calls_skip_the_login_shell() {
    let (_w, ssh, m) = with_machine("envful");
    ssh.host("envful", RUNS_IT);
    assert_eq!(machines::run(&m.host, "echo one").unwrap().trim(), "one");
    assert_eq!(machines::run(&m.host, "echo two").unwrap().trim(), "two");
    assert_eq!(ssh.calls_with("envful", "env -0"), 1, "read once");
    let file = std::fs::read_to_string(ssh.home("envful").join(".cue/env.sh")).unwrap();
    assert!(file.contains("export PATH="), "{file}");
    let two = ssh.calls().into_iter().find(|(_, c)| c.contains("echo two")).unwrap().1;
    assert!(two.contains(".cue/env.sh") && !two.contains("-lic"), "sourced, not through the login shell: {two}");
}

#[test]
fn a_machine_whose_environment_cant_be_read_still_runs_each_call_through_its_login_shell() {
    let (_w, ssh, m) = with_machine("envless");
    ssh.host("envless", &format!("case \"$1\" in *'env -0'*) exit 1 ;; esac\n{RUNS_IT}"));
    assert_eq!(machines::run(&m.host, "echo one").unwrap().trim(), "one");
    assert_eq!(machines::run(&m.host, "echo two").unwrap().trim(), "two");
    assert_eq!(ssh.calls_with("envless", "env -0"), 2, "asked again each time, never taken as written");
    assert!(ssh.calls().iter().filter(|(_, c)| c.contains("echo ")).all(|(_, c)| c.contains("-lic")), "{:?}", ssh.calls());
}

#[test]
fn a_slow_machine_holds_up_only_its_own_calls() {
    let (_w, ssh, slow) = with_machine("slowpoke");
    let fast = machines::add("quick", "").unwrap();
    let go = ssh.dir().join("go");
    ssh.host("slowpoke", &format!("while [ ! -e '{}' ]; do sleep 0.05; done; echo late", go.display()));
    ssh.host("quick", "echo here");
    let host = slow.host.clone();
    let waiting = std::thread::spawn(move || machines::run(&host, "true"));
    let start = Instant::now();
    assert_eq!(machines::run(&fast.host, "true").unwrap().trim(), "here");
    assert!(start.elapsed() < Duration::from_secs(5), "the quick one waited: {:?}", start.elapsed());
    assert!(!waiting.is_finished(), "the slow one is still out");
    std::fs::write(&go, "").unwrap();
    assert_eq!(waiting.join().unwrap().unwrap().trim(), "late");
}

#[test]
fn an_unreachable_machine_lists_no_crews_and_nothing_live_there() {
    let (_w, _ssh, m) = with_machine("crewless");
    let (changed, live) = crate::leads::refresh_remote(&m.name, &m.host);
    assert!(!changed);
    assert_eq!(live, None, "not reached: the sweep must not take its sessions for ended");
}

#[test]
fn a_machine_that_answers_says_which_of_its_sessions_are_live_and_garbage_says_none() {
    let (_w, ssh, m) = with_machine("crewed");
    ssh.host("crewed", "printf 'NORELAY\\n__CUE_PILEAD__\\n__CUE_LIVE__\\nsess-1\\n\\nsess-2\\n'");
    let (_, live) = crate::leads::refresh_remote(&m.name, &m.host);
    assert_eq!(live.unwrap(), ["sess-1".to_string(), "sess-2".to_string()].into_iter().collect());
    ssh.host("crewed", "printf '\\001garbage{\\n'");
    let (_, live) = crate::leads::refresh_remote(&m.name, &m.host);
    assert_eq!(live, Some(Default::default()), "reached, with nothing live it can name");
}

/// The times (seconds from the start) `machines` are read over `secs` of 10-second ticks, each read
/// answering `ok`.
fn reads(machines: &[(String, bool)], secs: u64, ok: impl Fn(&str) -> bool) -> Vec<(String, u64)> {
    let mut s = Schedule::default();
    let t0 = Instant::now();
    let mut out = vec![];
    for t in (0..=secs).step_by(10) {
        for m in s.due(machines, t0 + Duration::from_secs(t)) {
            s.done(&m, ok(&m));
            out.push((m, t));
        }
    }
    out
}

#[test]
fn a_quiet_machine_that_fails_is_tried_no_more_often_than_once_a_minute() {
    let tries: Vec<u64> = reads(&[("quiet".into(), false)], 700, |_| false).into_iter().map(|(_, t)| t).collect();
    assert_eq!(tries, vec![0, 60, 120, 200, 360, 660], "never sooner than its own pace, then backing off");
}

#[test]
fn a_machine_down_for_a_day_is_still_tried_every_5_minutes_beside_one_read_every_10_seconds() {
    let ms = vec![("down".to_string(), true), ("up".to_string(), true)];
    let all = reads(&ms, 86_400, |m| m == "up");
    let down: Vec<u64> = all.iter().filter(|(m, _)| m == "down").map(|(_, t)| *t).collect();
    assert!(down.windows(2).skip(5).all(|w| w[1] - w[0] == 300), "every 5 minutes for good: {:?}", &down[..8]);
    assert_eq!(all.iter().filter(|(m, _)| m == "up").count(), 8641, "the other one, every tick of the day");
}

#[test]
fn a_tick_a_moment_early_still_reads_the_machine_and_one_well_early_doesnt() {
    let mut s = Schedule::default();
    let t0 = Instant::now();
    let ms = vec![("m".to_string(), true)];
    s.due(&ms, t0);
    s.done("m", true);
    assert!(s.due(&ms, t0 + Duration::from_millis(8_900)).is_empty(), "over a second early");
    assert_eq!(s.due(&ms, t0 + Duration::from_millis(9_500)), vec!["m"], "half a second early counts");
}

#[test]
fn a_read_that_finishes_for_a_machine_never_scheduled_changes_nothing() {
    let mut s = Schedule::default();
    assert_eq!(s.done("ghost", false), None);
    assert_eq!(s.due(&[("ghost".into(), true)], Instant::now()), vec!["ghost"], "read at once, at its normal pace");
}

#[test]
fn a_machine_answering_slowly_is_waited_for_then_read_again_on_time() {
    let (_w, ssh, m) = with_machine("sluggish");
    ssh.host("sluggish", "sleep 1; printf '__CUE_LIVE__\\nslow-1\\n'");
    let mut s = Schedule::default();
    let t0 = Instant::now();
    let ms = vec![(m.name.clone(), true)];
    assert_eq!(s.due(&ms, t0), vec![m.name.clone()]);
    let (_, live) = crate::leads::refresh_remote(&m.name, &m.host);
    assert_eq!(live, Some(["slow-1".to_string()].into_iter().collect()));
    assert_eq!(s.done(&m.name, live.is_some()), None, "a slow answer is still an answer: no change to log");
    assert_eq!(s.due(&ms, t0 + Duration::from_secs(10)).len(), 1, "on its 10 s pace");
}
