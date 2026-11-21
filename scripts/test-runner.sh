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
for a in "$@"; do
  if [ "$skip_value" = 1 ]; then skip_value=0; continue; fi
  case "$a" in
    --skip|--test-threads|--format|--logfile|--color|--shuffle-seed|-Z) skip_value=1 ;;
    -*) ;;
    *) full=0 ;;
  esac
done
if [ "$full" = 1 ]; then
  shared="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || shared=""
  [ -d "$shared" ] || shared="$dir"
  lock="$shared/cue-full-test-run.lock"
  exec 9>>"$lock"
  if ! lockf -s -t 0 9; then
    echo "test-runner: another full test run of this repository is going (in another copy); waiting for its end…" >&2
    lockf -s 9
    echo "test-runner: its turn is over; running." >&2
  fi
fi

set +e
# Without the turn's file open: a program a test leaves running mustn't hold the turn after the run.
"$copy" "$@" 9>&-
exit $?
