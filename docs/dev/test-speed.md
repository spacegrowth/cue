# Test speed (#571)

The full Rust test run (`cargo test --lib`) took 30–45 minutes. This note records why, what changed,
and the times before and after.

## Why it was slow

- The World tests (`src/testing/world.rs`) take one process-wide lock (`db::test_turn()`), so they run
  one at a time and the suite's time is roughly the sum of theirs. The Desk landing tests make up most of it.
- Every Desk git call started three git processes: two `git config` reads (`--local`, then `--worktree`)
  to find the repository's own filters, then the command. One landing test (`done_lands_everything…`)
  started 263 git processes, 164 of them those filter reads.
- `/usr/bin/git` on macOS is a shim that looks up the real git on every call: about 54 ms a call against
  about 14 ms for the real one.

## What changed

- `safe_git::git_bin()` resolves git once and skips the `/usr/bin/git` shim for the developer tools' git.
  Every `Command::new("git")` in code and tests goes through it.
- The repository's own filters are read with one `git config --show-scope --get-regexp ^filter\.`
  (falls back to the two reads on a git older than 2.26), and not at all for ref lookups
  (`rev-parse`, `merge-base`, `rev-list`, `for-each-ref`, `worktree list`…), which never run a filter.
- `scripts/test-runner.sh`: a full run (no test filter) waits for any other full run of the same
  repository, in any of its copies, to end (a lock in the shared git folder). Filtered runs don't wait. The wait
  asks for the lock every few seconds and sleeps in between: `lockf` waiting on a descriptor spins a whole core.
- Updater tests fail after 5–10 s when the test server doesn't answer, instead of waiting 20–30 s.
- A failure the speed-up showed, fixed: a reopened item's own earlier landing commit, made in the same
  second as the reopen, was counted as new work (`unlanded.rs`; `git log --since` counts whole seconds).

## Times

Each file alone, nothing else of ours running (`--report-time`):

| File | Tests | Before | After |
|---|---|---|---|
| desk_land | 26 | 131 s | 33 s |
| desk_finish | 21 | 241 s | 45 s |
| desk_install | 19 | 167 s | 46 s |
| desk_spec_landing | 7 | 59 s | 10 s |
| desk_context | 10 | 6.5 s | 5.8 s |
| desk_commit | 3 | 237 s | 18 s |
| updater | 13 | up to 20 s a stuck test | 1.5 s |

(The before runs had a machine load around 30; the after runs around 80–90, so the after times are if
anything high.)

Full run, `cargo test --lib`, alone:

| | Tests run time | Load |
|---|---|---|
| Before | 2453 s (41 min) | 30–47 |
| After | 415 s (6.9 min); 478 s with the build | 9 → 7 |

The after run: 1743 passed, 1 failed (`desk_no_code::start_on_a_code_item_queued_after_another…`, which
also failed before and is fixed on main by #581). The slowest tests left are `typing_failures` (about
40 s each, mostly waiting their turn on the World lock).

## Card tests (#678)

A Desk item's session doesn't run every test before it says done. It runs:

    tests/run.sh card

That compares this checkout with main (the branch's commits, plus what isn't committed yet, plus new files) and runs
only the tests for the files that changed. It says first which sets it picked and why.

- Documents only (`docs/`, `work/`, `*.md`): nothing runs.
- Window files: the area's specs (`ui/desk/` → `desk-*.spec.mjs`…), always with `fits.spec.mjs`, the node tests and
  `scripts/check-window.mjs`.
- Rust files: `cargo build`, `cargo run -p xtask -- check`, then `cargo test --lib` for the matching files of
  `src/testing/tests` and the unit tests inside the changed files themselves.
- Shared ground (the hub's core, the data model, the database, the test harness, `tests/run.sh`, `Cargo.toml`), or a
  file no line of the map matches: a "wide card", the full suite for that side (`unit` or `ui`).

`tests/run.sh card --list` says what it would run and runs nothing. `tests/run.sh card --list ui/desk/list.js` (any
files) answers for those files instead of the checkout's own changes.

The full Rust run is left to the run after landing: started from a Desk item's copy
(`.claude/worktrees/desk-<number>`), `scripts/test-runner.sh` and `tests/run.sh unit` stop and point here.
`CUE_FULL_RUN=1` goes ahead anyway; a wide card sets it itself. Outside a Desk item's copy nothing changed.

### Adding a line to the map

The map is `tests/card-tests.map`: one line per rule, `<path pattern>  <what>  [names…]`, explained at its top.
A card that went wide for a file with an obvious narrow set, or a card whose set missed a test that the full run then
caught, adds a line:

    src-tauri/src/desk/hub/turn/said.rs    rust desk_to_user desk_notes

Names are files of `src-tauri/src/testing/tests` (for `rust`) or `tests/ui/specs` (for `window`); `set name = …`
names a group once. `node tests/card-tests.test.js` checks the map (every name is a real test file) and the picker;
`node tests/card-pick.js --check` says what is wrong with the map.

## Full run (#678)

Every test runs once for what landed, not once per card: `scripts/full-run.sh`, a plain script, started on this
machine or another machine from inside any clone or checkout of the repository.

    scripts/full-run.sh --at <commit> [--since <last passed commit>] [--dir <work folder>] [--out <result file>] [--again]

It keeps one clean copy of the repository in its work folder (`cue-full-run` in your cache folder unless `--dir` or
`CUE_FULL_RUN_DIR` says otherwise): a detached git worktree moved to each commit, its build folder and `node_modules`
kept between runs (`npm ci` only when `package-lock.json` changed). One run at a time per work folder; a second call
waits (`lockf`, or `flock` where there is none), asking every few seconds and using no processor in between. A call
waiting for an older commit than another waiting call gives up (exit 2): the later run covers it. The checkout it is
started from is left alone.

A run is not made twice. If the result file already holds a finished result for the same `--at` and `--since`, the
script prints its path and exits as that run did. A second call for a run still going waits for it, then answers with
its result. Cue relies on this: after a restart it asks for the same two commits again. `--again` forces a new run.
Cue runs the script as it is at the commit (`git show <commit>:scripts/full-run.sh | bash -s -- …`), on this machine
and on another one alike.

Steps, in order, each timed, each with a log in `<work folder>/logs`: build, code check (`xtask check`), rust tests
(`cargo test --lib`, all of them), node tests, window tests (Playwright). A failed build stops the rest; any other
failure doesn't, so one run reports everything that fails.

With `--since`, a failure is traced:

1. What failed is run once more at `--at`. What passes that time fails only sometimes: it goes in `flaky` and is not
   blamed. If nothing still fails, there is no culprit.
2. If it already fails at `--since`, there is no culprit: it failed before this batch.
3. The first-parent commits of `since..at` are searched by halving, in the same copy, running only what still fails.
   The first commit where it fails is the culprit.

A node test file or a spec that is not in a commit does not fail there, so one added failing is blamed on the commit
that added it. A Rust test name that matches no test passes by itself.

What Cue does with the result:

- No culprit, or a culprit that is not an item's own landed commit: the install is not held. That commit counts as
  passed for the install, and Cue keeps a separate note that its tests failed. The Desk line names the tests and says
  the app still updates.
- A culprit that is an item's landed commit: the item is reopened (once per commit, and only when it has landed; an
  item already at work gets a note). The install stays on the last commit that passed for at most 4 hours from that
  failure. Then it takes the tip and one Desk line says the app was installed although the full run still fails,
  naming the item. A later passing run clears this.
- No run finished for 2 hours after main moved: the install takes the tip and says it had no full run.
- A passed commit that is no longer on main counts as nothing passed; the rules above still apply.

The result file (`<work folder>/result.json`, or `--out`), whose path is the last line the script prints:

| Field | |
|---|---|
| `at`, `since` | the commits, in full (`since` is null when not given) |
| `passed` | true when every step passed |
| `started`, `ended` | milliseconds since 1970 |
| `steps` | `[{name, ok, secs, log}]`, one for each step that ran |
| `failed` | names: Rust tests, node test files, spec files, or a step's name (`build`, `code check`) |
| `culprit` | `{commit, subject}`, or null |
| `culprit_note` | why there is no culprit, or what else to know about it |
| `flaky` | the names of `failed` that passed when run once more |
| `tail` | the last 60 lines of the first failing step's log |

Exit 0 when it passed, 1 when something failed, 2 when the run couldn't be made (a bad commit, no copy; then no result
file is written). The result never says where it ran.

`tests/release/full_run.test.sh` runs the script against a throwaway repository with stand-in steps
(`CUE_FULL_RUN_STEPS` names a file that replaces them), in seconds.
