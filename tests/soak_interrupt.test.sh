#!/bin/bash
# tests/run.sh soak, interrupted: the World soak's test process (and the e2e run) end with it, by pid. Builds
# and starts a real soak for a few seconds, so it's run by hand, not by tests/run.sh unit:
#   tests/soak_interrupt.test.sh
set -uo pipefail
unset TMUX TMUX_PANE
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIR="$(mktemp -d "${TMPDIR:-/tmp}/cue-soak-int.XXXX")"
CUE_SOAK_DIR="$DIR" "$ROOT/tests/run.sh" soak 5 >/dev/null 2>&1 &
RUN=$!
# The World soak's test binary, once it runs (it's built first).
soaking() { pgrep -f -- "--ignored testing::soak::soak --nocapture" | xargs -n1 ps -o pid=,command= -p 2>/dev/null | awk '/deps\/cue_lib-/ {print $1}'; }
for _ in $(seq 1 600); do [ -n "$(soaking)" ] && break; sleep 0.5; done
[ -n "$(soaking)" ] || { echo "FAIL the World soak never started"; kill $RUN; exit 1; }
kill -INT $RUN
wait $RUN
# This checkout's e2e app and hooks (another checkout's e2e run is none of this test's business).
apps() { pgrep -f "^$ROOT/src-tauri/target/e2e/debug/cue"; }
for _ in $(seq 1 40); do [ -z "$(soaking)$(apps)" ] && break; sleep 0.5; done
if [ -z "$(soaking)$(apps)" ]; then echo "ok   an interrupted soak leaves nothing running"; rm -rf "$DIR"; exit 0; fi
echo "FAIL left running: $(soaking) $(apps)"; exit 1
