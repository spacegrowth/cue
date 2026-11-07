# End-to-end journeys (stability plan, Layer C)

The real Cue window, driven over WebDriver: a debug build with a WebDriver server inside the app, a private
Cue per journey, hook events through the real socket and the real `cue hook`, real tmux panes. This is the
one layer with Cue's real main thread, real WKWebView and real pseudo-terminals, so it's the one that sees a
freeze.

    tests/e2e/run.sh                          # build, then every journey (about 1 minute once built)
    tests/e2e/run.sh journeys/terminal.test.js   # one file; any `node --test` arguments

Needs Rust, Node 22, tmux. Runs unattended and headless: each Cue is quiet (`CUE_QUIET`), so its window is
hidden; WebDriver still clicks, types, evaluates and takes screenshots of it.

## The driver: WebdriverIO's embedded WebDriver server

Chosen over the fallback (a debug-only "evaluate this" socket message plus `screencapture`) because it
worked on this Mac in the spike, and gives real clicks and keys, not just evaluation:

- Rust crate `tauri-plugin-wdio-webdriver` 1.5.1 (crates.io, MIT, from github.com/webdriverio/desktop-mobile,
  a fork of Choochmeque/tauri-plugin-webdriver). It runs a W3C WebDriver server inside the app on
  `127.0.0.1:$TAURI_WEBDRIVER_PORT` (loopback only), using WKWebView's own calls. Verified here: session,
  find, click, keys (into xterm's textarea too), execute (sync and async), screenshot of the hidden window.
  Round trip for an evaluation: 10–30 ms.
- Client: plain `webdriverio` 9.31.9 (`remote()`), not the `@wdio/tauri-service` runner (1.5.1, which wraps
  the same plugin): the journeys need to start, kill and relaunch Cue themselves with private folders, and a
  plain client keeps that in our code. `node:test` runs the journeys.

### Never in a release build

- Cargo feature `e2e` (off by default) is the only way the crate is compiled in; `tests/e2e/run.sh` builds
  with it into `src-tauri/target/e2e`, so the normal debug build and `tests/run.sh` don't carry it either.
- `startup/mod.rs` has `compile_error!` for `feature = "e2e"` without `debug_assertions`: a release build with the
  feature doesn't compile.
- Checked: `cargo tree -i tauri-plugin-wdio-webdriver` finds nothing without the feature; a release build's
  binary has no `wdio`/`webdriver`/`TAURI_WEBDRIVER_PORT` string or symbol; the e2e debug binary has them.

## Isolation (what a run never touches)

Each journey gets its own folder under one run folder (`/tmp/cue-e2e.XXXX`, short because Unix socket
paths must fit 104 bytes): `cue/` (CUE_HOME), `home/` (HOME: Cue's view of `~/.claude`, `~/.relay-tasks`,
the window-state file), `tmux/` (TMUX_TMPDIR). Cue is started from an environment built from nothing
(`World.env`), with `TMUX`/`TMUX_PANE` never in it.

tmux: an inherited `$TMUX` sends tmux to the server you work in whatever else is set, so `run.sh` unsets it,
every tmux call in `lib/tmux.js` names its socket (`-S <journey>/tmux/tmux-<uid>/default`, exactly where
Cue looks with that TMUX_TMPDIR), and `Tmux` refuses a socket that is the default server's or the one
`$TMUX` names. Cleanup ends only those sockets' servers (`tmux -S <sock> kill-server`), and kills only
processes running this worktree's e2e binary (Cue and its hooks). A failed journey keeps its folder with
`cue.log` and `window.png`; the run prints where.

## Journeys (13)

| File | Journey |
|---|---|
| asks | fresh Cue, empty board, screenshot · permission → card → Allow → the hook gets allow · same with Deny |
| sessions | reply on a finished card typed into its tmux pane, exactly · a setting survives a relaunch · Park from ⋯ ends the pane and stays parked across a relaunch |
| terminal | open, type, close the built-in terminal · 1,000 keys while Cue's tmux client is SIGSTOPped · 1,000 keys while the pane's program is stopped · **freeze regression**: terminal nobody reads (tmux server SIGSTOPped), 2,000 keys, window answers < 500 ms throughout, "isn't responding" line, Resume after SIGCONT, typing goes in again |
| load | 1,000 hook events at 100/s across 20 sessions while typing 1,000 keys: window < 500 ms throughout, every key and event lands |
| relay | Review to a lead while another report waits (regression for 2e9abc0): cards render, review typed into the lead, window < 500 ms · the other card says "missed" again when the lead settles, with nothing else happening |

The relay journey fails with 2e9abc0 reverted (Cue deadlocks; the review never reaches the lead) and passes
with it: checked by reverting, rebuilding and running it.

Timing (M-series Mac, built): whole suite ~60 s, launch ~1.5 s per journey, the longest journey (load) ~12 s.
The first `run.sh` builds the e2e target (~4 min cold, seconds after).

Waiting: every wait is `waitFor` on something observable (the socket's `list`, the window's state or text,
tmux's view of the pane, a file the stand-in writes). Responsiveness is `watchLatency`: evaluations back to
back, each one needing the window's main thread, for the whole of the heavy step.

## What the stalled-terminal journey found

Stopping Cue's tmux **client** (`kill -STOP`) doesn't stall anything: tmux's server reads and writes the
client's terminal itself (the client hands its tty over), so Cue's writes keep flowing; 8 MB went in with the
client stopped, and every byte arrived after `SIGCONT`. A stopped program in the pane is the same: tmux's
server holds its input. Neither is a gap in Cue's stuck detection; both are journeys now (all typing arrives,
no "isn't responding" line). What leaves Cue's pseudo-terminal unread is a stopped tmux **server** (or a dead
SSH line): its ~1 KB buffer fills, the next write waits, and after 2 s Cue latches the input and the window
shows the line. That's the freeze journey. 1,000 keys is just under that buffer, so it types 2,000.

## Bugs found

- **Review cards went stale after a lead settled** (fixed: hub/shown.rs). After Review is typed into a lead,
  its other waiting reports read "busy" for the 8 s settle (`REVIEW_SETTLE_MS`). When that time passed
  nothing changed in the store, so the kept snapshot wasn't rebuilt and the window never heard: the card
  never said "Its lead hasn't picked this up." again until an unrelated event redrew. Now each build says
  when its next time-based change is (`NextChange`) and one timer per hub builds it again then.

## Not covered (and why)

- Resume / Restart of a parked session start the real agent (`claude --resume`, found on the Mac's PATH or
  `/opt/homebrew/bin`, or a new iTerm/Terminal window via AppleScript). Driving them needs a way to point
  Cue at a stand-in agent binary, which Cue doesn't have; Park and its persistence are covered.
- Real notifications, voice: `CUE_QUIET` keeps them off (plan §9).
- The window is hidden (quiet), so `requestAnimationFrame` doesn't run: what xterm draws isn't asserted;
  what reaches the pane is (tmux and the stand-in's file are the truth).
