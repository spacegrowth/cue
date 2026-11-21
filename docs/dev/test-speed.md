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
  repository, in any of its copies, to end (a lock in the shared git folder). Filtered runs don't wait.
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
