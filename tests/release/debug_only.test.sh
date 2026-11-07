#!/bin/bash
# scripts/release/debug_only.sh: a binary naming a chaos knob or the WebDriver server is refused; a clean
# one passes. With a release build in src-tauri/target/release, that binary is checked too.
#   tests/release/debug_only.test.sh
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/cue-debug-only.XXXX")"
trap 'rm -rf "$WORK"' EXIT
. "$ROOT/scripts/release/common.sh"
. "$ROOT/scripts/release/debug_only.sh"
failed=0
expect() {  # expect <pass|refuse> <what> <binary>
  if (check_debug_only "$3") 2>/dev/null; then got=pass; else got=refuse; fi
  if [ "$got" = "$1" ]; then echo "ok   $2"; else echo "FAIL $2 (expected $1, got $got)"; failed=1; fi
}
printf 'plain code\0more\0' >"$WORK/clean"
expect pass "a binary with no debug-only code" "$WORK/clean"
for trace in CUE_CHAOS_DB_LOCK_SECS TAURI_WEBDRIVER_PORT tauri-plugin-wdio-webdriver; do
  printf 'code\0%s\0' "$trace" >"$WORK/dirty"
  expect refuse "a binary naming $trace" "$WORK/dirty"
done
release="$ROOT/src-tauri/target/release/cue"
[ -f "$release" ] && expect pass "this checkout's release build" "$release"
exit $failed
