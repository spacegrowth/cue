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
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
export CUE_E2E_BIN="$REPO/src-tauri/target/e2e/debug/cue"

(cd "$REPO/src-tauri" && cargo build -q --features e2e --target-dir target/e2e)
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
