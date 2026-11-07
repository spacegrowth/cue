# The soak

Cue under a busy day's load for a long while, judged by detectors that fail the run (stability plan §5).
It is the pre-release gate's long step and a nightly run.

    tests/run.sh soak [minutes]        # 10 unless told; the last line out is the report's path

Two runs side by side, each on a private Cue that never touches yours, your tmux or your folders:

- **The World soak** (`src-tauri/src/testing/soak/`, an `#[ignore]`d test): the hub driven headless on its own
  socket, with the World's fakes standing in for the terminal, Notification Center and the launcher, and a
  fake `ssh` playing a machine. Alone: `cargo test --lib -- --ignored testing::soak::soak --nocapture`.
- **The e2e soak** (`tests/e2e/soak/`): the real debug app (built with the `e2e` feature) with real tmux panes,
  the real hook binary, the real window over WebDriver. Alone: `tests/e2e/run.sh soak/soak.test.js`.

Each writes what it measured as JSON; `cargo run -p xtask -- soak-report <json>…` (src-tauri/xtask/src/soak)
judges both with one set of thresholds and prints the page. Alone, each run only measures (the World soak's
test also fails on its own verdict).

## The load

| | World soak | e2e soak |
|---|---|---|
| Sessions (CUE_SOAK_SESSIONS, 40), each round after round: a prompt, 1.5–4.5 s of work with its transcript growing (~3 KB/s), a permission ask every third round (held open until answered 0.2–1.5 s later), its turn's end, a 2–6 s pause | hook events over the socket; asks held on the socket as the hook holds them | the same events; asks through the real hook binary (`cue-hook`) |
| Bursts: 200 hook events within a second, every 5 s (status-line costs, compactions) | ✓ | ✓ |
| Typing: ten typists, a message a second each — a reply on a finished turn's card, or a message to a session at work (held until its turn ends) | into the FakeTerminal | into real tmux panes |
| The window's poll of Cue's state (`list`), 5 a second | ✓ | ✓ |
| The built-in terminal opened and closed, every 15 s | — (no PTY in the World) | ✓ |
| A machine whose line drops every 30 s (CUE_CHAOS_SSH_DROP_EVERY) | a fake `ssh` whose watcher stream ends with exit 255 | — (no machine) |
| The main thread timed | — (no main thread) | an evaluated script's round trip every 250 ms, and the app's own probe pings |

## The detectors (each a failure)

- The window answers in over 500 ms: a `list` poll's round trip (World), an evaluated script's round trip or a
  main-thread probe ping (e2e).
- Any wait for a lock over 1 s (every busy lock's waits are counted by name: `locking::stats`, read with
  `{"type":"probe"}` / `cue-ctl probe`).
- Any lock taken again on the thread holding it.
- Resident memory, or the window's state (the `list` answer's size), past 2× what it was at the end of
  warm-up (a fifth of the run, at least a minute, at most half).
- A hook waiting over 10 s for a decision that was given, or never hearing it.
- Any panic (a panic hook in the World soak; cue.log in the e2e soak).
- Any hook event the socket refused.

The page: p50/p99/max answer times, the probe pings, events, typing and polls a second, each busy lock's
waits as a histogram, memory and the window's state every tenth of the run, the decisions' trips, why
typing was refused (refusals are expected — a card gone, a question first — and not a failure), and every
failure in words.

## Chaos knobs

Set in the environment of `tests/run.sh soak`; both runs take them. Debug builds only: a release build reads
none of them, and their names aren't in the binary (`strings target/release/cue | grep CUE_CHAOS` finds
nothing; the e2e debug binary has all five).

| Knob | The real adapter (debug app) | The World's fake |
|---|---|---|
| `CUE_CHAOS_TMUX_DELAY_MS` | every tmux command (focus/exec.rs `capped`) answers that late | `FakeTerminal::lags`: every trip, on the caller's thread |
| `CUE_CHAOS_SSH_DROP_EVERY` | every N s Cue's ssh processes are ended and new ones are refused for 2 s | the machine's watcher stream ends every N s (30 unset) |
| `CUE_CHAOS_XPC_DELAY_MS` | every job on Notification Center's thread, and the login-item status, waits that long | `FakeNotes::lags`: on a thread of its own, as the real one |
| `CUE_CHAOS_DB_LOCK_SECS` | another connection holds cue.db (`BEGIN EXCLUSIVE`) that long, every 30 s | the same (the World's database is real) |
| `CUE_CHAOS_FS_DELAY_MS` | every transcript read waits that long first | the same (the World's transcripts are real files) |

    CUE_CHAOS_TMUX_DELAY_MS=150 CUE_CHAOS_DB_LOCK_SECS=3 tests/run.sh soak 10

## The nightly run

`scripts/soak-nightly.sh [minutes]` runs `tests/run.sh soak 60` on the checkout it's in, kept awake with
`caffeinate`, into `~/Library/Logs/Cue/soak/<date-time>/` (`report.txt`, `rust.json`, `e2e.json`, their logs,
`run.log`); `~/Library/Logs/Cue/soak/latest` points at the newest; `history.log` gets a line a night (verdict,
commit, report). Thirty days are kept. The machine's real Cue keeps running; the soak never touches it.

### Turning the nightly run on

The launchd job is a template, not installed. To run it at 03:00 every night, from the checkout to soak:

    sed "s#__REPO__#$PWD#g; s#__HOME__#$HOME#g" scripts/launchd/dev.spacegrowth.cue.soak.plist \
      > ~/Library/LaunchAgents/dev.spacegrowth.cue.soak.plist
    mkdir -p ~/Library/Logs/Cue/soak
    launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/dev.spacegrowth.cue.soak.plist

Run it once now: `launchctl kickstart gui/$(id -u)/dev.spacegrowth.cue.soak`. Turn it off:
`launchctl bootout gui/$(id -u)/dev.spacegrowth.cue.soak` and remove the plist. A Mac asleep at 03:00 runs it
when it wakes.

## Reading a report

    == rust soak: 10.0 min, 40 sessions, chaos: none — PASS
      the state over the socket (list): n=2810 p50 24.0 ms, p99 61.0 ms, max 90.2 ms
      lock waits (busy locks only, longest first):
        the hub's store     4512 waits  max 14.1 ms  <1ms:3100 <5ms:980 <10ms:400 <50ms:32

A lock with a long tail (its `<500ms`/`<1000ms` buckets filling) is the next thing to fix before it crosses
the line; the run's JSON has every sample for a closer look.
