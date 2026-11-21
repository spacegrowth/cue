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
set +e
"$copy" "$@"
exit $?
