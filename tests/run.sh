#!/bin/bash
# Cue's tests, headless, never touching your real Cue or stealing the window's focus.
#   tests/run.sh [unit|ui|e2e|soak [minutes]|all]      (all when none is named)
#   unit  Rust tests (the app and src-tauri/xtask), then the protocol tests against a private, silent Cue: its own socket (CUE_HOME), no
#         notifications, no windows (CUE_QUIET). Your real Cue keeps running untouched. Then scripts/release.sh's
#         gates against stand-ins (tests/release/gates.test.sh): nothing built or published.
#   ui    every node test (tests/*.test.js, tests/ui/*.test.js), then the real UI in headless Chromium on the fake
#         bridge (tests/ui: screens against their baselines, and interactions). Needs `npm ci` once.
#   e2e   the real app driven over WebDriver, a private Cue per journey (tests/e2e/run.sh, docs/dev/e2e.md).
#   soak  the long stress run (docs/dev/soak.md), 10 minutes unless told: the World soak (src-tauri/src/testing/soak)
#         and the e2e soak against the real debug app (tests/e2e/soak) side by side, then both judged on one page.
#         The last line out is the page's path; any detector's failure is a non-zero exit. CUE_SOAK_DIR: where the
#         run's files go (a new /tmp folder otherwise); CUE_CHAOS_*: the chaos knobs, for both.
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
  "$ROOT/tests/release/gates.test.sh"
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
  export CUE_SOAK_MINUTES="${1:-10}"
  local dir="${CUE_SOAK_DIR:-$(mktemp -d /tmp/cue-soak.XXXX)}" failed=0
  mkdir -p "$dir"
  rm -f "$dir/rust.json" "$dir/e2e.json"
  echo "soak: $CUE_SOAK_MINUTES min, its files in $dir" >&2
  # Side by side: the gate takes the run's length, not twice it. Each one's output goes to its own log.
  (cd "$ROOT/src-tauri" && CUE_SOAK_OUT="$dir/rust.json" cargo test -q --lib -- --ignored testing::soak::soak --nocapture) >"$dir/rust.log" 2>&1 &
  local rust=$!
  CUE_SOAK_OUT="$dir/e2e.json" "$ROOT/tests/e2e/run.sh" soak/soak.test.js >"$dir/e2e.log" 2>&1 || failed=1
  wait $rust || failed=1
  local runs=()
  for run in rust e2e; do
    if [ -f "$dir/$run.json" ]; then runs+=("$dir/$run.json"); else echo "soak: the $run soak measured nothing (see $dir/$run.log)" >&2; failed=1; fi
  done
  if [ ${#runs[@]} -gt 0 ]; then
    (cd "$ROOT/src-tauri" && cargo run -q -p xtask -- soak-report "${runs[@]}") >"$dir/report.txt" || failed=1
  fi
  cat "$dir/report.txt" 2>/dev/null || true
  echo "$dir/report.txt"
  return $failed
}

case "${1:-all}" in
  unit) unit ;;
  ui) ui ;;
  e2e) e2e ;;
  soak) soak "$2" ;;
  all) ui; unit; e2e; soak ;;
  *) echo "usage: tests/run.sh [unit|ui|e2e|soak [minutes]|all]"; exit 2 ;;
esac
