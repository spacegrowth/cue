#!/bin/bash
# Cargo's runner for this repository's programs (.cargo/config.toml).
#
# A test program runs from a private copy, never from target/: a build that rewrites or replaces the program at
# its target/ path while a run still uses it makes macOS refuse every process that run starts (the kernel logs
# "ASP: Security policy would not allow process … cue_lib-…"), and a test run that keeps starting processes
# then floods syspolicyd until the whole Mac stops responding. Sessions share this checkout and its target/,
# so one session's build during another's test run is ordinary. Anything else (cargo run) runs where it is.
set -e
bin="$1"
shift
case "$bin" in
  */deps/*) ;;
  *) exec "$bin" "$@" ;;
esac
dir="${TMPDIR:-/tmp}/cue-test-programs"
mkdir -p "$dir"
# Its real path ($TMPDIR is under /var, a link to /private/var): Tauri refuses to start an updater from a program
# whose path goes through a link, so every updater test would fail from the copy.
dir="$(cd "$dir" && pwd -P)"
# Copies a killed run left behind, a day on.
find "$dir" -type f -mmin +1440 -delete 2>/dev/null || true
copy="$dir/$(basename "$bin")-$$"
trap 'rm -f "$copy"' EXIT
# A clone (instant, no extra space on APFS), keeping the program's times: a test reads them.
cp -cp "$bin" "$copy" 2>/dev/null || cp -p "$bin" "$copy"

# A full run of Cue's tests (no test named: every one) takes the whole Mac's attention for minutes, so only one goes at
# a time across all of this repository's copies (its worktrees share one git folder, where the turn is kept); another
# waits for it. A run of some tests by name goes at once.
full=1
case "$(basename "$bin")" in cue_lib-*) ;; *) full=0 ;; esac
skip_value=0
listing=0
for a in "$@"; do
  if [ "$skip_value" = 1 ]; then skip_value=0; continue; fi
  case "$a" in
    --list) listing=1 ;;
    --skip|--test-threads|--format|--logfile|--color|--shuffle-seed|-Z) skip_value=1 ;;
    -*) ;;
    *) full=0 ;;
  esac
done
# A Desk item's copy (.claude/worktrees/desk-<number>) doesn't make a full run at all: its session runs the tests for
# what it changed (tests/run.sh card), and every test runs once after landing. CUE_FULL_RUN=1 goes ahead anyway
# (tests/run.sh card sets it for a wide card). Naming the tests (--list) is no run.
if [ "$full" = 1 ] && [ "$listing" = 0 ] && [ "${CUE_FULL_RUN:-}" != 1 ]; then
  top="$(git rev-parse --show-toplevel 2>/dev/null)" || top="$PWD"
  if [[ "$top" =~ /\.claude/worktrees/desk-[0-9]+$ ]]; then
    echo "test-runner: not run. This is a Desk item's copy, and a full run of Cue's tests is left to the run after landing." >&2
    echo "test-runner: run tests/run.sh card instead: it runs the tests for what you changed. (CUE_FULL_RUN=1 runs everything anyway.)" >&2
    exit 1
  fi
fi
if [ "$full" = 1 ]; then
  shared="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || shared=""
  [ -d "$shared" ] || shared="$dir"
  lock="$shared/cue-full-test-run.lock"
  exec 9>>"$lock"
  # Asked again every few seconds, sleeping in between: lockf waiting on a descriptor (`lockf -s 9`) spins a whole
  # processor core for as long as it waits.
  if ! lockf -s -t 0 9; then
    echo "test-runner: another full test run of this repository is going (in another copy); waiting for its end…" >&2
    until lockf -s -t 0 9; do sleep "${CUE_TEST_LOCK_POLL:-5}"; done
    echo "test-runner: its turn is over; running." >&2
  fi
fi

set +e
# Without the turn's file open: a program a test leaves running mustn't hold the turn after the run.
"$copy" "$@" 9>&-
exit $?
