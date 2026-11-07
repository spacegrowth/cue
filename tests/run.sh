#!/bin/bash
# Cue's tests, headless, never touching your real Cue or stealing the window's focus.
#   tests/run.sh [unit|ui|e2e|soak|all]      (all when none is named)
#   unit  Rust tests (the app and src-tauri/xtask), then the protocol tests against a private, silent Cue: its own socket (CUE_HOME), no
#         notifications, no windows (CUE_QUIET). Your real Cue keeps running untouched.
#   ui    every node test (tests/*.test.js, tests/ui/*.test.js), then the real UI in headless Chromium on the fake
#         bridge (tests/ui: screens against their baselines, and interactions). Needs `npm ci` once.
#   e2e   the real app driven over WebDriver, a private Cue per journey (tests/e2e/run.sh, docs/dev/e2e.md).
#   soak  the long stress run (src-tauri/src/testing/soak.rs), once it's there.
set -eo pipefail   # a failed step ends the run here
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Nothing here may reach the tmux server you're in: a Cue or test started from a tmux pane would inherit it.
unset TMUX TMUX_PANE

unit() {
  export CUE_HOME="$(mktemp -d /tmp/cue-test.XXXX)"
  export CUE_QUIET=1
  export CUE_BIN="$ROOT/src-tauri/target/debug/cue"
  # The protocol tests' stand-in agents, and the private Cue typing into them, on a tmux server of their own:
  # with $TMUX unset, a bare `tmux` would otherwise start sessions on your default server.
  export TMUX_TMPDIR="$CUE_HOME/tmux"
  mkdir -p "$TMUX_TMPDIR"
  (cd "$ROOT/src-tauri" && cargo test -q --workspace 2>&1 | grep -E "test result|FAILED|panicked" ; cargo build -q)
  "$CUE_BIN" >/dev/null 2>&1 &
  CUE_PID=$!
  trap end_unit EXIT
  for i in $(seq 1 50); do [ -S "$CUE_HOME/cue.sock" ] && break; sleep 0.2; done
  python3 "$ROOT/tests/test_protocol.py"
}

# What the unit run started, and only that: its private Cue (by its pid), whatever else runs with its CUE_HOME
# (the hooks its tests ran), the private tmux server by its own socket, then the folder. Never another Cue built
# from this checkout (a dev Cue, hooks shimmed to it). Each step may find nothing left to end.
end_unit() {
  kill "$CUE_PID" 2>/dev/null || true
  ps -E -ww -x -o pid=,command= | awk -v me=$$ -v home="CUE_HOME=$CUE_HOME" '$1 != me && index($0 " ", home " ") { print $1 }' | xargs kill 2>/dev/null || true
  tmux -S "$TMUX_TMPDIR/tmux-$(id -u)/default" kill-server 2>/dev/null || true
  rm -rf "$CUE_HOME"
}

ui() {
  for t in "$ROOT"/tests/*.test.js "$ROOT"/tests/ui/*.test.js; do node "$t"; done
  [ -d "$ROOT/node_modules/@playwright/test" ] || { echo "ui: run npm ci first (Playwright isn't installed)"; return 1; }
  (cd "$ROOT" && npx playwright test -c tests/ui/playwright.config.mjs)
}

e2e() {
  "$ROOT/tests/e2e/run.sh"
}

soak() {
  if [ -f "$ROOT/src-tauri/src/testing/soak.rs" ]; then (cd "$ROOT/src-tauri" && cargo test -q -- --ignored soak)
  else echo "soak: not yet (the soak harness lands as src-tauri/src/testing/soak.rs)"; fi
}

case "${1:-all}" in
  unit) unit ;;
  ui) ui ;;
  e2e) e2e ;;
  soak) soak ;;
  all) ui; unit; e2e; soak ;;
  *) echo "usage: tests/run.sh [unit|ui|e2e|soak|all]"; exit 2 ;;
esac
