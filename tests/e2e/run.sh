#!/bin/bash
# The end-to-end journeys against the real Cue window (docs/dev/e2e.md). Builds the debug app with its
# WebDriver server (the `e2e` feature, in a target folder of its own), then runs every journey, each on a
# private Cue: its own CUE_HOME, HOME and tmux server under one run folder. Whatever happens, every Cue,
# hook and tmux server this run started is ended on the way out. Your Cue and your tmux are never touched.
#   tests/e2e/run.sh                      every journey
#   tests/e2e/run.sh journeys/asks.test.js   one file (any node --test arguments)
set -eo pipefail
# An inherited $TMUX would send tmux to the server you work in, whatever else is set: never let it through.
unset TMUX TMUX_PANE
# Hooks started from a session Cue hosts or drives tell Cue nothing (tests/run.sh says why).
unset CUE_HOSTED CUE_DRIVEN_BY CUE_HOOK_RECONNECT_FOR
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
# In the run's own CARGO_TARGET_DIR when it has one (copies that share target/ build over each other).
E2E_TARGET="${CARGO_TARGET_DIR:-$REPO/src-tauri/target}/e2e"
export CUE_E2E_BIN="$E2E_TARGET/debug/cue"

# CUE_E2E_BUILT=1: the caller built it just now (the soak, so no build runs beside its other run).
[ "${CUE_E2E_BUILT:-}" = 1 ] || (cd "$REPO/src-tauri" && cargo build -q --features e2e --target-dir "$E2E_TARGET")
[ -d "$HERE/node_modules" ] || (cd "$HERE" && npm ci --no-audit --no-fund --silent)

# Short: Unix socket paths (Cue's, tmux's) must fit in 104 bytes.
export CUE_E2E_ROOT="$(mktemp -d /tmp/cue-e2e.XXXX)"
cleanup() {
  local status=$?
  pkill -KILL -f "^$CUE_E2E_BIN" 2>/dev/null || true   # every test Cue, and every hook it ran (the same binary)
  for sock in "$CUE_E2E_ROOT"/*/tmux/tmux-*/default; do
    [ -S "$sock" ] && tmux -S "$sock" kill-server 2>/dev/null || true
  done
  if [ $status -eq 0 ]; then rm -rf "$CUE_E2E_ROOT"; else echo "kept for a look: $CUE_E2E_ROOT"; fi
  exit $status
}
trap cleanup EXIT INT TERM

cd "$HERE"
if [ $# -eq 0 ]; then set -- journeys/*.test.js; fi
node --test --test-concurrency=1 --test-reporter=spec "$@"
